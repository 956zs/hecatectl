//! Display rules copied from the vendor app (docs/ui-features.md §2.3, §6.4). Version strings use
//! `protocol::format_version` (§6.5).

/// Battery level as the vendor shows it: capped at 100, and 0 shown as 5.
pub fn battery_percent(level: u8) -> u8 {
    match level {
        0 => 5,
        l => l.min(100),
    }
}

/// The receiver firmware row: shown for the 2.4G receiver only, and only when its version differs
/// from the mouse's (FUN_140243840).
pub fn receiver_version(connection: &str, receiver: Option<u16>, mouse: Option<u16>) -> Option<u16> {
    receiver.filter(|r| connection == "receiver" && Some(*r) != mouse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_levels() {
        assert_eq!(battery_percent(90), 90);
        assert_eq!(battery_percent(1), 1);
        assert_eq!(battery_percent(100), 100);
    }

    #[test]
    fn battery_zero_and_overflow() {
        assert_eq!(battery_percent(0), 5);
        assert_eq!(battery_percent(101), 100);
        assert_eq!(battery_percent(255), 100);
    }

    #[test]
    fn receiver_row_when_versions_differ() {
        assert_eq!(receiver_version("receiver", Some(0x0102), Some(0x0313)), Some(0x0102));
        assert_eq!(receiver_version("receiver", Some(0x0102), None), Some(0x0102));
    }

    #[test]
    fn no_receiver_row() {
        assert_eq!(receiver_version("receiver", Some(0x0313), Some(0x0313)), None);
        assert_eq!(receiver_version("receiver", None, Some(0x0313)), None);
        assert_eq!(receiver_version("wired", Some(0x0102), Some(0x0313)), None);
        assert_eq!(receiver_version("", Some(0x0102), None), None);
    }
}
