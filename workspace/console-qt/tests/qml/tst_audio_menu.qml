import QtQuick
import QtQuick.Controls as QQC2
import QtTest
import Kante
import "../../qml"

/**
 * The menu of a strip in the audio column (NarrowMixer, docs/DESIGN-
 * DIALOGE.md P02 A): one width for every strip, switches, Remove as red
 * text in the foot, asked first.
 */
TestCase {
    id: test
    name: "AudioMenu"
    when: windowShown
    width: 1000
    height: 800

    property var removals: []
    property var sent: []

    QtObject {
        id: fakeCore
        property bool connected: true
        function choices(kind) {
            return kind === "bus_limits" ? JSON.stringify({ buses: 8, name_max: 40 }) : "[]"
        }
        function shortName(name, chars) {
            return name.slice(0, chars)
        }
    }

    readonly property var show: ({
        inputs: [{ id: "cam", number: 1, name: "Cam", has_audio: true, listen: "off",
                   audio: { on: true, volume: 80, follow_video: true }, sends: { b1: 40 },
                   phones: { on: false, level: 100 } }],
        buses: [{ id: "b1", name: "Stream", volume: 100, mute: false, listen: false, pre_fader: false }],
        outputs: [{ id: "o1", name: "CDN", spec: { kind: "srt" }, bus: "b1" },
                  { id: "o2", name: "Wall", spec: { kind: "display" }, bus: null }],
        master: { volume: 100, mute: false },
        phones: { volume: 100, mute: false, pre_fader: false }
    })

    Component {
        id: mixerComponent
        NarrowMixer {
            width: 300
            height: 600
            core: fakeCore
            show: test.show
            act: action => test.sent.push(action)
            askRemove: (kind, item) => test.removals.push([kind, item.id])
        }
    }

    function init() {
        removals = []
        sent = []
    }

    function find(item, test) {
        if (!item) {
            return null
        }
        if (test(item)) {
            return item
        }
        const children = item.children || []
        for (let i = 0; i < children.length; i++) {
            const found = find(children[i], test)
            if (found) {
                return found
            }
        }
        return null
    }
    function menuOf(mixer) {
        for (let i = 0; i < mixer.data.length; i++) {
            if (mixer.data[i].openFor !== undefined) {
                return mixer.data[i]
            }
        }
        return null
    }
    function shown(item) {
        for (let at = item; at; at = at.parent) {
            if (!at.visible) {
                return false
            }
        }
        return true
    }
    function button(menu, text) {
        return find(menu.contentItem, i => i.clicked !== undefined && i.text === text && shown(i))
    }

    function test_a_bus_menu() {
        const mixer = createTemporaryObject(mixerComponent, test.Window.contentItem)
        const menu = menuOf(mixer)
        mixer.openMenu("b1", mixer)
        tryCompare(menu, "opened", true)
        const busWidth = menu.width

        // The output that takes its sound: a switch, on; a screen has none.
        const cdn = find(menu.contentItem, i => i.text === "CDN" && i.toggled !== undefined)
        verify(cdn instanceof KanteSwitch, "CDN is a KanteSwitch")
        compare(cdn.checked, true)
        verify(!find(menu.contentItem, i => i.text === "Wall" && i.toggled !== undefined))

        // Remove: red text, not a red block; it asks first.
        const remove = button(menu, "Remove bus")
        verify(remove, "Remove bus")
        compare(remove.emphasis, KanteButton.Emphasis.Quiet)
        compare(remove.destructive, true)
        mouseClick(remove)
        tryCompare(menu, "opened", false)
        compare(removals, [["bus", "b1"]])
        compare(sent, [])

        // An input's menu is as wide: the menu does not jump. (Opened at
        // once at the same item, the popover takes it for the closing click.)
        wait(Qt.styleHints.mouseDoubleClickInterval + 50)
        mixer.openMenu("cam", mixer)
        tryCompare(menu, "opened", true)
        compare(menu.width, busWidth)
        verify(button(menu, "Channel strip…"), "Channel strip…")
        verify(!button(menu, "Remove bus"), "no Remove for an input")
        const follow = find(menu.contentItem, i => i.text === "Sound follows picture")
        verify(follow instanceof KanteSwitch, "Sound follows picture is a KanteSwitch")
    }
}
