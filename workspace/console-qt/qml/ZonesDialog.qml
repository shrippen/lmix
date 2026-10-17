import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Zones (show mode SND.10): the show names zones and their channels; where
 * each plays is this machine's (device, device channels, level), so the
 * show runs elsewhere after a new patch (SSH.3).
 */
KanteDialog {
    id: dialog

    required property var act
    required property var zones

    /** Zone being edited by name; "" for a new one. */
    property string editing: ""
    /** Outputs of the zone being edited, as rows. */
    property var rows: []

    function edit(zone) {
        editing = zone ? zone.name : ""
        zoneName.text = zone ? zone.name : ""
        channels.value = zone ? zone.channels : 2
        rows = zone ? zone.outputs.map(o => ({ device: o.device, channels: o.channels.join(", "), gain: o.gain_db }))
                    : [{ device: "", channels: "1, 2", gain: 0 }]
        form.visible = true
    }

    function save() {
        var name = zoneName.text.trim()
        if (editing !== "" && editing !== name)
            act({ action: "remove_zone", name: editing })
        act({ action: "set_zone", zone: { name: name, channels: channels.value } })
        var outputs = rows.filter(r => r.channels.trim() !== "").map(r => ({
            device: r.device.trim(),
            channels: r.channels.split(/[\s,]+/).filter(c => c !== "").map(c => parseInt(c, 10)),
            gain_db: r.gain
        }))
        if (outputs.length > 0)
            act({ action: "set_zone_patch", patch: { zone: name, outputs: outputs } })
        else
            act({ action: "remove_zone_patch", zone: name })
        form.visible = false
    }

    function setRow(index, field, value) {
        var next = rows.slice()
        next[index] = Object.assign({}, next[index])
        next[index][field] = value
        rows = next
    }

    function where(zone) {
        if (zone.outputs.length === 0)
            return qsTr("plays nowhere on this machine")
        return zone.outputs.map(o => (o.device !== "" ? o.device : qsTr("default output")) + " " + o.channels.join("/")).join(" · ")
    }

    title: qsTr("Zones")
    modal: true
    width: Kirigami.Units.gridUnit * 44
    standardButtons: QQC2.Dialog.Close
    contentHeight: column.implicitHeight

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.zones.length === 0
            wrapMode: Text.WordWrap
            text: qsTr("No zones yet. A zone is a named sound output such as \"front\" or \"back\"; sound cues play into zones. Where a zone plays is set per machine.")
        }
        Repeater {
            model: dialog.zones

            RowLayout {
                id: zone
                required property var modelData
                Layout.fillWidth: true

                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 7
                    text: zone.modelData.name
                    font.bold: true
                    elide: Text.ElideRight
                }
                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                    text: qsTr("%1 ch.").arg(zone.modelData.channels)
                    color: KanteStyle.mutedTextColor
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    text: dialog.where(zone.modelData)
                    color: zone.modelData.outputs.length === 0 ? KanteStyle.warningColor : KanteStyle.textColor
                    elide: Text.ElideRight
                }
                KanteButton {
                    text: qsTr("Edit…")
                    onClicked: dialog.edit(zone.modelData)
                }
                KanteButton {
                    text: qsTr("Remove")
                    onClicked: dialog.act({ action: "remove_zone", name: zone.modelData.name })
                }
            }
        }
        KanteButton {
            visible: !form.visible
            text: qsTr("+ Zone")
            onClicked: dialog.edit(null)
        }

        ColumnLayout {
            id: form
            visible: false
            Layout.fillWidth: true

            RowLayout {
                KanteSectionLabel { text: qsTr("Name") }
                KanteTextField {
                    id: zoneName
                    Layout.fillWidth: true
                    placeholderText: qsTr("e.g. front")
                }
                KanteSectionLabel { text: qsTr("Channels") }
                QQC2.SpinBox {
                    id: channels
                    from: 1
                    to: 8
                    value: 2
                    KanteFieldSkin { control: channels }
                }
            }
            KanteSectionLabel { text: qsTr("On this machine") }
            Repeater {
                model: dialog.rows

                RowLayout {
                    id: output
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true

                    KanteTextField {
                        Layout.fillWidth: true
                        text: output.modelData.device
                        placeholderText: qsTr("device, empty: default output")
                        onEditingFinished: dialog.setRow(output.index, "device", text)
                    }
                    KanteTextField {
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                        text: output.modelData.channels
                        placeholderText: qsTr("channels: 1, 2")
                        onEditingFinished: dialog.setRow(output.index, "channels", text)
                    }
                    QQC2.SpinBox {
                        id: gain
                        from: -60
                        to: 12
                        value: Math.round(output.modelData.gain)
                        textFromValue: v => qsTr("%1 dB").arg(v)
                        onValueModified: dialog.setRow(output.index, "gain", value)
                        KanteFieldSkin { control: gain }
                    }
                    KanteButton {
                        text: "×"
                        Accessible.name: qsTr("Remove output")
                        onClicked: {
                            var next = dialog.rows.slice()
                            next.splice(output.index, 1)
                            dialog.rows = next
                        }
                    }
                }
            }
            RowLayout {
                KanteButton {
                    text: qsTr("+ Output")
                    enabled: dialog.rows.length < 4
                    onClicked: dialog.rows = dialog.rows.concat([{ device: "", channels: "", gain: 0 }])
                }
                Item { Layout.fillWidth: true }
                KanteButton {
                    text: qsTr("Save")
                    emphasis: KanteButton.Emphasis.Primary
                    enabled: zoneName.text.trim() !== ""
                    onClicked: dialog.save()
                }
                KanteButton {
                    text: qsTr("Cancel")
                    onClicked: form.visible = false
                }
            }
        }
    }
}
