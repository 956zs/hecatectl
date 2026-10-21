use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

const QML_FILES: &[&str] = &[
    "qml/Main.qml",
    "qml/NavRail.qml",
    "qml/HeaderBar.qml",
    "qml/StatusBanner.qml",
    "qml/Card.qml",
    "qml/Icon.qml",
    "qml/InfoRow.qml",
    "qml/SegmentedButtons.qml",
    "qml/TonalButton.qml",
    "qml/Snackbar.qml",
    "qml/SettingRow.qml",
    "qml/ToggleSwitch.qml",
    "qml/DropDown.qml",
    "qml/StageCard.qml",
    "qml/AppDialog.qml",
    "qml/PerformancePage.qml",
    "qml/BuiltinSettingsPage.qml",
];
const QML_SINGLETONS: &[&str] = &["qml/Format.qml", "qml/Model.qml"];

fn main() {
    let files = QML_FILES
        .iter()
        .map(|f| QmlFile::from(*f))
        .chain(QML_SINGLETONS.iter().map(|f| QmlFile::from(*f).singleton(true)));
    let builder = CxxQtBuilder::new_qml_module(QmlModule::new("HecateGui").qml_files(files))
        .qt_module("Quick")
        .files(["src/bridge/device.rs", "src/bridge/i18n.rs", "src/bridge/theme.rs"]);
    // SAFETY: only adds a warning flag. GCC 16 warns about QChar inside the Qt 6.11 headers.
    let builder = unsafe { builder.cc_builder(|cc| {
        cc.flag_if_supported("-Wno-sfinae-incomplete");
    }) };
    builder.build();
}
