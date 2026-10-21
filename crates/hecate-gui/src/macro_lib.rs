//! The PC-side macro library. As in the vendor app (docs/ui-features.md §3.4, §4), macros live on the
//! PC and one is copied into a button's macro slot when it is assigned. Each macro is one JSON file in
//! the library directory, named by a numeric id that keeps creation order.

use crate::keymap::{macro_event_json, parse_macro_event, LoopMode};
use hecate_core::slots::{MacroEvent, MACRO_EVENTS};
use serde_json::{json, Value};
use std::path::PathBuf;

const FORMAT: &str = "hecate-gui macro";
const VERSION: u64 = 1;
const ID_DIGITS: usize = 4;
/// A new macro plays once until the user picks another mode.
pub const DEFAULT_LOOP: LoopMode = LoopMode::Count(1);

#[derive(Debug, Clone, PartialEq)]
pub struct MacroDoc {
    pub name: String,
    pub loop_mode: LoopMode,
    pub events: Vec<MacroEvent>,
}

impl MacroDoc {
    /// The file contents.
    pub fn to_json(&self) -> Value {
        let mut v = json!({
            "format": FORMAT,
            "version": VERSION,
            "name": self.name,
            "events": self.events.iter().map(|e| json!({
                "state": match e.state { hecate_core::slots::KeyState::Up => "up", _ => "down" },
                "kind": e.kind,
                "code": e.code,
                "delay": e.delay_ms,
            })).collect::<Vec<_>>(),
        });
        v.as_object_mut().expect("object").extend(self.loop_mode.to_json().as_object().expect("object").clone());
        v
    }

    /// The view for the pages: events carry display names.
    pub fn to_view(&self, id: &str) -> Value {
        let mut v = json!({
            "id": id,
            "name": self.name,
            "events": self.events.iter().map(macro_event_json).collect::<Vec<_>>(),
        });
        v.as_object_mut().expect("object").extend(self.loop_mode.to_json().as_object().expect("object").clone());
        v
    }

    /// Parses a file, or a doc sent by the editor (`format`/`version` may then be absent).
    pub fn from_value(v: &Value, require_header: bool) -> Result<Self, String> {
        let obj = v.as_object().ok_or("not a JSON object")?;
        if require_header {
            if obj.get("format").and_then(Value::as_str) != Some(FORMAT) {
                return Err(format!("'format' is not \"{FORMAT}\""));
            }
            if obj.get("version").and_then(Value::as_u64) != Some(VERSION) {
                return Err(format!("'version' is not {VERSION}"));
            }
        }
        let name = obj.get("name").and_then(Value::as_str).ok_or("'name' must be a string")?.to_owned();
        let loop_mode = LoopMode::from_json(obj)?;
        let events = obj
            .get("events")
            .and_then(Value::as_array)
            .ok_or("'events' must be a list")?
            .iter()
            .enumerate()
            .map(|(i, e)| parse_macro_event(e).map_err(|err| format!("event {}: {err}", i + 1)))
            .collect::<Result<Vec<_>, _>>()?;
        if events.len() > *MACRO_EVENTS.end() {
            return Err(format!("{} events, at most {}", events.len(), MACRO_EVENTS.end()));
        }
        Ok(Self { name, loop_mode, events })
    }

    pub fn from_text(text: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("invalid JSON: {e}"))?;
        Self::from_value(&v, true)
    }
}

/// Library ids are digits only, so they can never point outside the directory.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 9 && id.bytes().all(|b| b.is_ascii_digit())
}

/// The id after the highest existing one.
pub fn next_id(existing: &[String]) -> String {
    let next = existing.iter().filter_map(|id| id.parse::<u32>().ok()).max().map_or(1, |n| n + 1);
    format!("{next:0ID_DIGITS$}")
}

/// Device macros not already in the library (same name and events), as new docs.
pub fn import(library: &[MacroDoc], device: &[(String, Vec<MacroEvent>)]) -> Vec<MacroDoc> {
    let mut out: Vec<MacroDoc> = Vec::new();
    for (name, events) in device {
        let known = library.iter().chain(&out).any(|d| d.name == *name && d.events == *events);
        if !known {
            out.push(MacroDoc { name: name.clone(), loop_mode: DEFAULT_LOOP, events: events.clone() });
        }
    }
    out
}

pub struct Library {
    dir: PathBuf,
}

impl Library {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err(format!("'{id}' is not a macro id"));
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    /// Every macro by id, in id order, and a message for each file that could not be read.
    pub fn load(&self) -> (Vec<(String, MacroDoc)>, Vec<String>) {
        let mut docs = Vec::new();
        let mut problems = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return (docs, problems) };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = path.file_stem().and_then(|s| s.to_str()).filter(|s| valid_id(s)) else { continue };
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| MacroDoc::from_text(&t)) {
                Ok(doc) => docs.push((id.to_owned(), doc)),
                Err(e) => problems.push(format!("{}: {e}", path.display())),
            }
        }
        docs.sort_by(|a, b| a.0.cmp(&b.0));
        (docs, problems)
    }

    /// Writes through a temporary file so a crash never leaves half a macro.
    pub fn save(&self, id: &str, doc: &MacroDoc) -> Result<(), String> {
        let path = self.path(id)?;
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(&doc.to_json()).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, text + "\n").map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Ids of every macro file, readable or not.
    fn ids(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).filter(|s| valid_id(s)).map(str::to_owned))
            .collect()
    }

    /// Saves `doc` under a new id (never one a broken file still holds) and returns it.
    pub fn create(&self, doc: &MacroDoc) -> Result<String, String> {
        let ids = self.ids();
        let id = next_id(&ids);
        self.save(&id, doc)?;
        Ok(id)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let path = self.path(id)?;
        std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hecate_core::slots::KeyState;

    fn ev(state: KeyState, kind: u8, code: u16, delay_ms: u16) -> MacroEvent {
        MacroEvent { state, kind, code, delay_ms }
    }

    fn sample() -> MacroDoc {
        MacroDoc {
            name: "切回去:D".into(),
            loop_mode: LoopMode::UntilReleased,
            events: vec![ev(KeyState::Down, 0, 1, 64), ev(KeyState::Down, 1, 0x50, 71), ev(KeyState::Up, 1, 0x50, 50), ev(KeyState::Up, 0, 1, 0)],
        }
    }

    /// A unique scratch directory, removed by the test.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hecate-gui-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn json_round_trip() {
        let doc = sample();
        let text = serde_json::to_string(&doc.to_json()).unwrap();
        assert_eq!(MacroDoc::from_text(&text), Ok(doc));
        let counted = MacroDoc { loop_mode: LoopMode::Count(250), ..sample() };
        assert_eq!(MacroDoc::from_text(&counted.to_json().to_string()), Ok(counted));
    }

    #[test]
    fn file_format() {
        let v = sample().to_json();
        assert_eq!(v["format"], FORMAT);
        assert_eq!(v["version"], 1);
        assert_eq!(v["loop"], "released");
        assert_eq!(v["events"][0], json!({ "state": "down", "kind": 0, "code": 1, "delay": 64 }));
    }

    #[test]
    fn view_names_events() {
        let v = sample().to_view("0003");
        assert_eq!(v["id"], "0003");
        assert_eq!(v["events"][1]["name"], "Left");
        assert_eq!(v["events"][0]["name"], "LCtrl");
    }

    #[test]
    fn malformed_files() {
        let cases = [
            ("", "invalid JSON"),
            ("[]", "not a JSON object"),
            (r#"{"format": "other", "version": 1}"#, "'format'"),
            (r#"{"format": "hecate-gui macro", "version": 2}"#, "'version'"),
            (r#"{"format": "hecate-gui macro", "version": 1, "loop": "released", "events": []}"#, "'name'"),
            (r#"{"format": "hecate-gui macro", "version": 1, "name": "a", "events": []}"#, "'loop'"),
            (r#"{"format": "hecate-gui macro", "version": 1, "name": "a", "loop": "released"}"#, "'events'"),
            (r#"{"format": "hecate-gui macro", "version": 1, "name": "a", "loop": "released", "events": [{"state": "pressed", "kind": 1, "code": 4, "delay": 0}]}"#, "event 1"),
            (r#"{"format": "hecate-gui macro", "version": 1, "name": "a", "loop": "count", "count": 0, "events": []}"#, "count"),
        ];
        for (text, needle) in cases {
            let e = MacroDoc::from_text(text).unwrap_err();
            assert!(e.contains(needle), "{text}: {e}");
        }
    }

    #[test]
    fn too_many_events() {
        let mut doc = sample();
        doc.events = vec![ev(KeyState::Down, 1, 4, 0); 71];
        assert!(MacroDoc::from_text(&doc.to_json().to_string()).unwrap_err().contains("71 events"));
        doc.events.truncate(70);
        assert!(MacroDoc::from_text(&doc.to_json().to_string()).is_ok());
    }

    #[test]
    fn editor_docs_need_no_header() {
        let v = json!({ "name": "a", "loop": "anyKey", "events": [] });
        assert!(MacroDoc::from_value(&v, false).is_ok());
        assert!(MacroDoc::from_value(&v, true).is_err());
    }

    #[test]
    fn ids() {
        assert!(valid_id("0001") && valid_id("12"));
        for bad in ["", "../x", "a1", "1.json", "1234567890", " 1"] {
            assert!(!valid_id(bad), "{bad:?}");
        }
        assert_eq!(next_id(&[]), "0001");
        assert_eq!(next_id(&["0001".into(), "0007".into(), "0003".into()]), "0008");
        assert_eq!(next_id(&["9999".into()]), "10000");
    }

    #[test]
    fn import_skips_known_macros() {
        let lib = vec![sample()];
        let device = vec![
            (sample().name, sample().events),
            ("切螢幕:D".to_string(), vec![ev(KeyState::Down, 1, 0x4F, 0), ev(KeyState::Up, 1, 0x4F, 0)]),
            ("切螢幕:D".to_string(), vec![ev(KeyState::Down, 1, 0x4F, 0), ev(KeyState::Up, 1, 0x4F, 0)]),
        ];
        let new = import(&lib, &device);
        assert_eq!(new.len(), 1);
        assert_eq!(new[0].name, "切螢幕:D");
        assert_eq!(new[0].loop_mode, DEFAULT_LOOP);
        assert!(import(&lib, &[]).is_empty());
    }

    #[test]
    fn library_files_round_trip() {
        let dir = scratch("lib");
        let lib = Library::new(dir.clone());
        assert_eq!(lib.load().0.len(), 0);
        let a = lib.create(&sample()).unwrap();
        let b = lib.create(&MacroDoc { name: "b".into(), ..sample() }).unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("0001", "0002"));
        std::fs::write(dir.join("0005.json"), "{ broken").unwrap();
        assert_eq!(lib.create(&sample()).unwrap(), "0006");
        lib.remove("0006").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        let (docs, problems) = lib.load();
        assert_eq!(docs.iter().map(|(id, d)| (id.as_str(), d.name.as_str())).collect::<Vec<_>>(), [("0001", "切回去:D"), ("0002", "b")]);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("0005.json"));
        lib.save(&a, &MacroDoc { name: "renamed".into(), ..sample() }).unwrap();
        assert_eq!(lib.load().0[0].1.name, "renamed");
        lib.remove(&b).unwrap();
        assert!(lib.remove("../x").is_err());
        assert_eq!(lib.load().0.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
