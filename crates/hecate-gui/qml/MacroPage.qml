import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import HecateGui

// Vendor tab "Macro Management" (ui-features §4). The library lives on the PC (Macros); a macro reaches
// the mouse when the KeySet page assigns it to a button.
ScrollView {
    id: page

    contentWidth: availableWidth
    clip: true

    property string selectedId: ""
    // Keep a macro open: the first one when nothing (or a deleted one) is selected.
    readonly property var firstId: Model.macros.length > 0 ? Model.macros[0].id : ""
    onFirstIdChanged: if (macro === null) selectedId = firstId
    Component.onCompleted: selectedId = firstId
    readonly property var macro: Model.macros.find(m => m.id === selectedId) ?? null
    readonly property int maxEvents: Model.options.macroEventsMax
    property int selectedEvent: -1
    property bool autoDelay: true
    property int defaultDelay: 50
    property bool recording: false
    property double lastEventTime: 0

    // Events without display names, as Macros.save expects them.
    function plainEvents() {
        return macro ? macro.events.map(e => ({ state: e.state, kind: e.kind, code: e.code, delay: e.delay })) : []
    }

    function save(changes) {
        const doc = Object.assign({ name: macro.name, events: plainEvents(), loop: macro.loop, count: macro.count ?? 1 }, changes)
        Macros.save(selectedId, JSON.stringify(doc))
    }

    function setEvents(events, select) {
        save({ events: events })
        selectedEvent = Math.min(select, events.length - 1)
    }

    function insertEvents(list) {
        const events = plainEvents()
        if (events.length + list.length > maxEvents) {
            Model.notify(I18n.text("macro.full").arg(maxEvents))
            return
        }
        const at = selectedEvent >= 0 ? selectedEvent + 1 : events.length
        events.splice(at, 0, ...list)
        setEvents(events, at + list.length - 1)
    }

    function moveEvent(to) {
        const events = plainEvents()
        const from = selectedEvent
        if (from < 0 || to < 0 || to >= events.length || to === from)
            return
        const [ev] = events.splice(from, 1)
        events.splice(to, 0, ev)
        setEvents(events, to)
    }

    function editEvents(action) {
        switch (action) {
        case "top":
            moveEvent(0)
            break
        case "move_up":
            moveEvent(selectedEvent - 1)
            break
        case "move_down":
            moveEvent(selectedEvent + 1)
            break
        case "bottom":
            moveEvent(macro.events.length - 1)
            break
        case "delete_event": {
            const events = plainEvents()
            events.splice(selectedEvent, 1)
            setEvents(events, selectedEvent)
            break
        }
        }
    }

    // A key from the recorder: in automatic mode the previous event gets the real gap as its delay.
    function record(ev, state) {
        const events = plainEvents()
        if (events.length >= maxEvents) {
            recording = false
            Model.notify(I18n.text("macro.full").arg(maxEvents))
            return
        }
        const now = Date.now()
        if (autoDelay && events.length > 0 && lastEventTime > 0)
            events[events.length - 1].delay = Math.min(65535, Math.round(now - lastEventTime))
        lastEventTime = now
        events.push({ state: state, kind: ev.kind, code: ev.code, delay: autoDelay ? 0 : defaultDelay })
        setEvents(events, events.length - 1)
    }

    function eventFromKey(event) {
        const json = Device.keyFromScanCode(event.nativeScanCode)
        return json.length > 0 ? JSON.parse(json) : null
    }

    GridLayout {
        width: page.availableWidth
        columns: page.availableWidth >= 900 ? 2 : 1
        columnSpacing: 16
        rowSpacing: 16

        Card {
            Layout.fillWidth: true
            Layout.preferredWidth: 2
            Layout.alignment: Qt.AlignTop
            title: I18n.text("macro.list")
            icon: "list"

            Flow {
                Layout.fillWidth: true
                spacing: 8

                TonalButton {
                    icon: "add"
                    text: I18n.text("macro.new")
                    onClicked: nameDialog.ask("", id => {
                        const created = Macros.create(nameDialog.value)
                        if (created.length > 0)
                            page.selectedId = created
                    })
                }

                TonalButton {
                    icon: "download"
                    text: I18n.text("macro.import_device")
                    enabled: Model.deviceMacros.length > 0
                    onClicked: Model.notify(I18n.text("macro.imported").arg(Macros.importDevice(Device.deviceMacrosJson)))
                }
            }

            Label {
                Layout.fillWidth: true
                visible: Model.macros.length === 0
                text: I18n.text("macro.library_empty")
                wrapMode: Text.Wrap
                color: Theme.surfaceVariantContent
            }

            Repeater {
                model: Model.macros

                delegate: Rectangle {
                    id: item

                    required property var modelData
                    readonly property bool active: page.selectedId === modelData.id

                    Layout.fillWidth: true
                    implicitHeight: 52
                    radius: 14
                    color: active ? Theme.secondaryContainer
                         : hover.hovered ? Qt.tint(Theme.surfaceContainerHighest, Qt.alpha(Theme.surfaceContent, 0.06))
                         : Theme.surfaceContainerHighest
                    border.width: active ? 2 : 0
                    border.color: Theme.primary

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 14
                        anchors.rightMargin: 14
                        spacing: 10

                        Icon {
                            name: "keyboard_command_key"
                            size: 20
                            color: item.active ? Theme.secondaryContainerContent : Theme.primary
                        }

                        Label {
                            Layout.fillWidth: true
                            text: item.modelData.name
                            elide: Text.ElideRight
                            font.weight: Font.Medium
                            color: item.active ? Theme.secondaryContainerContent : Theme.surfaceContent
                        }

                        Label {
                            text: I18n.text("macro.event_count").arg(item.modelData.events.length)
                            font.pixelSize: 12
                            color: item.active ? Theme.secondaryContainerContent : Theme.surfaceVariantContent
                        }
                    }

                    HoverHandler {
                        id: hover
                        cursorShape: Qt.PointingHandCursor
                    }

                    TapHandler {
                        onTapped: {
                            page.selectedId = item.modelData.id
                            page.selectedEvent = -1
                            page.recording = false
                        }
                    }
                }
            }

            Flow {
                Layout.fillWidth: true
                visible: page.macro !== null
                spacing: 8

                TonalButton {
                    icon: "edit"
                    text: I18n.text("macro.rename")
                    onClicked: nameDialog.ask(page.macro.name, () => page.save({ name: nameDialog.value }))
                }

                TonalButton {
                    icon: "delete"
                    text: I18n.text("macro.delete")
                    onClicked: deleteConfirm.open()
                }
            }
        }

        Card {
            Layout.fillWidth: true
            Layout.preferredWidth: 3
            Layout.alignment: Qt.AlignTop
            title: page.macro ? page.macro.name : I18n.text("macro.none_selected")
            icon: "edit_note"

            ColumnLayout {
                Layout.fillWidth: true
                visible: page.macro !== null
                spacing: 12

                Label {
                    text: I18n.text("loop.title")
                    font.weight: Font.Medium
                    color: Theme.surfaceContent
                }

                LoopChoice {
                    Layout.fillWidth: true
                    mode: page.macro ? { loop: page.macro.loop, count: page.macro.count ?? 1 } : ({ loop: "count", count: 1 })
                    onEdited: mode => page.save(mode)
                }

                Label {
                    text: I18n.text("macro.delay_option")
                    font.weight: Font.Medium
                    color: Theme.surfaceContent
                }

                ChoiceGrid {
                    Layout.fillWidth: true
                    columns: 2
                    choices: [{ label: I18n.text("macro.delay_auto"), value: true }, { label: I18n.text("macro.delay_default"), value: false }]
                    current: page.autoDelay
                    onChosen: v => page.autoDelay = v
                }

                // The default delay, also used for inserted events.
                RowLayout {
                    spacing: 8

                    SpinBox {
                        from: 0
                        to: 65535
                        value: page.defaultDelay
                        editable: true
                        Material.accent: Theme.primary
                        onValueModified: page.defaultDelay = value
                    }

                    Label {
                        text: I18n.text("macro.ms")
                        color: Theme.surfaceVariantContent
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    DropDown {
                        id: insertChoice
                        implicitWidth: 170
                        choices: Model.options.mouseButtons.map(b => ({ label: I18n.text(b.nameKey), value: "mouse:" + b.value }))
                                 .concat([{ label: I18n.text("macro.key_down"), value: "down" }, { label: I18n.text("macro.key_up"), value: "up" }])
                        current: "mouse:1"
                        onChosen: v => current = v
                    }

                    TonalButton {
                        icon: "add"
                        text: I18n.text("macro.insert")
                        onClicked: {
                            const v = insertChoice.current
                            if (v.startsWith("mouse:")) {
                                const mask = parseInt(v.slice(6))
                                page.insertEvents([
                                    { state: "down", kind: 4, code: mask, delay: page.defaultDelay },
                                    { state: "up", kind: 4, code: mask, delay: page.defaultDelay }
                                ])
                            } else {
                                keyDialog.eventState = v
                                keyDialog.open()
                            }
                        }
                    }

                    Item { Layout.fillWidth: true }

                    TonalButton {
                        icon: page.recording ? "stop_circle" : "fiber_manual_record"
                        text: page.recording ? I18n.text("macro.record_stop") : I18n.text("macro.record")
                        onClicked: {
                            page.recording = !page.recording
                            page.lastEventTime = 0
                            if (page.recording)
                                recorder.forceActiveFocus()
                        }
                    }
                }

                // Keyboard recorder: focused while recording.
                Item {
                    id: recorder
                    implicitHeight: 0
                    focus: page.recording

                    Keys.onPressed: event => {
                        event.accepted = true
                        const ev = event.isAutoRepeat ? null : page.eventFromKey(event)
                        if (page.recording && ev)
                            page.record(ev, "down")
                    }
                    Keys.onReleased: event => {
                        event.accepted = true
                        const ev = event.isAutoRepeat ? null : page.eventFromKey(event)
                        if (page.recording && ev)
                            page.record(ev, "up")
                    }
                }

                Label {
                    Layout.fillWidth: true
                    visible: page.recording
                    text: I18n.text("macro.recording_hint")
                    wrapMode: Text.Wrap
                    color: Theme.primary
                }

                // Event table: Key / Action / Delay.
                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: 12
                    Layout.rightMargin: 12
                    spacing: 12

                    Label {
                        Layout.preferredWidth: 28
                        text: "#"
                        color: Theme.surfaceVariantContent
                        font.pixelSize: 12
                    }

                    Label {
                        Layout.fillWidth: true
                        text: I18n.text("macro.col_key")
                        color: Theme.surfaceVariantContent
                        font.pixelSize: 12
                    }

                    Label {
                        Layout.preferredWidth: 72
                        text: I18n.text("macro.col_action")
                        color: Theme.surfaceVariantContent
                        font.pixelSize: 12
                    }

                    Label {
                        Layout.preferredWidth: 96
                        text: I18n.text("macro.col_delay")
                        color: Theme.surfaceVariantContent
                        font.pixelSize: 12
                    }
                }

                Repeater {
                    model: page.macro ? page.macro.events : []

                    delegate: Rectangle {
                        id: eventRow

                        required property var modelData
                        required property int index
                        readonly property bool active: page.selectedEvent === index

                        Layout.fillWidth: true
                        implicitHeight: 40
                        radius: 10
                        color: active ? Theme.secondaryContainer : (index % 2 === 0 ? Theme.surfaceContainerHighest : "transparent")

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 12
                            spacing: 12

                            Label {
                                Layout.preferredWidth: 28
                                text: String(eventRow.index + 1)
                                color: Theme.surfaceVariantContent
                                font.pixelSize: 12
                            }

                            Label {
                                Layout.fillWidth: true
                                text: Model.eventName(eventRow.modelData)
                                elide: Text.ElideRight
                                font.weight: Font.Medium
                                color: eventRow.active ? Theme.secondaryContainerContent : Theme.surfaceContent
                            }

                            Label {
                                Layout.preferredWidth: 72
                                text: I18n.text(eventRow.modelData.state === "up" ? "macro.up" : "macro.down")
                                color: eventRow.active ? Theme.secondaryContainerContent : Theme.surfaceVariantContent
                            }

                            RowLayout {
                                Layout.preferredWidth: 96
                                spacing: 4

                                // Compact field: the Material TextField is taller than a table row.
                                Rectangle {
                                    Layout.preferredWidth: 64
                                    implicitHeight: 28
                                    radius: 8
                                    color: Theme.surfaceContainer
                                    border.width: delayInput.activeFocus ? 2 : 1
                                    border.color: delayInput.activeFocus ? Theme.primary : Theme.outlineVariant

                                    TextInput {
                                        id: delayInput
                                        anchors.fill: parent
                                        anchors.leftMargin: 8
                                        anchors.rightMargin: 8
                                        verticalAlignment: TextInput.AlignVCenter
                                        horizontalAlignment: TextInput.AlignRight
                                        text: String(eventRow.modelData.delay)
                                        validator: IntValidator { bottom: 0; top: 65535 }
                                        font.family: Theme.fontFamily
                                        font.pixelSize: 13
                                        color: Theme.surfaceContent
                                        selectByMouse: true
                                        onEditingFinished: {
                                            const delay = parseInt(text)
                                            text = Qt.binding(() => String(eventRow.modelData.delay))
                                            if (!isNaN(delay) && delay !== eventRow.modelData.delay) {
                                                const events = page.plainEvents()
                                                events[eventRow.index].delay = delay
                                                page.setEvents(events, eventRow.index)
                                            }
                                        }
                                    }
                                }

                                Label {
                                    text: I18n.text("macro.ms")
                                    font.pixelSize: 12
                                    color: Theme.surfaceVariantContent
                                }
                            }
                        }

                        TapHandler {
                            onTapped: page.selectedEvent = eventRow.index
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 4

                    Repeater {
                        model: [
                            { icon: "vertical_align_top", action: "top" },
                            { icon: "arrow_upward", action: "move_up" },
                            { icon: "arrow_downward", action: "move_down" },
                            { icon: "vertical_align_bottom", action: "bottom" },
                            { icon: "delete", action: "delete_event" }
                        ]

                        delegate: ToolButton {
                            required property var modelData
                            enabled: page.selectedEvent >= 0
                            contentItem: Icon {
                                name: parent.modelData.icon
                                size: 20
                                color: parent.enabled ? Theme.surfaceContent : Theme.outline
                            }
                            ToolTip.visible: hovered
                            ToolTip.text: I18n.text("macro." + modelData.action)
                            onClicked: page.editEvents(modelData.action)
                        }
                    }

                    ToolButton {
                        enabled: page.macro !== null && page.macro.events.length > 0
                        contentItem: Icon {
                            name: "clear_all"
                            size: 20
                            color: parent.enabled ? Theme.surfaceContent : Theme.outline
                        }
                        ToolTip.visible: hovered
                        ToolTip.text: I18n.text("macro.clear")
                        onClicked: page.setEvents([], -1)
                    }

                    Item { Layout.fillWidth: true }

                    Label {
                        text: I18n.text("macro.events_used").arg(page.macro ? page.macro.events.length : 0).arg(page.maxEvents)
                        font.pixelSize: 12
                        color: Theme.surfaceVariantContent
                    }
                }
            }
        }
    }

    NameDialog {
        id: nameDialog
    }

    AppDialog {
        id: deleteConfirm
        title: I18n.text("macro.delete")
        text: I18n.text("macro.delete_confirm").arg(page.macro ? page.macro.name : "")
        cancelText: I18n.text("dialog.cancel")
        onAccepted: {
            Macros.remove(page.selectedId)
            page.selectedId = page.firstId
        }
    }

    // "Please press a key": inserts one key-down or key-up event.
    Dialog {
        id: keyDialog

        property string eventState: "down"

        parent: Overlay.overlay
        anchors.centerIn: parent
        width: 360
        modal: true
        title: I18n.text("macro.press_key")
        Material.background: Theme.surfaceContainerHigh
        Material.foreground: Theme.surfaceContent
        Material.roundedScale: Material.ExtraLargeScale
        onOpened: keyCatcher.forceActiveFocus()

        contentItem: Item {
            id: keyCatcher
            implicitHeight: 64
            focus: true

            Icon {
                anchors.centerIn: parent
                name: "keyboard"
                size: 40
                color: Theme.primary
            }

            Keys.onPressed: event => {
                event.accepted = true
                const ev = page.eventFromKey(event)
                if (!ev)
                    return
                page.insertEvents([{ state: keyDialog.eventState, kind: ev.kind, code: ev.code, delay: page.defaultDelay }])
                keyDialog.close()
            }
        }

        footer: DialogButtonBox {
            Material.background: "transparent"

            Button {
                text: I18n.text("dialog.cancel")
                flat: true
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            }
        }
    }
}
