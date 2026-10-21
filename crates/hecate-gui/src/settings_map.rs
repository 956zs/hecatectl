//! Settings <-> QML: the JSON view of [`Settings`] the pages bind to, and the patches they send back.
//!
//! A patch is a JSON object with one or more of the view's keys, e.g. `{"reportRate": 1000}` or
//! `{"stageDpi": {"stage": 1, "dpi": 800}}`. [`apply`] turns it into new settings the way the
//! vendor UI handlers change their struct; `Mouse::apply` then validates and writes the diff.

use crate::dpi_slider;
use hecate_core::protocol;
use hecate_core::settings::{DpiValue, Rgb, Settings, MAX_SELECTABLE_STAGES, POWER_HP, POWER_LP};
use serde_json::{json, Map, Value};

/// Stored values of the "Peak Performance" sensor sleep combo, unit 10 s (labels 30s .. 15Min).
pub const SENSOR_SLEEP_TIMES: [u8; 6] = [3, 6, 12, 30, 60, 90];
/// Stored values of "Mouse sleep time after stopping", unit 10 s (labels 1Min .. 35Min).
pub const MOUSE_SLEEP_TIMES: [u8; 8] = [6, 30, 60, 90, 120, 150, 180, 210];
const TIME_UNIT_SECONDS: u32 = 10;
/// LOD choices: "1MM" and "2MM".
const LOD_CHOICES: [u8; 2] = [1, 2];

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    ReportRate(u32),
    PowerMode(u8),
    Lod(u8),
    RippleControl(bool),
    LinearCorrection(bool),
    MotionSync(bool),
    PeakPerformance(bool),
    SensorSleepTime(u8),
    MouseSleepTime(u8),
    StageCount(u8),
    CurrentStage { stage: u8, expect: StageExpect },
    /// A typed DPI value; snapped and clamped like the vendor's edit box.
    StageDpi { stage: u8, dpi: u32, expect: StageExpect },
    /// A slider position, applied on release.
    StagePosition { stage: u8, position: u32, expect: StageExpect },
    StageColor { stage: u8, color: Rgb, expect: StageExpect },
}

/// What the page showed for a stage when the user acted on it. A stage change is refused when the
/// stage no longer holds this, e.g. after a queued DPI Quantity change shifted the stages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageExpect {
    pub reg: DpiValue,
    pub color: Rgb,
}

fn hex_color(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn parse_color(s: &str) -> Option<Rgb> {
    let hex = s.strip_prefix('#').filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()))?;
    let v = u32::from_str_radix(hex, 16).ok()?;
    let [_, r, g, b] = v.to_be_bytes();
    Some(Rgb { r, g, b })
}

/// Stage count as the vendor UI shows it: 0 shown as 1, more than 6 shown as 6 (FUN_140236460).
fn shown_stage_count(s: &Settings) -> u8 {
    s.dpi_stage_count.value.clamp(1, MAX_SELECTABLE_STAGES)
}

/// The view the QML pages bind to.
pub fn to_json(s: &Settings) -> Value {
    let stages: Vec<Value> = (0..usize::from(MAX_SELECTABLE_STAGES))
        .map(|i| {
            let reg = s.dpi_stages[i].value;
            let dpi = reg.dpi();
            json!({
                "dpi": dpi,
                "position": dpi_slider::position(dpi),
                "color": hex_color(s.dpi_colors[i].value),
                // Sent back as the expectation of stage patches.
                "reg": [reg.x, reg.y, reg.ex],
            })
        })
        .collect();
    json!({
        "reportRate": protocol::rate_hz(s.report_rate.value).unwrap_or(0),
        // The vendor loader clamps the mode to 0..1 and shows LOD values >= 2 as 2MM (FUN_1402375e0, FUN_140237730).
        "powerMode": s.power_mode.value.min(POWER_HP),
        "lod": if s.lod.value >= 2 { 2 } else { 1 },
        "rippleControl": s.ripple_control.value,
        "linearCorrection": s.linear_correction.value,
        "motionSync": s.motion_sync.value,
        "peakPerformance": s.custom_sleep.value,
        "sensorSleepTime": s.sensor_sleep_time.value,
        "mouseSleepTime": s.all_led_off_time.value,
        "dpiStageCount": shown_stage_count(s),
        "currentDpiStage": s.current_dpi_stage.value,
        "stages": stages,
    })
}

/// Constant choices and limits for the pages.
pub fn options(product_url: &str) -> Value {
    let times = |values: &[u8]| -> Vec<Value> {
        values.iter().map(|v| json!({ "value": v, "seconds": u32::from(*v) * TIME_UNIT_SECONDS })).collect()
    };
    json!({
        "reportRates": protocol::SETTABLE_RATES.iter().map(|(hz, _)| *hz).collect::<Vec<_>>(),
        "powerModes": [POWER_LP, POWER_HP],
        "lods": LOD_CHOICES,
        "sensorSleepTimes": times(&SENSOR_SLEEP_TIMES),
        "mouseSleepTimes": times(&MOUSE_SLEEP_TIMES),
        "maxDpiStages": MAX_SELECTABLE_STAGES,
        "sliderMax": dpi_slider::MAX_POSITION,
        "productUrl": product_url,
    })
}

fn uint(value: &Value, key: &str) -> Result<u32, String> {
    value.as_u64().and_then(|v| u32::try_from(v).ok()).ok_or_else(|| format!("'{key}' must be a whole number"))
}

fn small(value: &Value, key: &str) -> Result<u8, String> {
    u8::try_from(uint(value, key)?).map_err(|_| format!("'{key}' is out of range"))
}

fn boolean(value: &Value, key: &str) -> Result<bool, String> {
    value.as_bool().ok_or_else(|| format!("'{key}' must be true or false"))
}

/// `{"reg": [x, y, ex], "color": "#rrggbb"}`: the stage as the page showed it.
fn parse_expect(v: &Value, key: &str) -> Result<StageExpect, String> {
    let bad = || format!("'{key}.expect' must be {{\"reg\": [x, y, ex], \"color\": \"#rrggbb\"}}");
    let obj = v.as_object().ok_or_else(bad)?;
    let reg: Vec<u8> = obj
        .get("reg")
        .and_then(Value::as_array)
        .ok_or_else(bad)?
        .iter()
        .map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok()))
        .collect::<Option<_>>()
        .ok_or_else(bad)?;
    let [x, y, ex] = reg[..] else { return Err(bad()) };
    let color = obj.get("color").and_then(Value::as_str).and_then(parse_color).ok_or_else(bad)?;
    Ok(StageExpect { reg: DpiValue { x, y, ex }, color })
}

/// `{"stage": n, <field>: ..., "expect": {...}}` objects of the per-stage changes.
fn stage_patch<'a>(value: &'a Value, key: &str, field: Option<&str>) -> Result<(u8, Option<&'a Value>, StageExpect), String> {
    let obj = value.as_object().ok_or_else(|| format!("'{key}' must be an object"))?;
    let stage = small(obj.get("stage").ok_or_else(|| format!("'{key}' needs 'stage'"))?, &format!("{key}.stage"))?;
    let v = match field {
        Some(f) => Some(obj.get(f).ok_or_else(|| format!("'{key}' needs '{f}'"))?),
        None => None,
    };
    let expect = parse_expect(obj.get("expect").ok_or_else(|| format!("'{key}' needs 'expect'"))?, key)?;
    if let Some(extra) = obj.keys().find(|k| !["stage", "expect"].contains(&k.as_str()) && Some(k.as_str()) != field) {
        return Err(format!("'{key}' has an unknown key '{extra}'"));
    }
    Ok((stage, v, expect))
}

/// Parses a patch object. Unknown keys and wrong types are errors naming the key. The changes
/// come out in key order; the pages send one key per patch.
pub fn parse_patch(text: &str) -> Result<Vec<Change>, String> {
    let map: Map<String, Value> = match serde_json::from_str(text) {
        Ok(Value::Object(map)) => map,
        Ok(_) => return Err("the patch is not a JSON object".into()),
        Err(e) => return Err(format!("the patch is not valid JSON: {e}")),
    };
    if map.is_empty() {
        return Err("the patch is empty".into());
    }
    map.iter()
        .map(|(key, v)| {
            let k = key.as_str();
            Ok(match k {
                "reportRate" => Change::ReportRate(uint(v, k)?),
                "powerMode" => Change::PowerMode(small(v, k)?),
                "lod" => Change::Lod(small(v, k)?),
                "rippleControl" => Change::RippleControl(boolean(v, k)?),
                "linearCorrection" => Change::LinearCorrection(boolean(v, k)?),
                "motionSync" => Change::MotionSync(boolean(v, k)?),
                "peakPerformance" => Change::PeakPerformance(boolean(v, k)?),
                "sensorSleepTime" => Change::SensorSleepTime(small(v, k)?),
                "mouseSleepTime" => Change::MouseSleepTime(small(v, k)?),
                "dpiStageCount" => Change::StageCount(small(v, k)?),
                "currentDpiStage" => {
                    let (stage, _, expect) = stage_patch(v, k, None)?;
                    Change::CurrentStage { stage, expect }
                }
                "stageDpi" => {
                    let (stage, dpi, expect) = stage_patch(v, k, Some("dpi"))?;
                    Change::StageDpi { stage, dpi: uint(dpi.unwrap_or(&Value::Null), "stageDpi.dpi")?, expect }
                }
                "stagePosition" => {
                    let (stage, p, expect) = stage_patch(v, k, Some("position"))?;
                    Change::StagePosition { stage, position: uint(p.unwrap_or(&Value::Null), "stagePosition.position")?, expect }
                }
                "stageColor" => {
                    let (stage, c, expect) = stage_patch(v, k, Some("color"))?;
                    let color = c.and_then(Value::as_str).and_then(parse_color).ok_or("'stageColor.color' must be \"#rrggbb\"")?;
                    Change::StageColor { stage, color, expect }
                }
                _ => return Err(format!("unknown setting '{key}'")),
            })
        })
        .collect()
}

fn choice(name: &str, value: u8, allowed: &[u8]) -> Result<u8, String> {
    if allowed.contains(&value) {
        Ok(value)
    } else {
        Err(format!("{name} {value} is not one of {allowed:?}"))
    }
}

/// A visible stage (below the stage count) that still holds what the page showed.
fn visible_stage(s: &Settings, stage: u8, expect: &StageExpect) -> Result<usize, String> {
    if stage >= shown_stage_count(s) {
        return Err(format!("DPI stage {} is not shown (stage count {})", stage + 1, shown_stage_count(s)));
    }
    let i = usize::from(stage);
    if s.dpi_stages[i].value != expect.reg || s.dpi_colors[i].value != expect.color {
        return Err(format!("DPI stage {} changed since it was shown; nothing was written", stage + 1));
    }
    Ok(i)
}

/// The vendor's "DPI Quantity" handler (FUN_140236ef0): when the active stage falls outside the
/// new count, it moves into the last visible slot and the stages from there up shift one slot back,
/// so the active DPI and colour are kept.
fn set_stage_count(s: &mut Settings, count: u8) -> Result<(), String> {
    if !(1..=MAX_SELECTABLE_STAGES).contains(&count) {
        return Err(format!("{count} DPI stages: choose 1 to {MAX_SELECTABLE_STAGES}"));
    }
    let current = usize::from(s.current_dpi_stage.value);
    let last = usize::from(count) - 1;
    s.dpi_stage_count.set(count);
    if last < current && current < s.dpi_stages.len() {
        let (dpi, color) = (s.dpi_stages[current], s.dpi_colors[current]);
        for i in (last + 1..=current).rev() {
            s.dpi_stages[i] = s.dpi_stages[i - 1];
            s.dpi_colors[i] = s.dpi_colors[i - 1];
        }
        s.dpi_stages[last] = dpi;
        s.dpi_colors[last] = color;
        s.current_dpi_stage.set(count - 1);
    }
    Ok(())
}

/// New settings with `changes` applied, in order.
pub fn apply(old: &Settings, changes: &[Change]) -> Result<Settings, String> {
    let mut s = old.clone();
    for change in changes {
        match *change {
            Change::ReportRate(hz) => {
                let code = protocol::rate_code(hz).ok_or_else(|| format!("{hz} Hz is not offered for this mouse"))?;
                s.report_rate.set(code);
            }
            Change::PowerMode(m) => s.power_mode.set(choice("power mode", m, &[POWER_LP, POWER_HP])?),
            Change::Lod(l) => s.lod.set(choice("LOD", l, &LOD_CHOICES)?),
            Change::RippleControl(on) => s.ripple_control.set(on),
            Change::LinearCorrection(on) => s.linear_correction.set(on),
            Change::MotionSync(on) => s.motion_sync.set(on),
            Change::PeakPerformance(on) => s.custom_sleep.set(on),
            Change::SensorSleepTime(t) => s.sensor_sleep_time.set(choice("sensor sleep time", t, &SENSOR_SLEEP_TIMES)?),
            Change::MouseSleepTime(t) => s.all_led_off_time.set(choice("mouse sleep time", t, &MOUSE_SLEEP_TIMES)?),
            Change::StageCount(n) => set_stage_count(&mut s, n)?,
            Change::CurrentStage { stage, expect } => {
                let i = visible_stage(&s, stage, &expect)?;
                s.current_dpi_stage.set(i as u8);
            }
            Change::StageDpi { stage, dpi, expect } => {
                let i = visible_stage(&s, stage, &expect)?;
                s.dpi_stages[i].set(DpiValue::from_dpi(dpi_slider::snap(dpi))?);
            }
            Change::StagePosition { stage, position, expect } => {
                let i = visible_stage(&s, stage, &expect)?;
                if position > dpi_slider::MAX_POSITION {
                    return Err(format!("slider position {position} is above {}", dpi_slider::MAX_POSITION));
                }
                s.dpi_stages[i].set(DpiValue::from_dpi(dpi_slider::to_dpi(position))?);
            }
            Change::StageColor { stage, color, expect } => {
                let i = visible_stage(&s, stage, &expect)?;
                s.dpi_colors[i].set(color);
            }
        }
    }
    Ok(s)
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

    /// The vendor default profile for the fields this module touches (defaultconfiguration.json).
    fn defaults() -> Settings {
        let mut b = [0xFF; CONFIG_BLOCK_LEN];
        put(&mut b, 0x00, &[1]); // 1000 Hz
        put(&mut b, 0x02, &[6]);
        put(&mut b, 0x04, &[2]);
        put(&mut b, 0x0A, &[0]);
        let dpi = [7, 15, 31, 63, 127, 239, 239, 239]; // 400, 800, 1600, 3200, 6400, 12000
        let colors = [[255, 0, 0], [0, 255, 0], [0, 255, 150], [255, 100, 0], [0, 0, 255], [255, 0, 255], [255, 0, 255], [255, 0, 255]];
        for i in 0..8 {
            put(&mut b, 0x0C + 4 * i, &[dpi[i], dpi[i], 0]);
            put(&mut b, 0x2C + 4 * i, &colors[i]);
        }
        for (at, v) in [(0xAB, 1), (0xAD, 6), (0xAF, 0), (0xB1, 0), (0xB5, 0), (0xB7, 6), (0xB9, 0)] {
            put(&mut b, at, &[v]);
        }
        settings::decode(&b)
    }

    fn patch(text: &str) -> Settings {
        let changes = parse_patch(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        apply(&defaults(), &changes).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    fn patch_err(text: &str) -> String {
        match parse_patch(text) {
            Err(e) => e,
            Ok(changes) => apply(&defaults(), &changes).expect_err(text),
        }
    }

    /// The expectation the page sends with a patch for stage `i` of `s`.
    fn exp(s: &Settings, i: usize) -> StageExpect {
        StageExpect { reg: s.dpi_stages[i].value, color: s.dpi_colors[i].value }
    }

    /// The same as patch JSON.
    fn expect_json(s: &Settings, i: usize) -> String {
        let r = s.dpi_stages[i].value;
        format!(r#""expect": {{"reg": [{}, {}, {}], "color": "{}"}}"#, r.x, r.y, r.ex, hex_color(s.dpi_colors[i].value))
    }

    /// Every patch result must pass the core's validation against the original.
    fn valid(new: &Settings) {
        settings::validate(&defaults(), new).unwrap();
    }

    #[test]
    fn view_of_the_defaults() {
        let v = to_json(&defaults());
        assert_eq!(v["reportRate"], 1000);
        assert_eq!(v["powerMode"], 0);
        assert_eq!(v["lod"], 1);
        assert_eq!(v["motionSync"], true);
        assert_eq!(v["peakPerformance"], false);
        assert_eq!(v["sensorSleepTime"], 6);
        assert_eq!(v["mouseSleepTime"], 6);
        assert_eq!(v["dpiStageCount"], 6);
        assert_eq!(v["currentDpiStage"], 2);
        let stages = v["stages"].as_array().unwrap();
        assert_eq!(stages.len(), 6);
        assert_eq!(stages[0], json!({ "dpi": 400, "position": 14, "color": "#ff0000", "reg": [7, 7, 0] }));
        assert_eq!(stages[5]["dpi"], 12000);
        assert_eq!(stages[5]["position"], 299);
    }

    #[test]
    fn view_of_an_erased_block_uses_parser_defaults() {
        let v = to_json(&settings::decode(&[0xFF; CONFIG_BLOCK_LEN]));
        assert!(v["dpiStageCount"].as_u64().unwrap() >= 1);
        assert_eq!(v["stages"].as_array().unwrap().len(), 6);
    }

    #[test]
    fn scalar_patches() {
        let s = patch(r#"{"reportRate": 125}"#);
        assert_eq!(s.report_rate.value, 0x08);
        valid(&s);
        let s = patch(r#"{"powerMode": 1, "lod": 2, "rippleControl": true, "linearCorrection": true, "motionSync": false}"#);
        assert_eq!((s.power_mode.value, s.lod.value), (POWER_HP, 2));
        assert!(s.ripple_control.value && s.linear_correction.value && !s.motion_sync.value);
        valid(&s);
        let s = patch(r#"{"peakPerformance": true, "sensorSleepTime": 90, "mouseSleepTime": 210}"#);
        assert!(s.custom_sleep.value);
        assert_eq!((s.sensor_sleep_time.value, s.all_led_off_time.value), (90, 210));
        valid(&s);
    }

    #[test]
    fn every_offered_choice_passes_core_validation() {
        for hz in [125, 250, 500, 1000] {
            valid(&patch(&format!(r#"{{"reportRate": {hz}}}"#)));
        }
        for t in SENSOR_SLEEP_TIMES {
            valid(&patch(&format!(r#"{{"sensorSleepTime": {t}}}"#)));
        }
        for t in MOUSE_SLEEP_TIMES {
            valid(&patch(&format!(r#"{{"mouseSleepTime": {t}}}"#)));
        }
        for n in 1..=6 {
            valid(&patch(&format!(r#"{{"dpiStageCount": {n}}}"#)));
        }
    }

    #[test]
    fn untouched_patch_fields_stay() {
        let s = patch(r#"{"lod": 2}"#);
        let d = defaults();
        assert_eq!(s.report_rate, d.report_rate);
        assert_eq!(s.dpi_stages, d.dpi_stages);
        assert_eq!(settings::diff(&d, &s).len(), 1);
    }

    #[test]
    fn stage_dpi_is_snapped_like_the_vendor() {
        let d = defaults();
        let s = patch(&format!(r#"{{"stageDpi": {{"stage": 0, "dpi": 6050, {}}}}}"#, expect_json(&d, 0)));
        assert_eq!(s.dpi_stages[0].value.dpi(), 6000);
        let s = patch(&format!(r#"{{"stageDpi": {{"stage": 1, "dpi": 40000, {}}}}}"#, expect_json(&d, 1)));
        assert_eq!(s.dpi_stages[1].value.dpi(), 32000);
        let s = patch(&format!(r#"{{"stageDpi": {{"stage": 1, "dpi": 1, {}}}}}"#, expect_json(&d, 1)));
        assert_eq!(s.dpi_stages[1].value.dpi(), 50);
        valid(&s);
    }

    #[test]
    fn slider_positions_set_the_stage() {
        let e = expect_json(&defaults(), 2);
        for p in [0, 1, 239, 240, 439, 440, 499] {
            let s = patch(&format!(r#"{{"stagePosition": {{"stage": 2, "position": {p}, {e}}}}}"#));
            assert_eq!(s.dpi_stages[2].value.dpi(), dpi_slider::to_dpi(p));
            valid(&s);
        }
        assert!(patch_err(&format!(r#"{{"stagePosition": {{"stage": 2, "position": 500, {e}}}}}"#)).contains("500"));
    }

    #[test]
    fn colours_and_current_stage() {
        let d = defaults();
        let s = patch(&format!(r##"{{"stageColor": {{"stage": 3, "color": "#12ab9F", {}}}}}"##, expect_json(&d, 3)));
        assert_eq!(s.dpi_colors[3].value, Rgb { r: 0x12, g: 0xab, b: 0x9f });
        let s = patch(&format!(r#"{{"currentDpiStage": {{"stage": 5, {}}}}}"#, expect_json(&d, 5)));
        assert_eq!(s.current_dpi_stage.value, 5);
        valid(&s);
    }

    #[test]
    fn hidden_stages_are_refused() {
        let d = apply(&defaults(), &[Change::StageCount(3)]).unwrap();
        for change in [
            Change::CurrentStage { stage: 3, expect: exp(&d, 3) },
            Change::StageDpi { stage: 3, dpi: 800, expect: exp(&d, 3) },
            Change::StageColor { stage: 5, color: Rgb { r: 0, g: 0, b: 0 }, expect: exp(&d, 5) },
        ] {
            let e = apply(&d, std::slice::from_ref(&change)).unwrap_err();
            assert!(e.contains("not shown"), "{change:?}: {e}");
        }
    }

    /// Review R3: stages 400/800/1600*/...; DPI Quantity 2 is written (1600 moves to index 1), then the
    /// queued patch typed into the card that showed 800 (index 1) arrives. It must not land on 1600.
    #[test]
    fn r3_queued_stage_patch_is_refused_after_a_shift() {
        let shown = defaults();
        let after_count = apply(&shown, &[Change::StageCount(2)]).unwrap();
        assert_eq!(after_count.dpi_stages[1].value.dpi(), 1600);
        for queued in [
            Change::StageDpi { stage: 1, dpi: 900, expect: exp(&shown, 1) },
            Change::StagePosition { stage: 1, position: 40, expect: exp(&shown, 1) },
            Change::StageColor { stage: 1, color: Rgb { r: 1, g: 2, b: 3 }, expect: exp(&shown, 1) },
            Change::CurrentStage { stage: 0, expect: exp(&shown, 2) },
        ] {
            let e = apply(&after_count, std::slice::from_ref(&queued)).unwrap_err();
            assert!(e.contains("changed since it was shown"), "{queued:?}: {e}");
        }
        // On the settings the user saw, the same patch applies.
        let typed = Change::StageDpi { stage: 1, dpi: 900, expect: exp(&shown, 1) };
        assert_eq!(apply(&shown, &[typed]).unwrap().dpi_stages[1].value.dpi(), 900);
    }

    #[test]
    fn stage_patches_need_a_well_formed_expectation() {
        assert!(patch_err(r#"{"stageDpi": {"stage": 0, "dpi": 800}}"#).contains("'expect'"));
        assert!(patch_err(r#"{"currentDpiStage": 1}"#).contains("must be an object"));
        for bad in [
            r##"{"reg": [7, 7], "color": "#ff0000"}"##,
            r##"{"reg": [7, 7, 300], "color": "#ff0000"}"##,
            r##"{"reg": [7, 7, 0], "color": "red"}"##,
            r##"{"color": "#ff0000"}"##,
            r#"[]"#,
        ] {
            let text = format!(r#"{{"stageDpi": {{"stage": 0, "dpi": 800, "expect": {bad}}}}}"#);
            assert!(patch_err(&text).contains("'stageDpi.expect'"), "{bad}");
        }
    }

    #[test]
    fn shrinking_moves_the_active_stage_into_view() {
        // Defaults: 6 stages, active stage 2 (1600 DPI, colour #00ff96).
        let s = patch(r#"{"dpiStageCount": 2}"#);
        assert_eq!(s.dpi_stage_count.value, 2);
        assert_eq!(s.current_dpi_stage.value, 1);
        let dpis: Vec<u32> = s.dpi_stages.iter().map(|d| d.value.dpi()).collect();
        assert_eq!(&dpis[..6], &[400, 1600, 800, 3200, 6400, 12000]);
        assert_eq!(s.dpi_colors[1].value, Rgb { r: 0, g: 255, b: 150 });
        assert_eq!(s.dpi_colors[2].value, Rgb { r: 0, g: 255, b: 0 });
        valid(&s);
    }

    #[test]
    fn shrinking_to_one_stage() {
        let s = patch(r#"{"dpiStageCount": 1}"#);
        assert_eq!(s.current_dpi_stage.value, 0);
        let dpis: Vec<u32> = s.dpi_stages.iter().take(4).map(|d| d.value.dpi()).collect();
        assert_eq!(dpis, [1600, 400, 800, 3200]);
        valid(&s);
    }

    #[test]
    fn shrinking_above_the_active_stage_keeps_order() {
        let s = patch(r#"{"dpiStageCount": 3}"#);
        assert_eq!(s.current_dpi_stage.value, 2);
        assert_eq!(s.dpi_stages, defaults().dpi_stages);
        valid(&s);
    }

    #[test]
    fn bad_values_are_refused() {
        assert!(patch_err(r#"{"reportRate": 2000}"#).contains("2000 Hz"));
        assert!(patch_err(r#"{"powerMode": 2}"#).contains("power mode"));
        assert!(patch_err(r#"{"lod": 0}"#).contains("LOD"));
        assert!(patch_err(r#"{"sensorSleepTime": 7}"#).contains("sensor sleep time"));
        assert!(patch_err(r#"{"mouseSleepTime": 5}"#).contains("mouse sleep time"));
        assert!(patch_err(r#"{"dpiStageCount": 0}"#).contains("DPI stages"));
        assert!(patch_err(r#"{"dpiStageCount": 7}"#).contains("DPI stages"));
    }

    #[test]
    fn malformed_patches_name_the_problem() {
        assert!(patch_err("").contains("not valid JSON"));
        assert!(patch_err("[]").contains("not a JSON object"));
        assert!(patch_err("{}").contains("empty"));
        assert!(patch_err(r#"{"rate": 1000}"#).contains("'rate'"));
        assert!(patch_err(r#"{"reportRate": "1000"}"#).contains("'reportRate'"));
        assert!(patch_err(r#"{"reportRate": -1}"#).contains("'reportRate'"));
        assert!(patch_err(r#"{"motionSync": 1}"#).contains("'motionSync'"));
        assert!(patch_err(r#"{"lod": 300}"#).contains("'lod'"));
        assert!(patch_err(r#"{"stageDpi": 800}"#).contains("'stageDpi'"));
        assert!(patch_err(r#"{"stageDpi": {"dpi": 800}}"#).contains("'stage'"));
        let e = expect_json(&defaults(), 0);
        assert!(patch_err(&format!(r#"{{"stageDpi": {{"stage": 0, "dpi": 800, {e}, "x": 1}}}}"#)).contains("'x'"));
        assert!(patch_err(&format!(r##"{{"stageColor": {{"stage": 0, "color": "#fff", {e}}}}}"##)).contains("#rrggbb"));
        assert!(patch_err(&format!(r##"{{"stageColor": {{"stage": 0, "color": "#gg0000", {e}}}}}"##)).contains("#rrggbb"));
    }

    #[test]
    fn options_list_the_vendor_choices() {
        let o = options("https://example.invalid/");
        assert_eq!(o["reportRates"], json!([125, 250, 500, 1000]));
        assert_eq!(o["sensorSleepTimes"][0], json!({ "value": 3, "seconds": 30 }));
        assert_eq!(o["sensorSleepTimes"][5], json!({ "value": 90, "seconds": 900 }));
        assert_eq!(o["mouseSleepTimes"][7], json!({ "value": 210, "seconds": 2100 }));
        assert_eq!(o["maxDpiStages"], 6);
        assert_eq!(o["sliderMax"], 499);
        assert_eq!(o["productUrl"], "https://example.invalid/");
    }

    #[test]
    fn colour_strings() {
        assert_eq!(hex_color(Rgb { r: 0, g: 255, b: 150 }), "#00ff96");
        assert_eq!(parse_color("#00FF96"), Some(Rgb { r: 0, g: 255, b: 150 }));
        assert_eq!(parse_color("00ff96"), None);
        assert_eq!(parse_color("#00ff9"), None);
        assert_eq!(parse_color("#+0ff96"), None);
    }
}
