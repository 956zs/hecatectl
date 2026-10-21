//! Button functions (docs/ui-features.md §3): `Keys[i] = (type, p1, p2)` <-> what the KeySet page shows,
//! the assignments it sends, and the write plan for each. A shortcut or macro slot is always written
//! (and verified) before the key that plays it is bound, so a key never points at a stale slot.

use crate::hid;
use crate::macro_name::{self, NameEncoding};
use hecate_core::settings::{KeyBinding, Settings};
use hecate_core::slots::{
    KeyEvent, KeyState, Macro, MacroEvent, Shortcut, Slots, EVENT_CONSUMER, EVENT_KEY, EVENT_MODIFIER, EVENT_MOUSE, MACRO_EVENTS,
};
use serde_json::{json, Map, Value};

/// Keys[0..5]: left, right, wheel, back, forward (the buttons the vendor UI offers).
pub const REMAPPABLE: usize = 5;

const TYPE_DISABLED: u8 = 0;
const TYPE_MOUSE: u8 = 1;
const TYPE_DPI: u8 = 2;
const TYPE_SCROLL_H: u8 = 3;
const TYPE_FIRE: u8 = 4;
const TYPE_SHORTCUT: u8 = 5;
const TYPE_MACRO: u8 = 6;
const TYPE_REPORT: u8 = 7;
const TYPE_SCROLL_V: u8 = 0x0B;

/// Left click: the last key with it cannot be changed (FUN_1401f6510).
const LEFT_CLICK: u8 = 1;
const FIRE_INTERVAL: std::ops::RangeInclusive<u8> = 10..=255;
const FIRE_COUNT: std::ops::RangeInclusive<u8> = 0..=3;
/// A combination is [modifier down, keys down, keys up in reverse, modifier up]; 6 events at most.
const COMBINATION_KEYS_MAX: usize = 2;

/// Macro play mode, Keys[i].p2 for type 6 (§3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopMode {
    UntilReleased,
    UntilAnyKey,
    UntilPressedAgain,
    /// Play 1..=250 times.
    Count(u8),
}

impl LoopMode {
    pub const COUNT_MAX: u8 = 250;

    /// The vendor loader shows 0 as a count of 1 (r_keyled_macro.c).
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0xFE => Some(Self::UntilReleased),
            0xFF => Some(Self::UntilAnyKey),
            0xFD => Some(Self::UntilPressedAgain),
            0 => Some(Self::Count(1)),
            1..=Self::COUNT_MAX => Some(Self::Count(b)),
            _ => None,
        }
    }

    pub fn byte(self) -> u8 {
        match self {
            Self::UntilReleased => 0xFE,
            Self::UntilAnyKey => 0xFF,
            Self::UntilPressedAgain => 0xFD,
            Self::Count(n) => n,
        }
    }

    pub fn to_json(self) -> Value {
        match self {
            Self::UntilReleased => json!({ "loop": "released" }),
            Self::UntilAnyKey => json!({ "loop": "anyKey" }),
            Self::UntilPressedAgain => json!({ "loop": "pressedAgain" }),
            Self::Count(n) => json!({ "loop": "count", "count": n }),
        }
    }

    pub fn from_json(obj: &Map<String, Value>) -> Result<Self, String> {
        match obj.get("loop").and_then(Value::as_str) {
            Some("released") => Ok(Self::UntilReleased),
            Some("anyKey") => Ok(Self::UntilAnyKey),
            Some("pressedAgain") => Ok(Self::UntilPressedAgain),
            Some("count") => {
                let n = obj.get("count").and_then(Value::as_u64).ok_or("'count' must be a whole number")?;
                match u8::try_from(n) {
                    Ok(n @ 1..=Self::COUNT_MAX) => Ok(Self::Count(n)),
                    _ => Err(format!("play count {n} is not 1..={}", Self::COUNT_MAX)),
                }
            }
            _ => Err("'loop' must be \"released\", \"anyKey\", \"pressedAgain\" or \"count\"".into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyFunction {
    Disabled,
    /// Mouse button mask 1/2/4/8/16.
    Mouse(u8),
    /// 1 loop, 2 DPI+, 3 DPI-.
    Dpi(u8),
    /// 1 up, 2 down.
    ScrollVertical(u8),
    /// 1 left, 2 right.
    ScrollHorizontal(u8),
    Fire { interval: u8, count: u8 },
    /// Plays shortcut slot i (combination or multimedia key).
    Shortcut,
    /// Plays macro slot `slot`.
    Macro { slot: u8, loop_mode: LoopMode },
    ReportRate,
    /// Anything the vendor UI cannot produce, kept as stored.
    Unknown(KeyBinding),
}

fn is_mouse_mask(m: u8) -> bool {
    hid::MOUSE_BUTTONS.iter().any(|(mask, _)| *mask == u16::from(m))
}

impl KeyFunction {
    pub fn from_binding(b: KeyBinding) -> Self {
        let unknown = Self::Unknown(b);
        match (b.kind, b.param1) {
            (TYPE_DISABLED, _) => Self::Disabled,
            (TYPE_MOUSE, m) if is_mouse_mask(m) => Self::Mouse(m),
            (TYPE_DPI, a @ 1..=3) => Self::Dpi(a),
            (TYPE_SCROLL_H, d @ 1..=2) => Self::ScrollHorizontal(d),
            (TYPE_FIRE, interval) => Self::Fire { interval, count: b.param2 },
            (TYPE_SHORTCUT, _) => Self::Shortcut,
            (TYPE_MACRO, slot) => LoopMode::from_byte(b.param2).map_or(unknown, |loop_mode| Self::Macro { slot, loop_mode }),
            (TYPE_REPORT, _) => Self::ReportRate,
            (TYPE_SCROLL_V, d @ 1..=2) => Self::ScrollVertical(d),
            _ => unknown,
        }
    }

    /// The binding the vendor dispatcher writes (FUN_1401f0e30) for key `key`.
    pub fn to_binding(self, key: usize) -> Result<KeyBinding, String> {
        let b = |kind, param1, param2| Ok(KeyBinding { kind, param1, param2 });
        match self {
            Self::Disabled => b(TYPE_DISABLED, 0, 0),
            Self::Mouse(m) if is_mouse_mask(m) => b(TYPE_MOUSE, m, 0),
            Self::Dpi(a @ 1..=3) => b(TYPE_DPI, a, 0),
            Self::ScrollVertical(d @ 1..=2) => b(TYPE_SCROLL_V, d, 0),
            Self::ScrollHorizontal(d @ 1..=2) => b(TYPE_SCROLL_H, d, 0),
            Self::Fire { interval, count } if FIRE_INTERVAL.contains(&interval) && FIRE_COUNT.contains(&count) => {
                b(TYPE_FIRE, interval, count)
            }
            Self::Fire { interval, count } => Err(format!(
                "fire key interval {interval} / count {count}: allowed {}..={} and {}..={}",
                FIRE_INTERVAL.start(),
                FIRE_INTERVAL.end(),
                FIRE_COUNT.start(),
                FIRE_COUNT.end()
            )),
            Self::Shortcut => b(TYPE_SHORTCUT, 0, 0),
            // The vendor always pairs key i with macro slot i.
            Self::Macro { loop_mode, .. } => {
                if let LoopMode::Count(n) = loop_mode {
                    if !(1..=LoopMode::COUNT_MAX).contains(&n) {
                        return Err(format!("play count {n} is not 1..={}", LoopMode::COUNT_MAX));
                    }
                }
                b(TYPE_MACRO, key as u8, loop_mode.byte())
            }
            Self::ReportRate => b(TYPE_REPORT, 0, 0),
            Self::Mouse(_) | Self::Dpi(_) | Self::ScrollVertical(_) | Self::ScrollHorizontal(_) => {
                Err(format!("{self:?} is not a function the vendor offers"))
            }
            Self::Unknown(_) => Err("an unknown function cannot be assigned".into()),
        }
    }
}

fn state_name(s: KeyState) -> &'static str {
    match s {
        KeyState::Down => "down",
        KeyState::Up => "up",
        KeyState::Other => "other",
    }
}

fn event_json(state: KeyState, kind: u8, code: u16) -> Value {
    let mut v = hid::event_name(kind, code);
    let obj = v.as_object_mut().expect("event_name gives an object");
    obj.insert("state".into(), json!(state_name(state)));
    obj.insert("kind".into(), json!(kind));
    obj.insert("code".into(), json!(code));
    v
}

pub fn macro_event_json(e: &MacroEvent) -> Value {
    let mut v = event_json(e.state, e.kind, e.code);
    v.as_object_mut().expect("object").insert("delay".into(), json!(e.delay_ms));
    v
}

/// The consumer usage when the shortcut is exactly what the vendor builds for a multimedia item.
fn multimedia_usage(s: &Shortcut) -> Option<u16> {
    let first = s.events.first()?;
    (first.kind == EVENT_CONSUMER && *s == Shortcut::multimedia(first.code) && hid::MULTIMEDIA.iter().any(|m| m.1 == first.code))
        .then_some(first.code)
}

fn is_left_click(b: &KeyBinding) -> bool {
    b.kind == TYPE_MOUSE && b.param1 == LEFT_CLICK
}

/// The vendor rule: the only key left with left click cannot be changed.
pub fn locked(s: &Settings, key: usize) -> bool {
    is_left_click(&s.keys[key].value) && s.keys.iter().filter(|k| is_left_click(&k.value)).count() == 1
}

fn function_json(f: KeyFunction, key: usize, slots: &Slots, encoding: NameEncoding) -> Value {
    match f {
        KeyFunction::Disabled => json!({ "category": "disabled" }),
        KeyFunction::Mouse(m) => json!({ "category": "mouse", "button": m }),
        KeyFunction::Dpi(a) => json!({ "category": "dpi", "action": a }),
        KeyFunction::ScrollVertical(d) => json!({ "category": "scrollV", "dir": d }),
        KeyFunction::ScrollHorizontal(d) => json!({ "category": "scrollH", "dir": d }),
        KeyFunction::Fire { interval, count } => json!({ "category": "fire", "interval": interval, "count": count }),
        KeyFunction::ReportRate => json!({ "category": "report" }),
        KeyFunction::Shortcut => match &slots.shortcuts[key] {
            Some(s) => match multimedia_usage(s) {
                Some(usage) => json!({ "category": "multimedia", "usage": usage }),
                None => {
                    let events: Vec<Value> = s.events.iter().map(|e| event_json(e.state, e.kind, e.code)).collect();
                    json!({ "category": "combination", "valid": true, "events": events })
                }
            },
            None => json!({ "category": "combination", "valid": false, "events": [] }),
        },
        KeyFunction::Macro { slot, loop_mode } => {
            let stored = slots.macros.get(usize::from(slot)).and_then(Option::as_ref);
            let mut v = json!({
                "category": "macro",
                "slot": slot,
                "valid": stored.is_some(),
                "name": stored.map(|m| macro_name::decode(&m.name, encoding)).unwrap_or_default(),
            });
            v.as_object_mut().expect("object").extend(loop_mode.to_json().as_object().expect("object").clone());
            v
        }
        KeyFunction::Unknown(b) => json!({ "category": "unknown", "kind": b.kind, "p1": b.param1, "p2": b.param2 }),
    }
}

/// What the KeySet page shows for Keys[0..5].
pub fn keys_view(s: &Settings, slots: &Slots, encoding: NameEncoding) -> Value {
    let keys: Vec<Value> = (0..REMAPPABLE)
        .map(|i| {
            let b = s.keys[i].value;
            json!({
                "index": i,
                "kind": b.kind,
                "p1": b.param1,
                "p2": b.param2,
                "locked": locked(s, i),
                "function": function_json(KeyFunction::from_binding(b), i, slots, encoding),
            })
        })
        .collect();
    Value::Array(keys)
}

/// Constant choices for the KeySet and Macro pages.
pub fn options() -> Value {
    json!({
        "remappableKeys": REMAPPABLE,
        "mouseButtons": hid::MOUSE_BUTTONS.iter().map(|(m, k)| json!({ "value": m, "nameKey": k })).collect::<Vec<_>>(),
        "multimedia": hid::MULTIMEDIA.iter().map(|(k, u)| json!({ "usage": u, "nameKey": k })).collect::<Vec<_>>(),
        "combinationModifiers": hid::COMBINATION_MODIFIERS.iter().map(|(m, n)| json!({ "value": m, "label": n })).collect::<Vec<_>>(),
        "fireInterval": [FIRE_INTERVAL.start(), FIRE_INTERVAL.end()],
        "fireCount": [FIRE_COUNT.start(), FIRE_COUNT.end()],
        "combinationKeysMax": COMBINATION_KEYS_MAX,
        "loopCountMax": LoopMode::COUNT_MAX,
        "macroEventsMax": MACRO_EVENTS.end(),
    })
}

/// The macros stored in the slots, for "import from device".
pub fn device_macros(slots: &Slots, encoding: NameEncoding) -> Value {
    let list: Vec<Value> = slots
        .macros
        .iter()
        .enumerate()
        .filter_map(|(i, m)| m.as_ref().map(|m| (i, m)))
        .map(|(i, m)| {
            json!({
                "slot": i,
                "name": macro_name::decode(&m.name, encoding),
                "events": m.events.iter().map(macro_event_json).collect::<Vec<_>>(),
            })
        })
        .collect();
    Value::Array(list)
}

/// What an assignment writes into the key's slot.
#[derive(Debug, Clone, PartialEq)]
pub enum SlotContent {
    Shortcut(Shortcut),
    Macro { name: String, events: Vec<MacroEvent> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub key: usize,
    pub function: KeyFunction,
    pub slot: Option<SlotContent>,
}

fn field<'a>(obj: &'a Map<String, Value>, key: &str) -> Result<&'a Value, String> {
    obj.get(key).ok_or_else(|| format!("'{key}' is missing"))
}

fn uint<T: TryFrom<u64>>(obj: &Map<String, Value>, key: &str) -> Result<T, String> {
    field(obj, key)?.as_u64().and_then(|v| T::try_from(v).ok()).ok_or_else(|| format!("'{key}' is out of range"))
}

fn object<'a>(v: &'a Value, what: &str) -> Result<&'a Map<String, Value>, String> {
    v.as_object().ok_or_else(|| format!("{what} must be an object"))
}

fn parse_state(obj: &Map<String, Value>) -> Result<KeyState, String> {
    match field(obj, "state")?.as_str() {
        Some("down") => Ok(KeyState::Down),
        Some("up") => Ok(KeyState::Up),
        _ => Err("'state' must be \"down\" or \"up\"".into()),
    }
}

fn parse_kind(obj: &Map<String, Value>, allowed: &[u8]) -> Result<u8, String> {
    let kind: u8 = uint(obj, "kind")?;
    if allowed.contains(&kind) {
        Ok(kind)
    } else {
        Err(format!("event type {kind} is not one of {allowed:?}"))
    }
}

/// One macro event `{"state", "kind", "code", "delay"}`.
pub fn parse_macro_event(v: &Value) -> Result<MacroEvent, String> {
    let obj = object(v, "an event")?;
    Ok(MacroEvent {
        state: parse_state(obj)?,
        kind: parse_kind(obj, &[EVENT_MODIFIER, EVENT_KEY, EVENT_CONSUMER, EVENT_MOUSE])?,
        code: uint(obj, "code")?,
        delay_ms: uint(obj, "delay")?,
    })
}

/// Combination key events (§3.3): modifier down, keys down in order, keys up in reverse, modifier up.
pub fn combination(modifier: u16, keys: &[(u8, u16)]) -> Result<Shortcut, String> {
    if keys.is_empty() {
        return Err("press a key for the combination first".into());
    }
    if keys.len() > COMBINATION_KEYS_MAX {
        return Err(format!("a combination holds at most {COMBINATION_KEYS_MAX} keys besides the modifier"));
    }
    let ev = |state, (kind, code): (u8, u16)| KeyEvent { state, kind, code };
    let mut events = vec![ev(KeyState::Down, (EVENT_MODIFIER, modifier))];
    events.extend(keys.iter().map(|k| ev(KeyState::Down, *k)));
    events.extend(keys.iter().rev().map(|k| ev(KeyState::Up, *k)));
    events.push(ev(KeyState::Up, (EVENT_MODIFIER, modifier)));
    Ok(Shortcut { events })
}

fn parse_function(obj: &Map<String, Value>) -> Result<(KeyFunction, Option<SlotContent>), String> {
    let category = field(obj, "category")?.as_str().ok_or("'category' must be a string")?;
    Ok(match category {
        "disabled" => (KeyFunction::Disabled, None),
        "mouse" => (KeyFunction::Mouse(uint(obj, "button")?), None),
        "dpi" => (KeyFunction::Dpi(uint(obj, "action")?), None),
        "scrollV" => (KeyFunction::ScrollVertical(uint(obj, "dir")?), None),
        "scrollH" => (KeyFunction::ScrollHorizontal(uint(obj, "dir")?), None),
        "fire" => (KeyFunction::Fire { interval: uint(obj, "interval")?, count: uint(obj, "count")? }, None),
        "report" => (KeyFunction::ReportRate, None),
        "multimedia" => {
            let usage: u16 = uint(obj, "usage")?;
            if !hid::MULTIMEDIA.iter().any(|m| m.1 == usage) {
                return Err(format!("consumer usage 0x{usage:04X} is not a multimedia item"));
            }
            (KeyFunction::Shortcut, Some(SlotContent::Shortcut(Shortcut::multimedia(usage))))
        }
        "combination" => {
            let modifier: u16 = uint(obj, "modifier")?;
            // The vendor's "Close" choice disables the key (FUN_1401ec000).
            if modifier == 0 {
                return Ok((KeyFunction::Disabled, None));
            }
            if !hid::COMBINATION_MODIFIERS.iter().any(|m| m.0 == modifier) {
                return Err(format!("modifier 0x{modifier:02X} is not Shift, Ctrl, Alt or Win"));
            }
            let keys = field(obj, "keys")?
                .as_array()
                .ok_or("'keys' must be a list")?
                .iter()
                .map(|k| {
                    let k = object(k, "a key")?;
                    Ok((parse_kind(k, &[EVENT_KEY, EVENT_CONSUMER])?, uint(k, "code")?))
                })
                .collect::<Result<Vec<(u8, u16)>, String>>()?;
            (KeyFunction::Shortcut, Some(SlotContent::Shortcut(combination(modifier, &keys)?)))
        }
        "macro" => {
            let name = field(obj, "name")?.as_str().ok_or("'name' must be a string")?.to_owned();
            let events = field(obj, "events")?
                .as_array()
                .ok_or("'events' must be a list")?
                .iter()
                .map(parse_macro_event)
                .collect::<Result<Vec<_>, _>>()?;
            let loop_mode = LoopMode::from_json(obj)?;
            (KeyFunction::Macro { slot: 0, loop_mode }, Some(SlotContent::Macro { name, events }))
        }
        other => return Err(format!("unknown function category '{other}'")),
    })
}

/// Parses `{"key": i, "function": {...}}` from the KeySet page.
pub fn parse_assignment(text: &str) -> Result<Assignment, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| format!("the assignment is not valid JSON: {e}"))?;
    let root = object(&root, "the assignment")?;
    let key: usize = uint(root, "key")?;
    if key >= REMAPPABLE {
        return Err(format!("button {key} cannot be remapped (only 0..{REMAPPABLE})"));
    }
    let (mut function, slot) = parse_function(object(field(root, "function")?, "'function'")?)?;
    if let KeyFunction::Macro { slot, .. } = &mut function {
        *slot = key as u8;
    }
    Ok(Assignment { key, function, slot })
}

/// One write, in the order they must run.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    WriteShortcut(usize, Shortcut),
    WriteMacro(usize, Macro),
    /// New settings for `Mouse::apply`: only the key binding differs.
    Bind(Box<Settings>),
}

/// The writes for an assignment: the slot first, always written whole and read back even when the
/// last read showed the same content (another process may have changed it since), then the binding
/// (skipped when unchanged; the worker checks the binding is still current before skipping).
pub fn plan(s: &Settings, a: &Assignment, encoding: NameEncoding) -> Result<Vec<Step>, String> {
    if a.key >= REMAPPABLE {
        return Err(format!("button {} cannot be remapped", a.key));
    }
    if locked(s, a.key) && a.function != KeyFunction::Mouse(LEFT_CLICK) {
        return Err("among all the buttons, at least one must keep the left-click function".into());
    }
    let binding = a.function.to_binding(a.key)?;
    let mut steps = Vec::new();
    match &a.slot {
        Some(SlotContent::Shortcut(shortcut)) => {
            shortcut.encode()?;
            steps.push(Step::WriteShortcut(a.key, shortcut.clone()));
        }
        Some(SlotContent::Macro { name, events }) => {
            let name = macro_name::encode(name, encoding).map_err(|e| e.to_string())?;
            let m = Macro { name, events: events.clone() };
            m.encode()?;
            steps.push(Step::WriteMacro(a.key, m));
        }
        None => {}
    }
    let mut new = s.clone();
    new.keys[a.key].set(binding);
    if new.keys[a.key] != s.keys[a.key] {
        steps.push(Step::Bind(Box::new(new)));
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hecate_core::protocol::{checksum, CONFIG_BLOCK_LEN};
    use hecate_core::settings::{self, Block};

    fn put(b: &mut Block, at: usize, data: &[u8]) {
        b[at..at + data.len()].copy_from_slice(data);
        b[at + data.len()] = checksum(data);
    }

    /// The owner's key bindings (flash 0x60..0x9F as read live): back and forward play shortcut slots 3/4.
    fn owner_settings() -> Settings {
        let mut b = [0xFF; CONFIG_BLOCK_LEN];
        let keys: [[u8; 3]; 16] = [
            [1, 1, 0], [1, 2, 0], [1, 4, 0], [5, 0, 0], [5, 0, 0], [2, 1, 0], [0, 0, 0], [0, 0, 0],
            [7, 0, 0], [2, 2, 0], [2, 3, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0],
        ];
        for (i, k) in keys.iter().enumerate() {
            put(&mut b, 0x60 + 4 * i, k);
        }
        settings::decode(&b)
    }

    /// The owner's slots: Volume Down / Up in shortcut slots 3/4, stale macros "切回去:D" / "切螢幕:D"
    /// (Ctrl+Win+Left / Right) in macro slots 3/4.
    fn owner_slots() -> Slots {
        let mut slots = Slots::default();
        slots.shortcuts[3] = Shortcut::decode(&[0x02, 0x82, 0xea, 0x00, 0x42, 0xea, 0x00, 0xbb]);
        slots.shortcuts[4] = Shortcut::decode(&[0x02, 0x82, 0xe9, 0x00, 0x42, 0xe9, 0x00, 0xbd]);
        let mut slot3 = vec![0x08, 0xa4, 0xc1, 0xa6, 0x5e, 0xa5, 0x68, 0x3a, 0x44];
        slot3.resize(0x1F, 0xFF);
        slot3.extend([
            0x06, 0x80, 0x01, 0x00, 0x00, 0x40, 0x80, 0x08, 0x00, 0x00, 0x90, 0x81, 0x50, 0x00, 0x00, 0x47, 0x41, 0x50,
            0x00, 0x00, 0x32, 0x40, 0x01, 0x00, 0x00, 0x05, 0x40, 0x08, 0x00, 0x00, 0x00, 0x0d,
        ]);
        slots.macros[3] = Macro::decode(&slot3);
        slots
    }

    fn assign(text: &str) -> Assignment {
        parse_assignment(text).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    #[test]
    fn owner_slots_decode() {
        let slots = owner_slots();
        assert!(slots.shortcuts[3].is_some() && slots.shortcuts[4].is_some());
        assert_eq!(slots.macros[3].as_ref().unwrap().events.len(), 6);
    }

    #[test]
    fn every_category_round_trips() {
        let functions = [
            KeyFunction::Disabled,
            KeyFunction::Mouse(1),
            KeyFunction::Mouse(2),
            KeyFunction::Mouse(4),
            KeyFunction::Mouse(8),
            KeyFunction::Mouse(16),
            KeyFunction::Dpi(1),
            KeyFunction::Dpi(2),
            KeyFunction::Dpi(3),
            KeyFunction::ScrollVertical(1),
            KeyFunction::ScrollVertical(2),
            KeyFunction::ScrollHorizontal(1),
            KeyFunction::ScrollHorizontal(2),
            KeyFunction::Fire { interval: 10, count: 0 },
            KeyFunction::Fire { interval: 255, count: 3 },
            KeyFunction::Shortcut,
            KeyFunction::Macro { slot: 3, loop_mode: LoopMode::UntilReleased },
            KeyFunction::Macro { slot: 3, loop_mode: LoopMode::UntilAnyKey },
            KeyFunction::Macro { slot: 3, loop_mode: LoopMode::UntilPressedAgain },
            KeyFunction::Macro { slot: 3, loop_mode: LoopMode::Count(1) },
            KeyFunction::Macro { slot: 3, loop_mode: LoopMode::Count(250) },
            KeyFunction::ReportRate,
        ];
        for f in functions {
            let b = f.to_binding(3).unwrap_or_else(|e| panic!("{f:?}: {e}"));
            assert_eq!(KeyFunction::from_binding(b), f, "{b:?}");
        }
    }

    #[test]
    fn vendor_encodings() {
        let b = |kind, param1, param2| KeyBinding { kind, param1, param2 };
        assert_eq!(KeyFunction::Mouse(8).to_binding(0), Ok(b(1, 8, 0)));
        assert_eq!(KeyFunction::Dpi(3).to_binding(0), Ok(b(2, 3, 0)));
        assert_eq!(KeyFunction::ScrollVertical(2).to_binding(0), Ok(b(0x0B, 2, 0)));
        assert_eq!(KeyFunction::ScrollHorizontal(1).to_binding(0), Ok(b(3, 1, 0)));
        assert_eq!(KeyFunction::Fire { interval: 40, count: 2 }.to_binding(0), Ok(b(4, 40, 2)));
        assert_eq!(KeyFunction::Shortcut.to_binding(4), Ok(b(5, 0, 0)));
        assert_eq!(KeyFunction::Macro { slot: 9, loop_mode: LoopMode::Count(7) }.to_binding(2), Ok(b(6, 2, 7)));
        assert_eq!(KeyFunction::ReportRate.to_binding(0), Ok(b(7, 0, 0)));
        assert_eq!(KeyFunction::Disabled.to_binding(0), Ok(b(0, 0, 0)));
    }

    #[test]
    fn vendor_defaults_with_p2_copies_decode() {
        // The factory profile stores (1,1,1), (2,1,1): p2 is not read for these types.
        let b = |kind, param1, param2| KeyBinding { kind, param1, param2 };
        assert_eq!(KeyFunction::from_binding(b(1, 1, 1)), KeyFunction::Mouse(1));
        assert_eq!(KeyFunction::from_binding(b(1, 16, 16)), KeyFunction::Mouse(16));
        assert_eq!(KeyFunction::from_binding(b(2, 1, 1)), KeyFunction::Dpi(1));
        assert_eq!(KeyFunction::from_binding(b(0, 5, 5)), KeyFunction::Disabled);
    }

    #[test]
    fn unknown_bindings() {
        let b = |kind, param1, param2| KeyBinding { kind, param1, param2 };
        for raw in [b(9, 0, 0), b(10, 1, 0), b(8, 0, 0), b(0x0C, 0, 0), b(0xFF, 0xFF, 0xFF), b(1, 3, 0), b(1, 0, 0), b(2, 4, 0), b(3, 0, 0), b(0x0B, 3, 0), b(6, 0, 251)] {
            assert_eq!(KeyFunction::from_binding(raw), KeyFunction::Unknown(raw), "{raw:?}");
            assert!(KeyFunction::Unknown(raw).to_binding(0).is_err());
        }
    }

    #[test]
    fn loop_modes() {
        assert_eq!(LoopMode::from_byte(0), Some(LoopMode::Count(1)));
        assert_eq!(LoopMode::from_byte(250), Some(LoopMode::Count(250)));
        assert_eq!(LoopMode::from_byte(0xFB), None);
        assert_eq!(LoopMode::from_byte(0xFC), None);
        for m in [LoopMode::UntilReleased, LoopMode::UntilAnyKey, LoopMode::UntilPressedAgain, LoopMode::Count(42)] {
            let j = m.to_json();
            assert_eq!(LoopMode::from_json(j.as_object().unwrap()), Ok(m));
        }
        assert!(LoopMode::from_json(json!({ "loop": "count", "count": 0 }).as_object().unwrap()).is_err());
        assert!(LoopMode::from_json(json!({ "loop": "count", "count": 251 }).as_object().unwrap()).is_err());
        assert!(LoopMode::from_json(json!({ "loop": "forever" }).as_object().unwrap()).is_err());
    }

    #[test]
    fn invalid_values_are_refused() {
        assert!(KeyFunction::Mouse(3).to_binding(0).is_err());
        assert!(KeyFunction::Dpi(0).to_binding(0).is_err());
        assert!(KeyFunction::ScrollVertical(3).to_binding(0).is_err());
        assert!(KeyFunction::Fire { interval: 9, count: 0 }.to_binding(0).is_err());
        assert!(KeyFunction::Fire { interval: 10, count: 4 }.to_binding(0).is_err());
        assert!(KeyFunction::Macro { slot: 0, loop_mode: LoopMode::Count(0) }.to_binding(0).is_err());
    }

    #[test]
    fn owner_keys_view_shows_volume_keys() {
        let v = keys_view(&owner_settings(), &owner_slots(), NameEncoding::Big5);
        assert_eq!(v[0]["function"], json!({ "category": "mouse", "button": 1 }));
        assert_eq!(v[0]["locked"], true);
        assert_eq!(v[1]["locked"], false);
        assert_eq!(v[3]["function"], json!({ "category": "multimedia", "usage": 0xEA }));
        assert_eq!(v[4]["function"], json!({ "category": "multimedia", "usage": 0xE9 }));
        assert_eq!(v.as_array().unwrap().len(), REMAPPABLE);
    }

    #[test]
    fn keys_view_of_combinations_macros_and_empty_slots() {
        let mut s = owner_settings();
        let mut slots = owner_slots();
        slots.shortcuts[1] = Some(combination(0x01, &[(EVENT_KEY, 0x06)]).unwrap());
        s.keys[1].set(KeyBinding { kind: 5, param1: 0, param2: 0 });
        s.keys[2].set(KeyBinding { kind: 5, param1: 0, param2: 0 });
        s.keys[3].set(KeyBinding { kind: 6, param1: 3, param2: 0xFF });
        let v = keys_view(&s, &slots, NameEncoding::Big5);
        assert_eq!(v[1]["function"]["category"], "combination");
        assert_eq!(v[1]["function"]["events"][0], json!({ "state": "down", "kind": 0, "code": 1, "name": "LCtrl" }));
        assert_eq!(v[1]["function"]["events"][1]["name"], "C");
        assert_eq!(v[2]["function"], json!({ "category": "combination", "valid": false, "events": [] }));
        assert_eq!(v[3]["function"], json!({ "category": "macro", "slot": 3, "valid": true, "name": "切回去:D", "loop": "anyKey" }));
    }

    #[test]
    fn options_list_the_vendor_choices() {
        let o = options();
        assert_eq!(o["remappableKeys"], 5);
        assert_eq!(o["multimedia"].as_array().unwrap().len(), 18);
        assert_eq!(o["multimedia"][6], json!({ "usage": 0xE9, "nameKey": "media.volume_up" }));
        assert_eq!(o["mouseButtons"][3], json!({ "value": 8, "nameKey": "mouse.back" }));
        assert_eq!(o["fireInterval"], json!([10, 255]));
        assert_eq!(o["macroEventsMax"], 70);
    }

    #[test]
    fn device_macros_lists_named_slots() {
        let list = device_macros(&owner_slots(), NameEncoding::Big5);
        let list = list.as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["slot"], 3);
        assert_eq!(list[0]["name"], "切回去:D");
        assert_eq!(list[0]["events"][0], json!({ "state": "down", "kind": 0, "code": 1, "delay": 64, "name": "LCtrl" }));
        assert_eq!(list[0]["events"][2]["name"], "Left");
    }

    #[test]
    fn combination_event_order() {
        let s = combination(0x01, &[(EVENT_KEY, 0x04), (EVENT_KEY, 0x05)]).unwrap();
        let order: Vec<(KeyState, u8, u16)> = s.events.iter().map(|e| (e.state, e.kind, e.code)).collect();
        assert_eq!(
            order,
            [
                (KeyState::Down, 0, 1),
                (KeyState::Down, 1, 4),
                (KeyState::Down, 1, 5),
                (KeyState::Up, 1, 5),
                (KeyState::Up, 1, 4),
                (KeyState::Up, 0, 1)
            ]
        );
        assert_eq!(s.encode().unwrap().len(), 3 * 6 + 2);
        assert!(combination(0x01, &[]).is_err());
        assert!(combination(0x01, &[(EVENT_KEY, 4); 3]).is_err());
    }

    #[test]
    fn simple_assignments_only_bind() {
        let s = owner_settings();
        let a = assign(r#"{"key": 3, "function": {"category": "mouse", "button": 8}}"#);
        let steps = plan(&s, &a, NameEncoding::Big5).unwrap();
        assert_eq!(steps.len(), 1);
        let Step::Bind(new) = &steps[0] else { panic!("{steps:?}") };
        assert_eq!(new.keys[3].value, KeyBinding { kind: 1, param1: 8, param2: 0 });
        assert_eq!(settings::diff(&s, new).len(), 1);
    }

    #[test]
    fn multimedia_writes_the_slot_then_binds() {
        let a = assign(r#"{"key": 1, "function": {"category": "multimedia", "usage": 205}}"#);
        let steps = plan(&owner_settings(), &a, NameEncoding::Big5).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0], Step::WriteShortcut(1, Shortcut::multimedia(0xCD)));
        let Step::Bind(new) = &steps[1] else { panic!("{steps:?}") };
        assert_eq!(new.keys[1].value, KeyBinding { kind: 5, param1: 0, param2: 0 });
    }

    #[test]
    fn unchanged_bindings_are_skipped() {
        let a = assign(r#"{"key": 1, "function": {"category": "mouse", "button": 2}}"#);
        assert_eq!(plan(&owner_settings(), &a, NameEncoding::Big5).unwrap(), []);
        // Same binding, new slot content: only the slot is written.
        let a = assign(r#"{"key": 4, "function": {"category": "multimedia", "usage": 234}}"#);
        let steps = plan(&owner_settings(), &a, NameEncoding::Big5).unwrap();
        assert_eq!(steps, [Step::WriteShortcut(4, Shortcut::multimedia(0xEA))]);
    }

    /// Review R2: the last read says slot 3 already holds Volume Down, but another process may have
    /// changed it; the slot is written and read back anyway.
    #[test]
    fn r2_slot_is_written_even_when_the_snapshot_matches() {
        let a = assign(r#"{"key": 3, "function": {"category": "multimedia", "usage": 234}}"#);
        assert_eq!(owner_slots().shortcuts[3], Some(Shortcut::multimedia(0xEA)));
        assert_eq!(plan(&owner_settings(), &a, NameEncoding::Big5).unwrap(), [Step::WriteShortcut(3, Shortcut::multimedia(0xEA))]);
        let m = r#"{"key": 3, "function": {"category": "macro", "name": "切回去:D", "loop": "anyKey", "events": [
            {"state": "down", "kind": 0, "code": 1, "delay": 64}, {"state": "down", "kind": 0, "code": 8, "delay": 144},
            {"state": "down", "kind": 1, "code": 80, "delay": 71}, {"state": "up", "kind": 1, "code": 80, "delay": 50},
            {"state": "up", "kind": 0, "code": 1, "delay": 5}, {"state": "up", "kind": 0, "code": 8, "delay": 0}]}}"#;
        let steps = plan(&owner_settings(), &assign(m), NameEncoding::Big5).unwrap();
        assert_eq!(steps.first(), Some(&Step::WriteMacro(3, owner_slots().macros[3].clone().unwrap())));
    }

    #[test]
    fn combination_assignment() {
        let a = assign(r#"{"key": 2, "function": {"category": "combination", "modifier": 1, "keys": [{"kind": 1, "code": 6}]}}"#);
        let steps = plan(&owner_settings(), &a, NameEncoding::Big5).unwrap();
        assert_eq!(steps[0], Step::WriteShortcut(2, combination(1, &[(EVENT_KEY, 6)]).unwrap()));
        assert!(matches!(steps[1], Step::Bind(_)));
        // "Close" disables the key without touching the slot.
        let a = assign(r#"{"key": 2, "function": {"category": "combination", "modifier": 0, "keys": []}}"#);
        assert_eq!(a.function, KeyFunction::Disabled);
        assert_eq!(a.slot, None);
    }

    #[test]
    fn macro_assignment_writes_slot_i_then_binds_type_6() {
        let a = assign(
            r#"{"key": 1, "function": {"category": "macro", "name": "切回去:D", "loop": "count", "count": 2,
                "events": [{"state": "down", "kind": 1, "code": 4, "delay": 50}, {"state": "up", "kind": 1, "code": 4, "delay": 0}]}}"#,
        );
        let steps = plan(&owner_settings(), &a, NameEncoding::Big5).unwrap();
        assert_eq!(steps.len(), 2);
        let Step::WriteMacro(1, m) = &steps[0] else { panic!("{steps:?}") };
        assert_eq!(m.name, [0xa4, 0xc1, 0xa6, 0x5e, 0xa5, 0x68, 0x3a, 0x44]);
        assert_eq!(m.events.len(), 2);
        let Step::Bind(new) = &steps[1] else { panic!("{steps:?}") };
        assert_eq!(new.keys[1].value, KeyBinding { kind: 6, param1: 1, param2: 2 });
    }

    #[test]
    fn macro_assignment_errors() {
        let base = |name: &str, events: &str| {
            format!(r#"{{"key": 1, "function": {{"category": "macro", "name": "{name}", "loop": "released", "events": {events}}}}}"#)
        };
        let two = r#"[{"state": "down", "kind": 1, "code": 4, "delay": 0}, {"state": "up", "kind": 1, "code": 4, "delay": 0}]"#;
        let one = r#"[{"state": "down", "kind": 1, "code": 4, "delay": 0}]"#;
        let err = |name: &str, events: &str| plan(&owner_settings(), &assign(&base(name, events)), NameEncoding::Big5).unwrap_err();
        assert!(err("宏设", two).contains("cannot be stored"));
        assert!(err(&"a".repeat(31), two).contains("31 bytes"));
        assert!(err("ok", one).contains("1 events"));
        assert!(parse_assignment(&base("ok", r#"[{"state": "down", "kind": 3, "code": 4, "delay": 0}]"#)).is_err());
        assert!(parse_assignment(&base("ok", r#"[{"state": "down", "kind": 1, "code": 4, "delay": 70000}]"#)).is_err());
    }

    #[test]
    fn last_left_click_is_locked() {
        let a = assign(r#"{"key": 0, "function": {"category": "mouse", "button": 2}}"#);
        assert!(plan(&owner_settings(), &a, NameEncoding::Big5).unwrap_err().contains("left-click"));
        // Re-assigning left click itself is allowed (and a no-op).
        let a = assign(r#"{"key": 0, "function": {"category": "mouse", "button": 1}}"#);
        assert_eq!(plan(&owner_settings(), &a, NameEncoding::Big5).unwrap(), []);
        // With a second left click elsewhere, key 0 is free.
        let mut s = owner_settings();
        s.keys[1].set(KeyBinding { kind: 1, param1: 1, param2: 0 });
        assert!(!locked(&s, 0));
        let a = assign(r#"{"key": 0, "function": {"category": "mouse", "button": 2}}"#);
        assert_eq!(plan(&s, &a, NameEncoding::Big5).unwrap().len(), 1);
    }

    #[test]
    fn malformed_assignments() {
        for (text, needle) in [
            ("", "not valid JSON"),
            ("[]", "must be an object"),
            (r#"{"function": {"category": "report"}}"#, "'key'"),
            (r#"{"key": 5, "function": {"category": "report"}}"#, "cannot be remapped"),
            (r#"{"key": 0}"#, "'function'"),
            (r#"{"key": 0, "function": {"category": "lights"}}"#, "'lights'"),
            (r#"{"key": 0, "function": {"category": "mouse"}}"#, "'button'"),
            (r#"{"key": 0, "function": {"category": "multimedia", "usage": 1}}"#, "not a multimedia item"),
            (r#"{"key": 0, "function": {"category": "combination", "modifier": 16, "keys": []}}"#, "modifier"),
            (r#"{"key": 0, "function": {"category": "combination", "modifier": 1, "keys": [{"kind": 0, "code": 2}]}}"#, "event type"),
        ] {
            let e = parse_assignment(text).unwrap_err();
            assert!(e.contains(needle), "{text}: {e}");
        }
    }
}
