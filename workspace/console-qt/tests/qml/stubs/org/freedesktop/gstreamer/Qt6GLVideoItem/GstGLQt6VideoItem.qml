import QtQuick

// Stand-in for gst-plugin-qml6's item in the QML tests: no GStreamer
// there, and no picture is needed to count items. A test sets
// itemInitialized, as the scene graph does once the item is drawn.
Item {
    property bool forceAspectRatio: false
    property bool acceptEvents: true
    property bool itemInitialized: false
}
