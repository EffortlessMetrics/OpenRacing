//! Fanatec tuning menu protocol: wheelbase parameter read/write.
//!
//! # Wire protocol (verified from gotzl/hid-fanatecff)
//!
//! Fanatec DD1, DD2, CSL DD, and CSL Elite wheelbases expose a tuning
//! menu through 64-byte HID output reports with prefix `[0xFF, 0x03, ...]`.
//!
//! ## Tuning report commands
//!
//! | Byte\[2\] | Purpose |
//! |-----------|---------|
//! | `0x00`    | Write a parameter value at a given address |
//! | `0x01`    | Select tuning slot (1–5) |
//! | `0x04`    | Reset / request current values from device |
//! | `0x06`    | Toggle advanced mode |
//!
//! ## Tuning parameters
//!
//! From `FTEC_TUNING_ATTRS` macro in `hid-ftec.h`:
//!
//! | Name | Addr | Description | Range | Conv |
//! |------|------|-------------|-------|------|
//! | SLOT | 0x02 | Profile slot | 1–5 | noop |
//! | SEN  | 0x03 | Sensitivity (range) | 90–max | special |
//! | FF   | 0x04 | FFB strength | 0–100 | noop |
//! | SHO  | 0x05 | Vibration motor | 0–100 | ×10 |
//! | BLI  | 0x06 | Brake level indicator | 0–101 | noop |
//! | FFS  | 0x07 | FFB scaling | 0–1 | noop |
//! | DRI  | 0x09 | Drift mode | −5–3 | signed |
//! | FOR  | 0x0a | Force effect strength | 0–120 | ×10 |
//! | SPR  | 0x0b | Spring effect strength | 0–120 | ×10 |
//! | DPR  | 0x0c | Damper effect strength | 0–120 | ×10 |
//! | NDP  | 0x0d | Natural damper | 0–100 | noop |
//! | NFR  | 0x0e | Natural friction | 0–100 | noop |
//! | FEI  | 0x11 | Force effect intensity | 0–100 | steps10 |
//! | ACP  | 0x13 | Analogue paddles | 1–4 | noop |
//! | INT  | 0x14 | FFB interpolation filter | 0–20 | noop |
//! | NIN  | 0x15 | Natural inertia | 0–100 | noop |
//! | FUL  | 0x16 | FullForce | 0–100 | noop |
//!
//! ## Device availability
//!
//! Not all parameters are available on all devices:
//! - **All with tuning menu**: SLOT, SEN, FF, FEI, FOR, SPR, DPR, ACP
//! - **CSL Elite / CSL Elite PS4**: + DRI
//! - **CSL Elite + DD bases**: + BLI, SHO
//! - **DD1 / DD2 / CSL DD**: + NDP, NFR, NIN, INT, FFS
//! - **CSL DD only**: + FUL

/// Tuning report size (64 bytes).
pub const TUNING_REPORT_SIZE: usize = 64;

/// Tuning report header byte 0.
pub const TUNING_HEADER_0: u8 = 0xFF;

/// Tuning report header byte 1.
pub const TUNING_HEADER_1: u8 = 0x03;

/// Maximum number of tuning slots.
pub const MAX_SLOTS: u8 = 5;

// ---------------------------------------------------------------------------
// Tuning parameter definitions
// ---------------------------------------------------------------------------

/// A tuning parameter identifier with its HID address and valid range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuningParam {
    /// Short name (e.g. "SEN", "FF").
    pub name: &'static str,
    /// HID address byte in the tuning report.
    pub addr: u8,
    /// Minimum allowed value (user-facing, before conversion).
    pub min: i16,
    /// Maximum allowed value (user-facing, before conversion).
    pub max: i16,
    /// Conversion type for encoding/decoding.
    pub conv: ConversionType,
}

/// Value conversion types matching the kernel driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionType {
    /// No conversion (identity).
    Noop,
    /// Multiply by 10 to display, divide by 10 to encode.
    TimesTen,
    /// Round to nearest multiple of 10.
    StepsTen,
    /// Signed byte interpretation.
    Signed,
    /// Special sensitivity encoding (device-dependent).
    Sensitivity,
}

/// All known tuning parameters.
///
/// Source: `FTEC_TUNING_ATTRS` macro in `hid-ftec.h`.
pub const PARAMS: &[TuningParam] = &[
    TuningParam { name: "SLOT", addr: 0x02, min: 1, max: 5, conv: ConversionType::Noop },
    TuningParam { name: "SEN", addr: 0x03, min: 90, max: 0, conv: ConversionType::Sensitivity },
    TuningParam { name: "FF", addr: 0x04, min: 0, max: 100, conv: ConversionType::Noop },
    TuningParam { name: "SHO", addr: 0x05, min: 0, max: 100, conv: ConversionType::TimesTen },
    TuningParam { name: "BLI", addr: 0x06, min: 0, max: 101, conv: ConversionType::Noop },
    TuningParam { name: "FFS", addr: 0x07, min: 0, max: 1, conv: ConversionType::Noop },
    TuningParam { name: "DRI", addr: 0x09, min: -5, max: 3, conv: ConversionType::Signed },
    TuningParam { name: "FOR", addr: 0x0a, min: 0, max: 120, conv: ConversionType::TimesTen },
    TuningParam { name: "SPR", addr: 0x0b, min: 0, max: 120, conv: ConversionType::TimesTen },
    TuningParam { name: "DPR", addr: 0x0c, min: 0, max: 120, conv: ConversionType::TimesTen },
    TuningParam { name: "NDP", addr: 0x0d, min: 0, max: 100, conv: ConversionType::Noop },
    TuningParam { name: "NFR", addr: 0x0e, min: 0, max: 100, conv: ConversionType::Noop },
    TuningParam { name: "FEI", addr: 0x11, min: 0, max: 100, conv: ConversionType::StepsTen },
    TuningParam { name: "ACP", addr: 0x13, min: 1, max: 4, conv: ConversionType::Noop },
    TuningParam { name: "INT", addr: 0x14, min: 0, max: 20, conv: ConversionType::Noop },
    TuningParam { name: "NIN", addr: 0x15, min: 0, max: 100, conv: ConversionType::Noop },
    TuningParam { name: "FUL", addr: 0x16, min: 0, max: 100, conv: ConversionType::Noop },
];

/// Look up a tuning parameter by name.
pub fn param_by_name(name: &str) -> Option<&'static TuningParam> {
    PARAMS.iter().find(|p| p.name == name)
}

/// Look up a tuning parameter by address.
pub fn param_by_addr(addr: u8) -> Option<&'static TuningParam> {
    PARAMS.iter().find(|p| p.addr == addr)
}

// ---------------------------------------------------------------------------
// Value conversion
// ---------------------------------------------------------------------------

/// Convert a user-facing value to a raw device byte.
///
/// For `TimesTen`: divide by 10.
/// For `StepsTen`: round to nearest 10 then divide by 10.
/// For `Signed`: cast to i8 then to u8.
/// For `Sensitivity`: not implemented here (device-dependent, requires max_range).
/// For `Noop`: identity.
pub fn encode_value(conv: ConversionType, value: i16) -> u8 {
    match conv {
        ConversionType::Noop => value as u8,
        ConversionType::TimesTen => (value / 10) as u8,
        ConversionType::StepsTen => (10 * (value / 10) / 10) as u8,
        ConversionType::Signed => value as i8 as u8,
        ConversionType::Sensitivity => value as u8,
    }
}

/// Convert a raw device byte to a user-facing value.
///
/// For `TimesTen`: multiply by 10.
/// For `StepsTen`: multiply by 10.
/// For `Signed`: interpret as i8.
/// For `Sensitivity`: not implemented here (device-dependent).
/// For `Noop`: identity.
pub fn decode_value(conv: ConversionType, raw: u8) -> i16 {
    match conv {
        ConversionType::Noop => raw as i16,
        ConversionType::TimesTen => (raw as i16) * 10,
        ConversionType::StepsTen => (raw as i16) * 10,
        ConversionType::Signed => (raw as i8) as i16,
        ConversionType::Sensitivity => raw as i16,
    }
}

// ---------------------------------------------------------------------------
// Report encoders
// ---------------------------------------------------------------------------

/// Encode a tuning parameter write command.
///
/// `addr` is the parameter address (e.g. 0x04 for FF).
/// `raw_value` is the already-converted device byte.
///
/// Returns a 64-byte report: `[0xFF, 0x03, 0x00, ..., value at addr+1]`.
///
/// Source: `ftec_tuning_write()` in hid-ftecff-tuning.c.
pub fn encode_write(addr: u8, raw_value: u8) -> [u8; TUNING_REPORT_SIZE] {
    let mut buf = [0u8; TUNING_REPORT_SIZE];
    buf[0] = TUNING_HEADER_0;
    buf[1] = TUNING_HEADER_1;
    buf[2] = 0x00;
    let offset = (addr as usize) + 1;
    if offset < TUNING_REPORT_SIZE {
        buf[offset] = raw_value;
    }
    buf
}

/// Encode a slot-select command.
///
/// `slot` is 1–5.
///
/// Returns a 64-byte report: `[0xFF, 0x03, 0x01, slot, ...]`.
///
/// Source: `ftec_tuning_select()` in hid-ftecff-tuning.c.
pub fn encode_select_slot(slot: u8) -> [u8; TUNING_REPORT_SIZE] {
    let s = slot.clamp(1, MAX_SLOTS);
    let mut buf = [0u8; TUNING_REPORT_SIZE];
    buf[0] = TUNING_HEADER_0;
    buf[1] = TUNING_HEADER_1;
    buf[2] = 0x01;
    buf[3] = s;
    buf
}

/// Encode a reset / request-current-values command.
///
/// Returns a 64-byte report: `[0xFF, 0x03, 0x04, ...]`.
///
/// Source: `ftec_tuning_reset()` in hid-ftecff-tuning.c.
pub fn encode_reset() -> [u8; TUNING_REPORT_SIZE] {
    let mut buf = [0u8; TUNING_REPORT_SIZE];
    buf[0] = TUNING_HEADER_0;
    buf[1] = TUNING_HEADER_1;
    buf[2] = 0x04;
    buf
}

/// Encode an advanced-mode toggle command.
///
/// Returns a 64-byte report: `[0xFF, 0x03, 0x06, ...]`.
///
/// Source: `ftec_tuning_advanced_mode_store()` in hid-ftecff-tuning.c.
pub fn encode_toggle_advanced_mode() -> [u8; TUNING_REPORT_SIZE] {
    let mut buf = [0u8; TUNING_REPORT_SIZE];
    buf[0] = TUNING_HEADER_0;
    buf[1] = TUNING_HEADER_1;
    buf[2] = 0x06;
    buf
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Parameter lookup
    // -----------------------------------------------------------------------

    #[test]
    fn param_count() {
        assert_eq!(PARAMS.len(), 17);
    }

    #[test]
    fn lookup_by_name_ff() {
        let p = param_by_name("FF");
        assert!(p.is_some());
        if let Some(p) = p {
            assert_eq!(p.addr, 0x04);
            assert_eq!(p.min, 0);
            assert_eq!(p.max, 100);
        }
    }

    #[test]
    fn lookup_by_name_not_found() {
        assert!(param_by_name("DOES_NOT_EXIST").is_none());
    }

    #[test]
    fn lookup_by_addr_slot() {
        let p = param_by_addr(0x02);
        assert!(p.is_some());
        if let Some(p) = p {
            assert_eq!(p.name, "SLOT");
        }
    }

    #[test]
    fn lookup_by_addr_not_found() {
        assert!(param_by_addr(0x99).is_none());
    }

    #[test]
    fn all_addrs_unique() {
        let mut addrs: Vec<u8> = PARAMS.iter().map(|p| p.addr).collect();
        addrs.sort();
        addrs.dedup();
        assert_eq!(addrs.len(), PARAMS.len());
    }

    #[test]
    fn all_names_unique() {
        let mut names: Vec<&str> = PARAMS.iter().map(|p| p.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), PARAMS.len());
    }

    // -----------------------------------------------------------------------
    // Value conversion
    // -----------------------------------------------------------------------

    #[test]
    fn encode_noop() {
        assert_eq!(encode_value(ConversionType::Noop, 42), 42);
    }

    #[test]
    fn encode_times_ten() {
        assert_eq!(encode_value(ConversionType::TimesTen, 120), 12);
        assert_eq!(encode_value(ConversionType::TimesTen, 50), 5);
    }

    #[test]
    fn encode_steps_ten() {
        assert_eq!(encode_value(ConversionType::StepsTen, 95), 9);
        assert_eq!(encode_value(ConversionType::StepsTen, 100), 10);
    }

    #[test]
    fn encode_signed_negative() {
        assert_eq!(encode_value(ConversionType::Signed, -5), (-5i8) as u8);
    }

    #[test]
    fn decode_noop() {
        assert_eq!(decode_value(ConversionType::Noop, 42), 42);
    }

    #[test]
    fn decode_times_ten() {
        assert_eq!(decode_value(ConversionType::TimesTen, 12), 120);
    }

    #[test]
    fn decode_signed_negative() {
        let raw = (-5i8) as u8;
        assert_eq!(decode_value(ConversionType::Signed, raw), -5);
    }

    #[test]
    fn roundtrip_noop() {
        for v in 0..=100i16 {
            let raw = encode_value(ConversionType::Noop, v);
            let dec = decode_value(ConversionType::Noop, raw);
            assert_eq!(dec, v);
        }
    }

    #[test]
    fn roundtrip_times_ten() {
        for v in (0..=120i16).step_by(10) {
            let raw = encode_value(ConversionType::TimesTen, v);
            let dec = decode_value(ConversionType::TimesTen, raw);
            assert_eq!(dec, v, "roundtrip failed for {v}");
        }
    }

    #[test]
    fn roundtrip_signed() {
        for v in -5..=3i16 {
            let raw = encode_value(ConversionType::Signed, v);
            let dec = decode_value(ConversionType::Signed, raw);
            assert_eq!(dec, v, "roundtrip failed for {v}");
        }
    }

    // -----------------------------------------------------------------------
    // Report encoding — write
    // -----------------------------------------------------------------------

    #[test]
    fn write_ff_report() {
        let buf = encode_write(0x04, 75);
        assert_eq!(buf[0], TUNING_HEADER_0);
        assert_eq!(buf[1], TUNING_HEADER_1);
        assert_eq!(buf[2], 0x00);
        assert_eq!(buf[5], 75); // addr 0x04 + 1 = offset 5
    }

    #[test]
    fn write_sen_report() {
        let buf = encode_write(0x03, 90);
        assert_eq!(buf[4], 90); // addr 0x03 + 1 = offset 4
    }

    #[test]
    fn write_report_size() {
        assert_eq!(encode_write(0x04, 0).len(), TUNING_REPORT_SIZE);
    }

    #[test]
    fn write_zeros_elsewhere() {
        let buf = encode_write(0x04, 50);
        for (i, &b) in buf.iter().enumerate() {
            match i {
                0 => assert_eq!(b, 0xFF),
                1 => assert_eq!(b, 0x03),
                5 => assert_eq!(b, 50), // addr+1
                _ => assert_eq!(b, 0x00, "non-zero at offset {i}"),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Report encoding — select slot
    // -----------------------------------------------------------------------

    #[test]
    fn select_slot_1() {
        let buf = encode_select_slot(1);
        assert_eq!(buf[0], TUNING_HEADER_0);
        assert_eq!(buf[1], TUNING_HEADER_1);
        assert_eq!(buf[2], 0x01);
        assert_eq!(buf[3], 1);
    }

    #[test]
    fn select_slot_5() {
        let buf = encode_select_slot(5);
        assert_eq!(buf[3], 5);
    }

    #[test]
    fn select_slot_clamps_low() {
        let buf = encode_select_slot(0);
        assert_eq!(buf[3], 1);
    }

    #[test]
    fn select_slot_clamps_high() {
        let buf = encode_select_slot(10);
        assert_eq!(buf[3], MAX_SLOTS);
    }

    // -----------------------------------------------------------------------
    // Report encoding — reset
    // -----------------------------------------------------------------------

    #[test]
    fn reset_report() {
        let buf = encode_reset();
        assert_eq!(buf[0], TUNING_HEADER_0);
        assert_eq!(buf[1], TUNING_HEADER_1);
        assert_eq!(buf[2], 0x04);
        // rest should be zero
        for &b in &buf[3..] {
            assert_eq!(b, 0x00);
        }
    }

    // -----------------------------------------------------------------------
    // Report encoding — toggle advanced mode
    // -----------------------------------------------------------------------

    #[test]
    fn toggle_advanced_mode_report() {
        let buf = encode_toggle_advanced_mode();
        assert_eq!(buf[0], TUNING_HEADER_0);
        assert_eq!(buf[1], TUNING_HEADER_1);
        assert_eq!(buf[2], 0x06);
    }

    // -----------------------------------------------------------------------
    // Report sizes
    // -----------------------------------------------------------------------

    #[test]
    fn all_reports_64_bytes() {
        assert_eq!(encode_write(0x04, 50).len(), 64);
        assert_eq!(encode_select_slot(1).len(), 64);
        assert_eq!(encode_reset().len(), 64);
        assert_eq!(encode_toggle_advanced_mode().len(), 64);
    }

    // -----------------------------------------------------------------------
    // Cross-validation with header macro data
    // -----------------------------------------------------------------------

    #[test]
    fn verify_header_defined_params() {
        // Verify the addr/name/range from FTEC_TUNING_ATTRS in hid-ftec.h
        let test_cases: &[(&str, u8, i16, i16)] = &[
            ("SLOT", 0x02, 1, 5),
            ("FF", 0x04, 0, 100),
            ("SHO", 0x05, 0, 100),
            ("BLI", 0x06, 0, 101),
            ("FFS", 0x07, 0, 1),
            ("DRI", 0x09, -5, 3),
            ("FOR", 0x0a, 0, 120),
            ("SPR", 0x0b, 0, 120),
            ("DPR", 0x0c, 0, 120),
            ("NDP", 0x0d, 0, 100),
            ("NFR", 0x0e, 0, 100),
            ("FEI", 0x11, 0, 100),
            ("ACP", 0x13, 1, 4),
            ("INT", 0x14, 0, 20),
            ("NIN", 0x15, 0, 100),
            ("FUL", 0x16, 0, 100),
        ];
        for &(name, addr, min, max) in test_cases {
            let p = param_by_name(name);
            assert!(p.is_some(), "param {name} not found");
            if let Some(p) = p {
                assert_eq!(p.addr, addr, "addr mismatch for {name}");
                assert_eq!(p.min, min, "min mismatch for {name}");
                assert_eq!(p.max, max, "max mismatch for {name}");
            }
        }
    }
}
