import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Cut, the four transition buttons (click: take preview; right click:
 * effect and length), fade to black and the overlay channels (click: the
 * preview input on, or the overlay off).
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

    function inputById(id) {
        return show ? show.inputs.find(i => i.id === id) || null : null
    }
    readonly property var effects: Texts.choices("effects", core.choices("effects"))
    readonly property var durations: JSON.parse(core.choices("durations"))

    function effectLabel(name) {
        return Texts.label("effects", name)
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

        KanteButton {
            id: button
            required property string key
            readonly property var modelData: panel.transitionBySlot(key) || ({ slot: Number(key), effect: "cut", duration_ms: 0 })

            Layout.fillWidth: true
            size: KanteButton.Size.Large
            text: modelData.slot + " " + panel.effectLabel(modelData.effect) + "\n" + modelData.duration_ms + " ms"
            enabled: panel.core.connected && panel.canTake
            onClicked: panel.act({ action: "transition", slot: modelData.slot })
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: qsTr("Click: take preview. Right click: effect and length.")

            TapHandler {
                acceptedButtons: Qt.RightButton
                onTapped: menu.popup()
            }

            QQC2.Menu {
                id: menu
                KantePopupSkin { popup: menu }

                ChoiceMenu {
                    title: qsTr("Effect")
                    choices: panel.effects
                    current: button.modelData.effect
                    onPicked: value => panel.act({ action: "set_transition", slot: button.modelData.slot, effect: value, duration_ms: button.modelData.duration_ms })
                }
                ChoiceMenu {
                    title: qsTr("Length")
                    choices: panel.durations.map(ms => ({ value: ms, label: Texts.label("durations", ms) }))
                    current: button.modelData.duration_ms
                    onPicked: value => panel.act({ action: "set_transition", slot: button.modelData.slot, effect: button.modelData.effect, duration_ms: value })
                }
            }
        }
    }

    // T-bar: preview fades in over program while pulled; at the end it is
    // taken. Moves are sent at most about 30 times a second.
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

    KanteButton {
        Layout.fillWidth: true
        text: panel.show && panel.show.fade_to_black ? qsTr("FTB on") : qsTr("FTB")
        emphasis: panel.show && panel.show.fade_to_black ? KanteButton.Emphasis.Destructive : KanteButton.Emphasis.Normal
        enabled: panel.core.connected
        onClicked: panel.act({ action: "fade_to_black" })
    }

    KanteSectionLabel {
        visible: overlayRow.visible
        text: qsTr("Overlays")
        color: KanteStyle.mutedTextColor
    }
    RowLayout {
        id: overlayRow
        Layout.fillWidth: true
        visible: panel.overlays.length > 0
        spacing: Kirigami.Units.smallSpacing

        Repeater {
            model: panel.overlays.length
            KanteButton {
                required property int index
                readonly property int channel: index + 1
                readonly property var shown: panel.overlays[index] ? panel.inputById(panel.overlays[index]) : null
                Layout.fillWidth: true
                text: shown ? channel + "\n" + shown.number : channel
                emphasis: shown ? KanteButton.Emphasis.Destructive : KanteButton.Emphasis.Normal
                enabled: panel.core.connected && (shown !== null || panel.canTake)
                Accessible.name: shown ? qsTr("Overlay %1: %2, click to take off").arg(channel).arg(shown.name)
                    : qsTr("Overlay %1: empty, click to show preview").arg(channel)
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: shown ? qsTr("Overlay %1 shows %2 %3. Click: take off.").arg(channel).arg(shown.number).arg(shown.name)
                    : qsTr("Overlay %1 is empty. Click: show the preview input.").arg(channel)
                onClicked: panel.act(shown ? { action: "overlay_out", channel: channel }
                    : { action: "overlay_in", channel: channel, input: panel.show.preview })
            }
        }
    }
}
