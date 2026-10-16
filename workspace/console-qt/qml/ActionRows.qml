import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Up to eight actions run in order, as editor rows. Actions come from a
 * few presets; anything else as JSON, the same text the API takes.
 * Shared by shortcuts, triggers and cues.
 */
ColumnLayout {
    id: editor

    required property var inputs
    required property var outputs
    /** Why the rows cannot be saved; empty when they can. */
    property string error: ""
    /** Rows that hold an action. */
    property int filled: 0

    readonly property int rows: 8
    /** Preset kinds; `param` says what the second field picks. */
    readonly property var kinds: [
        { kind: "", label: qsTr("—"), param: "" },
        { kind: "cut", label: qsTr("Cut"), param: "" },
        { kind: "transition", label: qsTr("Transition button"), param: "button" },
        { kind: "preview_input", label: qsTr("Input into preview"), param: "input" },
        { kind: "cut_direct", label: qsTr("Input onto program"), param: "input" },
        { kind: "quick_play", label: qsTr("Quick play of input"), param: "input" },
        { kind: "go_input", label: qsTr("GO of input"), param: "input" },
        { kind: "overlay_in", label: qsTr("Overlay 1 on with input"), param: "input" },
        { kind: "overlay_out", label: qsTr("Overlay 1 off"), param: "" },
        { kind: "overlay_out_all", label: qsTr("All overlays off"), param: "" },
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
        case "quick_play": return { action: "quick_play", input: n }
        case "go_input": return { action: "go_input", input: n }
        case "overlay_in": return { action: "overlay_in", channel: 1, input: n }
        case "overlay_out": return { action: "overlay_out", channel: 1 }
        case "overlay_out_all": return { action: "overlay_out_all" }
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
        // lmix-core fills in default durations (DEFAULT_OVERLAY_MS, DEFAULT_FTB_MS).
        var overlayDefault = a => !a.duration_ms || a.duration_ms === 500
        var ftbDefault = a => !a.duration_ms || a.duration_ms === 1000
        var a = action
        var plain = Object.keys(a).length
        if (a.action === "cut" && plain === 1) return { kind: "cut", index: 0 }
        if (a.action === "transition" && plain === 2) return { kind: "transition", index: number(a.slot) }
        if (["preview_input", "cut_direct", "quick_play", "go_input"].indexOf(a.action) >= 0 && typeof a.input === "number" && plain === 2)
            return { kind: a.action, index: number(a.input) }
        if (a.action === "overlay_in" && a.channel === 1 && typeof a.input === "number" && plain <= 4 && overlayDefault(a))
            return { kind: "overlay_in", index: number(a.input) }
        if (a.action === "overlay_out" && a.channel === 1 && plain <= 3 && overlayDefault(a)) return { kind: "overlay_out", index: 0 }
        if (a.action === "overlay_out_all" && plain <= 2 && overlayDefault(a)) return { kind: "overlay_out_all", index: 0 }
        if (a.action === "fade_to_black" && ftbDefault(a)) return { kind: "fade_to_black", index: 0 }
        if ((a.action === "start_output" || a.action === "stop_output") && typeof a.output === "number")
            return { kind: a.action, index: number(a.output) }
        if (a.action === "set_audio" && typeof a.input === "number")
            return { kind: a.on ? "audio_on" : "audio_off", index: number(a.input) }
        return { kind: "json", index: 0, json: JSON.stringify(a) }
    }

    /** Fill the rows from actions; the rest stay empty. */
    function load(actions) {
        for (var i = 0; i < rows; i++) {
            var row = actionRows.itemAt(i)
            var r = actions && actions[i] ? rowOf(actions[i]) : { kind: "", index: 0 }
            row.kindIndex = Math.max(0, kinds.findIndex(k => k.kind === r.kind))
            row.paramIndex = r.index
            row.json = r.json || ""
        }
        validate()
    }

    /** The rows as actions, empty rows left out. Only call without `error`. */
    function collect() {
        var actions = []
        for (var i = 0; i < rows; i++) {
            var row = actionRows.itemAt(i)
            var action = actionOf(kinds[row.kindIndex].kind, row.paramIndex, row.json)
            if (action)
                actions.push(action)
        }
        return actions
    }

    /** Counts the rows in use and says which JSON row does not parse. */
    function validate() {
        var count = 0
        for (var r = 0; r < rows; r++) {
            var used = actionRows.itemAt(r)
            if (used && kinds[used.kindIndex].kind !== "")
                count++
        }
        filled = count
        for (var i = 0; i < rows; i++) {
            var row = actionRows.itemAt(i)
            if (!row || kinds[row.kindIndex].kind !== "json")
                continue
            try {
                JSON.parse(row.json)
            } catch (e) {
                error = qsTr("Action %1 is not valid JSON").arg(i + 1)
                return
            }
        }
        error = ""
    }

    spacing: Kirigami.Units.smallSpacing

    KanteSectionLabel { text: qsTr("Actions, in this order") }
    Repeater {
        id: actionRows
        model: editor.rows

        RowLayout {
            id: row
            required property int index
            property alias kindIndex: kindBox.currentIndex
            property alias paramIndex: paramBox.currentIndex
            property alias json: jsonField.text
            onJsonChanged: editor.validate()
            readonly property string param: editor.kinds[kindBox.currentIndex].param
            Layout.fillWidth: true

            QQC2.Label {
                text: (row.index + 1) + "."
                font.family: KanteStyle.monoFamily
            }
            QQC2.ComboBox {
                id: kindBox
                Layout.preferredWidth: Kirigami.Units.gridUnit * 13
                model: editor.kinds.map(k => k.label)
                Accessible.name: qsTr("Action %1").arg(row.index + 1)
                onCurrentIndexChanged: editor.validate()
                KanteFieldSkin { control: kindBox }
            }
            QQC2.ComboBox {
                id: paramBox
                Layout.fillWidth: true
                visible: row.param !== "" && row.param !== "json"
                model: editor.choicesFor(row.param)
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
        Layout.fillWidth: true
        visible: editor.error.length > 0
        text: editor.error
        color: KanteStyle.negativeTextColor
        wrapMode: Text.WordWrap
    }
}
