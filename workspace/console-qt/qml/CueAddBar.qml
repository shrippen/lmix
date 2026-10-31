import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import Kante

/**
 * Adds a cue of each kind (PLAN-SEITEN, click dummy): after the chosen
 * cue, inside its group too, or at the end of the list when standby is
 * past it; the inspector then shows the new cue. What a kind cannot do
 * without is asked first (a file, the actions) or taken from the chosen
 * cue (what a fade fades, what a start starts). Edit mode only, not for a
 * list from a script.
 *
 *   ADD  [SOUND] [PICTURE] [FADE] [ACTIONS] [WAIT] [NOTE] [START]
 */
RowLayout {
    id: bar

    /** The ShowView: list, chosen cue, show, addCue(). */
    required property var showView

    /** What a new wait waits and a new fade takes. */
    readonly property int newWaitMs: 5000
    readonly property int newFadeMs: 5000
    /** A new fade fades to silence (SOUND_GAIN_MIN_DB) or clear. */
    readonly property real silentDb: -60

    readonly property var chosen: showView.selectedCue
    readonly property var zones: showView.show && showView.show.zones ? showView.show.zones : []
    readonly property var stages: showView.show && showView.show.stages ? showView.show.stages : []
    /** Id of the cue the new one goes after, taken when a dialog opens:
     *  standby may move meanwhile. Empty: at the end. */
    property string after: ""

    /** A cue as other cues name it: its number, else its id. */
    function ref(cue) {
        return cue.number !== "" ? cue.number : cue.id
    }

    function isMedia(cue) {
        return cue !== null && (cue.kind === "sound" || cue.kind === "picture")
    }

    /** What a new fade fades: the chosen sound or picture to silence or
     *  clear, then stopped; else the first zone to silence, else the first
     *  stage to black; null when the show has none of them. */
    function fadeFor(cue) {
        if (isMedia(cue)) {
            var out = { target: { cue: ref(cue) }, then: "stop" }
            if (cue.kind === "picture") {
                out.opacity = 0
            } else {
                out.gain_db = silentDb
            }
            return out
        }
        if (zones.length > 0) {
            return { target: { zone: zones[0].name }, gain_db: silentDb }
        }
        if (stages.length > 0) {
            return { target: { stage: stages[0].name }, brightness: 0 }
        }
        return null
    }

    /** Notes where the new cue goes, then opens `dialog`. */
    function ask(dialog) {
        after = chosen !== null ? chosen.id : ""
        dialog.open()
    }

    /** A new cue of `kind` with `fields`, after `at` (a cue id) or at the
     *  end. */
    function add(kind, fields, at) {
        showView.addCue(Object.assign({ kind: kind }, fields), at)
        showView.forceActiveFocus()
    }

    function here() {
        return chosen !== null ? chosen.id : ""
    }

    function localPath(url) {
        return decodeURIComponent(url.toString().replace(/^file:\/\//, ""))
    }

    spacing: Kirigami.Units.smallSpacing

    KanteSectionLabel { text: qsTr("Add") }
    KanteButton {
        focusPolicy: Qt.NoFocus
        size: KanteButton.Size.Small
        text: qsTr("Sound")
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.text: qsTr("Plays a sound file into the zones")
        onClicked: bar.ask(soundDialog)
    }
    KanteButton {
        focusPolicy: Qt.NoFocus
        size: KanteButton.Size.Small
        enabled: bar.stages.length > 0
        text: qsTr("Picture")
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.text: bar.stages.length > 0 ? qsTr("Shows a video or still on stage %1").arg(bar.stages[0].name)
                                                 : qsTr("The show has no stage yet")
        onClicked: bar.ask(pictureDialog)
    }
    KanteButton {
        focusPolicy: Qt.NoFocus
        size: KanteButton.Size.Small
        enabled: bar.fadeFor(bar.chosen) !== null
        text: qsTr("Fade")
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.text: bar.isMedia(bar.chosen) ? qsTr("Fades out cue %1 and stops it").arg(bar.ref(bar.chosen))
                         : bar.zones.length > 0 ? qsTr("Fades zone %1 to silence").arg(bar.zones[0].name)
                         : bar.stages.length > 0 ? qsTr("Fades stage %1 to black").arg(bar.stages[0].name)
                         : qsTr("The show has nothing to fade: no sound or picture chosen, no zone, no stage")
        onClicked: bar.add("fade", { duration_ms: bar.newFadeMs, fade: bar.fadeFor(bar.chosen) }, bar.here())
    }
    KanteButton {
        focusPolicy: Qt.NoFocus
        size: KanteButton.Size.Small
        text: qsTr("Actions")
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.text: qsTr("Runs actions of the catalogue, e.g. a cut or a title")
        onClicked: {
            actionDialog.actions = []
            bar.ask(actionDialog)
        }
    }
    KanteButton {
        focusPolicy: Qt.NoFocus
        size: KanteButton.Size.Small
        text: qsTr("Wait")
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.text: qsTr("Only runs for a while, e.g. before the next cue follows")
        onClicked: bar.add("wait", { duration_ms: bar.newWaitMs }, bar.here())
    }
    KanteButton {
        focusPolicy: Qt.NoFocus
        size: KanteButton.Size.Small
        text: qsTr("Note")
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.text: qsTr("Does nothing: a stage direction or a section mark")
        onClicked: bar.add("note", {}, bar.here())
    }
    KanteButton {
        focusPolicy: Qt.NoFocus
        size: KanteButton.Size.Small
        enabled: bar.chosen !== null
        text: qsTr("Start")
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.text: bar.chosen !== null ? qsTr("Starts cue %1 without moving standby").arg(bar.ref(bar.chosen))
                                               : qsTr("Choose the cue it starts first")
        onClicked: bar.add("start", { target: { cue: bar.ref(bar.chosen) } }, bar.here())
    }

    FileDialog {
        id: soundDialog
        title: qsTr("Sound file")
        nameFilters: [qsTr("Sound files (*.wav *.flac *.ogg *.opus *.mp3 *.aif *.aiff *.m4a)"), qsTr("All files (*)")]
        onAccepted: bar.add("sound", { sound: { file: bar.localPath(selectedFile) } }, bar.after)
        onRejected: bar.showView.forceActiveFocus()
    }
    FileDialog {
        id: pictureDialog
        title: qsTr("Video or still")
        nameFilters: [qsTr("Videos and stills (*.mp4 *.mov *.mkv *.webm *.png *.jpg *.jpeg)"), qsTr("All files (*)")]
        onAccepted: bar.add("picture", { picture: { file: bar.localPath(selectedFile), stage: bar.stages[0].name } }, bar.after)
        onRejected: bar.showView.forceActiveFocus()
    }
    ActionListDialog {
        id: actionDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        inputs: bar.showView.show ? bar.showView.show.inputs : []
        outputs: bar.showView.show ? bar.showView.show.outputs : []
        needsAction: true
        title: qsTr("Actions of the new cue")
        onDone: (label, actions) => bar.add("actions", { actions: actions }, bar.after)
        onClosed: bar.showView.forceActiveFocus()
    }
}
