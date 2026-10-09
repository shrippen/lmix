import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Cut, the four transition buttons (click: take preview; right click:
 * effect and length) and fade to black.
 */
ColumnLayout {
    id: panel

    required property var core
    required property var show
    required property var act
    required property var transitions
    required property var transitionBySlot

    readonly property bool canTake: show !== null && show.preview !== null
    readonly property var effects: JSON.parse(core.choices("effects"))
    readonly property var durations: JSON.parse(core.choices("durations"))

    function effectLabel(name) {
        var match = effects.find(e => e.value === name)
        return match ? match.label : name
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
                    choices: panel.durations.map(ms => ({ value: ms, label: qsTr("%1 ms").arg(ms) }))
                    current: button.modelData.duration_ms
                    onPicked: value => panel.act({ action: "set_transition", slot: button.modelData.slot, effect: button.modelData.effect, duration_ms: value })
                }
            }
        }
    }

    KanteButton {
        Layout.fillWidth: true
        text: panel.show && panel.show.fade_to_black ? qsTr("FTB on") : qsTr("FTB")
        emphasis: panel.show && panel.show.fade_to_black ? KanteButton.Emphasis.Destructive : KanteButton.Emphasis.Normal
        enabled: panel.core.connected
        onClicked: panel.act({ action: "fade_to_black" })
    }
}
