import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HecateGui

// A setting: title and optional help text on the left, its control(s) on the right.
RowLayout {
    id: row

    property string title
    property string help
    default property alias controls: slot.data

    spacing: 16

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 2

        Label {
            Layout.fillWidth: true
            text: row.title
            wrapMode: Text.Wrap
            font.weight: Font.Medium
            color: Theme.surfaceContent
        }

        Label {
            Layout.fillWidth: true
            visible: row.help.length > 0
            text: row.help
            wrapMode: Text.Wrap
            font.pixelSize: 13
            color: Theme.surfaceVariantContent
        }
    }

    RowLayout {
        id: slot
        Layout.alignment: Qt.AlignVCenter | Qt.AlignRight
        spacing: 12
    }
}
