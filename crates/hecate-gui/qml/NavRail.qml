import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HecateGui

// Navigation rail: one entry per page, `model` items are { label: i18n key, icon: symbol name }.
Item {
    id: rail

    property var model: []
    property int currentIndex: 0

    implicitWidth: 88

    ColumnLayout {
        anchors.top: parent.top
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.topMargin: 8
        spacing: 8

        Rectangle {
            Layout.alignment: Qt.AlignHCenter
            Layout.bottomMargin: 16
            implicitWidth: 56
            implicitHeight: 56
            radius: 18
            color: Theme.primaryContainer

            Icon {
                anchors.centerIn: parent
                name: "mouse"
                size: 28
                filled: true
                color: Theme.primaryContainerContent
            }
        }

        Repeater {
            model: rail.model

            delegate: Item {
                id: entry

                required property var modelData
                required property int index
                readonly property bool active: rail.currentIndex === index

                Layout.alignment: Qt.AlignHCenter
                implicitWidth: 80
                implicitHeight: 60

                Rectangle {
                    id: pill
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 56
                    height: 32
                    radius: height / 2
                    color: entry.active ? Theme.secondaryContainer
                         : hover.hovered ? Qt.alpha(Theme.surfaceContent, 0.08) : "transparent"

                    Behavior on color {
                        ColorAnimation { duration: 150 }
                    }

                    Icon {
                        anchors.centerIn: parent
                        name: entry.modelData.icon
                        filled: entry.active
                        color: entry.active ? Theme.secondaryContainerContent : Theme.surfaceVariantContent
                    }
                }

                Label {
                    anchors.top: pill.bottom
                    anchors.topMargin: 4
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: I18n.text(entry.modelData.label)
                    font.pixelSize: 12
                    font.weight: entry.active ? Font.DemiBold : Font.Medium
                    color: entry.active ? Theme.surfaceContent : Theme.surfaceVariantContent
                }

                HoverHandler {
                    id: hover
                    cursorShape: Qt.PointingHandCursor
                }

                TapHandler {
                    onTapped: rail.currentIndex = entry.index
                }
            }
        }
    }
}
