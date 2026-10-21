import QtQuick
import HecateGui

// Material Symbols glyph by ligature name.
Text {
    property string name
    property real size: 24
    property bool filled: false

    text: name
    color: Theme.surfaceContent
    font.family: Theme.iconFontFamily
    font.pixelSize: size
    font.variableAxes: ({ "FILL": filled ? 1 : 0, "opsz": Math.min(48, Math.max(20, size)) })
    verticalAlignment: Text.AlignVCenter
    horizontalAlignment: Text.AlignHCenter
}
