import QtQuick
import QtQuick.Controls as QQC2
import Kante

/**
 * Peak level in dBFS as a bar: green up to -18, yellow to -6, red above
 * (the range a broadcast mix keeps to). The tooltip names the value.
 */
KanteProgressBar {
    id: meter

    /** Peak in dBFS; undefined while nothing is measured. */
    property var peakDb: undefined

    readonly property real floorDb: -60
    readonly property real level: peakDb === undefined ? 0 : Math.max(0, Math.min(1, (peakDb - floorDb) / -floorDb))
    readonly property real greenTop: (-18 - floorDb) / -floorDb
    readonly property real yellowTop: (-6 - floorDb) / -floorDb

    parts: [
        { value: Math.min(level, greenTop), color: KanteStyle.positiveTextColor },
        { value: Math.max(0, Math.min(level, yellowTop) - greenTop), color: KanteStyle.warningColor },
        { value: Math.max(0, level - yellowTop), color: KanteStyle.negativeTextColor }
    ]

    Accessible.role: Accessible.ProgressBar
    Accessible.name: QQC2.ToolTip.text

    HoverHandler { id: hover }
    QQC2.ToolTip.visible: hover.hovered
    QQC2.ToolTip.text: peakDb === undefined ? qsTr("No sound measured")
        : qsTr("Peak %1 dBFS (green to −18, yellow to −6, red above)").arg(peakDb.toFixed(1))
}
