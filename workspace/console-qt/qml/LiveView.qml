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
 *
 * Only while it is shown: on a page that is not (or a tile that is not),
 * the picture stops after `holdMs`, as the tile picture does, so a page
 * change keeps it running. A hidden picture still made the window draw
 * every frame of it, and decoded it (31.10.2026).
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

    /** How long a hidden picture keeps running (a page change). */
    property int holdMs: 2000
    /** The source asked of the core for the video, "" for none. */
    property string asked: ""

    Timer {
        id: hold
        interval: view.holdMs
        onTriggered: {
            if (!view.visible && view.asked !== "") {
                view.asked = ""
                view.core.releaseLive(video)
            }
        }
    }

    function apply() {
        // `visible` is false too while a page above it is hidden.
        count(tiled && visible)
        if (!video.itemInitialized) {
            return
        }
        var want = source === "" || tiled ? "" : source
        if (want !== "" && want === asked && !visible) {
            if (!hold.running) {
                hold.restart()
            }
            return
        }
        hold.stop()
        if (!visible) {
            want = ""
        }
        if (want === asked) {
            return
        }
        asked = want
        if (want === "") {
            core.releaseLive(video)
        } else {
            core.showLive(video, want)
        }
    }

    onSourceChanged: apply()
    onTiledChanged: apply()
    onVisibleChanged: apply()
    Component.onCompleted: count(tiled && visible)
    Component.onDestruction: {
        count(false)
        core.releaseLive(video)
    }
}
