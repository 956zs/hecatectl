//! Diagnostic: replays the vendor app's connect sequence step by step and tries a flash read
//! after each step, to find what the mouse needs before it answers `08 08`.
//! Sends only commands the vendor app sends on every connect (03, 01, 02) and flash reads;
//! never writes flash. Run with HECATECTL_DEBUG=1 to see every packet.

use hecate_core::hidraw::{self, Device};
use hecate_core::protocol::{self as p, Packet};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const READ_TIMEOUT: Duration = Duration::from_millis(1000);
const WAKE_WAIT: Duration = Duration::from_secs(20);
const BATTERY_TIMEOUT: Duration = Duration::from_millis(5000);

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

fn step(dev: &mut Device, name: &str, req: &Packet, timeout: Duration) -> Option<Packet> {
    let t = Instant::now();
    match dev.transact_with(req, 1, timeout) {
        Ok(r) => {
            println!("{name:<14} OK  {:>4} ms  {}", t.elapsed().as_millis(), hex(&r));
            Some(r)
        }
        Err(e) => {
            println!("{name:<14} ERR {:>4} ms  {e}", t.elapsed().as_millis());
            None
        }
    }
}

fn main() -> Result<(), String> {
    let node = hidraw::find_devices().into_iter().next().ok_or("no device")?.node;
    let mut dev = Device::open(&node)?;
    let online = p::build(p::CMD_ONLINE, &[])?;
    let read0 = p::flash_read_request(0);

    eprintln!("waiting for the mouse to be online (move it)...");
    let deadline = Instant::now() + WAKE_WAIT;
    loop {
        if let Some(r) = step(&mut dev, "online", &online, Duration::from_millis(200)) {
            if r[p::DATA_OFFSET] != 0 {
                break;
            }
        }
        if Instant::now() > deadline {
            return Err("mouse stayed offline".into());
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    step(&mut dev, "read 0x000 #1", &read0, READ_TIMEOUT);

    // 01: device info with 4 random bytes at [6..9], [5]=08 (FUN_180015c90)
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let r = n.to_le_bytes();
    step(&mut dev, "devinfo 01", &p::build(0x01, &[0, 0, 0, 0x08, r[0], r[1], r[2], r[3]])?, READ_TIMEOUT);
    step(&mut dev, "read 0x000 #2", &read0, READ_TIMEOUT);

    // 02: SetActivedStatus(1), [5]=01 [6]=01 (UsbMouse_SetActivedStatus)
    step(&mut dev, "active=1", &p::build(0x02, &[0, 0, 0, 0x01, 0x01])?, READ_TIMEOUT);
    for i in 3..=5 {
        step(&mut dev, &format!("read 0x000 #{i}"), &read0, READ_TIMEOUT);
    }
    step(&mut dev, "read 0x100", &p::flash_read_request(0x100), READ_TIMEOUT);
    step(&mut dev, "battery", &p::build(p::CMD_BATTERY, &[])?, BATTERY_TIMEOUT);
    step(&mut dev, "version", &p::build(p::CMD_VERSION, &[])?, READ_TIMEOUT);

    // The vendor app sends SetActivedStatus(0) when it closes (FUN_14009f580 -> FUN_1400fc550).
    step(&mut dev, "active=0", &p::build(0x02, &[0, 0, 0, 0x01, 0x00])?, READ_TIMEOUT);
    Ok(())
}
