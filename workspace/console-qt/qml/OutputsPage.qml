import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtCore
import org.kde.kirigami as Kirigami
import Kante
import org.lmix.console

/**
 * The Outputs page (docs/PLAN-SEITEN.md): recordings, streams, NDI and
 * screens in a list with state, running time and link health; start and
 * stop each, or the streams and recordings together; the selected one in
 * detail; the multiview. Stopping asks first (UI.10). Its own panic stops
 * every recording and stream, after asking.
 */
AppPage {
    id: root

    /** The window (AppShell): show, actions, helpers. */
    required property var shell
    readonly property var show: shell.show
    readonly property var backend: shell.backend
    readonly property var outputs: show && show.outputs ? show.outputs : []
    readonly property var links: backend.links.length > 0 ? JSON.parse(backend.links) : ({})
    readonly property var running: outputs.filter(o => o.status.state === "running")
    /** SRT streams: lmix-core's group `streams` (STR.1). */
    readonly property var streams: outputs.filter(o => o.spec.kind === "srt")
    readonly property var recordings: outputs.filter(o => o.spec.kind === "record")
    function act(action) { shell.act(action) }
    // Rows by id: a new state keeps them (and their running-time timers).
    KeyedModel { id: outputModel }
    Component.onCompleted: outputModel.sync(outputs.map(o => o.id))
    onOutputsChanged: outputModel.sync(outputs.map(o => o.id))

    property string selectedId: ""
    readonly property var selected: outputs.find(o => o.id === selectedId) || outputs[0] || null


    function stop(output) {
        if (!root.shell.confirmStop) {
            act({ action: "stop_output", output: output.id })
            return
        }
        stopDialog.output = output
        stopDialog.open()
    }
    /** Every running recording and stream (SRT, RTMP), one by one: the
     *  core's groups cover SRT streams only. */
    function stopMedia() {
        running.filter(o => ["record", "srt", "rtmp"].indexOf(o.spec.kind) >= 0)
            .forEach(o => act({ action: "stop_output", output: o.id }))
    }
    function stopGroup(group) {
        if (!root.shell.confirmStop) {
            if (group === "media") {
                stopMedia()
            } else {
                act({ action: "stop_outputs", group: group })
            }
            return
        }
        stopDialog.output = null
        stopDialog.group = group
        stopDialog.open()
    }

    pageId: "outputs"
    title: qsTr("Outputs")
    subtitle: qsTr("%1 of %2 running").arg(running.length).arg(outputs.length)
    busy: running.length > 0
    busyKind: KantePageBar.Kind.Info
    panicText: qsTr("Stop outputs")
    panicTip: qsTr("Stop every recording and stream (asks first); mix and show go on")
    // Recordings and streams only: screens and NDI may feed a stage.
    onPanicRequested: stopGroup("media")

    tools: [
        KanteButton {
            focusPolicy: Qt.NoFocus
            text: qsTr("+ Output")
            enabled: root.backend.connected
            onClicked: addOutputDialog.open()
        },
        KanteButton {
            visible: root.streams.length > 1
            focusPolicy: Qt.NoFocus
            readonly property bool all: root.streams.every(o => o.status.state === "running")
            text: all ? qsTr("Stop all streams") : qsTr("Start all streams")
            enabled: root.backend.connected
            onClicked: all ? root.stopGroup("streams") : root.act({ action: "start_outputs", group: "streams" })
        },
        KanteButton {
            visible: root.recordings.length > 1
            focusPolicy: Qt.NoFocus
            readonly property bool all: root.recordings.every(o => o.status.state === "running")
            text: all ? qsTr("Stop all recordings") : qsTr("Start all recordings")
            enabled: root.backend.connected
            onClicked: all ? root.stopGroup("recordings") : root.act({ action: "start_outputs", group: "recordings" })
        },
        QQC2.CheckBox {
            id: askBox
            focusPolicy: Qt.NoFocus
            text: qsTr("Ask before stopping")
            checked: root.shell.confirmStop
            onToggled: root.shell.confirmStop = checked
            KanteCheckSkin { control: askBox }
        }
    ]

    RowLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.largeSpacing

        KanteCard {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumWidth: Kirigami.Units.gridUnit * 30

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: Kirigami.Units.smallSpacing
                spacing: 0

                RowLayout {
                    Layout.fillWidth: true
                    KanteSectionLabel { Layout.preferredWidth: Kirigami.Units.gridUnit * 12; text: qsTr("Output") }
                    KanteSectionLabel { Layout.preferredWidth: Kirigami.Units.gridUnit * 6; text: qsTr("Kind") }
                    KanteSectionLabel { Layout.preferredWidth: Kirigami.Units.gridUnit * 7; text: qsTr("State") }
                    KanteSectionLabel { Layout.preferredWidth: Kirigami.Units.gridUnit * 5; text: qsTr("Time") }
                    KanteSectionLabel { Layout.fillWidth: true; text: qsTr("Link") }
                }
                ListView {
                    id: list
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: outputModel
                    delegate: OutputRow {}
                    QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                    KanteEmptyState {
                        anchors.centerIn: parent
                        visible: list.count === 0
                        text: qsTr("No outputs yet: + Output adds a recording, stream, NDI or screen")
                    }
                }
            }
        }

        ColumnLayout {
            Layout.preferredWidth: Kirigami.Units.gridUnit * 20
            Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            Layout.fillHeight: true
            spacing: Kirigami.Units.largeSpacing

            KanteCard {
                visible: root.selected !== null
                Layout.fillWidth: true
                implicitHeight: detail.implicitHeight + Kirigami.Units.largeSpacing * 2
                ColumnLayout {
                    id: detail
                    anchors.fill: parent
                    anchors.margins: Kirigami.Units.largeSpacing
                    readonly property var output: root.selected
                    readonly property var spec: output ? output.spec : ({})
                    KanteSectionLabel { text: detail.output ? detail.output.name : "" }
                    Detail { label: qsTr("Kind"); value: root.kindName(detail.spec) }
                    Detail { label: qsTr("Source"); value: detail.output ? root.sourceName(detail.output) : "" }
                    Detail { label: qsTr("Sound"); value: detail.output && detail.output.aux ? Texts.auxName(detail.output.aux - 1) : qsTr("of the source") }
                    Detail { label: qsTr("Target"); value: root.targetName(detail.spec) }
                    Detail {
                        visible: detail.spec.video_kbps !== undefined
                        label: qsTr("Video")
                        value: detail.spec.video_kbps !== undefined ? qsTr("%1 kbit/s").arg(detail.spec.video_kbps) : ""
                    }
                    Detail {
                        visible: detail.output !== null && root.links[detail.output.id] !== undefined
                        label: qsTr("Link")
                        value: detail.output && root.links[detail.output.id] ? Texts.linkDetail(root.links[detail.output.id]) : ""
                    }
                    Detail {
                        visible: detail.output !== null && detail.output.status.state === "failed"
                        label: qsTr("Failed")
                        value: detail.output && detail.output.status.message ? detail.output.status.message : ""
                        valueColor: KanteStyle.negativeTextColor
                    }
                }
            }

            // OUT.10: program left or right, meters on the tiles; the mixer
            // is built anew, like for the format.
            KanteCard {
                Layout.fillWidth: true
                implicitHeight: mv.implicitHeight + Kirigami.Units.largeSpacing * 2
                ColumnLayout {
                    id: mv
                    anchors.fill: parent
                    anchors.margins: Kirigami.Units.largeSpacing
                    readonly property var layout: root.show && root.show.multiview ? root.show.multiview : ({ program: "right", meters: "shown" })
                    KanteSectionLabel { text: qsTr("Multiview") }
                    QQC2.CheckBox {
                        id: mvLeft
                        text: qsTr("Program on the left")
                        checked: mv.layout.program === "left"
                        onToggled: root.act({ action: "set_multiview", layout: { program: checked ? "left" : "right", meters: mv.layout.meters } })
                        KanteCheckSkin { control: mvLeft }
                    }
                    QQC2.CheckBox {
                        id: mvMeters
                        text: qsTr("Level meters")
                        checked: mv.layout.meters === "shown"
                        onToggled: root.act({ action: "set_multiview", layout: { program: mv.layout.program, meters: checked ? "shown" : "hidden" } })
                        KanteCheckSkin { control: mvMeters }
                    }
                }
            }
            Item { Layout.fillHeight: true }
        }
    }

    function kindName(spec) {
        switch (spec.kind) {
        case "record": return qsTr("Recording")
        case "srt": return qsTr("Stream SRT")
        case "rtmp": return qsTr("Stream RTMP")
        case "ndi": return qsTr("NDI")
        case "display": return qsTr("Screen")
        }
        return spec.kind || ""
    }
    function targetName(spec) {
        switch (spec.kind) {
        case "record": return spec.folder
        case "srt": return spec.uri
        case "rtmp": return spec.url
        case "ndi": return spec.name
        case "display": return spec.mode ? spec.mode.mode : ""
        }
        return ""
    }
    function sourceName(output) {
        var source = output.source || { from: "program" }
        if (source.from === "input") {
            var input = shell.inputById(source.input)
            return input ? qsTr("Input %1 %2").arg(input.number).arg(input.name) : qsTr("Input")
        }
        return source.from === "preview" ? qsTr("Preview") : (source.from === "multiview" ? qsTr("Multiview") : qsTr("Program"))
    }

    component Detail: RowLayout {
        property string label
        property string value
        property color valueColor: KanteStyle.textColor
        Layout.fillWidth: true
        QQC2.Label {
            Layout.preferredWidth: Kirigami.Units.gridUnit * 5
            text: parent.label
            color: KanteStyle.mutedTextColor
        }
        QQC2.Label {
            Layout.fillWidth: true
            text: parent.value
            color: parent.valueColor
            elide: Text.ElideMiddle
        }
    }

    component OutputRow: Rectangle {
        id: row
        required property string key
        readonly property var output: root.outputs.find(o => o.id === key)
            || ({ id: key, number: 0, name: "", spec: { kind: "" }, status: { state: "stopped" } })
        readonly property bool isRunning: output.status.state === "running"
        readonly property var link: root.links[output.id]
        property string elapsed: ""
        width: ListView.view ? ListView.view.width : 0
        implicitHeight: rowLayout.implicitHeight + Kirigami.Units.smallSpacing * 2
        color: root.selected !== null && root.selected.id === output.id ? KanteStyle.sunkenColor : "transparent"

        TapHandler {
            onTapped: root.selectedId = row.output.id
        }
        Timer {
            running: row.isRunning
            repeat: true
            triggeredOnStart: true
            interval: 1000
            onTriggered: row.elapsed = root.backend.runningTime(row.output.status.since_ms)
        }

        RowLayout {
            id: rowLayout
            anchors.fill: parent
            anchors.leftMargin: Kirigami.Units.smallSpacing
            anchors.rightMargin: Kirigami.Units.smallSpacing
            QQC2.Label {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                text: row.output.number + " " + row.output.name
                elide: Text.ElideRight
            }
            QQC2.Label {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                text: root.kindName(row.output.spec)
                color: KanteStyle.mutedTextColor
            }
            Item {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 7
                implicitHeight: pill.implicitHeight
                KantePill {
                    id: pill
                    status: row.isRunning ? KantePill.State.Running
                        : (row.output.status.state === "failed" ? KantePill.State.Failed : KantePill.State.Off)
                    text: row.isRunning ? qsTr("running") : (row.output.status.state === "failed" ? qsTr("failed") : qsTr("stopped"))
                }
            }
            QQC2.Label {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                text: row.isRunning ? row.elapsed : "—"
                font.family: KanteStyle.monoFamily
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: row.link ? Texts.linkRate(row.link) : "—"
                font.family: KanteStyle.monoFamily
                color: KanteStyle.mutedTextColor
                elide: Text.ElideRight
            }
            KanteButton {
                size: KanteButton.Size.Small
                emphasis: row.isRunning ? KanteButton.Emphasis.Destructive : KanteButton.Emphasis.Normal
                text: row.isRunning ? qsTr("Stop") : qsTr("Start")
                enabled: root.backend.connected
                onClicked: row.isRunning ? root.stop(row.output) : root.act({ action: "start_output", output: row.output.number })
            }
            KanteButton {
                size: KanteButton.Size.Small
                emphasis: KanteButton.Emphasis.Quiet
                text: qsTr("Remove")
                enabled: root.backend.connected && !row.isRunning
                onClicked: root.act({ action: "remove_output", output: row.output.number })
            }
        }
    }

    StopOutputDialog {
        id: stopDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        onAskAgainChosen: on => root.shell.confirmStop = on
        onMediaRequested: root.stopMedia()
    }

    AddOutputDialog {
        id: addOutputDialog
        parent: QQC2.Overlay.overlay
        core: root.backend
        inputs: root.shell.inputs
    }
}
