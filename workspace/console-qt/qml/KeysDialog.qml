import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The keys overview (docs/DESIGN-DIALOGE.md S02, A): every key as keycaps in two
 * columns, grouped into the window's keys, the page's and the show's own shortcuts,
 * with a search over key and description. "Shortcuts…" at the start of the footer
 * (KanteDialog.footerStart) opens the shortcuts to change them.
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

    /** `entry` has every word of `text` in its description, key or gesture, case aside;
     *  a number also finds a range of keys ("3" finds 1…5). Typed as a key ("ctrl+s",
     *  a "+" after its first part) it is one: each part must be a cap of the entry's key,
     *  in any order, the last one typed so far only its start ("ctrl+sh" finds Shift);
     *  a "+" at the end, the next part not typed yet, counts for nothing. */
    function matches(entry, text) {
        let typed = String(text).trim().toLowerCase()
        if (typed.length === 0) {
            return true
        }
        if (typed.indexOf("+") > 0) {
            if (typed.endsWith("+") && !typed.endsWith("++")) {
                typed = typed.slice(0, -1)
            }
            const have = entry.key ? caps(entry.key).map(c => c.toLowerCase()) : []
            const parts = caps(typed).map(p => p.trim())
            return parts.every((part, i) => have.some(cap => capFits(cap, part, i === parts.length - 1)))
        }
        const hay = [entry.what, entry.key, entry.gesture].filter(s => !!s).join(" ").toLowerCase()
        const have = entry.key ? caps(entry.key) : []
        return typed.split(/\s+/).every(w => hay.indexOf(w) >= 0 || have.some(cap => inRange(cap, w)))
    }

    /** `cap` is the typed `part`, or starts with it while it is still being typed. */
    function capFits(cap, part, typing) {
        return cap === part || (typing && cap.startsWith(part)) || inRange(cap, part)
    }

    /** `cap` is a range of numbered keys ("1…9") and holds the number `part`. */
    function inRange(cap, part) {
        const range = /^(\d+)\s*[…–-]\s*(\d+)$/.exec(cap)
        if (!range || !/^\d+$/.test(part)) {
            return false
        }
        const n = Number(part)
        return n >= Number(range[1]) && n <= Number(range[2])
    }

    /** A key as caps: "Ctrl+Shift+S" is three, "Ctrl++" two, "1…9" one. */
    function caps(key) {
        return String(key).match(/[^+]+|\+(?=$)/g) || []
    }

    title: qsTr("Keys")
    modal: true
    size: KanteDialog.Size.Medium
    standardButtons: QQC2.Dialog.Close
    footerStart: [
        KanteButton {
            text: qsTr("Shortcuts…")
            emphasis: KanteButton.Emphasis.Quiet
            onClicked: {
                dialog.close()
                dialog.shortcutsRequested()
            }
        }
    ]
    contentHeight: column.implicitHeight
    onOpened: searchField.forceActiveFocus()
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
