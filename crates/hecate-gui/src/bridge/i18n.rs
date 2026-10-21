//! `StringTable` QML singleton: UI text from the shipped string tables, and the language setting.
//! QML reads text through the `I18n` singleton (qml/I18n.qml), which ties every lookup to `language`
//! so a switch re-evaluates all text bindings at once.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, language, READ, NOTIFY)]
        #[qproperty(QString, setting, READ, NOTIFY)]
        type StringTable = super::StringTableRust;

        /// Text for `key` in the language in use.
        #[qinvokable]
        fn text(&self, key: &QString) -> QString;

        /// Switches to "auto" (follow the locale), "zh_TW" or "en" and stores the choice in the user
        /// config file. Nothing changes when the file cannot be written (see `failed`).
        #[qinvokable]
        fn set_language(self: Pin<&mut Self>, setting: &QString);

        #[qsignal]
        fn failed(self: Pin<&mut Self>, message: QString);
    }
}

use crate::config::{self, AUTO};
use crate::strings::{Strings, LANGUAGES};
use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

pub struct StringTableRust {
    /// The table in use: "zh_TW" or "en".
    language: QString,
    /// The stored choice: "auto", "zh_TW" or "en".
    setting: QString,
    strings: Strings,
}

/// The table for a setting: a shipped language, or the locale for "auto".
fn strings_for(setting: &str) -> Option<Strings> {
    let default = config::get().default_language;
    if setting == AUTO {
        return Some(Strings::from_env(None, default));
    }
    LANGUAGES.iter().map(|(code, _)| *code).find(|code| *code == setting).map(|code| Strings::new(code, default))
}

impl Default for StringTableRust {
    fn default() -> Self {
        let cfg = config::get();
        let strings = Strings::from_env(cfg.language, cfg.default_language);
        Self {
            language: QString::from(strings.language()),
            setting: QString::from(cfg.language.unwrap_or(AUTO)),
            strings,
        }
    }
}

impl qobject::StringTable {
    pub fn text(&self, key: &QString) -> QString {
        QString::from(&self.strings.get(&key.to_string()))
    }

    pub fn set_language(mut self: Pin<&mut Self>, setting: &QString) {
        let setting = setting.to_string();
        let Some(strings) = strings_for(&setting) else {
            let message = self.strings.get("language.save_failed").replace("%1", &format!("'{setting}'"));
            self.failed(QString::from(&message));
            return;
        };
        let saved = config::user_config_path()
            .ok_or_else(|| "no config directory ($XDG_CONFIG_HOME and $HOME are unset)".to_string())
            .and_then(|path| config::save_language(&path, &setting));
        if let Err(e) = saved {
            eprintln!("hecate-gui: {e}");
            let message = self.strings.get("language.save_failed").replace("%1", &e);
            self.failed(QString::from(&message));
            return;
        }
        let language = QString::from(strings.language());
        self.as_mut().rust_mut().strings = strings;
        let setting = QString::from(&setting);
        if self.setting != setting {
            self.as_mut().rust_mut().setting = setting;
            self.as_mut().setting_changed();
        }
        if self.language != language {
            self.as_mut().rust_mut().language = language;
            self.as_mut().language_changed();
        }
    }
}
