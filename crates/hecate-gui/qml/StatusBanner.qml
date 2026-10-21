import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HecateGui

// Shown while the pages cannot be used: no mouse, mouse asleep, or the first read still running.
Rectangle {
    id: banner

    readonly property string mode: !Device.present ? "offline"
                                  : !Device.online ? "asleep"
                                  : !Device.loaded ? "loading" : ""

    visible: mode.length > 0
    implicitHeight: row.implicitHeight + 2 * 16
    radius: 20
    color: Theme.secondaryContainer

    RowLayout {
        id: row
        anchors.fill: parent
        anchors.margins: 16
        anchors.leftMargin: 20
        spacing: 16

        Icon {
            name: banner.mode === "offline" ? "usb_off" : banner.mode === "asleep" ? "bedtime" : "hourglass_top"
            size: 28
            color: Theme.secondaryContainerContent

            SequentialAnimation on opacity {
                running: banner.visible && banner.mode !== "offline"
                loops: Animation.Infinite
                alwaysRunToEnd: true
                NumberAnimation { to: 0.35; duration: 900; easing.type: Easing.InOutSine }
                NumberAnimation { to: 1; duration: 900; easing.type: Easing.InOutSine }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2

            Label {
                Layout.fillWidth: true
                text: banner.mode === "offline" ? I18n.text("status.no_device")
                    : banner.mode === "asleep" ? I18n.text("status.asleep")
                    : I18n.text("status.loading")
                wrapMode: Text.Wrap
                font.pixelSize: 15
                font.weight: Font.DemiBold
                color: Theme.secondaryContainerContent
            }

            Label {
                Layout.fillWidth: true
                visible: text.length > 0
                text: banner.mode === "offline" ? I18n.text("status.no_device_hint")
                    : banner.mode === "asleep" ? I18n.text("status.asleep_hint") : ""
                wrapMode: Text.Wrap
                font.pixelSize: 13
                color: Theme.secondaryContainerContent
                opacity: 0.85
            }
        }
    }
}
