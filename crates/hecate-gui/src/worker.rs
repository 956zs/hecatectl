//! Device worker thread: owns the hidraw handle, polls for hot-plug and sleep, and runs the
//! commands the UI sends. Nothing here touches Qt; results go out through a `publish` callback.

use crate::keymap::{self, Step};
use crate::macro_name::NameEncoding;
use crate::settings_map;
use hecate_core::hidraw::{self, DeviceInfo};
use hecate_core::mouse::{Mouse, PAIRING_POLLS, PAIRING_POLL_INTERVAL};
use hecate_core::protocol::{PairingStatus, FLASH_CHUNK};
use hecate_core::settings::Settings;
use hecate_core::slots::Slots;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

const NO_DEVICE: &str = "no HECATE G3M PRO (updated) found";

pub enum Command {
    Refresh,
    /// A settings patch from the pages (see `settings_map`).
    Change(String),
    /// A button assignment from the KeySet page (see `keymap`).
    Assign(String),
    SetLongRange(bool),
    FactoryReset,
    StartPairing,
}

/// What the UI shows. Values read while the mouse was awake are kept while it sleeps.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub present: bool,
    pub online: bool,
    /// "receiver" or "wired"; empty without a device.
    pub connection: &'static str,
    pub battery_percent: Option<u8>,
    pub charging: bool,
    pub firmware: Option<u16>,
    /// bcdDevice of the open hidraw node: the receiver's firmware when connected through it.
    pub receiver_firmware: Option<u16>,
    /// Long-distance radio mode, read with the settings.
    pub long_range: Option<bool>,
    /// The settings block as flash holds it; None until the first full read on this connection.
    pub settings: Option<Settings>,
    /// Shortcut and macro slots, read with the settings.
    pub slots: Option<Slots>,
}

/// Bytes at 0x000 compared on every online poll: rate, DPI count and active stage, LOD. One read,
/// and it catches the on-mouse DPI and report-rate buttons as well as other programs.
const HEAD_LEN: usize = FLASH_CHUNK;

impl Snapshot {
    /// The same device state with nothing read from flash: the pages show "loading" until a full read
    /// succeeds, so values that may no longer be in flash are never shown or edited.
    pub fn loading(&self) -> Snapshot {
        Snapshot { settings: None, slots: None, long_range: None, ..self.clone() }
    }
}

/// Whether the first bytes of flash differ from the settings last read.
pub fn head_changed(settings: &Settings, head: &[u8]) -> bool {
    head.get(..HEAD_LEN) != settings.raw().get(..HEAD_LEN)
}

pub enum Event {
    State(Box<Snapshot>),
    Busy(bool),
    Error(String),
    /// Factory reset done and everything read again.
    ResetDone,
    /// Pairing status polls so far.
    PairingProgress(u32),
    PairingFinished(bool),
}

/// Poll intervals while no mouse answers and while it is online.
#[derive(Clone, Copy)]
pub struct Intervals {
    pub offline: Duration,
    pub online: Duration,
}

/// Runs until the command channel closes or `publish` returns false (the UI object is gone).
pub fn run(commands: Receiver<Command>, intervals: Intervals, encoding: NameEncoding, publish: impl FnMut(Event) -> bool) {
    let mut w = Worker { publish, alive: true, conn: None, state: Snapshot::default(), stale: true, poll_error: None, encoding };
    w.poll();
    while w.alive {
        // Like the vendor connector: "connected" means online with the full read done.
        let connected = w.state.online && w.state.settings.is_some();
        let interval = if connected { intervals.online } else { intervals.offline };
        match commands.recv_timeout(interval) {
            Ok(Command::Refresh) => w.command(|w| w.sync(true)),
            Ok(Command::Change(patch)) => w.command(|w| w.change(&patch)),
            Ok(Command::Assign(assignment)) => w.command(|w| w.assign(&assignment)),
            Ok(Command::SetLongRange(on)) => w.command(|w| w.set_long_range(on)),
            Ok(Command::FactoryReset) => w.command(Worker::factory_reset),
            Ok(Command::StartPairing) => w.command(Worker::pair),
            Err(RecvTimeoutError::Timeout) => w.poll(),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

struct Conn {
    node: PathBuf,
    mouse: Mouse,
}

struct Worker<P> {
    publish: P,
    alive: bool,
    conn: Option<Conn>,
    state: Snapshot,
    /// Firmware and settings still need reading (new device, wake-up, or a failed read).
    stale: bool,
    /// Last error from polling and how many polls in a row hit it. Only a repeat is reported, and
    /// only once: over 2.4G a single read often fails while the mouse dozes off or wakes up.
    poll_error: Option<(String, u32)>,
    /// Encoding for macro names written to the mouse.
    encoding: NameEncoding,
}

/// UI key for the connection kind (hidraw::PRODUCT_IDS names the cable product "wired").
fn connection_key(info: &DeviceInfo) -> &'static str {
    if info.kind == "wired" {
        "wired"
    } else {
        "receiver"
    }
}

impl<P: FnMut(Event) -> bool> Worker<P> {
    fn emit(&mut self, event: Event) {
        if !(self.publish)(event) {
            self.alive = false;
        }
    }

    fn report(&mut self, error: String) {
        eprintln!("hecate-gui: {error}");
        self.emit(Event::Error(error));
    }

    fn poll(&mut self) {
        match self.sync(false) {
            Ok(()) => self.poll_error = None,
            Err(e) => {
                let count = match &self.poll_error {
                    Some((last, n)) if *last == e => n.saturating_add(1),
                    _ => 1,
                };
                self.poll_error = Some((e.clone(), count));
                if count == 2 {
                    self.report(e);
                }
            }
        }
    }

    /// A UI command: busy while it runs, and every failure is reported.
    fn command(&mut self, f: impl FnOnce(&mut Self) -> Result<(), String>) {
        self.emit(Event::Busy(true));
        if let Err(e) = f(self) {
            // Already shown: a poll hitting the same error does not show it again.
            self.poll_error = Some((e.clone(), 2));
            self.report(e);
        }
        self.emit(Event::Busy(false));
    }

    fn publish_state(&mut self, next: Snapshot) {
        if next != self.state {
            self.state = next.clone();
            self.emit(Event::State(Box::new(next)));
        }
    }

    /// Reads the hardware into the snapshot and publishes it when it changed. `full` also
    /// re-reads firmware, settings and long-range mode.
    fn sync(&mut self, full: bool) -> Result<(), String> {
        let mut next = self.state.clone();
        let result = self.read(&mut next, full);
        self.publish_state(next);
        result
    }

    fn read(&mut self, next: &mut Snapshot, full: bool) -> Result<(), String> {
        let found = hidraw::find_devices();
        // Stay on the open node while it exists; otherwise take the first device found.
        if let Some(c) = &self.conn {
            if !found.iter().any(|d| d.node == c.node) {
                self.conn = None;
            }
        }
        let info = match &self.conn {
            Some(c) => found.iter().find(|d| d.node == c.node),
            None => found.first(),
        };
        let Some(info) = info.cloned() else {
            *next = Snapshot::default();
            return Ok(());
        };
        let conn = match &mut self.conn {
            Some(c) => c,
            None => {
                *next = Snapshot { present: true, connection: connection_key(&info), ..Snapshot::default() };
                self.stale = true;
                let dev = hidraw::Device::open(&info.node)?;
                self.conn.insert(Conn { node: info.node.clone(), mouse: Mouse::new(dev) })
            }
        };
        next.receiver_firmware = info.receiver_version;
        let was_online = next.online;
        next.online = false;
        let online = match conn.mouse.online() {
            Ok(online) => online,
            Err(e) => {
                // Reopen on the next poll in case the node was replaced.
                self.conn = None;
                return Err(e);
            }
        };
        next.online = online;
        if !online {
            return Ok(());
        }
        if full || !was_online {
            self.stale = true;
        }
        if let (false, Some(settings)) = (self.stale, &next.settings) {
            if head_changed(settings, &conn.mouse.read_flash(0, HEAD_LEN)?) {
                self.stale = true;
            }
        }
        if self.stale {
            // One locked operation, so settings and slots come from the same moment. The mouse was
            // just seen online, so nothing inside waits for it while holding the lock.
            let read = conn.mouse.exclusive(|m| Ok((m.version()?, m.read_settings()?, m.read_slots()?, m.long_range()?)));
            match read {
                Ok((firmware, settings, slots, long_range)) => {
                    next.firmware = Some(firmware);
                    next.settings = Some(settings);
                    next.slots = Some(slots);
                    next.long_range = Some(long_range);
                    self.stale = false;
                }
                Err(e) => {
                    *next = next.loading();
                    return Err(e);
                }
            }
        }
        let battery = conn.mouse.battery()?;
        next.battery_percent = Some(battery.percent);
        next.charging = battery.charging;
        Ok(())
    }

    /// The mouse, after a poll has seen it online.
    fn awake_mouse(&mut self) -> Result<&mut Mouse, String> {
        self.sync(false)?;
        if !self.state.online {
            return Err("the mouse is asleep; move it to wake it".into());
        }
        Ok(&mut self.conn.as_mut().ok_or(NO_DEVICE)?.mouse)
    }

    /// Applies a settings patch: the vendor-style change on the settings last read, then
    /// `Mouse::apply` (validate, write the diff, verify by read-back). After any failure the
    /// settings are read again, so the UI only shows what is in flash.
    fn change(&mut self, patch: &str) -> Result<(), String> {
        let result = self.try_change(patch);
        if result.is_err() {
            self.reload();
        }
        result
    }

    /// After a failed write, reset or read: back to "loading" until a full read succeeds. A failing
    /// re-read leaves the pages in that state; the original error is the one reported.
    fn reload(&mut self) {
        let loading = self.state.loading();
        self.publish_state(loading);
        self.stale = true;
        let _ = self.sync(true);
    }

    fn try_change(&mut self, patch: &str) -> Result<(), String> {
        let changes = settings_map::parse_patch(patch)?;
        self.awake_mouse()?;
        let old = self.state.settings.clone().ok_or("the settings have not been read yet")?;
        let new = settings_map::apply(&old, &changes)?;
        let fresh = self.conn.as_mut().ok_or(NO_DEVICE)?.mouse.apply(&old, &new)?;
        let mut next = self.state.clone();
        next.settings = Some(fresh);
        self.publish_state(next);
        Ok(())
    }

    /// Assigns a button: the planned slot write first (verified by `write_shortcut` / `write_macro`),
    /// then the binding through `Mouse::apply`. After any failure everything is read again.
    fn assign(&mut self, assignment: &str) -> Result<(), String> {
        let result = self.try_assign(assignment);
        if result.is_err() {
            self.reload();
        }
        result
    }

    fn try_assign(&mut self, assignment: &str) -> Result<(), String> {
        let assignment = keymap::parse_assignment(assignment)?;
        self.awake_mouse()?;
        let settings = self.state.settings.clone().ok_or("the settings have not been read yet")?;
        let mut slots = self.state.slots.clone().ok_or("the key slots have not been read yet")?;
        let steps = keymap::plan(&settings, &assignment, self.encoding)?;
        let mouse = &mut self.conn.as_mut().ok_or(NO_DEVICE)?.mouse;
        // Slot and binding in one locked operation; the steps are already in slot-first order. The
        // binding step may be skipped as unchanged, so first check the settings are still what was read.
        let settings = mouse.exclusive(|mouse| {
            let mut settings = settings;
            if mouse.read_settings()? != settings {
                return Err("the mouse's settings changed since they were read; nothing was written".into());
            }
            for step in steps {
                match step {
                    Step::WriteShortcut(i, shortcut) => {
                        mouse.write_shortcut(i, &shortcut)?;
                        slots.shortcuts[i] = Some(shortcut);
                    }
                    Step::WriteMacro(i, m) => {
                        mouse.write_macro(i, &m)?;
                        slots.macros[i] = Some(m);
                    }
                    Step::Bind(new) => settings = mouse.apply(&settings, &new)?,
                }
            }
            Ok(settings)
        })?;
        let mut next = self.state.clone();
        next.settings = Some(settings);
        next.slots = Some(slots);
        self.publish_state(next);
        Ok(())
    }

    /// `08 16`, then reads the mode back. After a failure everything is read again.
    fn set_long_range(&mut self, on: bool) -> Result<(), String> {
        let result = self.awake_mouse().and_then(|m| {
            m.set_long_range(on)?;
            m.long_range()
        });
        match result {
            Ok(mode) => {
                let mut next = self.state.clone();
                next.long_range = Some(mode);
                self.publish_state(next);
                Ok(())
            }
            Err(e) => {
                self.reload();
                Err(e)
            }
        }
    }

    /// `08 09`, then a full re-read like the vendor (FUN_140242e80).
    fn factory_reset(&mut self) -> Result<(), String> {
        let result = self.awake_mouse().and_then(|m| m.factory_reset());
        // Everything read before is stale after a reset, acknowledged or not.
        let loading = self.state.loading();
        self.publish_state(loading);
        self.stale = true;
        let reread = self.sync(true);
        result?;
        reread?;
        self.emit(Event::ResetDone);
        Ok(())
    }

    /// Pairing like the vendor: StartPair on the receiver, then status polls. Everything is read
    /// again afterwards, since the mouse on the other end may have changed.
    fn pair(&mut self) -> Result<(), String> {
        self.emit(Event::PairingProgress(0));
        let result = self.run_pairing();
        self.emit(Event::PairingFinished(result == Ok(true)));
        self.reload();
        result.map(|_| ())
    }

    /// True when paired. An unanswered status poll counts as failed, as in the vendor app.
    fn run_pairing(&mut self) -> Result<bool, String> {
        self.sync(false)?;
        let Worker { conn, publish, alive, .. } = self;
        let mouse = &mut conn.as_mut().ok_or(NO_DEVICE)?.mouse;
        // StartPair and the polls in one locked operation, as core asks.
        mouse.exclusive(|mouse| {
            // The vendor ignores the StartPair reply and polls either way (FUN_1400fc730).
            if let Err(e) = mouse.start_pairing() {
                eprintln!("hecate-gui: start pairing: {e}");
            }
            for poll in 1..=PAIRING_POLLS {
                match mouse.pairing_status() {
                    Ok(PairingStatus::InProgress(_)) => {
                        if !publish(Event::PairingProgress(poll)) {
                            *alive = false;
                        }
                    }
                    Ok(PairingStatus::Paired) => return Ok(true),
                    Ok(PairingStatus::Failed) | Err(_) => return Ok(false),
                }
                std::thread::sleep(PAIRING_POLL_INTERVAL);
            }
            Ok(false)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hecate_core::protocol::{checksum, CONFIG_BLOCK_LEN};
    use hecate_core::settings::{self, Block};

    fn read_settings() -> Settings {
        let mut b: Block = [0xFF; CONFIG_BLOCK_LEN];
        for (at, v) in [(0x00, 1u8), (0x02, 6), (0x04, 2), (0x0A, 1)] {
            b[at] = v;
            b[at + 1] = checksum(&[v]);
        }
        settings::decode(&b)
    }

    /// Review R1: a change at 0x000..0x00A made outside this app (DPI or rate button on the mouse, the
    /// CLI) is noticed on the next online poll.
    #[test]
    fn r1_head_change_marks_the_snapshot_stale() {
        let s = read_settings();
        let head = s.raw()[..HEAD_LEN].to_vec();
        assert!(!head_changed(&s, &head));
        // DPI button: active stage 2 -> 3.
        let mut moved = head.clone();
        moved[4] = 3;
        moved[5] = checksum(&[3]);
        assert!(head_changed(&s, &moved));
        // Rate button: 1000 -> 125 Hz.
        let mut rate = head.clone();
        rate[0] = 0x08;
        assert!(head_changed(&s, &rate));
        // A short read never counts as unchanged.
        assert!(head_changed(&s, &head[..4]));
        assert_eq!(HEAD_LEN, 10);
    }

    /// Review R4: after a failed write, reset or read, nothing read from flash stays published.
    #[test]
    fn r4_loading_drops_everything_read_from_flash() {
        let s = Snapshot {
            present: true,
            online: true,
            connection: "receiver",
            battery_percent: Some(90),
            charging: false,
            firmware: Some(0x0313),
            receiver_firmware: Some(0x0102),
            long_range: Some(false),
            settings: Some(read_settings()),
            slots: Some(Slots::default()),
        };
        let l = s.loading();
        assert_eq!((l.settings.is_none(), l.slots.is_none(), l.long_range.is_none()), (true, true, true));
        assert_eq!((l.present, l.online, l.connection, l.battery_percent), (true, true, "receiver", Some(90)));
        assert_eq!(l.firmware, Some(0x0313));
    }
}
