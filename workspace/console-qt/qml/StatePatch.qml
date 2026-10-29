pragma Singleton
import QtQuick

/**
 * Applies the show state's patches from Core.statePatched (format in
 * src/state_patch.rs). Containers on a changed path are copied, the rest
 * is kept: a binding that reads an unchanged part gets the same object
 * back, and Qt sends no change for it.
 */
QtObject {
    function apply(root, ops) {
        // Copies made by this call: changed in place by later operations.
        var fresh = new Set()
        function own(value) {
            if (fresh.has(value)) {
                return value
            }
            var copy = Array.isArray(value) ? value.slice() : Object.assign({}, value)
            fresh.add(copy)
            return copy
        }
        for (var n = 0; n < ops.length; n++) {
            var op = ops[n]
            var path = op.p
            if (path.length === 0) {
                root = op.v
                continue
            }
            root = own(root)
            var at = root
            for (var i = 0; i < path.length - 1; i++) {
                at = at[path[i]] = own(at[path[i]])
            }
            var last = path[path.length - 1]
            if (op.d) {
                delete at[last]
            } else if (op.a) {
                var old = at[last]
                var made = op.a.map(e => e.k !== undefined ? old[e.k] : e.v)
                fresh.add(made)
                at[last] = made
            } else {
                at[last] = op.v
            }
        }
        return root
    }
}
