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
    // Vendor icon steps (docs/ui-features.md §6.4): charging, 100, 80-99, 60-79, 20-59, 0-19.
    readonly property string batteryIcon: {
        const p = Device.batteryPercent
        if (Device.charging)
            return "battery_charging_full"
        if (p < 0)
            return "battery_unknown"
        if (p >= 100)
            return "battery_full"
        if (p >= 80)
            return "battery_6_bar"
        if (p >= 60)
            return "battery_5_bar"
        if (p >= 20)
            return "battery_3_bar"
        return "battery_1_bar"
    }
    readonly property string firmwareText: Device.firmware.length > 0 ? Device.firmware : I18n.text("value.unknown")
}
