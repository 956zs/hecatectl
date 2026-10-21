//! Device worker thread: owns the hidraw handle, polls for hot-plug and sleep, and runs the
//! commands the UI sends. Nothing here touches Qt; results go out through a `publish` callback.

use hecate_core::hidraw::{self, DeviceInfo};
use hecate_core::mouse::Mouse;
use hecate_core::protocol;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

pub enum Command {
    Refresh,
    SetReportRate(u32),
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
    pub report_rate: Option<u32>,
}

pub enum Event {
    State(Snapshot),
    Busy(bool),
    Error(String),
}

/// Runs until the command channel closes or `publish` returns false (the UI object is gone).
pub fn run(commands: Receiver<Command>, interval: Duration, publish: impl FnMut(Event) -> bool) {
    let mut w = Worker { publish, alive: true, conn: None, state: Snapshot::default(), stale: true, poll_error: None };
    w.poll();
    while w.alive {
        match commands.recv_timeout(interval) {
            Ok(Command::Refresh) => w.command(|w| w.sync(true)),
            Ok(Command::SetReportRate(hz)) => w.command(|w| w.set_report_rate(hz)),
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
    /// Firmware and report rate still need reading (new device, wake-up, or a failed read).
    stale: bool,
    /// Last error from polling, so a persisting problem is reported once.
    poll_error: Option<String>,
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
            Err(e) if self.poll_error.as_ref() == Some(&e) => {}
            Err(e) => {
                self.poll_error = Some(e.clone());
                self.report(e);
            }
        }
    }

    /// A UI command: busy while it runs, and every failure is reported.
    fn command(&mut self, f: impl FnOnce(&mut Self) -> Result<(), String>) {
        self.emit(Event::Busy(true));
        if let Err(e) = f(self) {
            self.poll_error = Some(e.clone());
            self.report(e);
        }
        self.emit(Event::Busy(false));
    }

    /// Reads the hardware into the snapshot and publishes it when it changed. `full` also
    /// re-reads firmware and report rate.
    fn sync(&mut self, full: bool) -> Result<(), String> {
        let mut next = self.state.clone();
        let result = self.read(&mut next, full);
        if next != self.state {
            self.state = next.clone();
            self.emit(Event::State(next));
        }
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
                self.conn.insert(Conn { node: info.node, mouse: Mouse::new(dev) })
            }
        };
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
            next.report_rate = conn.mouse.report_rate_code()?.and_then(protocol::rate_hz);
            self.stale = false;
        }
        let battery = conn.mouse.battery()?;
        next.battery_percent = Some(battery.percent);
        next.charging = battery.charging;
        Ok(())
    }

    /// Writes the rate, then re-reads everything whether or not the write worked, so the UI only
    /// shows what is in flash.
    fn set_report_rate(&mut self, hz: u32) -> Result<(), String> {
        self.sync(false)?;
        let conn = self.conn.as_mut().ok_or("no HECATE G3M PRO (updated) found")?;
        let written = conn.mouse.set_report_rate(hz).map(|_| ());
        let reread = self.sync(true);
        written.and(reread)
    }
}
