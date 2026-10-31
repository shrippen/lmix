import QtQuick
import QtTest
import "../../qml"

/**
 * Detached pages (docs/PLAN-SEITEN.md, "Seiten lösbar"): which pages are
 * out and where their windows were, as AppShell keeps it in the settings
 * (PageArrangement), and the window a page goes to (PageWindow).
 */
TestCase {
    id: test
    name: "PageArrangement"
    when: windowShown
    width: 800
    height: 600

    readonly property var order: ["mix", "show", "audio", "outputs", "setup"]

    Component {
        id: arrangementComponent
        PageArrangement {}
    }

    function arrangement(text) {
        return createTemporaryObject(arrangementComponent, test, { text: text })
    }

    function test_a_broken_text_counts_as_no_page_out() {
        for (const text of ["", "nonsense", "[1, 2]", "null", "42"]) {
            const a = arrangement(text)
            compare(Object.keys(a.places).length, 0, text)
            verify(!a.isDetached("show"), text)
        }
    }

    function test_detaching_and_docking_keep_the_place() {
        const a = arrangement("{}")
        a.text = a.changed("show", true, { x: 2000, y: 40, width: 1280, height: 720, screen: "DP-2", visibility: "normal" })
        verify(a.isDetached("show"))
        verify(!a.isDetached("mix"))
        // Docked: out no more, but the place stays for the next time.
        a.text = a.changed("show", false, {})
        verify(!a.isDetached("show"))
        compare(a.places.show.x, 2000)
        compare(a.places.show.screen, "DP-2")
        a.text = a.changed("show", true, {})
        verify(a.isDetached("show"))
        compare(a.places.show.width, 1280)
    }

    function test_the_main_window_keeps_one_page() {
        const a = arrangement("{}")
        verify(a.canDetach(order, "mix"))
        verify(!a.canDetach(order, "light"), "no such page")
        for (const id of ["mix", "show", "audio", "outputs"]) {
            a.text = a.changed(id, true, {})
        }
        verify(!a.canDetach(order, "mix"), "already out")
        verify(!a.canDetach(order, "setup"), "the last one here")
    }

    function test_the_next_docked_page_follows_the_one_that_goes() {
        const a = arrangement("{}")
        compare(a.dockedAfter(order, "mix"), "show")
        compare(a.dockedAfter(order, "setup"), "mix", "round to the first")
        a.text = a.changed("show", true, {})
        compare(a.dockedAfter(order, "mix"), "audio", "over a detached one")
        for (const id of ["audio", "outputs", "setup"]) {
            a.text = a.changed(id, true, {})
        }
        compare(a.dockedAfter(order, "mix"), "", "none left")
    }

    readonly property var screens: [
        { name: "HDMI-1", virtualX: 0, virtualY: 0, width: 1920, height: 1080 },
        { name: "DP-2", virtualX: 1920, virtualY: 0, width: 2560, height: 1440 }
    ]
    readonly property var size: ({ width: 1600, height: 1000 })

    function test_a_window_comes_back_on_its_screen() {
        const a = arrangement("{}")
        const at = a.fit({ screen: "DP-2", x: 2100, y: 100, width: 1280, height: 720 }, screens, screens[0], size)
        compare(at.screen.name, "DP-2")
        compare(at.x, 2100)
        compare(at.y, 100)
        compare(at.width, 1280)
        compare(at.height, 720)
        compare(at.visibility, "normal")
    }

    function test_a_window_off_its_screen_moves_back_in() {
        const a = arrangement("{}")
        const at = a.fit({ screen: "DP-2", x: 4400, y: 1300, width: 1280, height: 720 }, screens, screens[0], size)
        compare(at.x, 1920 + 2560 - 1280)
        compare(at.y, 1440 - 720)
    }

    function test_a_window_of_a_screen_gone_goes_to_the_fallback() {
        const a = arrangement("{}")
        const at = a.fit({ screen: "DP-9", x: 5000, y: 100, width: 2400, height: 720, visibility: "maximized" },
                         screens, screens[0], size)
        compare(at.screen.name, "HDMI-1")
        compare(at.width, 1920, "at most as wide as the screen")
        compare(at.x, 0, "centred")
        compare(at.y, Math.round((1080 - 720) / 2))
        compare(at.visibility, "maximized")
    }

    function test_the_first_time_on_the_other_screen() {
        const a = arrangement("{}")
        compare(a.otherScreen(screens, screens[0]).name, "DP-2")
        compare(a.otherScreen(screens, screens[1]).name, "HDMI-1")
        compare(a.otherScreen([screens[0]], screens[0]).name, "HDMI-1", "one screen: that one")
        const at = a.fit({}, screens, a.otherScreen(screens, screens[0]), size)
        compare(at.screen.name, "DP-2")
        compare(at.width, 1600)
        compare(at.x, 1920 + Math.round((2560 - 1600) / 2))
    }

    function test_the_place_of_a_window() {
        const a = arrangement("{}")
        const win = { x: 10, y: 20, width: 800, height: 600, screen: { name: "DP-2" } }
        const normal = a.placeOf(win, Window.Windowed)
        compare(normal.visibility, "normal")
        compare(normal.screen, "DP-2")
        compare(normal.width, 800)
        // Maximized: the normal geometry from before stays.
        const big = a.placeOf(win, Window.Maximized)
        compare(big.visibility, "maximized")
        compare(big.x, undefined)
        compare(a.placeOf(win, Window.FullScreen).visibility, "fullscreen")
    }

    // --- the window ---

    QtObject {
        id: fakeShell
        property bool quitting: false
        property var tilesAtlas: null
        property bool recording: false
        property bool streaming: false
        property var show: null
        property var backend: QtObject {
            property bool connected: true
            property string message: ""
            signal answered(string action, string refusal)
        }
        property int panics: 0
        function panicAll() { panics += 1 }
        function act(action) {}
        function keyTaken(sequence) { return false }
        function openKeys(page) {}
    }

    Component {
        id: windowComponent
        PageWindow {
            shell: fakeShell
            pageId: "show"
        }
    }
    Component {
        id: pageComponent
        AppPage {
            pageId: "show"
            title: "Show"
            panicText: "Stop show"
            property int pagePanics: 0
            onPanicRequested: pagePanics += 1
        }
    }
    SignalSpy {
        id: dockSpy
        signalName: "dockRequested"
    }

    function makeWindow() {
        const win = createTemporaryObject(windowComponent, null)
        const page = createTemporaryObject(pageComponent, win.pageArea)
        win.page = page
        win.show()
        return win
    }

    function findButton(item, text) {
        if (item.text === text && item.clicked !== undefined) {
            return item
        }
        for (let i = 0; i < item.children.length; i++) {
            const found = findButton(item.children[i], text)
            if (found) {
                return found
            }
        }
        return null
    }

    function test_the_window_shows_the_page_and_its_panics() {
        const win = makeWindow()
        compare(win.title, "lmix — Show")
        compare(win.page.parent, win.pageArea)
        compare(win.page.width, win.pageArea.width, "the page fills the window")
        compare(win.tilesAtlas, fakeShell.tilesAtlas, "the shell's tile picture, not its own")
        const pagePanic = findButton(win.contentItem, "Stop show")
        verify(pagePanic !== null)
        pagePanic.clicked()
        compare(win.page.pagePanics, 1)
        const all = findButton(win.contentItem, "Panic")
        verify(all !== null)
        all.clicked()
        compare(fakeShell.panics, 1)
        win.close()
    }

    function test_closing_the_window_brings_the_page_back() {
        const win = makeWindow()
        dockSpy.target = win
        dockSpy.clear()
        win.close()
        compare(dockSpy.count, 1)
        verify(win.visible, "the shell takes it away, not the close")
        findButton(win.contentItem, "Back to the main window").clicked()
        compare(dockSpy.count, 2)
        // lmix quits: the page stays out for the next start.
        fakeShell.quitting = true
        win.close()
        compare(dockSpy.count, 2)
        verify(!win.visible)
        fakeShell.quitting = false
    }

    function test_messages_only_in_the_window_in_front() {
        const win = makeWindow()
        compare(win.toasts.listening, win.active)
        win.close()
    }
}
