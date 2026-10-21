import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import HecateGui

ApplicationWindow {
    id: window

    width: 1180
    height: 800
    minimumWidth: 800
    minimumHeight: 560
    visible: true
    title: I18n.text("app.title")
    color: Theme.background
    font.family: Theme.fontFamily
    font.pixelSize: 15

    // Set from the command line by main.rs.
    property int startPage: 0
    property string screenshotPath

    Material.theme: Theme.dark ? Material.Dark : Material.Light
    Material.accent: Theme.primary
    Material.primary: Theme.primary
    Material.background: Theme.background
    Material.foreground: Theme.surfaceContent

    // Everything visible, as one item so --screenshot can grab it.
    Rectangle {
        id: surface
        anchors.fill: parent
        color: Theme.background

        RowLayout {
            anchors.fill: parent
            anchors.margins: 12
            anchors.leftMargin: 4
            spacing: 4

            NavRail {
                id: rail
                Layout.fillHeight: true
                currentIndex: window.startPage
                model: [
                    { label: "nav.performance", icon: "speed" },
                    { label: "nav.builtin", icon: "settings" }
                ]
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                radius: 24
                color: Theme.surfaceContainerLow

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 20
                    spacing: 16

                    HeaderBar {
                        Layout.fillWidth: true
                    }

                    StatusBanner {
                        Layout.fillWidth: true
                    }

                    StackLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        currentIndex: rail.currentIndex

                        PerformancePage {}
                        BuiltinSettingsPage {}
                    }
                }
            }
        }

        Snackbar {
            id: snackbar
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 28
        }
    }

    // --screenshot: grab once the settings are loaded (plus a moment to settle), or after 20 s.
    Timer {
        property int ticks: 0

        running: window.screenshotPath.length > 0
        interval: 500
        repeat: true
        onTriggered: {
            ticks += 1
            if (ticks < 4 || (!Device.loaded && ticks < 40))
                return
            stop()
            if (!Device.loaded)
                console.warn("screenshot taken before the settings were read")
            surface.grabToImage(result => {
                if (!result.saveToFile(window.screenshotPath))
                    console.warn("cannot save", window.screenshotPath)
                Qt.quit()
            })
        }
    }

    Connections {
        target: Device
        function onErrorOccurred(message) {
            snackbar.show(I18n.text("error.message").arg(message))
        }
    }
}
