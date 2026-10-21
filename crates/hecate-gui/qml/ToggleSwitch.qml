import QtQuick
import HecateGui

// Material 3 switch. Shows `checked` only; a tap emits `toggled` and waits for the new value.
Item {
    id: toggle

    property bool checked

    signal toggled(bool value)

    implicitWidth: 52
    implicitHeight: 32
    opacity: enabled ? 1 : 0.38

    Rectangle {
        anchors.fill: parent
        radius: height / 2
        color: toggle.checked ? Theme.primary : Theme.surfaceContainerHighest
        border.width: toggle.checked ? 0 : 2
        border.color: Theme.outline

        Behavior on color {
            ColorAnimation { duration: 150 }
        }

        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: toggle.checked ? 24 : 16
            height: width
            radius: width / 2
            x: toggle.checked ? parent.width - width - 4 : 8
            color: toggle.checked ? Theme.primaryContent : Theme.outline

            Behavior on x {
                NumberAnimation { duration: 150; easing.type: Easing.OutCubic }
            }
            Behavior on width {
                NumberAnimation { duration: 150 }
            }
        }
    }

    HoverHandler {
        cursorShape: Qt.PointingHandCursor
    }

    TapHandler {
        onTapped: toggle.toggled(!toggle.checked)
    }
}
