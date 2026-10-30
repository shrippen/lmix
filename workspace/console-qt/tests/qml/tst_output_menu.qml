import QtQuick
import QtTest
import "../../qml"

/**
 * An output's menu (OutputMenu, docs/DESIGN-DIALOGE.md M07 B): head with
 * name, kind and bitrate while it sends; Stop…, Settings…, Copy address,
 * Statistics; Remove locked while it sends, with the reason.
 */
TestCase {
    id: test
    name: "OutputMenu"
    when: windowShown
    width: 800
    height: 600

    property var requests: []

    readonly property var stream: ({ id: "o1", number: 1, name: "Sendehaus",
        spec: { kind: "srt", uri: "srt://example.org:9921?streamid=secret" }, status: { state: "running", since_ms: 0 } })
    readonly property var stats: ({ output: "o1", kbps: 6000,
        srt: { connected: true, packets_lost: 3, packets_dropped: 0, rtt_ms: 12 } })
    readonly property var rtmp: ({ id: "o2", number: 2, name: "Plattform",
        spec: { kind: "rtmp", url: "rtmp://live.example/app", key: "key", video_kbps: 4500 },
        status: { state: "running", since_ms: 0 } })

    Component {
        id: menuComponent
        OutputMenu {
            output: test.stream
            link: test.stats
            elapsed: "00:01:05"
            onStartRequested: test.requests.push("start")
            onStopRequested: test.requests.push("stop")
            onSettingsRequested: test.requests.push("settings")
            onRemoveRequested: test.requests.push("remove")
        }
    }

    function init() {
        requests = []
    }

    function find(menu, test) {
        for (let i = 0; i < menu.count; i++) {
            if (test(menu.itemAt(i))) {
                return menu.itemAt(i)
            }
        }
        return null
    }
    function named(menu, name) {
        return find(menu, item => item.objectName === name)
    }
    function entry(menu, text) {
        return find(menu, item => item.text === text && item.triggered !== undefined)
    }
    function rateLabel(menu) {
        const head = named(menu, "head")
        return head.children.find(c => c.objectName === "rate")
    }

    function test_head_shows_the_rate_while_it_sends() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        menu.open()
        tryCompare(menu, "opened", true)
        compare(menu.rate, Texts.linkRate(stats))
        verify(rateLabel(menu).visible)
        menu.output = Object.assign({}, stream, { status: { state: "stopped" } })
        compare(menu.rate, "")
        verify(!rateLabel(menu).visible)
        // Without a link: the rate that is set.
        menu.link = undefined
        menu.output = Object.assign({}, stream, { spec: { kind: "rtmp", url: "rtmp://live.example/app/key", video_kbps: 4500 } })
        compare(menu.rate, "4500 kbit/s")
    }

    // RTMP and recordings: the measured rate in the head, not the one set,
    // and no SRT rows under Statistics.
    function test_rtmp_and_recordings_show_the_measured_rate() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        menu.output = rtmp
        menu.link = { output: "o2", kbps: 4730 }
        compare(menu.rate, Texts.linkRate(menu.link))
        compare(menu.rate, qsTr("%1 Mbit/s").arg(Number(4.7).toLocaleString(Qt.locale(), "f", 1)))
        const stats = entry(menu, "Statistics")
        menu.open()
        stats.subMenu.open()
        tryCompare(stats.subMenu, "opened", true)
        const rows = []
        for (let i = 0; i < stats.subMenu.count; i++) {
            const item = stats.subMenu.itemAt(i)
            if (item.valueText !== undefined && item.visible) {
                rows.push(item.text + "=" + item.valueText)
            }
        }
        compare(rows, ["Running=00:01:05", "Rate=" + menu.rate])
        // An SRT output nobody receives says so.
        menu.link = { output: "o1", kbps: 0, srt: { connected: false, packets_lost: 0, packets_dropped: 0, rtt_ms: null } }
        compare(menu.rate, "no receiver")
        // The tooltip of an RTMP or recording output is its rate alone.
        compare(Texts.linkDetail({ output: "o2", kbps: 4730 }), Texts.linkRate({ output: "o2", kbps: 4730 }))
    }

    function test_remove_is_locked_while_it_sends() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        menu.open()
        tryCompare(menu, "opened", true)
        const remove = named(menu, "remove")
        compare(remove.enabled, false)
        compare(remove.destructive, true)
        verify(named(menu, "removeHint").visible, "says why")
        entry(menu, "Stop…").triggered()
        compare(requests, ["stop"])

        menu.output = Object.assign({}, stream, { status: { state: "stopped" } })
        compare(remove.enabled, true)
        verify(!named(menu, "removeHint").visible)
        // Hidden, it takes no room: an empty line stood under Remove….
        compare(named(menu, "removeHint").height, 0)
        remove.triggered()
        entry(menu, "Start").triggered()
        entry(menu, "Settings…").triggered()
        compare(requests, ["stop", "remove", "start", "settings"])
        // Remove comes last.
        let last = null
        for (let i = 0; i < menu.count; i++) {
            if (menu.itemAt(i).triggered !== undefined) {
                last = menu.itemAt(i)
            }
        }
        verify(last === remove)
    }

    // A stream key in the address stays out of the menu; the copy is whole.
    function test_the_address_is_short_in_the_menu() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        const copy = named(menu, "copyAddress")
        compare(copy.valueText, "srt://example.org:9921")
        compare(menu.address, "srt://example.org:9921?streamid=secret")
        menu.output = Object.assign({}, stream, { spec: { kind: "record", folder: "/home/me/Videos/Show" } })
        compare(copy.valueText, "Show")
        menu.output = Object.assign({}, stream, { spec: { kind: "display", mode: null } })
        compare(copy.visible, false)
    }

    function test_statistics_while_it_sends() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        const stats = entry(menu, "Statistics")
        verify(stats && stats.subMenu)
        compare(stats.enabled, true)
        menu.open()
        stats.subMenu.open()
        tryCompare(stats.subMenu, "opened", true)
        const rows = []
        for (let i = 0; i < stats.subMenu.count; i++) {
            const item = stats.subMenu.itemAt(i)
            if (item.valueText !== undefined && item.visible) {
                rows.push(item.text + "=" + item.valueText)
            }
        }
        compare(rows, ["Running=00:01:05", "Rate=" + Texts.linkRate(test.stats), "Packets lost=3", "Packets dropped=0", "Round trip=12 ms"])
        menu.output = Object.assign({}, stream, { status: { state: "stopped" } })
        compare(stats.enabled, false)
    }
}
