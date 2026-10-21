import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import HecateGui

// One DPI stage: tap to make it current, edit its DPI, pick its colour. All three go through
// signals; the card shows only the values read from the mouse.
Rectangle {
    id: card

    required property int index
    required property var stage
    property bool current

    signal selected
    signal dpiEntered(int dpi)
    signal colorClicked

    implicitHeight: column.implicitHeight + 2 * 12
    radius: 16
    color: current ? Theme.secondaryContainer : Theme.surfaceContainerHighest
    border.width: current ? 2 : 0
    border.color: Theme.primary
    opacity: enabled ? 1 : 0.6

    TapHandler {
        enabled: !card.current
        onTapped: card.selected()
    }

    HoverHandler {
        enabled: !card.current
        cursorShape: Qt.PointingHandCursor
    }

    ColumnLayout {
        id: column
        anchors.fill: parent
        anchors.margins: 12
        spacing: 6

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            // Radio mark like the vendor's "current" button.
            Rectangle {
                implicitWidth: 18
                implicitHeight: 18
                radius: 9
                color: "transparent"
                border.width: 2
                border.color: card.current ? Theme.primary : Theme.outline

                Rectangle {
                    anchors.centerIn: parent
                    width: 8
                    height: 8
                    radius: 4
                    visible: card.current
                    color: Theme.primary
                }
            }

            Label {
                Layout.fillWidth: true
                text: card.current ? I18n.text("dpi.current") : String(card.index + 1)
                elide: Text.ElideRight
                font.pixelSize: 13
                font.weight: Font.Medium
                color: card.current ? Theme.secondaryContainerContent : Theme.surfaceVariantContent
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            TextField {
                Layout.fillWidth: true
                text: String(card.stage.dpi)
                inputMethodHints: Qt.ImhDigitsOnly
                validator: IntValidator { bottom: 0; top: 99999 }
                horizontalAlignment: TextInput.AlignHCenter
                font.pixelSize: 17
                font.weight: Font.DemiBold
                Material.accent: Theme.primary
                Material.foreground: Theme.surfaceContent

                onEditingFinished: {
                    const dpi = parseInt(text)
                    text = Qt.binding(() => String(card.stage.dpi))
                    if (!isNaN(dpi) && dpi !== card.stage.dpi)
                        card.dpiEntered(dpi)
                }
            }

            Rectangle {
                implicitWidth: 26
                implicitHeight: 26
                radius: 13
                color: card.stage.color
                border.width: 2
                border.color: Theme.outlineVariant

                HoverHandler {
                    cursorShape: Qt.PointingHandCursor
                }

                TapHandler {
                    onTapped: card.colorClicked()
                }
            }
        }
    }
}
