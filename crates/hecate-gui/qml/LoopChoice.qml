import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import HecateGui

// Macro play mode (`Type of cycle`): three loop modes or a play count. Shows `mode` ({ loop, count })
// and emits `edited` with the new one.
ColumnLayout {
    id: choice

    property var mode: ({ loop: "count", count: 1 })

    signal edited(var mode)

    spacing: 8

    ChoiceGrid {
        Layout.fillWidth: true
        columns: 2
        choices: ["released", "anyKey", "pressedAgain", "count"].map(l => ({ label: I18n.text("loop." + l), value: l }))
        current: choice.mode.loop
        onChosen: l => choice.edited({ loop: l, count: choice.mode.count ?? 1 })
    }

    RowLayout {
        visible: choice.mode.loop === "count"
        spacing: 8

        SpinBox {
            from: 1
            to: Model.options.loopCountMax
            value: choice.mode.count ?? 1
            editable: true
            Material.accent: Theme.primary
            onValueModified: choice.edited({ loop: "count", count: value })
        }

        Label {
            text: I18n.text("loop.count_unit")
            color: Theme.surfaceVariantContent
        }
    }
}
