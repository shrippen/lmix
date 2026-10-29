import QtQuick
import org.freedesktop.gstreamer.Qt6GLVideoItem

/**
 * Every slot live in one picture, lmix-core's tile picture: the mixer
 * scales the inputs on its GPU into one grid, the frames come over one
 * socket (with the CUDA mixer without leaving the GPU) as one texture.
 * Input tiles draw their cell of it (LiveView.cell, AtlasCell); this
 * item's video only lends the picture its GL context and draws nothing.
 * Runs while a tile uses it; one per window.
 */
Item {
    id: atlas

    required property var core
    /** Tiles that show a cell now (LiveView counts itself in and out). */
    property int users: 0
    /** `{columns, rows, cell_width, cell_height}`, null until it runs. */
    readonly property var grid: core.tiles !== "" ? JSON.parse(core.tiles) : null
    readonly property bool wanted: users > 0

    // Never on screen; the sink needs an item in the window.
    x: -width - 1
    width: 1
    height: 1

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
