import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * One field of a title template (GFX.7): an editor by the field's kind.
 * `edited(value)` fires when the operator is done with it (Enter, focus
 * out, a pick); `typed(value)` on every change, for "Apply" mode.
 */
RowLayout {
    id: row

    /** lmix `TemplateField`. */
    required property var field
    /** Current value as text. */
    property string value: ""
    /** "Source: column" when a data source feeds the field. */
    property string boundTo: ""
    signal edited(string value)
    signal typed(string value)
    signal bindRequested(var button)

    spacing: Kirigami.Units.smallSpacing

    QQC2.Label {
        Layout.preferredWidth: Kirigami.Units.gridUnit * 9
        Layout.alignment: Qt.AlignTop
        Layout.topMargin: Kirigami.Units.smallSpacing
        text: row.field.label
        elide: Text.ElideRight
    }

    Loader {
        Layout.fillWidth: true
        enabled: row.boundTo === ""
        sourceComponent: {
            switch (row.field.kind) {
            case "long_text": return longText
            case "toggle": return toggle
            case "choice": return choice
            default: return line
            }
        }
    }

    QQC2.Label {
        visible: row.boundTo !== ""
        text: "← " + row.boundTo
        color: KanteStyle.mutedTextColor
        font.family: KanteStyle.monoFamily
    }

    KanteButton {
        id: bindButton
        text: qsTr("Data…")
        emphasis: KanteButton.Emphasis.Quiet
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.text: qsTr("Feed this field from a data source column")
        onClicked: row.bindRequested(bindButton)
    }

    Component {
        id: line
        KanteTextField {
            text: row.value
            placeholderText: row.field.kind === "color" ? "#rrggbb"
                : row.field.kind === "image" ? qsTr("Image address or URL")
                : row.field.default || ""
            inputMethodHints: row.field.kind === "number" ? Qt.ImhFormattedNumbersOnly : Qt.ImhNone
            onTextEdited: row.typed(text)
            onEditingFinished: if (text !== row.value) row.edited(text)
        }
    }

    Component {
        id: longText
        QQC2.TextArea {
            text: row.value
            wrapMode: TextEdit.Wrap
            implicitHeight: Kirigami.Units.gridUnit * 4
            onTextChanged: if (activeFocus) row.typed(text)
            onActiveFocusChanged: if (!activeFocus && text !== row.value) row.edited(text)
        }
    }

    Component {
        id: toggle
        QQC2.Switch {
            id: sw
            checked: row.value === "true"
            onToggled: {
                row.typed(checked ? "true" : "false")
                row.edited(checked ? "true" : "false")
            }
            KanteCheckSkin { control: sw; shape: KanteCheckSkin.Shape.Switch }
        }
    }

    Component {
        id: choice
        QQC2.ComboBox {
            id: box
            model: row.field.choices
            currentIndex: Math.max(0, row.field.choices.indexOf(row.value))
            onActivated: index => {
                row.typed(row.field.choices[index])
                row.edited(row.field.choices[index])
            }
            KanteFieldSkin { control: box }
        }
    }
}
