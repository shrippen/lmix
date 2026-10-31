import QtQuick
import QtQuick.Layouts
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import Kante

/**
 * The file of a sound or picture cue: typed, or picked with "Choose…".
 * Like a time field (DurationField) a path that cannot be used, one that
 * is not absolute, stays in the field, marked, and Enter on it keeps the
 * keys there; it goes when it is fixed, on Escape, or when `key` (whose
 * file it is) changes.
 *
 *   [/media/fx/rain.flac            ] [CHOOSE…]
 */
RowLayout {
    id: row

    /** The file; empty shows an empty field. */
    property string path: ""
    /** Whose file it is, e.g. the cue's id. */
    property var key
    property alias nameFilters: dialog.nameFilters
    property alias title: dialog.title
    property alias field: field
    /** The file dialog is open. */
    readonly property bool picking: dialog.visible
    /** A new, absolute path, typed or picked. */
    signal edited(string path)
    /** Enter took the path: the keys may go back to the show. */
    signal accepted()

    spacing: Kirigami.Units.smallSpacing

    KanteTextField {
        id: field
        Layout.fillWidth: true
        font.family: KanteStyle.monoFamily
        invalid: text.trim() !== "" && !text.trim().startsWith("/")
        Accessible.name: row.title

        // What is typed stays until it is taken; a new state meanwhile
        // does not overwrite it.
        property bool typing: false
        onActiveFocusChanged: if (activeFocus) { typing = true }
        Binding on text {
            when: !field.typing
            value: row.path
            restoreMode: Binding.RestoreNone
        }
        Keys.onReturnPressed: event => event.accepted = field.invalid
        Keys.onEnterPressed: event => event.accepted = field.invalid
        onAccepted: row.accepted()
        onEditingFinished: {
            if (invalid) {
                return
            }
            var next = text.trim()
            if (next !== "" && next !== row.path) {
                row.edited(next)
            }
            typing = false
        }
    }
    KanteButton {
        focusPolicy: Qt.NoFocus
        text: qsTr("Choose…")
        onClicked: dialog.open()
    }

    Connections {
        target: row
        function onKeyChanged() { field.typing = false }
    }

    FileDialog {
        id: dialog
        fileMode: FileDialog.OpenFile
        onAccepted: {
            field.typing = false
            row.edited(decodeURIComponent(selectedFile.toString().replace(/^file:\/\//, "")))
        }
    }
}
