//! Device worker thread: owns the hidraw handle, polls for hot-plug and sleep, and runs the
//! commands the UI sends. Nothing here touches Qt; results go out through a `publish` callback.

use crate::settings_map;
use hecate_core::hidraw::{self, DeviceInfo};
use hecate_core::mouse::{Mouse, PAIRING_POLLS, PAIRING_POLL_INTERVAL};
use hecate_core::protocol::PairingStatus;
use hecate_core::settings::Settings;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

const NO_DEVICE: &str = "no HECATE G3M PRO (updated) found";

pub enum Command {
    Refresh,
    /// A settings patch from the pages (see `settings_map`).
    Change(String),
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
pub fn run(commands: Receiver<Command>, intervals: Intervals, publish: impl FnMut(Event) -> bool) {
    let mut w = Worker { publish, alive: true, conn: None, state: Snapshot::default(), stale: true, poll_error: None };
    w.poll();
    while w.alive {
        // Like the vendor connector: "connected" means online with the full read done.
        let connected = w.state.online && w.state.settings.is_some();
        let interval = if connected { intervals.online } else { intervals.offline };
        match commands.recv_timeout(interval) {
            Ok(Command::Refresh) => w.command(|w| w.sync(true)),
            Ok(Command::Change(patch)) => w.command(|w| w.change(&patch)),
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
        if self.stale {
            next.firmware = Some(conn.mouse.version()?);
            next.settings = Some(conn.mouse.read_settings()?);
            next.long_range = Some(conn.mouse.long_range()?);
            self.stale = false;
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
            // The original error is the one to report; a failing re-read shows up on the next poll.
            let _ = self.sync(true);
        }
        result
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
                let _ = self.sync(true);
                Err(e)
            }
        }
    }

    /// `08 09`, then a full re-read like the vendor (FUN_140242e80).
    fn factory_reset(&mut self) -> Result<(), String> {
        let result = self.awake_mouse().and_then(|m| m.factory_reset());
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
        let _ = self.sync(true);
        result.map(|_| ())
    }

    /// True when paired. An unanswered status poll counts as failed, as in the vendor app.
    fn run_pairing(&mut self) -> Result<bool, String> {
        self.sync(false)?;
        // The vendor ignores the StartPair reply and polls either way (FUN_1400fc730).
        if let Err(e) = self.conn.as_mut().ok_or(NO_DEVICE)?.mouse.start_pairing() {
            eprintln!("hecate-gui: start pairing: {e}");
        }
        for poll in 1..=PAIRING_POLLS {
            let status = self.conn.as_mut().ok_or(NO_DEVICE)?.mouse.pairing_status();
            match status {
                Ok(PairingStatus::InProgress(_)) => self.emit(Event::PairingProgress(poll)),
                Ok(PairingStatus::Paired) => return Ok(true),
                Ok(PairingStatus::Failed) | Err(_) => return Ok(false),
            }
            std::thread::sleep(PAIRING_POLL_INTERVAL);
        }
        Ok(false)
    }
}
