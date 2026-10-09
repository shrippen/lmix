import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * One shortcut: label, key (pressed, not typed), shown on the web page
 * "Keys", and up to eight actions run in order. Actions come from a few
 * presets; anything else as JSON, the same text the API takes.
 */
KanteDialog {
    id: dialog

    required property var act
    required property var inputs
    required property var outputs
    /** Shortcut being edited; `null` adds one. Set before `open()`. */
    property var shortcut: null

    readonly property int rows: 8
    /** Preset kinds; `param` says what the second field picks. */
    readonly property var kinds: [
        { kind: "", label: qsTr("—"), param: "" },
        { kind: "cut", label: qsTr("Cut"), param: "" },
        { kind: "transition", label: qsTr("Transition button"), param: "button" },
        { kind: "preview_input", label: qsTr("Input into preview"), param: "input" },
        { kind: "cut_direct", label: qsTr("Input onto program"), param: "input" },
        { kind: "overlay_in", label: qsTr("Overlay 1 on with input"), param: "input" },
        { kind: "overlay_out", label: qsTr("Overlay 1 off"), param: "" },
        { kind: "fade_to_black", label: qsTr("Fade to black"), param: "" },
        { kind: "start_output", label: qsTr("Start output"), param: "output" },
        { kind: "stop_output", label: qsTr("Stop output"), param: "output" },
        { kind: "audio_on", label: qsTr("Sound of input on"), param: "input" },
        { kind: "audio_off", label: qsTr("Sound of input off"), param: "input" },
        { kind: "json", label: qsTr("Other (JSON)"), param: "json" }
    ]

    function choicesFor(param) {
        if (param === "input")
            return inputs.map(i => i.number + " " + i.name)
        if (param === "output")
            return outputs.map(o => o.number + " " + o.name)
        if (param === "button")
            return ["1", "2", "3", "4"]
        return []
    }

    /** Preset row → action; `null` for an empty row. */
    function actionOf(kind, index, json) {
        var n = index + 1
        switch (kind) {
        case "": return null
        case "cut": return { action: "cut" }
        case "transition": return { action: "transition", slot: n }
        case "preview_input": return { action: "preview_input", input: n }
        case "cut_direct": return { action: "cut_direct", input: n }
        case "overlay_in": return { action: "overlay_in", channel: 1, input: n }
        case "overlay_out": return { action: "overlay_out", channel: 1 }
        case "fade_to_black": return { action: "fade_to_black" }
        case "start_output": return { action: "start_output", output: n }
        case "stop_output": return { action: "stop_output", output: n }
        case "audio_on": return { action: "set_audio", input: n, on: true }
        case "audio_off": return { action: "set_audio", input: n, on: false }
        default: return JSON.parse(json)
        }
    }

    /** Action → preset row { kind, index, json }; JSON when it is no preset. */
    function rowOf(action) {
        var number = v => typeof v === "number" ? v - 1 : 0
        var a = action
        var plain = Object.keys(a).length
        if (a.action === "cut" && plain === 1) return { kind: "cut", index: 0 }
        if (a.action === "transition" && plain === 2) return { kind: "transition", index: number(a.slot) }
        if ((a.action === "preview_input" || a.action === "cut_direct") && typeof a.input === "number" && plain === 2)
            return { kind: a.action, index: number(a.input) }
        if (a.action === "overlay_in" && a.channel === 1 && typeof a.input === "number" && plain <= 4 && !a.duration_ms)
            return { kind: "overlay_in", index: number(a.input) }
        if (a.action === "overlay_out" && a.channel === 1 && plain <= 3 && !a.duration_ms) return { kind: "overlay_out", index: 0 }
        if (a.action === "fade_to_black" && !a.duration_ms) return { kind: "fade_to_black", index: 0 }
        if ((a.action === "start_output" || a.action === "stop_output") && typeof a.output === "number")
            return { kind: a.action, index: number(a.output) }
        if (a.action === "set_audio" && typeof a.input === "number")
            return { kind: a.on ? "audio_on" : "audio_off", index: number(a.input) }
        return { kind: "json", index: 0, json: JSON.stringify(a) }
    }

    title: shortcut ? qsTr("Shortcut %1").arg(shortcut.label) : qsTr("New shortcut")
    modal: true
    width: Kirigami.Units.gridUnit * 38
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    onOpened: {
        label.text = shortcut ? shortcut.label : ""
        keyButton.sequence = shortcut && shortcut.key ? shortcut.key : ""
        web.checked = shortcut ? shortcut.web : true
        for (var i = 0; i < rows; i++) {
            var row = actionRows.itemAt(i)
            var r = shortcut && shortcut.actions[i] ? rowOf(shortcut.actions[i]) : { kind: "", index: 0 }
            row.kindIndex = Math.max(0, kinds.findIndex(k => k.kind === r.kind))
            row.paramIndex = r.index
            row.json = r.json || ""
        }
        validate()
    }

    /** Says which JSON row does not parse; OK waits for valid rows. */
    function validate() {
        for (var i = 0; i < rows; i++) {
            var row = actionRows.itemAt(i)
            if (!row || kinds[row.kindIndex].kind !== "json")
                continue
            try {
                JSON.parse(row.json)
            } catch (e) {
                error.text = qsTr("Action %1 is not valid JSON").arg(i + 1)
                return
            }
        }
        error.text = ""
    }

    Component.onCompleted: standardButton(QQC2.Dialog.Ok).enabled = Qt.binding(() => error.text === "" && label.text.trim() !== "")

    onAccepted: {
        var actions = []
        for (var i = 0; i < rows; i++) {
            var row = actionRows.itemAt(i)
            var action = actionOf(kinds[row.kindIndex].kind, row.paramIndex, row.json)
            if (action)
                actions.push(action)
        }
        var spec = { label: label.text.trim(), key: keyButton.sequence || null, web: web.checked, actions: actions }
        if (shortcut)
            act({ action: "set_shortcut", target: shortcut.number, shortcut: spec })
        else
            act({ action: "add_shortcut", shortcut: spec })
    }

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            KanteTextField {
                id: label
                Layout.fillWidth: true
                placeholderText: qsTr("Label")
            }
            // Press a key to set it; Escape clears.
            KanteButton {
                id: keyButton
                property string sequence: ""
                property bool listening: false
                text: listening ? qsTr("Press a key…") : (sequence || qsTr("No key"))
                onClicked: {
                    listening = true
                    forceActiveFocus()
                }
                onActiveFocusChanged: if (!activeFocus) listening = false
                Keys.onPressed: event => {
                    if (!listening)
                        return
                    event.accepted = true
                    if (event.key === Qt.Key_Escape) {
                        sequence = ""
                        listening = false
                        return
                    }
                    var modifier = [Qt.Key_Control, Qt.Key_Shift, Qt.Key_Alt, Qt.Key_Meta].indexOf(event.key) >= 0
                    if (modifier)
                        return
                    var parts = []
                    if (event.modifiers & Qt.ControlModifier) parts.push("Ctrl")
                    if (event.modifiers & Qt.AltModifier) parts.push("Alt")
                    if (event.modifiers & Qt.ShiftModifier) parts.push("Shift")
                    var name = event.key >= Qt.Key_F1 && event.key <= Qt.Key_F35 ? "F" + (event.key - Qt.Key_F1 + 1)
                        : event.key === Qt.Key_Space ? "Space"
                        : event.key === Qt.Key_Return || event.key === Qt.Key_Enter ? "Return"
                        : event.text.toUpperCase()
                    if (!name)
                        return
                    parts.push(name)
                    sequence = parts.join("+")
                    listening = false
                }
            }
            QQC2.CheckBox {
                id: web
                text: qsTr("Web page")
                KanteCheckSkin { control: web }
            }
        }

        KanteSectionLabel { text: qsTr("Actions, in this order") }
        Repeater {
            id: actionRows
            model: dialog.rows

            RowLayout {
                id: row
                required property int index
                property alias kindIndex: kindBox.currentIndex
                property alias paramIndex: paramBox.currentIndex
                property alias json: jsonField.text
                onJsonChanged: dialog.validate()
                readonly property string param: dialog.kinds[kindBox.currentIndex].param
                Layout.fillWidth: true

                QQC2.Label {
                    text: (row.index + 1) + "."
                    font.family: KanteStyle.monoFamily
                }
                QQC2.ComboBox {
                    id: kindBox
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 13
                    model: dialog.kinds.map(k => k.label)
                    Accessible.name: qsTr("Action %1").arg(row.index + 1)
                    onCurrentIndexChanged: dialog.validate()
                    KanteFieldSkin { control: kindBox }
                }
                QQC2.ComboBox {
                    id: paramBox
                    Layout.fillWidth: true
                    visible: row.param !== "" && row.param !== "json"
                    model: dialog.choicesFor(row.param)
                    KanteFieldSkin { control: paramBox }
                }
                KanteTextField {
                    id: jsonField
                    Layout.fillWidth: true
                    visible: row.param === "json"
                    placeholderText: '{"action": "…"}'
                    font.family: KanteStyle.monoFamily
                }
                Item {
                    Layout.fillWidth: true
                    visible: row.param === ""
                }
            }
        }

        QQC2.Label {
            id: error
            Layout.fillWidth: true
            visible: text.length > 0
            color: KanteStyle.negativeTextColor
            wrapMode: Text.WordWrap
        }
    }
}
