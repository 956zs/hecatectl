import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HecateGui

// Rounded surface with an optional icon + title line; children go below the title.
Rectangle {
    id: card

    property string title
    property string icon
    default property alias content: body.data

    implicitHeight: column.implicitHeight + 2 * 20
    radius: 20
    color: Theme.surfaceContainerHigh

    ColumnLayout {
        id: column
        anchors.fill: parent
        anchors.margins: 20
        spacing: 12

        RowLayout {
            visible: card.title.length > 0
            spacing: 10

            Icon {
                visible: card.icon.length > 0
                name: card.icon
                size: 22
                color: Theme.primary
            }

            Label {
                text: card.title
                font.pixelSize: 17
                font.weight: Font.DemiBold
                color: Theme.surfaceContent
            }
        }

        ColumnLayout {
            id: body
            Layout.fillWidth: true
            spacing: 12
        }
    }
}
