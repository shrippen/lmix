import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Cut, the four transition buttons (click: take preview; right click:
 * effect and length), quick play of preview (SWI.8, the same menu without
 * the stinger), the T-bar and the overlay channels in a 4 × 2 grid
 * (click: the preview input on, or the overlay off; right click: the last
 * input again, the input prepared for the next take away, settings), with
 * all overlays off. Fade to black is the page's own button (MixPage).
 * Sized to fit beside the monitors at 1440 × 900 without scrolling.
 */
ColumnLayout {
    id: panel

    required property var core
    required property var show
    required property var act
    required property var transitions
    required property var transitionBySlot

    readonly property bool canTake: show !== null && show.preview !== null
    readonly property var overlays: show && show.overlays ? show.overlays : []
    readonly property var overlayLast: show && show.overlay_last ? show.overlay_last : []
    /** Inputs going on with the next take (OVL.3), and each channel's settings (OVL.2). */
    readonly property var overlayPreview: show && show.overlay_preview ? show.overlay_preview : []
    readonly property var overlayChannels: show && show.overlay_channels ? show.overlay_channels : []
    readonly property bool anyOverlay: overlays.some(id => id !== null)

    function inputById(id) {
        return show ? show.inputs.find(i => i.id === id) || null : null
    }
    readonly property var effects: Texts.choices("effects", core.choices("effects"))
    readonly property var durations: JSON.parse(core.choices("durations"))

    function effectLabel(name) {
        return Texts.label("effects", name)
    }

    /** `set_transition` for `button` with a new length; a stinger keeps
     *  its clip, its cut point at most the new length. */
    function withLength(button, ms) {
        var action = { action: "set_transition", slot: button.slot, effect: button.effect, duration_ms: ms }
        if (button.effect === "stinger" && button.stinger)
            action.stinger = { input: button.stinger.input, cut_ms: Math.min(button.stinger.cut_ms, ms) }
        return action
    }

    function editStinger(button) {
        stingerDialog.button = button
        stingerDialog.open()
    }

    OverlayDialog {
        id: overlayDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: panel.act
    }

    StingerDialog {
        id: stingerDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: panel.act
        inputs: panel.show ? panel.show.inputs : []
    }

    spacing: Kirigami.Units.smallSpacing

    KanteButton {
        Layout.fillWidth: true
        text: qsTr("Cut")
        emphasis: KanteButton.Emphasis.Primary
        size: KanteButton.Size.Large
        enabled: panel.core.connected && panel.canTake
        onClicked: panel.act({ action: "cut" })
    }

    Repeater {
        model: panel.transitions

        // The right click sits on this item: a disabled button (nothing in
        // preview) passes it through, so effect and length stay settable.
        Item {
            id: button
            required property string key
            readonly property var modelData: panel.transitionBySlot(key) || ({ slot: Number(key), effect: "cut", duration_ms: 0 })

            Layout.fillWidth: true
            implicitHeight: take.implicitHeight

            KanteButton {
                id: take
                anchors.fill: parent
                text: button.modelData.slot + " " + panel.effectLabel(button.modelData.effect) + "\n" + button.modelData.duration_ms + " ms"
                enabled: panel.core.connected && panel.canTake
                onClicked: panel.act({ action: "transition", slot: button.modelData.slot })
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Click: take preview. Right click: effect and length.")
            }

            TapHandler {
                acceptedButtons: Qt.RightButton
                enabled: panel.core.connected
                onTapped: menu.popup()
            }

            QQC2.Menu {
                id: menu
                KantePopupSkin { popup: menu }

                ChoiceMenu {
                    title: qsTr("Effect")
                    choices: panel.effects
                    current: button.modelData.effect
                    onPicked: value => {
                        // A stinger needs its clip and cut point first.
                        if (value === "stinger")
                            panel.editStinger(button.modelData)
                        else
                            panel.act({ action: "set_transition", slot: button.modelData.slot, effect: value, duration_ms: button.modelData.duration_ms })
                    }
                }
                ChoiceMenu {
                    title: qsTr("Length")
                    choices: panel.durations.map(ms => ({ value: ms, label: Texts.label("durations", ms) }))
                    current: button.modelData.duration_ms
                    onPicked: value => panel.act(panel.withLength(button.modelData, value))
                }
                QQC2.MenuItem {
                    text: qsTr("Stinger clip and cut point…")
                    visible: button.modelData.effect === "stinger"
                    height: visible ? implicitHeight : 0
                    onTriggered: panel.editStinger(button.modelData)
                }
            }
        }
    }

    // Quick play (SWI.8): preview to program with its own effect, played
    // if it is a clip. The right click sits on the item, as above.
    Item {
        id: quick
        readonly property var setting: panel.show && panel.show.quick_play ? panel.show.quick_play : ({ effect: "fade", duration_ms: 500 })

        Layout.fillWidth: true
        implicitHeight: quickButton.implicitHeight

        KanteButton {
            id: quickButton
            anchors.fill: parent
            text: qsTr("Quick play") + "\n" + panel.effectLabel(quick.setting.effect) + " " + quick.setting.duration_ms + " ms"
            enabled: panel.core.connected && panel.canTake
            onClicked: panel.act({ action: "quick_play" })
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: qsTr("Click: preview to program with this effect, and play it. Double click on the preview monitor does the same. Right click: effect and length.")
        }

        TapHandler {
            acceptedButtons: Qt.RightButton
            enabled: panel.core.connected
            onTapped: quickMenu.popup()
        }

        QQC2.Menu {
            id: quickMenu
            KantePopupSkin { popup: quickMenu }

            ChoiceMenu {
                title: qsTr("Effect")
                choices: panel.effects.filter(e => e.value !== "stinger")
                current: quick.setting.effect
                onPicked: value => panel.act({ action: "set_quick_play", effect: value, duration_ms: quick.setting.duration_ms })
            }
            ChoiceMenu {
                title: qsTr("Length")
                choices: panel.durations.map(ms => ({ value: ms, label: Texts.label("durations", ms) }))
                current: quick.setting.duration_ms
                onPicked: value => panel.act({ action: "set_quick_play", effect: quick.setting.effect, duration_ms: value })
            }
        }
    }

    // T-bar: preview fades in over program while pulled; at the end it is
    // taken. Moves are sent at most about 30 times a second.
    // Label beside the bar, not above it: the column fits at 1440 × 900.
    RowLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing
        KanteSectionLabel {
            text: qsTr("T-bar")
            color: KanteStyle.mutedTextColor
        }
        QQC2.Slider {
            id: tBar
            Layout.fillWidth: true
            from: 0
            to: 1
            enabled: panel.core.connected && panel.canTake
            value: pressed ? value : (panel.show ? panel.show.t_bar : 0)
            Accessible.name: qsTr("T-bar")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: qsTr("Pull to mix preview in; at the end it goes to program.")
            KanteSliderSkin { control: tBar }
            onMoved: tBarSend.restart()
            onPressedChanged: if (!pressed) {
                tBarSend.stop()
                panel.act({ action: "t_bar", position: value >= 0.98 ? 1 : value })
            }
            Timer {
                id: tBarSend
                interval: 33
                onTriggered: panel.act({ action: "t_bar", position: Math.min(tBar.value, 0.99) })
            }
        }
    }

    // Heading and "All off" in one row: the column fits at 1440 × 900.
    RowLayout {
        Layout.fillWidth: true
        visible: overlayRow.visible
        spacing: Kirigami.Units.smallSpacing
        KanteSectionLabel {
            Layout.fillWidth: true
            text: qsTr("Overlays")
            color: KanteStyle.mutedTextColor
        }
        KanteButton {
            emphasis: KanteButton.Emphasis.Quiet
            size: KanteButton.Size.Small
            text: qsTr("All off")
            enabled: panel.core.connected && panel.anyOverlay
            Accessible.name: qsTr("All overlays off")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: qsTr("Takes every overlay channel off, each with its fade.")
            onClicked: panel.act({ action: "overlay_out_all" })
        }
    }
    GridLayout {
        id: overlayRow
        Layout.fillWidth: true
        visible: panel.overlays.length > 0
        columns: 4
        rowSpacing: Kirigami.Units.smallSpacing
        columnSpacing: Kirigami.Units.smallSpacing

        Repeater {
            model: panel.overlays.length

            // The right click sits on this item, as on the transition
            // buttons: an empty channel with nothing in preview is disabled.
            Item {
                id: channelItem
                required property int index
                readonly property int channel: index + 1
                readonly property var shown: panel.overlays[index] ? panel.inputById(panel.overlays[index]) : null
                readonly property var last: panel.overlayLast[index] ? panel.inputById(panel.overlayLast[index]) : null
                readonly property var prepared: panel.overlayPreview[index] ? panel.inputById(panel.overlayPreview[index]) : null
                readonly property var settings: panel.overlayChannels[index] || null
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                implicitHeight: channelButton.implicitHeight
                implicitWidth: channelButton.implicitWidth

                KanteButton {
                    id: channelButton
                    readonly property int channel: channelItem.channel
                    readonly property var shown: channelItem.shown
                    readonly property var last: channelItem.last
                    anchors.fill: parent
                    size: KanteButton.Size.Small
                    // The channel only; the fill says on (destructive) or
                    // going on with the next take (primary), the tooltip what.
                    text: String(channel)
                    emphasis: shown ? KanteButton.Emphasis.Destructive
                        : channelItem.prepared ? KanteButton.Emphasis.Primary : KanteButton.Emphasis.Normal
                    enabled: panel.core.connected && (shown !== null || panel.canTake)
                    Accessible.name: (shown ? qsTr("Overlay %1: %2, click to take off").arg(channel).arg(shown.name)
                        : qsTr("Overlay %1: empty, click to show preview").arg(channel))
                        + (channelItem.prepared ? ", " + qsTr("with the next take: %1").arg(channelItem.prepared.name) : "")
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: (shown ? qsTr("Overlay %1 shows %2 %3. Click: take off.").arg(channel).arg(shown.number).arg(shown.name)
                        : last ? qsTr("Overlay %1 is empty. Click: show the preview input. Right click: %2 again.").arg(channel).arg(last.name)
                        : qsTr("Overlay %1 is empty. Click: show the preview input.").arg(channel))
                        + (channelItem.prepared ? " " + qsTr("With the next take: %1 %2.").arg(channelItem.prepared.number).arg(channelItem.prepared.name) : "")
                        + (channelItem.settings && channelItem.settings.rect ? " " + qsTr("In a box (picture in picture).") : "")
                        + " " + qsTr("Right click: settings.")
                    onClicked: panel.act(shown ? { action: "overlay_out", channel: channel }
                        : { action: "overlay_in", channel: channel, input: panel.show.preview })
                }

                TapHandler {
                    acceptedButtons: Qt.RightButton
                    enabled: panel.core.connected
                    onTapped: channelMenu.popup()
                }

                QQC2.Menu {
                    id: channelMenu
                    KantePopupSkin { popup: channelMenu }
                    // Texts never empty: a hidden item with none loops on its width.
                    QQC2.MenuItem {
                        visible: channelItem.shown === null && channelItem.last !== null
                        height: visible ? implicitHeight : 0
                        text: qsTr("Show %1 %2 again").arg(channelItem.last ? channelItem.last.number : "")
                            .arg(channelItem.last ? channelItem.last.name : "")
                        onTriggered: panel.act({ action: "overlay_last", channel: channelItem.channel })
                    }
                    QQC2.MenuItem {
                        visible: channelItem.prepared !== null
                        height: visible ? implicitHeight : 0
                        text: qsTr("Not with the next take: %1 %2").arg(channelItem.prepared ? channelItem.prepared.number : "")
                            .arg(channelItem.prepared ? channelItem.prepared.name : "")
                        onTriggered: panel.act({ action: "overlay_preview", channel: channelItem.channel, input: null })
                    }
                    QQC2.MenuItem {
                        text: qsTr("Settings…")
                        onTriggered: {
                            overlayDialog.channel = channelItem.channel
                            overlayDialog.settings = channelItem.settings
                            overlayDialog.open()
                        }
                    }
                }
            }
        }
    }
}
