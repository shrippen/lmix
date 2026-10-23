import QtQuick
import org.lmix.console

/**
 * lmix: the window and its pages, in the order of the page bar
 * (docs/PLAN-SEITEN.md).
 */
AppShell {
    id: shell

    MixPage {
        shell: shell
    }
    ShowPage {
        shell: shell
    }
}
