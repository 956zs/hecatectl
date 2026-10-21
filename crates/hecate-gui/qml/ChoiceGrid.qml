import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HecateGui

// Chips for one choice out of `choices` ([{ label, value }]). Shows `current` only; a tap emits `chosen`.
GridLayout {
    id: grid

    property var choices: []
    property var current
    // Equal columns, or chips as wide as their labels (the rest of the row is shared out).
    property bool equalWidths: true

    signal chosen(var value)

    columns: 2
    columnSpacing: 8
    rowSpacing: 8

    Repeater {
        model: grid.choices

        delegate: Rectangle {
            id: chip

            required property var modelData
            readonly property bool selected: modelData.value === grid.current

            Layout.fillWidth: true
            Layout.preferredWidth: grid.equalWidths ? 1 : -1
            implicitWidth: chipRow.implicitWidth + 24
            implicitHeight: 40
            radius: 12
            color: selected ? Theme.secondaryContainer
                 : hover.hovered ? Qt.tint(Theme.surfaceContainerHighest, Qt.alpha(Theme.surfaceContent, 0.08))
                 : Theme.surfaceContainerHighest
            border.width: selected ? 0 : 1
            border.color: Theme.outlineVariant
            opacity: grid.enabled ? 1 : 0.5

            Behavior on color {
                ColorAnimation { duration: 150 }
            }

            RowLayout {
                id: chipRow
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 12
                spacing: 6

                Icon {
                    visible: chip.selected
                    name: "check"
                    size: 18
                    color: Theme.secondaryContainerContent
                }

                Label {
                    Layout.fillWidth: true
                    text: chip.modelData.label
                    elide: Text.ElideRight
                    font.weight: Font.Medium
                    color: chip.selected ? Theme.secondaryContainerContent : Theme.surfaceContent
                }
            }

            HoverHandler {
                id: hover
                cursorShape: Qt.PointingHandCursor
            }

            TapHandler {
                onTapped: grid.chosen(chip.modelData.value)
            }
        }
    }
}
