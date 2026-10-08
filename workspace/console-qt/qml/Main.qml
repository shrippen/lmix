import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante
import org.lmix.console

Kirigami.ApplicationWindow {
    id: root

    title: "lmix"
    width: Kirigami.Units.gridUnit * 80
    height: Kirigami.Units.gridUnit * 48
    color: KanteStyle.backgroundColor

    Core {
        id: backend
    }

    Binding {
        target: KanteStyle
        property: "kind"
        value: backend.style
    }

    KanteScope {
        target: root.contentItem
    }

    readonly property var show: backend.state.length > 0 ? JSON.parse(backend.state) : null
    readonly property var inputs: show ? show.inputs : []

    function inputById(id) {
        for (var i = 0; i < inputs.length; i++) {
            if (inputs[i].id === id) {
                return inputs[i]
            }
        }
        return null
    }

    readonly property var previewInput: show && show.preview ? inputById(show.preview) : null
    readonly property var programInput: show && show.program ? inputById(show.program) : null

    pageStack.initialPage: Kirigami.Page {
        title: "lmix"
        padding: Kirigami.Units.largeSpacing

        actions: [
            Kirigami.Action {
                displayComponent: KantePill {
                    status: backend.connected ? KantePill.State.Running : KantePill.State.Failed
                    text: backend.connected ? qsTr("lmix-core") : qsTr("lmix-core offline")
                }
            }
        ]

        ColumnLayout {
            anchors.fill: parent
            spacing: Kirigami.Units.largeSpacing

            RowLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: Kirigami.Units.largeSpacing

                Monitor {
                    label: qsTr("Preview")
                    detail: root.previewInput ? root.previewInput.number + " " + root.previewInput.name : ""
                    tally: KanteStyle.positiveTextColor
                    source: root.previewInput ? root.previewInput.id : ""
                }

                ColumnLayout {
                    Layout.alignment: Qt.AlignVCenter
                    Layout.fillWidth: false
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 8
                    spacing: Kirigami.Units.smallSpacing

                    KanteButton {
                        Layout.fillWidth: true
                        text: qsTr("Cut")
                        emphasis: KanteButton.Emphasis.Primary
                        size: KanteButton.Size.Large
                        enabled: backend.connected
                        onClicked: backend.cut()
                    }
                    Repeater {
                        model: 4
                        KanteButton {
                            required property int index
                            Layout.fillWidth: true
                            text: root.show && root.show.transitions[index] ? backend.effectLabel(root.show.transitions[index].effect) : (index + 1)
                            enabled: backend.connected
                            onClicked: backend.transition(index + 1)
                        }
                    }
                }

                Monitor {
                    label: qsTr("Program")
                    detail: root.programInput ? root.programInput.number + " " + root.programInput.name : ""
                    tally: KanteStyle.negativeTextColor
                    source: "program"
                }
            }

            GridView {
                id: bar
                Layout.fillWidth: true
                Layout.preferredHeight: cellHeight * Math.min(2, Math.ceil(count / Math.max(1, Math.floor(width / cellWidth))))
                cellWidth: Kirigami.Units.gridUnit * 12 + Kirigami.Units.smallSpacing
                cellHeight: Kirigami.Units.gridUnit * 9
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: root.inputs
                QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                delegate: InputTile {
                    required property var modelData
                    width: bar.cellWidth - Kirigami.Units.smallSpacing
                    height: bar.cellHeight - Kirigami.Units.smallSpacing
                    core: backend
                    input: modelData
                    onProgram: root.show !== null && root.show.program === modelData.id
                    onPreview: root.show !== null && root.show.preview === modelData.id
                }
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: backend.message.length > 0
                text: backend.message
                color: KanteStyle.negativeTextColor
                wrapMode: Text.Wrap
            }
        }
    }

    component Monitor: KanteCard {
        id: monitor

        property string label
        property string detail
        property color tally
        property alias source: view.source

        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: 1
        barColor: tally

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: Kirigami.Units.smallSpacing
            anchors.topMargin: monitor.barHeight + Kirigami.Units.smallSpacing

            RowLayout {
                KanteSectionLabel {
                    text: monitor.label
                    color: monitor.tally
                }
                KanteSectionLabel {
                    Layout.fillWidth: true
                    text: monitor.detail
                    elide: Text.ElideRight
                }
            }
            LiveView {
                id: view
                Layout.fillWidth: true
                Layout.fillHeight: true
                core: backend
            }
        }
    }
}
