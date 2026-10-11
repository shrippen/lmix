import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Titles (GFX.4, GFX.7, GFX.8): graphics channels with eight layers; per
 * layer a template, its fields as a form, play / next / stop / clear,
 * saved data sets and data source bindings. Fields go out as they are
 * edited ("Live"), or together with "Apply".
 */
KanteDialog {
    id: dialog

    required property var act
    /** The Console's `Core` (refreshes the template list). */
    required property var core
    /** `ShowState.graphics`, `datasets`, `data_sources`. */
    required property var graphics
    required property var datasets
    required property var sources
    /** lmix `Template` list. */
    required property var templates
    /** For actions of triggers. */
    required property var inputs
    required property var outputs
    required property string coreUrl

    property int current: 0
    readonly property var channel: graphics.length > 0 ? graphics[Math.min(current, graphics.length - 1)] : null
    property int layer: 1
    readonly property var loaded: channel ? channel.layers.find(l => l.layer === layer) || null : null
    /** Template the form shows: the loaded one, or the one picked to load. */
    property string pickedTemplate: templates.length > 0 ? templates[0].id : ""
    readonly property var template: templates.find(t => t.id === (loaded ? loaded.template : pickedTemplate)) || null
    property bool live: true
    /** Fields changed but not applied yet (not live). */
    property var pending: ({})
    /** Field the data source menu binds. */
    property string binding: ""
    /** Working copy of the layer's place (lmix `LayerTransform`). */
    property var place: loaded && loaded.transform ? Object.assign({}, loaded.transform) : ({})

    /** Language of the Console, for the templates' own texts. */
    readonly property string lang: Qt.locale().name.split("_")[0]
    function texts(t) { return (t && t.i18n && t.i18n[dialog.lang]) || {} }
    function templateName(t) { return dialog.texts(t).name || t.name }

    function ref() { return dialog.channel.number }
    function phaseText(phase) {
        switch (phase) {
        case "loaded": return qsTr("Ready")
        case "playing": return qsTr("On air")
        case "stopped": return qsTr("Stopped")
        }
        return ""
    }
    function phasePill(phase) {
        switch (phase) {
        case "playing": return KantePill.State.Running
        case "loaded": return KantePill.State.Review
        }
        return KantePill.State.Off
    }
    function fieldValue(field) {
        if (dialog.pending[field.id] !== undefined) return dialog.pending[field.id]
        if (dialog.loaded && dialog.loaded.values[field.id] !== undefined) return dialog.loaded.values[field.id]
        return field.default || ""
    }
    function boundTo(field) {
        if (!dialog.loaded || !dialog.loaded.bindings) return ""
        var b = dialog.loaded.bindings.find(x => x.field === field.id)
        if (!b) return ""
        var s = dialog.sources.find(x => x.id === b.source)
        return (s ? s.name : "?") + ": " + b.column
    }
    /** Field values of the form, for loading or saving a set. */
    function formData() {
        var data = {}
        if (!dialog.template) return data
        for (var i = 0; i < dialog.template.fields.length; i++) {
            var f = dialog.template.fields[i]
            var v = dialog.fieldValue(f)
            if (v !== "") data[f.id] = v
        }
        return data
    }
    function load(play) {
        dialog.act({ action: "title_load", graphics: ref(), layer: dialog.layer, template: dialog.pickedTemplate,
            data: dialog.formData(), play: play })
        dialog.pending = {}
    }
    function setField(id, value) {
        if (dialog.live && dialog.loaded) {
            var data = {}
            data[id] = value
            dialog.act({ action: "title_update", graphics: ref(), layer: dialog.layer, data: data })
        } else {
            var p = Object.assign({}, dialog.pending)
            p[id] = value
            dialog.pending = p
        }
    }

    onLayerChanged: pending = {}
    onCurrentChanged: pending = {}
    onOpened: core.refreshLists()

    title: qsTr("Titles")
    modal: true
    width: Kirigami.Units.gridUnit * 46
    standardButtons: QQC2.Dialog.Close
    contentHeight: column.implicitHeight

    TriggersDialog {
        id: triggersDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: dialog.act
        inputs: dialog.inputs
        outputs: dialog.outputs
    }

    ChoiceMenu {
        id: bindMenu
        choices: {
            var list = [{ value: "", label: qsTr("Not bound") }]
            for (var i = 0; i < dialog.sources.length; i++) {
                var s = dialog.sources[i]
                for (var j = 0; j < s.columns.length; j++)
                    list.push({ value: JSON.stringify({ source: s.id, column: s.columns[j] }), label: s.name + ": " + s.columns[j] })
            }
            return list
        }
        onPicked: value => dialog.act({ action: "title_bind", graphics: dialog.ref(), layer: dialog.layer,
            field: dialog.binding, binding: value === "" ? null : JSON.parse(value) })
    }

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            QQC2.ComboBox {
                id: channelBox
                Layout.fillWidth: true
                visible: dialog.graphics.length > 0
                model: dialog.graphics.map(g => g.number + " " + g.name)
                currentIndex: dialog.current
                onActivated: index => dialog.current = index
                KanteFieldSkin { control: channelBox }
            }
            KanteTextField {
                id: newName
                Layout.fillWidth: dialog.graphics.length === 0
                placeholderText: qsTr("New graphics channel")
            }
            KanteButton {
                text: qsTr("+ Channel")
                onClicked: {
                    dialog.act({ action: "add_graphics", name: newName.text.trim() || null })
                    dialog.current = dialog.graphics.length
                    newName.text = ""
                }
            }
            KanteButton {
                text: qsTr("Preview")
                visible: dialog.channel !== null
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Open the channel's picture in the web browser")
                onClicked: Qt.openUrlExternally(dialog.coreUrl + dialog.channel.page + "?preview")
            }
            KanteButton {
                text: dialog.channel && dialog.channel.triggers && dialog.channel.triggers.length > 0
                    ? qsTr("Triggers (%1)…").arg(dialog.channel.triggers.length) : qsTr("Triggers…")
                visible: dialog.channel !== null
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Run actions when a title reports, e.g. a countdown at zero")
                onClicked: {
                    triggersDialog.input = null
                    triggersDialog.graphics = dialog.channel
                    triggersDialog.open()
                }
            }
            KanteButton {
                text: qsTr("Remove")
                visible: dialog.channel !== null
                onClicked: dialog.act({ action: "remove_graphics", graphics: dialog.ref() })
            }
        }

        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.channel === null
            wrapMode: Text.WordWrap
            text: qsTr("No graphics channel yet. A channel stacks up to eight titles, such as a lower third, a clock and a ticker.")
        }

        // Layers 1–8, top layer first.
        Repeater {
            model: dialog.channel ? 8 : 0

            Rectangle {
                id: layerRow
                required property int index
                readonly property int number: 8 - index
                readonly property var title: dialog.channel.layers.find(l => l.layer === number) || null
                readonly property var info: title ? dialog.templates.find(t => t.id === title.template) || null : null
                Layout.fillWidth: true
                implicitHeight: layerLine.implicitHeight + Kirigami.Units.smallSpacing
                color: dialog.layer === number ? KanteStyle.selectionColor : "transparent"

                MouseArea {
                    anchors.fill: parent
                    onClicked: dialog.layer = layerRow.number
                }

                RowLayout {
                    id: layerLine
                    anchors.fill: parent
                    anchors.leftMargin: Kirigami.Units.smallSpacing

                    QQC2.Label {
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 2
                        text: layerRow.number
                        font.family: KanteStyle.monoFamily
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: layerRow.title ? (layerRow.info ? dialog.templateName(layerRow.info) : layerRow.title.template) : "—"
                        color: layerRow.title ? Kirigami.Theme.textColor : KanteStyle.mutedTextColor
                        elide: Text.ElideRight
                    }
                    KantePill {
                        visible: layerRow.title !== null
                        status: layerRow.title ? dialog.phasePill(layerRow.title.phase) : KantePill.State.Off
                        text: layerRow.title ? dialog.phaseText(layerRow.title.phase) : ""
                    }
                    KanteButton {
                        text: qsTr("Play")
                        emphasis: KanteButton.Emphasis.Primary
                        enabled: layerRow.title !== null
                        onClicked: dialog.act({ action: "title_play", graphics: dialog.ref(), layer: layerRow.number })
                    }
                    KanteButton {
                        text: qsTr("Next")
                        visible: layerRow.info !== null && layerRow.info.steps > 0
                        enabled: layerRow.title !== null && layerRow.title.phase === "playing"
                            && layerRow.title.step < layerRow.info.steps
                        onClicked: dialog.act({ action: "title_next", graphics: dialog.ref(), layer: layerRow.number })
                    }
                    KanteButton {
                        text: qsTr("Stop")
                        enabled: layerRow.title !== null && layerRow.title.phase === "playing"
                        onClicked: dialog.act({ action: "title_stop", graphics: dialog.ref(), layer: layerRow.number })
                    }
                    KanteButton {
                        text: qsTr("Clear")
                        emphasis: KanteButton.Emphasis.Quiet
                        enabled: layerRow.title !== null
                        onClicked: dialog.act({ action: "title_clear", graphics: dialog.ref(), layer: layerRow.number })
                    }
                }
            }
        }

        KanteSectionLabel {
            visible: dialog.channel !== null
            text: qsTr("Layer %1").arg(dialog.layer)
        }

        RowLayout {
            Layout.fillWidth: true
            visible: dialog.channel !== null
            QQC2.ComboBox {
                id: templateBox
                Layout.fillWidth: true
                model: dialog.templates.map(t => dialog.templateName(t) + (t.builtin ? "" : "  (" + t.id + ")"))
                currentIndex: Math.max(0, dialog.templates.findIndex(t => t.id === (dialog.loaded ? dialog.loaded.template : dialog.pickedTemplate)))
                onActivated: index => {
                    dialog.pickedTemplate = dialog.templates[index].id
                    dialog.pending = {}
                }
                KanteFieldSkin { control: templateBox }
            }
            KanteButton {
                text: qsTr("Load")
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Load the template with the form's fields, ready to play")
                enabled: dialog.pickedTemplate !== ""
                onClicked: dialog.load(false)
            }
            KanteButton {
                text: qsTr("Load and play")
                enabled: dialog.pickedTemplate !== ""
                onClicked: dialog.load(true)
            }
        }

        Repeater {
            model: dialog.channel && dialog.template ? dialog.template.fields : []

            TitleField {
                required property var modelData
                Layout.fillWidth: true
                field: modelData
                texts: dialog.texts(dialog.template)
                value: dialog.fieldValue(modelData)
                boundTo: dialog.boundTo(modelData)
                onEdited: value => { if (dialog.live) dialog.setField(modelData.id, value) }
                onTyped: value => { if (!dialog.live) dialog.setField(modelData.id, value) }
                onStepped: amount => {
                    if (dialog.loaded)
                        dialog.act({ action: "title_add", graphics: dialog.ref(), layer: dialog.layer,
                            field: modelData.id, amount: amount })
                }
                onBindRequested: button => {
                    dialog.binding = modelData.id
                    bindMenu.popup(button, 0, button.height)
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: dialog.channel !== null && dialog.loaded !== null

            QQC2.Switch {
                id: liveSwitch
                text: qsTr("Live")
                checked: dialog.live
                onToggled: dialog.live = checked
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Send each field when it is edited; off: send them together with Apply")
                KanteCheckSkin { control: liveSwitch; shape: KanteCheckSkin.Shape.Switch }
            }
            KanteButton {
                text: qsTr("Apply")
                visible: !dialog.live
                enabled: Object.keys(dialog.pending).length > 0
                onClicked: {
                    dialog.act({ action: "title_update", graphics: dialog.ref(), layer: dialog.layer, data: dialog.pending })
                    dialog.pending = {}
                }
            }
            Item { Layout.fillWidth: true }
            Repeater {
                model: dialog.loaded && dialog.template ? dialog.template.functions : []
                KanteButton {
                    required property var modelData
                    text: modelData
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: qsTr("Call the template's function %1").arg(modelData)
                    onClicked: dialog.act({ action: "title_call", graphics: dialog.ref(), layer: dialog.layer, function: modelData })
                }
            }
        }

        // Where the layer sits (GFX.18): edited as a copy, Move sends all of it.
        KanteSectionLabel {
            visible: dialog.loaded !== null
            text: qsTr("Place")
        }
        Flow {
            Layout.fillWidth: true
            visible: dialog.loaded !== null
            spacing: Kirigami.Units.smallSpacing

            Repeater {
                model: [
                    { key: "x", label: qsTr("X %"), from: -200, to: 200, scale: 100 },
                    { key: "y", label: qsTr("Y %"), from: -200, to: 200, scale: 100 },
                    { key: "scale", label: qsTr("Size %"), from: 5, to: 1000, scale: 100 },
                    { key: "rotation", label: qsTr("Turn °"), from: -360, to: 360, scale: 1 },
                    { key: "opacity", label: qsTr("Opacity %"), from: 0, to: 100, scale: 100 },
                    { key: "saturation", label: qsTr("Colour %"), from: 0, to: 400, scale: 100 }
                ]
                RowLayout {
                    required property var modelData
                    spacing: 2
                    QQC2.Label { text: modelData.label; color: KanteStyle.mutedTextColor }
                    QQC2.SpinBox {
                        id: placeBox
                        from: modelData.from
                        to: modelData.to
                        editable: true
                        value: Math.round((dialog.place[modelData.key] ?? (modelData.key === "x" || modelData.key === "y" || modelData.key === "rotation" ? 0 : 1)) * modelData.scale)
                        Accessible.name: modelData.label
                        onValueModified: {
                            const p = Object.assign({}, dialog.place)
                            p[modelData.key] = value / modelData.scale
                            dialog.place = p
                        }
                        KanteFieldSkin { control: placeBox }
                    }
                }
            }
            RowLayout {
            spacing: 2
            QQC2.SpinBox {
                id: moveMs
                from: 0
                to: 10000
                stepSize: 100
                editable: true
                value: 500
                Accessible.name: qsTr("Move time, ms")
                KanteFieldSkin { control: moveMs }
            }
            QQC2.Label { text: "ms" }
            }
            KanteButton {
                text: qsTr("Move")
                onClicked: dialog.act({ action: "title_transform", graphics: dialog.ref(), layer: dialog.layer,
                    transform: dialog.place, duration_ms: moveMs.value })
            }
            KanteButton {
                text: qsTr("Reset")
                emphasis: KanteButton.Emphasis.Quiet
                onClicked: {
                    dialog.place = {}
                    dialog.act({ action: "title_transform", graphics: dialog.ref(), layer: dialog.layer,
                        transform: {}, duration_ms: moveMs.value })
                }
            }
        }

        // Saved data sets (GFX.8).
        RowLayout {
            Layout.fillWidth: true
            visible: dialog.channel !== null && dialog.template !== null

            QQC2.ComboBox {
                id: setBox
                Layout.fillWidth: true
                enabled: dialog.datasets.length > 0
                model: dialog.datasets.map(d => d.name)
                KanteFieldSkin { control: setBox }
            }
            KanteButton {
                text: qsTr("Use set")
                enabled: dialog.loaded !== null && dialog.datasets.length > 0
                onClicked: dialog.act({ action: "apply_dataset", graphics: dialog.ref(), layer: dialog.layer,
                    name: dialog.datasets[setBox.currentIndex].name })
            }
            KanteButton {
                text: qsTr("Remove set")
                emphasis: KanteButton.Emphasis.Quiet
                enabled: dialog.datasets.length > 0
                onClicked: dialog.act({ action: "remove_dataset", name: dialog.datasets[setBox.currentIndex].name })
            }
            KanteTextField {
                id: setName
                placeholderText: qsTr("Set name")
            }
            KanteButton {
                text: qsTr("Save set")
                enabled: setName.text.trim() !== ""
                onClicked: {
                    dialog.act({ action: "save_dataset", name: setName.text.trim(),
                        template: dialog.template ? dialog.template.id : null, data: dialog.formData() })
                    setName.text = ""
                }
            }
        }
    }
}
