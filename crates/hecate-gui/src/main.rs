//! hecate-gui: settings app for the HECATE G3M PRO (updated) mouse.

mod bridge;
mod config;
mod display;
mod dpi_slider;
mod palette;
mod settings_map;
mod strings;
mod worker;

use cxx_qt_lib::{
    QGuiApplication, QMap, QMapPair_QString_QVariant, QQmlApplicationEngine, QQuickStyle, QString, QUrl, QVariant,
};
use std::process::ExitCode;

/// Main.qml in the `HecateGui` QML module registered by build.rs.
const MAIN_QML: &str = "qrc:/qt/qml/HecateGui/qml/Main.qml";
/// Base Qt Quick Controls style; the QML recolours it from `Theme`.
const STYLE: &str = "Material";
/// Matches data/hecate-gui.desktop; also the Wayland app id.
const APP_ID: &str = "hecate-gui";

/// Development options. Everything else on the command line is left to Qt.
#[derive(Debug, Default, PartialEq)]
struct Options {
    /// `--screenshot PATH`: save the window as an image once the settings are loaded (at most
    /// 20 s after start), then quit.
    /// With `QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software` nothing appears on screen.
    screenshot: Option<String>,
    /// `--page N`: start on navigation entry N.
    page: i32,
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--screenshot" => {
                opts.screenshot = Some(it.next().filter(|p| !p.is_empty()).ok_or("--screenshot needs a path")?.clone())
            }
            "--page" => {
                let n = it.next().ok_or("--page needs a number")?;
                opts.page = n.parse().ok().filter(|n| *n >= 0).ok_or_else(|| format!("--page: '{n}' is not a page number"))?;
            }
            _ => {}
        }
    }
    Ok(opts)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match parse_options(&args) {
        Ok(opts) => opts,
        Err(e) => {
            eprintln!("hecate-gui: {e}");
            return ExitCode::FAILURE;
        }
    };

    QQuickStyle::set_style(&QString::from(STYLE));
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();
    let (Some(mut app), Some(mut engine)) = (app.as_mut(), engine.as_mut()) else {
        eprintln!("hecate-gui: cannot create the Qt application");
        return ExitCode::FAILURE;
    };
    app.as_mut().set_application_name(&QString::from(APP_ID));
    QGuiApplication::set_desktop_file_name(&QString::from(APP_ID));

    let mut initial = QMap::<QMapPair_QString_QVariant>::default();
    initial.insert(QString::from("startPage"), QVariant::from(&opts.page));
    if let Some(path) = &opts.screenshot {
        initial.insert(QString::from("screenshotPath"), QVariant::from(&QString::from(path)));
    }
    engine.as_mut().set_initial_properties(&initial);
    engine
        .as_mut()
        .on_object_creation_failed(|_, url| {
            eprintln!("hecate-gui: cannot load {url}");
            std::process::exit(1);
        })
        .release();
    engine.as_mut().load(&QUrl::from(MAIN_QML));

    match app.exec() {
        0 => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        parse_options(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn no_options() {
        assert_eq!(parse(&[]), Ok(Options::default()));
    }

    #[test]
    fn screenshot_and_page() {
        let o = parse(&["--page", "1", "--screenshot", "/tmp/s.png"]).unwrap();
        assert_eq!(o, Options { screenshot: Some("/tmp/s.png".into()), page: 1 });
    }

    #[test]
    fn qt_arguments_are_ignored() {
        assert_eq!(parse(&["-platform", "offscreen", "--page", "0"]), Ok(Options::default()));
    }

    #[test]
    fn missing_or_bad_values() {
        assert!(parse(&["--screenshot"]).is_err());
        assert!(parse(&["--screenshot", ""]).is_err());
        assert!(parse(&["--page"]).is_err());
        assert!(parse(&["--page", "x"]).is_err());
        assert!(parse(&["--page", "-1"]).is_err());
    }
}
