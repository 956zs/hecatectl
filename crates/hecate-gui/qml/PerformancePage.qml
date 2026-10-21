import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HecateGui

ScrollView {
    id: page

    contentWidth: availableWidth
    clip: true

    ColumnLayout {
        width: page.availableWidth
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
                Layout.topMargin: 4
                model: Device.reportRates
                current: Device.reportRate
                labelFor: hz => I18n.text("performance.rate_value").arg(hz)
                enabled: Device.online && !Device.busy
                onActivated: hz => Device.setReportRate(hz)
            }

            Label {
                visible: Device.present && !Device.online
                text: I18n.text("performance.offline_hint")
                font.pixelSize: 13
                color: Theme.surfaceVariantContent
            }
        }
    }
}
