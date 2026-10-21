import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import HecateGui

// Vendor tab "KeySet" (ui-features §3): the five remappable buttons and the function categories.
// Choices that need no further input assign at once, like the vendor; the others have an Apply button.
ScrollView {
    id: page

    contentWidth: availableWidth
    clip: true

    readonly property var options: Model.options
    property int selected: -1
    readonly property var key: selected >= 0 ? (Model.keys[selected] ?? null) : null
    readonly property var current: key ? key.function : ({ category: "" })
    property string category: ""

    // Editor state, reset from the button's current function when the selection changes.
    property int fireInterval: 40
    property int fireCount: 0
    property int comboModifier: 0
    property var comboKeys: []
    property string macroId: ""
    property var macroLoop: ({ loop: "count", count: 1 })

    readonly property var categories: [
        { value: "mouse", label: I18n.text("cat.mouse") },
        { value: "report", label: I18n.text("cat.report") },
        { value: "fire", label: I18n.text("cat.fire") },
        { value: "combination", label: I18n.text("cat.combination") },
        { value: "scrollV", label: I18n.text("cat.scroll_v") },
        { value: "scrollH", label: I18n.text("cat.scroll_h") },
        { value: "dpi", label: I18n.text("cat.dpi") },
        { value: "multimedia", label: I18n.text("cat.multimedia") },
        { value: "macro", label: I18n.text("cat.macro") },
        { value: "disabled", label: I18n.text("cat.disabled") }
    ]
    readonly property var buttonIcons: ["left_click", "right_click", "mouse", "arrow_back", "arrow_forward"]

    function select(index) {
        selected = index
        const fn = Model.keys[index].function
        category = fn.category === "unknown" ? "" : fn.category
        fireInterval = fn.category === "fire" ? Math.max(options.fireInterval[0], fn.interval) : 40
        fireCount = fn.category === "fire" ? Math.min(options.fireCount[1], fn.count) : 0
        comboModifier = 0
        comboKeys = []
        if (fn.category === "combination" && fn.valid) {
            const mod = fn.events.find(e => e.kind === 0)
            comboModifier = mod ? mod.code : 0
            comboKeys = fn.events.filter(e => e.kind !== 0 && e.state === "down")
        }
        macroId = ""
        macroLoop = fn.category === "macro" ? { loop: fn.loop, count: fn.count ?? 1 } : { loop: "count", count: 1 }
        if (fn.category === "macro") {
            const m = Model.macros.find(m => m.name === fn.name)
            macroId = m ? m.id : ""
        }
    }

    function assign(fn) {
        Model.assign(selected, fn)
    }

    GridLayout {
        width: page.availableWidth
        visible: Model.loaded
        columns: page.availableWidth >= 900 ? 2 : 1
        columnSpacing: 16
        rowSpacing: 16

        Card {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            title: I18n.text("keys.buttons")
            icon: "mouse"

            Label {
                Layout.fillWidth: true
                text: I18n.text("keys.hint")
                wrapMode: Text.Wrap
                font.pixelSize: 13
                color: Theme.surfaceVariantContent
            }

            Repeater {
                model: Model.keys

                delegate: Rectangle {
                    id: row

                    required property var modelData
                    required property int index
                    readonly property bool active: page.selected === index

                    Layout.fillWidth: true
                    implicitHeight: rowLayout.implicitHeight + 24
                    radius: 16
                    color: active ? Theme.secondaryContainer
                         : hover.hovered ? Qt.tint(Theme.surfaceContainerHighest, Qt.alpha(Theme.surfaceContent, 0.06))
                         : Theme.surfaceContainerHighest
                    border.width: active ? 2 : 0
                    border.color: Theme.primary

                    RowLayout {
                        id: rowLayout
                        anchors.fill: parent
                        anchors.margins: 12
                        anchors.leftMargin: 16
                        spacing: 14

                        Icon {
                            name: page.buttonIcons[row.index]
                            size: 24
                            color: row.active ? Theme.secondaryContainerContent : Theme.primary
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2

                            Label {
                                text: I18n.text("keys.button" + row.index)
                                font.weight: Font.DemiBold
                                color: row.active ? Theme.secondaryContainerContent : Theme.surfaceContent
                            }

                            Label {
                                Layout.fillWidth: true
                                text: I18n.text("cat." + (row.modelData.function.category === "scrollV" ? "scroll_v"
                                      : row.modelData.function.category === "scrollH" ? "scroll_h"
                                      : row.modelData.function.category)) + I18n.text("keys.label_join")
                                      + Model.functionLabel(row.modelData.function)
                                elide: Text.ElideRight
                                font.pixelSize: 13
                                color: row.active ? Theme.secondaryContainerContent : Theme.surfaceVariantContent
                            }
                        }

                        Icon {
                            visible: row.modelData.locked
                            name: "lock"
                            size: 18
                            color: Theme.surfaceVariantContent
                        }
                    }

                    HoverHandler {
                        id: hover
                        cursorShape: Qt.PointingHandCursor
                    }

                    TapHandler {
                        onTapped: page.select(row.index)
                    }
                }
            }
        }

        Card {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            visible: page.key !== null
            enabled: Model.editable && page.key !== null && !page.key.locked
            title: page.selected >= 0 ? I18n.text("keys.button" + page.selected) : ""
            icon: "tune"

            Label {
                Layout.fillWidth: true
                visible: page.key !== null && page.key.locked
                text: I18n.text("keys.locked")
                wrapMode: Text.Wrap
                color: Theme.error
            }

            ChoiceGrid {
                Layout.fillWidth: true
                columns: 2
                choices: page.categories
                current: page.category
                onChosen: c => page.category = c
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.topMargin: 4
                implicitHeight: 1
                color: Theme.outlineVariant
            }

            // Mousekey
            ChoiceGrid {
                Layout.fillWidth: true
                visible: page.category === "mouse"
                choices: page.options.mouseButtons.map(b => ({ label: I18n.text(b.nameKey), value: b.value }))
                current: page.current.category === "mouse" ? page.current.button : -1
                onChosen: b => page.assign({ category: "mouse", button: b })
            }

            // DPI key
            ChoiceGrid {
                Layout.fillWidth: true
                visible: page.category === "dpi"
                columns: 3
                choices: [1, 2, 3].map(a => ({ label: I18n.text(["", "dpi.loop", "dpi.up", "dpi.down"][a]), value: a }))
                current: page.current.category === "dpi" ? page.current.action : -1
                onChosen: a => page.assign({ category: "dpi", action: a })
            }

            // Scroll up / down
            ChoiceGrid {
                Layout.fillWidth: true
                visible: page.category === "scrollV"
                choices: [{ label: I18n.text("scroll.up"), value: 1 }, { label: I18n.text("scroll.down"), value: 2 }]
                current: page.current.category === "scrollV" ? page.current.dir : -1
                onChosen: d => page.assign({ category: "scrollV", dir: d })
            }

            // Scroll left / right
            ChoiceGrid {
                Layout.fillWidth: true
                visible: page.category === "scrollH"
                choices: [{ label: I18n.text("scroll.left"), value: 1 }, { label: I18n.text("scroll.right"), value: 2 }]
                current: page.current.category === "scrollH" ? page.current.dir : -1
                onChosen: d => page.assign({ category: "scrollH", dir: d })
            }

            // Multimedia
            ChoiceGrid {
                Layout.fillWidth: true
                visible: page.category === "multimedia"
                columns: 3
                choices: page.options.multimedia.map(m => ({ label: I18n.text(m.nameKey), value: m.usage }))
                current: page.current.category === "multimedia" ? page.current.usage : -1
                onChosen: u => page.assign({ category: "multimedia", usage: u })
            }

            // Report key and disabled: nothing to choose.
            TonalButton {
                visible: page.category === "report" || page.category === "disabled"
                icon: "check"
                text: I18n.text("keys.apply")
                enabled: page.current.category !== page.category
                onClicked: page.assign({ category: page.category })
            }

            // Fire key
            ColumnLayout {
                Layout.fillWidth: true
                visible: page.category === "fire"
                spacing: 8

                SettingRow {
                    Layout.fillWidth: true
                    title: I18n.text("fire.count")

                    Slider {
                        implicitWidth: 180
                        from: page.options.fireCount[0]
                        to: page.options.fireCount[1]
                        stepSize: 1
                        snapMode: Slider.SnapAlways
                        value: page.fireCount
                        Material.accent: Theme.primary
                        onMoved: page.fireCount = Math.round(value)
                    }

                    Label {
                        Layout.preferredWidth: 32
                        text: String(page.fireCount)
                        horizontalAlignment: Text.AlignRight
                        font.weight: Font.DemiBold
                        color: Theme.surfaceContent
                    }
                }

                SettingRow {
                    Layout.fillWidth: true
                    title: I18n.text("fire.interval")

                    Slider {
                        implicitWidth: 180
                        from: page.options.fireInterval[0]
                        to: page.options.fireInterval[1]
                        stepSize: 1
                        snapMode: Slider.SnapAlways
                        value: page.fireInterval
                        Material.accent: Theme.primary
                        onMoved: page.fireInterval = Math.round(value)
                    }

                    TextField {
                        implicitWidth: 72
                        text: String(page.fireInterval)
                        horizontalAlignment: TextInput.AlignHCenter
                        validator: IntValidator { bottom: page.options.fireInterval[0]; top: page.options.fireInterval[1] }
                        Material.accent: Theme.primary
                        onEditingFinished: {
                            page.fireInterval = parseInt(text)
                            text = Qt.binding(() => String(page.fireInterval))
                        }
                    }
                }

                Label {
                    Layout.fillWidth: true
                    text: I18n.text("fire.hint")
                    wrapMode: Text.Wrap
                    font.pixelSize: 13
                    color: Theme.surfaceVariantContent
                }

                TonalButton {
                    Layout.alignment: Qt.AlignRight
                    icon: "check"
                    text: I18n.text("keys.apply")
                    onClicked: page.assign({ category: "fire", interval: page.fireInterval, count: page.fireCount })
                }
            }

            // Combination key: one modifier (or Close = disabled) plus captured keys.
            ColumnLayout {
                Layout.fillWidth: true
                visible: page.category === "combination"
                spacing: 10

                ChoiceGrid {
                    Layout.fillWidth: true
                    columns: 5
                    choices: [{ label: I18n.text("combination.close"), value: 0 }]
                             .concat(page.options.combinationModifiers.map(m => ({ label: m.label, value: m.value })))
                    current: page.comboModifier
                    onChosen: m => page.comboModifier = m
                }

                KeyCaptureField {
                    Layout.fillWidth: true
                    enabled: page.comboModifier !== 0
                    maxKeys: page.options.combinationKeysMax
                    keys: page.comboKeys
                    placeholder: I18n.text("combination.input")
                    onEdited: keys => page.comboKeys = keys
                    // A modifier pressed in the box picks the matching radio (left and right alike).
                    onModifierPressed: mask => {
                        const left = (mask | (mask >> 4)) & 0x0F
                        if (page.options.combinationModifiers.some(m => m.value === left))
                            page.comboModifier = left
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    Item { Layout.fillWidth: true }

                    TonalButton {
                        icon: "backspace"
                        text: I18n.text("combination.clear")
                        onClicked: page.comboKeys = []
                    }

                    TonalButton {
                        icon: "check"
                        text: I18n.text("keys.apply")
                        enabled: page.comboModifier === 0 || page.comboKeys.length > 0
                        onClicked: page.assign({
                            category: "combination",
                            modifier: page.comboModifier,
                            keys: page.comboKeys.map(k => ({ kind: k.kind, code: k.code }))
                        })
                    }
                }
            }

            // Macro: a library macro is copied into this button's slot.
            ColumnLayout {
                Layout.fillWidth: true
                visible: page.category === "macro"
                spacing: 10

                Label {
                    Layout.fillWidth: true
                    visible: Model.macros.length === 0
                    text: I18n.text("macro.list_empty")
                    wrapMode: Text.Wrap
                    color: Theme.surfaceVariantContent
                }

                ChoiceGrid {
                    Layout.fillWidth: true
                    visible: Model.macros.length > 0
                    choices: Model.macros.map(m => ({ label: m.name, value: m.id }))
                    current: page.macroId
                    onChosen: id => {
                        page.macroId = id
                        const m = Model.macros.find(m => m.id === id)
                        page.macroLoop = { loop: m.loop, count: m.count ?? 1 }
                    }
                }

                Label {
                    text: I18n.text("loop.title")
                    font.weight: Font.Medium
                    color: Theme.surfaceContent
                }

                LoopChoice {
                    Layout.fillWidth: true
                    mode: page.macroLoop
                    onEdited: mode => page.macroLoop = mode
                }

                TonalButton {
                    Layout.alignment: Qt.AlignRight
                    icon: "check"
                    text: I18n.text("keys.apply")
                    enabled: page.macroId.length > 0
                    onClicked: {
                        const m = Model.macros.find(m => m.id === page.macroId)
                        if (!m) {
                            Model.notify(I18n.text("macro.select"))
                            return
                        }
                        const fn = Object.assign({ category: "macro", name: m.name, events: m.events.map(e => ({ state: e.state, kind: e.kind, code: e.code, delay: e.delay })) }, page.macroLoop)
                        page.assign(fn)
                    }
                }
            }
        }
    }
}
