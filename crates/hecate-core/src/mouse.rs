//! Mouse operations built on the transport, following the vendor library's sequences.

use crate::backup::{self, RestorePlan};
use crate::hidraw::Device;
use crate::protocol::{self as p, Battery, PairingStatus};
use crate::settings::{self, Settings};
use crate::slots::{self, Macro, Shortcut, Slots, SLOTS};
use std::time::{Duration, Instant};

const WAKE_WAIT: Duration = Duration::from_secs(15);
const WAKE_POLL: Duration = Duration::from_millis(300);
/// ClearSetting: one try, 2000 ms (UsbMouse_ClearSetting @0x180012230 called by HECATE FUN_1400f9af0).
const RESET_TIMEOUT: Duration = Duration::from_millis(2000);
/// StartPair: 2 tries, 200 ms (FUN_180015f60). Not resent more often: it restarts pairing.
const START_PAIR_TRIES: u32 = 2;
const START_PAIR_TIMEOUT: Duration = Duration::from_millis(200);
/// The vendor polls the pairing status every 200 ms and gives up after 100 polls (FUN_1400f87c0).
pub const PAIRING_POLL_INTERVAL: Duration = Duration::from_millis(200);
pub const PAIRING_POLLS: u32 = 100;

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

    /// `08 17`: long-distance radio mode (UsbMouse_GetLongRangeMode @0x1800128f0). Not a flash field.
    /// The vendor queries it only while the mouse is online.
    pub fn long_range(&mut self) -> Result<bool, String> {
        let resp = self.dev.transact(&p::build(p::CMD_GET_LONG_RANGE, &[])?)?;
        Ok(resp[p::DATA_OFFSET] != 0)
    }

    /// `08 16 .. 0A on`, applied immediately. Fails unless the reply echoes the requested mode
    /// (UsbMouse_SetLongRangeMode @0x180012730).
    pub fn set_long_range(&mut self, on: bool) -> Result<(), String> {
        let resp = self.dev.transact(&p::set_long_range_request(on))?;
        match resp[p::DATA_OFFSET] {
            v if v == u8::from(on) => Ok(()),
            v => Err(format!("long-range mode: the mouse answered {v} instead of {}", u8::from(on))),
        }
    }

    /// `08 09` ClearSetting: factory reset of settings, keys and macros, sent once like the vendor.
    /// Everything read before is stale afterwards.
    pub fn factory_reset(&mut self) -> Result<(), String> {
        self.dev.transact_with(&p::build(p::CMD_CLEAR_SETTING, &[])?, 1, RESET_TIMEOUT)?;
        Ok(())
    }

    /// `08 05 .. 02` StartPair: puts the receiver into pairing mode. The vendor ignores the reply
    /// (FUN_1400fc730) and goes straight to polling `pairing_status`.
    pub fn start_pairing(&mut self) -> Result<(), String> {
        self.dev.transact_with(&p::start_pair_request(), START_PAIR_TRIES, START_PAIR_TIMEOUT)?;
        Ok(())
    }

    /// `08 06` GetPairingStatus (FUN_180016000). The vendor counts an unanswered poll as failed.
    pub fn pairing_status(&mut self) -> Result<PairingStatus, String> {
        let resp = self.dev.transact(&p::build(p::CMD_PAIRING_STATUS, &[])?)?;
        Ok(p::parse_pairing_status(&resp))
    }

    /// All 16 shortcut and 16 macro slots, in the vendor's read order (FUN_180013a20): every size
    /// probe first, then the bodies of the slots in use. Unused or corrupt slots are None.
    pub fn read_slots(&mut self) -> Result<Slots, String> {
        self.require_online()?;
        let mut shortcut_len = [0; SLOTS];
        for (i, len) in shortcut_len.iter_mut().enumerate() {
            *len = slots::shortcut_len(self.read_flash(slots::shortcut_addr(i)?, 1)?[0]);
        }
        let mut macro_len = [0; SLOTS];
        for (j, len) in macro_len.iter_mut().enumerate() {
            *len = slots::macro_len(self.read_flash(slots::macro_addr(j)? + slots::MACRO_COUNT_OFFSET, 1)?[0]);
        }
        let mut out = Slots::default();
        for (i, &len) in shortcut_len.iter().enumerate().filter(|(_, len)| **len > 0) {
            out.shortcuts[i] = Shortcut::decode(&self.read_flash(slots::shortcut_addr(i)?, len)?);
        }
        for (j, &len) in macro_len.iter().enumerate().filter(|(_, len)| **len > 0) {
            out.macros[j] = Macro::decode(&self.read_flash(slots::macro_addr(j)?, len)?);
        }
        Ok(out)
    }

    /// Writes shortcut slot `i` whole (FUN_180006e60 format), then reads it back and compares.
    /// Keys[i] must have type 5 for the mouse to play it.
    pub fn write_shortcut(&mut self, i: usize, s: &Shortcut) -> Result<(), String> {
        let addr = slots::shortcut_addr(i)?;
        let bytes = s.encode()?;
        self.write_verified(addr, &bytes)
    }

    /// Writes macro slot `j` whole (FUN_180006a70 format), then reads it back and compares.
    /// Keys[j] must be `(6, j, loop mode)` for the mouse to play it.
    pub fn write_macro(&mut self, j: usize, m: &Macro) -> Result<(), String> {
        let addr = slots::macro_addr(j)?;
        let bytes = m.encode()?;
        self.write_verified(addr, &bytes)
    }

    /// Raw flash 0x0000..0x1B00: the settings block, then the whole shortcut area, then the whole macro
    /// area, in the vendor's area order (FUN_180013a20) but including unused slot bytes.
    pub fn read_image(&mut self) -> Result<Vec<u8>, String> {
        self.require_online()?;
        let areas = [(0, p::CONFIG_BLOCK_LEN), (0x100, 0x200), (0x300, slots::USED_FLASH_LEN - 0x300)];
        let mut image = Vec::with_capacity(slots::USED_FLASH_LEN);
        for (addr, len) in areas {
            image.extend(self.read_flash(addr, len)?);
        }
        Ok(image)
    }

    /// Sends a restore plan in order, reading back each write before the next. Every write is checked
    /// against the vendor's group and slot shapes first, so nothing outside them is ever sent.
    pub fn restore(&mut self, plan: &RestorePlan) -> Result<(), String> {
        backup::check_writes(&plan.writes)?;
        for w in &plan.writes {
            self.write_verified(w.addr, &w.bytes)?;
        }
        Ok(())
    }

    fn write_verified(&mut self, addr: u16, bytes: &[u8]) -> Result<(), String> {
        self.require_online()?;
        self.write_flash(addr, bytes)?;
        let back = self.read_flash(addr, bytes.len())?;
        compare_readback(addr, bytes, &back)
    }
}

fn compare_readback(addr: u16, want: &[u8], got: &[u8]) -> Result<(), String> {
    if got.len() != want.len() {
        return Err(format!("read-back at 0x{addr:04x} returned {} bytes instead of {}", got.len(), want.len()));
    }
    match got.iter().zip(want).position(|(g, w)| g != w) {
        None => Ok(()),
        Some(k) => Err(format!(
            "write at 0x{addr:04x} was acknowledged but byte {k} reads back as {:02x} instead of {:02x}",
            got[k], want[k]
        )),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readback_comparison() {
        assert_eq!(compare_readback(0x100, &[1, 2, 3], &[1, 2, 3]), Ok(()));
        assert_eq!(compare_readback(0x100, &[], &[]), Ok(()));
        let err = compare_readback(0x300, &[1, 2, 3], &[1, 9, 3]).unwrap_err();
        assert!(err.contains("0x0300") && err.contains("byte 1") && err.contains("09 instead of 02"), "{err}");
        assert!(compare_readback(0x300, &[1, 2, 3], &[1, 2, 4]).unwrap_err().contains("byte 2"));
        assert!(compare_readback(0x300, &[1, 2, 3], &[1, 2]).unwrap_err().contains("2 bytes instead of 3"));
    }

    #[test]
    fn describes_rates() {
        assert_eq!(describe_rate(Some(0x01)), "1000 Hz");
        assert_eq!(describe_rate(Some(0x40)), "8000 Hz");
        assert_eq!(describe_rate(Some(0x03)), "unknown code 0x03");
        assert!(describe_rate(None).starts_with("invalid"));
    }
}
