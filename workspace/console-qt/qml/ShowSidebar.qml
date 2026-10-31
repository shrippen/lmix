import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The Show page's side bar (PLAN-SEITEN, click dummy): what runs, the
 * cue lists and the moods (MOOD.6). Running cues fill their row as they go, with the time gone and
 * the time left (SUI.4); a list chosen here becomes the active one, so GO
 * means it. Pause and pads wait for the core (RUN.5, RUN.12).
 *
 * A click on a mood sets it; in edit mode "+" adds a cue that sets it,
 * after the chosen cue.
 *
 *   ACTIVE 2 | LISTS | MOODS
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
            model: [qsTr("Active"), qsTr("Lists"), qsTr("Moods")]
            counts: [String(side.showView.running.length), String(side.showView.lists.length), String(side.showView.moods.length)]
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
                        id: pauseAll
                        // Resume when anything is paused, else pause (RUN.5).
                        readonly property bool anyPaused: side.showView.running.some(r => r.paused)
                        focusPolicy: Qt.NoFocus
                        size: KanteButton.Size.Small
                        text: pauseAll.anyPaused ? qsTr("Resume all") : qsTr("Pause all")
                        enabled: side.showView.running.length > 0
                        onClicked: side.showView.act({ action: pauseAll.anyPaused ? "cue_resume" : "cue_pause" })
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
                                        text: (run.modelData.paused ? qsTr("paused") + " · " : "")
                                            + (run.modelData.phase === "pre_wait" ? qsTr("pre wait %1").arg(side.showView.clock(run.done))
                                            : run.modelData.phase === "post_wait" ? qsTr("post wait %1").arg(side.showView.clock(run.done))
                                            : side.showView.clock(run.done))
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
                                text: run.modelData.paused ? "▶" : "⏸"
                                Accessible.name: run.modelData.paused ? qsTr("Resume cue %1").arg(run.modelData.number)
                                    : qsTr("Pause cue %1").arg(run.modelData.number)
                                QQC2.ToolTip.visible: hovered
                                QQC2.ToolTip.text: run.modelData.paused ? qsTr("Resume this cue") : qsTr("Pause this cue")
                                onClicked: side.showView.act({
                                    action: run.modelData.paused ? "cue_resume" : "cue_pause",
                                    list: run.modelData.list,
                                    cue: run.modelData.cue
                                })
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

            // Moods: a click sets one, "+" makes it a cue.
            ColumnLayout {
                spacing: 0

                QQC2.Label {
                    Layout.fillWidth: true
                    visible: side.showView.moods.length === 0
                    text: qsTr("No moods yet: they come on the Moods page.")
                    wrapMode: Text.Wrap
                    color: KanteStyle.mutedTextColor
                }
                ListView {
                    objectName: "moods"
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: side.showView.moods
                    boundsBehavior: Flickable.StopAtBounds
                    QQC2.ScrollBar.vertical: QQC2.ScrollBar {}

                    delegate: KanteListRow {
                        id: moodRow
                        required property var modelData
                        objectName: "mood:" + modelData.name
                        width: ListView.view.width
                        focusOnClick: false
                        density: KanteListRow.Density.Compact
                        text: modelData.variant ? modelData.variant.name : modelData.group.name
                        // Variants under their group's base.
                        leadingText: modelData.variant ? "›" : ""
                        leadingWidth: Kirigami.Units.gridUnit
                        selected: side.showView.show !== null && side.showView.show.mood
                            && side.showView.show.mood.mood === modelData.id
                        opacity: moodDrag.active ? 0.5 : 1
                        // Into the list while editing: a cue that sets it
                        // (MOOD.6). On an item over the whole row: the row's
                        // default property is its marker.
                        Item {
                            parent: moodRow
                            anchors.fill: parent
                            DragHandler {
                                id: moodDrag
                                target: null
                                enabled: side.showView.editing && !side.showView.fromScript && side.showView.list !== null
                                onCentroidChanged: if (active) side.showView.moodDragMoved(centroid.scenePosition)
                                onActiveChanged: if (!active) side.showView.moodDropped(moodRow.modelData.id, centroid.scenePosition)
                            }
                        }
                        onClicked: {
                            side.showView.act({ action: "set_mood", mood: modelData.id })
                            side.showView.forceActiveFocus()
                        }
                        KanteSwatch {
                            visible: !moodRow.modelData.variant && !!moodRow.modelData.group.color
                            swatchColor: moodRow.modelData.group.color ? KanteStyle.labelColor(moodRow.modelData.group.color) : "transparent"
                            size: KanteSwatch.Size.Small
                        }
                        trailing: KanteButton {
                            objectName: "addMood"
                            visible: side.showView.editing && !side.showView.fromScript && side.showView.list !== null
                            focusPolicy: Qt.NoFocus
                            size: KanteButton.Size.Small
                            text: "+"
                            Accessible.name: qsTr("Add a cue that sets %1").arg(moodRow.modelData.name)
                            QQC2.ToolTip.visible: hovered
                            QQC2.ToolTip.text: qsTr("Add a cue that sets this mood, after the chosen cue")
                            onClicked: side.showView.addCue({ kind: "mood", mood: { mood: moodRow.modelData.id } },
                                                            side.showView.selectedCue !== null ? side.showView.selectedCue.id : "")
                        }
                    }
                }
            }
        }
    }
}
