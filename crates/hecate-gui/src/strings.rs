//! UI text: the shipped `i18n/*.json` tables (compiled in) and locale selection.

use std::collections::HashMap;

/// Language code and table source, in the order offered when a locale only matches by language.
pub const LANGUAGES: [(&str, &str); 2] =
    [("zh_TW", include_str!("../i18n/zh_TW.json")), ("en", include_str!("../i18n/en.json"))];

fn table(language: &str) -> HashMap<String, String> {
    let (_, src) = LANGUAGES
        .iter()
        .find(|(code, _)| *code == language)
        .unwrap_or_else(|| panic!("no string table for '{language}'"));
    serde_json::from_str(src).unwrap_or_else(|e| panic!("i18n/{language}.json: {e}"))
}

pub struct Strings {
    language: &'static str,
    table: HashMap<String, String>,
    fallback: HashMap<String, String>,
}

impl Strings {
    /// Table for `language`, falling back to `default` for missing keys. Both must be available.
    pub fn new(language: &'static str, default: &str) -> Self {
        Self { language, table: table(language), fallback: table(default) }
    }

    /// Table for `forced`, or else picked from the process locale.
    pub fn from_env(forced: Option<&'static str>, default: &'static str) -> Self {
        if let Some(language) = forced {
            return Self::new(language, default);
        }
        let var = |name| std::env::var(name).ok();
        let vars = [var("LC_ALL"), var("LC_MESSAGES"), var("LANG")];
        let codes: Vec<&'static str> = LANGUAGES.iter().map(|(code, _)| *code).collect();
        Self::new(pick_language(vars.each_ref().map(Option::as_deref), &codes, default), default)
    }

    pub fn language(&self) -> &'static str {
        self.language
    }

    /// Text for `key`: the selected table, then the default language, then the key itself.
    pub fn get(&self, key: &str) -> String {
        self.table.get(key).or_else(|| self.fallback.get(key)).cloned().unwrap_or_else(|| key.to_owned())
    }
}

/// Picks a language for the POSIX locale variables `[LC_ALL, LC_MESSAGES, LANG]`.
///
/// The first non-empty variable decides, as in POSIX. Its `ll_CC` part (without `.codeset` or
/// `@modifier`) is matched exactly first, then by language alone; anything else gives `default`.
pub fn pick_language<'a>(vars: [Option<&str>; 3], available: &[&'a str], default: &'a str) -> &'a str {
    let Some(locale) = vars.into_iter().flatten().find(|v| !v.is_empty()) else { return default };
    let name = locale.split(['.', '@']).next().unwrap_or_default();
    let lang = |code: &str| code.split('_').next().unwrap_or_default().to_owned();
    available
        .iter()
        .find(|code| **code == name)
        .or_else(|| available.iter().find(|code| lang(code) == lang(name) && !name.is_empty()))
        .copied()
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AVAILABLE: [&str; 2] = ["zh_TW", "en"];

    fn pick(vars: [Option<&str>; 3]) -> &'static str {
        pick_language(vars, &AVAILABLE, "zh_TW")
    }

    #[test]
    fn picks_exact_locale() {
        assert_eq!(pick([None, None, Some("en_US.UTF-8")]), "en");
        assert_eq!(pick([None, None, Some("zh_TW.UTF-8")]), "zh_TW");
    }

    #[test]
    fn matches_by_language_when_region_differs() {
        assert_eq!(pick([None, None, Some("en_GB.UTF-8@euro")]), "en");
        assert_eq!(pick([None, None, Some("zh_HK.UTF-8")]), "zh_TW");
        assert_eq!(pick([None, None, Some("en")]), "en");
    }

    #[test]
    fn lc_all_then_lc_messages_then_lang() {
        assert_eq!(pick([Some("en_US.UTF-8"), Some("zh_TW.UTF-8"), Some("zh_TW.UTF-8")]), "en");
        assert_eq!(pick([None, Some("en_US.UTF-8"), Some("zh_TW.UTF-8")]), "en");
        assert_eq!(pick([Some(""), Some(""), Some("en_US.UTF-8")]), "en");
    }

    #[test]
    fn first_set_variable_decides_even_if_unsupported() {
        assert_eq!(pick([Some("fr_FR.UTF-8"), None, Some("en_US.UTF-8")]), "zh_TW");
    }

    #[test]
    fn defaults_for_unset_c_and_unknown_locales() {
        assert_eq!(pick([None, None, None]), "zh_TW");
        assert_eq!(pick([Some(""), Some(""), Some("")]), "zh_TW");
        assert_eq!(pick([None, None, Some("C")]), "zh_TW");
        assert_eq!(pick([None, None, Some("C.UTF-8")]), "zh_TW");
        assert_eq!(pick([None, None, Some("POSIX")]), "zh_TW");
        assert_eq!(pick([None, None, Some("de_DE.UTF-8")]), "zh_TW");
        assert_eq!(pick([None, None, Some(".UTF-8")]), "zh_TW");
        assert_eq!(pick_language([None, None, Some("en_US")], &[], "zh_TW"), "zh_TW");
    }

    #[test]
    fn shipped_tables_parse_and_share_keys() {
        let zh = table("zh_TW");
        let en = table("en");
        assert!(!zh.is_empty());
        let mut zh_keys: Vec<_> = zh.keys().collect();
        let mut en_keys: Vec<_> = en.keys().collect();
        zh_keys.sort();
        en_keys.sort();
        assert_eq!(zh_keys, en_keys);
        assert!(zh.values().chain(en.values()).all(|v| !v.trim().is_empty()));
    }

    #[test]
    fn lookup_uses_selected_table() {
        assert_eq!(Strings::new("en", "zh_TW").get("nav.device"), "Device");
        assert_eq!(Strings::new("zh_TW", "zh_TW").get("nav.device"), "裝置");
    }

    #[test]
    fn lookup_falls_back_to_default_language_then_key() {
        let mut s = Strings::new("en", "zh_TW");
        s.table.remove("nav.device");
        assert_eq!(s.get("nav.device"), "裝置");
        assert_eq!(s.get("no.such.key"), "no.such.key");
        assert_eq!(s.get(""), "");
    }
}
