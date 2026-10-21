use hecate_core::mouse::{self, Mouse};
use hecate_core::settings::{self, Checked, KeyBinding, Settings};
use hecate_core::{hidraw, protocol};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: hecatectl [--device /dev/hidrawN] <command>

commands:
  list            show detected mice and their hidraw nodes
  info            connection, firmware version, battery and report rate
  rate            show the report (polling) rate
  rate <HZ>       set the report rate: 125, 250, 500 or 1000
  dump            hex dump of the settings block (flash 0x000-0x0ff, read-only)
  show            decoded settings block (read-only)";

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
        ["info"] => cmd_info(&mut open(device)?),
        ["rate"] => cmd_rate_get(&mut open(device)?),
        ["rate", hz] => cmd_rate_set(&mut open(device)?, hz),
        ["dump"] => cmd_dump(&mut open(device)?),
        ["show"] => cmd_show(&mut open(device)?),
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

fn open(device: Option<PathBuf>) -> Result<Mouse, String> {
    let node = match device {
        Some(p) => p,
        None => {
            let devs = hidraw::find_devices();
            match devs.as_slice() {
                [] => return Err("no HECATE G3M PRO (updated) found".into()),
                [one] => one.node.clone(),
                many => {
                    let nodes: Vec<String> = many.iter().map(|d| format!("{} ({})", d.node.display(), d.kind)).collect();
                    return Err(format!("several mice found, pick one with --device: {}", nodes.join(", ")));
                }
            }
        }
    };
    Ok(Mouse::new(hidraw::Device::open(&node)?))
}

fn cmd_list() -> Result<(), String> {
    let devs = hidraw::find_devices();
    if devs.is_empty() {
        return Err("no HECATE G3M PRO (updated) found".into());
    }
    for d in devs {
        println!("{}  {:04x}:{:04x}  {}", d.node.display(), hidraw::VENDOR_ID, d.product_id, d.kind);
    }
    Ok(())
}

fn cmd_info(m: &mut Mouse) -> Result<(), String> {
    m.require_online()?;
    let v = m.version()?;
    println!("firmware:    0x{v:04x}");
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
        let name = KEY_NAMES.get(i).map_or_else(|| format!("key {}", i + 1), |n| n.to_string());
        row(&mut out, &name, describe_key(&key.value), key);
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
}
