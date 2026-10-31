import QtQuick
import QtQuick.Controls as QQC2
import QtTest
import Kante
import "../../qml"

/**
 * The window with pages (AppShell) and a page in a window of its own
 * (docs/PLAN-SEITEN.md, "Seiten lösbar"): the page goes into a PageWindow
 * and comes back, made anew each time (one at a time, so no picture runs
 * twice); the main window keeps one page; Ctrl+n and the bar bring a
 * detached page's window to the front; the place stays in the settings
 * (the start from them is in tst_page_arrangement: fit). With a stand-in
 * Core (tests/qml/stubs).
 */
TestCase {
    id: test
    name: "AppShell"
    when: windowShown
    width: 400
    height: 300

    Component {
        id: shellComponent
        AppShell {
            id: shell
            width: 1200
            height: 800
            pageEntries: [
                AppShell.PageEntry {
                    pageId: "mix"
                    page: AppPage {
                        required property var shell
                        pageId: "mix"
                        title: "Mix"
                        hasProgram: true
                        panicText: "FTB"
                        fileItems: [
                            QQC2.MenuItem {
                                text: "Inputs from a show…"
                            }
                        ]
                        tools: [
                            QQC2.Button {
                                objectName: "mixTool"
                                text: "Latency"
                            }
                        ]
                        // A live picture, as the Mix page has: counted by the Core stand-in.
                        LiveView {
                            objectName: "mixProgram"
                            anchors.fill: parent
                            core: shell.backend
                            source: "program"
                        }
                    }
                },
                AppShell.PageEntry {
                    pageId: "show"
                    page: AppPage {
                        required property var shell
                        pageId: "show"
                        title: "Show"
                        panicText: "Stop show"
                        // A page's own dialog, as the real pages have: it goes
                        // with the page (and must not take the window down).
                        KanteDialog {
                            objectName: "showDialog"
                            parent: QQC2.Overlay.overlay
                            title: "Cue lists"
                        }
                    }
                },
                AppShell.PageEntry {
                    pageId: "setup"
                    page: AppPage {
                        required property var shell
                        pageId: "setup"
                        title: "Setup"
                    }
                }
            ]
        }
    }

    property var shell: null

    function init() {
        shell = createTemporaryObject(shellComponent, null)
        shell.visible = true
        // A clean start: every page in the main window.
        for (const id of shell.pageOrder) {
            if (shell.isDetached(id)) {
                shell.dockPage(id)
            }
        }
        shell.showPage("mix")
        tryCompare(shell, "currentPage", "mix")
    }

    function cleanup() {
        if (!shell) {
            return
        }
        for (const id of Object.keys(shell.pageWindows)) {
            shell.dockPage(id)
        }
        shell.quitting = true
        shell.close()
        shell = null
    }

    function page(id) {
        return shell.pageItems[shell.pageOrder.indexOf(id)]
    }

    function find(item, name) {
        if (item.objectName === name) {
            return item
        }
        for (let i = 0; i < item.children.length; i++) {
            const found = find(item.children[i], name)
            if (found) {
                return found
            }
        }
        return null
    }

    function test_a_page_goes_to_its_own_window_and_comes_back() {
        const docked = page("show")
        shell.showPage("show")
        shell.detachPage("show")
        verify(shell.isDetached("show"))
        const win = shell.pageWindows["show"]
        verify(win !== undefined)
        tryCompare(win, "visible", true)
        compare(win.page, page("show"), "the page is made anew in the window")
        verify(page("show") !== docked)
        compare(win.page.Window.window, win)
        compare(shell.currentPage, "setup", "the main window shows the next page here")
        compare(shell.pageItems.length, 3)

        win.dockRequested()
        verify(!shell.isDetached("show"))
        compare(shell.currentPage, "show", "back and shown")
        compare(page("show").Window.window, shell)
        tryVerify(() => !win || !win.visible, 1000, "its window goes")
    }

    function test_the_main_window_keeps_one_page() {
        shell.detachPage("mix")
        shell.detachPage("show")
        verify(shell.isDetached("mix") && shell.isDetached("show"))
        shell.detachPage("setup")
        verify(!shell.isDetached("setup"), "the last page stays")
        compare(shell.currentPage, "setup")
        compare(shell.currentItem, page("setup"))
    }

    function test_a_live_picture_runs_once() {
        const before = shell.backend.live.filter(l => l.source === "program").length
        shell.detachPage("mix")
        const win = shell.pageWindows["mix"]
        verify(win !== undefined)
        // The old page's picture goes with it (deferred), the new one asks.
        tryVerify(() => shell.backend.live.filter(l => l.source === "program").length <= Math.max(1, before))
        compare(find(win.contentItem, "mixProgram").Window.window, win)
    }

    function test_tools_and_file_entries_follow_the_page() {
        shell.detachPage("mix")
        const win = shell.pageWindows["mix"]
        const tool = find(win.contentItem, "mixTool")
        verify(tool !== null, "the page's tools in its window's header")
        shell.dockPage("mix")
        shell.showPage("mix")
        verify(find(shell.contentItem, "mixTool") !== null, "and back in the main window's header")
    }

    function test_ctrl_n_and_the_bar_bring_the_window_to_the_front() {
        shell.detachPage("show")
        const win = shell.pageWindows["show"]
        shell.showPage("mix")
        compare(shell.currentPage, "mix")
        // Ctrl+2: the Show page's window, the main window keeps Mix.
        shell.showPage("show")
        compare(shell.currentPage, "mix")
        verify(shell.isDetached("show"))
    }

    function test_the_place_stays_for_the_next_time() {
        shell.detachPage("show")
        const win = shell.pageWindows["show"]
        win.width = 700
        win.height = 500
        win.placed()
        const place = shell.pageArrangement.places.show
        verify(place.detached)
        compare(place.width, 700)
        compare(place.height, 500)
        compare(place.visibility, "normal")
        // Back in the main window: the place stays for the next detaching.
        shell.dockPage("show")
        verify(!shell.pageArrangement.places.show.detached)
        shell.detachPage("show")
        const again = shell.pageWindows["show"]
        tryCompare(again, "visible", true)
        compare(again.width, 700)
        compare(again.height, 500)
        // lmix quits with the page out: it stays out for the next start.
        shell.quitting = true
        shell.close()
        verify(shell.pageArrangement.places.show.detached)
    }

    // A window torn down with a Kante dialog in it crashed (todo.md, Kante):
    // docking releases it, quitting empties it first.
    function test_a_window_goes_down_cleanly() {
        shell.detachPage("show")
        shell.detachPage("mix")
        const show = shell.pageWindows["show"]
        const mix = shell.pageWindows["mix"]
        shell.dockPage("show")
        wait(200)
        // lmix quits with Mix out: the closed window goes (Qt deletes it,
        // or the engine at the end), its page and dialogs went first.
        shell.close()
        verify(shell.quitting)
        verify(mix.page === null, "the page went first")
        tryVerify(() => !mix || !mix.visible)
        wait(200)
    }

    function test_the_small_program_only_without_one_elsewhere() {
        shell.showPage("show")
        verify(!shell.programElsewhere)
        shell.detachPage("mix")
        tryCompare(shell, "programElsewhere", true)
        shell.dockPage("mix")
        verify(!shell.programElsewhere)
    }
}
