import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import Kante

/**
 * Show view (show mode SUI.1–4, SUI.13): the active cue list with its
 * standby, GO, panic and, in the side bar, the running cues and the lists. Choosing a list makes it the
 * active one, so GO here, the space key and GO from outside all mean the
 * same list. Times run on between two states of lmix-core, which sends one
 * when something happens, not every frame.
 */
ColumnLayout {
    id: view

    required property var act
    required property var show
    /** The window's Core (colour names in the inspector). */
    required property var core

    /** More columns: waits and follows (SUI.13). */
    property bool advanced: false
    /** The whole window dimmed for a dark room (SUI.6). */
    property bool darkRoom: false
    /** Groups shown closed, by cue id (SUI.2). */
    property var closed: ({})
    /** Edit mode: the buttons that change cues, zones, targets and the
     *  script show; in show mode only running the show remains. */
    property bool editing: true

    readonly property var lists: show && show.cue_lists ? show.cue_lists : []
    readonly property var list: {
        var active = lists.find(l => l.active)
        return active !== undefined ? active : (lists.length > 0 ? lists[0] : null)
    }
    /** The cue chosen for the inspector, by id; empty: the standby cue.
     *  A new standby (GO, a click on a row) chooses that one again. */
    property string selectedId: ""
    readonly property var selectedCue: {
        var found = list && selectedId !== "" ? findCue(list.cues, selectedId) : null
        return found !== null ? found : standbyCue
    }
    readonly property int standbyAt: list ? list.standby : 0
    onStandbyAtChanged: selectedId = ""
    readonly property var running: show && show.running_cues ? show.running_cues : []
    readonly property var runningIds: running.map(r => r.cue)
    readonly property var standbyCue: list && list.standby <= list.cues.length ? list.cues[list.standby - 1] : null
    readonly property bool fromScript: list !== null && list.source !== undefined && list.source !== null
    /** What would go wrong when the lists run (SUI.5). */
    readonly property var warnings: show && show.cue_warnings ? show.cue_warnings : []
    /** Zones that play nowhere on this machine (SND.16). */
    readonly property var silentZones: show && show.zones ? show.zones.filter(z => z.outputs.length === 0) : []

    /** A warning in the Console's language. */
    function problem(w) {
        switch (w.problem) {
        case "start_cue_gone": return qsTr("starts cue %1, which is not there").arg(w.target)
        case "start_list_gone": return qsTr("starts a cue list that is not there")
        case "target_gone": return qsTr("sends to target %1, which this machine does not have").arg(w.target)
        case "target_unreached": return qsTr("sends to target %1, which did not answer: %2").arg(w.target).arg(w.reason)
        case "input_gone": return qsTr("uses input %1, which is not there").arg(w.input)
        case "media_missing": return qsTr("uses input %1, whose media file is missing").arg(w.input)
        default: return w.message
        }
    }

    /** The cue with `id` in `cues` or their groups, or null. */
    function findCue(cues, id) {
        for (var i = 0; i < cues.length; i++) {
            if (cues[i].id === id) {
                return cues[i]
            }
            var inside = findCue(cues[i].children || [], id)
            if (inside) {
                return inside
            }
        }
        return null
    }

    /** Changes only the fields in `changes` (a JSON merge patch: nested
     *  objects merge, null clears) of the cue with `id` in the active list,
     *  inside groups too. Two quick edits each send only their own field,
     *  so neither brings back an old value of the other; lmix-core keeps
     *  standby and running cues (IMP.5). */
    function updateCue(id, changes) {
        act({ action: "update_cue", list: list.number, cue: id, changes: changes })
    }

    /** "12 Rain" of the cue with `id` in any list, for a warning. */
    function cueLabel(listId, cueId) {
        var owner = lists.find(l => l.id === listId)
        var cue = owner ? findCue(owner.cues, cueId) : null
        if (!cue)
            return ""
        return (cue.number !== "" ? cue.number + " " : "") + cueName(cue)
    }

    /** When the last state came, and now (for running times). */
    property real stateAt: Date.now()
    property real now: Date.now()
    // When the running cues came: `show` stays the same object (ShowState).
    onRunningChanged: stateAt = Date.now()

    Timer {
        interval: 100
        repeat: true
        running: view.visible && view.running.length > 0
        onTriggered: view.now = Date.now()
    }

    function go() {
        act({ action: "cue_go" })
    }

    function panic() {
        act({ action: "cue_panic" })
    }

    function elapsed(run) {
        return run.elapsed_ms + Math.max(0, now - stateAt)
    }

    /** 1:05.3, or 0:12.0 */
    function clock(ms) {
        if (ms === undefined || ms === null)
            return ""
        var tenths = Math.max(0, Math.floor(ms / 100))
        var minutes = Math.floor(tenths / 600)
        var seconds = (tenths % 600) / 10
        return minutes + ":" + (seconds < 10 ? "0" : "") + seconds.toFixed(1)
    }

    function cueName(cue) {
        if (cue.name !== "")
            return cue.name
        switch (cue.kind) {
        case "wait": return qsTr("Wait %1").arg(clock(cue.duration_ms || 0))
        case "note": return qsTr("Note")
        case "group": return qsTr("Group")
        case "start": return cue.target && cue.target.cue ? qsTr("Start %1").arg(cue.target.cue) : qsTr("Start")
        case "sound": return cue.sound ? cue.sound.file.split("/").pop() : qsTr("Sound")
        case "picture": return cue.picture ? cue.picture.file.split("/").pop() : qsTr("Picture")
        case "fade": return fadeName(cue)
        default: return cue.actions && cue.actions.length > 0 ? cue.actions[0].action : qsTr("Actions")
        }
    }

    function fadeName(cue) {
        var target = cue.fade ? cue.fade.target : null
        if (target && target.cue !== undefined)
            return qsTr("Fade %1").arg(target.cue)
        if (target && target.zone !== undefined)
            return qsTr("Fade zone %1").arg(target.zone)
        if (target && target.stage !== undefined)
            return qsTr("Fade stage %1").arg(target.stage)
        return qsTr("Fade")
    }

    function kindLabel(cue) {
        switch (cue.kind) {
        case "wait": return qsTr("Wait")
        case "note": return qsTr("Note")
        case "group": return cue.mode === "first" ? qsTr("Group, first") : qsTr("Group, timeline")
        case "start": return qsTr("Start")
        case "sound": return qsTr("Sound")
        case "picture": return cue.picture ? qsTr("Picture on %1").arg(cue.picture.stage) : qsTr("Picture")
        case "fade": return qsTr("Fade")
        default: return qsTr("Actions: %1").arg(cue.actions ? cue.actions.length : 0)
        }
    }

    function followLabel(follow) {
        switch (follow) {
        case "after_wait": return qsTr("after wait")
        case "after_end": return qsTr("after end")
        default: return ""
        }
    }

    /** The cues as rows: groups open unless closed, children indented. */
    readonly property var rows: {
        var out = []
        function add(cue, depth, top) {
            out.push({ cue: cue, depth: depth, top: top })
            if (cue.kind === "group" && !closed[cue.id])
                cue.children.forEach(child => add(child, depth + 1, top))
        }
        if (list)
            list.cues.forEach((cue, i) => add(cue, 0, i + 1))
        return out
    }

    spacing: Kirigami.Units.largeSpacing

    // Space is GO, Escape panic, twice a hard stop (CTL.6). The view keeps
    // the focus (its buttons take none), so a dialog's Escape stays the
    // dialog's and space never presses a focused button.
    focus: true
    onVisibleChanged: if (visible) forceActiveFocus()
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Space && !event.isAutoRepeat) {
            view.go()
            event.accepted = true
        } else if (event.key === Qt.Key_Escape) {
            view.panic()
            event.accepted = true
        }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing

        KanteSectionLabel { text: qsTr("Cue list") }
        QQC2.ComboBox {
            id: listBox
            Layout.preferredWidth: Kirigami.Units.gridUnit * 16
            visible: view.lists.length > 0
            model: view.lists.map(l => l.number + " " + l.name + (l.emergency ? " · " + qsTr("emergency") : ""))
            currentIndex: view.list ? view.lists.indexOf(view.list) : -1
            focusPolicy: Qt.NoFocus
            onActivated: index => {
                view.act({ action: "set_active_cue_list", list: view.lists[index].number })
                view.forceActiveFocus()
            }
            KanteFieldSkin { control: listBox }
        }
        QQC2.Label {
            Layout.fillWidth: true
            visible: view.fromScript
            text: view.fromScript ? qsTr("From %1").arg(view.list.source.replace(/^script:/, "")) : ""
            color: KanteStyle.mutedTextColor
            elide: Text.ElideMiddle
        }
        Item { Layout.fillWidth: !view.fromScript }
        QQC2.Label {
            visible: view.show !== null && !!view.show.script_folder
            text: view.show && view.show.script_folder ? qsTr("Script: %1").arg(view.show.script_folder) : ""
            color: KanteStyle.mutedTextColor
            elide: Text.ElideMiddle
            Layout.maximumWidth: Kirigami.Units.gridUnit * 16
        }
        KanteButton {
            focusPolicy: Qt.NoFocus
            visible: view.editing
            text: qsTr("Cues…")
            onClicked: cueListsDialog.open()
        }
        CueListsDialog {
            id: cueListsDialog
            parent: QQC2.Overlay.overlay
            anchors.centerIn: parent
            act: view.act
            inputs: view.show ? view.show.inputs : []
            outputs: view.show ? view.show.outputs : []
            lists: view.lists
            onClosed: view.forceActiveFocus()
        }
        KanteButton {
            focusPolicy: Qt.NoFocus
            visible: view.editing
            text: qsTr("Zones…")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: qsTr("The show's sound zones and where they play on this machine")
            onClicked: zonesDialog.open()
        }
        ZonesDialog {
            id: zonesDialog
            parent: QQC2.Overlay.overlay
            anchors.centerIn: parent
            act: view.act
            zones: view.show && view.show.zones ? view.show.zones : []
            onClosed: view.forceActiveFocus()
        }
        KanteButton {
            focusPolicy: Qt.NoFocus
            visible: view.editing
            text: qsTr("Targets…")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: qsTr("Output targets (HTTP, Home Assistant, OSC) and OSC subscribers of this machine")
            onClicked: targetsDialog.open()
        }
        KanteButton {
            focusPolicy: Qt.NoFocus
            text: qsTr("Check targets")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: qsTr("Ask every target the cues send to; those that do not answer show up as warnings")
            onClicked: view.act({ action: "check_targets" })
        }
        TargetsDialog {
            id: targetsDialog
            parent: QQC2.Overlay.overlay
            anchors.centerIn: parent
            act: view.act
            targets: view.show && view.show.targets ? view.show.targets : []
            feeds: view.show && view.show.feeds ? view.show.feeds : []
            onClosed: view.forceActiveFocus()
        }
        KanteButton {
            focusPolicy: Qt.NoFocus
            visible: view.editing
            text: qsTr("Script…")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: qsTr("A folder of Markdown files with lmix-cue blocks; its lists follow every saved change")
            onClicked: scriptDialog.open()
        }
        KanteButton {
            focusPolicy: Qt.NoFocus
            visible: view.show !== null && !!view.show.script_folder
            text: qsTr("No script")
            onClicked: view.act({ action: "set_script_folder", folder: null })
        }
        FolderDialog {
            id: scriptDialog
            title: qsTr("Script folder")
            onAccepted: {
                var path = decodeURIComponent(selectedFolder.toString().replace(/^file:\/\//, ""))
                view.act({ action: "set_script_folder", folder: path })
                view.forceActiveFocus()
            }
        }
        KanteButton {
            focusPolicy: Qt.NoFocus
            checkable: true
            checked: view.darkRoom
            text: qsTr("Dark")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: qsTr("Dim the window for a dark room")
            onToggled: view.darkRoom = checked
        }
        QQC2.CheckBox {
            id: advancedBox
            text: qsTr("Advanced")
            checked: view.advanced
            focusPolicy: Qt.NoFocus
            onToggled: view.advanced = checked
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: qsTr("Show waits and follows of every cue")
            KanteCheckSkin { control: advancedBox }
        }
    }

    // Warnings before the show (SUI.5).
    ColumnLayout {
        Layout.fillWidth: true
        visible: view.warnings.length + view.silentZones.length > 0
        spacing: 0

        KanteSectionLabel {
            text: qsTr("Warnings: %1").arg(view.warnings.length + view.silentZones.length)
            color: KanteStyle.warningColor
        }
        Repeater {
            model: view.silentZones
            QQC2.Label {
                required property var modelData
                Layout.fillWidth: true
                text: qsTr("Zone %1 plays nowhere on this machine (Zones…)").arg(modelData.name)
                color: KanteStyle.warningColor
                elide: Text.ElideRight
            }
        }
        Repeater {
            model: view.warnings.slice(0, 5)
            QQC2.Label {
                required property var modelData
                Layout.fillWidth: true
                text: qsTr("Cue %1: %2").arg(view.cueLabel(modelData.list, modelData.cue)).arg(view.problem(modelData))
                color: KanteStyle.warningColor
                elide: Text.ElideRight
            }
        }
        QQC2.Label {
            visible: view.warnings.length > 5
            text: qsTr("… and %1 more").arg(view.warnings.length - 5)
            color: KanteStyle.mutedTextColor
        }
    }

    QQC2.Label {
        Layout.fillWidth: true
        visible: view.lists.length === 0
        wrapMode: Text.WordWrap
        text: qsTr("No cue lists yet. Add one under Cues… or choose a script folder with Script….")
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: Kirigami.Units.largeSpacing
        visible: view.list !== null

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: Kirigami.Units.largeSpacing

            KanteCard {
                Layout.fillWidth: true
                Layout.fillHeight: true

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: Kirigami.Units.smallSpacing
                    spacing: 0

                    // Column heads.
                    RowLayout {
                        Layout.fillWidth: true
                        Layout.bottomMargin: Kirigami.Units.smallSpacing
                        KanteSectionLabel { Layout.preferredWidth: Kirigami.Units.gridUnit * 4; text: qsTr("No.") }
                        KanteSectionLabel { Layout.fillWidth: true; text: qsTr("Cue") }
                        KanteSectionLabel { Layout.preferredWidth: Kirigami.Units.gridUnit * 8; text: qsTr("Kind") }
                        KanteSectionLabel { visible: view.advanced; Layout.preferredWidth: Kirigami.Units.gridUnit * 4; text: qsTr("Pre") }
                        KanteSectionLabel { visible: view.advanced; Layout.preferredWidth: Kirigami.Units.gridUnit * 4; text: qsTr("Post") }
                        KanteSectionLabel { visible: view.advanced; Layout.preferredWidth: Kirigami.Units.gridUnit * 6; text: qsTr("Follow") }
                    }

                    ListView {
                        id: cueView
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        model: view.rows
                        currentIndex: view.rows.findIndex(r => r.depth === 0 && r.top === (view.list ? view.list.standby : 0))
                        highlightFollowsCurrentItem: false
                        onCurrentIndexChanged: if (currentIndex >= 0) positionViewAtIndex(currentIndex, ListView.Contain)
                        QQC2.ScrollBar.vertical: QQC2.ScrollBar {}

                        delegate: Rectangle {
                            id: row
                            required property var modelData
                            required property int index
                            readonly property var cue: modelData.cue
                            readonly property bool standby: modelData.depth === 0 && view.list.standby === modelData.top
                            readonly property bool runs: view.runningIds.indexOf(cue.id) >= 0
                            readonly property bool chosen: view.editing && inspector.cue !== null && inspector.cue.id === cue.id

                            width: cueView.width
                            implicitHeight: rowLayout.implicitHeight + Kirigami.Units.smallSpacing * 2
                            color: standby ? KanteStyle.selectionColor : (chosen ? KanteStyle.tintHighlightColor : "transparent")
                            border.width: chosen ? 1 : 0
                            border.color: KanteStyle.focusColor

                            Rectangle {
                                width: Kirigami.Units.smallSpacing
                                height: parent.height
                                visible: row.runs
                                color: KanteStyle.accentColor
                            }

                            TapHandler {
                                onTapped: {
                                    view.forceActiveFocus()
                                    // A cue in a group is only chosen; standby
                                    // is always a cue at the top.
                                    view.selectedId = row.cue.id
                                    if (row.modelData.depth === 0) {
                                        view.act({ action: "cue_standby", to: { index: row.modelData.top } })
                                    }
                                }
                            }

                            RowLayout {
                                id: rowLayout
                                anchors.fill: parent
                                anchors.leftMargin: Kirigami.Units.largeSpacing
                                anchors.rightMargin: Kirigami.Units.smallSpacing

                                QQC2.Label {
                                    Layout.preferredWidth: Kirigami.Units.gridUnit * 4
                                    text: (row.standby ? "▶ " : "") + row.cue.number
                                    font.family: KanteStyle.monoFamily
                                }
                                RowLayout {
                                    Layout.fillWidth: true
                                    Layout.leftMargin: row.modelData.depth * Kirigami.Units.gridUnit
                                    spacing: Kirigami.Units.smallSpacing
                                    KanteButton {
                                        focusPolicy: Qt.NoFocus
                                        visible: row.cue.kind === "group"
                                        size: KanteButton.Size.Small
                                        emphasis: KanteButton.Emphasis.Quiet
                                        text: view.closed[row.cue.id] ? "▸" : "▾"
                                        Accessible.name: view.closed[row.cue.id] ? qsTr("Open group") : qsTr("Close group")
                                        onClicked: {
                                            var next = Object.assign({}, view.closed)
                                            next[row.cue.id] = !view.closed[row.cue.id]
                                            view.closed = next
                                        }
                                    }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 0
                                        QQC2.Label {
                                            Layout.fillWidth: true
                                            text: view.cueName(row.cue)
                                            font.bold: row.standby
                                            font.italic: row.cue.kind === "note"
                                            elide: Text.ElideRight
                                        }
                                        QQC2.Label {
                                            Layout.fillWidth: true
                                            visible: row.cue.note !== undefined && row.cue.note !== ""
                                            text: row.cue.note || ""
                                            color: KanteStyle.mutedTextColor
                                            elide: Text.ElideRight
                                            maximumLineCount: 1
                                        }
                                    }
                                }
                                QQC2.Label {
                                    Layout.preferredWidth: Kirigami.Units.gridUnit * 8
                                    text: view.kindLabel(row.cue) + (row.cue.duration_ms ? " · " + view.clock(row.cue.duration_ms) : "")
                                    color: KanteStyle.mutedTextColor
                                    elide: Text.ElideRight
                                }
                                QQC2.Label {
                                    visible: view.advanced
                                    Layout.preferredWidth: Kirigami.Units.gridUnit * 4
                                    text: row.cue.pre_wait_ms ? view.clock(row.cue.pre_wait_ms) : ""
                                    font.family: KanteStyle.monoFamily
                                }
                                QQC2.Label {
                                    visible: view.advanced
                                    Layout.preferredWidth: Kirigami.Units.gridUnit * 4
                                    text: row.cue.post_wait_ms ? view.clock(row.cue.post_wait_ms) : ""
                                    font.family: KanteStyle.monoFamily
                                }
                                QQC2.Label {
                                    visible: view.advanced
                                    Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                                    text: view.followLabel(row.cue.follow)
                                    color: KanteStyle.mutedTextColor
                                }
                            }
                        }
                    }
                }
            }

            ShowInspector {
                id: inspector
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 11
                visible: view.editing
                showView: view
                core: view.core
            }
        }

        // Standby, GO and panic (SUI.3).
        ColumnLayout {
            Layout.preferredWidth: Kirigami.Units.gridUnit * 18
            Layout.maximumWidth: Kirigami.Units.gridUnit * 18
            Layout.fillHeight: true
            spacing: Kirigami.Units.largeSpacing

            KanteSectionLabel { text: qsTr("Standby") }
            KanteHeading {
                Layout.fillWidth: true
                level: 1
                text: view.standbyCue ? (view.standbyCue.number !== "" ? view.standbyCue.number + "  " : "") + view.cueName(view.standbyCue) : qsTr("End of the list")
                wrapMode: Text.WordWrap
            }
            QQC2.Label {
                Layout.fillWidth: true
                visible: view.standbyCue !== null && view.standbyCue.note !== undefined && view.standbyCue.note !== ""
                text: view.standbyCue && view.standbyCue.note ? view.standbyCue.note : ""
                wrapMode: Text.WordWrap
                font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.3
            }
            KanteButton {
                focusPolicy: Qt.NoFocus
                Layout.fillWidth: true
                size: KanteButton.Size.Large
                emphasis: KanteButton.Emphasis.Primary
                raised: true
                text: qsTr("GO")
                enabled: view.standbyCue !== null
                onClicked: view.go()
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Start the standby cue (space)")
            }
            RowLayout {
                Layout.fillWidth: true
                KanteButton {
                    focusPolicy: Qt.NoFocus
                    Layout.fillWidth: true
                    text: qsTr("Back")
                    enabled: view.list !== null && view.list.standby > 1
                    onClicked: view.act({ action: "cue_standby", to: "previous" })
                }
                KanteButton {
                    focusPolicy: Qt.NoFocus
                    Layout.fillWidth: true
                    text: qsTr("Next")
                    enabled: view.list !== null && view.list.standby <= view.list.cues.length
                    onClicked: view.act({ action: "cue_standby", to: "next" })
                }
            }
            Item { Layout.fillHeight: true }
            KanteButton {
                focusPolicy: Qt.NoFocus
                Layout.fillWidth: true
                size: KanteButton.Size.Large
                emphasis: KanteButton.Emphasis.Destructive
                text: qsTr("Panic")
                onClicked: view.panic()
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Stop every cue over the panic time; again: at once (Escape)")
            }
            QQC2.Label {
                Layout.fillWidth: true
                // Release: a MIDI or OSC button gives the next GO only once let
                // go (RUN.6); space ignores its own repeats anyway.
                text: view.show ? qsTr("Panic time %1 s · GO guard %2 ms").arg((view.show.panic_ms / 1000).toFixed(1)).arg(view.show.go_guard_ms)
                                  + (view.show.go_rearm === "release" ? qsTr(" · GO again once let go") : "") : ""
                color: KanteStyle.mutedTextColor
                font.family: KanteStyle.monoFamily
                wrapMode: Text.WordWrap
            }
        }

        // What runs (SUI.4) and the lists.
        ShowSidebar {
            Layout.preferredWidth: Kirigami.Units.gridUnit * 16
            Layout.maximumWidth: Kirigami.Units.gridUnit * 16
            Layout.fillHeight: true
            showView: view
            onEditLists: cueListsDialog.open()
        }
    }
}
