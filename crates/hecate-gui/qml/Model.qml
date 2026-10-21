pragma Singleton
import QtQuick
import HecateGui

// The settings view and the constant options from Device, parsed once, and the change paths back.
QtObject {
    readonly property var settings: Device.settingsJson.length > 0 ? JSON.parse(Device.settingsJson) : null
    readonly property var options: JSON.parse(Device.optionsJson)
    readonly property bool loaded: settings !== null && Device.loaded
    // Same keys as the view; used until the first read so bindings stay defined.
    readonly property var blank: ({
        reportRate: 0, powerMode: -1, lod: 0, rippleControl: false, linearCorrection: false,
        motionSync: false, peakPerformance: false, sensorSleepTime: 0, mouseSleepTime: 0,
        dpiStageCount: 0, currentDpiStage: -1, stages: []
    })
    readonly property var s: settings !== null ? settings : blank
    readonly property var currentStage: s.stages[s.currentDpiStage] ?? null
    // Input only with settings read from an awake mouse and no write in flight, so a second change
    // cannot queue behind one that moves things (review R3).
    readonly property bool editable: Device.online && loaded && !Device.busy

    // Buttons 0..4 (keymap.rs), the macros in the device slots, and the PC macro library.
    readonly property var keys: Device.keysJson.length > 0 ? JSON.parse(Device.keysJson) : []
    readonly property var deviceMacros: Device.deviceMacrosJson.length > 0 ? JSON.parse(Device.deviceMacrosJson) : []
    readonly property var macros: Macros.listJson.length > 0 ? JSON.parse(Macros.listJson) : []

    // A message for the snackbar (Main.qml shows it).
    signal notify(string text)

    // What a stage patch expects the stage to still hold (settings_map.rs StageExpect).
    function expectOf(stage) {
        return { reg: stage.reg, color: stage.color }
    }

    function change(patch) {
        Device.change(JSON.stringify(patch))
    }

    function assign(key, fn) {
        Device.assign(JSON.stringify({ key: key, "function": fn }))
    }

    // Vendor time labels: "30s", "1Min".
    function timeLabel(seconds) {
        return seconds < 60 ? I18n.text("time.seconds").arg(seconds) : I18n.text("time.minutes").arg(seconds / 60)
    }

    // A Macros problem (`{key, arg}` or `{text}`) as text.
    function problemText(json) {
        const p = JSON.parse(json)
        if (!p.key)
            return p.text
        const text = I18n.text(p.key)
        return text.includes("%1") ? text.arg(p.arg) : text
    }

    function timeChoices(list) {
        return list.map(t => ({ label: timeLabel(t.seconds), value: t.value }))
    }

    // An event's code as text: key cap names come from Rust, the rest from the string tables.
    function eventName(ev) {
        return ev.nameKey ? I18n.text(ev.nameKey) : ev.name
    }

    // "LCtrl + C" from a combination's key-down events.
    function comboText(events) {
        return events.filter(e => e.state === "down").map(eventName).join(I18n.text("keys.combo_join"))
    }

    function loopText(fn) {
        return fn.loop === "count" ? I18n.text("loop.count_value").arg(fn.count) : I18n.text("loop." + fn.loop)
    }

    // What a button does, for its card.
    function functionLabel(fn) {
        switch (fn.category) {
        case "mouse": return eventName({ nameKey: options.mouseButtons.find(b => b.value === fn.button)?.nameKey })
        case "dpi": return I18n.text(["", "dpi.loop", "dpi.up", "dpi.down"][fn.action])
        case "scrollV": return I18n.text(fn.dir === 1 ? "scroll.up" : "scroll.down")
        case "scrollH": return I18n.text(fn.dir === 1 ? "scroll.left" : "scroll.right")
        case "fire": return I18n.text("fire.value").arg(fn.interval).arg(fn.count)
        case "report": return I18n.text("report.key")
        case "multimedia": return I18n.text(options.multimedia.find(m => m.usage === fn.usage)?.nameKey ?? "value.unknown")
        case "combination": return fn.valid ? comboText(fn.events) : I18n.text("combination.empty")
        case "macro": return fn.valid ? fn.name : I18n.text("macro.slot_empty")
        case "disabled": return I18n.text("cat.disabled")
        default: return I18n.text("cat.unknown_value").arg(fn.kind).arg(fn.p1).arg(fn.p2)
        }
    }
}
