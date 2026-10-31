import QtQuick
import org.lmix.console

/**
 * lmix: the window and its pages, in the order of the page bar
 * (docs/PLAN-SEITEN.md). The shell makes each page, in its stack or in a
 * window of its own (a detached page).
 */
AppShell {
    id: shell

    pageEntries: [
        AppShell.PageEntry {
            pageId: "mix"
            page: MixPage {}
        },
        AppShell.PageEntry {
            pageId: "show"
            page: ShowPage {}
        },
        AppShell.PageEntry {
            pageId: "audio"
            page: AudioPage {}
        },
        AppShell.PageEntry {
            pageId: "outputs"
            page: OutputsPage {}
        },
        AppShell.PageEntry {
            pageId: "setup"
            page: SetupPage {}
        }
    ]
}
