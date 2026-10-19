import QtQuick
import Kante
import org.freedesktop.gstreamer.Qt6GLVideoItem

/**
 * A live picture from the engine: `source` is "program", "preview", an
 * input id, or "" for nothing. GStreamer draws into the item on the GPU (qml6glsink); the
 * stage stays dark in every style, as Kante's image stages do.
 */
Item {
    id: view

    required property var core
    /** "program", "preview", an input id (tile size) or "". */
    property string source: ""

    Rectangle {
        anchors.fill: parent
        color: KanteStyle.sunkenColor
    }

    GstGLQt6VideoItem {
        id: video
        anchors.fill: parent
        visible: view.source !== ""
        forceAspectRatio: true
        acceptEvents: false
        onItemInitializedChanged: view.apply()
    }

    function apply() {
        if (!video.itemInitialized) {
            return
        }
        if (source === "") {
            core.releaseLive(video)
        } else {
            core.showLive(video, source)
        }
    }

    onSourceChanged: apply()
    Component.onDestruction: core.releaseLive(video)
}
