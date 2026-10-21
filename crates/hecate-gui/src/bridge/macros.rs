//! `Macros` QML singleton: the PC-side macro library (see macro_lib.rs). Files are small, so the
//! library is read and written on the GUI thread.

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
        #[qproperty(QString, list_json, READ, NOTIFY)]
        type Macros = super::MacrosRust;

        /// New empty macro named `name`: its id, or an empty string after `failed`.
        #[qinvokable]
        fn create(self: Pin<&mut Self>, name: &QString) -> QString;

        /// Saves an edited macro `{name, loop, count?, events}`; false after `failed`.
        #[qinvokable]
        fn save(self: Pin<&mut Self>, id: &QString, doc: &QString) -> bool;

        #[qinvokable]
        fn remove(self: Pin<&mut Self>, id: &QString);

        /// Adds the device macros (`Device.deviceMacrosJson`) not yet in the library; returns how many.
        #[qinvokable]
        fn import_device(self: Pin<&mut Self>, device_macros: &QString) -> i32;

        /// Why `name` cannot be stored, as `{"key", "arg"}`, or an empty string when it can.
        #[qinvokable]
        fn name_problem(&self, name: &QString) -> QString;

        /// `{"key", "arg"}` (i18n) or `{"text"}`.
        #[qsignal]
        fn failed(self: Pin<&mut Self>, problem: QString);
    }

    impl cxx_qt::Initialize for Macros {}
}

use crate::config;
use crate::keymap::parse_macro_event;
use crate::macro_lib::{self, Library, MacroDoc, DEFAULT_LOOP};
use crate::macro_name::{self, NameEncoding, NameError};
use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use serde_json::{json, Value};

pub struct MacrosRust {
    list_json: QString,
    library: Library,
    encoding: NameEncoding,
}

impl Default for MacrosRust {
    fn default() -> Self {
        let cfg = config::get();
        Self {
            list_json: QString::default(),
            library: Library::new(cfg.macro_dir_path().unwrap_or_else(|| cfg.macro_dir.clone())),
            encoding: cfg.macro_name_encoding,
        }
    }
}

fn name_problem_json(e: &NameError) -> Value {
    let (key, arg) = e.message();
    json!({ "key": key, "arg": arg })
}

/// Device macros `[{slot, name, events}]` as (name, events).
fn parse_device_macros(text: &str) -> Result<Vec<(String, Vec<hecate_core::slots::MacroEvent>)>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("device macros: {e}"))?;
    v.as_array()
        .ok_or("device macros: not a list")?
        .iter()
        .map(|m| {
            let name = m.get("name").and_then(Value::as_str).ok_or("device macro without a name")?.to_owned();
            let events = m
                .get("events")
                .and_then(Value::as_array)
                .ok_or("device macro without events")?
                .iter()
                .map(parse_macro_event)
                .collect::<Result<Vec<_>, _>>()?;
            Ok((name, events))
        })
        .collect()
}

impl qobject::Macros {
    fn reload(mut self: Pin<&mut Self>) {
        let (docs, problems) = self.library.load();
        for p in problems {
            eprintln!("hecate-gui: macro library: {p}");
        }
        let list: Vec<Value> = docs.iter().map(|(id, doc)| doc.to_view(id)).collect();
        let json = QString::from(&Value::Array(list).to_string());
        if self.list_json != json {
            self.as_mut().rust_mut().list_json = json;
            self.as_mut().list_json_changed();
        }
    }

    fn fail(self: Pin<&mut Self>, problem: Value) {
        self.failed(QString::from(&problem.to_string()));
    }

    fn check_name(self: Pin<&mut Self>, name: &str) -> bool {
        match macro_name::encode(name, self.encoding) {
            Ok(_) => true,
            Err(e) => {
                self.fail(name_problem_json(&e));
                false
            }
        }
    }

    pub fn create(mut self: Pin<&mut Self>, name: &QString) -> QString {
        let name = name.to_string();
        if !self.as_mut().check_name(&name) {
            return QString::default();
        }
        let doc = MacroDoc { name: name.trim().to_owned(), loop_mode: DEFAULT_LOOP, events: Vec::new() };
        match self.library.create(&doc) {
            Ok(id) => {
                self.as_mut().reload();
                QString::from(&id)
            }
            Err(e) => {
                self.fail(json!({ "text": e }));
                QString::default()
            }
        }
    }

    pub fn save(mut self: Pin<&mut Self>, id: &QString, doc: &QString) -> bool {
        let parsed = serde_json::from_str::<Value>(&doc.to_string())
            .map_err(|e| e.to_string())
            .and_then(|v| MacroDoc::from_value(&v, false));
        let doc = match parsed {
            Ok(doc) => doc,
            Err(e) => {
                self.fail(json!({ "text": e }));
                return false;
            }
        };
        if !self.as_mut().check_name(&doc.name) {
            return false;
        }
        let saved = self.library.save(&id.to_string(), &doc);
        self.as_mut().reload();
        match saved {
            Ok(()) => true,
            Err(e) => {
                self.fail(json!({ "text": e }));
                false
            }
        }
    }

    pub fn remove(mut self: Pin<&mut Self>, id: &QString) {
        let removed = self.library.remove(&id.to_string());
        self.as_mut().reload();
        if let Err(e) = removed {
            self.fail(json!({ "text": e }));
        }
    }

    pub fn import_device(mut self: Pin<&mut Self>, device_macros: &QString) -> i32 {
        let device = match parse_device_macros(&device_macros.to_string()) {
            Ok(d) => d,
            Err(e) => {
                self.fail(json!({ "text": e }));
                return 0;
            }
        };
        let existing: Vec<MacroDoc> = self.library.load().0.into_iter().map(|(_, d)| d).collect();
        let mut added = 0;
        for doc in macro_lib::import(&existing, &device) {
            match self.library.create(&doc) {
                Ok(_) => added += 1,
                Err(e) => {
                    self.as_mut().fail(json!({ "text": e }));
                    break;
                }
            }
        }
        self.as_mut().reload();
        added
    }

    pub fn name_problem(&self, name: &QString) -> QString {
        match macro_name::encode(&name.to_string(), self.encoding) {
            Ok(_) => QString::default(),
            Err(e) => QString::from(&name_problem_json(&e).to_string()),
        }
    }
}

impl cxx_qt::Initialize for qobject::Macros {
    fn initialize(self: Pin<&mut Self>) {
        self.reload();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_macro_list_parses() {
        let text = r#"[{"slot": 3, "name": "切回去:D", "events": [{"state": "down", "kind": 0, "code": 1, "delay": 64, "name": "LCtrl"}]}]"#;
        let list = parse_device_macros(text).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "切回去:D");
        assert_eq!(list[0].1[0].delay_ms, 64);
        assert!(parse_device_macros("{}").is_err());
        assert!(parse_device_macros(r#"[{"name": "a"}]"#).is_err());
    }

    #[test]
    fn name_problems_name_the_i18n_key() {
        assert_eq!(name_problem_json(&NameError::TooLong(31)), json!({ "key": "macro.name_too_long", "arg": "30" }));
    }
}
