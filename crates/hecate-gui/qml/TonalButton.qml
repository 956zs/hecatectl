import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HecateGui

// Filled tonal button with an optional leading icon.
Rectangle {
    id: button

    property string icon
    property string text

    signal clicked

    implicitWidth: row.implicitWidth + 2 * 20
    implicitHeight: 40
    radius: height / 2
    color: hover.hovered ? Qt.tint(Theme.secondaryContainer, Qt.alpha(Theme.secondaryContainerContent, 0.08))
                         : Theme.secondaryContainer
    opacity: enabled ? 1 : 0.5

    Behavior on color {
        ColorAnimation { duration: 150 }
    }

    RowLayout {
        id: row
        anchors.centerIn: parent
        spacing: 8

        Icon {
            visible: button.icon.length > 0
            name: button.icon
            size: 18
            color: Theme.secondaryContainerContent
        }

        Label {
            text: button.text
            font.weight: Font.Medium
            color: Theme.secondaryContainerContent
        }
    }

    HoverHandler {
        id: hover
        cursorShape: Qt.PointingHandCursor
    }

    TapHandler {
        onTapped: button.clicked()
    }
}
