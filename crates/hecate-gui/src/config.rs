//! App settings: the shipped `resources/config.json`, with `$XDG_CONFIG_HOME/hecate-gui/config.json`
//! merged over it when present.

use crate::macro_name::NameEncoding;
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
pub const AUTO: &str = "auto";

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
    /// Macro library directory; relative paths are under `$XDG_CONFIG_HOME` (default `~/.config`).
    pub macro_dir: PathBuf,
    /// Encoding of macro names written to the mouse.
    pub macro_name_encoding: NameEncoding,
}

pub fn get() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(load)
}

/// `$XDG_CONFIG_HOME/hecate-gui/config.json`.
pub fn user_config_path() -> Option<PathBuf> {
    xdg_dir(std::env::var_os("XDG_CONFIG_HOME"), std::env::var_os("HOME"), ".config").map(|d| d.join("hecate-gui/config.json"))
}

fn load() -> Config {
    let shipped = || parse(None).expect("resources/config.json is valid");
    let Some(path) = user_config_path() else { return shipped() };
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
        macro_dir: PathBuf::from(string("macro_dir")?),
        macro_name_encoding: NameEncoding::parse(&string("macro_name_encoding")?)
            .ok_or("'macro_name_encoding' must be \"big5\" or \"utf-8\"")?,
    })
}

/// The user config file contents with "language" set to `language` ("auto" or a shipped language),
/// every other key and value kept as they were. A missing or empty file gives a new one; a file that is
/// not a JSON object is refused rather than overwritten.
pub fn with_language(existing: Option<&str>, language: &str) -> Result<String, String> {
    if language != AUTO && !strings::LANGUAGES.iter().any(|(code, _)| *code == language) {
        return Err(format!("'{language}' is not a language setting"));
    }
    let mut map = match existing.filter(|t| !t.trim().is_empty()) {
        None => Map::new(),
        Some(text) => object(text).map_err(|e| format!("{e}; the file was left unchanged"))?,
    };
    map.insert("language".into(), Value::String(language.into()));
    serde_json::to_string_pretty(&Value::Object(map)).map(|t| t + "\n").map_err(|e| e.to_string())
}

/// Stores the language setting in the user config file at `path`, through a temporary file in the
/// same directory and a rename, so the file is never half written.
pub fn save_language(path: &Path, language: &str) -> Result<(), String> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let text = with_language(existing.as_deref(), language).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// An XDG base directory: the variable when it holds an absolute path, else `$HOME/<fallback>`.
pub fn xdg_dir(var: Option<OsString>, home: Option<OsString>, fallback: &str) -> Option<PathBuf> {
    match var.map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => Some(dir),
        _ => home.map(PathBuf::from).filter(|h| h.is_absolute()).map(|h| h.join(fallback)),
    }
}

impl Config {
    pub fn macro_dir_path(&self) -> Option<PathBuf> {
        if self.macro_dir.is_absolute() {
            return Some(self.macro_dir.clone());
        }
        xdg_dir(std::env::var_os("XDG_CONFIG_HOME"), std::env::var_os("HOME"), ".config").map(|d| d.join(&self.macro_dir))
    }

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

    /// A scratch directory under the system temp dir, never the real config.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hecate-gui-config-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn language_into_a_missing_or_empty_file() {
        for existing in [None, Some(""), Some("  \n")] {
            let text = with_language(existing, "zh_TW").unwrap();
            assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), serde_json::json!({ "language": "zh_TW" }));
        }
    }

    #[test]
    fn language_keeps_other_keys_and_their_order() {
        let existing = r#"{"device_poll_online_ms": 3000, "language": "en", "font_family": "Noto Sans"}"#;
        let text = with_language(Some(existing), "zh_TW").unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["device_poll_online_ms", "language", "font_family"]);
        assert_eq!(v["device_poll_online_ms"], 3000);
        assert_eq!(v["font_family"], "Noto Sans");
        assert_eq!(v["language"], "zh_TW");
        // The result still loads as a config.
        assert_eq!(parse(Some(&text)).unwrap().language, Some("zh_TW"));
    }

    #[test]
    fn each_language_setting() {
        for (setting, loaded) in [("auto", None), ("zh_TW", Some("zh_TW")), ("en", Some("en"))] {
            let text = with_language(None, setting).unwrap();
            assert_eq!(parse(Some(&text)).unwrap().language, loaded, "{setting}");
        }
        assert!(with_language(None, "fr").unwrap_err().contains("'fr'"));
        assert!(with_language(None, "").is_err());
    }

    #[test]
    fn invalid_files_are_refused() {
        for bad in ["{ broken", "[]", "\"zh_TW\"", "42"] {
            let e = with_language(Some(bad), "zh_TW").unwrap_err();
            assert!(e.contains("left unchanged"), "{bad}: {e}");
        }
    }

    #[test]
    fn save_language_creates_updates_and_refuses() {
        let dir = scratch("lang");
        let path = dir.join("hecate-gui/config.json");
        // Missing file and directory: created.
        save_language(&path, "en").unwrap();
        assert_eq!(parse(Some(&std::fs::read_to_string(&path).unwrap())).unwrap().language, Some("en"));
        // Existing keys survive an update.
        std::fs::write(&path, r#"{"theme_poll_ms": 4000, "language": "en"}"#).unwrap();
        save_language(&path, "auto").unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v, serde_json::json!({ "theme_poll_ms": 4000, "language": "auto" }));
        assert!(!path.with_extension("json.tmp").exists());
        // An invalid file is not touched.
        std::fs::write(&path, "{ broken").unwrap();
        assert!(save_language(&path, "zh_TW").unwrap_err().contains("left unchanged"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken");
        std::fs::remove_dir_all(&dir).unwrap();
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
        assert_eq!(c.macro_dir, Path::new("hecate-gui/macros"));
        assert_eq!(c.macro_name_encoding, NameEncoding::Big5);
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
        assert!(parse(Some(r#"{"macro_name_encoding": "gbk"}"#)).unwrap_err().contains("'macro_name_encoding'"));
        assert_eq!(parse(Some(r#"{"macro_name_encoding": "utf-8"}"#)).unwrap().macro_name_encoding, NameEncoding::Utf8);
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
