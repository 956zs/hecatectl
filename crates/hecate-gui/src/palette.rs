//! Material You palette from matugen's colors.json (written by the Quickshell "ii" shell), with the
//! shipped dark palette in `resources/theme-fallback.json` filling in whatever is missing or invalid.

use serde_json::Value;
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::Path;
use std::time::{Duration, SystemTime};

const FALLBACK: &str = include_str!("../resources/theme-fallback.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Parses `#rrggbb`, or `#aarrggbb` as Qt writes it.
pub fn parse_hex(s: &str) -> Option<Rgba> {
    let hex = s.strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    let [a, r, g, b] = v.to_be_bytes();
    match hex.len() {
        6 => Some(Rgba { r, g, b, a: 0xff }),
        8 => Some(Rgba { r, g, b, a }),
        _ => None,
    }
}

/// Colour roles keyed like the colors.json entries. The fallback file decides which roles exist.
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    colors: BTreeMap<String, Rgba>,
}

impl Palette {
    pub fn fallback() -> Self {
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(FALLBACK) else {
            panic!("resources/theme-fallback.json is not a JSON object");
        };
        let colors = map
            .iter()
            .map(|(role, v)| {
                let rgba = v.as_str().and_then(parse_hex);
                (role.clone(), rgba.unwrap_or_else(|| panic!("resources/theme-fallback.json: bad colour for '{role}'")))
            })
            .collect();
        Self { colors }
    }

    /// Overlays the roles found in `text` on the fallback palette. Returns the palette and a
    /// description of every role that could not be taken from `text`.
    pub fn from_json(text: &str) -> (Self, Vec<String>) {
        let mut palette = Self::fallback();
        if text.trim().is_empty() {
            return (palette, vec!["empty file".into()]);
        }
        let map = match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(map)) => map,
            Ok(_) => return (palette, vec!["not a JSON object".into()]),
            Err(e) => return (palette, vec![format!("invalid JSON: {e}")]),
        };
        let mut problems = Vec::new();
        for (role, color) in palette.colors.iter_mut() {
            match map.get(role) {
                None => problems.push(format!("'{role}' is missing")),
                Some(Value::String(s)) => match parse_hex(s) {
                    Some(c) => *color = c,
                    None => problems.push(format!("'{role}' has an invalid colour '{s}'")),
                },
                Some(_) => problems.push(format!("'{role}' is not a string")),
            }
        }
        (palette, problems)
    }

    /// Reads `path`; a missing file silently gives the fallback, other problems are logged.
    pub fn load(path: Option<&Path>) -> Self {
        let Some(path) = path else { return Self::fallback() };
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let (palette, problems) = Self::from_json(&text);
                if !problems.is_empty() {
                    eprintln!("hecate-gui: {}: {}; using fallback colours there", path.display(), problems.join(", "));
                }
                palette
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Self::fallback(),
            Err(e) => {
                eprintln!("hecate-gui: {}: {e}; using fallback colours", path.display());
                Self::fallback()
            }
        }
    }

    pub fn get(&self, role: &str) -> Option<Rgba> {
        self.colors.get(role).copied()
    }

    /// Whether the background is dark (relative luminance below one half).
    pub fn is_dark(&self) -> bool {
        let Some(c) = self.get("background") else { return true };
        let lin = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b) < 0.5
    }
}

fn modified(path: Option<&Path>) -> Option<SystemTime> {
    std::fs::metadata(path?).and_then(|m| m.modified()).ok()
}

/// Loads `path` and returns the palette with the file's mtime, for [`watch`].
pub fn load_stamped(path: Option<&Path>) -> (Palette, Option<SystemTime>) {
    let stamp = modified(path);
    (Palette::load(path), stamp)
}

/// Polls the mtime of `path` every `interval` and hands each changed palette to `on_change`.
/// Returns when `on_change` returns false.
pub fn watch(path: Option<&Path>, mut last: Option<SystemTime>, interval: Duration, mut on_change: impl FnMut(Palette) -> bool) {
    loop {
        std::thread::sleep(interval);
        let stamp = modified(path);
        if stamp != last {
            last = stamp;
            if !on_change(Palette::load(path)) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgba = Rgba { r: 0xff, g: 0, b: 0, a: 0xff };

    #[test]
    fn parses_hex_colours() {
        assert_eq!(parse_hex("#ff0000"), Some(RED));
        assert_eq!(parse_hex("#C5c5D6"), Some(Rgba { r: 0xc5, g: 0xc5, b: 0xd6, a: 0xff }));
        assert_eq!(parse_hex("#80112233"), Some(Rgba { r: 0x11, g: 0x22, b: 0x33, a: 0x80 }));
    }

    #[test]
    fn rejects_bad_hex() {
        for bad in ["", "#", "ff0000", "#ff000", "#ff00000", "#gg0000", "#+f0000", "# f0000", "#ff0000 ", "red"] {
            assert_eq!(parse_hex(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn fallback_is_complete_and_dark() {
        let p = Palette::fallback();
        for role in ["background", "surface_container", "primary", "on_surface", "outline", "error"] {
            assert!(p.get(role).is_some(), "{role}");
        }
        assert!(p.is_dark());
    }

    #[test]
    fn full_file_overrides_every_role() {
        let fallback = Palette::fallback();
        let text = format!(
            "{{{}}}",
            fallback.colors.keys().map(|k| format!("\"{k}\": \"#ff0000\"")).collect::<Vec<_>>().join(",")
        );
        let (p, problems) = Palette::from_json(&text);
        assert!(problems.is_empty(), "{problems:?}");
        assert!(p.colors.values().all(|c| *c == RED));
    }

    #[test]
    fn missing_keys_keep_fallback_and_are_reported() {
        let (p, problems) = Palette::from_json(r##"{"primary": "#ff0000", "extra_key": "#00ff00"}"##);
        assert_eq!(p.get("primary"), Some(RED));
        assert_eq!(p.get("background"), Palette::fallback().get("background"));
        assert_eq!(p.get("extra_key"), None);
        assert!(problems.iter().any(|m| m.contains("'background' is missing")));
        assert!(!problems.iter().any(|m| m.starts_with("'primary'")));
    }

    #[test]
    fn invalid_values_keep_fallback() {
        let (p, problems) = Palette::from_json(r##"{"primary": "#zzzzzz", "background": 12, "surface": null}"##);
        let fb = Palette::fallback();
        assert_eq!(p.get("primary"), fb.get("primary"));
        assert_eq!(p.get("background"), fb.get("background"));
        assert_eq!(p.get("surface"), fb.get("surface"));
        assert!(problems.iter().any(|m| m.contains("'primary' has an invalid colour '#zzzzzz'")));
        assert!(problems.iter().any(|m| m.contains("'background' is not a string")));
        assert!(problems.iter().any(|m| m.contains("'surface' is not a string")));
    }

    #[test]
    fn empty_and_whitespace_files_give_fallback() {
        for text in ["", "  \n"] {
            let (p, problems) = Palette::from_json(text);
            assert_eq!(p, Palette::fallback());
            assert_eq!(problems, ["empty file"]);
        }
    }

    #[test]
    fn non_object_json_gives_fallback() {
        for text in ["[]", "\"#ff0000\"", "42", "null"] {
            let (p, problems) = Palette::from_json(text);
            assert_eq!(p, Palette::fallback(), "{text}");
            assert_eq!(problems, ["not a JSON object"], "{text}");
        }
    }

    #[test]
    fn broken_json_gives_fallback() {
        let (p, problems) = Palette::from_json("{\"primary\": ");
        assert_eq!(p, Palette::fallback());
        assert!(problems[0].starts_with("invalid JSON"));
    }

    #[test]
    fn light_background_is_not_dark() {
        let (p, _) = Palette::from_json(r##"{"background": "#fdf8fd"}"##);
        assert!(!p.is_dark());
    }

    #[test]
    fn load_handles_missing_path_and_file() {
        assert_eq!(Palette::load(None), Palette::fallback());
        assert_eq!(Palette::load(Some(Path::new("/nonexistent/hecate-gui/colors.json"))), Palette::fallback());
    }
}
