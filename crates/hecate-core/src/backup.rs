//! Backup and restore of everything the vendor app stores on the mouse: flash 0x0000-0x1AFF
//! (settings block, 16 shortcut slots, 16 macro slots; protocol.md §3.3).
//!
//! A backup keeps the raw image. A restore never writes it back blindly: it plans the same writes the
//! vendor save path would send to turn the current flash into the backup (settings groups via
//! `settings::diff`, whole slots via the slot codecs) and nothing outside those ranges.

use crate::mouse::Mouse;
use crate::protocol::CONFIG_BLOCK_LEN;
use crate::settings::{self, Block, FlashWrite, Settings};
use crate::slots::{self, Macro, Shortcut, Slots, SLOTS};
use std::fmt::Write as _;
use std::ops::Range;
use std::time::{SystemTime, UNIX_EPOCH};

pub const IMAGE_LEN: usize = slots::USED_FLASH_LEN;
const FORMAT_NAME: &str = "hecatectl backup";
pub const FORMAT_VERSION: i128 = 1;
/// Image bytes per line in the file, like `hecatectl dump`.
const LINE_BYTES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    pub created_unix: u64,
    pub product_id: u16,
    /// Mouse firmware (cmd 0x12).
    pub firmware: u16,
    /// USB bcdDevice of the receiver or cable, when known.
    pub receiver: Option<u16>,
    image: Vec<u8>,
}

impl Backup {
    pub fn new(
        created_unix: u64,
        product_id: u16,
        firmware: u16,
        receiver: Option<u16>,
        image: Vec<u8>,
    ) -> Result<Self, String> {
        if image.len() != IMAGE_LEN {
            return Err(format!("backup image is {} bytes, expected {IMAGE_LEN}", image.len()));
        }
        Ok(Self { created_unix, product_id, firmware, receiver, image })
    }

    /// Reads the firmware version and the whole image (`Mouse::read_image`) in one locked operation and
    /// stamps them with the current time.
    pub fn take(mouse: &mut Mouse, product_id: u16, receiver: Option<u16>) -> Result<Self, String> {
        mouse.require_online()?;
        let (firmware, image) = mouse.exclusive(|m| Ok((m.version()?, m.read_image()?)))?;
        let created_unix = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        Self::new(created_unix, product_id, firmware, receiver, image)
    }

    /// Flash bytes 0x0000..0x1B00.
    pub fn image(&self) -> &[u8] {
        &self.image
    }

    pub fn settings(&self) -> Settings {
        settings::decode(&block(&self.image))
    }

    pub fn slots(&self) -> Slots {
        Slots::from_image(&self.image)
    }

    /// JSON with the image as hex lines `"0000: 01 54 .."`, 16 bytes each, so backups diff line by line.
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        let receiver = self.receiver.map_or("null".to_string(), |v| format!("\"{v:04x}\""));
        let lines: Vec<String> = self
            .image
            .chunks(LINE_BYTES)
            .enumerate()
            .map(|(i, row)| {
                let hex: Vec<String> = row.iter().map(|b| format!("{b:02x}")).collect();
                format!("    \"{:04x}: {}\"", i * LINE_BYTES, hex.join(" "))
            })
            .collect();
        writeln!(out, "{{").unwrap();
        writeln!(out, "  \"format\": \"{FORMAT_NAME}\",").unwrap();
        writeln!(out, "  \"version\": {FORMAT_VERSION},").unwrap();
        writeln!(out, "  \"created_unix\": {},", self.created_unix).unwrap();
        writeln!(out, "  \"product_id\": \"{:04x}\",", self.product_id).unwrap();
        writeln!(out, "  \"firmware\": \"{:04x}\",", self.firmware).unwrap();
        writeln!(out, "  \"receiver\": {receiver},").unwrap();
        writeln!(out, "  \"image\": [\n{}\n  ]", lines.join(",\n")).unwrap();
        writeln!(out, "}}").unwrap();
        out
    }

    /// Strict inverse of `serialize`: the format name and version, every field, the line addresses,
    /// the line lengths and the hex must all be right. Unknown fields are rejected.
    pub fn parse(text: &str) -> Result<Self, String> {
        let Json::Obj(fields) = parse_json(text)? else { return Err("expected a JSON object".into()) };
        let field = |key: &str| fields.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        let need = |key: &str| field(key).ok_or_else(|| format!("missing field \"{key}\""));
        if need("format")? != &Json::Str(FORMAT_NAME.into()) {
            return Err(format!("not a hecatectl backup (\"format\" must be \"{FORMAT_NAME}\")"));
        }
        match need("version")? {
            Json::Int(FORMAT_VERSION) => {}
            Json::Int(v) => {
                return Err(format!("backup format version {v} is not supported (this build reads {FORMAT_VERSION})"));
            }
            _ => return Err("\"version\" must be a number".into()),
        }
        const KNOWN: [&str; 7] = ["format", "version", "created_unix", "product_id", "firmware", "receiver", "image"];
        if let Some((k, _)) = fields.iter().find(|(k, _)| !KNOWN.contains(&k.as_str())) {
            return Err(format!("unknown field \"{k}\""));
        }
        let created_unix = match need("created_unix")? {
            Json::Int(v) => u64::try_from(*v).map_err(|_| "\"created_unix\" must be 0..=2^64-1".to_string())?,
            _ => return Err("\"created_unix\" must be a number".into()),
        };
        let receiver = match need("receiver")? {
            Json::Null => None,
            v => Some(hex16("receiver", v)?),
        };
        let Json::Arr(lines) = need("image")? else { return Err("\"image\" must be an array of strings".into()) };
        Self::new(
            created_unix,
            hex16("product_id", need("product_id")?)?,
            hex16("firmware", need("firmware")?)?,
            receiver,
            parse_image(lines)?,
        )
    }
}

fn block(image: &[u8]) -> Block {
    image[..CONFIG_BLOCK_LEN].try_into().expect("image holds the settings block")
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A 16-bit value written as four hex digits.
fn hex16(key: &str, v: &Json) -> Result<u16, String> {
    match v {
        Json::Str(s) if s.len() == 4 && is_hex(s) => Ok(u16::from_str_radix(s, 16).expect("four hex digits")),
        _ => Err(format!("\"{key}\" must be a string of four hex digits")),
    }
}

fn parse_image(lines: &[Json]) -> Result<Vec<u8>, String> {
    let want = IMAGE_LEN / LINE_BYTES;
    if lines.len() != want {
        return Err(format!("\"image\" has {} lines, expected {want}", lines.len()));
    }
    let mut image = Vec::with_capacity(IMAGE_LEN);
    for (i, line) in lines.iter().enumerate() {
        let at = i * LINE_BYTES;
        let bad = |what: &str| format!("image line {} (0x{at:04x}): {what}", i + 1);
        let Json::Str(line) = line else { return Err(bad("not a string")) };
        let Some((addr, bytes)) = line.split_once(": ") else { return Err(bad("expected \"AAAA: xx xx ..\"")) };
        if addr.len() != 4 || !is_hex(addr) || usize::from_str_radix(addr, 16) != Ok(at) {
            return Err(bad(&format!("address \"{addr}\" should be {at:04x}")));
        }
        let row: Vec<&str> = bytes.split(' ').collect();
        if row.len() != LINE_BYTES || row.iter().any(|b| b.len() != 2 || !is_hex(b)) {
            return Err(bad(&format!("expected {LINE_BYTES} hex bytes separated by single spaces")));
        }
        image.extend(row.iter().map(|b| u8::from_str_radix(b, 16).expect("two hex digits")));
    }
    Ok(image)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The group's check byte is wrong in the backup, so its value there is unknown.
    InvalidInBackup,
    /// Not part of any group the vendor writes.
    Unmodelled,
}

/// Settings bytes that differ from the backup but are left alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub range: Range<usize>,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePlan {
    /// Settings groups in vendor order, then shortcut slots, then macro slots, like FUN_180006630.
    pub writes: Vec<FlashWrite>,
    pub skipped: Vec<Skipped>,
}

/// Writes that turn the `current` flash image into `backup`, for a device with `product_id`.
///
/// - Settings: `settings::diff`, keeping only groups whose check byte is valid in the backup. UI-choice
///   validation is not applied: every value comes from a group the mouse itself stored.
/// - Slots: a slot is written whole when its decoded content differs. A slot that is empty in the backup
///   gets the vendor's disabled form (`[00 FF]`, or the macro name with count 0).
/// - Differing bytes outside the vendor's groups, and groups invalid in the backup, are reported in
///   `skipped` and never written.
pub fn plan_restore(product_id: u16, current: &[u8], backup: &Backup) -> Result<RestorePlan, String> {
    if product_id != backup.product_id {
        return Err(format!(
            "the backup was taken from product {:04x}, this device is {product_id:04x}; refusing to restore",
            backup.product_id
        ));
    }
    if current.len() != IMAGE_LEN {
        return Err(format!("current image is {} bytes, expected {IMAGE_LEN}", current.len()));
    }
    let target = backup.image();
    let (cur, bkp) = (settings::decode(&block(current)), backup.settings());
    let groups = settings::group_ranges(&bkp);
    let mut writes: Vec<FlashWrite> = settings::diff(&cur, &bkp)
        .into_iter()
        .filter(|w| groups.iter().any(|(r, valid)| *valid && r.start == usize::from(w.addr)))
        .collect();

    let mut skipped: Vec<Skipped> = groups
        .iter()
        .filter(|(r, valid)| !valid && current[r.clone()] != target[r.clone()])
        .map(|(r, _)| Skipped { range: r.clone(), reason: SkipReason::InvalidInBackup })
        .collect();
    let modelled = |at: usize| groups.iter().any(|(r, _)| r.contains(&at));
    let unmodelled = (0..CONFIG_BLOCK_LEN).filter(|&at| !modelled(at) && current[at] != target[at]);
    skipped.extend(runs(unmodelled).into_iter().map(|range| Skipped { range, reason: SkipReason::Unmodelled }));
    skipped.sort_by_key(|s| s.range.start);

    for i in 0..SLOTS {
        let r = slots::shortcut_range(i);
        let want = Shortcut::decode(&target[r.clone()]);
        if Shortcut::decode(&current[r.clone()]) != want {
            let bytes = match want {
                Some(s) => s.encode()?,
                None => slots::DISABLED_SHORTCUT.to_vec(),
            };
            writes.push(FlashWrite { addr: r.start as u16, bytes });
        }
    }
    for j in 0..SLOTS {
        let r = slots::macro_range(j);
        let want = Macro::decode(&target[r.clone()]);
        if Macro::decode(&current[r.clone()]) != want {
            let bytes = match want {
                Some(m) => m.encode()?,
                None => slots::disabled_macro(&slots::stored_name(&target[r.clone()]))?,
            };
            writes.push(FlashWrite { addr: r.start as u16, bytes });
        }
    }
    check_writes(&writes)?;
    Ok(RestorePlan { writes, skipped })
}

/// Consecutive addresses merged into ranges.
fn runs(addrs: impl Iterator<Item = usize>) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for at in addrs {
        match out.last_mut() {
            Some(r) if r.end == at => r.end += 1,
            _ => out.push(at..at + 1),
        }
    }
    out
}

/// Rejects any write the vendor writer would not send: a whole settings group, or a whole slot of the
/// length its own header gives (or the disabled form), at a slot start.
pub fn check_writes(writes: &[FlashWrite]) -> Result<(), String> {
    let groups = settings::group_ranges(&settings::decode(&[0; CONFIG_BLOCK_LEN]));
    for w in writes {
        let (at, b) = (usize::from(w.addr), w.bytes.as_slice());
        let ok = if at < CONFIG_BLOCK_LEN {
            groups.iter().any(|(r, _)| *r == (at..at + b.len()))
        } else if (0..SLOTS).any(|i| slots::shortcut_range(i).start == at) {
            b == slots::DISABLED_SHORTCUT || (!b.is_empty() && b.len() == slots::shortcut_len(b[0]))
        } else if (0..SLOTS).any(|j| slots::macro_range(j).start == at) {
            match b.get(usize::from(slots::MACRO_COUNT_OFFSET)) {
                Some(0) => b.len() == slots::DISABLED_MACRO_LEN,
                Some(&n) => b.len() == slots::macro_len(n),
                None => false,
            }
        } else {
            false
        };
        if !ok {
            let len = b.len();
            return Err(format!("refusing a write of {len} bytes at 0x{at:04x}: not a group or slot the vendor writes"));
        }
    }
    Ok(())
}

/// `2026-10-21 10:20:30 UTC` (civil-from-days, proleptic Gregorian calendar).
pub fn utc_text(unix: u64) -> String {
    let (days, secs) = ((unix / 86_400) as i64, unix % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC", secs / 3600, secs / 60 % 60, secs % 60)
}

/// The JSON subset the backup file needs: no fractions or exponents.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Json {
    Null,
    Bool(bool),
    Int(i128),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

const MAX_DEPTH: usize = 16;

fn parse_json(text: &str) -> Result<Json, String> {
    let mut r = Reader { s: text.as_bytes(), i: 0 };
    let v = r.value(0)?;
    r.ws();
    if r.i != r.s.len() {
        return r.err("unexpected text after the JSON value");
    }
    Ok(v)
}

struct Reader<'a> {
    s: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("invalid JSON: {what} at byte {}", self.i))
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        let hit = self.peek() == Some(c);
        self.i += usize::from(hit);
        hit
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > MAX_DEPTH {
            return self.err("nesting too deep");
        }
        self.ws();
        match self.peek() {
            None => self.err("unexpected end"),
            Some(b'{') => {
                self.i += 1;
                let mut fields: Vec<(String, Json)> = Vec::new();
                self.ws();
                if self.eat(b'}') {
                    return Ok(Json::Obj(fields));
                }
                loop {
                    self.ws();
                    let key = self.string()?;
                    if fields.iter().any(|(k, _)| *k == key) {
                        return self.err(&format!("duplicate key \"{key}\""));
                    }
                    self.ws();
                    if !self.eat(b':') {
                        return self.err("expected ':'");
                    }
                    fields.push((key, self.value(depth + 1)?));
                    self.ws();
                    if self.eat(b'}') {
                        return Ok(Json::Obj(fields));
                    }
                    if !self.eat(b',') {
                        return self.err("expected ',' or '}'");
                    }
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.eat(b']') {
                    return Ok(Json::Arr(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.ws();
                    if self.eat(b']') {
                        return Ok(Json::Arr(items));
                    }
                    if !self.eat(b',') {
                        return self.err("expected ',' or ']'");
                    }
                }
            }
            Some(b'"') => self.string().map(Json::Str),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'n') => self.word("null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => self.err("unexpected character"),
        }
    }

    fn word(&mut self, word: &str, v: Json) -> Result<Json, String> {
        if !self.s[self.i..].starts_with(word.as_bytes()) {
            return self.err("invalid literal");
        }
        self.i += word.len();
        Ok(v)
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        self.eat(b'-');
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        if matches!(self.peek(), Some(b'.' | b'e' | b'E')) {
            return self.err("only whole numbers are supported");
        }
        let text = std::str::from_utf8(&self.s[start..self.i]).expect("ASCII digits");
        let digits = text.trim_start_matches('-');
        if digits.is_empty() || (digits.len() > 1 && digits.starts_with('0')) {
            return self.err("invalid number");
        }
        text.parse().map(Json::Int).or_else(|_| self.err("number out of range"))
    }

    fn string(&mut self) -> Result<String, String> {
        if !self.eat(b'"') {
            return self.err("expected a string");
        }
        let mut out = Vec::new();
        loop {
            let Some(c) = self.peek() else { return self.err("unterminated string") };
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let Some(e) = self.peek() else { return self.err("unterminated string") };
                    self.i += 1;
                    match e {
                        b'"' | b'\\' | b'/' => out.push(e),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0C),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let hex = self.s.get(self.i..self.i + 4).and_then(|h| std::str::from_utf8(h).ok());
                            let ch = hex
                                .filter(|h| is_hex(h))
                                .and_then(|h| char::from_u32(u32::from_str_radix(h, 16).expect("four hex digits")));
                            let Some(ch) = ch else { return self.err("unsupported \\u escape") };
                            self.i += 4;
                            out.extend(ch.encode_utf8(&mut [0; 4]).as_bytes());
                        }
                        _ => return self.err("invalid escape"),
                    }
                }
                c if c < 0x20 => return self.err("control character in string"),
                c => out.push(c),
            }
        }
        String::from_utf8(out).or_else(|_| self.err("invalid UTF-8 in string"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::checksum;
    use crate::slots::{KeyState, MacroEvent, EVENT_KEY, EVENT_MODIFIER};

    fn put(img: &mut [u8], at: usize, data: &[u8]) {
        img[at..at + data.len()].copy_from_slice(data);
        img[at + data.len()] = checksum(data);
    }

    fn put_slot(img: &mut [u8], at: usize, bytes: &[u8]) {
        img[at..at + bytes.len()].copy_from_slice(bytes);
    }

    fn ctrl_win_left() -> Macro {
        let ev = |state, kind, code, delay_ms| MacroEvent { state, kind, code, delay_ms };
        Macro {
            name: vec![0xA4, 0xC1, 0xA6, 0x5E, 0xA5, 0x68, b':', b'D'], // Big5, as found on the live mouse
            events: vec![
                ev(KeyState::Down, EVENT_MODIFIER, 0x01, 64),
                ev(KeyState::Down, EVENT_MODIFIER, 0x08, 144),
                ev(KeyState::Down, EVENT_KEY, 0x50, 71),
                ev(KeyState::Up, EVENT_KEY, 0x50, 50),
                ev(KeyState::Up, EVENT_MODIFIER, 0x01, 5),
                ev(KeyState::Up, EVENT_MODIFIER, 0x08, 0),
            ],
        }
    }

    /// Shaped like the owner's mouse: 1000 Hz, 6 stages, back/forward = volume via shortcut slots 3/4,
    /// a stale macro in slot 3, 0xC1 and 0xE1 erased, everything else 0xFF.
    fn owner_image() -> Vec<u8> {
        let mut img = vec![0xFF; IMAGE_LEN];
        put(&mut img, 0x00, &[0x01]);
        put(&mut img, 0x02, &[6]);
        put(&mut img, 0x04, &[2]);
        put(&mut img, 0x0A, &[1]);
        for (i, x) in [7u8, 15, 31, 63, 127, 239, 239, 239].into_iter().enumerate() {
            put(&mut img, 0x0C + 4 * i, &[x, x, 0]);
            put(&mut img, 0x2C + 4 * i, &[255, 0, i as u8]);
        }
        for (i, key) in [[1, 1, 0], [1, 2, 0], [1, 4, 0], [5, 0, 0], [5, 0, 0], [2, 1, 0]].iter().enumerate() {
            put(&mut img, 0x60 + 4 * i, key);
        }
        put(&mut img, 0xAB, &[1]);
        put(&mut img, 0xB7, &[6]);
        put_slot(&mut img, 0x160, &Shortcut::multimedia(0xEA).encode().unwrap());
        put_slot(&mut img, 0x180, &Shortcut::multimedia(0xE9).encode().unwrap());
        put_slot(&mut img, slots::macro_range(3).start, &ctrl_win_left().encode().unwrap());
        img
    }

    fn backup_of(image: Vec<u8>) -> Backup {
        Backup::new(1_761_040_000, 0xD3FB, 0x0313, Some(0x0102), image).unwrap()
    }

    fn apply(image: &[u8], writes: &[FlashWrite]) -> Vec<u8> {
        let mut out = image.to_vec();
        for w in writes {
            let at = usize::from(w.addr);
            out[at..at + w.bytes.len()].copy_from_slice(&w.bytes);
        }
        out
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
    fn serialize_parse_round_trip() {
        for b in [
            backup_of(owner_image()),
            Backup::new(0, 0xD3FC, 0, None, vec![0; IMAGE_LEN]).unwrap(),
            Backup::new(u64::MAX, 0xFFFF, 0xFFFF, Some(0), garbage(9, IMAGE_LEN)).unwrap(),
        ] {
            let text = b.serialize();
            assert_eq!(Backup::parse(&text), Ok(b.clone()));
            assert_eq!(Backup::parse(&text).unwrap().serialize(), text);
        }
    }

    #[test]
    fn serialized_layout() {
        let text = backup_of(owner_image()).serialize();
        assert!(text.starts_with("{\n  \"format\": \"hecatectl backup\",\n  \"version\": 1,\n"), "{text}");
        assert!(text.contains("  \"created_unix\": 1761040000,\n  \"product_id\": \"d3fb\",\n"));
        assert!(text.contains("  \"firmware\": \"0313\",\n  \"receiver\": \"0102\",\n"));
        assert!(text.contains("    \"0000: 01 54 06 4f 02 53 ff ff ff ff 01 54 07 07 00 47\",\n"));
        assert!(text.contains("    \"1af0: ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff\"\n  ]\n}\n"));
        assert_eq!(text.lines().filter(|l| l.starts_with("    \"")).count(), IMAGE_LEN / 16);
        let none = Backup::new(0, 1, 2, None, vec![0; IMAGE_LEN]).unwrap().serialize();
        assert!(none.contains("\"receiver\": null,"));
    }

    #[test]
    fn new_rejects_wrong_image_length() {
        assert!(Backup::new(0, 0, 0, None, Vec::new()).is_err());
        assert!(Backup::new(0, 0, 0, None, vec![0; IMAGE_LEN - 1]).is_err());
        assert!(Backup::new(0, 0, 0, None, vec![0; IMAGE_LEN + 1]).is_err());
    }

    #[test]
    fn parse_rejects_malformed_files() {
        let good = backup_of(owner_image()).serialize();
        let lines: Vec<&str> = good.lines().collect();
        let without_line = |n: usize| {
            let mut l = lines.clone();
            l.remove(n);
            l.join("\n")
        };
        let cases: Vec<(String, &str)> = vec![
            (String::new(), "invalid JSON"),
            ("[]".into(), "expected a JSON object"),
            (good[..good.len() / 2].into(), "invalid JSON"),
            (format!("{good} x"), "unexpected text after"),
            (good.replace("hecatectl backup", "other"), "not a hecatectl backup"),
            (good.replace("\"version\": 1", "\"version\": 2"), "version 2 is not supported"),
            (good.replace("\"version\": 1", "\"version\": \"1\""), "\"version\" must be a number"),
            (good.replace("\"version\": 1", "\"version\": 1.0"), "only whole numbers"),
            (good.replace("\"version\": 1", "\"version\": 01"), "invalid number"),
            (good.replace("\"created_unix\": 1761040000", "\"created_unix\": -1"), "\"created_unix\" must be 0..=2^64-1"),
            (good.replace("\"created_unix\": 1761040000", "\"created_unix\": 18446744073709551616"), "\"created_unix\" must be"),
            (good.replace("\"created_unix\": 1761040000", &format!("\"created_unix\": 1{}", "0".repeat(40))), "number out of range"),
            (good.replace("\"product_id\": \"d3fb\"", "\"product_id\": \"d3f\""), "\"product_id\" must be"),
            (good.replace("\"product_id\": \"d3fb\"", "\"product_id\": \"zzzz\""), "\"product_id\" must be"),
            (good.replace("\"firmware\": \"0313\"", "\"firmware\": 787"), "\"firmware\" must be"),
            (good.replace("\"receiver\": \"0102\"", "\"receiver\": true"), "\"receiver\" must be"),
            (good.replace("  \"firmware\": \"0313\",\n", ""), "missing field \"firmware\""),
            (good.replace("\"format\"", "\"note\": \"x\", \"format\""), "unknown field \"note\""),
            (good.replace("\"format\"", "\"version\": 1, \"format\""), "duplicate key \"version\""),
            (good.replace("\"image\": [", "\"image\": {").replace("  ]\n}", "  }\n}"), "invalid JSON"),
            (good.replace("\"0010: ", "\"0020: "), "image line 2 (0x0010): address"),
            (good.replace("\"0000: 01 54", "\"0000: 0g 54"), "image line 1 (0x0000): expected 16 hex bytes"),
            (good.replace("\"0000: 01 54", "\"0000: 01  54"), "expected 16 hex bytes"),
            (good.replace("\"0000: 01 54", "\"0000: 01 54 00"), "expected 16 hex bytes"),
            (good.replace("\"0000: 01 54", "\"0000:01 54"), "expected \"AAAA: xx xx ..\""),
            (good.replace("\"0000: 01 54 06 4f 02 53 ff ff ff ff 01 54 07 07 00 47\"", "0"), "image line 1 (0x0000): not a string"),
            (without_line(9), "has 431 lines"),
            (good.replace("\"format\"", &format!("\"deep\": {}0{}, \"format\"", "[".repeat(40), "]".repeat(40))), "nesting too deep"),
        ];
        for (text, want) in cases {
            match Backup::parse(&text) {
                Ok(_) => panic!("accepted a file that should fail with \"{want}\""),
                Err(e) => assert!(e.contains(want), "expected \"{want}\", got \"{e}\""),
            }
        }
    }

    #[test]
    fn parse_accepts_reformatted_json() {
        // Whitespace and upper-case hex do not matter; the image lines must keep their exact shape.
        let good = backup_of(owner_image()).serialize();
        let compact: String = good.lines().map(str::trim).collect::<Vec<_>>().join("");
        assert_eq!(Backup::parse(&compact), Ok(backup_of(owner_image())));
        let upper = good.replace("\"0000: 01 54 06 4f", "\"0000: 01 54 06 4F").replace("\"d3fb\"", "\"D3FB\"");
        assert_eq!(Backup::parse(&upper), Ok(backup_of(owner_image())));
    }

    #[test]
    fn json_reader() {
        assert_eq!(parse_json(" null "), Ok(Json::Null));
        let items = vec![Json::Bool(true), Json::Bool(false), Json::Arr(vec![]), Json::Obj(vec![])];
        assert_eq!(parse_json("[true,false,[],{}]"), Ok(Json::Arr(items)));
        assert_eq!(parse_json("-0"), Ok(Json::Int(0)));
        assert_eq!(parse_json("18446744073709551615"), Ok(Json::Int(u64::MAX.into())));
        assert_eq!(parse_json(r#""a\"\\\/\b\f\n\r\t\u00e9\u5b8f""#), Ok(Json::Str("a\"\\/\u{8}\u{c}\n\r\té宏".into())));
        let fields = vec![("a".into(), Json::Int(1)), ("b".into(), Json::Arr(vec![Json::Int(2)]))];
        assert_eq!(parse_json(r#"{"a":1,"b":[2]}"#), Ok(Json::Obj(fields)));
        let bad_syntax = ["", "nul", "[1,]", "[1 2]", "{\"a\" 1}", "{1:2}", "{\"a\":1,}", "-", "1e5", "+1"];
        let bad_strings = ["\"abc", "\"\\x\"", "\"\\ud800\"", "\"\\u12\"", "\"a\nb\""];
        for bad in bad_syntax.into_iter().chain(bad_strings) {
            assert!(parse_json(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn identical_state_plans_nothing() {
        let img = owner_image();
        let plan = plan_restore(0xD3FB, &img, &backup_of(img.clone())).unwrap();
        assert_eq!(plan, RestorePlan { writes: vec![], skipped: vec![] });
        let erased = vec![0xFF; IMAGE_LEN];
        assert_eq!(plan_restore(0xD3FB, &erased, &backup_of(erased.clone())).unwrap().writes, vec![]);
    }

    #[test]
    fn one_rate_change_is_one_write() {
        let mut current = owner_image();
        put(&mut current, 0x00, &[0x08]); // the mouse went back to 125 Hz
        let plan = plan_restore(0xD3FB, &current, &backup_of(owner_image())).unwrap();
        assert_eq!(plan.writes, [FlashWrite { addr: 0x0000, bytes: vec![0x01, 0x54] }]);
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn a_differing_slot_is_one_whole_slot_write() {
        let mut current = owner_image();
        put_slot(&mut current, 0x160, &Shortcut::multimedia(0xE2).encode().unwrap()); // mute instead of volume down
        let plan = plan_restore(0xD3FB, &current, &backup_of(owner_image())).unwrap();
        assert_eq!(plan.writes, [FlashWrite { addr: 0x160, bytes: Shortcut::multimedia(0xEA).encode().unwrap() }]);

        let mut current = owner_image();
        current[slots::macro_range(3).start + 0x22] ^= 0x01; // corrupt the macro: it no longer decodes
        let plan = plan_restore(0xD3FB, &current, &backup_of(owner_image())).unwrap();
        assert_eq!(plan.writes, [FlashWrite { addr: 0x780, bytes: ctrl_win_left().encode().unwrap() }]);
    }

    #[test]
    fn slots_empty_in_the_backup_get_the_disabled_form() {
        let mut current = owner_image();
        put_slot(&mut current, 0x1A0, &Shortcut::multimedia(0xCD).encode().unwrap()); // slot 5 used now
        put_slot(&mut current, slots::macro_range(7).start, &ctrl_win_left().encode().unwrap());
        let mut target = owner_image();
        // backup macro slot 9: name kept but no events, as the vendor leaves it
        put_slot(&mut target, slots::macro_range(9).start, &slots::disabled_macro(b"old").unwrap());
        put_slot(&mut current, slots::macro_range(9).start, &ctrl_win_left().encode().unwrap());
        let plan = plan_restore(0xD3FB, &current, &backup_of(target.clone())).unwrap();
        let mut erased_macro = vec![0x00];
        erased_macro.extend([0xFF; 30]);
        erased_macro.extend([0x00, 0x55]);
        let mut named = vec![3, b'o', b'l', b'd'];
        named.extend([0xFF; 27]);
        named.extend([0x00, 0x55]);
        assert_eq!(
            plan.writes,
            [
                FlashWrite { addr: 0x1A0, bytes: vec![0x00, 0xFF] },
                FlashWrite { addr: slots::macro_range(7).start as u16, bytes: erased_macro },
                FlashWrite { addr: slots::macro_range(9).start as u16, bytes: named },
            ]
        );
        let after = apply(&current, &plan.writes);
        assert_eq!(Slots::from_image(&after), Slots::from_image(&target));
    }

    #[test]
    fn product_mismatch_is_refused() {
        let img = owner_image();
        let err = plan_restore(0xD3FC, &img, &backup_of(img.clone())).unwrap_err();
        assert!(err.contains("d3fb") && err.contains("d3fc") && err.contains("refusing"), "{err}");
    }

    #[test]
    fn wrong_current_length_is_refused() {
        let b = backup_of(owner_image());
        assert!(plan_restore(0xD3FB, &[], &b).is_err());
        assert!(plan_restore(0xD3FB, &vec![0xFF; IMAGE_LEN - 1], &b).is_err());
    }

    #[test]
    fn unmodelled_and_invalid_bytes_are_reported_not_written() {
        let mut current = owner_image();
        current[0x06] = 0x00; // xSpindown group, UNCONFIRMED, not modelled
        current[0xBB] = 0x12; // between groups
        current[0xBC] = 0x34;
        current[0xF0] = 0x00; // tail of the block
        put(&mut current, 0xC1, &[1]); // valid now, erased in the backup
        let plan = plan_restore(0xD3FB, &current, &backup_of(owner_image())).unwrap();
        assert_eq!(plan.writes, vec![]);
        let skipped: Vec<(Range<usize>, SkipReason)> = plan.skipped.iter().map(|s| (s.range.clone(), s.reason)).collect();
        assert_eq!(
            skipped,
            [
                (0x06..0x07, SkipReason::Unmodelled),
                (0xBB..0xBD, SkipReason::Unmodelled),
                (0xC1..0xC3, SkipReason::InvalidInBackup),
                (0xF0..0xF1, SkipReason::Unmodelled),
            ]
        );
    }

    #[test]
    fn restore_after_reset_converges_and_touches_only_vendor_ranges() {
        // A factory-reset-like device: different settings everywhere, all slots erased.
        let mut current = vec![0xFF; IMAGE_LEN];
        put(&mut current, 0x00, &[0x08]);
        put(&mut current, 0x02, &[4]);
        put(&mut current, 0x04, &[0]);
        for i in 0..8 {
            put(&mut current, 0x0C + 4 * i, &[0x1F, 0x1F, 0]);
        }
        for i in 0..16 {
            put(&mut current, 0x60 + 4 * i, &[1, 1, 1]);
        }
        let backup = backup_of(owner_image());
        let plan = plan_restore(0xD3FB, &current, &backup).unwrap();
        assert!(!plan.writes.is_empty());
        // vendor order: settings, then shortcut slots, then macro slots
        let kind = |addr: u16| match addr {
            0..=0xFF => 0,
            0x100..=0x2FF => 1,
            _ => 2,
        };
        let kinds: Vec<u8> = plan.writes.iter().map(|w| kind(w.addr)).collect();
        assert!(kinds.windows(2).all(|k| k[0] <= k[1]), "{kinds:?}");
        assert_eq!(check_writes(&plan.writes), Ok(()));
        let after = apply(&current, &plan.writes);
        assert_eq!(plan_restore(0xD3FB, &after, &backup).unwrap().writes, vec![]);
        assert_eq!(Slots::from_image(&after), backup.slots());
        assert!(settings::diff(&settings::decode(&block(&after)), &backup.settings())
            .iter()
            .all(|w| !settings::group_ranges(&backup.settings()).iter().any(|(r, v)| *v && r.start == usize::from(w.addr))));
        // bytes outside every write are untouched
        let span = |w: &FlashWrite| usize::from(w.addr)..usize::from(w.addr) + w.bytes.len();
        let written: Vec<Range<usize>> = plan.writes.iter().map(span).collect();
        for at in (0..IMAGE_LEN).filter(|at| !written.iter().any(|r| r.contains(at))) {
            assert_eq!(after[at], current[at], "0x{at:04x}");
        }
    }

    #[test]
    fn check_writes_allows_only_vendor_shapes() {
        let w = |addr: u16, bytes: Vec<u8>| vec![FlashWrite { addr, bytes }];
        assert_eq!(check_writes(&[]), Ok(()));
        assert_eq!(check_writes(&w(0x00, vec![0x01, 0x54])), Ok(()));
        assert_eq!(check_writes(&w(0xA0, vec![0; 7])), Ok(()));
        assert_eq!(check_writes(&w(0x1A0, vec![0x00, 0xFF])), Ok(()));
        assert_eq!(check_writes(&w(0x160, Shortcut::multimedia(0xEA).encode().unwrap())), Ok(()));
        assert_eq!(check_writes(&w(0x780, ctrl_win_left().encode().unwrap())), Ok(()));
        assert_eq!(check_writes(&w(0x300, slots::disabled_macro(b"x").unwrap())), Ok(()));
        for (addr, bytes) in [
            (0x00, vec![0x01]),                              // half a group
            (0x06, vec![0x00, 0x55]),                        // UNCONFIRMED group
            (0xBB, vec![0x00]),                              // between groups
            (0xDB, vec![0, 0, 0, 0x55]),                     // buggy vendor group
            (0x101, vec![0x00, 0xFF]),                       // not a slot start
            (0x160, vec![0x02, 0x82]),                       // shortcut cut short
            (0x160, vec![]),                                 // empty
            (0x300, vec![0xFF; 0x20]),                       // macro without count byte
            (0x300, vec![0xFF; 0x21]),                       // count 0xFF is not a macro
            (0x1B00, vec![0x00, 0xFF]),                      // past the used flash
        ] {
            assert!(check_writes(&w(addr, bytes.clone())).is_err(), "0x{addr:04x} {bytes:02x?}");
        }
    }

    #[test]
    fn merges_runs() {
        assert_eq!(runs([].into_iter()), Vec::<Range<usize>>::new());
        let single = runs([5].into_iter());
        assert_eq!((single.len(), single[0].clone()), (1, 5..6));
        assert_eq!(runs([1, 2, 3, 7, 8, 10].into_iter()), [1..4, 7..9, 10..11]);
    }

    #[test]
    fn formats_utc() {
        assert_eq!(utc_text(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(utc_text(86_399), "1970-01-01 23:59:59 UTC");
        assert_eq!(utc_text(951_782_400), "2000-02-29 00:00:00 UTC");
        assert_eq!(utc_text(1_761_040_000), "2025-10-21 09:46:40 UTC");
        assert_eq!(utc_text(4_102_444_800), "2100-01-01 00:00:00 UTC");
        assert_eq!(utc_text(253_402_300_799), "9999-12-31 23:59:59 UTC");
        assert!(utc_text(u64::MAX).ends_with(" UTC"));
    }

    #[test]
    fn backup_decodes_its_settings_and_slots() {
        let b = backup_of(owner_image());
        assert_eq!(b.settings().report_rate.value, 0x01);
        assert_eq!(b.settings().keys[3].value.kind, 5);
        assert_eq!(b.slots().shortcuts[3], Some(Shortcut::multimedia(0xEA)));
        assert_eq!(b.slots().macros[3], Some(ctrl_win_left()));
        assert_eq!(b.image().len(), IMAGE_LEN);
    }
}
