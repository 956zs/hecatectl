use hecate_core::mouse::{self, Mouse};
use hecate_core::protocol::{self, PairingStatus};
use hecate_core::settings::{self, Checked, KeyBinding, Settings};
use hecate_core::slots::{self, KeyState, Slots};
use hecate_core::hidraw;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: hecatectl [--device /dev/hidrawN] <command>

commands:
  list              show detected mice and their hidraw nodes
  info              connection, firmware and receiver versions, battery and report rate
  rate              show the report (polling) rate
  rate <HZ>         set the report rate: 125, 250, 500 or 1000
  dump              hex dump of the settings block (flash 0x000-0x0ff, read-only)
  show              decoded settings block (read-only)
  slots             decoded shortcut and macro slots (read-only)
  longrange         show the long-distance mode
  longrange on|off  set the long-distance mode
  reset --yes       factory reset: all settings, key mappings and macros
  pair --yes        put the receiver into pairing mode and wait for the mouse";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hecatectl: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(mut args: Vec<String>) -> Result<(), String> {
    let device = take_device_arg(&mut args)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["list"] => cmd_list(),
        ["info"] => {
            let node = resolve_node(device)?;
            cmd_info(&mut open_node(&node)?, hidraw::receiver_version(&node))
        }
        ["rate"] => cmd_rate_get(&mut open(device)?),
        ["rate", hz] => cmd_rate_set(&mut open(device)?, hz),
        ["dump"] => cmd_dump(&mut open(device)?),
        ["show"] => cmd_show(&mut open(device)?),
        ["slots"] => cmd_slots(&mut open(device)?),
        ["longrange"] => cmd_long_range(&mut open(device)?, None),
        ["longrange", v] => cmd_long_range(&mut open(device)?, Some(parse_on_off(v)?)),
        ["reset", "--yes"] => cmd_reset(&mut open(device)?),
        ["reset"] => Err("factory reset erases all settings, key mappings and macros on the mouse; \
                          run 'hecatectl reset --yes' to do it"
            .into()),
        ["pair", "--yes"] => cmd_pair(&mut open(device)?),
        ["pair"] => Err("pairing puts the receiver into pairing mode and replaces its current link; \
                         run 'hecatectl pair --yes' to do it"
            .into()),
        [] | ["-h" | "--help" | "help"] => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(format!("unknown command '{}'\n\n{USAGE}", args.join(" "))),
    }
}

fn take_device_arg(args: &mut Vec<String>) -> Result<Option<PathBuf>, String> {
    let Some(i) = args.iter().position(|a| a == "--device" || a == "-d") else { return Ok(None) };
    if i + 1 >= args.len() {
        return Err("--device needs a path".into());
    }
    let path = args.remove(i + 1);
    args.remove(i);
    Ok(Some(PathBuf::from(path)))
}

fn resolve_node(device: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(p) = device {
        return Ok(p);
    }
    let devs = hidraw::find_devices();
    match devs.as_slice() {
        [] => Err("no HECATE G3M PRO (updated) found".into()),
        [one] => Ok(one.node.clone()),
        many => {
            let nodes: Vec<String> = many.iter().map(|d| format!("{} ({})", d.node.display(), d.kind)).collect();
            Err(format!("several mice found, pick one with --device: {}", nodes.join(", ")))
        }
    }
}

fn open_node(node: &Path) -> Result<Mouse, String> {
    Ok(Mouse::new(hidraw::Device::open(node)?))
}

fn open(device: Option<PathBuf>) -> Result<Mouse, String> {
    open_node(&resolve_node(device)?)
}

fn cmd_list() -> Result<(), String> {
    let devs = hidraw::find_devices();
    if devs.is_empty() {
        return Err("no HECATE G3M PRO (updated) found".into());
    }
    for d in devs {
        println!(
            "{}  {:04x}:{:04x}  {}  receiver {}",
            d.node.display(),
            hidraw::VENDOR_ID,
            d.product_id,
            d.kind,
            version_text(d.receiver_version)
        );
    }
    Ok(())
}

fn version_text(v: Option<u16>) -> String {
    match v {
        Some(v) => format!("{} (0x{v:04x})", protocol::format_version(v)),
        None => "unknown".into(),
    }
}

fn cmd_info(m: &mut Mouse, receiver: Option<u16>) -> Result<(), String> {
    m.require_online()?;
    println!("firmware:    {}", version_text(Some(m.version()?)));
    println!("receiver:    {}", version_text(receiver));
    let b = m.battery()?;
    println!(
        "battery:     {}%{} ({} mV)",
        b.percent,
        if b.charging { ", charging" } else { "" },
        b.millivolts
    );
    println!("report rate: {}", mouse::describe_rate(m.report_rate_code()?));
    Ok(())
}

fn cmd_rate_get(m: &mut Mouse) -> Result<(), String> {
    m.require_online()?;
    println!("{}", mouse::describe_rate(m.report_rate_code()?));
    Ok(())
}

fn cmd_rate_set(m: &mut Mouse, hz: &str) -> Result<(), String> {
    let hz: u32 = hz.trim_end_matches("Hz").trim_end_matches("hz").parse().map_err(|_| format!("'{hz}' is not a rate in Hz"))?;
    if m.set_report_rate(hz)? {
        println!("report rate set to {hz} Hz (verified by read-back)");
    } else {
        println!("report rate is already {hz} Hz");
    }
    Ok(())
}

fn cmd_dump(m: &mut Mouse) -> Result<(), String> {
    m.require_online()?;
    let data = m.read_flash(0, protocol::CONFIG_BLOCK_LEN)?;
    for (i, row) in data.chunks(16).enumerate() {
        let hex: Vec<String> = row.iter().map(|b| format!("{b:02x}")).collect();
        println!("{:04x}  {}", i * 16, hex.join(" "));
    }
    Ok(())
}

fn cmd_show(m: &mut Mouse) -> Result<(), String> {
    print!("{}", format_settings(&m.read_settings()?));
    Ok(())
}

const INVALID: &str = "  [bad check byte, vendor default shown]";

fn row(out: &mut String, name: &str, value: impl std::fmt::Display, c: &Checked<impl Sized>) {
    let mark = if c.valid { "" } else { INVALID };
    writeln!(out, "{name:<20} {value}{mark}").expect("write to String");
}

fn on_off(c: &Checked<bool>) -> &'static str {
    if c.value {
        "on"
    } else {
        "off"
    }
}

/// One line per flash group; groups with a bad check byte are marked.
fn format_settings(s: &Settings) -> String {
    let mut out = String::new();
    row(&mut out, "report rate", mouse::describe_rate(Some(s.report_rate.value)), &s.report_rate);
    row(&mut out, "DPI stages", s.dpi_stage_count.value, &s.dpi_stage_count);
    row(&mut out, "active DPI stage", s.current_dpi_stage.value as u32 + 1, &s.current_dpi_stage);
    for (i, (dpi, color)) in s.dpi_stages.iter().zip(&s.dpi_colors).enumerate() {
        let v = dpi.value;
        let mut value = format!("{} DPI", v.dpi());
        if !v.is_selectable() {
            write!(value, " (register {:02x} {:02x} {:02x} is not what the vendor UI writes)", v.x, v.y, v.ex)
                .expect("write to String");
        }
        if i >= usize::from(s.dpi_stage_count.value) {
            value.push_str(" (not in use)");
        }
        row(&mut out, &format!("DPI stage {}", i + 1), value, dpi);
        let c = color.value;
        row(&mut out, "  colour", format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b), color);
    }
    row(&mut out, "DPI LED mode", s.dpi_led.mode.value, &s.dpi_led.mode);
    row(&mut out, "DPI LED brightness", s.dpi_led.brightness.value, &s.dpi_led.brightness);
    row(&mut out, "DPI LED breath speed", s.dpi_led.breath_speed.value, &s.dpi_led.breath_speed);
    row(&mut out, "DPI LED enable", s.dpi_led.enable.value, &s.dpi_led.enable);
    let bar = s.led_bar.value;
    let bar_text = format!(
        "mode {}, #{:02x}{:02x}{:02x}, speed {}, brightness {}",
        bar.mode, bar.color.r, bar.color.g, bar.color.b, bar.speed, bar.brightness
    );
    row(&mut out, "LED bar", bar_text, &s.led_bar);
    row(&mut out, "LED bar enable", s.led_bar_enable.value, &s.led_bar_enable);
    row(&mut out, "LOD", lod(s.lod.value), &s.lod);
    row(&mut out, "key debounce", s.key_debounce.value, &s.key_debounce);
    row(&mut out, "motion sync", on_off(&s.motion_sync), &s.motion_sync);
    row(&mut out, "mouse sleep time", ten_seconds(s.all_led_off_time.value), &s.all_led_off_time);
    row(&mut out, "linear correction", on_off(&s.linear_correction), &s.linear_correction);
    row(&mut out, "ripple control", on_off(&s.ripple_control), &s.ripple_control);
    row(&mut out, "move-off LED", on_off(&s.move_off_led), &s.move_off_led);
    row(&mut out, "peak performance", on_off(&s.custom_sleep), &s.custom_sleep);
    row(&mut out, "sensor sleep time", ten_seconds(s.sensor_sleep_time.value), &s.sensor_sleep_time);
    let power = match s.power_mode.value {
        settings::POWER_LP => "LP".to_string(),
        settings::POWER_HP => "HP".to_string(),
        v => format!("unknown ({v})"),
    };
    row(&mut out, "power mode", power, &s.power_mode);
    row(&mut out, "angle tune", s.angle_tune.value, &s.angle_tune);
    row(&mut out, "angle tune enable", s.angle_tune_enable.value, &s.angle_tune_enable);
    row(&mut out, "wheel changes DPI", s.wheel_changes_dpi.value, &s.wheel_changes_dpi);
    row(&mut out, "fury mode", s.fury_mode.value, &s.fury_mode);
    for (i, key) in s.keys.iter().enumerate() {
        row(&mut out, &key_name(i), describe_key(&key.value), key);
    }
    out
}

/// sensorSleepTime and allLedOffTime are stored in 10-second units (docs/ui-features.md §2.1, §2.3).
fn ten_seconds(v: u8) -> String {
    let secs = u32::from(v) * 10;
    match (secs / 60, secs % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s} s"),
    }
}

/// LOD radio "1MM" / "2MM". The vendor shows anything below 2 as 1MM and anything above as 2MM (FUN_140237730).
fn lod(v: u8) -> String {
    match v {
        1 | 2 => format!("{v} mm"),
        0 => "0 (vendor UI shows 1 mm)".into(),
        _ => format!("{v} (vendor UI shows 2 mm)"),
    }
}

/// Keys[0..5] are the buttons the vendor UI remaps; Keys[5] is very likely the DPI button (docs/ui-features.md §3.1).
const KEY_NAMES: [&str; 6] = ["left button", "right button", "wheel button", "back button", "forward button", "DPI button"];

/// Key function per docs/ui-features.md §3.2, followed by the raw `[type p1 p2]`.
fn describe_key(k: &KeyBinding) -> String {
    let (p1, p2) = (k.param1, k.param2);
    let pick = |names: &[(u8, &str)]| names.iter().find(|(v, _)| *v == p1).map(|(_, n)| n.to_string());
    let text = match k.kind {
        0 => Some("disabled".to_string()),
        1 => pick(&[(1, "left click"), (2, "right click"), (4, "middle click"), (8, "back"), (16, "forward")]),
        2 => pick(&[(1, "DPI loop"), (2, "DPI+"), (3, "DPI-")]),
        3 => pick(&[(1, "scroll left"), (2, "scroll right")]),
        0x0B => pick(&[(1, "scroll up"), (2, "scroll down")]),
        // interval unit UNCONFIRMED
        4 if p2 == 0 => Some(format!("fire key, interval {p1}, continuous")),
        4 => Some(format!("fire key, interval {p1}, {p2} presses")),
        5 => Some("combination / multimedia key (this key's shortcut slot)".to_string()),
        6 => Some(match p2 {
            0xFE => format!("macro slot {p1}, loop until released"),
            0xFF => format!("macro slot {p1}, loop until any key is pressed"),
            0xFD => format!("macro slot {p1}, loop until this key is pressed"),
            1..=250 => format!("macro slot {p1}, play {p2} times"),
            _ => format!("macro slot {p1}, loop mode 0x{p2:02x}"),
        }),
        7 => Some("report rate switch".to_string()),
        _ => None,
    };
    let raw = format!("[{} {p1} {p2}]", k.kind);
    match text {
        Some(t) => format!("{t}  {raw}"),
        None => format!("unknown function  {raw}"),
    }
}

fn key_name(i: usize) -> String {
    KEY_NAMES.get(i).map_or_else(|| format!("key {}", i + 1), |n| n.to_string())
}

fn parse_on_off(v: &str) -> Result<bool, String> {
    match v {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(format!("'{v}' is not on or off")),
    }
}

fn cmd_long_range(m: &mut Mouse, set: Option<bool>) -> Result<(), String> {
    m.require_online()?;
    if let Some(on) = set {
        m.set_long_range(on)?;
    }
    println!("long-distance mode: {}", if m.long_range()? { "on" } else { "off" });
    Ok(())
}

fn cmd_reset(m: &mut Mouse) -> Result<(), String> {
    m.require_online()?;
    m.factory_reset()?;
    println!("The device has returned to its default settings.");
    Ok(())
}

/// Vendor RepairDongle (HECATE FUN_1400fc730 / FUN_1400f87c0): StartPair, whose result is ignored, then poll
/// the status every 200 ms: 3 succeeds, 0 or 2 or no answer fails, more than 100 retries fails.
fn cmd_pair(m: &mut Mouse) -> Result<(), String> {
    const FAILED: &str = "Pairing failed, please make sure both the device and the dongle are properly connected.";
    if let Err(e) = m.start_pairing() {
        eprintln!("start pairing: {e} (polling anyway, like the vendor app)");
    }
    println!("pairing started: put the mouse into pairing mode...");
    for poll in 0..=mouse::PAIRING_POLLS {
        match m.pairing_status() {
            Ok(PairingStatus::Paired) => {
                println!("Pairing success!");
                return Ok(());
            }
            Ok(PairingStatus::Failed) => return Err(FAILED.into()),
            Err(e) => return Err(format!("{FAILED} ({e})")),
            Ok(PairingStatus::InProgress(_)) => {}
        }
        if poll < mouse::PAIRING_POLLS {
            std::thread::sleep(mouse::PAIRING_POLL_INTERVAL);
        }
    }
    Err(format!("{FAILED} (timed out)"))
}

fn cmd_slots(m: &mut Mouse) -> Result<(), String> {
    print!("{}", format_slots(&m.read_slots()?));
    Ok(())
}

/// Used slots only. Slot i is played by Keys[i] (type 5 = shortcut, type 6 = macro).
fn format_slots(s: &Slots) -> String {
    let mut out = String::new();
    for (i, sc) in s.shortcuts.iter().enumerate() {
        let Some(sc) = sc else { continue };
        let events: Vec<String> = sc.events.iter().map(|e| describe_event(e.state, e.kind, e.code)).collect();
        writeln!(out, "shortcut slot {i} ({}): {}", key_name(i), events.join(", ")).expect("write to String");
    }
    for (j, mc) in s.macros.iter().enumerate() {
        let Some(mc) = mc else { continue };
        let name = mc.name.escape_ascii();
        writeln!(out, "macro slot {j} ({}) \"{name}\", {} events:", key_name(j), mc.events.len()).expect("write to String");
        for e in &mc.events {
            writeln!(out, "    {}, {} ms", describe_event(e.state, e.kind, e.code), e.delay_ms).expect("write to String");
        }
    }
    let shortcuts = s.shortcuts.iter().flatten().count();
    let macros = s.macros.iter().flatten().count();
    writeln!(out, "{shortcuts} of {} shortcut slots and {macros} of {} macro slots in use", slots::SLOTS, slots::SLOTS)
        .expect("write to String");
    out
}

/// Event per docs/ui-features.md §3.3.
fn describe_event(state: KeyState, kind: u8, code: u16) -> String {
    let state = match state {
        KeyState::Down => "down",
        KeyState::Up => "up",
        KeyState::Other => "other",
    };
    let what = match kind {
        slots::EVENT_MODIFIER => format!("modifier {}", modifier_names(code)),
        slots::EVENT_KEY => format!("key 0x{code:02x}"),
        slots::EVENT_CONSUMER => format!("consumer 0x{code:04x}"),
        slots::EVENT_MOUSE => match code {
            1 => "mouse left".into(),
            2 => "mouse right".into(),
            4 => "mouse middle".into(),
            8 => "mouse back".into(),
            16 => "mouse forward".into(),
            _ => format!("mouse buttons 0x{code:02x}"),
        },
        _ => format!("type {kind} code 0x{code:04x}"),
    };
    format!("{state} {what}")
}

/// Modifier mask bits from the vendor table (FUN_14000bab0); other bits are shown in hex.
fn modifier_names(mask: u16) -> String {
    let mut names: Vec<String> = [(0x01, "LCtrl"), (0x02, "LShift"), (0x04, "LAlt"), (0x08, "LWin")]
        .iter()
        .filter(|(bit, _)| mask & bit != 0)
        .map(|(_, n)| n.to_string())
        .collect();
    let rest = mask & !0x0F;
    if rest != 0 || names.is_empty() {
        names.push(format!("0x{rest:02x}"));
    }
    names.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use hecate_core::settings::decode;

    #[test]
    fn erased_block_marks_every_group() {
        let text = format_settings(&decode(&[0xff; protocol::CONFIG_BLOCK_LEN]));
        assert_eq!(text.lines().count(), 55); // one per modelled group
        assert!(text.lines().all(|l| l.ends_with(INVALID)), "{text}");
        assert!(text.starts_with("report rate          1000 Hz"));
        assert!(text.contains("DPI stage 1          12800 DPI"));
    }

    #[test]
    fn valid_groups_are_unmarked() {
        let mut block = [0xff; protocol::CONFIG_BLOCK_LEN];
        block[0x00..0x02].copy_from_slice(&protocol::guarded(0x08));
        block[0x0c..0x0f].copy_from_slice(&[0x01, 0x01, 0x44]);
        block[0x0f] = protocol::checksum(&[0x01, 0x01, 0x44]);
        block[0x10..0x13].copy_from_slice(&[0x07, 0x0f, 0x00]);
        block[0x13] = protocol::checksum(&[0x07, 0x0f, 0x00]);
        block[0xb9..0xbb].copy_from_slice(&protocol::guarded(0x05));
        let text = format_settings(&decode(&block));
        let line = |prefix: &str| text.lines().find(|l| l.starts_with(prefix)).unwrap().to_string();
        assert_eq!(line("report rate"), "report rate          125 Hz");
        assert_eq!(line("DPI stage 1"), "DPI stage 1          12900 DPI");
        assert_eq!(
            line("DPI stage 2"),
            "DPI stage 2          400 DPI (register 07 0f 00 is not what the vendor UI writes)"
        );
        assert_eq!(line("power mode"), "power mode           unknown (5)");
        assert!(line("DPI stage 3").ends_with(INVALID));
        // the stage count is invalid here, so its default 6 applies
        assert!(!line("DPI stage 6").contains("not in use"));
        assert!(line("DPI stage 7").contains("12800 DPI (not in use)"));
        assert!(line("left button").contains("disabled  [0 0 0]"));
        assert!(line("key 16").ends_with(INVALID));
    }

    #[test]
    fn ten_second_units() {
        assert_eq!(ten_seconds(0), "0 s");
        assert_eq!(ten_seconds(3), "30 s");
        assert_eq!(ten_seconds(6), "1 min");
        assert_eq!(ten_seconds(9), "1 min 30 s");
        assert_eq!(ten_seconds(90), "15 min");
        assert_eq!(ten_seconds(210), "35 min");
        assert_eq!(ten_seconds(255), "42 min 30 s");
    }

    #[test]
    fn lod_labels() {
        assert_eq!(lod(1), "1 mm");
        assert_eq!(lod(2), "2 mm");
        assert_eq!(lod(0), "0 (vendor UI shows 1 mm)");
        assert_eq!(lod(255), "255 (vendor UI shows 2 mm)");
    }

    #[test]
    fn key_descriptions() {
        let d = |kind, param1, param2| describe_key(&KeyBinding { kind, param1, param2 });
        assert_eq!(d(0, 0, 0), "disabled  [0 0 0]");
        assert_eq!(d(1, 1, 1), "left click  [1 1 1]");
        assert_eq!(d(1, 16, 0), "forward  [1 16 0]");
        assert_eq!(d(2, 3, 0), "DPI-  [2 3 0]");
        assert_eq!(d(3, 2, 0), "scroll right  [3 2 0]");
        assert_eq!(d(0x0B, 1, 0), "scroll up  [11 1 0]");
        assert_eq!(d(4, 10, 0), "fire key, interval 10, continuous  [4 10 0]");
        assert_eq!(d(4, 255, 3), "fire key, interval 255, 3 presses  [4 255 3]");
        assert_eq!(d(5, 0, 0), "combination / multimedia key (this key's shortcut slot)  [5 0 0]");
        assert_eq!(d(6, 2, 0xFE), "macro slot 2, loop until released  [6 2 254]");
        assert_eq!(d(6, 2, 0xFD), "macro slot 2, loop until this key is pressed  [6 2 253]");
        assert_eq!(d(6, 0, 250), "macro slot 0, play 250 times  [6 0 250]");
        assert_eq!(d(6, 0, 0), "macro slot 0, loop mode 0x00  [6 0 0]");
        assert_eq!(d(7, 0, 0), "report rate switch  [7 0 0]");
        // unknown type, or a known type with an unknown parameter
        assert_eq!(d(9, 0, 0), "unknown function  [9 0 0]");
        assert_eq!(d(1, 3, 0), "unknown function  [1 3 0]");
        assert_eq!(d(2, 0, 0), "unknown function  [2 0 0]");
        assert_eq!(d(0xFF, 0xFF, 0xFF), "unknown function  [255 255 255]");
    }

    #[test]
    fn on_off_arguments() {
        assert_eq!(parse_on_off("on"), Ok(true));
        assert_eq!(parse_on_off("off"), Ok(false));
        assert!(parse_on_off("").is_err());
        assert!(parse_on_off("ON").is_err());
        assert!(parse_on_off("1").is_err());
    }

    #[test]
    fn version_texts() {
        assert_eq!(version_text(Some(0x0313)), "v3.19 (0x0313)");
        assert_eq!(version_text(Some(0)), "v0.00 (0x0000)");
        assert_eq!(version_text(None), "unknown");
    }

    #[test]
    fn key_names() {
        assert_eq!(key_name(0), "left button");
        assert_eq!(key_name(5), "DPI button");
        assert_eq!(key_name(6), "key 7");
        assert_eq!(key_name(15), "key 16");
    }

    #[test]
    fn event_descriptions() {
        assert_eq!(describe_event(KeyState::Down, slots::EVENT_CONSUMER, 0x00E9), "down consumer 0x00e9");
        assert_eq!(describe_event(KeyState::Up, slots::EVENT_KEY, 0x06), "up key 0x06");
        assert_eq!(describe_event(KeyState::Down, slots::EVENT_MODIFIER, 0x03), "down modifier LCtrl+LShift");
        assert_eq!(describe_event(KeyState::Down, slots::EVENT_MOUSE, 1), "down mouse left");
        assert_eq!(describe_event(KeyState::Up, slots::EVENT_MOUSE, 3), "up mouse buttons 0x03");
        assert_eq!(describe_event(KeyState::Other, 9, 0xBEEF), "other type 9 code 0xbeef");
    }

    #[test]
    fn modifier_masks() {
        assert_eq!(modifier_names(0x0F), "LCtrl+LShift+LAlt+LWin");
        assert_eq!(modifier_names(0x08), "LWin");
        assert_eq!(modifier_names(0x00), "0x00");
        assert_eq!(modifier_names(0x11), "LCtrl+0x10");
        assert_eq!(modifier_names(0xFFF0), "0xfff0");
    }

    #[test]
    fn slot_listing() {
        assert_eq!(format_slots(&Slots::default()), "0 of 16 shortcut slots and 0 of 16 macro slots in use\n");
        let mut s = Slots::default();
        s.shortcuts[0] = Some(slots::Shortcut::multimedia(0x00E9));
        s.macros[6] = Some(slots::Macro {
            name: vec![b'a', 0xE5],
            events: vec![
                slots::MacroEvent { state: KeyState::Down, kind: slots::EVENT_KEY, code: 4, delay_ms: 50 },
                slots::MacroEvent { state: KeyState::Up, kind: slots::EVENT_KEY, code: 4, delay_ms: 0 },
            ],
        });
        let text = format_slots(&s);
        assert_eq!(
            text,
            "shortcut slot 0 (left button): down consumer 0x00e9, up consumer 0x00e9\n\
             macro slot 6 (key 7) \"a\\xe5\", 2 events:\n    down key 0x04, 50 ms\n    up key 0x04, 0 ms\n\
             1 of 16 shortcut slots and 1 of 16 macro slots in use\n"
        );
    }
}
