//! Packet framing used by the HECATE vendor library (HIDUsb.dll).
//!
//! Every request and response is a 17-byte HID report on report ID 0x08:
//! `[0x08, cmd, b2 .. b15, checksum]`, where `checksum = 0x55 - sum(bytes[0..16])`.
//! A response belongs to a request when its report ID and command byte match
//! and its checksum is valid.

pub const REPORT_ID: u8 = 0x08;
pub const PACKET_LEN: usize = 17;
const CHECKSUM_BASE: u8 = 0x55;

pub const CMD_ONLINE: u8 = 0x03;
pub const CMD_BATTERY: u8 = 0x04;
pub const CMD_START_PAIR: u8 = 0x05;
pub const CMD_PAIRING_STATUS: u8 = 0x06;
pub const CMD_FLASH_WRITE: u8 = 0x07;
pub const CMD_FLASH_READ: u8 = 0x08;
pub const CMD_CLEAR_SETTING: u8 = 0x09;
pub const CMD_VERSION: u8 = 0x12;
pub const CMD_SET_LONG_RANGE: u8 = 0x16;
pub const CMD_GET_LONG_RANGE: u8 = 0x17;

/// Data bytes carried by one flash read/write packet (resp[6..16] / req[6..16]).
pub const FLASH_CHUNK: usize = 10;
/// Response offset of the first data byte.
pub const DATA_OFFSET: usize = 6;

/// Flash address of the report-rate field (`[code, 0x55 - code]`).
pub const ADDR_REPORT_RATE: u16 = 0x0000;
/// Size of the settings block at the start of flash.
pub const CONFIG_BLOCK_LEN: usize = 0x100;

/// Report-rate codes (HECATE.exe FUN_1402374d0). The vendor app offers only the
/// first four for the G3M PRO updated version, so only those may be written.
pub const SETTABLE_RATES: [(u32, u8); 4] = [(125, 0x08), (250, 0x04), (500, 0x02), (1000, 0x01)];
const OTHER_RATES: [(u32, u8); 3] = [(2000, 0x10), (4000, 0x20), (8000, 0x40)];

pub type Packet = [u8; PACKET_LEN];

/// `0x55 - sum(bytes)` with wrapping byte arithmetic.
pub fn checksum(bytes: &[u8]) -> u8 {
    let sum = bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
    CHECKSUM_BASE.wrapping_sub(sum)
}

/// Builds a request: report ID, command, `body` copied to bytes 2.., checksum last.
/// `body` longer than the 14 payload bytes is an error.
pub fn build(cmd: u8, body: &[u8]) -> Result<Packet, String> {
    let max = PACKET_LEN - 3;
    if body.len() > max {
        return Err(format!("payload is {} bytes, max is {max}", body.len()));
    }
    let mut p = [0u8; PACKET_LEN];
    p[0] = REPORT_ID;
    p[1] = cmd;
    p[2..2 + body.len()].copy_from_slice(body);
    p[PACKET_LEN - 1] = checksum(&p[..PACKET_LEN - 1]);
    Ok(p)
}

/// True when `resp` is a well-formed answer to `req`.
pub fn is_response_to(req: &Packet, resp: &[u8]) -> bool {
    resp.len() == PACKET_LEN
        && resp[0] == req[0]
        && resp[1] == req[1]
        && resp[PACKET_LEN - 1] == checksum(&resp[..PACKET_LEN - 1])
}

/// Read request for the 10 bytes at `addr`: `08 08 00 AH AL 0A` (FUN_180013880).
pub fn flash_read_request(addr: u16) -> Packet {
    let [hi, lo] = addr.to_be_bytes();
    build(CMD_FLASH_READ, &[0, hi, lo, FLASH_CHUNK as u8]).expect("fixed-size body")
}

/// Write requests for `data` at `addr`, split into 10-byte packets:
/// `08 07 00 AH AL len data..` (FUN_18001e700).
pub fn flash_write_requests(addr: u16, data: &[u8]) -> Result<Vec<Packet>, String> {
    if data.is_empty() {
        return Err("nothing to write".into());
    }
    let end = addr as usize + data.len();
    if end > u16::MAX as usize + 1 {
        return Err(format!("write of {} bytes at 0x{addr:04x} runs past the address space", data.len()));
    }
    data.chunks(FLASH_CHUNK)
        .enumerate()
        .map(|(i, chunk)| {
            let [hi, lo] = (addr + (i * FLASH_CHUNK) as u16).to_be_bytes();
            let mut body = vec![0, hi, lo, chunk.len() as u8];
            body.extend_from_slice(chunk);
            build(CMD_FLASH_WRITE, &body)
        })
        .collect()
}

/// A one-byte setting as stored in flash: `[value, 0x55 - value]`.
pub fn guarded(value: u8) -> [u8; 2] {
    [value, checksum(&[value])]
}

/// The value of a stored `[value, check]` pair, or None when the check byte is wrong
/// (the vendor parser then falls back to its default, FUN_180005840).
pub fn unguard(pair: [u8; 2]) -> Option<u8> {
    (pair[1] == checksum(&pair[..1])).then_some(pair[0])
}

pub fn rate_code(hz: u32) -> Option<u8> {
    SETTABLE_RATES.iter().find(|(r, _)| *r == hz).map(|(_, c)| *c)
}

pub fn rate_hz(code: u8) -> Option<u32> {
    SETTABLE_RATES.iter().chain(OTHER_RATES.iter()).find(|(_, c)| *c == code).map(|(r, _)| *r)
}

#[derive(Debug, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
    pub millivolts: u16,
}

/// Battery response: `[6]` level %, `[7]` charging flag, `[8..10]` mV big-endian (FUN_180015e90).
pub fn parse_battery(resp: &Packet) -> Battery {
    Battery {
        percent: resp[6],
        charging: resp[7] != 0,
        millivolts: u16::from_be_bytes([resp[8], resp[9]]),
    }
}

/// Firmware version: `[6] << 8 | [7]` (FUN_1800160c0).
pub fn parse_version(resp: &Packet) -> u16 {
    u16::from_be_bytes([resp[6], resp[7]])
}

/// A version as the vendor shows it: `v{hi}.0{lo}` when lo < 16, else `v{hi}.{lo}`, both decimal
/// (HECATE.exe FUN_140243840). Used for the mouse (cmd 0x12) and the receiver (USB bcdDevice).
pub fn format_version(v: u16) -> String {
    let [hi, lo] = v.to_be_bytes();
    if lo < 16 {
        format!("v{hi}.0{lo}")
    } else {
        format!("v{hi}.{lo}")
    }
}

/// `08 16 00 00 00 0A on` (UsbMouse_SetLongRangeMode @0x180012730). The reply must echo `on` at [6].
pub fn set_long_range_request(on: bool) -> Packet {
    build(CMD_SET_LONG_RANGE, &[0, 0, 0, 0x0A, u8::from(on)]).expect("fixed-size body")
}

/// `08 05 00 00 00 02`: puts the receiver into pairing mode (FUN_180015f60).
pub fn start_pair_request() -> Packet {
    build(CMD_START_PAIR, &[0, 0, 0, 0x02]).expect("fixed-size body")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingStatus {
    Paired,
    Failed,
    /// Any other status byte: still pairing.
    InProgress(u8),
}

/// `08 06` reply byte [6]: 3 = paired, 0 or 2 = failed, anything else = in progress (HECATE FUN_1400f87c0).
pub fn parse_pairing_status(resp: &Packet) -> PairingStatus {
    match resp[DATA_OFFSET] {
        3 => PairingStatus::Paired,
        0 | 2 => PairingStatus::Failed,
        s => PairingStatus::InProgress(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_of_empty_is_base() {
        assert_eq!(checksum(&[]), 0x55);
    }

    #[test]
    fn checksum_wraps() {
        // 0x08 + 0x04 = 0x0c -> 0x55 - 0x0c = 0x49
        assert_eq!(checksum(&[0x08, 0x04]), 0x49);
        // sum 0x1fe wraps to 0xfe -> 0x55 - 0xfe = 0x57
        assert_eq!(checksum(&[0xff, 0xff]), 0x57);
    }

    #[test]
    fn build_places_fields() {
        let p = build(0x12, &[]).unwrap();
        assert_eq!(p[0], REPORT_ID);
        assert_eq!(p[1], 0x12);
        assert!(p[2..16].iter().all(|b| *b == 0));
        assert_eq!(p[16], 0x55u8.wrapping_sub(0x08 + 0x12));
    }

    #[test]
    fn build_copies_full_body() {
        let body: Vec<u8> = (1..=14).collect();
        let p = build(0x08, &body).unwrap();
        assert_eq!(&p[2..16], &body[..]);
        assert_eq!(p[16], checksum(&p[..16]));
    }

    #[test]
    fn build_rejects_oversized_body() {
        assert!(build(0x08, &[0u8; 15]).is_err());
    }

    #[test]
    fn response_matching() {
        let req = build(0x04, &[]).unwrap();
        let mut resp = build(0x04, &[0, 0, 0, 0, 87, 0, 0x0f, 0xa0]).unwrap();
        assert!(is_response_to(&req, &resp));
        resp[16] ^= 1; // bad checksum
        assert!(!is_response_to(&req, &resp));
        let other = build(0x03, &[]).unwrap(); // different command
        assert!(!is_response_to(&req, &other));
        assert!(!is_response_to(&req, &resp[..16])); // short read
    }

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    #[test]
    fn known_request_packets_match_spec() {
        // docs/protocol.md "Ready-made request packets"
        assert_eq!(build(CMD_ONLINE, &[]).unwrap().to_vec(), hex("08 03 00 00 00 00 00 00 00 00 00 00 00 00 00 00 4A"));
        assert_eq!(build(CMD_BATTERY, &[]).unwrap().to_vec(), hex("08 04 00 00 00 00 00 00 00 00 00 00 00 00 00 00 49"));
        assert_eq!(build(CMD_VERSION, &[]).unwrap().to_vec(), hex("08 12 00 00 00 00 00 00 00 00 00 00 00 00 00 00 3B"));
    }

    #[test]
    fn flash_read_request_layout() {
        assert_eq!(flash_read_request(0).to_vec(), hex("08 08 00 00 00 0A 00 00 00 00 00 00 00 00 00 00 3B"));
        let p = flash_read_request(0x01a4);
        assert_eq!(&p[..6], &[0x08, 0x08, 0x00, 0x01, 0xa4, 0x0a]);
        assert_eq!(p[16], checksum(&p[..16]));
    }

    #[test]
    fn rate_write_packet_matches_vendor() {
        // 1000 Hz: the exact packet the vendor save path sends for this change.
        let pkts = flash_write_requests(ADDR_REPORT_RATE, &guarded(0x01)).unwrap();
        assert_eq!(pkts.len(), 1);
        assert_eq!(pkts[0].to_vec(), hex("08 07 00 00 00 02 01 54 00 00 00 00 00 00 00 00 EF"));
        // The packet checksum is the same for every rate because value + check is constant.
        for (_, code) in SETTABLE_RATES {
            let p = flash_write_requests(ADDR_REPORT_RATE, &guarded(code)).unwrap();
            assert_eq!(p[0][16], 0xEF);
        }
    }

    #[test]
    fn flash_write_chunks_and_addresses() {
        let data: Vec<u8> = (0..23).collect();
        let pkts = flash_write_requests(0x00fa, &data).unwrap();
        assert_eq!(pkts.len(), 3);
        assert_eq!(&pkts[0][..6], &[0x08, 0x07, 0x00, 0x00, 0xfa, 10]);
        assert_eq!(&pkts[1][..6], &[0x08, 0x07, 0x00, 0x01, 0x04, 10]);
        assert_eq!(&pkts[2][..6], &[0x08, 0x07, 0x00, 0x01, 0x0e, 3]);
        assert_eq!(&pkts[2][6..9], &[20, 21, 22]);
        assert!(pkts[2][9..16].iter().all(|b| *b == 0));
        for p in &pkts {
            assert_eq!(p[16], checksum(&p[..16]));
        }
    }

    #[test]
    fn flash_write_rejects_empty_and_overflow() {
        assert!(flash_write_requests(0, &[]).is_err());
        assert!(flash_write_requests(0xffff, &[1, 2]).is_err());
        assert!(flash_write_requests(0xffff, &[1]).is_ok());
    }

    #[test]
    fn guard_round_trip() {
        for v in [0x00, 0x01, 0x08, 0x55, 0x56, 0xff] {
            assert_eq!(unguard(guarded(v)), Some(v));
        }
        assert_eq!(guarded(0x08), [0x08, 0x4d]);
        assert_eq!(unguard([0x01, 0x00]), None);
        assert_eq!(unguard([0xff, 0xff]), None); // erased flash
    }

    #[test]
    fn rate_mapping() {
        assert_eq!(rate_code(1000), Some(0x01));
        assert_eq!(rate_code(125), Some(0x08));
        assert_eq!(rate_code(2000), None); // not offered for this model
        assert_eq!(rate_code(0), None);
        assert_eq!(rate_hz(0x02), Some(500));
        assert_eq!(rate_hz(0x40), Some(8000));
        assert_eq!(rate_hz(0x00), None);
        assert_eq!(rate_hz(0x03), None);
    }

    #[test]
    fn parses_battery_and_version() {
        let resp = build(CMD_BATTERY, &[0, 0, 0, 0, 87, 1, 0x0f, 0xa0]).unwrap();
        assert_eq!(parse_battery(&resp), Battery { percent: 87, charging: true, millivolts: 4000 });
        let empty = build(CMD_BATTERY, &[]).unwrap();
        assert_eq!(parse_battery(&empty), Battery { percent: 0, charging: false, millivolts: 0 });
        let ver = build(CMD_VERSION, &[0, 0, 0, 0, 0x01, 0x23]).unwrap();
        assert_eq!(parse_version(&ver), 0x0123);
    }

    #[test]
    fn formats_versions_like_the_vendor() {
        assert_eq!(format_version(0x0313), "v3.19"); // live mouse firmware
        assert_eq!(format_version(0x0102), "v1.02"); // live receiver bcdDevice
        assert_eq!(format_version(0x0000), "v0.00");
        assert_eq!(format_version(0x030F), "v3.015"); // lo < 16 gets the "0" even when it has two digits
        assert_eq!(format_version(0x0310), "v3.16");
        assert_eq!(format_version(0xFFFF), "v255.255");
    }

    #[test]
    fn device_action_packets_match_vendor() {
        let mut on = hex("08 16 00 00 00 0A 01 00 00 00 00 00 00 00 00 00 00");
        on[16] = checksum(&on[..16]);
        assert_eq!(set_long_range_request(true).to_vec(), on);
        assert_eq!(set_long_range_request(false)[6], 0);
        assert_eq!(set_long_range_request(false)[16], checksum(&set_long_range_request(false)[..16]));
        assert_eq!(start_pair_request()[..7], [0x08, 0x05, 0, 0, 0, 0x02, 0]);
        assert_eq!(start_pair_request()[16], 0x55u8.wrapping_sub(0x08 + 0x05 + 0x02));
        // ready-made packets in docs/protocol.md §6
        let get_long_range = hex("08 17 00 00 00 00 00 00 00 00 00 00 00 00 00 00 36");
        assert_eq!(build(CMD_GET_LONG_RANGE, &[]).unwrap().to_vec(), get_long_range);
        let pairing_status = hex("08 06 00 00 00 00 00 00 00 00 00 00 00 00 00 00 47");
        assert_eq!(build(CMD_PAIRING_STATUS, &[]).unwrap().to_vec(), pairing_status);
        assert_eq!(build(CMD_CLEAR_SETTING, &[]).unwrap()[..2], [0x08, 0x09]);
    }

    #[test]
    fn parses_pairing_status() {
        let status = |s: u8| parse_pairing_status(&build(CMD_PAIRING_STATUS, &[0, 0, 0, 0, s]).unwrap());
        assert_eq!(status(3), PairingStatus::Paired);
        assert_eq!(status(0), PairingStatus::Failed);
        assert_eq!(status(2), PairingStatus::Failed);
        assert_eq!(status(1), PairingStatus::InProgress(1));
        assert_eq!(status(4), PairingStatus::InProgress(4));
        assert_eq!(status(0xFF), PairingStatus::InProgress(0xFF));
    }
}
