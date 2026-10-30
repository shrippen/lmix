pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The show's zones side by side (SUI.8): each with its level as heard,
 * its volume in dB and mute. A volume move sets the zone at once; a cue
 * that fades the zone moves the slider to where the fade goes. In the show
 * view and on the Audio page.
 */
Flow {
    id: bar

    required property var act
    required property var core
    /** `zones` of the show state. */
    property var zones: []
    /** AppShell.levels, or null. */
    property var levels: null
    /** Width of one zone. */
    property real zoneWidth: Kirigami.Units.gridUnit * 14
    /** `min_db` and `max_db` of a zone's volume (lmix-types). */
    readonly property var limits: JSON.parse(core.choices("level_limits"))

    spacing: Kirigami.Units.largeSpacing

    // Rows by name: every volume step brings a new state, and a rebuilt
    // row would end the drag under the finger.
    KeyedModel { id: zoneModel }
    Component.onCompleted: zoneModel.sync(zones.map(z => z.name))
    onZonesChanged: zoneModel.sync(zones.map(z => z.name))

    Repeater {
        model: zoneModel
        delegate: ColumnLayout {
            id: zone
            required property string key
            readonly property var modelData: bar.zones.find(z => z.name === key) ?? { name: key, gain_db: 0, mute: false, outputs: [] }
            width: bar.zoneWidth
            spacing: 0

            /** Sets the fields in `changes` at once. The core sends the
             *  zone's whole level (strom mutes by volume 0), so this also
             *  ends a fade of the zone that a cue runs. */
            function set(changes) {
                bar.act(Object.assign({ action: "set_zone_level", zone: zone.modelData.name, duration_ms: 0 }, changes))
            }

            RowLayout {
                Layout.fillWidth: true
                QQC2.Label {
                    Layout.fillWidth: true
                    text: zone.modelData.name
                    elide: Text.ElideRight
                    color: zone.modelData.outputs.length === 0 ? KanteStyle.warningColor : KanteStyle.textColor
                    QQC2.ToolTip.visible: zone.modelData.outputs.length === 0 && nameHover.hovered
                    QQC2.ToolTip.text: qsTr("Plays nowhere on this machine")
                    HoverHandler { id: nameHover }
                }
                QQC2.Switch {
                    id: mute
                    text: qsTr("Mute")
                    checked: zone.modelData.mute
                    onToggled: zone.set({ mute: checked })
                    KanteCheckSkin { control: mute; shape: KanteCheckSkin.Shape.Switch }
                }
            }
            KanteAudioMeter {
                Layout.fillWidth: true
                level: (bar.levels ? bar.levels.peak("zone:" + zone.modelData.name) : undefined) ?? -100
                showScale: true
                Accessible.name: qsTr("Level of %1").arg(zone.modelData.name)
            }
            RowLayout {
                Layout.fillWidth: true
                QQC2.Slider {
                    id: volume
                    Layout.fillWidth: true
                    from: bar.limits.min_db
                    to: bar.limits.max_db
                    stepSize: 0.5
                    value: zone.modelData.gain_db
                    Accessible.name: qsTr("Volume of %1").arg(zone.modelData.name)
                    onMoved: zone.set({ gain_db: value })
                    KanteSliderSkin { control: volume }
                }
                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 3.5
                    horizontalAlignment: Text.AlignRight
                    text: qsTr("%1 dB").arg(volume.value.toFixed(1))
                    font.family: KanteStyle.monoFamily
                }
            }
        }
    }
}
