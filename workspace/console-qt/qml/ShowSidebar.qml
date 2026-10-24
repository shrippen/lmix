import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The Show page's side bar (PLAN-SEITEN, click dummy): what runs, and the
 * cue lists. Running cues fill their row as they go, with the time gone and
 * the time left (SUI.4); a list chosen here becomes the active one, so GO
 * means it. Pause and pads wait for the core (RUN.5, RUN.12).
 *
 *   ACTIVE 2 | LISTS
 *   ▌▓▓▓▓▓▓▓░░░░░░  3 · Rain        ✕
 *   ▌ 0:12.4            −0:47.6
 */
KanteCard {
    id: side

    /** The ShowView: its state, times and actions. */
    required property var showView
    /** Asks for the cue lists dialog (edit mode). */
    signal editLists()

    property int tab: 0

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.smallSpacing
        spacing: Kirigami.Units.smallSpacing

        KanteTabBar {
            Layout.fillWidth: true
            activeFocusOnTab: false
            model: [qsTr("Active"), qsTr("Lists")]
            counts: [String(side.showView.running.length), String(side.showView.lists.length)]
            currentIndex: side.tab
            onActivated: index => {
                side.tab = index
                side.showView.forceActiveFocus()
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: side.tab

            // Running cues.
            ColumnLayout {
                spacing: Kirigami.Units.smallSpacing

                RowLayout {
                    Layout.fillWidth: true
                    KanteSectionLabel {
                        Layout.fillWidth: true
                        text: qsTr("Running: %1").arg(side.showView.running.length)
                    }
                    KanteButton {
                        focusPolicy: Qt.NoFocus
                        size: KanteButton.Size.Small
                        text: qsTr("Stop all")
                        enabled: side.showView.running.length > 0
                        onClicked: side.showView.act({ action: "cue_stop" })
                    }
                }
                QQC2.Label {
                    visible: side.showView.running.length === 0
                    text: qsTr("Nothing runs.")
                    color: KanteStyle.mutedTextColor
                }
                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: 1
                    model: side.showView.running
                    boundsBehavior: Flickable.StopAtBounds
                    QQC2.ScrollBar.vertical: QQC2.ScrollBar {}

                    delegate: Rectangle {
                        id: run
                        required property var modelData
                        readonly property real done: side.showView.elapsed(modelData)
                        readonly property bool timed: !!modelData.total_ms
                        readonly property real progress: timed ? Math.min(1, done / modelData.total_ms) : 0

                        width: ListView.view.width
                        implicitHeight: runColumn.implicitHeight + Kirigami.Units.smallSpacing * 2
                        color: KanteStyle.sunkenColor
                        Accessible.role: Accessible.StaticText
                        Accessible.name: qsTr("Cue %1 %2, %3").arg(modelData.number).arg(modelData.name).arg(timeText.text)

                        // The fill: how far the cue has got.
                        Rectangle {
                            width: parent.width * run.progress
                            height: parent.height
                            color: KanteStyle.tint(KanteStyle.positiveTextColor, 0.22)
                        }
                        Rectangle {
                            width: 3
                            height: parent.height
                            color: KanteStyle.accentColor
                        }

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: Kirigami.Units.largeSpacing
                            anchors.rightMargin: Kirigami.Units.smallSpacing
                            spacing: Kirigami.Units.smallSpacing

                            ColumnLayout {
                                id: runColumn
                                Layout.fillWidth: true
                                spacing: 0
                                QQC2.Label {
                                    Layout.fillWidth: true
                                    text: (run.modelData.number !== "" ? run.modelData.number + " · " : "") + run.modelData.name
                                    elide: Text.ElideRight
                                }
                                RowLayout {
                                    Layout.fillWidth: true
                                    QQC2.Label {
                                        Layout.fillWidth: true
                                        text: run.modelData.phase === "pre_wait" ? qsTr("pre wait %1").arg(side.showView.clock(run.done))
                                            : run.modelData.phase === "post_wait" ? qsTr("post wait %1").arg(side.showView.clock(run.done))
                                            : side.showView.clock(run.done)
                                        font.family: KanteStyle.monoFamily
                                    }
                                    QQC2.Label {
                                        id: timeText
                                        visible: run.timed
                                        text: run.timed ? "−" + side.showView.clock(Math.max(0, run.modelData.total_ms - run.done)) : ""
                                        font.family: KanteStyle.monoFamily
                                        color: KanteStyle.strongTextColor
                                    }
                                }
                            }
                            KanteButton {
                                focusPolicy: Qt.NoFocus
                                size: KanteButton.Size.Small
                                emphasis: KanteButton.Emphasis.Quiet
                                text: "✕"
                                Accessible.name: qsTr("Stop cue %1").arg(run.modelData.number)
                                QQC2.ToolTip.visible: hovered
                                QQC2.ToolTip.text: qsTr("Stop this cue")
                                onClicked: side.showView.act({ action: "cue_stop", list: run.modelData.list, cue: run.modelData.cue })
                            }
                        }
                    }
                }
            }

            // Cue lists: a click makes one the active list.
            ColumnLayout {
                spacing: 0

                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: side.showView.lists
                    boundsBehavior: Flickable.StopAtBounds
                    QQC2.ScrollBar.vertical: QQC2.ScrollBar {}

                    delegate: KanteListRow {
                        required property var modelData
                        width: ListView.view.width
                        focusOnClick: false
                        leadingText: String(modelData.number)
                        text: modelData.name
                        subtitle: modelData.emergency ? qsTr("emergency") : ""
                        meta: qsTr("Cues: %1").arg(modelData.cues.length)
                        selected: side.showView.list !== null && side.showView.list.id === modelData.id
                        onClicked: {
                            side.showView.act({ action: "set_active_cue_list", list: modelData.number })
                            side.showView.forceActiveFocus()
                        }
                    }
                }
                KanteButton {
                    Layout.topMargin: Kirigami.Units.smallSpacing
                    focusPolicy: Qt.NoFocus
                    visible: side.showView.editing
                    text: qsTr("Cues…")
                    onClicked: side.editLists()
                }
            }
        }
    }
}
