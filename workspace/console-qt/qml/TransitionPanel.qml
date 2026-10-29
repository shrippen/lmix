import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Cut, the four transition buttons (click: take preview; right click:
 * effect and length), quick play of preview (SWI.8, the same menu without
 * the stinger), the T-bar and the overlay channels in a row of up to 8
 * (click: the preview input on, or the overlay off; right click: the last
 * input again, the input prepared for the next take away, settings), with
 * all overlays off. Fade to black is the page's own button (MixPage).
 * The transitions in two columns: the panel is no higher than the
 * monitors at 1440 × 900.
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
        return show ? StatePatch.byId(show.inputs, id) || null : null
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

    GridLayout {
        Layout.fillWidth: true
        columns: 2
        rowSpacing: Kirigami.Units.smallSpacing
        columnSpacing: Kirigami.Units.smallSpacing

        Repeater {
            model: panel.transitions

            // The right click sits on this item: a disabled button (nothing in
            // preview) passes it through, so effect and length stay settable.
            Item {
                id: button
                required property string key
                readonly property var modelData: panel.transitionBySlot(key) || ({ slot: Number(key), effect: "cut", duration_ms: 0 })

                Layout.fillWidth: true
                // Both columns as wide, whatever the effect's name.
                Layout.preferredWidth: 1
                Layout.minimumWidth: 0
                implicitHeight: take.implicitHeight

                KanteButton {
                    id: take
                    anchors.fill: parent
                    // The length is in the tooltip and the menu: on the button it
                    // took a second line, and a cut has none to speak of.
                    text: button.modelData.slot + " " + panel.effectLabel(button.modelData.effect)
                    enabled: panel.core.connected && panel.canTake
                    onClicked: panel.act({ action: "transition", slot: button.modelData.slot })
                    QQC2.ToolTip.visible: hovered
                    // The effect's whole name: two columns cut long ones.
                    QQC2.ToolTip.text: panel.effectLabel(button.modelData.effect)
                        + (button.modelData.effect === "cut" ? "" : ", " + Texts.label("durations", button.modelData.duration_ms)) + ". "
                        + qsTr("Click: take preview. Right click: effect and length.")
                }

                TapHandler {
                    acceptedButtons: Qt.RightButton
                    enabled: panel.core.connected
                    onTapped: menu.popup()
                }

                EffectMenu {
                    id: menu
                    heading: qsTr("Button %1 · Effect").arg(button.modelData.slot)
                    effects: panel.effects
                    effect: button.modelData.effect
                    duration: button.modelData.duration_ms
                    durations: panel.durations
                    onEffectPicked: value => panel.act({ action: "set_transition", slot: button.modelData.slot, effect: value, duration_ms: button.modelData.duration_ms })
                    onLengthPicked: ms => panel.act(panel.withLength(button.modelData, ms))
                    // A stinger needs its clip and cut point first.
                    onStingerRequested: panel.editStinger(button.modelData)
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
            text: qsTr("Quick play")
            enabled: panel.core.connected && panel.canTake
            onClicked: panel.act({ action: "quick_play" })
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: panel.effectLabel(quick.setting.effect)
                + (quick.setting.effect === "cut" ? "" : ", " + Texts.label("durations", quick.setting.duration_ms)) + ". " + qsTr("Click: preview to program with this effect, and play it. Double click on the preview monitor does the same. Right click: effect and length.")
        }

        TapHandler {
            acceptedButtons: Qt.RightButton
            enabled: panel.core.connected
            onTapped: quickMenu.popup()
        }

        EffectMenu {
            id: quickMenu
            heading: qsTr("Quick play · Effect")
            effects: panel.effects
            stinger: false
            effect: quick.setting.effect
            duration: quick.setting.duration_ms
            durations: panel.durations
            onEffectPicked: value => panel.act({ action: "set_quick_play", effect: value, duration_ms: quick.setting.duration_ms })
            onLengthPicked: ms => panel.act({ action: "set_quick_play", effect: quick.setting.effect, duration_ms: ms })
        }
    }

    // T-bar: preview fades in over program while pulled; at the end it is
    // taken. Moves are sent at most about 30 times a second. Across the
    // column, without a label (its name is for screen readers).
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
        columns: Math.max(1, Math.min(8, panel.overlays.length))
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
                Layout.minimumWidth: 0
                implicitHeight: channelButton.implicitHeight
                implicitWidth: channelButton.implicitWidth

                KanteButton {
                    id: channelButton
                    readonly property int channel: channelItem.channel
                    readonly property var shown: channelItem.shown
                    readonly property var last: channelItem.last
                    anchors.fill: parent
                    size: KanteButton.Size.Small
                    // Eight in a row: the digit needs the whole width.
                    leftPadding: 0
                    rightPadding: 0
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

                // M10 A: a head with the channel's state, icons, the
                // settings set apart.
                QQC2.Menu {
                    id: channelMenu
                    KantePopupSkin { popup: channelMenu }
                    KanteMenuSection {
                        width: parent ? parent.width : implicitWidth
                        rule: false
                        text: channelItem.shown
                            ? qsTr("Overlay %1 · on · %2").arg(channelItem.channel).arg(channelItem.shown.name)
                            : qsTr("Overlay %1 · off").arg(channelItem.channel)
                    }
                    // Texts never empty: a hidden item with none loops on its width.
                    KanteMenuItem {
                        visible: channelItem.shown === null && channelItem.last !== null
                        height: visible ? implicitHeight : 0
                        icon.name: "view-visible"
                        text: qsTr("Show %1 %2 again").arg(channelItem.last ? channelItem.last.number : "")
                            .arg(channelItem.last ? channelItem.last.name : "")
                        onTriggered: panel.act({ action: "overlay_last", channel: channelItem.channel })
                    }
                    KanteMenuItem {
                        visible: channelItem.prepared !== null
                        height: visible ? implicitHeight : 0
                        icon.name: "edit-clear"
                        text: qsTr("Not with the next take: %1 %2").arg(channelItem.prepared ? channelItem.prepared.number : "")
                            .arg(channelItem.prepared ? channelItem.prepared.name : "")
                        onTriggered: panel.act({ action: "overlay_preview", channel: channelItem.channel, input: null })
                    }
                    QQC2.MenuSeparator {
                        visible: (channelItem.shown === null && channelItem.last !== null) || channelItem.prepared !== null
                        height: visible ? implicitHeight : 0
                    }
                    KanteMenuItem {
                        text: qsTr("Settings…")
                        icon.name: "configure"
                        valueText: channelItem.settings && channelItem.settings.rect ? qsTr("In a box") : ""
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
