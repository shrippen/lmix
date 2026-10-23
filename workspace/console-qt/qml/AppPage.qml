import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * One page of the lmix window (AppShell): its title row and its content.
 * The title row holds the page's tools and its own panic, which stops only
 * what the page is about ("Panic all" in the page bar stops everything).
 *
 *   MIX  1080p30 · …   tools …                       [Fade to black]
 *   ┌ content ─────────────────────────────────────────────────────┐
 */
FocusScope {
    id: page

    /** `mix`, `show`, …: for `--page`, the saved page and Ctrl+n. */
    required property string pageId
    required property string title
    property string subtitle
    /** Keys that work only while this page is shown, as
     *  `{ what, key }`, for the keys overview. */
    property var pageKeys: []
    /** Something runs here (cues, a recording): a dot in the page bar. */
    property bool busy: false
    /** How the dot reads: KantePageBar.Kind (Normal, Info, Warning, Error). */
    property int busyKind: KantePageBar.Kind.Normal
    /** The page shows program itself; otherwise the bar shows it small. */
    property bool hasProgram: false
    /** Entries for the window's File menu. */
    property list<QtObject> fileItems
    /** The page's own panic; none without a text. */
    property string panicText
    property string panicTip
    signal panicRequested()
    /** What takes the keyboard when the page is shown. */
    property Item focusTarget: null
    property alias tools: toolsRow.data
    default property alias content: area.data

    function takeFocus() {
        if (focusTarget) {
            focusTarget.forceActiveFocus()
        } else {
            page.forceActiveFocus()
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.largeSpacing

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing
            KanteHeading {
                level: 2
                pageTitle: true
                text: page.title
            }
            QQC2.Label {
                visible: page.subtitle.length > 0
                text: page.subtitle
                font.family: KanteStyle.monoFamily
                color: KanteStyle.mutedTextColor
                elide: Text.ElideRight
                Layout.maximumWidth: Kirigami.Units.gridUnit * 20
            }
            RowLayout {
                id: toolsRow
                visible: children.length > 0
                spacing: Kirigami.Units.smallSpacing
            }
            Item { Layout.fillWidth: true }
            KanteButton {
                visible: page.panicText.length > 0
                focusPolicy: Qt.NoFocus
                emphasis: KanteButton.Emphasis.Destructive
                text: page.panicText
                QQC2.ToolTip.visible: hovered && page.panicTip.length > 0
                QQC2.ToolTip.text: page.panicTip
                onClicked: page.panicRequested()
            }
        }

        // Height from the page, not from the children: the monitors take
        // what is left.
        Item {
            id: area
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredHeight: 0
        }
    }
}
