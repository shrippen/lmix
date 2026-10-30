import QtQuick
import QtTest
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
            outputById: id => ({ id: id, number: 1, name: "Stream", spec: { kind: "srt", uri: "srt://:9000" },
                status: { state: test.state, since_ms: 0 } })
            confirmStop: false
            onSettingsRequested: outputId => test.settings.push(outputId)
        }
    }

    function init() {
        sent = []
        settings = []
        state = "stopped"
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

    function test_settings_names_the_output() {
        const bar = createTemporaryObject(barComponent, test.Window.contentItem)
        menuOf(bar).settingsRequested()
        compare(settings, ["o1"])
    }
}
