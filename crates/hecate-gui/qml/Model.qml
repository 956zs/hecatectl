pragma Singleton
import QtQuick
import HecateGui

// The settings view and the constant options from Device, parsed once, and the change path back.
QtObject {
    readonly property var settings: Device.settingsJson.length > 0 ? JSON.parse(Device.settingsJson) : null
    readonly property var options: JSON.parse(Device.optionsJson)
    readonly property bool loaded: settings !== null
    // Same keys as the view; used until the first read so bindings stay defined.
    readonly property var blank: ({
        reportRate: 0, powerMode: -1, lod: 0, rippleControl: false, linearCorrection: false,
        motionSync: false, peakPerformance: false, sensorSleepTime: 0, mouseSleepTime: 0,
        dpiStageCount: 0, currentDpiStage: -1, stages: []
    })
    readonly property var s: loaded ? settings : blank
    readonly property var currentStage: s.stages[s.currentDpiStage] ?? null
    // Input only with settings read from an awake mouse. Changes made while a write runs queue up
    // in the worker and apply to the settings read back by then.
    readonly property bool editable: Device.online && loaded

    function change(patch) {
        Device.change(JSON.stringify(patch))
    }

    // Vendor time labels: "30s", "1Min".
    function timeLabel(seconds) {
        return seconds < 60 ? I18n.text("time.seconds").arg(seconds) : I18n.text("time.minutes").arg(seconds / 60)
    }

    function timeChoices(list) {
        return list.map(t => ({ label: timeLabel(t.seconds), value: t.value }))
    }
}
