import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Output targets and OSC subscribers of this machine (show mode EXT.1–5,
 * CTL.4): not in the show, so a show runs on another machine after its
 * own patch. A secret is never shown; an empty field keeps it.
 */
KanteDialog {
    id: dialog

    required property var act
    required property var targets
    required property var feeds

    /** Target being edited, by name; "" for a new one. */
    property string editing: ""
    /** Subscriber being edited, by name; "" for a new one. */
    property string editingFeed: ""
    readonly property var kinds: ["http", "osc_udp", "osc_tcp"]
    readonly property var occasions: ["standby", "running", "panic"]

    function kindLabel(kind) {
        switch (kind) {
        case "osc_udp": return qsTr("OSC (UDP)")
        case "osc_tcp": return qsTr("OSC (TCP)")
        default: return qsTr("HTTP")
        }
    }

    function occasionLabel(on) {
        switch (on) {
        case "running": return qsTr("Running cue")
        case "panic": return qsTr("Panic")
        default: return qsTr("Standby")
        }
    }

    function editTarget(target) {
        editing = target ? target.name : ""
        targetName.text = target ? target.name : ""
        targetKind.currentIndex = target ? kinds.indexOf(target.kind) : 0
        addresses.text = target ? target.addresses.join(" ") : ""
        secret.text = ""
        timeout.value = target ? target.timeout_ms : 3000
        targetForm.visible = true
    }

    function saveTarget() {
        var spec = {
            name: targetName.text.trim(),
            kind: kinds[targetKind.currentIndex],
            addresses: addresses.text.split(/[\s,]+/).filter(a => a !== ""),
            timeout_ms: timeout.value
        }
        // Empty keeps the secret set; a new one replaces it.
        if (secret.text !== "")
            spec.secret = secret.text
        act({ action: "set_target", target: spec })
        targetForm.visible = false
    }

    /** Subscriber messages as editable rows. */
    property var rows: []

    function editFeed(feed) {
        editingFeed = feed ? feed.name : ""
        feedName.text = feed ? feed.name : ""
        var osc = targets.filter(t => t.kind === "osc_udp").map(t => t.name)
        feedTarget.model = osc
        feedTarget.currentIndex = feed ? Math.max(0, osc.indexOf(feed.target)) : 0
        rows = feed ? feed.messages.map(m => ({ on: m.on, address: m.address, args: m.args.join(", ") }))
                    : [{ on: "standby", address: "/lmix/standby", args: "{number} {name}" }]
        feedForm.visible = true
    }

    function saveFeed() {
        var feed = {
            name: feedName.text.trim(),
            target: feedTarget.currentText,
            messages: rows.filter(r => r.address.trim() !== "").map(r => ({
                on: r.on,
                address: r.address.trim(),
                args: r.args.split(",").map(a => a.trim()).filter(a => a !== "")
            }))
        }
        if (editingFeed !== "" && editingFeed !== feed.name)
            act({ action: "remove_feed", name: editingFeed })
        act({ action: "set_feed", feed: feed })
        feedForm.visible = false
    }

    function setRow(index, field, value) {
        var next = rows.slice()
        next[index] = Object.assign({}, next[index])
        next[index][field] = value
        rows = next
    }

    title: qsTr("Targets and subscribers")
    modal: true
    width: Kirigami.Units.gridUnit * 48
    standardButtons: QQC2.Dialog.Close
    contentHeight: column.implicitHeight

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        KanteSectionLabel { text: qsTr("Output targets of this machine") }
        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.targets.length === 0
            wrapMode: Text.WordWrap
            text: qsTr("No targets yet. Cues send to a target by its name, e.g. Home Assistant for light, or Companion for buttons.")
        }
        Repeater {
            model: dialog.targets

            RowLayout {
                id: target
                required property var modelData
                Layout.fillWidth: true

                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 7
                    text: target.modelData.name
                    font.bold: true
                    elide: Text.ElideRight
                }
                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                    text: dialog.kindLabel(target.modelData.kind)
                    color: KanteStyle.mutedTextColor
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    Layout.minimumWidth: Kirigami.Units.gridUnit * 10
                    text: target.modelData.addresses.join(" ")
                    font.family: KanteStyle.monoFamily
                    elide: Text.ElideMiddle
                }
                QQC2.Label {
                    visible: target.modelData.has_secret
                    text: qsTr("with secret")
                    color: KanteStyle.mutedTextColor
                }
                KanteButton {
                    text: qsTr("Test")
                    visible: target.modelData.kind === "http"
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: qsTr("Send a request to the first address; a failure shows below the window")
                    onClicked: dialog.act({ action: "test_target", name: target.modelData.name })
                }
                KanteButton {
                    text: qsTr("Edit…")
                    onClicked: dialog.editTarget(target.modelData)
                }
                KanteButton {
                    text: qsTr("Remove")
                    onClicked: dialog.act({ action: "remove_target", name: target.modelData.name })
                }
            }
        }
        KanteButton {
            visible: !targetForm.visible
            text: qsTr("+ Target")
            onClicked: dialog.editTarget(null)
        }

        GridLayout {
            id: targetForm
            visible: false
            Layout.fillWidth: true
            columns: 2

            KanteSectionLabel { text: qsTr("Name") }
            KanteTextField {
                id: targetName
                Layout.fillWidth: true
                // A new name would lose the secret: remove and add instead.
                readOnly: dialog.editing !== ""
                placeholderText: qsTr("e.g. Haus")
            }
            KanteSectionLabel { text: qsTr("Kind") }
            QQC2.ComboBox {
                id: targetKind
                model: dialog.kinds.map(k => dialog.kindLabel(k))
                KanteFieldSkin { control: targetKind }
            }
            KanteSectionLabel { text: qsTr("Addresses") }
            KanteTextField {
                id: addresses
                Layout.fillWidth: true
                placeholderText: dialog.kinds[targetKind.currentIndex] === "http" ? qsTr("http://ha.local:8123, a backup after a space") : qsTr("host:port")
            }
            KanteSectionLabel { text: qsTr("Secret") }
            KanteTextField {
                id: secret
                Layout.fillWidth: true
                echoMode: TextInput.Password
                placeholderText: dialog.editing !== "" ? qsTr("unchanged") : qsTr("token, e.g. Home Assistant's long-lived token")
            }
            KanteSectionLabel { text: qsTr("Time limit ms") }
            QQC2.SpinBox {
                id: timeout
                from: 100
                to: 30000
                stepSize: 500
                value: 3000
                editable: true
                KanteFieldSkin { control: timeout }
            }
            Item { implicitWidth: 1 }
            RowLayout {
                KanteButton {
                    text: qsTr("Save")
                    emphasis: KanteButton.Emphasis.Primary
                    enabled: targetName.text.trim() !== "" && addresses.text.trim() !== ""
                    onClicked: dialog.saveTarget()
                }
                KanteButton {
                    text: qsTr("Cancel")
                    onClicked: targetForm.visible = false
                }
            }
        }

        KanteSectionLabel {
            Layout.topMargin: Kirigami.Units.largeSpacing
            text: qsTr("OSC subscribers")
        }
        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.feeds.length === 0
            wrapMode: Text.WordWrap
            text: qsTr("No subscribers yet. A subscriber sends standby, the running cue's remaining time and panic to an OSC (UDP) target, e.g. Companion's custom variables. Placeholders: {list} {number} {name} {note} {remaining} {elapsed} {total} {mode}.")
        }
        Repeater {
            model: dialog.feeds

            RowLayout {
                id: feed
                required property var modelData
                Layout.fillWidth: true

                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 7
                    text: feed.modelData.name
                    font.bold: true
                    elide: Text.ElideRight
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    text: qsTr("to %1, %2 messages").arg(feed.modelData.target).arg(feed.modelData.messages.length)
                    color: KanteStyle.mutedTextColor
                }
                KanteButton {
                    text: qsTr("Edit…")
                    onClicked: dialog.editFeed(feed.modelData)
                }
                KanteButton {
                    text: qsTr("Remove")
                    onClicked: dialog.act({ action: "remove_feed", name: feed.modelData.name })
                }
            }
        }
        KanteButton {
            visible: !feedForm.visible
            enabled: dialog.targets.some(t => t.kind === "osc_udp")
            text: qsTr("+ Subscriber")
            QQC2.ToolTip.visible: hovered && !enabled
            QQC2.ToolTip.text: qsTr("Add an OSC (UDP) target first")
            onClicked: dialog.editFeed(null)
        }

        ColumnLayout {
            id: feedForm
            visible: false
            Layout.fillWidth: true

            RowLayout {
                KanteSectionLabel { text: qsTr("Name") }
                KanteTextField {
                    id: feedName
                    Layout.fillWidth: true
                    placeholderText: qsTr("e.g. Stream Deck")
                }
                KanteSectionLabel { text: qsTr("Target") }
                QQC2.ComboBox {
                    id: feedTarget
                    KanteFieldSkin { control: feedTarget }
                }
            }
            Repeater {
                model: dialog.rows

                RowLayout {
                    id: message
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true

                    QQC2.ComboBox {
                        id: occasion
                        model: dialog.occasions.map(o => dialog.occasionLabel(o))
                        currentIndex: dialog.occasions.indexOf(message.modelData.on)
                        onActivated: index => dialog.setRow(message.index, "on", dialog.occasions[index])
                        KanteFieldSkin { control: occasion }
                    }
                    KanteTextField {
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                        text: message.modelData.address
                        placeholderText: qsTr("/address")
                        onEditingFinished: dialog.setRow(message.index, "address", text)
                    }
                    KanteTextField {
                        Layout.fillWidth: true
                        text: message.modelData.args
                        placeholderText: qsTr("values, comma separated: {number} {name}")
                        onEditingFinished: dialog.setRow(message.index, "args", text)
                    }
                    KanteButton {
                        text: "×"
                        Accessible.name: qsTr("Remove message")
                        onClicked: {
                            var next = dialog.rows.slice()
                            next.splice(message.index, 1)
                            dialog.rows = next
                        }
                    }
                }
            }
            RowLayout {
                KanteButton {
                    text: qsTr("+ Message")
                    onClicked: dialog.rows = dialog.rows.concat([{ on: "running", address: "", args: "{remaining}" }])
                }
                Item { Layout.fillWidth: true }
                KanteButton {
                    text: qsTr("Save")
                    emphasis: KanteButton.Emphasis.Primary
                    enabled: feedName.text.trim() !== "" && feedTarget.currentText !== "" && dialog.rows.length > 0
                    onClicked: dialog.saveFeed()
                }
                KanteButton {
                    text: qsTr("Cancel")
                    onClicked: feedForm.visible = false
                }
            }
        }
    }
}
