//! Mouse operations built on the transport, following the vendor library's sequences.

use crate::hidraw::Device;
use crate::protocol::{self as p, Battery};
use crate::settings::{self, Settings};
use std::time::{Duration, Instant};

const WAKE_WAIT: Duration = Duration::from_secs(15);
const WAKE_POLL: Duration = Duration::from_millis(300);

pub struct Mouse {
    dev: Device,
}

impl Mouse {
    pub fn new(dev: Device) -> Self {
        Self { dev }
    }

    /// `08 03`: whether the mouse is connected (through the receiver or the cable).
    pub fn online(&mut self) -> Result<bool, String> {
        let resp = self.dev.transact(&p::build(p::CMD_ONLINE, &[])?)?;
        Ok(resp[p::DATA_OFFSET] != 0)
    }

    /// Waits up to WAKE_WAIT for a sleeping mouse to come online.
    pub fn require_online(&mut self) -> Result<(), String> {
        if self.online()? {
            return Ok(());
        }
        eprintln!("mouse is asleep, move it to wake it up...");
        let deadline = Instant::now() + WAKE_WAIT;
        while Instant::now() < deadline {
            std::thread::sleep(WAKE_POLL);
            if self.online()? {
                return Ok(());
            }
        }
        Err(format!(
            "the receiver answers but the mouse stayed offline for {} s (is it switched on and in 2.4G mode?)",
            WAKE_WAIT.as_secs()
        ))
    }

    pub fn battery(&mut self) -> Result<Battery, String> {
        let resp = self.dev.transact(&p::build(p::CMD_BATTERY, &[])?)?;
        Ok(p::parse_battery(&resp))
    }

    pub fn version(&mut self) -> Result<u16, String> {
        let resp = self.dev.transact(&p::build(p::CMD_VERSION, &[])?)?;
        Ok(p::parse_version(&resp))
    }

    /// Reads `len` bytes at `addr`. Unlike the vendor loader, any failed chunk is an error.
    pub fn read_flash(&mut self, addr: u16, len: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let at = addr as usize + out.len();
            let at = u16::try_from(at).map_err(|_| format!("read past address 0x{:x}", u16::MAX))?;
            let resp = self
                .dev
                .transact(&p::flash_read_request(at))
                .map_err(|e| format!("flash read at 0x{at:04x}: {e}"))?;
            let take = p::FLASH_CHUNK.min(len - out.len());
            out.extend_from_slice(&resp[p::DATA_OFFSET..p::DATA_OFFSET + take]);
        }
        Ok(out)
    }

    pub fn write_flash(&mut self, addr: u16, data: &[u8]) -> Result<(), String> {
        for req in p::flash_write_requests(addr, data)? {
            let at = u16::from_be_bytes([req[3], req[4]]);
            self.dev.transact(&req).map_err(|e| format!("flash write at 0x{at:04x}: {e}"))?;
        }
        Ok(())
    }

    /// Raw report-rate code stored at 0x0000, or None when the stored check byte is invalid.
    pub fn report_rate_code(&mut self) -> Result<Option<u8>, String> {
        let raw = self.read_flash(p::ADDR_REPORT_RATE, 2)?;
        Ok(p::unguard([raw[0], raw[1]]))
    }

    /// Sets the report rate: online check, write `[code, 0x55-code]` at 0x0000, read back.
    /// Returns false when the mouse already had that rate (nothing written).
    pub fn set_report_rate(&mut self, hz: u32) -> Result<bool, String> {
        let code = p::rate_code(hz).ok_or_else(|| {
            let allowed: Vec<String> = p::SETTABLE_RATES.iter().map(|(r, _)| r.to_string()).collect();
            format!("unsupported rate {hz} Hz (allowed: {})", allowed.join(", "))
        })?;
        self.require_online()?;
        if self.report_rate_code()? == Some(code) {
            return Ok(false);
        }
        self.write_flash(p::ADDR_REPORT_RATE, &p::guarded(code))?;
        match self.report_rate_code()? {
            Some(c) if c == code => Ok(true),
            other => Err(format!(
                "write was acknowledged but read-back shows {} instead of {hz} Hz",
                describe_rate(other)
            )),
        }
    }

    /// Online check, then the whole settings block (flash 0x000-0x0FF), decoded.
    pub fn read_settings(&mut self) -> Result<Settings, String> {
        self.require_online()?;
        let raw = self.read_flash(0, p::CONFIG_BLOCK_LEN)?;
        let block: settings::Block = raw.try_into().map_err(|_| "short settings read".to_string())?;
        Ok(settings::decode(&block))
    }

    /// Writes the groups that differ between `old` (as read from the mouse) and `new`, the way the
    /// vendor save path does (FUN_180006630), then re-reads and returns the fresh settings.
    /// Fails when a written group does not read back as written. After any error the flash state is
    /// unknown: re-read before the next apply.
    pub fn apply(&mut self, old: &Settings, new: &Settings) -> Result<Settings, String> {
        settings::validate(old, new)?;
        let writes = settings::diff(old, new);
        if writes.is_empty() {
            return Ok(old.clone());
        }
        self.require_online()?;
        for w in &writes {
            self.write_flash(w.addr, &w.bytes)?;
        }
        let fresh = self.read_settings()?;
        settings::verify(&writes, fresh.raw())?;
        Ok(fresh)
    }
}

pub fn describe_rate(code: Option<u8>) -> String {
    match code {
        Some(c) => match p::rate_hz(c) {
            Some(hz) => format!("{hz} Hz"),
            None => format!("unknown code 0x{c:02x}"),
        },
        None => "invalid (check byte mismatch, firmware uses its default)".into(),
    }
}
