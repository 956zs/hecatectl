//! `Device` QML singleton: mouse state for the UI. All USB I/O runs on the worker thread in
//! `crate::worker`; this object only sends it commands and applies the events it queues back.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, present, READ, NOTIFY)]
        #[qproperty(bool, online, READ, NOTIFY)]
        #[qproperty(QString, connection, READ, NOTIFY)]
        #[qproperty(i32, battery_percent, READ, NOTIFY)]
        #[qproperty(bool, charging, READ, NOTIFY)]
        #[qproperty(QString, firmware, READ, NOTIFY)]
        #[qproperty(QString, receiver_firmware, READ, NOTIFY)]
        #[qproperty(i32, long_range, READ, NOTIFY)]
        #[qproperty(bool, loaded, READ, NOTIFY)]
        #[qproperty(QString, settings_json, READ, NOTIFY)]
        #[qproperty(QString, options_json, READ, CONSTANT)]
        #[qproperty(bool, busy, READ, NOTIFY)]
        #[qproperty(bool, pairing, READ, NOTIFY)]
        #[qproperty(f64, pairing_progress, READ, NOTIFY)]
        #[qproperty(QString, last_error, READ, NOTIFY)]
        type Device = super::DeviceRust;

        /// Re-reads everything from the mouse.
        #[qinvokable]
        fn refresh(&self);

        /// Applies a settings patch (JSON object, see settings_map.rs), then reads the settings back.
        #[qinvokable]
        fn change(&self, patch: &QString);

        /// Long-distance radio mode (`08 16`), read back afterwards.
        #[qinvokable]
        fn set_long_range(&self, on: bool);

        /// Factory reset (`08 09`) followed by a full re-read; `resetFinished` on success.
        #[qinvokable]
        fn factory_reset(&self);

        /// Receiver pairing (`08 05` + status polls); `pairingFinished` with the result.
        #[qinvokable]
        fn start_pairing(&self);

        /// DPI at a slider position, for the label while dragging.
        #[qinvokable]
        fn dpi_at(&self, position: i32) -> i32;

        /// Emitted for every error, also when the message repeats.
        #[qsignal]
        fn error_occurred(self: Pin<&mut Self>, message: QString);

        #[qsignal]
        fn reset_finished(self: Pin<&mut Self>);

        #[qsignal]
        fn pairing_finished(self: Pin<&mut Self>, success: bool);
    }

    impl cxx_qt::Threading for Device {}
    impl cxx_qt::Initialize for Device {}
}

use super::update;
use crate::config;
use crate::display;
use crate::dpi_slider;
use crate::settings_map;
use crate::worker::{self, Command, Event};
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use hecate_core::mouse::PAIRING_POLLS;
use hecate_core::protocol;
use std::sync::mpsc::{self, Sender};

pub struct DeviceRust {
    present: bool,
    online: bool,
    connection: QString,
    /// -1 while unknown.
    battery_percent: i32,
    charging: bool,
    firmware: QString,
    /// Empty unless the receiver row is shown (`display::receiver_version`).
    receiver_firmware: QString,
    /// -1 while unknown, else 0 or 1.
    long_range: i32,
    /// The first full settings read on this connection has finished.
    loaded: bool,
    /// `settings_map::to_json`, empty until loaded.
    settings_json: QString,
    options_json: QString,
    busy: bool,
    pairing: bool,
    /// 0..1 of the vendor's pairing poll budget.
    pairing_progress: f64,
    last_error: QString,
    commands: Option<Sender<Command>>,
}

impl Default for DeviceRust {
    fn default() -> Self {
        let options = settings_map::options(&config::get().product_url);
        Self {
            present: false,
            online: false,
            connection: QString::default(),
            battery_percent: -1,
            charging: false,
            firmware: QString::default(),
            receiver_firmware: QString::default(),
            long_range: -1,
            loaded: false,
            settings_json: QString::default(),
            options_json: QString::from(&options.to_string()),
            busy: false,
            pairing: false,
            pairing_progress: 0.0,
            last_error: QString::default(),
            commands: None,
        }
    }
}

impl qobject::Device {
    pub fn refresh(&self) {
        self.send(Command::Refresh);
    }

    pub fn change(&self, patch: &QString) {
        self.send(Command::Change(patch.to_string()));
    }

    pub fn set_long_range(&self, on: bool) {
        self.send(Command::SetLongRange(on));
    }

    pub fn factory_reset(&self) {
        self.send(Command::FactoryReset);
    }

    pub fn start_pairing(&self) {
        self.send(Command::StartPairing);
    }

    pub fn dpi_at(&self, position: i32) -> i32 {
        let dpi = dpi_slider::to_dpi(u32::try_from(position).unwrap_or(0));
        i32::try_from(dpi).unwrap_or(i32::MAX)
    }

    fn send(&self, command: Command) {
        if let Some(tx) = &self.commands {
            // Fails only when the worker has exited, which it reports itself.
            let _ = tx.send(command);
        }
    }
}

fn apply(mut device: Pin<&mut qobject::Device>, event: Event) {
    match event {
        Event::State(s) => {
            update!(device, present, present_changed, s.present);
            update!(device, online, online_changed, s.online);
            update!(device, connection, connection_changed, QString::from(s.connection));
            let battery = s.battery_percent.map_or(-1, |l| i32::from(display::battery_percent(l)));
            update!(device, battery_percent, battery_percent_changed, battery);
            update!(device, charging, charging_changed, s.charging);
            let firmware = s.firmware.map(protocol::format_version).unwrap_or_default();
            update!(device, firmware, firmware_changed, QString::from(&firmware));
            let receiver = display::receiver_version(s.connection, s.receiver_firmware, s.firmware);
            let receiver = receiver.map(protocol::format_version).unwrap_or_default();
            update!(device, receiver_firmware, receiver_firmware_changed, QString::from(&receiver));
            update!(device, long_range, long_range_changed, s.long_range.map_or(-1, i32::from));
            let json = s.settings.as_ref().map(|s| settings_map::to_json(s).to_string()).unwrap_or_default();
            update!(device, settings_json, settings_json_changed, QString::from(&json));
            update!(device, loaded, loaded_changed, s.settings.is_some());
        }
        Event::Busy(busy) => update!(device, busy, busy_changed, busy),
        Event::ResetDone => device.as_mut().reset_finished(),
        Event::PairingProgress(poll) => {
            update!(device, pairing, pairing_changed, true);
            update!(device, pairing_progress, pairing_progress_changed, f64::from(poll) / f64::from(PAIRING_POLLS));
        }
        Event::PairingFinished(success) => {
            update!(device, pairing, pairing_changed, false);
            update!(device, pairing_progress, pairing_progress_changed, 0.0);
            device.as_mut().pairing_finished(success);
        }
        Event::Error(message) => {
            let message = QString::from(&message);
            update!(device, last_error, last_error_changed, message.clone());
            device.as_mut().error_occurred(message);
        }
    }
}

impl cxx_qt::Initialize for qobject::Device {
    fn initialize(mut self: Pin<&mut Self>) {
        let (tx, rx) = mpsc::channel();
        let qt = self.qt_thread();
        let cfg = config::get();
        let intervals = worker::Intervals { offline: cfg.device_poll_offline, online: cfg.device_poll_online };
        let spawned = std::thread::Builder::new().name("hecate-device".into()).spawn(move || {
            worker::run(rx, intervals, |event| qt.queue(move |device| apply(device, event)).is_ok());
        });
        match spawned {
            Ok(_) => self.as_mut().rust_mut().commands = Some(tx),
            Err(e) => apply(self.as_mut(), Event::Error(format!("cannot start the device thread: {e}"))),
        }
    }
}
