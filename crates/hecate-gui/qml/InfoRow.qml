import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HecateGui

// One labelled value in an information list.
RowLayout {
    id: row

    property string icon
    property string label
    property string value
    property color iconColor: Theme.surfaceVariantContent

    spacing: 14

    Rectangle {
        implicitWidth: 40
        implicitHeight: 40
        radius: 12
        color: Theme.surfaceContainerHighest

        Icon {
            anchors.centerIn: parent
            name: row.icon
            size: 22
            color: row.iconColor
        }
    }

    Label {
        Layout.fillWidth: true
        text: row.label
        color: Theme.surfaceVariantContent
        elide: Text.ElideRight
    }

    Label {
        text: row.value
        font.weight: Font.Medium
        color: Theme.surfaceContent
    }
}
