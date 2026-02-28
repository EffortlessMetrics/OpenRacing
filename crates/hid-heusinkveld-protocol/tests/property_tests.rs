//! Property-based tests for the Heusinkveld HID protocol crate.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID / PID constant values and device detection
//! - Model detection determinism and correctness
//! - Normalized pedal axis values stay in [0.0, 1.0]
//! - Input report parsing behavior for all u16 inputs
//! - PedalStatus flag decoding

use hid_heusinkveld_protocol::{
    HEUSINKVELD_PRO_PID, HEUSINKVELD_SPRINT_PID, HEUSINKVELD_ULTIMATE_PID, HEUSINKVELD_VENDOR_ID,
    HeusinkveldInputReport, HeusinkveldModel, PedalCapabilities, PedalModel, PedalStatus,
    REPORT_SIZE_INPUT, heusinkveld_model_from_info, is_heusinkveld_device,
};
use proptest::prelude::*;

// ── VID / PID invariants ──────────────────────────────────────────────────────

/// VID constant must equal the authoritative Heusinkveld USB vendor ID (0x16D0).
#[test]
fn test_vendor_id_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        HEUSINKVELD_VENDOR_ID, 0x16D0,
        "Heusinkveld VID must be 0x16D0"
    );
    Ok(())
}

/// Every known PID must be recognised by `heusinkveld_model_from_info`.
#[test]
fn test_all_known_pids_detected() -> Result<(), Box<dyn std::error::Error>> {
    let known = [
        (HEUSINKVELD_SPRINT_PID, HeusinkveldModel::Sprint),
        (HEUSINKVELD_ULTIMATE_PID, HeusinkveldModel::Ultimate),
        (HEUSINKVELD_PRO_PID, HeusinkveldModel::Pro),
    ];
    for (pid, expected) in known {
        let model = heusinkveld_model_from_info(HEUSINKVELD_VENDOR_ID, pid);
        assert_eq!(
            model, expected,
            "PID 0x{pid:04X} must classify as {expected:?}"
        );
        assert_ne!(
            model,
            HeusinkveldModel::Unknown,
            "PID 0x{pid:04X} must not classify as Unknown"
        );
    }
    Ok(())
}

/// Exact numeric values verified against Heusinkveld USB descriptors.
#[test]
fn test_pid_constant_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(HEUSINKVELD_SPRINT_PID, 0x1156);
    assert_eq!(HEUSINKVELD_ULTIMATE_PID, 0x1157);
    assert_eq!(HEUSINKVELD_PRO_PID, 0x1158);
    Ok(())
}

/// Max load must increase Sprint < Ultimate < Pro.
#[test]
fn test_max_load_ordering() -> Result<(), Box<dyn std::error::Error>> {
    let sprint = HeusinkveldModel::Sprint.max_load_kg();
    let ultimate = HeusinkveldModel::Ultimate.max_load_kg();
    let pro = HeusinkveldModel::Pro.max_load_kg();
    assert!(sprint > 0.0, "Sprint max load must be positive");
    assert!(
        ultimate > sprint,
        "Ultimate ({ultimate} kg) must exceed Sprint ({sprint} kg)"
    );
    assert!(
        pro > ultimate,
        "Pro ({pro} kg) must exceed Ultimate ({ultimate} kg)"
    );
    Ok(())
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    // ── Model detection: determinism ──────────────────────────────────────────

    /// HeusinkveldModel::from_product_id must return the same model for the same PID.
    #[test]
    fn prop_model_from_pid_deterministic(pid: u16) {
        let a = HeusinkveldModel::from_product_id(pid);
        let b = HeusinkveldModel::from_product_id(pid);
        prop_assert_eq!(a, b, "model must be stable for pid={:#06x}", pid);
    }

    /// heusinkveld_model_from_info with the correct VID must match from_product_id.
    #[test]
    fn prop_model_from_info_matches_from_pid_for_correct_vid(pid: u16) {
        let via_info = heusinkveld_model_from_info(HEUSINKVELD_VENDOR_ID, pid);
        let via_pid  = HeusinkveldModel::from_product_id(pid);
        prop_assert_eq!(
            via_info, via_pid,
            "heusinkveld_model_from_info with correct VID must match from_product_id for pid={:#06x}",
            pid
        );
    }

    /// heusinkveld_model_from_info with any VID other than HEUSINKVELD_VENDOR_ID must return Unknown.
    #[test]
    fn prop_wrong_vid_always_unknown(vid: u16, pid: u16) {
        prop_assume!(vid != HEUSINKVELD_VENDOR_ID);
        let model = heusinkveld_model_from_info(vid, pid);
        prop_assert_eq!(
            model,
            HeusinkveldModel::Unknown,
            "wrong VID {:#06x} must always return Unknown (pid={:#06x})",
            vid, pid
        );
    }

    // ── is_heusinkveld_device ─────────────────────────────────────────────────

    /// is_heusinkveld_device must return true only for HEUSINKVELD_VENDOR_ID.
    #[test]
    fn prop_is_heusinkveld_device_vid_check(vid: u16) {
        let result = is_heusinkveld_device(vid);
        if vid == HEUSINKVELD_VENDOR_ID {
            prop_assert!(result, "HEUSINKVELD_VENDOR_ID must be recognized");
        } else {
            prop_assert!(!result, "VID {:#06x} must not be recognized as Heusinkveld", vid);
        }
    }

    // ── Normalized pedal values stay in [0.0, 1.0] ───────────────────────────

    /// throttle_normalized() must always return a value in [0.0, 1.0].
    #[test]
    fn prop_throttle_normalized_in_range(throttle: u16) {
        let report = HeusinkveldInputReport {
            throttle,
            brake: 0,
            clutch: 0,
            status: 0,
        };
        let n = report.throttle_normalized();
        prop_assert!(
            (0.0f32..=1.0f32).contains(&n),
            "throttle_normalized() = {n} must be in [0.0, 1.0] for throttle={throttle}"
        );
    }

    /// brake_normalized() must always return a value in [0.0, 1.0].
    #[test]
    fn prop_brake_normalized_in_range(brake: u16) {
        let report = HeusinkveldInputReport {
            throttle: 0,
            brake,
            clutch: 0,
            status: 0,
        };
        let n = report.brake_normalized();
        prop_assert!(
            (0.0f32..=1.0f32).contains(&n),
            "brake_normalized() = {n} must be in [0.0, 1.0] for brake={brake}"
        );
    }

    /// clutch_normalized() must always return a value in [0.0, 1.0].
    #[test]
    fn prop_clutch_normalized_in_range(clutch: u16) {
        let report = HeusinkveldInputReport {
            throttle: 0,
            brake: 0,
            clutch,
            status: 0,
        };
        let n = report.clutch_normalized();
        prop_assert!(
            (0.0f32..=1.0f32).contains(&n),
            "clutch_normalized() = {n} must be in [0.0, 1.0] for clutch={clutch}"
        );
    }

    // ── Report parsing: correct parse vs. short buffer ────────────────────────

    /// Parsing an 8-byte buffer must always succeed (never panic or error).
    #[test]
    fn prop_parse_full_report_always_ok(data: [u8; 8]) {
        let result = HeusinkveldInputReport::parse(&data);
        prop_assert!(result.is_ok(), "parse of 8-byte buffer must always succeed");
    }

    /// Parsing a buffer shorter than REPORT_SIZE_INPUT must always fail.
    #[test]
    fn prop_parse_short_buffer_always_err(len in 0usize..REPORT_SIZE_INPUT) {
        let data = vec![0u8; len];
        let result = HeusinkveldInputReport::parse(&data);
        prop_assert!(
            result.is_err(),
            "parse of {len}-byte buffer must fail (need {REPORT_SIZE_INPUT})"
        );
    }

    // ── PedalStatus flag decoding ─────────────────────────────────────────────

    /// If bit 0 of flags is clear, status must be Disconnected.
    #[test]
    fn prop_disconnected_when_bit0_clear(flags in 0u8..=0xFEu8) {
        let flags_no_bit0 = flags & !0x01u8;
        let status = PedalStatus::from_flags(flags_no_bit0);
        prop_assert_eq!(
            status,
            PedalStatus::Disconnected,
            "flags={:#04x} with bit0 clear must give Disconnected",
            flags_no_bit0
        );
    }

    /// If bits 0 and 1 are set, and bit 2 is clear, status must be Ready.
    #[test]
    fn prop_ready_when_bits_0_and_1_set_bit2_clear(extra in 0u8..=0b11111000u8) {
        let flags = 0x03u8 | (extra & 0b11111000u8);
        // Mask out bit 2 to ensure not Error
        let flags = flags & !0x04u8;
        let status = PedalStatus::from_flags(flags);
        prop_assert_eq!(
            status,
            PedalStatus::Ready,
            "flags={:#04x} with bits 0+1 set and bit2 clear must give Ready",
            flags
        );
    }

    // ── PedalCapabilities: known models have positive max load ────────────────

    /// Every known PedalModel must report a strictly positive max_load_kg.
    #[test]
    fn prop_known_models_positive_max_load(idx in 0usize..3usize) {
        let models = [PedalModel::Sprint, PedalModel::Ultimate, PedalModel::Pro];
        let caps = PedalCapabilities::for_model(models[idx]);
        prop_assert!(
            caps.max_load_kg > 0.0,
            "model {:?} must have positive max_load_kg", models[idx]
        );
        prop_assert!(
            caps.pedal_count >= 2,
            "model {:?} must have at least 2 pedals", models[idx]
        );
    }
}
