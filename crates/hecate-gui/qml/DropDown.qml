import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import HecateGui

// Combo box over `choices` ([{ label, value }]). Shows `current` only; picking an entry emits
// `chosen` and waits for the new value.
ComboBox {
    id: box

    property var choices: []
    property var current

    signal chosen(var value)

    function indexOf(value) {
        return choices.findIndex(c => c.value === value)
    }

    implicitWidth: 150
    model: choices
    textRole: "label"
    currentIndex: indexOf(current)
    Material.background: Theme.surfaceContainerHighest
    Material.foreground: Theme.surfaceContent
    Material.accent: Theme.primary
    Material.roundedScale: Material.SmallScale

    onActivated: index => {
        const value = choices[index].value
        currentIndex = Qt.binding(() => indexOf(current))
        if (value !== current)
            chosen(value)
    }
}
