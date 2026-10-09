import QtQuick

/**
 * A list model that follows a list of keys with the fewest changes, so
 * delegates (and their live pictures) survive state updates. Delegates
 * get `key` and look up their data themselves.
 */
ListModel {
    id: model

    function indexOfKey(key, from) {
        for (var i = from; i < count; i++) {
            if (get(i).key === key) {
                return i
            }
        }
        return -1
    }

    function sync(keys) {
        for (var i = count - 1; i >= 0; i--) {
            if (keys.indexOf(get(i).key) < 0) {
                remove(i)
            }
        }
        for (var j = 0; j < keys.length; j++) {
            var at = indexOfKey(keys[j], j)
            if (at < 0) {
                insert(j, { key: keys[j] })
            } else if (at !== j) {
                move(at, j, 1)
            }
        }
    }
}
