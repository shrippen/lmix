import QtQuick
import QtTest
import QtQuick.Controls as QQC2
import "../../qml"

/**
 * The output buttons and their menu (OutputBar, OutputMenu): Start and
 * Stop… in the menu each send their own action, also when the state
 * moved since the menu opened; Settings… names the output.
 */
TestCase {
    id: test
    name: "OutputBar"
    when: windowShown
    width: 900
    height: 400

    property var sent: []
    property var settings: []
    property var state: "stopped"
    /** The output's send delay and start (STR.5). */
    property int delay: 0
    property real since: 0

    QtObject {
        id: fakeCore
        property bool connected: true
        property string links: ""
        function runningTime(since) { return "00:00:01" }
    }

    ListModel {
        id: keys
        ListElement { key: "o1" }
    }

    Component {
        id: barComponent
        OutputBar {
            core: fakeCore
            show: ({ outputs: [] })
            act: action => test.sent.push(action.action)
            outputs: keys
            outputById: id => ({ id: id, number: 1, name: "Stream", spec: { kind: "srt", uri: "srt://:9000", delay_s: test.delay },
                status: { state: test.state, since_ms: test.since } })
            confirmStop: false
            onSettingsRequested: outputId => test.settings.push(outputId)
        }
    }

    function init() {
        sent = []
        settings = []
        state = "stopped"
        delay = 0
        since = 0
    }

    function menuOf(bar) {
        const button = bar.children.find(c => c.modelData !== undefined)
        return button.children.find(c => c.stopRequested !== undefined)
            || button.data.find(c => c.stopRequested !== undefined)
    }

    // A stop from the menu went through the button's toggle: on an output
    // that had stopped meanwhile it started it again.
    function test_stop_never_starts() {
        const bar = createTemporaryObject(barComponent, test.Window.contentItem)
        const menu = menuOf(bar)
        menu.stopRequested()
        compare(sent.indexOf("start_output"), -1, "no start: " + sent)
        state = "running"
        sent = []
        menu.startRequested()
        compare(sent.indexOf("stop_output"), -1, "no stop: " + sent)
    }

    function test_each_request_sends_its_action() {
        const bar = createTemporaryObject(barComponent, test.Window.contentItem)
        const menu = menuOf(bar)
        menu.startRequested()
        state = "cards_off"
        menu.stopRequested()
        state = "running"
        menu.stopRequested()
        compare(sent, ["start_output", "stop_output", "stop_output"])
    }

    // REC.8: Mark shows while a recording runs and sends one mark.
    function test_mark_shows_while_a_recording_runs() {
        const recording = state => ({ outputs: [{ id: "o2", spec: { kind: "record", folder: "show" }, status: { state: state } }] })
        const bar = createTemporaryObject(barComponent, test.Window.contentItem, { show: recording("stopped") })
        const mark = findChild(bar, "markButton")
        verify(!mark.visible)
        bar.show = recording("running")
        verify(mark.visible)
        mark.clicked()
        compare(sent, ["mark_recording"])
    }

    // STR.5: a delayed stream counts down in place of its running time,
    // then runs; the tooltip names the delay.
    function test_a_delayed_stream_counts_down() {
        delay = 10
        since = Date.now() - 3000
        state = "running"
        const bar = createTemporaryObject(barComponent, test.Window.contentItem)
        const button = bar.children.find(c => c.modelData !== undefined)
        tryVerify(() => /^Stream  \u22120:0[67]$/.test(button.text), 1000, button.text)
        verify(button.QQC2.ToolTip.text.indexOf("Send delay 10 s, on air in ") >= 0, button.QQC2.ToolTip.text)
        since = Date.now() - 60000
        tryCompare(button, "text", "Stream  00:00:01", 2000)
    }

    function test_settings_names_the_output() {
        const bar = createTemporaryObject(barComponent, test.Window.contentItem)
        menuOf(bar).settingsRequested()
        compare(settings, ["o1"])
    }
}
