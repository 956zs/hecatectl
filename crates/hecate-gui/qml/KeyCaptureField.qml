import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HecateGui

// Records keys pressed while focused, as events { kind, code, name }. Modifiers are reported through
// `modifierPressed` instead of being recorded. A full box starts over with the next key.
Rectangle {
    id: field

    property var keys: []
    property int maxKeys: 1
    property string placeholder

    signal modifierPressed(int mask)
    // The new key list; the owner stores it and feeds it back through `keys`.
    signal edited(var keys)

    implicitHeight: 48
    implicitWidth: 200
    radius: 12
    color: Theme.surfaceContainerHighest
    border.width: activeFocus ? 2 : 1
    border.color: activeFocus ? Theme.primary : Theme.outlineVariant
    activeFocusOnTab: true
    opacity: enabled ? 1 : 0.5

    Keys.onPressed: event => {
        event.accepted = true
        if (event.isAutoRepeat)
            return
        const json = Device.keyFromScanCode(event.nativeScanCode)
        if (json.length === 0)
            return
        const ev = JSON.parse(json)
        if (ev.kind === 0) {
            field.modifierPressed(ev.code)
            return
        }
        if (field.keys.some(k => k.kind === ev.kind && k.code === ev.code))
            return
        field.edited((field.keys.length >= field.maxKeys ? [] : field.keys).concat([ev]))
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 14
        anchors.rightMargin: 14
        spacing: 8

        Icon {
            name: "keyboard"
            size: 20
            color: field.activeFocus ? Theme.primary : Theme.surfaceVariantContent
        }

        Label {
            Layout.fillWidth: true
            text: field.keys.length > 0 ? field.keys.map(Model.eventName).join(I18n.text("keys.combo_join")) : field.placeholder
            elide: Text.ElideRight
            color: field.keys.length > 0 ? Theme.surfaceContent : Theme.surfaceVariantContent
            font.weight: field.keys.length > 0 ? Font.DemiBold : Font.Normal
        }
    }

    HoverHandler {
        cursorShape: Qt.IBeamCursor
    }

    TapHandler {
        onTapped: field.forceActiveFocus()
    }
}
