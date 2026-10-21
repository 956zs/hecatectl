import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HecateGui

// Device name with connection, battery and awake/asleep chips.
RowLayout {
    id: header

    component Chip: Rectangle {
        id: chip

        property string icon
        property string text
        property color tint: Theme.surfaceVariantContent

        implicitWidth: chipRow.implicitWidth + 24
        implicitHeight: 32
        radius: height / 2
        color: Theme.surfaceContainerHigh

        RowLayout {
            id: chipRow
            anchors.centerIn: parent
            spacing: 6

            Icon {
                name: chip.icon
                size: 18
                filled: true
                color: chip.tint
            }

            Label {
                text: chip.text
                font.pixelSize: 13
                font.weight: Font.Medium
                color: Theme.surfaceContent
            }
        }
    }

    spacing: 12

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 10

        Label {
            Layout.fillWidth: true
            text: I18n.text("device.name")
            elide: Text.ElideRight
            font.pixelSize: 24
            font.weight: Font.DemiBold
            color: Theme.surfaceContent
        }

        Flow {
            Layout.fillWidth: true
            spacing: 8

            Chip {
                icon: Format.statusIcon
                text: Format.statusText
                tint: Device.present && Device.online ? Theme.primary : Theme.surfaceVariantContent
            }

            Chip {
                visible: Device.present
                icon: Format.connectionIcon
                text: Format.connectionText
            }

            Chip {
                visible: Device.present && Device.batteryPercent >= 0
                icon: Format.batteryIcon
                text: Device.charging ? I18n.text("battery.charging_value").arg(Device.batteryPercent) : Format.batteryText
                tint: Device.charging ? Theme.primary
                    : Device.batteryPercent <= 15 ? Theme.error : Theme.surfaceVariantContent
            }
        }
    }

    BusyIndicator {
        Layout.alignment: Qt.AlignTop
        implicitWidth: 36
        implicitHeight: 36
        running: Device.busy
        visible: running
    }
}
