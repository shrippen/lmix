pragma Singleton
import QtQuick

/**
 * Applies the show state's patches from Core.statePatched (format in
 * src/state_patch.rs). Containers on a changed path are copied, the rest
 * is kept: a binding that reads an unchanged part gets the same object
 * back, and Qt sends no change for it.
 */
QtObject {
    /** `[list, index by id]` of the lists `byId` was asked about last.
     *  Not a WeakMap: one in a property crashes Qt 6.11 (WeakMap.get in
     *  QV4::ESTable::get, 29.10.2026). Inside an object that never
     *  changes: bindings calling byId must not depend on the cache. */
    readonly property var cache: ({ indices: [] })

    /** The element of `list` with this `id`, or undefined. Each component
     *  looking up its input in the inputs once per state was quadratic in
     *  the inputs; the index is made once per list. A few lists are asked
     *  about at a time (the inputs, those with sound). */
    function byId(list, id) {
        if (!list) {
            return undefined
        }
        var indices = cache.indices
        var index
        for (var n = 0; n < indices.length; n++) {
            if (indices[n][0] === list) {
                index = indices[n][1]
                break
            }
        }
        if (index === undefined) {
            index = Object.create(null)
            for (var i = list.length - 1; i >= 0; i--) {
                index[list[i].id] = list[i]
            }
            cache.indices = [[list, index]].concat(indices.slice(0, 3))
        }
        return index[id]
    }

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
