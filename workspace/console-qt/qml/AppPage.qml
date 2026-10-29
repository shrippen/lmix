import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * One page of the lmix window (AppShell): its content, and what the
 * window's header shows for it while it is the current page: title,
 * subtitle, tools and its own panic, which stops only what the page is
 * about ("Panic all" beside it stops everything). The header holds them,
 * so the page keeps its height for the content.
 *
 *   lmix  File  MIX 1080p30  tools …   …   [FTB] [Panic all]
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
    /** The page's panic is in force (program black): its button says so. */
    property bool panicActive: false
    signal panicRequested()
    /** What takes the keyboard when the page is shown. */
    property Item focusTarget: null
    property alias tools: toolsRow.data
    /** The tools as one item: the window's header takes it in while the
     *  page is current. */
    readonly property Item toolsItem: toolsRow
    default property alias content: area.data

    function takeFocus() {
        if (focusTarget) {
            focusTarget.forceActiveFocus()
        } else {
            page.forceActiveFocus()
        }
    }

    // Shown only in the window's header (AppShell moves it there).
    RowLayout {
        id: toolsRow
        visible: children.length > 0 && parent !== page
        spacing: Kirigami.Units.smallSpacing
    }

    Item {
        id: area
        anchors.fill: parent
    }
}
