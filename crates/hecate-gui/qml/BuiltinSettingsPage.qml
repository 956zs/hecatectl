import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HecateGui

ScrollView {
    id: page

    contentWidth: availableWidth
    clip: true

    ColumnLayout {
        width: page.availableWidth
        spacing: 16

        Card {
            Layout.fillWidth: true
            title: I18n.text("device.info")
            icon: "info"

            InfoRow {
                Layout.fillWidth: true
                icon: Format.statusIcon
                iconColor: Device.present && Device.online ? Theme.primary : Theme.surfaceVariantContent
                label: I18n.text("status.title")
                value: Format.statusText
            }

            InfoRow {
                Layout.fillWidth: true
                icon: Format.connectionIcon
                label: I18n.text("connection.title")
                value: Format.connectionText
            }

            InfoRow {
                Layout.fillWidth: true
                icon: Format.batteryIcon
                label: I18n.text("battery.title")
                value: Device.charging && Device.batteryPercent >= 0
                       ? I18n.text("battery.charging_value").arg(Device.batteryPercent) : Format.batteryText
            }

            InfoRow {
                Layout.fillWidth: true
                icon: "memory"
                label: I18n.text("device.firmware")
                value: Format.firmwareText
            }

            TonalButton {
                Layout.alignment: Qt.AlignRight
                Layout.topMargin: 4
                icon: "refresh"
                text: Device.busy ? I18n.text("busy") : I18n.text("device.refresh")
                enabled: Device.present && !Device.busy
                onClicked: Device.refresh()
            }
        }
    }
}
