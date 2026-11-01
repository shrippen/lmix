import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Up to sixteen actions run in order, as editor rows (lmix-core's
 * AUTOMATION_ACTIONS and SHORTCUT_ACTIONS): eight show, another one
 * each time the last shown is used. Actions come from a
 * few presets; anything else as JSON, the same text the API takes.
 * Shared by shortcuts, triggers and cues. An OSC message (EXT.6) has
 * fields of its own: target, address, arguments as lmix-core reads them.
 */
ColumnLayout {
    id: editor

    required property var inputs
    required property var outputs
    /** Output targets of this machine (`state.targets`); OSC ones are offered. */
    property var targets: []
    /** Why the rows cannot be saved; empty when they can. */
    property string error: ""
    /** Rows that hold an action. */
    property int filled: 0
    /** The last row that holds an action, -1 for none. */
    property int lastUsed: -1

    readonly property int rows: 16
    /** Rows shown: eight, and one after the last used. */
    readonly property int shown: Math.max(8, Math.min(rows, lastUsed + 2))
    /** Names of the targets that speak OSC. */
    readonly property var oscTargets: (targets || []).filter(t => t.kind !== "http").map(t => t.name)
    // A target added or removed while open: the "needs a target" error follows.
    onOscTargetsChanged: validate()
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
        { kind: "overlay_preview", label: qsTr("Overlay 1 with the next take"), param: "input" },
        { kind: "fade_to_black", label: qsTr("Fade to black"), param: "" },
        { kind: "start_output", label: qsTr("Start output"), param: "output" },
        { kind: "stop_output", label: qsTr("Stop output"), param: "output" },
        { kind: "start_streams", label: qsTr("Start all streams"), param: "" },
        { kind: "stop_streams", label: qsTr("Stop all streams"), param: "" },
        { kind: "mark_recording", label: qsTr("Mark in recordings"), param: "" },
        { kind: "audio_on", label: qsTr("Sound of input on"), param: "input" },
        { kind: "audio_off", label: qsTr("Sound of input off"), param: "input" },
        { kind: "osc", label: qsTr("OSC message"), param: "osc" },
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

    /** Longest OSC address in bytes (lmix-core OSC_ADDRESS_MAX). */
    readonly property int oscAddressMax: 256

    /** An OSC address as lmix-core takes it (lmix_types::osc): `/`, then
     *  printable ASCII without `#` and `,`. */
    function oscAddressOk(address) {
        return /^\/[!-~]*$/.test(address) && !/[#,]/.test(address) && address.length <= oscAddressMax
    }

    /** Preset row → action; `null` for an empty row. `osc`: target,
     *  address and arguments of an OSC row. */
    function actionOf(kind, index, json, osc) {
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
        case "overlay_preview": return { action: "overlay_preview", channel: 1, input: n }
        case "fade_to_black": return { action: "fade_to_black" }
        case "start_output": return { action: "start_output", output: n }
        case "stop_output": return { action: "stop_output", output: n }
        case "start_streams": return { action: "start_outputs", group: "streams" }
        case "stop_streams": return { action: "stop_outputs", group: "streams" }
        case "mark_recording": return { action: "mark_recording" }
        case "audio_on": return { action: "set_audio", input: n, on: true }
        case "audio_off": return { action: "set_audio", input: n, on: false }
        case "osc": return { action: "osc_send", target: osc.target, address: osc.address.trim(), args: osc.args.trim() }
        default: return JSON.parse(json)
        }
    }

    /** Action → preset row { kind, index, json }; JSON when it is no preset. */
    function rowOf(action) {
        var number = v => typeof v === "number" ? v - 1 : 0
        // Without a length an overlay follows its channel (OVL.2); lmix-core
        // fills in the FTB default (DEFAULT_FTB_MS).
        var overlayDefault = a => a.duration_ms === undefined || a.duration_ms === null
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
        if (a.action === "overlay_preview" && a.channel === 1 && typeof a.input === "number" && plain === 3)
            return { kind: "overlay_preview", index: number(a.input) }
        if (a.action === "fade_to_black" && ftbDefault(a)) return { kind: "fade_to_black", index: 0 }
        if ((a.action === "start_output" || a.action === "stop_output") && typeof a.output === "number")
            return { kind: a.action, index: number(a.output) }
        if ((a.action === "start_outputs" || a.action === "stop_outputs") && a.group === "streams" && plain === 2)
            return { kind: a.action === "start_outputs" ? "start_streams" : "stop_streams", index: 0 }
        if (a.action === "mark_recording" && !a.text) return { kind: "mark_recording", index: 0 }
        if (a.action === "set_audio" && typeof a.input === "number")
            return { kind: a.on ? "audio_on" : "audio_off", index: number(a.input) }
        if (a.action === "osc_send" && plain <= 4)
            return { kind: "osc", index: 0, osc: { target: a.target, address: a.address, args: a.args || "" } }
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
            row.setOsc(r.osc || { target: "", address: "", args: "" })
        }
        validate()
    }

    /** The rows as actions, empty rows left out. Only call without `error`. */
    function collect() {
        var actions = []
        for (var i = 0; i < rows; i++) {
            var row = actionRows.itemAt(i)
            var action = actionOf(kinds[row.kindIndex].kind, row.paramIndex, row.json, row.osc())
            if (action)
                actions.push(action)
        }
        return actions
    }

    /** Counts the rows in use and says which JSON row does not parse. */
    function validate() {
        var count = 0
        var last = -1
        for (var r = 0; r < rows; r++) {
            var used = actionRows.itemAt(r)
            if (used && kinds[used.kindIndex].kind !== "") {
                count++
                last = r
            }
        }
        filled = count
        lastUsed = last
        for (var i = 0; i < rows; i++) {
            var row = actionRows.itemAt(i)
            if (row && kinds[row.kindIndex].kind === "osc") {
                var osc = row.osc()
                if (osc.target === "") {
                    error = qsTr("Action %1 needs an OSC target (Output targets)").arg(i + 1)
                    return
                }
                if (!oscAddressOk(osc.address.trim())) {
                    error = qsTr("Action %1: the OSC address starts with / and holds only letters, digits and signs of plain ASCII, no space, # or ,").arg(i + 1)
                    return
                }
                continue
            }
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
        objectName: "actionRowRepeater"
        model: editor.rows

        RowLayout {
            id: row
            required property int index
            property alias kindIndex: kindBox.currentIndex
            property alias paramIndex: paramBox.currentIndex
            property alias json: jsonField.text
            onJsonChanged: editor.validate()
            readonly property string param: editor.kinds[kindBox.currentIndex].param
            /** The OSC target chosen, by name: a target list that changes
             *  while the dialog is open keeps it. Empty: the first one. */
            property string oscChosen: ""
            onOscChosenChanged: if (row.param === "osc") editor.validate()
            /** The OSC targets, and first the chosen one if this machine lacks it. */
            readonly property var oscChoices: oscChosen !== "" && editor.oscTargets.indexOf(oscChosen) < 0 ? [oscChosen].concat(editor.oscTargets) : editor.oscTargets

            function osc() {
                return { target: oscChosen !== "" ? oscChosen : (editor.oscTargets[0] || ""), address: oscAddress.text, args: oscArgs.text }
            }
            function setOsc(o) {
                oscChosen = o.target
                oscAddress.text = o.address
                oscArgs.text = o.args
            }
            Layout.fillWidth: true
            visible: row.index < editor.shown

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
                visible: row.param !== "" && row.param !== "json" && row.param !== "osc"
                model: editor.choicesFor(row.param)
                KanteFieldSkin { control: paramBox }
            }
            QQC2.ComboBox {
                id: oscTarget
                objectName: "oscTargetBox"
                Layout.preferredWidth: Kirigami.Units.gridUnit * 7
                visible: row.param === "osc"
                model: row.oscChoices
                currentIndex: row.oscChosen === "" ? (row.oscChoices.length > 0 ? 0 : -1) : row.oscChoices.indexOf(row.oscChosen)
                onActivated: (index) => row.oscChosen = row.oscChoices[index]
                Accessible.name: qsTr("OSC target of action %1").arg(row.index + 1)
                KanteFieldSkin { control: oscTarget }
            }
            KanteTextField {
                id: oscAddress
                Layout.fillWidth: true
                visible: row.param === "osc"
                placeholderText: "/eos/go"
                font.family: KanteStyle.monoFamily
                Accessible.name: qsTr("OSC address of action %1").arg(row.index + 1)
                onTextChanged: if (row.param === "osc") editor.validate()
            }
            KanteTextField {
                id: oscArgs
                Layout.fillWidth: true
                visible: row.param === "osc"
                placeholderText: qsTr("Arguments: 1 0.5 \"text\" true")
                font.family: KanteStyle.monoFamily
                Accessible.name: qsTr("OSC arguments of action %1").arg(row.index + 1)
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                QQC2.ToolTip.text: qsTr("Separated by spaces: 1 is a whole number, 0.5 a decimal, \"two words\" text, true/false a truth value, nil empty; any other word is text.")
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
