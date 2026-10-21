//! Diagnostic: sends ClearSetting (08 09) once with a long timeout and prints every 08 report
//! seen for a while afterwards, to learn whether and when the mouse answers and comes back.

use hecate_core::hidraw::{self, Device};
use hecate_core::mouse::Mouse;
use hecate_core::protocol as p;
use std::time::{Duration, Instant};

fn main() -> Result<(), String> {
    let node = hidraw::find_devices().into_iter().next().ok_or("no device")?.node;
    let mut m = Mouse::new(Device::open(&node)?);
    m.require_online()?;
    let mut dev = Device::open(&node)?;
    let t = Instant::now();
    match dev.transact_with(&p::build(p::CMD_CLEAR_SETTING, &[])?, 1, Duration::from_secs(6)) {
        Ok(r) => println!("08 09 answered after {} ms: {:02x?}", t.elapsed().as_millis(), r),
        Err(e) => println!("08 09: {e} ({} ms)", t.elapsed().as_millis()),
    }
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(500));
        let online = m.online().map(|o| if o { "online" } else { "asleep" }).unwrap_or("no reply");
        println!("{:>5} ms  {online}", t.elapsed().as_millis());
    }
    Ok(())
}
