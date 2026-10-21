//! `Device` QML singleton: mouse state for the UI. All USB I/O runs on the worker thread in
//! `crate::worker`; this object only sends it commands and applies the events it queues back.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
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
        #[qproperty(i32, report_rate, READ, NOTIFY)]
        #[qproperty(QList_i32, report_rates, READ, CONSTANT)]
        #[qproperty(bool, busy, READ, NOTIFY)]
        #[qproperty(QString, last_error, READ, NOTIFY)]
        type Device = super::DeviceRust;

        /// Re-reads everything from the mouse.
        #[qinvokable]
        fn refresh(&self);

        /// Writes the report rate in Hz, then reads the settings back.
        #[qinvokable]
        fn set_report_rate(&self, hz: i32);

        /// Emitted for every error, also when the message repeats.
        #[qsignal]
        fn error_occurred(self: Pin<&mut Self>, message: QString);
    }

    impl cxx_qt::Threading for Device {}
    impl cxx_qt::Initialize for Device {}
}

use super::update;
use crate::config;
use crate::worker::{self, Command, Event};
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QList, QString};
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
    /// 0 while unknown.
    report_rate: i32,
    report_rates: QList<i32>,
    busy: bool,
    last_error: QString,
    commands: Option<Sender<Command>>,
}

impl Default for DeviceRust {
    fn default() -> Self {
        let mut report_rates = QList::default();
        for (hz, _) in protocol::SETTABLE_RATES {
            report_rates.append(hz as i32);
        }
        Self {
            present: false,
            online: false,
            connection: QString::default(),
            battery_percent: -1,
            charging: false,
            firmware: QString::default(),
            report_rate: 0,
            report_rates,
            busy: false,
            last_error: QString::default(),
            commands: None,
        }
    }
}

impl qobject::Device {
    pub fn refresh(&self) {
        self.send(Command::Refresh);
    }

    pub fn set_report_rate(&self, hz: i32) {
        match u32::try_from(hz) {
            Ok(hz) => self.send(Command::SetReportRate(hz)),
            Err(_) => eprintln!("hecate-gui: ignoring report rate {hz}"),
        }
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
            update!(device, battery_percent, battery_percent_changed, s.battery_percent.map_or(-1, i32::from));
            update!(device, charging, charging_changed, s.charging);
            let firmware = s.firmware.map(|v| format!("0x{v:04x}")).unwrap_or_default();
            update!(device, firmware, firmware_changed, QString::from(&firmware));
            let rate = s.report_rate.and_then(|hz| i32::try_from(hz).ok()).unwrap_or(0);
            update!(device, report_rate, report_rate_changed, rate);
        }
        Event::Busy(busy) => update!(device, busy, busy_changed, busy),
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
        let interval = config::get().device_poll;
        let spawned = std::thread::Builder::new().name("hecate-device".into()).spawn(move || {
            worker::run(rx, interval, |event| qt.queue(move |device| apply(device, event)).is_ok());
        });
        match spawned {
            Ok(_) => self.as_mut().rust_mut().commands = Some(tx),
            Err(e) => apply(self.as_mut(), Event::Error(format!("cannot start the device thread: {e}"))),
        }
    }
}
