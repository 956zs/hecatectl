import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HecateGui

// Shown while there is no mouse to talk to: nothing plugged in, or the mouse is asleep.
Rectangle {
    id: banner

    readonly property bool asleep: Device.present && !Device.online

    visible: !Device.present || !Device.online
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
            name: banner.asleep ? "bedtime" : "usb_off"
            size: 28
            color: Theme.secondaryContainerContent

            SequentialAnimation on opacity {
                running: banner.visible && banner.asleep
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
                text: banner.asleep ? I18n.text("status.asleep") : I18n.text("status.no_device")
                font.pixelSize: 15
                font.weight: Font.DemiBold
                color: Theme.secondaryContainerContent
            }

            Label {
                Layout.fillWidth: true
                text: banner.asleep ? I18n.text("status.asleep_hint") : I18n.text("status.no_device_hint")
                wrapMode: Text.Wrap
                font.pixelSize: 13
                color: Theme.secondaryContainerContent
                opacity: 0.85
            }
        }
    }
}
