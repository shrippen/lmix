import QtQuick
import QtTest
import "../../qml"

/**
 * An input tile draws its cell of the tile picture (LiveView, TilesAtlas)
 * when the picture has one: the mixer grows past its cells (lmix-core's
 * TILE_CELLS), later slots show a stream of their own.
 */
TestCase {
    id: test
    name: "LiveView"
    when: windowShown

    QtObject {
        id: fakeCore
        function showLive(item, source) {}
        function releaseLive(item) {}
    }

    Component {
        id: windowComponent
        Window {
            property alias view: view
            /** What TilesAtlas offers a window: its grid and a user count. */
            property var tilesAtlas: QtObject {
                property var grid: null
                property int users: 0
            }
            width: 160
            height: 90
            LiveView {
                id: view
                anchors.fill: parent
                core: fakeCore
                source: "input-1"
            }
        }
    }

    function test_slots_past_the_cells_get_their_own_stream() {
        const window = createTemporaryObject(windowComponent, test)
        const view = window.view
        view.cell = 25
        // Before the picture runs its size is open: the tile counts in.
        verify(view.tiled)
        window.tilesAtlas.grid = { columns: 5, rows: 4, cell_width: 384, cell_height: 216 }
        verify(!view.tiled, "slot 25 has no cell in 5 × 4")
        view.cell = 19
        verify(view.tiled, "the last cell")
        view.cell = 20
        verify(!view.tiled)
        compare(window.tilesAtlas.users, 0, "counted out again")
        view.source = "program"
        view.cell = 0
        verify(!view.tiled, "program never tiles")
    }
}
