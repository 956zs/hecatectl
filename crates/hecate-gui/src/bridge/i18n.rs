//! `I18n` QML singleton: UI text from the shipped string tables.

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
        #[qproperty(QString, language, READ, CONSTANT)]
        type I18n = super::I18nRust;

        /// Text for `key` in the selected language.
        #[qinvokable]
        fn text(&self, key: &QString) -> QString;
    }
}

use crate::config;
use crate::strings::Strings;
use cxx_qt_lib::QString;

pub struct I18nRust {
    language: QString,
    strings: Strings,
}

impl Default for I18nRust {
    fn default() -> Self {
        let cfg = config::get();
        let strings = Strings::from_env(cfg.language, cfg.default_language);
        Self { language: QString::from(strings.language()), strings }
    }
}

impl qobject::I18n {
    pub fn text(&self, key: &QString) -> QString {
        QString::from(&self.strings.get(&key.to_string()))
    }
}
