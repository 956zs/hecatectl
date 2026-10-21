//! Codes used in shortcut and macro events (docs/ui-features.md §3.2, §3.3, §4): HID keyboard usages
//! (type 1), modifier masks (type 0), consumer usages (type 2) and mouse button masks (type 4), plus the
//! Linux evdev code of each key so keys pressed in the window can be recorded. Key cap names follow the
//! vendor's key table (FUN_14000c240) and are not translated; consumer and mouse names are i18n keys.

use hecate_core::slots::{EVENT_CONSUMER, EVENT_KEY, EVENT_MODIFIER, EVENT_MOUSE};
use serde_json::{json, Value};

/// `(HID usage, evdev code, name)` of the keyboard keys (type 1). Usages from the HID usage tables,
/// keyboard page; evdev codes from linux/input-event-codes.h.
const KEYS: &[(u16, u16, &str)] = &[
    (0x04, 30, "A"), (0x05, 48, "B"), (0x06, 46, "C"), (0x07, 32, "D"), (0x08, 18, "E"), (0x09, 33, "F"),
    (0x0A, 34, "G"), (0x0B, 35, "H"), (0x0C, 23, "I"), (0x0D, 36, "J"), (0x0E, 37, "K"), (0x0F, 38, "L"),
    (0x10, 50, "M"), (0x11, 49, "N"), (0x12, 24, "O"), (0x13, 25, "P"), (0x14, 16, "Q"), (0x15, 19, "R"),
    (0x16, 31, "S"), (0x17, 20, "T"), (0x18, 22, "U"), (0x19, 47, "V"), (0x1A, 17, "W"), (0x1B, 45, "X"),
    (0x1C, 21, "Y"), (0x1D, 44, "Z"),
    (0x1E, 2, "1"), (0x1F, 3, "2"), (0x20, 4, "3"), (0x21, 5, "4"), (0x22, 6, "5"), (0x23, 7, "6"),
    (0x24, 8, "7"), (0x25, 9, "8"), (0x26, 10, "9"), (0x27, 11, "0"),
    (0x28, 28, "Enter"), (0x29, 1, "Esc"), (0x2A, 14, "Backspace"), (0x2B, 15, "Tab"), (0x2C, 57, "Space"),
    (0x2D, 12, "-"), (0x2E, 13, "="), (0x2F, 26, "["), (0x30, 27, "]"), (0x31, 43, "\\"),
    (0x33, 39, ";"), (0x34, 40, "'"), (0x35, 41, "`"), (0x36, 51, ","), (0x37, 52, "."), (0x38, 53, "/"),
    (0x39, 58, "CapsLock"),
    (0x3A, 59, "F1"), (0x3B, 60, "F2"), (0x3C, 61, "F3"), (0x3D, 62, "F4"), (0x3E, 63, "F5"), (0x3F, 64, "F6"),
    (0x40, 65, "F7"), (0x41, 66, "F8"), (0x42, 67, "F9"), (0x43, 68, "F10"), (0x44, 87, "F11"), (0x45, 88, "F12"),
    (0x46, 99, "PrintScreen"), (0x47, 70, "ScrollLock"), (0x48, 119, "Pause"), (0x49, 110, "Insert"),
    (0x4A, 102, "Home"), (0x4B, 104, "PgUp"), (0x4C, 111, "Del"), (0x4D, 107, "End"), (0x4E, 109, "PgDn"),
    (0x4F, 106, "Right"), (0x50, 105, "Left"), (0x51, 108, "Down"), (0x52, 103, "Up"),
    (0x53, 69, "NumLock"), (0x54, 98, "Num /"), (0x55, 55, "Num *"), (0x56, 74, "Num -"), (0x57, 78, "Num +"),
    (0x58, 96, "Num Enter"), (0x59, 79, "Num 1"), (0x5A, 80, "Num 2"), (0x5B, 81, "Num 3"), (0x5C, 75, "Num 4"),
    (0x5D, 76, "Num 5"), (0x5E, 77, "Num 6"), (0x5F, 71, "Num 7"), (0x60, 72, "Num 8"), (0x61, 73, "Num 9"),
    (0x62, 82, "Num 0"), (0x63, 83, "Num ."), (0x64, 86, "K45"), (0x65, 127, "Apps"), (0x67, 117, "Num ="),
    (0x68, 183, "F13"), (0x69, 184, "F14"), (0x6A, 185, "F15"), (0x6B, 186, "F16"), (0x6C, 187, "F17"),
    (0x6D, 188, "F18"), (0x6E, 189, "F19"), (0x6F, 190, "F20"), (0x70, 191, "F21"), (0x71, 192, "F22"),
    (0x72, 193, "F23"), (0x73, 194, "F24"), (0x85, 121, "K107"), (0x87, 89, "Ro"), (0x89, 124, "Yen"),
];

/// Usage 0x32 (non-US #) has no evdev code of its own: Linux reports it as KEY_BACKSLASH.
const NON_US_HASH: (u16, &str) = (0x32, "K42");

/// `(mask, evdev code, name)` of the modifiers (type 0), as in the vendor table (FUN_14000bab0).
const MODIFIERS: [(u16, u16, &str); 8] = [
    (0x01, 29, "LCtrl"),
    (0x02, 42, "LShift"),
    (0x04, 56, "LAlt"),
    (0x08, 125, "LWin"),
    (0x10, 97, "RCtrl"),
    (0x20, 54, "RShift"),
    (0x40, 100, "RAlt"),
    (0x80, 126, "RWin"),
];

/// Media keys on the keyboard become consumer events (type 2), like the vendor key table does.
const MEDIA_KEYS: [(u16, u16); 7] = [(113, 0xE2), (114, 0xEA), (115, 0xE9), (166, 0xB7), (164, 0xCD), (165, 0xB6), (163, 0xB5)];

/// The "Multimedia Function" items in vendor order: i18n key and consumer usage (FUN_14000bc10).
pub const MULTIMEDIA: [(&str, u16); 18] = [
    ("media.player", 0x0183),
    ("media.play_pause", 0x00CD),
    ("media.next", 0x00B5),
    ("media.previous", 0x00B6),
    ("media.stop", 0x00B7),
    ("media.mute", 0x00E2),
    ("media.volume_up", 0x00E9),
    ("media.volume_down", 0x00EA),
    ("media.mail", 0x018A),
    ("media.calculator", 0x0192),
    ("media.my_computer", 0x0194),
    ("media.home", 0x0223),
    ("media.search", 0x0221),
    ("media.web_forward", 0x0225),
    ("media.web_back", 0x0224),
    ("media.web_stop", 0x0226),
    ("media.refresh", 0x0227),
    ("media.favorites", 0x022A),
];

/// Mouse buttons: mask (type 1 key binding p1 and type 4 macro event code) and i18n key.
pub const MOUSE_BUTTONS: [(u16, &str); 5] =
    [(1, "mouse.left"), (2, "mouse.right"), (4, "mouse.wheel"), (8, "mouse.back"), (16, "mouse.forward")];

/// Combination-key modifier choices (radio `Shift` / `Ctrl` / `Alt` / `Win`): the left-hand masks.
pub const COMBINATION_MODIFIERS: [(u16, &str); 4] = [(0x02, "Shift"), (0x01, "Ctrl"), (0x04, "Alt"), (0x08, "Win")];

/// A key pressed in the window, by evdev code, as an event `(type, code)`.
pub fn from_evdev(evdev: u16) -> Option<(u8, u16)> {
    if let Some(&(mask, _, _)) = MODIFIERS.iter().find(|m| m.1 == evdev) {
        return Some((EVENT_MODIFIER, mask));
    }
    if let Some(&(_, usage)) = MEDIA_KEYS.iter().find(|m| m.0 == evdev) {
        return Some((EVENT_CONSUMER, usage));
    }
    KEYS.iter().find(|k| k.1 == evdev).map(|k| (EVENT_KEY, k.0))
}

fn modifier_name(mask: u16) -> Option<String> {
    let names: Vec<&str> = MODIFIERS.iter().filter(|m| mask & m.0 != 0).map(|m| m.2).collect();
    (mask != 0 && mask <= 0xFF && !names.is_empty()).then(|| names.join("+"))
}

fn key_name(usage: u16) -> Option<&'static str> {
    if usage == NON_US_HASH.0 {
        return Some(NON_US_HASH.1);
    }
    KEYS.iter().find(|k| k.0 == usage).map(|k| k.2)
}

/// How to show an event's code: `{"name": ...}` for key caps and modifiers, `{"nameKey": ...}` for
/// i18n names, and the raw hex code when unknown.
pub fn event_name(kind: u8, code: u16) -> Value {
    let named = |name: Option<String>| name.map(|n| json!({ "name": n }));
    let i18n = |key: Option<&str>| key.map(|k| json!({ "nameKey": k }));
    let found = match kind {
        EVENT_MODIFIER => named(modifier_name(code)),
        EVENT_KEY => named(key_name(code).map(str::to_owned)),
        EVENT_CONSUMER => i18n(MULTIMEDIA.iter().find(|m| m.1 == code).map(|m| m.0)),
        EVENT_MOUSE => i18n(MOUSE_BUTTONS.iter().find(|m| m.0 == code).map(|m| m.1)),
        _ => None,
    };
    found.unwrap_or_else(|| json!({ "name": format!("{kind}:0x{code:04X}") }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn tables_have_no_duplicates() {
        let usages: HashSet<u16> = KEYS.iter().map(|k| k.0).collect();
        assert_eq!(usages.len(), KEYS.len());
        let evdev: HashSet<u16> = KEYS.iter().map(|k| k.1).chain(MODIFIERS.iter().map(|m| m.1)).chain(MEDIA_KEYS.iter().map(|m| m.0)).collect();
        assert_eq!(evdev.len(), KEYS.len() + MODIFIERS.len() + MEDIA_KEYS.len());
        assert!(!usages.contains(&NON_US_HASH.0));
        let media: HashSet<u16> = MULTIMEDIA.iter().map(|m| m.1).collect();
        assert_eq!(media.len(), 18);
    }

    #[test]
    fn known_usages() {
        // Spot checks against the vendor key table and the HID usage tables.
        assert_eq!(from_evdev(1), Some((EVENT_KEY, 0x29))); // Esc
        assert_eq!(from_evdev(59), Some((EVENT_KEY, 0x3A))); // F1
        assert_eq!(from_evdev(30), Some((EVENT_KEY, 0x04))); // A
        assert_eq!(from_evdev(11), Some((EVENT_KEY, 0x27))); // 0
        assert_eq!(from_evdev(105), Some((EVENT_KEY, 0x50))); // Left, as in the owner's macro
        assert_eq!(from_evdev(106), Some((EVENT_KEY, 0x4F))); // Right
        assert_eq!(from_evdev(96), Some((EVENT_KEY, 0x58))); // keypad Enter
        assert_eq!(from_evdev(194), Some((EVENT_KEY, 0x73))); // F24
    }

    #[test]
    fn modifiers_and_media_keys() {
        assert_eq!(from_evdev(29), Some((EVENT_MODIFIER, 0x01)));
        assert_eq!(from_evdev(125), Some((EVENT_MODIFIER, 0x08)));
        assert_eq!(from_evdev(126), Some((EVENT_MODIFIER, 0x80)));
        assert_eq!(from_evdev(54), Some((EVENT_MODIFIER, 0x20)));
        assert_eq!(from_evdev(115), Some((EVENT_CONSUMER, 0xE9)));
        assert_eq!(from_evdev(113), Some((EVENT_CONSUMER, 0xE2)));
    }

    #[test]
    fn unknown_evdev_codes() {
        assert_eq!(from_evdev(0), None);
        assert_eq!(from_evdev(0x2ff), None);
    }

    #[test]
    fn multimedia_list_matches_the_vendor() {
        assert_eq!(MULTIMEDIA[0], ("media.player", 0x0183));
        assert_eq!(MULTIMEDIA[6], ("media.volume_up", 0x00E9));
        assert_eq!(MULTIMEDIA[7], ("media.volume_down", 0x00EA));
        assert_eq!(MULTIMEDIA[17], ("media.favorites", 0x022A));
    }

    #[test]
    fn event_names() {
        assert_eq!(event_name(EVENT_KEY, 0x50), json!({ "name": "Left" }));
        assert_eq!(event_name(EVENT_KEY, 0x32), json!({ "name": "K42" }));
        assert_eq!(event_name(EVENT_MODIFIER, 0x01), json!({ "name": "LCtrl" }));
        assert_eq!(event_name(EVENT_MODIFIER, 0x09), json!({ "name": "LCtrl+LWin" }));
        assert_eq!(event_name(EVENT_CONSUMER, 0xE9), json!({ "nameKey": "media.volume_up" }));
        assert_eq!(event_name(EVENT_MOUSE, 8), json!({ "nameKey": "mouse.back" }));
    }

    #[test]
    fn unknown_codes_show_raw() {
        assert_eq!(event_name(EVENT_KEY, 0x01), json!({ "name": "1:0x0001" }));
        assert_eq!(event_name(EVENT_MODIFIER, 0), json!({ "name": "0:0x0000" }));
        assert_eq!(event_name(EVENT_MODIFIER, 0x100), json!({ "name": "0:0x0100" }));
        assert_eq!(event_name(EVENT_CONSUMER, 0x1234), json!({ "name": "2:0x1234" }));
        assert_eq!(event_name(EVENT_MOUSE, 3), json!({ "name": "4:0x0003" }));
        assert_eq!(event_name(9, 1), json!({ "name": "9:0x0001" }));
    }
}
