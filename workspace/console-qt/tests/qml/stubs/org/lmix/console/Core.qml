import QtQuick

// Stand-in for the Console's Rust backend (src/bridge.rs, `Core`) in the
// QML tests of the whole window (AppShell): the same properties, signals
// and functions, no connection. `acted` keeps what was sent.
QtObject {
    property bool connected: true
    property string message: ""
    property int style: -1
    property int levelsTick: 0
    property string positions: ""
    property string links: ""
    property string machine: ""
    property string tiles: ""
    property string devices: ""
    property string recent: ""
    property string templates: ""
    property string coreUrl: "http://127.0.0.1:8200"
    property string windowMode: "normal"
    property string uiMode: ""
    property string startPage: ""
    property string latency: ""
    property bool measuring: false
    property string globalKeys: ""
    property bool globalHeld: false

    property var acted: []
    /** Live pictures asked for: `{ item, source }`, released ones gone. */
    property var live: []

    signal statePatched(string patch)
    signal globalShortcut(string id)
    signal answered(string action, string refusal)
    signal midiReceived(string message)

    function act(action) {
        acted = acted.concat([JSON.parse(action)])
        return true
    }
    function setGlobalShortcuts(json) {}
    function addInput(kind, a, b, c, d) { return true }
    function replaceSource(id, kind, a, b, c) { return true }
    function addOutput(form) { return true }
    function refreshLists() {}
    function measureLatency() {}
    function choices(list) { return "[]" }
    function runningTime(since) { return "" }
    function shortName(name, max) { return String(name).slice(0, max) }
    function peak(key) { return NaN }
    function channel(input, index) { return NaN }
    function showLive(item, source) {
        live = live.filter(l => l.item !== item).concat([{ item: item, source: source }])
    }
    function releaseLive(item) {
        live = live.filter(l => l.item !== item)
    }
}
