import QtQuick
import QtTest
import org.kde.kirigami as Kirigami
import "../../qml"

/**
 * DragScroll: a drag held near a list's edge scrolls it that way, also
 * in a list lower than both edge zones together.
 */
TestCase {
    id: test
    name: "DragScroll"
    when: windowShown
    width: 400
    height: 400

    Component {
        id: listComponent
        Flickable {
            property alias scroll: scroll
            width: 200
            contentHeight: 2000
            DragScroll {
                id: scroll
                flickable: parent
            }
        }
    }

    // In a list three grid units high the zones do not overlap: the
    // pointer at the bottom scrolls down, at the top up.
    function test_a_low_list_scrolls_toward_the_edge_held() {
        const list = createTemporaryObject(listComponent, test, { height: Kirigami.Units.gridUnit * 3 })
        list.contentY = 500
        list.scroll.dragging = true
        list.scroll.scenePosition = list.mapToItem(null, 10, list.height - 2)
        compare(list.scroll.direction, 1, "down at the bottom")
        list.scroll.scenePosition = list.mapToItem(null, 10, 2)
        compare(list.scroll.direction, -1, "up at the top")
        list.scroll.scenePosition = list.mapToItem(null, 10, list.height / 2)
        compare(list.scroll.direction, 0, "still in the middle")
    }
}
