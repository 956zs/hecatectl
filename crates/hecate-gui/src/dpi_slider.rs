//! The vendor's "Adjustable parameters" DPI slider for this model (docs/ui-features.md §7.2):
//! positions 0..=499 over 50..=32000 DPI.

use hecate_core::settings::{DPI_MAX, DPI_MIN};

pub const MAX_POSITION: u32 = 499;
/// Register steps of the 50-DPI range (50..=6000), two slider positions each.
const FINE_STEPS: u32 = 120;
/// Last DPI stored as DPI/50 - 1; above it the register holds DPI/100.
const LOW_RANGE_MAX: u32 = 26000;

/// Slider position -> DPI (HECATE.exe FUN_1402367d0, max 32000, sensor 3395).
pub fn to_dpi(position: u32) -> u32 {
    match position.min(MAX_POSITION) {
        p @ 0..=239 => (p / 2 + 1) * 50,
        p @ 240..=439 => 100 * p - 17900,
        p => (p - 179) * 100,
    }
}

/// DPI -> slider position, the way the vendor turns a typed value into one (FUN_140236030):
/// clamped to 50..=32000, then truncated to the step at or below it.
pub fn position(dpi: u32) -> u32 {
    let v = dpi.clamp(DPI_MIN, DPI_MAX);
    if v <= LOW_RANGE_MAX {
        let q = (v - 50) / 50;
        if q < FINE_STEPS {
            q * 2
        } else {
            (q - 119) / 2 + 239
        }
    } else {
        v / 100 + 179
    }
}

/// A typed DPI as the vendor stores it: snapped through the slider position.
pub fn snap(dpi: u32) -> u32 {
    to_dpi(position(dpi))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hecate_core::settings::DpiValue;

    /// Transcription of the register bytes FUN_1402367d0 stores for a slider position.
    fn vendor_register(p: u32) -> (u8, u8, u8) {
        if p < 440 {
            let v = if p < 240 { p / 2 } else { p * 2 - 359 };
            let hi = (v >> 8) as u8;
            (v as u8, v as u8, hi << 2 | hi << 6)
        } else {
            let x = (p + 0x4d) as u8;
            let hi = ((p - 0xb3) >> 8) as u8;
            (x, x, hi << 2 | hi << 6 | 0x22)
        }
    }

    #[test]
    fn every_position_encodes_like_the_vendor() {
        for p in 0..=MAX_POSITION {
            let dpi = to_dpi(p);
            let reg = DpiValue::from_dpi(dpi).unwrap_or_else(|e| panic!("position {p}: {e}"));
            assert_eq!((reg.x, reg.y, reg.ex), vendor_register(p), "position {p} ({dpi} DPI)");
            assert_eq!(reg.dpi(), dpi, "position {p}");
        }
    }

    #[test]
    fn every_position_round_trips() {
        for p in 0..=MAX_POSITION {
            // Below 6000 two positions share a step; the vendor maps the value to the even one.
            let expected = if p < 240 { p & !1 } else { p };
            assert_eq!(position(to_dpi(p)), expected, "position {p}");
        }
    }

    #[test]
    fn range_ends_and_seams() {
        assert_eq!(to_dpi(0), 50);
        assert_eq!(to_dpi(1), 50);
        assert_eq!(to_dpi(238), 6000);
        assert_eq!(to_dpi(239), 6000);
        assert_eq!(to_dpi(240), 6100);
        assert_eq!(to_dpi(439), 26000);
        assert_eq!(to_dpi(440), 26100);
        assert_eq!(to_dpi(499), 32000);
        assert_eq!(to_dpi(10_000), 32000);
    }

    #[test]
    fn dpi_is_monotonic_and_covers_every_selectable_value() {
        let values: Vec<u32> = (0..=MAX_POSITION).map(to_dpi).collect();
        assert!(values.windows(2).all(|w| w[0] <= w[1]));
        for dpi in (50..=6000).step_by(50).chain((6100..=32000).step_by(100)) {
            assert!(values.contains(&dpi), "{dpi} DPI not reachable");
            assert_eq!(snap(dpi), dpi);
        }
    }

    #[test]
    fn typed_values_snap_down_and_clamp() {
        assert_eq!(snap(6050), 6000);
        assert_eq!(snap(123), 100);
        assert_eq!(snap(99), 50);
        assert_eq!(snap(26050), 26000);
        assert_eq!(snap(26199), 26100);
        assert_eq!(snap(0), 50);
        assert_eq!(snap(u32::MAX), 32000);
    }
}
