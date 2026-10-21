import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import HecateGui

// Modal message. With `cancelText` empty it is a plain notice with one button.
Dialog {
    id: dialog

    property string text
    property string okText: I18n.text("dialog.ok")
    property string cancelText

    parent: Overlay.overlay
    anchors.centerIn: parent
    // Fixed: a width bound to the overlay makes the wrapped text loop on implicitHeight.
    width: 440
    modal: true
    Material.background: Theme.surfaceContainerHigh
    Material.foreground: Theme.surfaceContent
    Material.accent: Theme.primary
    Material.roundedScale: Material.ExtraLargeScale

    contentItem: Label {
        text: dialog.text
        wrapMode: Text.Wrap
        color: Theme.surfaceVariantContent
    }

    footer: DialogButtonBox {
        Material.background: "transparent"

        Button {
            visible: dialog.cancelText.length > 0
            text: dialog.cancelText
            flat: true
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
        }

        Button {
            text: dialog.okText
            flat: true
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
        }
    }
}
