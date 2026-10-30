import QtQuick

// Stand-in for gst-plugin-qml6's item in the QML tests: no GStreamer
// there, and no picture is needed to count items.
Item {
    property bool forceAspectRatio: false
    property bool acceptEvents: true
    readonly property bool itemInitialized: false
}
