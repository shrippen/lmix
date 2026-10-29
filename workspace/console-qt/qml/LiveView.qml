import QtQuick
import Kante
import org.freedesktop.gstreamer.Qt6GLVideoItem

/**
 * A live picture from the engine: `source` is "program", "preview", an
 * input id, or "" for nothing. GStreamer draws into the item on the GPU (qml6glsink); the
 * stage stays dark in every style, as Kante's image stages do.
 *
 * An input with its `cell` (its slot) shows that cell of the window's
 * tile picture (TilesAtlas) instead of a stream of its own: one picture
 * for all tiles, cut up on the GPU.
 */
Item {
    id: view

    required property var core
    /** "program", "preview", an input id (tile size) or "". */
    property string source: ""
    /** The input's slot: its cell in the tile picture; -1 for none. */
    property int cell: -1

    readonly property var atlas: Window.window ? Window.window.tilesAtlas ?? null : null
    readonly property bool tiled: atlas !== null && cell >= 0 && source !== ""
        && source !== "program" && source !== "preview"

    Rectangle {
        anchors.fill: parent
        color: KanteStyle.sunkenColor
    }

    // The cell, fitted as forceAspectRatio fits the video.
    ShaderEffectSource {
        id: tile
        readonly property real aspect: view.atlas && view.atlas.grid
            ? view.atlas.grid.cell_width / view.atlas.grid.cell_height : 16 / 9
        anchors.centerIn: parent
        width: Math.min(parent.width, parent.height * aspect)
        height: width / aspect
        visible: view.tiled && view.atlas.grid !== null
        sourceItem: view.tiled ? view.atlas.video : null
        sourceRect: view.tiled ? view.atlas.cellRect(view.cell) : Qt.rect(0, 0, 0, 0)
        // Only while it draws: the atlas has to be drawn once to start.
        hideSource: visible
        smooth: true
    }

    GstGLQt6VideoItem {
        id: video
        anchors.fill: parent
        visible: view.source !== "" && !view.tiled
        forceAspectRatio: true
        acceptEvents: false
        onItemInitializedChanged: view.apply()
    }

    // The atlas this view counts itself in with; kept, as Window.window
    // is gone by the time the view is destroyed.
    property var countedIn: null

    function count(on) {
        if (on && countedIn === null && atlas !== null) {
            countedIn = atlas
            countedIn.users += 1
        } else if (!on && countedIn !== null) {
            countedIn.users -= 1
            countedIn = null
        }
    }

    function apply() {
        count(tiled)
        if (!video.itemInitialized) {
            return
        }
        if (source === "" || tiled) {
            core.releaseLive(video)
        } else {
            core.showLive(video, source)
        }
    }

    onSourceChanged: apply()
    onTiledChanged: apply()
    Component.onCompleted: count(tiled)
    Component.onDestruction: {
        count(false)
        core.releaseLive(video)
    }
}
