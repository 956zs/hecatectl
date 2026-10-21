import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Dialogs
import QtQuick.Layouts
import HecateGui

// Vendor tab "Performance Settings": sensor options on the left, DPI on the right (ui-features §2.1, §2.2).
ScrollView {
    id: page

    contentWidth: availableWidth
    clip: true

    readonly property var s: Model.s
    readonly property var options: Model.options

    GridLayout {
        width: page.availableWidth
        visible: Model.loaded
        enabled: Model.editable
        columns: page.availableWidth >= 900 ? 2 : 1
        columnSpacing: 16
        rowSpacing: 16

        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            spacing: 16

            Card {
                Layout.fillWidth: true
                title: I18n.text("performance.report_rate")
                icon: "speed"

                Label {
                    Layout.fillWidth: true
                    text: I18n.text("performance.report_rate_desc")
                    wrapMode: Text.Wrap
                    color: Theme.surfaceVariantContent
                }

                SegmentedButtons {
                    model: page.options.reportRates
                    current: page.s.reportRate
                    labelFor: hz => I18n.text("performance.rate_value").arg(hz)
                    onActivated: hz => Model.change({ reportRate: hz })
                }
            }

            Card {
                Layout.fillWidth: true

                SettingRow {
                    Layout.fillWidth: true
                    title: I18n.text("performance.mode")

                    SegmentedButtons {
                        model: page.options.powerModes
                        current: page.s.powerMode
                        labelFor: m => I18n.text(m === 0 ? "performance.mode_lp" : "performance.mode_hp")
                        onActivated: m => Model.change({ powerMode: m })
                    }
                }

                SettingRow {
                    Layout.fillWidth: true
                    title: I18n.text("performance.lod")

                    SegmentedButtons {
                        model: page.options.lods
                        current: page.s.lod
                        labelFor: mm => I18n.text("performance.lod_value").arg(mm)
                        onActivated: mm => Model.change({ lod: mm })
                    }
                }

                SettingRow {
                    Layout.fillWidth: true
                    title: I18n.text("performance.wave")
                    help: I18n.text("performance.wave_help")

                    ToggleSwitch {
                        checked: page.s.rippleControl
                        onToggled: on => Model.change({ rippleControl: on })
                    }
                }

                SettingRow {
                    Layout.fillWidth: true
                    title: I18n.text("performance.linear")
                    help: I18n.text("performance.linear_help")

                    ToggleSwitch {
                        checked: page.s.linearCorrection
                        onToggled: on => Model.change({ linearCorrection: on })
                    }
                }

                SettingRow {
                    Layout.fillWidth: true
                    title: I18n.text("performance.motion_sync")
                    help: I18n.text("performance.motion_sync_help")

                    ToggleSwitch {
                        checked: page.s.motionSync
                        onToggled: on => Model.change({ motionSync: on })
                    }
                }

                SettingRow {
                    Layout.fillWidth: true
                    title: I18n.text("performance.peak")
                    help: I18n.text("performance.peak_help")

                    // The sensor sleep time only applies with Peak Performance on (vendor gate).
                    DropDown {
                        implicitWidth: 120
                        enabled: page.s.peakPerformance
                        choices: Model.timeChoices(page.options.sensorSleepTimes)
                        current: page.s.sensorSleepTime
                        onChosen: t => Model.change({ sensorSleepTime: t })
                    }

                    ToggleSwitch {
                        checked: page.s.peakPerformance
                        onToggled: on => Model.change({ peakPerformance: on })
                    }
                }
            }
        }

        Card {
            id: dpiCard
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            title: I18n.text("dpi.title")
            icon: "tune"

            SettingRow {
                Layout.fillWidth: true
                title: I18n.text("dpi.quantity")

                DropDown {
                    implicitWidth: 100
                    choices: Array.from({ length: page.options.maxDpiStages }, (_, i) => ({ label: String(i + 1), value: i + 1 }))
                    current: page.s.dpiStageCount
                    onChosen: n => Model.change({ dpiStageCount: n })
                }
            }

            GridLayout {
                Layout.fillWidth: true
                columns: 3
                columnSpacing: 10
                rowSpacing: 10

                Repeater {
                    model: page.s.stages.slice(0, page.s.dpiStageCount)

                    delegate: StageCard {
                        required property var modelData
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        stage: modelData
                        current: index === page.s.currentDpiStage
                        onSelected: Model.change({ currentDpiStage: { stage: index, expect: Model.expectOf(modelData) } })
                        onDpiEntered: dpi => Model.change({ stageDpi: { stage: index, dpi: dpi, expect: Model.expectOf(modelData) } })
                        onColorClicked: {
                            colorDialog.stage = index
                            colorDialog.expect = Model.expectOf(modelData)
                            colorDialog.selectedColor = modelData.color
                            colorDialog.open()
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: 4

                Label {
                    Layout.fillWidth: true
                    text: I18n.text("dpi.adjustable")
                    font.weight: Font.Medium
                    color: Theme.surfaceContent
                }

                Label {
                    readonly property int position: slider.pressed ? Math.round(slider.value) : (Model.currentStage?.position ?? 0)
                    text: I18n.text("dpi.value").arg(Device.dpiAt(position))
                    font.weight: Font.DemiBold
                    color: Theme.primary
                }
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: 8

                Label {
                    text: I18n.text("dpi.slow")
                    font.pixelSize: 13
                    color: Theme.surfaceVariantContent
                }

                // Written on release only, like the vendor slider.
                Slider {
                    id: slider

                    readonly property int stagePosition: Model.currentStage?.position ?? 0
                    // The stage as it was when the drag started; the patch expects it unchanged.
                    property int dragStage: -1
                    property var dragExpect: null

                    Layout.fillWidth: true
                    from: 0
                    to: page.options.sliderMax
                    stepSize: 1
                    snapMode: Slider.SnapAlways
                    focusPolicy: Qt.NoFocus
                    value: stagePosition
                    Material.accent: Theme.primary

                    onPressedChanged: {
                        if (pressed) {
                            dragStage = page.s.currentDpiStage
                            dragExpect = Model.currentStage ? Model.expectOf(Model.currentStage) : null
                            return
                        }
                        const position = Math.round(value)
                        value = Qt.binding(() => stagePosition)
                        if (position !== stagePosition && dragExpect)
                            Model.change({ stagePosition: { stage: dragStage, position: position, expect: dragExpect } })
                    }
                }

                Label {
                    text: I18n.text("dpi.quick")
                    font.pixelSize: 13
                    color: Theme.surfaceVariantContent
                }
            }

            Label {
                Layout.fillWidth: true
                text: I18n.text("dpi.help")
                wrapMode: Text.Wrap
                font.pixelSize: 13
                color: Theme.surfaceVariantContent
            }

            // Vendor status line: report rate, mode, LOD.
            Flow {
                Layout.fillWidth: true
                Layout.topMargin: 4
                spacing: 8

                Repeater {
                    model: [
                        I18n.text("performance.status_rate").arg(page.s.reportRate),
                        I18n.text("performance.status_mode").arg(I18n.text(page.s.powerMode === 1 ? "performance.mode_hp" : "performance.mode_lp")),
                        I18n.text("performance.status_lod").arg(page.s.lod)
                    ]

                    delegate: Rectangle {
                        required property string modelData
                        implicitWidth: statusText.implicitWidth + 20
                        implicitHeight: 28
                        radius: 8
                        color: Theme.surfaceContainerHighest

                        Label {
                            id: statusText
                            anchors.centerIn: parent
                            text: parent.modelData
                            font.pixelSize: 12
                            color: Theme.surfaceVariantContent
                        }
                    }
                }
            }
        }
    }

    ColorDialog {
        id: colorDialog

        property int stage: 0
        property var expect: null

        title: I18n.text("dpi.color_title")
        onAccepted: Model.change({ stageColor: { stage: stage, color: selectedColor.toString(), expect: expect } })
    }
}
