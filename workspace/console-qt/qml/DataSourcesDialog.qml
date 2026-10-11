import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Data sources (DAT.1–5): CSV, JSON or text files or addresses whose
 * selected row feeds title fields. A click on a row selects it; previous / next, or moving on by
 * itself every few seconds, looping if wanted. lmix-core reads a file
 * again when it changes, an address every 10 s.
 */
KanteDialog {
    id: dialog

    required property var act
    /** `ShowState.data_sources`. */
    required property var sources

    property int current: 0
    readonly property var source: sources.length > 0 ? sources[Math.min(current, sources.length - 1)] : null
    /** Kind of source by its name: JSON, text (one row per line), else CSV
     * (also a Google Sheet's CSV export). */
    function kindOf(file) {
        var name = file.trim().toLowerCase().split("?")[0]
        if (name.endsWith(".json")) return "json"
        if (name.endsWith(".txt")) return "text"
        return "csv"
    }
    /** Auto-advance choices in seconds; 0 off. */
    readonly property var intervals: [0, 3, 5, 10, 15, 30, 60]

    title: qsTr("Data sources")
    modal: true
    width: Kirigami.Units.gridUnit * 44
    standardButtons: QQC2.Dialog.Close
    contentHeight: column.implicitHeight

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            QQC2.ComboBox {
                id: sourceBox
                Layout.fillWidth: true
                visible: dialog.sources.length > 0
                model: dialog.sources.map(s => s.number + " " + s.name)
                currentIndex: dialog.current
                onActivated: index => dialog.current = index
                KanteFieldSkin { control: sourceBox }
            }
            KanteButton {
                text: qsTr("Remove")
                visible: dialog.source !== null
                onClicked: dialog.act({ action: "remove_data_source", source: dialog.source.number })
            }
        }

        RowLayout {
            Layout.fillWidth: true
            KanteTextField {
                id: path
                Layout.fillWidth: true
                placeholderText: qsTr("CSV, JSON or text file (relative to the show file), or http(s) address")
            }
            QQC2.CheckBox {
                id: header
                text: qsTr("First row names columns")
                checked: true
                visible: dialog.kindOf(path.text) === "csv"
                KanteCheckSkin { control: header }
            }
            KanteButton {
                text: qsTr("+ Source")
                enabled: path.text.trim() !== ""
                onClicked: {
                    var file = path.text.trim()
                    var kind = dialog.kindOf(file)
                    var spec = kind === "csv" ? { kind: kind, path: file, header: header.checked } : { kind: kind, path: file }
                    dialog.act({ action: "add_data_source", spec: spec })
                    dialog.current = dialog.sources.length
                    path.text = ""
                }
            }
        }

        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.source === null
            wrapMode: Text.WordWrap
            text: qsTr("No data sources yet. A data source is a table (CSV, JSON) whose selected row fills title fields, e.g. names and roles of guests.")
        }

        KanteCallout {
            Layout.fillWidth: true
            visible: dialog.source !== null && !!dialog.source.error
            kind: KanteCallout.Kind.Danger
            title: qsTr("The file cannot be read")
            text: dialog.source && dialog.source.error ? dialog.source.error : ""
        }

        QQC2.ScrollView {
            id: scroll
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(rows.implicitHeight, Kirigami.Units.gridUnit * 16)
            visible: dialog.source !== null && dialog.source.rows.length > 0

            ColumnLayout {
                id: rows
                width: scroll.availableWidth
                spacing: 0

                // Column names.
                RowLayout {
                    Layout.fillWidth: true
                    QQC2.Label { Layout.preferredWidth: Kirigami.Units.gridUnit * 2; text: "" }
                    Repeater {
                        model: dialog.source ? dialog.source.columns : []
                        QQC2.Label {
                            required property var modelData
                            Layout.fillWidth: true
                            Layout.preferredWidth: 1
                            text: modelData
                            font.family: KanteStyle.monoFamily
                            color: KanteStyle.mutedTextColor
                            elide: Text.ElideRight
                        }
                    }
                }

                Repeater {
                    model: dialog.source ? dialog.source.rows : []

                    Rectangle {
                        id: rowEntry
                        required property var modelData
                        required property int index
                        readonly property bool selected: dialog.source.row === index + 1
                        Layout.fillWidth: true
                        implicitHeight: cells.implicitHeight + Kirigami.Units.smallSpacing
                        color: selected ? KanteStyle.selectionColor : "transparent"

                        MouseArea {
                            anchors.fill: parent
                            onClicked: dialog.act({ action: "data_row", source: dialog.source.number, row: rowEntry.index + 1 })
                        }
                        RowLayout {
                            id: cells
                            anchors.fill: parent
                            anchors.leftMargin: Kirigami.Units.smallSpacing
                            QQC2.Label {
                                Layout.preferredWidth: Kirigami.Units.gridUnit * 2
                                text: (rowEntry.selected ? "▶ " : "") + (rowEntry.index + 1)
                                font.family: KanteStyle.monoFamily
                            }
                            Repeater {
                                model: rowEntry.modelData
                                QQC2.Label {
                                    required property var modelData
                                    Layout.fillWidth: true
                                    Layout.preferredWidth: 1
                                    text: modelData
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: dialog.source !== null

            KanteButton {
                text: qsTr("Previous")
                enabled: dialog.source !== null && dialog.source.row > 1
                onClicked: dialog.act({ action: "data_previous", source: dialog.source.number })
            }
            KanteButton {
                text: qsTr("Next")
                emphasis: KanteButton.Emphasis.Primary
                enabled: dialog.source !== null && (dialog.source.row < dialog.source.rows.length || dialog.source.loop_rows)
                onClicked: dialog.act({ action: "data_next", source: dialog.source.number })
            }
            Item { Layout.fillWidth: true }
            QQC2.Label { text: qsTr("Move on") }
            QQC2.ComboBox {
                id: autoBox
                model: dialog.intervals.map(s => s === 0 ? qsTr("off") : qsTr("every %1 s").arg(s))
                currentIndex: dialog.source && dialog.source.auto_ms
                    ? Math.max(0, dialog.intervals.indexOf(dialog.source.auto_ms / 1000)) : 0
                onActivated: index => dialog.act({ action: "set_data_auto", source: dialog.source.number,
                    interval_ms: dialog.intervals[index] === 0 ? null : dialog.intervals[index] * 1000,
                    loop_rows: loopBox.checked })
                KanteFieldSkin { control: autoBox }
            }
            QQC2.CheckBox {
                id: loopBox
                text: qsTr("Loop")
                checked: dialog.source !== null && dialog.source.loop_rows
                onToggled: dialog.act({ action: "set_data_auto", source: dialog.source.number,
                    interval_ms: dialog.source.auto_ms || null, loop_rows: checked })
                KanteCheckSkin { control: loopBox }
            }
        }
    }
}
