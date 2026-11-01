import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import Kante

/**
 * The guides over the monitors (OUT.12, SafeAreaGuides): a safe-area
 * frame, a crop, the thirds and the monitors they show on. The settings
 * decide what shows as chosen; a choice reports through `changed(key,
 * value)` with the keys "frame", "crop", "thirds" and "monitor".
 */
ColumnLayout {
    id: picker

    /** "", "ebu", "90" or "80" (SafeAreaGuides.frame). */
    property string frame: ""
    /** "", "4:3", "1:1" or "9:16" (SafeAreaGuides.crop). */
    property string crop: ""
    property bool thirds: false
    /** "preview", "program" or "both". */
    property string monitor: "preview"
    signal changed(string key, var value)

    readonly property var frames: ["", "ebu", "90", "80"]
    readonly property var crops: ["", "4:3", "1:1", "9:16"]
    readonly property var monitors: ["preview", "program", "both"]

    spacing: Kirigami.Units.smallSpacing

    KanteSectionLabel { text: qsTr("Guides") }
    KanteSegmented {
        objectName: "guideFrame"
        model: [qsTr("No frame"), qsTr("93/90 %"), qsTr("90 %"), qsTr("80 %")]
        tooltips: [qsTr("No safe-area frame"),
            qsTr("Action safe 93 % and title safe 90 %, dashed (EBU R 95, SMPTE)"),
            qsTr("A frame at 90 % of the picture"),
            qsTr("A frame at 80 % of the picture")]
        currentIndex: Math.max(0, picker.frames.indexOf(picker.frame))
        // The setting decides, not the click: the binding comes back.
        onActivated: index => {
            picker.changed("frame", picker.frames[index])
            currentIndex = Qt.binding(() => Math.max(0, picker.frames.indexOf(picker.frame)))
        }
    }
    KanteSegmented {
        objectName: "guideCrop"
        model: [qsTr("No crop"), "4:3", "1:1", "9:16"]
        tooltips: [qsTr("The whole picture"),
            qsTr("What a 4:3 picture keeps; the rest dimmed"),
            qsTr("What a square picture keeps; the rest dimmed"),
            qsTr("What a portrait picture keeps; the rest dimmed")]
        currentIndex: Math.max(0, picker.crops.indexOf(picker.crop))
        // The setting decides, not the click: the binding comes back.
        onActivated: index => {
            picker.changed("crop", picker.crops[index])
            currentIndex = Qt.binding(() => Math.max(0, picker.crops.indexOf(picker.crop)))
        }
    }
    KanteSwitch {
        objectName: "guideThirds"
        text: qsTr("Thirds")
        checked: picker.thirds
        onToggled: {
            picker.changed("thirds", checked)
            checked = Qt.binding(() => picker.thirds)
        }
    }
    KanteSegmented {
        objectName: "guideOn"
        model: [qsTr("Preview"), qsTr("Program"), qsTr("Both")]
        tooltips: [qsTr("Guides on the preview monitor"), qsTr("Guides on the program monitor"), qsTr("Guides on both monitors")]
        currentIndex: Math.max(0, picker.monitors.indexOf(picker.monitor))
        // The setting decides, not the click: the binding comes back.
        onActivated: index => {
            picker.changed("monitor", picker.monitors[index])
            currentIndex = Qt.binding(() => Math.max(0, picker.monitors.indexOf(picker.monitor)))
        }
    }
}
