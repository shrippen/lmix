import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante
import org.lmix.console

/**
 * The Setup page (docs/PLAN-SEITEN.md): what used to be spread over menus
 * and buttons, in sections. The show's settings travel with the show;
 * this computer's stay here (targets, keys, style).
 */
AppPage {
    id: root

    /** The window (AppShell): show, actions, helpers. */
    required property var shell
    readonly property var show: shell.show
    readonly property var backend: shell.backend
    function act(action) { shell.act(action) }

    pageId: "setup"
    title: qsTr("Setup")
    subtitle: qsTr("show settings travel with the show, this computer's stay here")

    readonly property real titleWidth: Kirigami.Units.gridUnit * 14

    QQC2.ScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            width: Math.min(scroll.availableWidth, Kirigami.Units.gridUnit * 50)
            spacing: Kirigami.Units.largeSpacing

            // --- the show ---
            KanteSettingRow {
                Layout.fillWidth: true
                section: qsTr("Show")
                titleWidth: root.titleWidth
                title: qsTr("Program format")
                hint: root.show && root.show.format ? qsTr("%1 × %2 at %3 fps").arg(root.show.format.width).arg(root.show.format.height).arg(root.show.format.framerate.replace("/1", "")) : ""
                KanteButton {
                    text: qsTr("Change…")
                    enabled: root.backend.connected
                    onClicked: formatDialog.open()
                }
            }
            KanteSettingRow {
                Layout.fillWidth: true
                titleWidth: root.titleWidth
                title: qsTr("Sound")
                hint: qsTr("48 kHz by default; 44.1 kHz for music productions (AUD.17)")
                KanteSegmented {
                    model: [qsTr("48 kHz"), qsTr("44.1 kHz")]
                    currentIndex: root.show && root.show.sample_rate === 44100 ? 1 : 0
                    onActivated: index => root.act({ action: "set_sample_rate", hz: index === 1 ? 44100 : 48000 })
                }
            }
            KanteSettingRow {
                Layout.fillWidth: true
                titleWidth: root.titleWidth
                title: qsTr("Package")
                hint: qsTr("The show with its media in one file, to move it to another machine")
                RowLayout {
                    KanteButton {
                        text: qsTr("Export…")
                        enabled: root.backend.connected
                        onClicked: root.shell.openShellDialog("exportPackage")
                    }
                    KanteButton {
                        text: qsTr("Import…")
                        enabled: root.backend.connected
                        onClicked: root.shell.openShellDialog("importPackage")
                    }
                }
            }

            // --- control: targets, data, zones, keys ---
            KanteSettingRow {
                Layout.fillWidth: true
                section: qsTr("Control")
                titleWidth: root.titleWidth
                title: qsTr("Targets")
                hint: qsTr("Output targets (HTTP, Home Assistant, OSC) and OSC subscribers of this machine")
                RowLayout {
                    KanteButton {
                        text: qsTr("Targets…")
                        enabled: root.backend.connected
                        onClicked: targetsDialog.open()
                    }
                    KanteButton {
                        text: qsTr("Check targets")
                        enabled: root.backend.connected
                        onClicked: root.act({ action: "check_targets" })
                    }
                }
            }
            KanteSettingRow {
                Layout.fillWidth: true
                titleWidth: root.titleWidth
                title: qsTr("Data sources")
                hint: qsTr("CSV, JSON or text files and addresses for title fields (DAT.1–5)")
                KanteButton {
                    text: qsTr("Data…")
                    enabled: root.backend.connected
                    onClicked: dataSourcesDialog.open()
                }
            }
            KanteSettingRow {
                Layout.fillWidth: true
                titleWidth: root.titleWidth
                title: qsTr("Zones")
                hint: qsTr("The show's sound zones and where they play on this machine")
                KanteButton {
                    text: qsTr("Zones…")
                    enabled: root.backend.connected
                    onClicked: zonesDialog.open()
                }
            }
            KanteSettingRow {
                Layout.fillWidth: true
                titleWidth: root.titleWidth
                title: qsTr("Shortcuts")
                hint: qsTr("Keys, MIDI and OSC that run actions (AUT.1–3)")
                KanteButton {
                    text: qsTr("Shortcuts…")
                    enabled: root.backend.connected
                    onClicked: root.shell.openShortcuts()
                }
            }
            KanteSettingRow {
                Layout.fillWidth: true
                titleWidth: root.titleWidth
                title: qsTr("Keys at the desktop")
                hint: qsTr("Shortcut keys work while lmix is not in front (AUT.12)")
                QQC2.Switch {
                    id: globalSwitch
                    checked: root.shell.globalShortcuts
                    onToggled: root.shell.setGlobalShortcuts(checked)
                    KanteCheckSkin { control: globalSwitch; shape: KanteCheckSkin.Shape.Switch }
                }
            }

            // --- this computer ---
            KanteSettingRow {
                Layout.fillWidth: true
                section: qsTr("This computer")
                titleWidth: root.titleWidth
                title: qsTr("Style")
                KanteSegmented {
                    readonly property var kinds: [KanteStyle.Kind.Kante, KanteStyle.Kind.KanteLight, KanteStyle.Kind.System]
                    model: [qsTr("Kante"), qsTr("Kante Light"), qsTr("System")]
                    currentIndex: kinds.indexOf(KanteStyle.kind)
                    tooltips: [qsTr("Kante: Gruvbox dark, Leinen when the system theme is light"), qsTr("Kante Light: only the shapes of Kante, on the system colours"), qsTr("System style (Breeze)")]
                    onActivated: index => root.shell.setStyle(kinds[index])
                }
            }
            KanteSettingRow {
                Layout.fillWidth: true
                titleWidth: root.titleWidth
                title: qsTr("Lock")
                hint: qsTr("While locked, closing inputs, loading a show, stopping outputs and changing format, sample rate or multiview are refused.")
                QQC2.Switch {
                    id: lockSwitch
                    checked: root.show !== null && root.show.locked === true
                    enabled: root.backend.connected
                    onToggled: root.act({ action: "set_lock", on: checked })
                    KanteCheckSkin { control: lockSwitch; shape: KanteCheckSkin.Shape.Switch }
                }
            }
            KanteSettingRow {
                Layout.fillWidth: true
                titleWidth: root.titleWidth
                title: qsTr("Latency")
                hint: Texts.latency(root.backend.latency)
                KanteButton {
                    text: qsTr("Measure")
                    busy: root.backend.measuring
                    enabled: root.backend.connected && !root.backend.measuring
                    onClicked: root.backend.measureLatency()
                }
            }
            KanteSettingRow {
                Layout.fillWidth: true
                titleWidth: root.titleWidth
                title: qsTr("Settings of this machine")
                hint: qsTr("Shortcuts for all shows, layout templates, targets, subscribers; no secrets")
                RowLayout {
                    KanteButton {
                        text: qsTr("Export…")
                        enabled: root.backend.connected
                        onClicked: root.shell.openShellDialog("exportSettings")
                    }
                    KanteButton {
                        text: qsTr("Import…")
                        enabled: root.backend.connected
                        onClicked: root.shell.openShellDialog("importSettings")
                    }
                    KanteButton {
                        emphasis: KanteButton.Emphasis.Destructive
                        text: qsTr("Factory settings…")
                        enabled: root.backend.connected
                        onClicked: root.shell.openShellDialog("resetSettings")
                    }
                }
            }
            KanteSettingRow {
                Layout.fillWidth: true
                titleWidth: root.titleWidth
                title: qsTr("lmix-core")
                hint: root.backend.connected ? qsTr("connected") : qsTr("offline")
                QQC2.Label {
                    text: root.backend.coreUrl
                    font.family: KanteStyle.monoFamily
                }
            }
            // The machine lmix-core runs on (RUN.8): cue times hold there.
            KanteSettingRow {
                Layout.fillWidth: true
                titleWidth: root.titleWidth
                title: qsTr("Cue timer")
                hint: {
                    var m = root.shell.machine
                    if (m === null) {
                        return qsTr("No report from lmix-core yet")
                    }
                    var w = root.shell.machineWarning
                    return w.level > 0 ? w.text + ": " + w.detail : w.detail
                }
                QQC2.Label {
                    text: Texts.cueTimer(root.shell.machine)
                    color: root.shell.machineWarning.level > 1 ? KanteStyle.negativeTextColor
                        : root.shell.machine !== null && root.shell.machine.cue_timer !== "realtime" ? KanteStyle.neutralTextColor
                        : KanteStyle.mutedTextColor
                }
            }
        }
    }

    FormatDialog {
        id: formatDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        format: root.show && root.show.format ? root.show.format : ({ width: 1920, height: 1080, framerate: "30/1" })
    }
    TargetsDialog {
        id: targetsDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        targets: root.show && root.show.targets ? root.show.targets : []
        feeds: root.show && root.show.feeds ? root.show.feeds : []
    }
    DataSourcesDialog {
        id: dataSourcesDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        coreUrl: root.backend.coreUrl
        sources: root.show && root.show.data_sources ? root.show.data_sources : []
    }
    ZonesDialog {
        id: zonesDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        zones: root.show && root.show.zones ? root.show.zones : []
    }
}
