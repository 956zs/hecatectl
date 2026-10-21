mod hidraw;
mod mouse;
mod protocol;

use mouse::Mouse;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: hecatectl [--device /dev/hidrawN] <command>

commands:
  list            show detected mice and their hidraw nodes
  info            connection, firmware version, battery and report rate
  rate            show the report (polling) rate
  rate <HZ>       set the report rate: 125, 250, 500 or 1000
  dump            hex dump of the settings block (flash 0x000-0x0ff, read-only)";

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
