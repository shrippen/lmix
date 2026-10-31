import QtQuick
import Kante
import org.freedesktop.gstreamer.Qt6GLVideoItem
import org.lmix.atlas

/**
 * A live picture from the engine: `source` is "program", "preview", an
 * input id, or "" for nothing. GStreamer draws into the item on the GPU (qml6glsink); the
 * stage stays dark in every style, as Kante's image stages do.
 *
 * An input with its `cell` (its slot) shows that cell of the window's
 * tile picture (TilesAtlas) instead of a stream of its own: one texture
 * for all tiles, each drawing its part (AtlasCell). Slots past the
 * picture's cells get their own stream.
 */
Item {
    id: view

    required property var core
    /** "program", "preview", an input id (tile size) or "". */
    property string source: ""
    /** The input's slot: its cell in the tile picture; -1 for none. */
    property int cell: -1

    readonly property var atlas: Window.window ? Window.window.tilesAtlas ?? null : null
    // The picture has cells for the first slots only (lmix-core's
    // TILE_CELLS); a later slot shows a stream of its own. Until the
    // picture runs its size is open: the tile counts itself in.
    readonly property var atlasGrid: atlas !== null ? atlas.grid : null
    readonly property bool tiled: atlas !== null && cell >= 0 && source !== ""
        && source !== "program" && source !== "preview"
        && (atlasGrid === null || cell < atlasGrid.columns * atlasGrid.rows)

    Rectangle {
        anchors.fill: parent
        color: KanteStyle.sunkenColor
    }

    // The cell, fitted as forceAspectRatio fits the video.
    AtlasCell {
        readonly property var grid: view.tiled ? view.atlas.grid : null
        readonly property real aspect: grid ? grid.cell_width / grid.cell_height : 16 / 9
        anchors.centerIn: parent
        width: Math.min(parent.width, parent.height * aspect)
        height: width / aspect
        visible: grid !== null
        cell: view.cell
        columns: grid ? grid.columns : 1
        rows: grid ? grid.rows : 1
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
