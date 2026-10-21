//! Typed model of the settings block at flash 0x000-0x0FF (docs/protocol.md §9).
//!
//! Every field is a group of data bytes followed by one check byte `0x55 - sum(data)`.
//! `decode` follows the vendor parser (HIDUsb.dll FUN_180005840 / FUN_1800063c0): a group whose check
//! byte is wrong is reported invalid and carries the default that parser substitutes.
//! `diff` follows the vendor writer (FUN_180005ca0 / FUN_180006630): only changed groups, each written
//! whole with its check byte, in the vendor's order.
//! Groups whose meaning is UNCONFIRMED (0x06, 0x08, 0xC3-0xD6, 0xD7, 0xD9, 0xDB, 0xDF, 0xE3, 0xE5) and
//! bytes outside any group are not modelled: they stay in the raw block and are never written.

use crate::protocol::{self, checksum, CONFIG_BLOCK_LEN};

pub type Block = [u8; CONFIG_BLOCK_LEN];

pub const DPI_STAGES: usize = 8;
pub const KEYS: usize = 16;
/// Stage counts offered by the UI (FUN_140236460 clamps maxDPI to 1..=6).
pub const MAX_SELECTABLE_STAGES: u8 = 6;

pub const DPI_MIN: u32 = 50;
pub const DPI_MAX: u32 = 32000;
/// Steps of 50 up to here, 100 above (FUN_140236030).
const DPI_FINE_MAX: u32 = 6000;
/// Above this the register holds DPI/100 with flag 0x22 instead of DPI/50 - 1 (FUN_1402367d0).
const DPI_LOW_RANGE_MAX: u32 = 26000;

/// Values the UI writes (HECATE.exe handlers).
const LOD_CHOICES: [u8; 2] = [1, 2]; // FUN_140237fd0
const LED_OFF_TIME_CHOICES: [u8; 8] = [6, 30, 60, 90, 120, 150, 180, 210]; // FUN_140242ba0
/// FUN_140231ec0 labels: 30s, 1, 2, 5, 10, 15 min. Its index 4 (10 min = 60) falls through and
/// writes 90 (vendor bug); we write the labelled value.
const SLEEP_TIME_CHOICES: [u8; 6] = [3, 6, 12, 30, 60, 90];
const POWER_MODE_CHOICES: [u8; 2] = [POWER_LP, POWER_HP]; // FUN_140237ee0

pub const POWER_LP: u8 = 0;
pub const POWER_HP: u8 = 1;

/// Flash addresses of the modelled groups (FUN_180005840 / FUN_1800063c0).
mod addr {
    pub const REPORT_RATE: usize = 0x00;
    pub const DPI_STAGE_COUNT: usize = 0x02;
    pub const CURRENT_DPI_STAGE: usize = 0x04;
    pub const LOD: usize = 0x0A;
    pub const DPI_VALUE: usize = 0x0C; // + 4 * stage
    pub const DPI_COLOR: usize = 0x2C; // + 4 * stage
    pub const DPI_LED_MODE: usize = 0x4C;
    pub const DPI_LED_BRIGHTNESS: usize = 0x4E;
    pub const DPI_LED_BREATH_SPEED: usize = 0x50;
    pub const DPI_LED_ENABLE: usize = 0x52;
    pub const KEY: usize = 0x60; // + 4 * key
    pub const LED_BAR: usize = 0xA0;
    pub const LED_BAR_ENABLE: usize = 0xA7;
    pub const KEY_DEBOUNCE: usize = 0xA9;
    pub const MOTION_SYNC: usize = 0xAB;
    pub const ALL_LED_OFF_TIME: usize = 0xAD;
    pub const LINEAR_CORRECTION: usize = 0xAF;
    pub const RIPPLE_CONTROL: usize = 0xB1;
    pub const MOVE_OFF_LED: usize = 0xB3;
    pub const CUSTOM_SLEEP: usize = 0xB5;
    pub const SENSOR_SLEEP_TIME: usize = 0xB7;
    pub const POWER_MODE: usize = 0xB9;
    pub const ANGLE_TUNE: usize = 0xBD;
    pub const ANGLE_TUNE_ENABLE: usize = 0xBF;
    pub const WHEEL_CHANGES_DPI: usize = 0xC1;
    pub const FURY_MODE: usize = 0xE1;
}

/// One decoded group. `valid` is false when its check byte was wrong; `value` is then the default
/// the vendor parser uses instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checked<T> {
    pub value: T,
    pub valid: bool,
}

impl<T> Checked<T> {
    fn from_group(value: Option<T>, default: T) -> Self {
        match value {
            Some(value) => Self { value, valid: true },
            None => Self { value: default, valid: false },
        }
    }

    /// Sets a value to be written. An invalid group set to its default is still written,
    /// which repairs its check byte.
    pub fn set(&mut self, value: T) {
        self.value = value;
        self.valid = true;
    }
}

/// DPI register of one stage: `[xDPI, yDPI, DPIex]` (§9.1). The vendor UI writes x == y and has
/// no separate Y value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpiValue {
    pub x: u8,
    pub y: u8,
    pub ex: u8,
}

impl DpiValue {
    pub fn from_dpi(dpi: u32) -> Result<Self, String> {
        let (x, y, ex) = encode_dpi(dpi)?;
        Ok(Self { x, y, ex })
    }

    /// The DPI the vendor UI shows for this register.
    pub fn dpi(&self) -> u32 {
        decode_dpi(self.x, self.ex)
    }

    /// True when the register is exactly what the vendor UI writes for `dpi()`.
    pub fn is_selectable(&self) -> bool {
        encode_dpi(self.dpi()) == Ok((self.x, self.y, self.ex))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// DPI indicator LED: four separate `[v, check]` groups at 0x4C..0x53.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DpiLed {
    pub mode: Checked<u8>,
    pub brightness: Checked<u8>,
    pub breath_speed: Checked<u8>,
    pub enable: Checked<u8>,
}

/// LED bar: one group `[mode, R, G, B, speed, brightness, check]` at 0xA0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedBar {
    pub mode: u8,
    pub color: Rgb,
    pub speed: u8,
    pub brightness: u8,
}

/// Button mapping `[type, param1, param2]`. Type semantics are UNCONFIRMED (§9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBinding {
    pub kind: u8,
    pub param1: u8,
    pub param2: u8,
}

/// The settings block. Field comments give the flash address and the vendor JSON key.
/// Units of the time values are UNCONFIRMED.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// 0x00 reportRate: rate code (`protocol::rate_hz`).
    pub report_rate: Checked<u8>,
    /// 0x02 maxDPI: number of DPI stages.
    pub dpi_stage_count: Checked<u8>,
    /// 0x04 currentDPI: active stage, 0-based. Invalid as well when not below the stage count.
    pub current_dpi_stage: Checked<u8>,
    /// 0x0A silenceHeight: lift-off distance, UI "1" / "2".
    pub lod: Checked<u8>,
    /// 0x0C + 4i DPIConfig[i].xDPI/yDPI/DPIex.
    pub dpi_stages: [Checked<DpiValue>; DPI_STAGES],
    /// 0x2C + 4i DPIConfig[i].Red/Green/Blue.
    pub dpi_colors: [Checked<Rgb>; DPI_STAGES],
    /// 0x4C..0x53 DPILed.
    pub dpi_led: DpiLed,
    /// 0x60 + 4i Keys[i].
    pub keys: [Checked<KeyBinding>; KEYS],
    /// 0xA0 LedBar.
    pub led_bar: Checked<LedBar>,
    /// 0xA7 LedBar.enable.
    pub led_bar_enable: Checked<u8>,
    /// 0xA9 keyDebounceTime.
    pub key_debounce: Checked<u8>,
    /// 0xAB motionSyncEnable.
    pub motion_sync: Checked<bool>,
    /// 0xAD allLedOffTime.
    pub all_led_off_time: Checked<u8>,
    /// 0xAF linearCorrectionEnable.
    pub linear_correction: Checked<bool>,
    /// 0xB1 rippleControlEnable.
    pub ripple_control: Checked<bool>,
    /// 0xB3 moveOffLedEnable.
    pub move_off_led: Checked<bool>,
    /// 0xB5 sensorCustomSleepTimeEnable (UI "Peak Performance").
    pub custom_sleep: Checked<bool>,
    /// 0xB7 sensorSleepTime.
    pub sensor_sleep_time: Checked<u8>,
    /// 0xB9 sensorPowerSavingModeEnable: `POWER_LP` or `POWER_HP`.
    pub power_mode: Checked<u8>,
    /// 0xBD sensorAngleTune.
    pub angle_tune: Checked<u8>,
    /// 0xBF enableSensorAngleTune.
    pub angle_tune_enable: Checked<u8>,
    /// 0xC1 enableWheelToChangeDPI.
    pub wheel_changes_dpi: Checked<u8>,
    /// 0xE1 sensor_fps20KEnable (UI "Fury Mode").
    pub fury_mode: Checked<u8>,
    raw: Block,
}

impl Settings {
    /// The block as read, including the bytes that are not modelled.
    pub fn raw(&self) -> &Block {
        &self.raw
    }
}

/// One queued flash write: group data followed by its check byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashWrite {
    pub addr: u16,
    pub bytes: Vec<u8>,
}

/// `N` data bytes at `addr`, or None when the check byte after them is wrong.
fn group<const N: usize>(block: &Block, addr: usize) -> Option<[u8; N]> {
    let data: [u8; N] = block[addr..addr + N].try_into().ok()?;
    (block[addr + N] == checksum(&data)).then_some(data)
}

/// Never fails: every byte pattern decodes, wrong check bytes give invalid groups.
pub fn decode(block: &Block) -> Settings {
    let byte = |at: usize, default: u8| Checked::from_group(group::<1>(block, at).map(|[v]| v), default);
    // The parser stores these as `v != 0` (FUN_180005840).
    let flag = |at: usize, default: bool| Checked::from_group(group::<1>(block, at).map(|[v]| v != 0), default);

    let dpi_stage_count = byte(addr::DPI_STAGE_COUNT, 6);
    let mut current_dpi_stage = byte(addr::CURRENT_DPI_STAGE, 2);
    if current_dpi_stage.value >= dpi_stage_count.value {
        current_dpi_stage = Checked { value: 2, valid: false };
    }
    Settings {
        report_rate: byte(addr::REPORT_RATE, 0x01),
        dpi_stage_count,
        current_dpi_stage,
        lod: byte(addr::LOD, 0),
        dpi_stages: std::array::from_fn(|i| {
            let value = group::<3>(block, addr::DPI_VALUE + 4 * i).map(|[x, y, ex]| DpiValue { x, y, ex });
            Checked::from_group(value, DpiValue { x: 0xFF, y: 0xFF, ex: 0 })
        }),
        dpi_colors: std::array::from_fn(|i| {
            let value = group::<3>(block, addr::DPI_COLOR + 4 * i).map(|[r, g, b]| Rgb { r, g, b });
            Checked::from_group(value, Rgb { r: 0xFF, g: 0xFF, b: 0xFF })
        }),
        dpi_led: DpiLed {
            mode: byte(addr::DPI_LED_MODE, 2),
            brightness: byte(addr::DPI_LED_BRIGHTNESS, 0x80),
            breath_speed: byte(addr::DPI_LED_BREATH_SPEED, 3),
            enable: byte(addr::DPI_LED_ENABLE, 1),
        },
        keys: std::array::from_fn(|i| {
            let value = group::<3>(block, addr::KEY + 4 * i)
                .map(|[kind, param1, param2]| KeyBinding { kind, param1, param2 });
            Checked::from_group(value, KeyBinding { kind: 0, param1: 0, param2: 0 })
        }),
        led_bar: Checked::from_group(
            group::<6>(block, addr::LED_BAR).map(|[mode, r, g, b, speed, brightness]| LedBar {
                mode,
                color: Rgb { r, g, b },
                speed,
                brightness,
            }),
            LedBar { mode: 1, color: Rgb { r: 0xFF, g: 0xFF, b: 0xFF }, speed: 8, brightness: 9 },
        ),
        led_bar_enable: byte(addr::LED_BAR_ENABLE, 1),
        key_debounce: byte(addr::KEY_DEBOUNCE, 8),
        motion_sync: flag(addr::MOTION_SYNC, true),
        all_led_off_time: byte(addr::ALL_LED_OFF_TIME, 6),
        linear_correction: flag(addr::LINEAR_CORRECTION, false),
        ripple_control: flag(addr::RIPPLE_CONTROL, false),
        move_off_led: flag(addr::MOVE_OFF_LED, false),
        custom_sleep: flag(addr::CUSTOM_SLEEP, false),
        sensor_sleep_time: byte(addr::SENSOR_SLEEP_TIME, 6),
        power_mode: byte(addr::POWER_MODE, POWER_LP),
        // No parser default for these four: the full read zeroes the struct first (FUN_180014510).
        angle_tune: byte(addr::ANGLE_TUNE, 0),
        angle_tune_enable: byte(addr::ANGLE_TUNE_ENABLE, 0),
        wheel_changes_dpi: byte(addr::WHEEL_CHANGES_DPI, 0),
        fury_mode: byte(addr::FURY_MODE, 0),
        raw: *block,
    }
}

trait GroupData {
    fn data(&self) -> Vec<u8>;
}

impl GroupData for u8 {
    fn data(&self) -> Vec<u8> {
        vec![*self]
    }
}

impl GroupData for bool {
    fn data(&self) -> Vec<u8> {
        vec![u8::from(*self)]
    }
}

impl GroupData for DpiValue {
    fn data(&self) -> Vec<u8> {
        vec![self.x, self.y, self.ex]
    }
}

impl GroupData for Rgb {
    fn data(&self) -> Vec<u8> {
        vec![self.r, self.g, self.b]
    }
}

impl GroupData for LedBar {
    fn data(&self) -> Vec<u8> {
        vec![self.mode, self.color.r, self.color.g, self.color.b, self.speed, self.brightness]
    }
}

impl GroupData for KeyBinding {
    fn data(&self) -> Vec<u8> {
        vec![self.kind, self.param1, self.param2]
    }
}

struct Entry {
    addr: usize,
    data: Vec<u8>,
    valid: bool,
}

/// Every modelled group in the vendor writer's order: FUN_180005ca0 scalars, then FUN_180006630
/// (per stage DPI value and colour, DPI LED, LED bar and its enable, keys).
fn entries(s: &Settings) -> Vec<Entry> {
    fn add<T: GroupData>(out: &mut Vec<Entry>, addr: usize, c: &Checked<T>) {
        out.push(Entry { addr, data: c.value.data(), valid: c.valid });
    }
    let mut e = Vec::new();
    add(&mut e, addr::REPORT_RATE, &s.report_rate);
    add(&mut e, addr::DPI_STAGE_COUNT, &s.dpi_stage_count);
    add(&mut e, addr::CURRENT_DPI_STAGE, &s.current_dpi_stage);
    add(&mut e, addr::LOD, &s.lod);
    add(&mut e, addr::KEY_DEBOUNCE, &s.key_debounce);
    add(&mut e, addr::MOTION_SYNC, &s.motion_sync);
    add(&mut e, addr::ALL_LED_OFF_TIME, &s.all_led_off_time);
    add(&mut e, addr::LINEAR_CORRECTION, &s.linear_correction);
    add(&mut e, addr::RIPPLE_CONTROL, &s.ripple_control);
    add(&mut e, addr::MOVE_OFF_LED, &s.move_off_led);
    add(&mut e, addr::CUSTOM_SLEEP, &s.custom_sleep);
    add(&mut e, addr::SENSOR_SLEEP_TIME, &s.sensor_sleep_time);
    add(&mut e, addr::POWER_MODE, &s.power_mode);
    add(&mut e, addr::ANGLE_TUNE, &s.angle_tune);
    add(&mut e, addr::ANGLE_TUNE_ENABLE, &s.angle_tune_enable);
    add(&mut e, addr::WHEEL_CHANGES_DPI, &s.wheel_changes_dpi);
    add(&mut e, addr::FURY_MODE, &s.fury_mode);
    for i in 0..DPI_STAGES {
        add(&mut e, addr::DPI_VALUE + 4 * i, &s.dpi_stages[i]);
        add(&mut e, addr::DPI_COLOR + 4 * i, &s.dpi_colors[i]);
    }
    add(&mut e, addr::DPI_LED_MODE, &s.dpi_led.mode);
    add(&mut e, addr::DPI_LED_BRIGHTNESS, &s.dpi_led.brightness);
    add(&mut e, addr::DPI_LED_BREATH_SPEED, &s.dpi_led.breath_speed);
    add(&mut e, addr::DPI_LED_ENABLE, &s.dpi_led.enable);
    add(&mut e, addr::LED_BAR, &s.led_bar);
    add(&mut e, addr::LED_BAR_ENABLE, &s.led_bar_enable);
    for (i, key) in s.keys.iter().enumerate() {
        add(&mut e, addr::KEY + 4 * i, key);
    }
    e
}

/// Flash range (data and check byte) and validity of every modelled group, in the vendor writer's
/// order. Bytes outside these ranges are never written.
pub fn group_ranges(s: &Settings) -> Vec<(std::ops::Range<usize>, bool)> {
    entries(s).into_iter().map(|e| (e.addr..e.addr + e.data.len() + 1, e.valid)).collect()
}

/// Group ranges whose stored bytes differ between a snapshot and a fresh read of the device. Empty means
/// the snapshot still describes every group a diff could write. Unmodelled bytes are not compared.
pub fn stale_groups(snapshot: &Settings, device: &Settings) -> Vec<std::ops::Range<usize>> {
    group_ranges(snapshot)
        .into_iter()
        .map(|(r, _)| r)
        .filter(|r| snapshot.raw[r.clone()] != device.raw[r.clone()])
        .collect()
}

fn changed<T: PartialEq>(old: &Checked<T>, new: &Checked<T>) -> bool {
    old.value != new.value || (!old.valid && new.valid)
}

/// Writes that turn `old` (what flash holds) into `new`: one per changed group, data plus check byte,
/// in the vendor's order. A group also counts as changed when `old` is invalid and `new` was `set`.
pub fn diff(old: &Settings, new: &Settings) -> Vec<FlashWrite> {
    entries(old)
        .into_iter()
        .zip(entries(new))
        .filter(|(o, n)| o.data != n.data || (!o.valid && n.valid))
        .map(|(_, mut n)| {
            n.data.push(checksum(&n.data));
            FlashWrite { addr: n.addr as u16, bytes: n.data }
        })
        .collect()
}

fn choice(name: &str, old: &Checked<u8>, new: &Checked<u8>, allowed: &[u8]) -> Result<(), String> {
    if changed(old, new) && !allowed.contains(&new.value) {
        return Err(format!("{name} {} is not one of the vendor choices {allowed:?}", new.value));
    }
    Ok(())
}

/// Rejects changed values the vendor UI cannot produce. Unchanged fields are not checked, so a
/// block with odd stored values can still be edited elsewhere.
pub fn validate(old: &Settings, new: &Settings) -> Result<(), String> {
    let rate = new.report_rate.value;
    if changed(&old.report_rate, &new.report_rate) && !protocol::SETTABLE_RATES.iter().any(|(_, c)| *c == rate) {
        return Err(format!("report rate code 0x{rate:02x} is not offered for this mouse"));
    }
    let count = new.dpi_stage_count.value;
    let count_changed = changed(&old.dpi_stage_count, &new.dpi_stage_count);
    if count_changed && !(1..=MAX_SELECTABLE_STAGES).contains(&count) {
        return Err(format!("{count} DPI stages: the vendor UI offers 1 to {MAX_SELECTABLE_STAGES}"));
    }
    let current = new.current_dpi_stage.value;
    if (count_changed || changed(&old.current_dpi_stage, &new.current_dpi_stage)) && current >= count {
        return Err(format!("active DPI stage {current} (0-based) is not below the stage count {count}"));
    }
    for (i, (o, n)) in old.dpi_stages.iter().zip(&new.dpi_stages).enumerate() {
        let v = n.value;
        if changed(o, n) && !v.is_selectable() {
            return Err(format!("DPI stage {i}: register {:02x} {:02x} {:02x} is not a selectable DPI", v.x, v.y, v.ex));
        }
    }
    choice("LOD", &old.lod, &new.lod, &LOD_CHOICES)?;
    choice("LED-off time", &old.all_led_off_time, &new.all_led_off_time, &LED_OFF_TIME_CHOICES)?;
    choice("sensor sleep time", &old.sensor_sleep_time, &new.sensor_sleep_time, &SLEEP_TIME_CHOICES)?;
    choice("power mode", &old.power_mode, &new.power_mode, &POWER_MODE_CHOICES)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// Checks that every write reads back byte for byte in `block` (the block re-read after writing).
pub fn verify(writes: &[FlashWrite], block: &Block) -> Result<(), String> {
    for w in writes {
        let at = usize::from(w.addr);
        let got = block.get(at..at + w.bytes.len()).unwrap_or(&[]);
        if got != w.bytes.as_slice() {
            return Err(format!(
                "write at 0x{:04x} was acknowledged but reads back as [{}] instead of [{}]",
                w.addr,
                hex(got),
                hex(&w.bytes)
            ));
        }
    }
    Ok(())
}

/// DPI -> `(xDPI, yDPI, DPIex)` for sensor 3395, max 32000 (HECATE.exe FUN_140236030 -> FUN_1402367d0).
/// Accepts only what the vendor UI can produce: 50..=6000 in steps of 50, then steps of 100 up to 32000.
pub fn encode_dpi(dpi: u32) -> Result<(u8, u8, u8), String> {
    let step = if dpi <= DPI_FINE_MAX { 50 } else { 100 };
    if !(DPI_MIN..=DPI_MAX).contains(&dpi) || !dpi.is_multiple_of(step) {
        return Err(format!(
            "{dpi} DPI is not selectable ({DPI_MIN}..={DPI_FINE_MAX} in steps of 50, then steps of 100 up to {DPI_MAX})"
        ));
    }
    let (v, flag) = if dpi <= DPI_LOW_RANGE_MAX { (dpi / 50 - 1, 0) } else { (dpi / 100, 0x22) };
    let hi = (v >> 8) as u8;
    let lo = v as u8;
    Ok((lo, lo, hi << 2 | hi << 6 | flag))
}

/// Stored `(xDPI, DPIex)` -> DPI as the vendor UI shows it (HECATE.exe FUN_140235080 / FUN_140235930,
/// branch max 32000 and sensor != 0xF6E): high bits from `ex` bits 6..7, x100 when `ex & 3` is set.
/// yDPI and `ex` bits 2..3 are not read. This replaces the inferred decoder of protocol.md §9.1;
/// both agree on every register `encode_dpi` produces.
pub fn decode_dpi(x: u8, ex: u8) -> u32 {
    let v = u32::from(ex >> 6) << 8 | u32::from(x);
    if ex & 0x03 != 0 {
        v * 100
    } else {
        v * 50 + 50
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes `data` and its check byte at `at`.
    fn put(b: &mut Block, at: usize, data: &[u8]) {
        b[at..at + data.len()].copy_from_slice(data);
        b[at + data.len()] = checksum(data);
    }

    /// The vendor default profile, hand-copied from HECATE 1.1.44
    /// `data/deviceresource/g3mpro_upgrades/defaultconfiguration.json`. The JSON has no value for 0xE1
    /// (fury mode) and its sensitivity floats are UNCONFIRMED, so those bytes stay erased (0xFF).
    fn vendor_block() -> Block {
        let mut b = [0xFF; CONFIG_BLOCK_LEN];
        put(&mut b, 0x00, &[1]); // reportRate
        put(&mut b, 0x02, &[6]); // maxDPI
        put(&mut b, 0x04, &[2]); // currentDPI
        put(&mut b, 0x06, &[0]); // xSpindown
        put(&mut b, 0x08, &[0]); // ySpindown
        put(&mut b, 0x0A, &[0]); // silenceHeight
        let dpi = [7, 15, 31, 63, 127, 239, 239, 239];
        let colors = [
            [255, 0, 0], [0, 255, 0], [0, 255, 150], [255, 100, 0], [0, 0, 255], [255, 0, 255], [255, 0, 255], [255, 0, 255],
        ];
        for i in 0..8 {
            put(&mut b, 0x0C + 4 * i, &[dpi[i], dpi[i], 0]);
            put(&mut b, 0x2C + 4 * i, &colors[i]);
        }
        put(&mut b, 0x4C, &[2]); // DPILed.mode
        put(&mut b, 0x4E, &[128]); // DPILed.brightness
        put(&mut b, 0x50, &[3]); // DPILed.breathSpeed
        put(&mut b, 0x52, &[0]); // DPILed.enable
        // Keys[0..11]; Keys[11..16] are all zero
        let keys = [[1, 1, 1], [1, 2, 2], [1, 4, 4], [1, 8, 8], [1, 16, 16], [2, 1, 1]]
            .into_iter()
            .chain([[0, 0, 0], [0, 0, 0], [7, 0, 0], [2, 2, 2], [2, 3, 3]]);
        for (i, key) in keys.chain(std::iter::repeat([0, 0, 0])).take(16).enumerate() {
            put(&mut b, 0x60 + 4 * i, &key);
        }
        put(&mut b, 0xA0, &[1, 255, 0, 255, 7, 9]); // LedBar mode, R, G, B, speed, brightness
        put(&mut b, 0xA7, &[0]); // LedBar.enable
        put(&mut b, 0xA9, &[8]); // keyDebounceTime
        put(&mut b, 0xAB, &[1]); // motionSyncEnable
        put(&mut b, 0xAD, &[6]); // allLedOffTime
        put(&mut b, 0xAF, &[0]); // linearCorrectionEnable
        put(&mut b, 0xB1, &[0]); // rippleControlEnable
        put(&mut b, 0xB3, &[0]); // moveOffLedEnable
        put(&mut b, 0xB5, &[0]); // sensorCustomSleepTimeEnable
        put(&mut b, 0xB7, &[6]); // sensorSleepTime
        put(&mut b, 0xB9, &[0]); // sensorPowerSavingModeEnable
        put(&mut b, 0xBD, &[2]); // sensorAngleTune
        put(&mut b, 0xBF, &[1]); // enableSensorAngleTune
        put(&mut b, 0xC1, &[1]); // enableWheelToChangeDPI
        b
    }

    /// Flash ranges (group data plus check) that may be written, straight from protocol.md §9.
    fn writable_ranges() -> Vec<std::ops::Range<usize>> {
        let one_byte = [0x00, 0x02, 0x04, 0x0A, 0x4C, 0x4E, 0x50, 0x52, 0xA7]
            .into_iter()
            .chain([0xA9, 0xAB, 0xAD, 0xAF, 0xB1, 0xB3, 0xB5, 0xB7, 0xB9, 0xBD, 0xBF, 0xC1, 0xE1]);
        let mut r: Vec<_> = one_byte.map(|a| a..a + 2).collect();
        r.extend((0..8).flat_map(|i| [0x0C + 4 * i..0x10 + 4 * i, 0x2C + 4 * i..0x30 + 4 * i]));
        r.extend((0..16).map(|i| 0x60 + 4 * i..0x64 + 4 * i));
        r.push(0xA0..0xA7);
        r
    }

    fn apply_writes(block: &Block, writes: &[FlashWrite]) -> Block {
        let mut out = *block;
        for w in writes {
            let at = usize::from(w.addr);
            out[at..at + w.bytes.len()].copy_from_slice(&w.bytes);
        }
        out
    }

    /// xorshift32, so the garbage blocks are reproducible.
    fn garbage(seed: u32) -> Block {
        let mut s = seed.max(1);
        std::array::from_fn(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            s as u8
        })
    }

    fn hex_bytes(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    #[test]
    fn decodes_vendor_defaults() {
        let s = decode(&vendor_block());
        assert_eq!(s.report_rate, Checked { value: 0x01, valid: true });
        assert_eq!(s.dpi_stage_count, Checked { value: 6, valid: true });
        assert_eq!(s.current_dpi_stage, Checked { value: 2, valid: true });
        assert_eq!(s.lod, Checked { value: 0, valid: true });
        let dpi: Vec<u32> = s.dpi_stages.iter().map(|d| d.value.dpi()).collect();
        assert_eq!(dpi, [400, 800, 1600, 3200, 6400, 12000, 12000, 12000]);
        assert!(s.dpi_stages.iter().all(|d| d.valid && d.value.is_selectable()));
        assert_eq!(s.dpi_colors[2], Checked { value: Rgb { r: 0, g: 255, b: 150 }, valid: true });
        assert_eq!(s.dpi_colors[3].value, Rgb { r: 255, g: 100, b: 0 });
        assert_eq!(s.dpi_led.mode.value, 2);
        assert_eq!(s.dpi_led.brightness.value, 128);
        assert_eq!(s.dpi_led.breath_speed.value, 3);
        assert_eq!(s.dpi_led.enable, Checked { value: 0, valid: true });
        assert_eq!(s.keys[4].value, KeyBinding { kind: 1, param1: 16, param2: 16 });
        assert_eq!(s.keys[8].value, KeyBinding { kind: 7, param1: 0, param2: 0 });
        assert_eq!(s.keys[10].value, KeyBinding { kind: 2, param1: 3, param2: 3 });
        assert_eq!(s.keys[15], Checked { value: KeyBinding { kind: 0, param1: 0, param2: 0 }, valid: true });
        assert_eq!(
            s.led_bar,
            Checked { value: LedBar { mode: 1, color: Rgb { r: 255, g: 0, b: 255 }, speed: 7, brightness: 9 }, valid: true }
        );
        assert_eq!(s.led_bar_enable.value, 0);
        assert_eq!(s.key_debounce.value, 8);
        assert_eq!(s.motion_sync, Checked { value: true, valid: true });
        assert_eq!(s.all_led_off_time.value, 6);
        assert!(!s.linear_correction.value && !s.ripple_control.value && !s.move_off_led.value && !s.custom_sleep.value);
        assert_eq!(s.sensor_sleep_time.value, 6);
        assert_eq!(s.power_mode, Checked { value: POWER_LP, valid: true });
        assert_eq!(s.angle_tune.value, 2);
        assert_eq!(s.angle_tune_enable.value, 1);
        assert_eq!(s.wheel_changes_dpi.value, 1);
        assert_eq!(s.fury_mode, Checked { value: 0, valid: false }); // not in the vendor JSON, left erased
        assert_eq!(s.raw(), &vendor_block());
    }

    #[test]
    fn equal_settings_give_no_writes() {
        for block in [vendor_block(), [0xFF; CONFIG_BLOCK_LEN], [0; CONFIG_BLOCK_LEN], garbage(7)] {
            let s = decode(&block);
            assert!(diff(&s, &s).is_empty());
            assert!(diff(&s, &s.clone()).is_empty());
            assert_eq!(validate(&s, &s), Ok(()));
        }
    }

    #[test]
    fn rate_change_is_one_vendor_packet() {
        let mut block = vendor_block();
        put(&mut block, 0x00, &[0x08]); // the mouse is at 125 Hz
        let old = decode(&block);
        let mut new = old.clone();
        new.report_rate.set(protocol::rate_code(1000).unwrap());
        let writes = diff(&old, &new);
        assert_eq!(writes, [FlashWrite { addr: 0x0000, bytes: vec![0x01, 0x54] }]);
        let pkts = protocol::flash_write_requests(writes[0].addr, &writes[0].bytes).unwrap();
        assert_eq!(pkts.len(), 1);
        assert_eq!(pkts[0].to_vec(), hex_bytes("08 07 00 00 00 02 01 54 00 00 00 00 00 00 00 00 EF"));
    }

    #[test]
    fn dpi_stage_change_is_one_four_byte_write() {
        let old = decode(&vendor_block());
        for i in 0..DPI_STAGES {
            let mut new = old.clone();
            new.dpi_stages[i].set(DpiValue::from_dpi(26100).unwrap());
            let writes = diff(&old, &new);
            let bytes = vec![0x05, 0x05, 0x66, checksum(&[0x05, 0x05, 0x66])];
            assert_eq!(writes, [FlashWrite { addr: (0x0C + 4 * i) as u16, bytes }]);
        }
    }

    #[test]
    fn several_changes_follow_vendor_order() {
        let old = decode(&vendor_block());
        let mut new = old.clone();
        // set in an order unrelated to the expected output
        new.keys[3].set(KeyBinding { kind: 2, param1: 1, param2: 1 });
        new.led_bar.set(LedBar { mode: 2, ..old.led_bar.value });
        new.dpi_led.mode.set(1);
        new.dpi_colors[2].set(Rgb { r: 1, g: 2, b: 3 });
        new.dpi_stages[1].set(DpiValue::from_dpi(1000).unwrap());
        new.dpi_colors[0].set(Rgb { r: 9, g: 9, b: 9 });
        new.fury_mode.set(1);
        new.power_mode.set(POWER_HP);
        new.motion_sync.set(false);
        new.report_rate.set(0x08);
        let writes = diff(&old, &new);
        let addrs: Vec<u16> = writes.iter().map(|w| w.addr).collect();
        // scalars, then stage 0 colour before stage 1 value (interleaved), DPI LED, LED bar, keys last
        assert_eq!(addrs, [0x00, 0xAB, 0xB9, 0xE1, 0x2C, 0x10, 0x34, 0x4C, 0xA0, 0x6C]);
        assert_eq!(writes[1].bytes, [0x00, 0x55]);
        assert_eq!(writes[8].bytes, [2, 255, 0, 255, 7, 9, checksum(&[2, 255, 0, 255, 7, 9])]);
        assert_eq!(writes[9].bytes, [2, 1, 1, checksum(&[2, 1, 1])]);
        assert_eq!(validate(&old, &new), Ok(()));
    }

    #[test]
    fn written_block_decodes_to_the_new_settings() {
        let old = decode(&vendor_block());
        let mut new = old.clone();
        new.report_rate.set(0x02);
        new.dpi_stage_count.set(4);
        new.current_dpi_stage.set(3);
        new.dpi_stages[3].set(DpiValue::from_dpi(32000).unwrap());
        new.linear_correction.set(true);
        new.fury_mode.set(1);
        let writes = diff(&old, &new);
        let after = apply_writes(old.raw(), &writes);
        assert_eq!(verify(&writes, &after), Ok(()));
        let back = decode(&after);
        assert!(diff(&back, &new).is_empty());
        assert_eq!(back.dpi_stages[3].value.dpi(), 32000);
        assert_eq!(back.fury_mode, Checked { value: 1, valid: true });
    }

    #[test]
    fn bad_check_gives_invalid_default() {
        // decoded group as (data bytes, valid)
        type Get = fn(&Settings) -> (Vec<u8>, bool);
        // (group address, data length, accessor, vendor default bytes)
        let cases: [(usize, usize, Get, Vec<u8>); 13] = [
            (0x00, 1, |s| (s.report_rate.value.data(), s.report_rate.valid), vec![0x01]),
            (0x02, 1, |s| (s.dpi_stage_count.value.data(), s.dpi_stage_count.valid), vec![6]),
            (0x04, 1, |s| (s.current_dpi_stage.value.data(), s.current_dpi_stage.valid), vec![2]),
            (0x14, 3, |s| (s.dpi_stages[2].value.data(), s.dpi_stages[2].valid), vec![0xFF, 0xFF, 0]),
            (0x48, 3, |s| (s.dpi_colors[7].value.data(), s.dpi_colors[7].valid), vec![0xFF, 0xFF, 0xFF]),
            (0x4E, 1, |s| (s.dpi_led.brightness.value.data(), s.dpi_led.brightness.valid), vec![0x80]),
            (0x52, 1, |s| (s.dpi_led.enable.value.data(), s.dpi_led.enable.valid), vec![1]),
            (0x9C, 3, |s| (s.keys[15].value.data(), s.keys[15].valid), vec![0, 0, 0]),
            (0xA0, 6, |s| (s.led_bar.value.data(), s.led_bar.valid), vec![1, 0xFF, 0xFF, 0xFF, 8, 9]),
            (0xA7, 1, |s| (s.led_bar_enable.value.data(), s.led_bar_enable.valid), vec![1]),
            (0xAB, 1, |s| (s.motion_sync.value.data(), s.motion_sync.valid), vec![1]),
            (0xB7, 1, |s| (s.sensor_sleep_time.value.data(), s.sensor_sleep_time.valid), vec![6]),
            (0xBD, 1, |s| (s.angle_tune.value.data(), s.angle_tune.valid), vec![0]),
        ];
        for (at, len, get, default) in cases {
            let good = decode(&vendor_block());
            assert!(get(&good).1, "0x{at:02x} valid in the fixture");
            let mut block = vendor_block();
            block[at + len] ^= 0x01;
            let bad = decode(&block);
            assert_eq!(get(&bad), (default, false), "group 0x{at:02x}");
            // nothing else is affected, and an untouched invalid group is not rewritten
            assert_eq!(diff(&good, &bad).len(), usize::from(get(&good).0 != get(&bad).0));
            assert!(diff(&bad, &bad.clone()).is_empty());
        }
    }

    #[test]
    fn current_stage_must_be_below_stage_count() {
        let mut block = vendor_block();
        put(&mut block, 0x04, &[5]);
        assert_eq!(decode(&block).current_dpi_stage, Checked { value: 5, valid: true });
        put(&mut block, 0x02, &[5]);
        assert_eq!(decode(&block).current_dpi_stage, Checked { value: 2, valid: false });
        // a bad stage count falls back to 6, which accepts 5 again
        block[0x03] ^= 0x01;
        let s = decode(&block);
        assert_eq!(s.dpi_stage_count, Checked { value: 6, valid: false });
        assert_eq!(s.current_dpi_stage, Checked { value: 5, valid: true });
    }

    #[test]
    fn setting_an_invalid_group_repairs_it() {
        let mut block = vendor_block();
        block[0x01] = 0x00; // rate check broken, decodes as the default 1000 Hz
        let old = decode(&block);
        assert_eq!(old.report_rate, Checked { value: 0x01, valid: false });
        let mut new = old.clone();
        new.report_rate.set(0x01);
        assert_eq!(diff(&old, &new), [FlashWrite { addr: 0, bytes: vec![0x01, 0x54] }]);
    }

    #[test]
    fn erased_zero_and_garbage_blocks_do_not_panic() {
        for block in [[0xFF; CONFIG_BLOCK_LEN], [0; CONFIG_BLOCK_LEN]] {
            let s = decode(&block);
            assert!(!s.report_rate.valid && !s.led_bar.valid && !s.keys[0].valid && !s.dpi_led.enable.valid);
            assert!(s.dpi_stages.iter().all(|c| !c.valid) && s.dpi_colors.iter().all(|c| !c.valid));
            assert_eq!(s.report_rate.value, 0x01);
            assert_eq!(s.dpi_stages[0].value.dpi(), decode_dpi(0xFF, 0));
            let _ = format!("{s:?}");
        }
        for seed in 1..200 {
            let s = decode(&garbage(seed));
            let _ = format!("{s:?}");
            let erased = decode(&[0xFF; CONFIG_BLOCK_LEN]);
            let _ = validate(&erased, &s);
            for w in diff(&erased, &s) {
                assert!(usize::from(w.addr) + w.bytes.len() <= CONFIG_BLOCK_LEN);
            }
        }
    }

    #[test]
    fn unknown_bytes_survive_and_are_never_written() {
        let ranges = writable_ranges();
        let modelled = |at: usize| ranges.iter().any(|r| r.contains(&at));
        let mut block = vendor_block();
        let noise = garbage(42);
        for at in (0..CONFIG_BLOCK_LEN).filter(|&at| !modelled(at)) {
            block[at] = noise[at];
        }
        let old = decode(&block);
        assert_eq!(old.raw(), &block);

        // Blocks that differ only in unmodelled bytes have no diff.
        assert!(diff(&decode(&vendor_block()), &old).is_empty());

        // Change every modelled group; no write may touch an unmodelled byte.
        let mut new = old.clone();
        new.report_rate.set(0x08);
        new.dpi_stage_count.set(5);
        new.current_dpi_stage.set(4);
        new.lod.set(2);
        for i in 0..DPI_STAGES {
            new.dpi_stages[i].set(DpiValue::from_dpi(100 * (i as u32 + 1)).unwrap());
            new.dpi_colors[i].set(Rgb { r: i as u8, g: 1, b: 2 });
        }
        new.dpi_led.mode.set(4);
        new.dpi_led.brightness.set(10);
        new.dpi_led.breath_speed.set(1);
        new.dpi_led.enable.set(1);
        for (i, k) in new.keys.iter_mut().enumerate() {
            k.set(KeyBinding { kind: 3, param1: i as u8, param2: 0 });
        }
        new.led_bar.set(LedBar { mode: 3, color: Rgb { r: 1, g: 2, b: 3 }, speed: 1, brightness: 2 });
        new.led_bar_enable.set(1);
        new.key_debounce.set(4);
        new.motion_sync.set(false);
        new.all_led_off_time.set(30);
        new.linear_correction.set(true);
        new.ripple_control.set(true);
        new.move_off_led.set(true);
        new.custom_sleep.set(true);
        new.sensor_sleep_time.set(30);
        new.power_mode.set(POWER_HP);
        new.angle_tune.set(5);
        new.angle_tune_enable.set(0);
        new.wheel_changes_dpi.set(0);
        new.fury_mode.set(1);
        assert_eq!(validate(&old, &new), Ok(()));
        let writes = diff(&old, &new);
        assert_eq!(writes.len(), 22 + 16 + 16 + 1);
        for w in &writes {
            let at = usize::from(w.addr);
            assert!(ranges.contains(&(at..at + w.bytes.len())), "write at 0x{at:02x} is not a modelled group");
        }
        let after = apply_writes(&block, &writes);
        for at in (0..CONFIG_BLOCK_LEN).filter(|&at| !modelled(at)) {
            assert_eq!(after[at], block[at], "unmodelled byte 0x{at:02x} changed");
        }
        assert!(diff(&decode(&after), &new).is_empty());
    }

    #[test]
    fn group_ranges_match_the_spec() {
        let groups = group_ranges(&decode(&vendor_block()));
        let mut got: Vec<_> = groups.iter().map(|(r, _)| r.clone()).collect();
        let mut want = writable_ranges();
        got.sort_by_key(|r| r.start);
        want.sort_by_key(|r| r.start);
        assert_eq!(got, want);
        assert_eq!(groups[0], (0x00..0x02, true)); // vendor order starts with the rate
        assert!(!groups.iter().find(|(r, _)| r.start == 0xE1).unwrap().1); // fury mode is erased in the fixture
        assert!(group_ranges(&decode(&[0xFF; CONFIG_BLOCK_LEN])).iter().all(|(_, valid)| !valid));
    }

    #[test]
    fn stale_groups_compare_only_modelled_bytes() {
        let snapshot = decode(&vendor_block());
        assert_eq!(stale_groups(&snapshot, &snapshot.clone()), Vec::<std::ops::Range<usize>>::new());
        let mut block = vendor_block();
        put(&mut block, 0x04, &[3]); // the DPI button changed the active stage
        block[0x0F] ^= 0x01; // stage 0 check byte corrupted
        block[0xBB] = 0x0A; // unmodelled byte: ignored
        block[0x06] = 0x00; // UNCONFIRMED group: ignored
        assert_eq!(stale_groups(&snapshot, &decode(&block)), [0x04..0x06, 0x0C..0x10]);
        assert_eq!(stale_groups(&decode(&[0xFF; CONFIG_BLOCK_LEN]), &decode(&[0x00; CONFIG_BLOCK_LEN])).len(), 55);
    }

    #[test]
    fn verify_reports_mismatches() {
        let writes = [FlashWrite { addr: 0x00, bytes: vec![0x01, 0x54] }];
        let mut block = vendor_block();
        assert_eq!(verify(&writes, &block), Ok(()));
        assert_eq!(verify(&[], &block), Ok(()));
        block[0x01] = 0x00;
        let err = verify(&writes, &block).unwrap_err();
        assert!(err.contains("0x0000") && err.contains("[01 00]") && err.contains("[01 54]"), "{err}");
        let past_end = [FlashWrite { addr: 0xFF, bytes: vec![1, 2] }];
        assert!(verify(&past_end, &block).is_err());
    }

    #[test]
    fn validate_rejects_values_the_ui_cannot_produce() {
        let old = decode(&vendor_block());
        let with = |f: fn(&mut Settings)| {
            let mut new = old.clone();
            f(&mut new);
            validate(&old, &new)
        };
        assert!(with(|s| s.report_rate.set(0x40)).is_err()); // 8000 Hz is hidden for this model
        assert!(with(|s| s.report_rate.set(0x00)).is_err());
        assert!(with(|s| s.report_rate.set(0x08)).is_ok());
        assert!(with(|s| s.dpi_stage_count.set(0)).is_err());
        assert!(with(|s| s.dpi_stage_count.set(7)).is_err());
        assert!(with(|s| s.dpi_stage_count.set(2)).is_err()); // active stage 2 would be out of range
        assert!(with(|s| {
            s.dpi_stage_count.set(1);
            s.current_dpi_stage.set(0);
        })
        .is_ok());
        assert!(with(|s| s.current_dpi_stage.set(6)).is_err());
        assert!(with(|s| s.current_dpi_stage.set(5)).is_ok());
        assert!(with(|s| s.dpi_stages[0].set(DpiValue { x: 0x08, y: 0x08, ex: 0 })).is_ok()); // 450
        assert!(with(|s| s.dpi_stages[0].set(DpiValue { x: 122, y: 122, ex: 0 })).is_err()); // 6150
        assert!(with(|s| s.dpi_stages[0].set(DpiValue { x: 7, y: 15, ex: 0 })).is_err()); // x != y
        assert!(with(|s| s.dpi_stages[0].set(DpiValue { x: 0xFF, y: 0xFF, ex: 0x22 })).is_err()); // 25500 in the high range
        assert!(with(|s| s.dpi_stages[0].set(DpiValue::from_dpi(6100).unwrap())).is_ok());
        assert!(with(|s| s.lod.set(3)).is_err());
        assert!(with(|s| s.lod.set(1)).is_ok());
        assert!(with(|s| s.all_led_off_time.set(7)).is_err());
        assert!(with(|s| s.all_led_off_time.set(210)).is_ok());
        assert!(with(|s| s.sensor_sleep_time.set(60)).is_ok());
        assert!(with(|s| s.sensor_sleep_time.set(45)).is_err());
        assert!(with(|s| s.sensor_sleep_time.set(90)).is_ok());
        assert!(with(|s| s.power_mode.set(2)).is_err());
        assert!(with(|s| s.power_mode.set(POWER_HP)).is_ok());
        // unchanged odd values are not checked
        let mut block = vendor_block();
        put(&mut block, 0x00, &[0x40]);
        put(&mut block, 0x0A, &[9]);
        let odd = decode(&block);
        let mut new = odd.clone();
        new.motion_sync.set(false);
        assert_eq!(validate(&odd, &new), Ok(()));
    }

    /// HECATE.exe's two-step encoder for max 32000 / sensor != 0xF6E, transcribed from the decompile:
    /// text -> slider position (FUN_140236030), position -> register (FUN_1402367d0).
    fn vendor_encode(dpi: i32) -> (u8, u8, u8) {
        let dpi = dpi.clamp(50, 32000);
        let pos = if dpi < 0x6591 {
            let t = (dpi - 0x32) / 0x32;
            if t < 0x78 {
                t << 1
            } else {
                (t - 0x77) / 2 + 0xef
            }
        } else {
            dpi / 100 + 0xb3
        };
        if pos < 0x1b8 {
            let raw = if pos < 0xf0 { pos / 2 } else { pos * 2 - 0x167 };
            let hi = (raw >> 8) as u8;
            (raw as u8, raw as u8, hi << 2 | hi << 6)
        } else {
            let lo = (pos as u8).wrapping_add(b'M');
            let hi = ((pos - 0xb3) >> 8) as u8;
            (lo, lo, hi << 2 | hi << 6 | 0x22)
        }
    }

    fn ui_values() -> impl Iterator<Item = u32> {
        (50..=6000).step_by(50).chain((6100..=32000).step_by(100))
    }

    #[test]
    fn dpi_round_trip_for_every_ui_value() {
        let mut n = 0;
        for dpi in ui_values() {
            let (x, y, ex) = encode_dpi(dpi).unwrap();
            assert_eq!((x, y, ex), vendor_encode(dpi as i32), "{dpi} DPI");
            assert_eq!(x, y);
            assert_eq!(decode_dpi(x, ex), dpi, "{dpi} DPI");
            let v = DpiValue::from_dpi(dpi).unwrap();
            assert_eq!(v.dpi(), dpi);
            assert!(v.is_selectable());
            n += 1;
        }
        assert_eq!(n, 120 + 260);
    }

    #[test]
    fn dpi_vendor_defaults() {
        for (x, dpi) in [(7, 400), (15, 800), (31, 1600), (63, 3200), (127, 6400), (239, 12000)] {
            assert_eq!(decode_dpi(x, 0), dpi);
            assert_eq!(encode_dpi(dpi), Ok((x, x, 0)));
        }
    }

    #[test]
    fn dpi_boundaries() {
        assert_eq!(encode_dpi(50), Ok((0, 0, 0)));
        assert_eq!(encode_dpi(6000), Ok((119, 119, 0)));
        assert_eq!(encode_dpi(6100), Ok((121, 121, 0)));
        assert_eq!(encode_dpi(12800), Ok((0xFF, 0xFF, 0)));
        assert_eq!(encode_dpi(12900), Ok((0x01, 0x01, 0x44)));
        assert_eq!(encode_dpi(26000), Ok((0x07, 0x07, 0x88)));
        assert_eq!(encode_dpi(26100), Ok((0x05, 0x05, 0x66)));
        assert_eq!(encode_dpi(32000), Ok((0x40, 0x40, 0x66)));
    }

    #[test]
    fn dpi_rejects_unselectable_values() {
        for dpi in [0, 25, 49, 75, 6050, 6150, 26050, 31950, 32100, 32050, 40000, u32::MAX] {
            assert!(encode_dpi(dpi).is_err(), "{dpi}");
            assert!(DpiValue::from_dpi(dpi).is_err(), "{dpi}");
        }
    }

    #[test]
    fn dpi_decode_follows_vendor_on_odd_registers() {
        // FUN_140235080 reads high bits 7..6 and flags 0x03 only; y and bits 3..2 are ignored.
        // ex = 0x48: bits 7..6 = 1, bits 3..2 = 2. Vendor: 0x110 * 50 + 50; the inferred decoder gave 0x211 * 50.
        assert_eq!(decode_dpi(0x10, 0x48), 0x110 * 50 + 50);
        assert_ne!(decode_dpi(0x10, 0x48), 0x211 * 50);
        // ex = 0x86: bits 7..6 = 2, bits 3..2 = 1, x100 flag in bit 1.
        assert_eq!(decode_dpi(0x05, 0x86), 0x205 * 100);
        assert_eq!(decode_dpi(7, 0x04), 400);
        assert_eq!(decode_dpi(7, 0x08), 400);
        assert_eq!(decode_dpi(7, 0x01), 700);
        assert_eq!(decode_dpi(7, 0x20), 400);
        assert_eq!(decode_dpi(0, 0xC0), 768 * 50 + 50);
        assert_eq!(decode_dpi(0xFF, 0xC3), 1023 * 100);
        let odd = DpiValue { x: 7, y: 15, ex: 0 };
        assert_eq!(odd.dpi(), 400);
        assert!(!odd.is_selectable());
        assert!(!DpiValue { x: 7, y: 7, ex: 0x04 }.is_selectable());
    }

    #[test]
    fn dpi_decode_never_panics() {
        for x in 0..=255u8 {
            for ex in 0..=255u8 {
                assert!(decode_dpi(x, ex) <= 102_300);
            }
        }
    }
}
