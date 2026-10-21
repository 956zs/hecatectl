//! Live check of the slot write path: rewrites shortcut slot N with the bytes it already holds,
//! then reads it back. Usage: slot_rewrite [N] (default 3). Refuses empty slots.

use hecate_core::hidraw::{self, Device};
use hecate_core::mouse::Mouse;

fn main() -> Result<(), String> {
    let slot: usize = std::env::args().nth(1).map(|s| s.parse().map_err(|_| "slot must be 0..15")).transpose()?.unwrap_or(3);
    let node = hidraw::find_devices().into_iter().next().ok_or("no device")?.node;
    let mut m = Mouse::new(Device::open(&node)?);
    let before = m.read_slots()?;
    let s = before.shortcuts.get(slot).cloned().flatten().ok_or(format!("shortcut slot {slot} is empty"))?;
    println!("slot {slot} before: {s:?}");
    m.write_shortcut(slot, &s)?;
    let after = m.read_slots()?;
    let a = after.shortcuts.get(slot).cloned().flatten().ok_or("slot empty after write")?;
    println!("slot {slot} after:  {a:?}");
    if a == s {
        println!("rewrite verified");
        Ok(())
    } else {
        Err("slot content changed".into())
    }
}
