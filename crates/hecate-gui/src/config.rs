//! App settings: the shipped `resources/config.json`, with `$XDG_CONFIG_HOME/hecate-gui/config.json`
//! merged over it when present.

use crate::strings;
use serde_json::{Map, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

const SHIPPED: &str = include_str!("../resources/config.json");
/// Floor for the poll intervals so polling never keeps the receiver busy.
const MIN_POLL: Duration = Duration::from_millis(500);
const MAX_POLL: Duration = Duration::from_secs(60);
/// `language` value that follows the locale.
const AUTO: &str = "auto";

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Device re-probe interval while no mouse answers (none found, or asleep); vendor: 1 s.
    pub device_poll_offline: Duration,
    /// Re-probe interval while the mouse is online; vendor: 5 s.
    pub device_poll_online: Duration,
    pub theme_poll: Duration,
    /// colors.json; relative paths are under `$XDG_STATE_HOME` (default `~/.local/state`).
    pub theme_colors_file: PathBuf,
    /// Forced UI language; None follows the locale.
    pub language: Option<&'static str>,
    /// Language for unmatched locales and for keys missing from a table.
    pub default_language: &'static str,
    pub font_family: String,
    pub icon_font_family: String,
    /// "Product Specification" page (vendor FUN_140243010).
    pub product_url: String,
}

pub fn get() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(load)
}

fn load() -> Config {
    let shipped = || parse(None).expect("resources/config.json is valid");
    let Some(path) = xdg_dir(std::env::var_os("XDG_CONFIG_HOME"), std::env::var_os("HOME"), ".config")
        .map(|d| d.join("hecate-gui/config.json"))
    else {
        return shipped();
    };
    let Ok(text) = std::fs::read_to_string(&path) else { return shipped() };
    parse(Some(&text)).unwrap_or_else(|e| {
        eprintln!("hecate-gui: {}: {e}; using the shipped settings", path.display());
        shipped()
    })
}

fn object(text: &str) -> Result<Map<String, Value>, String> {
    match serde_json::from_str(text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err("not a JSON object".into()),
        Err(e) => Err(format!("invalid JSON: {e}")),
    }
}

/// Merges the `user` JSON object over the shipped settings and validates the result.
pub fn parse(user: Option<&str>) -> Result<Config, String> {
    let mut merged = object(SHIPPED)?;
    if let Some(text) = user {
        for (key, value) in object(text)? {
            if !merged.contains_key(&key) {
                return Err(format!("unknown key '{key}'"));
            }
            merged.insert(key, value);
        }
    }
    let string = |key: &str| match merged.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        _ => Err(format!("'{key}' must be a string")),
    };
    let poll = |key: &str| match merged.get(key).and_then(Value::as_u64) {
        Some(ms) => Ok(Duration::from_millis(ms).clamp(MIN_POLL, MAX_POLL)),
        None => Err(format!("'{key}' must be a whole number of milliseconds")),
    };
    let shipped_language = |key: &str| {
        let value = string(key)?;
        strings::LANGUAGES
            .iter()
            .map(|(code, _)| *code)
            .find(|code| *code == value)
            .ok_or_else(|| format!("'{key}' must be one of the shipped languages, not '{value}'"))
    };
    let language = match string("language")?.as_str() {
        AUTO => None,
        _ => Some(shipped_language("language")?),
    };
    let default_language = shipped_language("default_language")?;
    Ok(Config {
        device_poll_offline: poll("device_poll_offline_ms")?,
        device_poll_online: poll("device_poll_online_ms")?,
        theme_poll: poll("theme_poll_ms")?,
        theme_colors_file: PathBuf::from(string("theme_colors_file")?),
        language,
        default_language,
        font_family: string("font_family")?,
        icon_font_family: string("icon_font_family")?,
        product_url: string("product_url")?,
    })
}

/// An XDG base directory: the variable when it holds an absolute path, else `$HOME/<fallback>`.
pub fn xdg_dir(var: Option<OsString>, home: Option<OsString>, fallback: &str) -> Option<PathBuf> {
    match var.map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => Some(dir),
        _ => home.map(PathBuf::from).filter(|h| h.is_absolute()).map(|h| h.join(fallback)),
    }
}

impl Config {
    pub fn theme_colors_path(&self) -> Option<PathBuf> {
        resolve_state_path(&self.theme_colors_file, std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
    }
}

fn resolve_state_path(file: &Path, xdg_state: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    if file.is_absolute() {
        return Some(file.to_path_buf());
    }
    xdg_dir(xdg_state, home, ".local/state").map(|d| d.join(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(s: &str) -> Option<OsString> {
        Some(OsString::from(s))
    }

    #[test]
    fn shipped_settings_are_valid() {
        let c = parse(None).unwrap();
        assert_eq!(c.device_poll_offline, Duration::from_secs(1));
        assert_eq!(c.device_poll_online, Duration::from_secs(5));
        assert_eq!(c.theme_poll, Duration::from_secs(2));
        assert_eq!(c.default_language, "zh_TW");
        assert_eq!(c.language, None);
        assert_eq!(c.theme_colors_file, Path::new("quickshell/user/generated/colors.json"));
        assert!(c.product_url.starts_with("https://"));
    }

    #[test]
    fn user_values_override_shipped_ones() {
        let c = parse(Some(r#"{"device_poll_online_ms": 3000, "default_language": "en"}"#)).unwrap();
        assert_eq!(c.device_poll_online, Duration::from_secs(3));
        assert_eq!(c.device_poll_offline, Duration::from_secs(1));
        assert_eq!(c.default_language, "en");
        assert_eq!(c.theme_poll, Duration::from_secs(2));
        assert_eq!(parse(Some("{}")).unwrap(), parse(None).unwrap());
    }

    #[test]
    fn language_can_be_forced() {
        assert_eq!(parse(Some(r#"{"language": "zh_TW"}"#)).unwrap().language, Some("zh_TW"));
        assert_eq!(parse(Some(r#"{"language": "auto"}"#)).unwrap().language, None);
    }

    #[test]
    fn poll_intervals_are_clamped() {
        let c = parse(Some(r#"{"device_poll_offline_ms": 0, "theme_poll_ms": 99999999}"#)).unwrap();
        assert_eq!(c.device_poll_offline, MIN_POLL);
        assert_eq!(c.theme_poll, MAX_POLL);
    }

    #[test]
    fn errors_name_the_bad_key() {
        assert_eq!(parse(Some(r#"{"poll": 1}"#)).unwrap_err(), "unknown key 'poll'");
        assert!(parse(Some(r#"{"device_poll_online_ms": -5}"#)).unwrap_err().contains("'device_poll_online_ms'"));
        assert!(parse(Some(r#"{"device_poll_offline_ms": "1000"}"#)).unwrap_err().contains("'device_poll_offline_ms'"));
        assert!(parse(Some(r#"{"font_family": 3}"#)).unwrap_err().contains("'font_family'"));
        assert!(parse(Some(r#"{"default_language": "fr"}"#)).unwrap_err().contains("'default_language'"));
        assert!(parse(Some(r#"{"language": "fr"}"#)).unwrap_err().contains("'language'"));
        assert!(parse(Some(r#"{"language": ""}"#)).unwrap_err().contains("'language'"));
    }

    #[test]
    fn rejects_non_object_and_broken_json() {
        assert_eq!(parse(Some("[]")).unwrap_err(), "not a JSON object");
        assert!(parse(Some("")).unwrap_err().starts_with("invalid JSON"));
        assert!(parse(Some("{")).unwrap_err().starts_with("invalid JSON"));
    }

    #[test]
    fn xdg_dir_prefers_absolute_variable() {
        assert_eq!(xdg_dir(os("/x/state"), os("/home/u"), ".local/state"), Some(PathBuf::from("/x/state")));
        assert_eq!(xdg_dir(os("rel/state"), os("/home/u"), ".local/state"), Some(PathBuf::from("/home/u/.local/state")));
        assert_eq!(xdg_dir(os(""), os("/home/u"), ".config"), Some(PathBuf::from("/home/u/.config")));
        assert_eq!(xdg_dir(None, None, ".config"), None);
        assert_eq!(xdg_dir(None, os("relative"), ".config"), None);
    }

    #[test]
    fn state_paths_resolve() {
        let rel = Path::new("quickshell/user/generated/colors.json");
        assert_eq!(
            resolve_state_path(rel, None, os("/home/u")),
            Some(PathBuf::from("/home/u/.local/state/quickshell/user/generated/colors.json"))
        );
        assert_eq!(resolve_state_path(rel, os("/s"), os("/home/u")), Some(PathBuf::from("/s/quickshell/user/generated/colors.json")));
        assert_eq!(resolve_state_path(Path::new("/abs/c.json"), None, None), Some(PathBuf::from("/abs/c.json")));
        assert_eq!(resolve_state_path(rel, None, None), None);
    }
}
