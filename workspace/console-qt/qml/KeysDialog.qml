import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The keys overview (docs/DESIGN-DIALOGE.md S02, A): every key as keycaps in two
 * columns, grouped into the window's keys, the page's and the show's own shortcuts,
 * with a search over key and description. "Shortcuts…" in the footer opens the
 * shortcuts to change them.
 *
 * An entry is `{ what, key }` (key as the user reads it, "Ctrl+K", one cap per part)
 * or `{ what, gesture }` for what is no key ("double click preview").
 */
KanteDialog {
    id: dialog

    /** The window's keys, on every page. */
    property var always: []
    /** The page shown now: its title and its own keys. */
    property string pageTitle: ""
    property var pageKeys: []
    /** The shortcuts of the show and of all shows that have a key. */
    property var own: []
    /** "Shortcuts…" was pressed. */
    signal shortcutsRequested()

    /** The search, as typed. */
    property alias query: searchField.text

    readonly property var groups: [
        { title: qsTr("Always"), keys: always },
        { title: pageTitle.length > 0 ? qsTr("Only on %1").arg(pageTitle) : "", keys: pageKeys },
        { title: qsTr("Own shortcuts"), keys: own }
    ].map(g => ({ title: g.title, keys: g.keys.filter(k => dialog.matches(k, query)) }))
     .filter(g => g.title.length > 0 && g.keys.length > 0)

    /** `entry` has every word of `text` in its description, key or gesture, case aside.
     *  Typed as a key ("ctrl+s", a "+" in it) it is one: the entry's key needs each
     *  part as a cap, in any order. */
    function matches(entry, text) {
        const typed = String(text).trim().toLowerCase()
        if (typed.length === 0) {
            return true
        }
        if (typed.indexOf("+") > 0) {
            const have = entry.key ? caps(entry.key).map(c => c.toLowerCase()) : []
            return caps(typed).every(part => have.indexOf(part.trim()) >= 0)
        }
        const hay = [entry.what, entry.key, entry.gesture].filter(s => !!s).join(" ").toLowerCase()
        return typed.split(/\s+/).every(w => hay.indexOf(w) >= 0)
    }

    /** A key as caps: "Ctrl+Shift+S" is three, "Ctrl++" two, "1…9" one. */
    function caps(key) {
        return String(key).match(/[^+]+|\+(?=$)/g) || []
    }

    title: qsTr("Keys")
    modal: true
    size: KanteDialog.Size.Medium
    // Help sits at the start of the footer in every layout: it opens the shortcuts.
    standardButtons: QQC2.Dialog.Help | QQC2.Dialog.Close
    contentHeight: column.implicitHeight
    onHelpRequested: {
        close()
        shortcutsRequested()
    }
    // Named again on every open and footer: the button box gives the standard text
    // back on a language change, and each style has a footer of its own.
    function nameShortcutsButton() {
        const button = standardButton(QQC2.Dialog.Help)
        if (button) {
            button.text = qsTr("Shortcuts…")
        }
    }
    Component.onCompleted: Qt.callLater(nameShortcutsButton)
    onFooterChanged: Qt.callLater(nameShortcutsButton)
    onOpened: {
        nameShortcutsButton()
        searchField.forceActiveFocus()
    }
    onClosed: searchField.clear()

    ColumnLayout {
        id: column
        anchors.fill: parent
        spacing: Kirigami.Units.smallSpacing

        KanteTextField {
            id: searchField
            objectName: "keysSearch"
            Layout.fillWidth: true
            placeholderText: qsTr("Search keys")
            Accessible.name: qsTr("Search keys")
        }

        QQC2.ScrollView {
            id: scroller
            Layout.fillWidth: true
            Layout.fillHeight: true
            implicitHeight: groupsColumn.implicitHeight
            contentWidth: availableWidth
            clip: true

            ColumnLayout {
                id: groupsColumn
                width: scroller.availableWidth
                spacing: Kirigami.Units.smallSpacing

                QQC2.Label {
                    Layout.fillWidth: true
                    visible: dialog.groups.length === 0
                    wrapMode: Text.WordWrap
                    color: KanteStyle.mutedTextColor
                    text: qsTr("No key matches \"%1\".").arg(dialog.query)
                }

                Repeater {
                    model: dialog.groups

                    ColumnLayout {
                        id: group
                        required property var modelData
                        required property int index
                        objectName: "keysGroup"
                        Layout.fillWidth: true
                        Layout.topMargin: index > 0 ? Kirigami.Units.smallSpacing : 0
                        spacing: Kirigami.Units.smallSpacing

                        KanteSectionLabel {
                            Layout.fillWidth: true
                            text: group.modelData.title
                        }
                        GridLayout {
                            Layout.fillWidth: true
                            columns: 2
                            columnSpacing: Kirigami.Units.gridUnit
                            rowSpacing: Kirigami.Units.smallSpacing

                            Repeater {
                                model: group.modelData.keys

                                RowLayout {
                                    id: entry
                                    required property var modelData
                                    objectName: "keysEntry"
                                    // Two equal columns: the width, not the content, sets them.
                                    Layout.fillWidth: true
                                    Layout.preferredWidth: 1
                                    spacing: Kirigami.Units.smallSpacing
                                    Accessible.role: Accessible.StaticText
                                    Accessible.name: entry.modelData.what + ": " + (entry.modelData.key || entry.modelData.gesture)

                                    Repeater {
                                        model: entry.modelData.key ? dialog.caps(entry.modelData.key) : []
                                        KanteKeycap {
                                            required property string modelData
                                            text: modelData
                                        }
                                    }
                                    QQC2.Label {
                                        visible: !entry.modelData.key && !!entry.modelData.gesture
                                        text: entry.modelData.gesture || ""
                                        font: KanteStyle.smallFont
                                        color: KanteStyle.mutedTextColor
                                    }
                                    QQC2.Label {
                                        Layout.fillWidth: true
                                        text: entry.modelData.what
                                        elide: Text.ElideRight
                                        Accessible.ignored: true
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
