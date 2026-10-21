import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import HecateGui

// Asks for a macro name and checks it against the device name rules (encoding, 30 bytes) as you type.
Dialog {
    id: dialog

    property alias value: field.text
    property var onOk: null
    readonly property string problem: Macros.nameProblem(field.text)

    function ask(initial, callback) {
        field.text = initial
        onOk = callback
        open()
    }

    parent: Overlay.overlay
    anchors.centerIn: parent
    // Fixed: a width bound to the overlay makes the wrapped text loop on implicitHeight.
    width: 420
    modal: true
    title: I18n.text("macro.name")
    Material.background: Theme.surfaceContainerHigh
    Material.foreground: Theme.surfaceContent
    Material.accent: Theme.primary
    Material.roundedScale: Material.ExtraLargeScale
    onOpened: field.forceActiveFocus()

    contentItem: ColumnLayout {
        spacing: 8

        TextField {
            id: field
            Layout.fillWidth: true
            Material.accent: Theme.primary
            onAccepted: if (dialog.problem.length === 0) dialog.accept()
        }

        Label {
            Layout.fillWidth: true
            visible: dialog.problem.length > 0 && field.text.length > 0
            text: Model.problemText(dialog.problem)
            wrapMode: Text.Wrap
            font.pixelSize: 13
            color: Theme.error
        }
    }

    footer: DialogButtonBox {
        Material.background: "transparent"

        Button {
            text: I18n.text("dialog.cancel")
            flat: true
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }

        Button {
            text: I18n.text("dialog.ok")
            flat: true
            enabled: dialog.problem.length === 0
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
        }
    }

    onAccepted: if (onOk) onOk()
}
