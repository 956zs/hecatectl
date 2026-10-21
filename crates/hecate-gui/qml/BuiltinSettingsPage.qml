import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import HecateGui

// Vendor tab "Built-in Settings" (ui-features §2.3). "Windows Mouse Property" is Windows-only and skipped.
ScrollView {
    id: page

    contentWidth: availableWidth
    clip: true

    readonly property var s: Model.s
    readonly property var options: Model.options

    GridLayout {
        width: page.availableWidth
        columns: page.availableWidth >= 900 ? 2 : 1
        columnSpacing: 16
        rowSpacing: 16

        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            spacing: 16

            // App setting, not a mouse setting: always available.
            Card {
                Layout.fillWidth: true
                title: I18n.text("language.title")
                icon: "translate"

                ChoiceGrid {
                    Layout.fillWidth: true
                    equalWidths: false
                    columns: 3
                    choices: ["auto", "zh_TW", "en"].map(l => ({ label: I18n.text("language." + l), value: l }))
                    current: StringTable.setting
                    onChosen: l => StringTable.setLanguage(l)
                }
            }

            Card {
                Layout.fillWidth: true
                visible: Model.loaded
                enabled: Model.editable
                title: I18n.text("builtin.sleep_mode")
                icon: "bedtime"

                SettingRow {
                    Layout.fillWidth: true
                    title: I18n.text("builtin.sleep_time")

                    DropDown {
                        choices: Model.timeChoices(page.options.mouseSleepTimes)
                        current: page.s.mouseSleepTime
                        onChosen: t => Model.change({ mouseSleepTime: t })
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                visible: Model.loaded
                enabled: Model.editable && Device.longRange >= 0
                title: I18n.text("builtin.advanced")
                icon: "settings_input_antenna"

                SettingRow {
                    Layout.fillWidth: true
                    title: I18n.text("builtin.long_range")
                    help: I18n.text("builtin.long_range_help")

                    ToggleSwitch {
                        checked: Device.longRange === 1
                        onToggled: on => Device.setLongRange(on)
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                visible: Device.present
                title: I18n.text("builtin.set_default")
                icon: "restart_alt"

                TonalButton {
                    icon: "restart_alt"
                    text: I18n.text("builtin.reset")
                    enabled: Device.online && !Device.busy
                    onClicked: resetConfirm.open()
                }
            }

            // Pairing runs through the 2.4G receiver.
            Card {
                Layout.fillWidth: true
                visible: Device.present && Device.connection === "receiver"
                title: I18n.text("builtin.pairing")
                icon: "link"

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 16

                    TonalButton {
                        icon: "link"
                        text: I18n.text("builtin.start_pairing")
                        enabled: !Device.busy
                        onClicked: pairConfirm.open()
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        visible: Device.pairing
                        spacing: 6

                        Label {
                            text: I18n.text("builtin.pairing_running")
                            color: Theme.surfaceVariantContent
                        }

                        ProgressBar {
                            Layout.fillWidth: true
                            value: Device.pairingProgress
                            Material.accent: Theme.primary
                        }
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            Layout.alignment: Qt.AlignTop
            spacing: 16

            Card {
                Layout.fillWidth: true
                title: I18n.text("builtin.device_info")
                icon: "info"

                InfoRow {
                    Layout.fillWidth: true
                    icon: Format.statusIcon
                    iconColor: Device.present && Device.online ? Theme.primary : Theme.surfaceVariantContent
                    label: I18n.text("status.title")
                    value: Format.statusText
                }

                InfoRow {
                    Layout.fillWidth: true
                    icon: Format.connectionIcon
                    label: I18n.text("connection.title")
                    value: Format.connectionText
                }

                InfoRow {
                    Layout.fillWidth: true
                    icon: Format.batteryIcon
                    iconColor: Device.charging ? Theme.primary : Theme.surfaceVariantContent
                    label: I18n.text("battery.title")
                    value: Format.batteryText
                }

                InfoRow {
                    Layout.fillWidth: true
                    visible: Device.receiverFirmware.length > 0
                    icon: "router"
                    label: I18n.text("builtin.receiver_firmware")
                    value: Device.receiverFirmware
                }

                InfoRow {
                    Layout.fillWidth: true
                    icon: "memory"
                    label: I18n.text("builtin.mouse_firmware")
                    value: Format.firmwareText
                }

                TonalButton {
                    Layout.alignment: Qt.AlignRight
                    Layout.topMargin: 4
                    icon: "refresh"
                    text: Device.busy ? I18n.text("busy") : I18n.text("builtin.refresh")
                    enabled: Device.present && !Device.busy
                    onClicked: Device.refresh()
                }
            }

            Card {
                Layout.fillWidth: true
                title: I18n.text("builtin.operation")
                icon: "menu_book"

                SettingRow {
                    Layout.fillWidth: true
                    title: I18n.text("builtin.instructions")

                    TonalButton {
                        icon: "open_in_new"
                        text: I18n.text("builtin.product_spec")
                        onClicked: Qt.openUrlExternally(page.options.productUrl)
                    }
                }
            }
        }
    }

    // The vendor resets without asking; we confirm first.
    AppDialog {
        id: resetConfirm
        title: I18n.text("builtin.reset")
        text: I18n.text("builtin.reset_confirm")
        cancelText: I18n.text("dialog.cancel")
        onAccepted: Device.factoryReset()
    }

    AppDialog {
        id: resetDone
        title: I18n.text("builtin.reset_done_title")
        text: I18n.text("builtin.reset_done")
    }

    AppDialog {
        id: pairConfirm
        title: I18n.text("builtin.pairing")
        text: I18n.text("builtin.pairing_confirm")
        okText: I18n.text("builtin.start_pairing")
        cancelText: I18n.text("dialog.cancel")
        onAccepted: Device.startPairing()
    }

    AppDialog {
        id: pairResult
        title: I18n.text("builtin.pairing")
    }

    Connections {
        target: Device

        function onResetFinished() {
            resetDone.open()
        }

        function onPairingFinished(success) {
            pairResult.text = I18n.text(success ? "builtin.pairing_success" : "builtin.pairing_failed")
            pairResult.open()
        }
    }
}
