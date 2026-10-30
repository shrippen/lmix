import QtQuick
import QtQuick.Controls as QQC2
import QtTest
import Kante
import "../../qml"

/**
 * Every question before something goes or stops looks alike
 * (docs/DESIGN-DIALOGE.md rule 1, F02, F12, F14): Kante's destructive
 * dialog in size S, the red button naming verb and object.
 */
TestCase {
    id: test
    name: "Confirmations"
    when: windowShown
    width: 1200
    height: 800

    QtObject {
        id: fakeCore
        signal answered(string action, string refusal)
    }
    Item {
        id: toastColumn
    }

    Component {
        id: removalsComponent
        Removals {
            core: fakeCore
            act: action => {}
            show: null
            column: toastColumn
        }
    }
    Component {
        id: resetComponent
        FactoryResetDialog {
            act: action => {}
        }
    }
    Component {
        id: stopComponent
        StopOutputDialog {
            act: action => {}
        }
    }

    function removalsDialog(removals) {
        for (let i = 0; i < removals.data.length; i++) {
            if (removals.data[i].request !== undefined) {
                return removals.data[i]
            }
        }
        return null
    }

    function checkDestructive(dialog, accept) {
        compare(dialog.destructive, true, dialog.title)
        compare(dialog.size, KanteDialog.Size.Small, dialog.title)
        compare(dialog.standardButton(QQC2.Dialog.Ok).text, accept)
    }

    function test_removing_resetting_and_stopping_ask_alike() {
        const removals = createTemporaryObject(removalsComponent, test)
        removals.ask("bus", { id: "b1", name: "In-Ear" })
        const asked = removalsDialog(removals)
        tryCompare(asked, "opened", true)
        checkDestructive(asked, "Remove bus")
        asked.reject()

        const reset = createTemporaryObject(resetComponent, test)
        reset.open()
        tryCompare(reset, "opened", true)
        checkDestructive(reset, "Reset settings")
        reset.reject()

        const stop = createTemporaryObject(stopComponent, test, { output: { id: "o1", name: "CDN" } })
        stop.open()
        tryCompare(stop, "opened", true)
        checkDestructive(stop, "Stop output")
        stop.reject()
        for (const [group, text] of [["streams", "Stop streams"], ["recordings", "Stop recordings"],
                                     ["media", "Stop recordings and streams"], ["all", "Stop all outputs"]]) {
            const many = createTemporaryObject(stopComponent, test, { group: group })
            compare(many.acceptText, text, group)
        }
    }
}
