import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HecateGui

// Transient message at the bottom of the window; hides itself after `timeout` ms.
Rectangle {
    id: bar

    property string message
    property bool shown: false
    property int timeout: 6000

    function show(text) {
        message = text
        shown = true
        hideTimer.restart()
    }

    width: Math.min(row.implicitWidth + 2 * 16, (parent ? parent.width : 600) - 48)
    implicitHeight: Math.max(48, row.implicitHeight + 2 * 12)
    radius: 12
    color: Theme.inverseSurface
    opacity: shown ? 1 : 0
    visible: opacity > 0
    transform: Translate {
        y: bar.shown ? 0 : 16
        Behavior on y {
            NumberAnimation { duration: 200; easing.type: Easing.OutCubic }
        }
    }

    Behavior on opacity {
        NumberAnimation { duration: 200 }
    }

    RowLayout {
        id: row
        anchors.fill: parent
        anchors.leftMargin: 16
        anchors.rightMargin: 8
        spacing: 12

        Icon {
            name: "error"
            size: 20
            color: Theme.inversePrimary
        }

        Label {
            Layout.fillWidth: true
            Layout.maximumWidth: 560
            text: bar.message
            wrapMode: Text.Wrap
            color: Theme.inverseSurfaceContent
        }

        Label {
            Layout.rightMargin: 8
            text: I18n.text("error.dismiss")
            font.weight: Font.DemiBold
            color: Theme.inversePrimary
            padding: 8

            HoverHandler {
                cursorShape: Qt.PointingHandCursor
            }

            TapHandler {
                onTapped: bar.shown = false
            }
        }
    }

    Timer {
        id: hideTimer
        interval: bar.timeout
        onTriggered: bar.shown = false
    }
}
