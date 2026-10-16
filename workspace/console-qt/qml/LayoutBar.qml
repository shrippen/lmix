import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import Kante

/**
 * Layouts: one button each (click: into preview; right click: edit,
 * save as template, remove), red on program and green in preview like a
 * tile. "+ Layout" adds one; "Templates" (LAY.5, kept apart from the show
 * for every show) adds one from a template, removes templates and moves
 * them to and from files.
 */
RowLayout {
    id: bar

    required property var core
    required property var show
    required property var act
    signal editRequested(var layout)

    readonly property var layouts: show && show.layouts ? show.layouts : []
    readonly property var templates: show && show.layout_templates ? show.layout_templates : []
    readonly property var templateChoices: templates.map(t => ({ value: t.name, label: t.name }))

    function localPath(url) {
        return decodeURIComponent(url.toString().replace(/^file:\/\//, ""))
    }

    spacing: Kirigami.Units.smallSpacing

    KanteSectionLabel { text: qsTr("Layouts") }

    Repeater {
        model: bar.layouts

        Item {
            id: entry
            required property var modelData
            readonly property bool onProgram: bar.show !== null && bar.show.program === modelData.id
            readonly property bool onPreview: bar.show !== null && bar.show.preview === modelData.id

            implicitWidth: button.implicitWidth
            implicitHeight: button.implicitHeight

            KanteButton {
                id: button
                anchors.fill: parent
                text: entry.modelData.number + " " + entry.modelData.name
                emphasis: entry.onProgram ? KanteButton.Emphasis.Destructive
                    : entry.onPreview ? KanteButton.Emphasis.Primary : KanteButton.Emphasis.Normal
                enabled: bar.core.connected && !entry.onProgram
                onClicked: bar.act({ action: "preview_layout", layout: entry.modelData.number })
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Click: into preview. Right click: edit or remove.")
            }

            // On the item: a disabled button (on program) passes it through.
            TapHandler {
                acceptedButtons: Qt.RightButton
                enabled: bar.core.connected
                onTapped: menu.popup()
            }

            QQC2.Menu {
                id: menu
                KantePopupSkin { popup: menu }
                QQC2.MenuItem {
                    text: qsTr("Edit…")
                    onTriggered: bar.editRequested(entry.modelData)
                }
                QQC2.MenuItem {
                    text: qsTr("Save as template")
                    onTriggered: bar.act({ action: "save_layout_template", layout: entry.modelData.number })
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: qsTr("Keeps its places for every show, under its name (one of the same name is replaced).")
                }
                QQC2.MenuItem {
                    text: qsTr("Remove")
                    enabled: !entry.onProgram
                    onTriggered: bar.act({ action: "remove_layout", layout: entry.modelData.number })
                }
            }
        }
    }

    KanteButton {
        text: qsTr("+ Layout")
        enabled: bar.core.connected && bar.layouts.length < 4
        onClicked: bar.editRequested(null)
    }

    KanteButton {
        text: qsTr("Templates")
        enabled: bar.core.connected
        onClicked: templateMenu.popup()
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.text: qsTr("Layout templates, kept for every show: a new layout from one, import, export.")

        QQC2.Menu {
            id: templateMenu
            KantePopupSkin { popup: templateMenu }

            ChoiceMenu {
                title: qsTr("New layout from")
                enabled: bar.templates.length > 0 && bar.layouts.length < 4
                choices: bar.templateChoices
                onPicked: value => bar.act({ action: "load_layout_template", template: value })
            }
            ChoiceMenu {
                title: qsTr("Remove template")
                enabled: bar.templates.length > 0
                choices: bar.templateChoices
                onPicked: value => bar.act({ action: "remove_layout_template", template: value })
            }
            QQC2.MenuItem {
                text: qsTr("Import…")
                onTriggered: importDialog.open()
            }
            QQC2.MenuItem {
                text: qsTr("Export all…")
                enabled: bar.templates.length > 0
                onTriggered: exportDialog.open()
            }
        }
    }

    FileDialog {
        id: importDialog
        title: qsTr("Import layout templates")
        nameFilters: [qsTr("Layout templates (*.json)"), qsTr("All files (*)")]
        onAccepted: bar.act({ action: "import_layout_templates", path: bar.localPath(selectedFile) })
    }
    FileDialog {
        id: exportDialog
        title: qsTr("Export layout templates")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "json"
        nameFilters: [qsTr("Layout templates (*.json)")]
        onAccepted: bar.act({ action: "export_layout_templates", path: bar.localPath(selectedFile) })
    }

    Item { Layout.fillWidth: true }
}
