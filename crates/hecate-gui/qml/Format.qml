pragma Singleton
import QtQuick
import HecateGui

// Display text and icons derived from Device state, shared by the header and the pages.
QtObject {
    readonly property string statusText: !Device.present ? I18n.text("status.no_device")
                                       : Device.online ? I18n.text("status.online")
                                       : I18n.text("status.asleep")
    readonly property string statusIcon: !Device.present ? "usb_off"
                                       : Device.online ? "check_circle" : "bedtime"
    readonly property string connectionText: Device.present ? I18n.text("connection." + Device.connection)
                                           : I18n.text("value.unknown")
    readonly property string connectionIcon: Device.connection === "wired" ? "usb" : "sensors"
    readonly property string batteryText: Device.batteryPercent < 0 ? I18n.text("value.unknown")
                                        : I18n.text("battery.value").arg(Device.batteryPercent)
    readonly property string batteryIcon: {
        const p = Device.batteryPercent
        if (Device.charging)
            return "battery_charging_full"
        if (p < 0)
            return "battery_unknown"
        if (p >= 95)
            return "battery_full"
        return "battery_" + Math.min(6, Math.floor(p * 7 / 100)) + "_bar"
    }
    readonly property string firmwareText: Device.firmware.length > 0 ? Device.firmware : I18n.text("value.unknown")
}
