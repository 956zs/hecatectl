//! Shortcut slots (flash 0x100-0x2FF) and macro slots (0x300-0x1AFF), docs/protocol.md §9.2 / §9.3 and
//! docs/ui-features.md §3.3 / §3.4 / §4. A key of type 5 plays shortcut slot i, type 6 plays macro slot i.
//!
//! `decode` follows the vendor parsers (HIDUsb.dll FUN_180006d60 / FUN_180002ba0). `encode` produces the
//! bytes the vendor writers send (FUN_180006e60 / FUN_180006a70 via FUN_180002ac0).

use crate::protocol::checksum;
use std::ops::RangeInclusive;

pub const SLOTS: usize = 16;
const SHORTCUT_BASE: usize = 0x100;
const SHORTCUT_STRIDE: usize = 0x20;
const MACRO_BASE: usize = 0x300;
const MACRO_STRIDE: usize = 0x180;
/// Offset of a macro slot's event count; the vendor size probe reads from here (FUN_180013a20).
pub const MACRO_COUNT_OFFSET: u16 = 0x1F;
/// Offset of a macro slot's first event.
const MACRO_EVENTS_OFFSET: usize = 0x20;

pub const SHORTCUT_EVENTS: RangeInclusive<usize> = 2..=6;
/// The macro editor refuses to append at 0x46 events (FUN_14020f2e0).
pub const MACRO_EVENTS: RangeInclusive<usize> = 2..=70;
/// Name field bytes 1..=0x1E. The UI input box is also limited to 20 (LengthValidator, measure UNCONFIRMED).
pub const MACRO_NAME_MAX: usize = 30;
/// The parsers keep only the low nibble of an event's first byte as its type.
pub const EVENT_KIND_MAX: u8 = 0x0F;

/// Event types (docs/ui-features.md §3.3).
pub const EVENT_MODIFIER: u8 = 0; // code = modifier mask: LCtrl 01, LShift 02, LAlt 04, LWin 08
pub const EVENT_KEY: u8 = 1; // code = keyboard HID usage
pub const EVENT_CONSUMER: u8 = 2; // code = consumer usage
pub const EVENT_MOUSE: u8 = 4; // code = mouse button mask (macros only)

/// Top two bits of an event's first byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    /// `10`
    Down,
    /// `01`
    Up,
    /// `00` or `11`: the parsers' third state, written back as `00`. Never produced by the UI.
    Other,
}

impl KeyState {
    fn from_byte(b: u8) -> Self {
        match b & 0xC0 {
            0x80 => KeyState::Down,
            0x40 => KeyState::Up,
            _ => KeyState::Other,
        }
    }

    fn bits(self) -> u8 {
        match self {
            KeyState::Down => 0x80,
            KeyState::Up => 0x40,
            KeyState::Other => 0x00,
        }
    }
}

/// Shortcut entry `[state | type, code lo, code hi]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub state: KeyState,
    pub kind: u8,
    pub code: u16,
}

/// Macro event `[state | type, code lo, code hi, delay hi, delay lo]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacroEvent {
    pub state: KeyState,
    pub kind: u8,
    pub code: u16,
    pub delay_ms: u16,
}

fn event_head(state: KeyState, kind: u8) -> Result<u8, String> {
    if kind > EVENT_KIND_MAX {
        return Err(format!("event type {kind} does not fit the 4-bit type field"));
    }
    Ok(state.bits() | kind)
}

fn check_count(what: &str, n: usize, allowed: &RangeInclusive<usize>) -> Result<(), String> {
    if !allowed.contains(&n) {
        return Err(format!("{what} has {n} events, allowed {}..={}", allowed.start(), allowed.end()));
    }
    Ok(())
}

/// Combination or multimedia key: 2..=6 events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub events: Vec<KeyEvent>,
}

impl Shortcut {
    /// Multimedia key: the consumer usage pressed then released, the slot the vendor builds (ui-features §3.3).
    pub fn multimedia(usage: u16) -> Self {
        let event = |state| KeyEvent { state, kind: EVENT_CONSUMER, code: usage };
        Self { events: vec![event(KeyState::Down), event(KeyState::Up)] }
    }

    /// Slot bytes from `0x100 + 0x20 * i`. None for a disabled, erased, short or corrupt slot (FUN_180006d60).
    pub fn decode(slot: &[u8]) -> Option<Self> {
        let n = usize::from(*slot.first()?);
        if !SHORTCUT_EVENTS.contains(&n) {
            return None;
        }
        let body = slot.get(..3 * n + 2)?;
        if body[3 * n + 1] != checksum(&body[..3 * n + 1]) {
            return None;
        }
        let events = body[1..=3 * n]
            .chunks(3)
            .map(|e| KeyEvent {
                state: KeyState::from_byte(e[0]),
                kind: e[0] & EVENT_KIND_MAX,
                code: u16::from_le_bytes([e[1], e[2]]),
            })
            .collect();
        Some(Self { events })
    }

    /// `[n, events.., check]`, 3n+2 bytes (FUN_180006e60).
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        check_count("shortcut", self.events.len(), &SHORTCUT_EVENTS)?;
        let mut out = vec![self.events.len() as u8];
        for e in &self.events {
            out.push(event_head(e.state, e.kind)?);
            out.extend(e.code.to_le_bytes());
        }
        out.push(checksum(&out));
        Ok(out)
    }
}

/// A macro as stored on the mouse. The name bytes are kept raw: their encoding is UNCONFIRMED.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Macro {
    pub name: Vec<u8>,
    pub events: Vec<MacroEvent>,
}

impl Macro {
    /// Slot bytes from `0x300 + 0x180 * j`. None when the events are missing, short or corrupt (FUN_180002ba0;
    /// the vendor then keeps the name but plays nothing). A name length outside 1..=30 gives an empty name.
    pub fn decode(slot: &[u8]) -> Option<Self> {
        let n = usize::from(*slot.get(usize::from(MACRO_COUNT_OFFSET))?);
        if !MACRO_EVENTS.contains(&n) {
            return None;
        }
        let end = MACRO_EVENTS_OFFSET + 5 * n;
        let body = slot.get(..=end)?;
        if body[end] != checksum(&body[usize::from(MACRO_COUNT_OFFSET)..end]) {
            return None;
        }
        let len = usize::from(body[0]);
        let name = if (1..=MACRO_NAME_MAX).contains(&len) { body[1..=len].to_vec() } else { Vec::new() };
        let events = body[MACRO_EVENTS_OFFSET..end]
            .chunks(5)
            .map(|e| MacroEvent {
                state: KeyState::from_byte(e[0]),
                kind: e[0] & EVENT_KIND_MAX,
                code: u16::from_le_bytes([e[1], e[2]]),
                delay_ms: u16::from_be_bytes([e[3], e[4]]),
            })
            .collect();
        Some(Self { name, events })
    }

    /// `[name length, name, 0xFF padding to 0x1F, n, events.., check over n and the events]`,
    /// 0x21 + 5n bytes (FUN_180002ac0 into a 0xFF-filled image, written by FUN_180006a70).
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        check_count("macro", self.events.len(), &MACRO_EVENTS)?;
        if self.name.len() > MACRO_NAME_MAX {
            return Err(format!("macro name is {} bytes, max {MACRO_NAME_MAX}", self.name.len()));
        }
        let mut out = vec![0xFF; MACRO_EVENTS_OFFSET];
        out[0] = self.name.len() as u8;
        out[1..=self.name.len()].copy_from_slice(&self.name);
        out[usize::from(MACRO_COUNT_OFFSET)] = self.events.len() as u8;
        for e in &self.events {
            out.push(event_head(e.state, e.kind)?);
            out.extend(e.code.to_le_bytes());
            out.extend(e.delay_ms.to_be_bytes());
        }
        out.push(checksum(&out[usize::from(MACRO_COUNT_OFFSET)..]));
        Ok(out)
    }
}

/// Everything in the slot area. Index i belongs to Keys[i].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Slots {
    pub shortcuts: [Option<Shortcut>; SLOTS],
    pub macros: [Option<Macro>; SLOTS],
}

fn slot_addr(what: &str, base: usize, stride: usize, i: usize) -> Result<u16, String> {
    if i >= SLOTS {
        return Err(format!("{what} slot {i} does not exist (0..{SLOTS})"));
    }
    Ok((base + stride * i) as u16)
}

pub fn shortcut_addr(i: usize) -> Result<u16, String> {
    slot_addr("shortcut", SHORTCUT_BASE, SHORTCUT_STRIDE, i)
}

pub fn macro_addr(j: usize) -> Result<u16, String> {
    slot_addr("macro", MACRO_BASE, MACRO_STRIDE, j)
}

/// Bytes to read for a shortcut slot whose first byte is `n`: 3n+2, or 0 when unused (FUN_180013a20).
pub fn shortcut_len(n: u8) -> usize {
    let n = usize::from(n);
    if SHORTCUT_EVENTS.contains(&n) {
        3 * n + 2
    } else {
        0
    }
}

/// Bytes to read for a macro slot whose count byte is `n`: 5n+0x21, or 0 when unused (FUN_180013a20).
pub fn macro_len(n: u8) -> usize {
    let n = usize::from(n);
    if MACRO_EVENTS.contains(&n) {
        5 * n + 0x21
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    fn key(state: KeyState, kind: u8, code: u16) -> KeyEvent {
        KeyEvent { state, kind, code }
    }

    fn mev(state: KeyState, kind: u8, code: u16, delay_ms: u16) -> MacroEvent {
        MacroEvent { state, kind, code, delay_ms }
    }

    /// Ctrl+C as the vendor combination key builds it (ui-features §3.3), bytes worked out by hand
    /// from FUN_180006e60: down=0x80, up=0x40, type in the low nibble, code lo/hi, check over [0..3n+1].
    fn ctrl_c() -> (Shortcut, Vec<u8>) {
        let s = Shortcut {
            events: vec![
                key(KeyState::Down, EVENT_MODIFIER, 0x01),
                key(KeyState::Down, EVENT_KEY, 0x06),
                key(KeyState::Up, EVENT_KEY, 0x06),
                key(KeyState::Up, EVENT_MODIFIER, 0x01),
            ],
        };
        (s, hex("04 80 01 00 81 06 00 41 06 00 40 01 00 C1"))
    }

    /// Macro "ab": key 0x04 down, 50 ms, up, 258 ms. Worked out from FUN_180002ac0: name, 0xFF padding,
    /// count at 0x1F, events with big-endian delay, check over 0x1F..0x20+5n.
    fn macro_ab() -> (Macro, Vec<u8>) {
        let m = Macro {
            name: b"ab".to_vec(),
            events: vec![mev(KeyState::Down, EVENT_KEY, 0x04, 50), mev(KeyState::Up, EVENT_KEY, 0x04, 0x0102)],
        };
        let mut bytes = vec![0x02, 0x61, 0x62];
        bytes.extend([0xFF; 28]);
        bytes.extend(hex("02 81 04 00 00 32 41 04 00 01 02 54"));
        (m, bytes)
    }

    /// xorshift32 bytes, reproducible.
    fn garbage(seed: u32, len: usize) -> Vec<u8> {
        let mut s = seed.max(1);
        (0..len)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                s as u8
            })
            .collect()
    }

    #[test]
    fn multimedia_slot_matches_vendor_bytes() {
        // Volume Up, ui-features §3.3
        let s = Shortcut::multimedia(0x00E9);
        assert_eq!(s.encode().unwrap(), hex("02 82 E9 00 42 E9 00 BD"));
        assert_eq!(Shortcut::decode(&hex("02 82 E9 00 42 E9 00 BD")), Some(s));
        // Favorites, the highest usage in the vendor table
        assert_eq!(Shortcut::multimedia(0x022A).encode().unwrap()[..7], hex("02 82 2A 02 42 2A 02")[..]);
    }

    #[test]
    fn combination_slot_matches_vendor_bytes() {
        let (s, bytes) = ctrl_c();
        assert_eq!(s.encode().unwrap(), bytes);
        assert_eq!(Shortcut::decode(&bytes), Some(s.clone()));
        // a full 0x20-byte slot read decodes the same
        let mut slot = bytes.clone();
        slot.resize(0x20, 0xFF);
        assert_eq!(Shortcut::decode(&slot), Some(s));
    }

    #[test]
    fn shortcut_round_trips_at_the_limits() {
        for n in SHORTCUT_EVENTS {
            let s = Shortcut { events: (0..n).map(|k| key(KeyState::Down, (k % 16) as u8, 0xFFFF - k as u16)).collect() };
            let bytes = s.encode().unwrap();
            assert_eq!(bytes.len(), 3 * n + 2);
            assert_eq!(bytes.len(), shortcut_len(bytes[0]));
            assert_eq!(Shortcut::decode(&bytes), Some(s));
        }
    }

    #[test]
    fn empty_erased_and_corrupt_shortcuts_are_none() {
        assert_eq!(Shortcut::decode(&[]), None);
        assert_eq!(Shortcut::decode(&[0xFF; 0x20]), None);
        assert_eq!(Shortcut::decode(&[0x00; 0x20]), None);
        assert_eq!(Shortcut::decode(&[0x00, 0xFF]), None); // vendor "disabled" slot
        let (_, bytes) = ctrl_c();
        assert_eq!(Shortcut::decode(&bytes[..bytes.len() - 1]), None); // short read
        let mut bad = bytes.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert_eq!(Shortcut::decode(&bad), None);
        for n in [1u8, 7] {
            let mut b = vec![n];
            b.extend([0x80; 23]);
            let c = checksum(&b[..3 * usize::from(n) + 1]);
            b[3 * usize::from(n) + 1] = c;
            assert_eq!(Shortcut::decode(&b), None, "n = {n}");
        }
    }

    #[test]
    fn shortcut_limits_are_rejected() {
        for n in [0, 1, 7, 20] {
            let s = Shortcut { events: vec![key(KeyState::Down, 1, 4); n] };
            assert!(s.encode().is_err(), "{n} events");
        }
        let s = Shortcut { events: vec![key(KeyState::Down, 16, 4), key(KeyState::Up, 1, 4)] };
        assert!(s.encode().is_err());
    }

    #[test]
    fn event_state_bits_follow_the_parser() {
        // 00 and 11 both decode as Other (FUN_180006d60); bits 4..5 are not part of the type.
        let b = |head: u8| {
            let mut v = vec![2, head, 0x04, 0x00, 0x40, 0x04, 0x00];
            v.push(checksum(&v));
            Shortcut::decode(&v).unwrap().events[0]
        };
        assert_eq!(b(0x81), key(KeyState::Down, 1, 4));
        assert_eq!(b(0x41), key(KeyState::Up, 1, 4));
        assert_eq!(b(0x01), key(KeyState::Other, 1, 4));
        assert_eq!(b(0xC1), key(KeyState::Other, 1, 4));
        assert_eq!(b(0xB2), key(KeyState::Down, 2, 4));
        // Other is written as 00 (FUN_180006e60)
        let s = Shortcut { events: vec![key(KeyState::Other, 1, 4), key(KeyState::Up, 1, 4)] };
        assert_eq!(s.encode().unwrap()[1], 0x01);
    }

    #[test]
    fn macro_matches_vendor_bytes() {
        let (m, bytes) = macro_ab();
        assert_eq!(bytes.len(), 0x21 + 5 * 2);
        assert_eq!(m.encode().unwrap(), bytes);
        assert_eq!(Macro::decode(&bytes), Some(m.clone()));
        assert_eq!(macro_len(bytes[0x1F]), bytes.len());
        // a full 0x180-byte slot read decodes the same
        let mut slot = bytes;
        slot.resize(0x180, 0xFF);
        assert_eq!(Macro::decode(&slot), Some(m));
    }

    #[test]
    fn macro_mouse_button_events() {
        // Insert event "Left Key": type 4, mask 1, down + up (ui-features §4)
        let events = vec![mev(KeyState::Down, EVENT_MOUSE, 1, 10), mev(KeyState::Up, EVENT_MOUSE, 1, 0)];
        let m = Macro { name: b"L".to_vec(), events };
        let bytes = m.encode().unwrap();
        assert_eq!(bytes[0x20..0x2A], hex("84 01 00 00 0A 44 01 00 00 00")[..]);
        assert_eq!(Macro::decode(&bytes), Some(m));
    }

    #[test]
    fn macro_round_trips_at_the_limits() {
        for n in [2, 3, 69, 70] {
            for name_len in [0, 1, 20, MACRO_NAME_MAX] {
                let m = Macro {
                    name: (0..name_len).map(|i| b'a' + (i % 26) as u8).collect(),
                    events: (0..n)
                        .map(|k| {
                            let state = if k % 2 == 0 { KeyState::Down } else { KeyState::Up };
                            mev(state, 1, k as u16, u16::MAX - k as u16)
                        })
                        .collect(),
                };
                let bytes = m.encode().unwrap();
                assert_eq!(bytes.len(), 0x21 + 5 * n);
                assert!(bytes.len() <= MACRO_STRIDE);
                assert!(bytes[1 + name_len..0x1F].iter().all(|b| *b == 0xFF), "name padding");
                assert_eq!(Macro::decode(&bytes), Some(m));
            }
        }
    }

    #[test]
    fn macro_name_bytes_are_kept_raw() {
        let name = "宏一".as_bytes().to_vec(); // 6 bytes of UTF-8; the vendor's encoding is UNCONFIRMED
        let m = Macro { name: name.clone(), events: macro_ab().0.events };
        assert_eq!(Macro::decode(&m.encode().unwrap()).unwrap().name, name);
        let raw = Macro { name: vec![0x00, 0xFF, 0x80], events: macro_ab().0.events };
        assert_eq!(Macro::decode(&raw.encode().unwrap()), Some(raw));
    }

    #[test]
    fn macro_name_length_outside_the_field_gives_empty_name() {
        // FUN_180002ba0 copies the name only for lengths 1..=0x1E; the events still decode.
        let (m, mut bytes) = macro_ab();
        for len in [0x00, 0x1F, 0xFF] {
            bytes[0] = len;
            assert_eq!(Macro::decode(&bytes), Some(Macro { name: Vec::new(), events: m.events.clone() }), "len {len}");
        }
    }

    #[test]
    fn empty_erased_and_corrupt_macros_are_none() {
        assert_eq!(Macro::decode(&[]), None);
        assert_eq!(Macro::decode(&[0xFF; 0x180]), None);
        assert_eq!(Macro::decode(&[0x00; 0x180]), None);
        let (_, bytes) = macro_ab();
        assert_eq!(Macro::decode(&bytes[..0x1F]), None);
        assert_eq!(Macro::decode(&bytes[..bytes.len() - 1]), None);
        let mut bad = bytes.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert_eq!(Macro::decode(&bad), None);
        // the check covers the count and events, not the name
        let mut renamed = bytes.clone();
        renamed[1] = b'x';
        assert!(Macro::decode(&renamed).is_some());
        let mut event = bytes;
        event[0x22] ^= 1;
        assert_eq!(Macro::decode(&event), None);
        // vendor "disabled" macro: count 0, check 0x55
        let mut disabled = vec![0x00];
        disabled.extend([0xFF; 30]);
        disabled.extend([0x00, 0x55]);
        assert_eq!(Macro::decode(&disabled), None);
    }

    #[test]
    fn macro_limits_are_rejected() {
        let ev = mev(KeyState::Down, 1, 4, 0);
        for n in [0, 1, 71, 100] {
            assert!(Macro { name: b"m".to_vec(), events: vec![ev; n] }.encode().is_err(), "{n} events");
        }
        assert!(Macro { name: vec![b'a'; 31], events: vec![ev; 2] }.encode().is_err());
        assert!(Macro { name: vec![b'a'; 30], events: vec![ev; 2] }.encode().is_ok());
        assert!(Macro { name: Vec::new(), events: vec![mev(KeyState::Up, 0x10, 0, 0); 2] }.encode().is_err());
        // a count byte of 71 is not a macro
        let mut b = vec![0u8; 0x21 + 5 * 71];
        b[0x1F] = 71;
        let end = b.len() - 1;
        b[end] = checksum(&b[0x1F..end]);
        assert_eq!(Macro::decode(&b), None);
    }

    #[test]
    fn garbage_never_panics_and_decodes_stably() {
        let (mut shortcuts, mut macros) = (0, 0);
        for seed in 1..400 {
            let len = (seed as usize * 7) % 0x190;
            let mut bytes = garbage(seed, len);
            // Even seeds get a plausible count and a valid check byte, so they reach the event decoder.
            let fix = seed % 2 == 0;
            if let Some(b) = bytes.first_mut() {
                *b = 2 + (*b % 5);
                let end = 3 * usize::from(*b) + 1;
                if fix && end < len {
                    bytes[end] = checksum(&bytes[..end]);
                }
            }
            if let Some(s) = Shortcut::decode(&bytes) {
                shortcuts += 1;
                let again = s.encode().unwrap();
                assert_eq!(Shortcut::decode(&again), Some(s));
            }
            if let Some(b) = bytes.get_mut(0x1F) {
                *b = 2 + (*b % 69);
                let end = 0x20 + 5 * usize::from(*b);
                if fix && end < len {
                    bytes[end] = checksum(&bytes[0x1F..end]);
                }
            }
            if let Some(m) = Macro::decode(&bytes) {
                macros += 1;
                let again = m.encode().unwrap();
                assert_eq!(Macro::decode(&again), Some(m));
            }
        }
        assert!(shortcuts > 50 && macros > 20, "only {shortcuts} shortcuts and {macros} macros decoded");
    }

    #[test]
    fn vendor_written_bytes_survive_decode_encode() {
        // Every byte pattern the vendor writer can produce comes back unchanged.
        for (s, bytes) in [ctrl_c(), (Shortcut::multimedia(0x0183), Shortcut::multimedia(0x0183).encode().unwrap())] {
            assert_eq!(Shortcut::decode(&bytes).unwrap().encode().unwrap(), bytes);
            assert_eq!(s.encode().unwrap(), bytes);
        }
        let (_, bytes) = macro_ab();
        assert_eq!(Macro::decode(&bytes).unwrap().encode().unwrap(), bytes);
    }

    #[test]
    fn slot_addresses_and_lengths() {
        assert_eq!(shortcut_addr(0), Ok(0x100));
        assert_eq!(shortcut_addr(15), Ok(0x2E0));
        assert!(shortcut_addr(16).is_err());
        assert_eq!(macro_addr(0), Ok(0x300));
        assert_eq!(macro_addr(15), Ok(0x1980)); // last slot ends at 0x1AFF
        assert!(macro_addr(16).is_err());
        assert!(macro_addr(usize::MAX).is_err());
        assert_eq!([0u8, 1, 2, 6, 7, 0xFF].map(shortcut_len), [0, 0, 8, 20, 0, 0]);
        assert_eq!([0u8, 1, 2, 70, 71, 0xFF].map(macro_len), [0, 0, 0x2B, 0x17F, 0, 0]);
    }
}
