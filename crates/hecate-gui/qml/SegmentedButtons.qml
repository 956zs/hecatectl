import QtQuick
import QtQuick.Controls
import HecateGui

// Connected button group (Material 3 expressive): one choice out of `model`.
// Selection follows `current` only; a click emits `activated` and waits for the new value.
Item {
    id: group

    property var model: []
    property var current
    property var labelFor: value => String(value)
    readonly property int count: model ? model.length : 0

    signal activated(var value)

    implicitWidth: row.implicitWidth
    implicitHeight: 44
    opacity: enabled ? 1 : 0.5

    Row {
        id: row
        spacing: 2

        Repeater {
            model: group.model

            delegate: Rectangle {
                id: segment

                required property var modelData
                required property int index
                readonly property bool selected: modelData === group.current
                readonly property real outer: height / 2
                property real inner: selected ? height / 2 : 8

                width: Math.max(96, content.implicitWidth + 40)
                height: group.implicitHeight
                topLeftRadius: index === 0 ? outer : inner
                bottomLeftRadius: index === 0 ? outer : inner
                topRightRadius: index === group.count - 1 ? outer : inner
                bottomRightRadius: index === group.count - 1 ? outer : inner
                color: selected ? Theme.primary
                     : hover.hovered ? Qt.tint(Theme.surfaceContainerHighest, Qt.alpha(Theme.surfaceContent, 0.08))
                     : Theme.surfaceContainerHighest

                Behavior on color {
                    ColorAnimation { duration: 150 }
                }
                Behavior on inner {
                    NumberAnimation { duration: 200; easing.type: Easing.OutCubic }
                }

                Row {
                    id: content
                    anchors.centerIn: parent
                    spacing: 6

                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: segment.selected
                        name: "check"
                        size: 18
                        color: Theme.primaryContent
                    }

                    Label {
                        anchors.verticalCenter: parent.verticalCenter
                        text: group.labelFor(segment.modelData)
                        font.weight: Font.Medium
                        color: segment.selected ? Theme.primaryContent : Theme.surfaceVariantContent
                    }
                }

                HoverHandler {
                    id: hover
                    enabled: !segment.selected
                    cursorShape: Qt.PointingHandCursor
                }

                TapHandler {
                    enabled: !segment.selected
                    onTapped: group.activated(segment.modelData)
                }
            }
        }
    }
}
