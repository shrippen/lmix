import QtQuick
import org.freedesktop.gstreamer.Qt6GLVideoItem

/**
 * Every slot live in one picture, lmix-core's tile picture: the mixer
 * scales the inputs on its GPU into one grid, the frames come over one
 * socket (with the CUDA mixer without leaving the GPU) and are drawn once.
 * Input tiles show their cell of it (LiveView.cell). Runs while a tile
 * uses it; one per window.
 */
Item {
    id: atlas

    required property var core
    /** Tiles that show a cell now (LiveView counts itself in and out). */
    property int users: 0
    /** `{columns, rows, cell_width, cell_height}`, null until it runs. */
    readonly property var grid: core.tiles !== "" ? JSON.parse(core.tiles) : null
    readonly property alias video: video
    readonly property bool wanted: users > 0

    // Never on screen: the tiles draw it, each its cell (hideSource).
    // At the picture's own pixel size, so a cell is drawn 1:1.
    x: -width - 1
    width: grid ? grid.columns * grid.cell_width / Screen.devicePixelRatio : 1
    height: grid ? grid.rows * grid.cell_height / Screen.devicePixelRatio : 1

    /** Cell of `slot` in this item's coordinates. */
    function cellRect(slot) {
        if (!grid || slot < 0) {
            return Qt.rect(0, 0, 0, 0)
        }
        const w = width / grid.columns
        const h = height / grid.rows
        return Qt.rect((slot % grid.columns) * w, Math.floor(slot / grid.columns) * h, w, h)
    }

    GstGLQt6VideoItem {
        id: video
        anchors.fill: parent
        forceAspectRatio: false
        acceptEvents: false
        onItemInitializedChanged: atlas.apply()
    }

    // A page change destroys its tiles before the next page's come: keep
    // the picture running over that gap.
    Timer {
        id: release
        interval: 2000
        onTriggered: {
            atlas.showing = false
            atlas.core.releaseLive(video)
        }
    }

    property bool showing: false

    function apply() {
        if (!video.itemInitialized) {
            return
        }
        if (!wanted) {
            if (showing) {
                release.restart()
            }
        } else if (release.running) {
            release.stop()
        } else if (!showing) {
            showing = true
            core.showLive(video, "tiles")
        }
    }

    onWantedChanged: apply()
    Component.onDestruction: core.releaseLive(video)
}
