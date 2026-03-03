//! Comprehensive edge-case and boundary-value tests for the Heusinkveld protocol.
//!
//! Covers report parsing, device identification, pedal capabilities,
//! status flag decoding, and cross-module consistency.

use hid_heusinkveld_protocol::{
    HeusinkveldError, HeusinkveldInputReport, HeusinkveldModel, PedalCapabilities, PedalModel,
    PedalStatus, HEUSINKVELD_HANDBRAKE_V1_PID, HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
    HEUSINKVELD_HANDBRAKE_V2_PID, HEUSINKVELD_LEGACY_SPRINT_PID, HEUSINKVELD_LEGACY_ULTIMATE_PID,
    HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_PRO_PID, HEUSINKVELD_SHIFTER_PID,
    HEUSINKVELD_SHIFTER_VENDOR_ID, HEUSINKVELD_SPRINT_PID, HEUSINKVELD_ULTIMATE_PID,
    HEUSINKVELD_VENDOR_ID, MAX_LOAD_CELL_VALUE, PRODUCT_ID_PRO, PRODUCT_ID_SPRINT,
    PRODUCT_ID_ULTIMATE, REPORT_SIZE_INPUT, VENDOR_ID, heusinkveld_model_from_info,
    is_heusinkveld_device,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Constant golden values
// ---------------------------------------------------------------------------

#[test]
fn constants_golden() {
    assert_eq!(VENDOR_ID, 0x30B7);
    assert_eq!(PRODUCT_ID_SPRINT, 0x1001);
    assert_eq!(PRODUCT_ID_ULTIMATE, 0x1003);
    assert_eq!(PRODUCT_ID_PRO, 0xF6D3);
    assert_eq!(REPORT_SIZE_INPUT, 8);
    assert_eq!(MAX_LOAD_CELL_VALUE, 0xFFFF);
}

#[test]
fn all_vendor_ids_distinct() {
    let vids = [
        HEUSINKVELD_VENDOR_ID,
        HEUSINKVELD_LEGACY_VENDOR_ID,
        HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
        HEUSINKVELD_SHIFTER_VENDOR_ID,
    ];
    let mut sorted = vids.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), vids.len(), "all VIDs must be distinct");
}

#[test]
fn all_current_pids_distinct() {
    let pids = [
        HEUSINKVELD_SPRINT_PID,
        HEUSINKVELD_ULTIMATE_PID,
        HEUSINKVELD_HANDBRAKE_V2_PID,
    ];
    let mut sorted = pids.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), pids.len());
}

// ---------------------------------------------------------------------------
// Report parsing – boundary values
// ---------------------------------------------------------------------------

#[test]
fn parse_all_zeros_report() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0u8; 8];
    let report = HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.throttle, 0);
    assert_eq!(report.brake, 0);
    assert_eq!(report.clutch, 0);
    assert_eq!(report.status, 0);
    Ok(())
}

#[test]
fn parse_all_ones_report() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0xFF_u8; 8];
    let report = HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.throttle, 0xFFFF);
    assert_eq!(report.brake, 0xFFFF);
    assert_eq!(report.clutch, 0xFFFF);
    assert_eq!(report.status, 0xFF);
    Ok(())
}

#[test]
fn parse_max_axis_values() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 8];
    // throttle = MAX (0xFFFF LE)
    data[0] = 0xFF;
    data[1] = 0xFF;
    // brake = 0
    // clutch = 0
    data[6] = 0x03; // status: connected + calibrated
    let report = HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.throttle, MAX_LOAD_CELL_VALUE);
    assert!((report.throttle_normalized() - 1.0).abs() < f32::EPSILON);
    assert!((report.brake_normalized() - 0.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn parse_too_short_report() {
    for len in 0..REPORT_SIZE_INPUT {
        let data = vec![0u8; len];
        let result = HeusinkveldInputReport::parse(&data);
        assert!(
            matches!(result, Err(HeusinkveldError::InvalidReportSize { .. })),
            "length {len} must fail with InvalidReportSize"
        );
    }
}

#[test]
fn parse_extra_bytes_ignored() -> Result<(), Box<dyn std::error::Error>> {
    // Reports longer than 8 bytes should still parse (extra bytes ignored)
    let data = [0x00_u8; 64];
    let report = HeusinkveldInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.throttle, 0);
    Ok(())
}

// ---------------------------------------------------------------------------
// Normalized values – boundary
// ---------------------------------------------------------------------------

#[test]
fn normalized_at_zero() {
    let report = HeusinkveldInputReport::default();
    // default throttle/brake/clutch = 0
    let report_zero = HeusinkveldInputReport {
        throttle: 0,
        brake: 0,
        clutch: 0,
        status: report.status,
    };
    assert!((report_zero.throttle_normalized() - 0.0).abs() < f32::EPSILON);
    assert!((report_zero.brake_normalized() - 0.0).abs() < f32::EPSILON);
    assert!((report_zero.clutch_normalized() - 0.0).abs() < f32::EPSILON);
}

#[test]
fn normalized_at_max() {
    let report = HeusinkveldInputReport {
        throttle: MAX_LOAD_CELL_VALUE,
        brake: MAX_LOAD_CELL_VALUE,
        clutch: MAX_LOAD_CELL_VALUE,
        status: 0,
    };
    assert!((report.throttle_normalized() - 1.0).abs() < f32::EPSILON);
    assert!((report.brake_normalized() - 1.0).abs() < f32::EPSILON);
    assert!((report.clutch_normalized() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn normalized_at_midpoint() {
    let mid = MAX_LOAD_CELL_VALUE / 2;
    let report = HeusinkveldInputReport {
        throttle: mid,
        brake: mid,
        clutch: mid,
        status: 0,
    };
    assert!((report.throttle_normalized() - 0.5).abs() < 0.001);
    assert!((report.brake_normalized() - 0.5).abs() < 0.001);
    assert!((report.clutch_normalized() - 0.5).abs() < 0.001);
}

// ---------------------------------------------------------------------------
// Status flags – exhaustive bit combinations
// ---------------------------------------------------------------------------

#[test]
fn status_flags_all_combinations() {
    // status byte: bit 0 = connected, bit 1 = calibrated, bit 2 = fault
    let report = HeusinkveldInputReport {
        status: 0x00,
        ..Default::default()
    };
    assert!(!report.is_connected());
    assert!(!report.is_calibrated());
    assert!(!report.has_fault());

    let report = HeusinkveldInputReport {
        status: 0x01,
        ..Default::default()
    };
    assert!(report.is_connected());
    assert!(!report.is_calibrated());
    assert!(!report.has_fault());

    let report = HeusinkveldInputReport {
        status: 0x02,
        ..Default::default()
    };
    assert!(!report.is_connected());
    assert!(report.is_calibrated());
    assert!(!report.has_fault());

    let report = HeusinkveldInputReport {
        status: 0x03,
        ..Default::default()
    };
    assert!(report.is_connected());
    assert!(report.is_calibrated());
    assert!(!report.has_fault());

    let report = HeusinkveldInputReport {
        status: 0x04,
        ..Default::default()
    };
    assert!(!report.is_connected());
    assert!(!report.is_calibrated());
    assert!(report.has_fault());

    let report = HeusinkveldInputReport {
        status: 0x07,
        ..Default::default()
    };
    assert!(report.is_connected());
    assert!(report.is_calibrated());
    assert!(report.has_fault());
}

#[test]
fn status_high_bits_do_not_affect_flags() {
    // Bits 3-7 should not affect the flag checks
    let report = HeusinkveldInputReport {
        status: 0xF8, // high bits set, low 3 clear
        ..Default::default()
    };
    assert!(!report.is_connected());
    assert!(!report.is_calibrated());
    assert!(!report.has_fault());
}

// ---------------------------------------------------------------------------
// PedalStatus::from_flags
// ---------------------------------------------------------------------------

#[test]
fn pedal_status_from_flags_comprehensive() {
    assert_eq!(PedalStatus::from_flags(0x00), PedalStatus::Disconnected);
    assert_eq!(PedalStatus::from_flags(0x01), PedalStatus::Calibrating);
    assert_eq!(PedalStatus::from_flags(0x02), PedalStatus::Disconnected);
    assert_eq!(PedalStatus::from_flags(0x03), PedalStatus::Ready);
    assert_eq!(PedalStatus::from_flags(0x04), PedalStatus::Disconnected);
    assert_eq!(PedalStatus::from_flags(0x05), PedalStatus::Calibrating);
    assert_eq!(PedalStatus::from_flags(0x06), PedalStatus::Disconnected);
    assert_eq!(PedalStatus::from_flags(0x07), PedalStatus::Error);
}

#[test]
fn pedal_status_default_is_disconnected() {
    assert_eq!(PedalStatus::default(), PedalStatus::Disconnected);
}

// ---------------------------------------------------------------------------
// Device identification – multi-VID
// ---------------------------------------------------------------------------

#[test]
fn model_from_vid_pid_all_current_devices() {
    assert_eq!(
        HeusinkveldModel::from_vid_pid(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID),
        HeusinkveldModel::Sprint
    );
    assert_eq!(
        HeusinkveldModel::from_vid_pid(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_ULTIMATE_PID),
        HeusinkveldModel::Ultimate
    );
    assert_eq!(
        HeusinkveldModel::from_vid_pid(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_HANDBRAKE_V2_PID),
        HeusinkveldModel::HandbrakeV2
    );
}

#[test]
fn model_from_vid_pid_legacy_devices() {
    assert_eq!(
        HeusinkveldModel::from_vid_pid(HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_LEGACY_SPRINT_PID),
        HeusinkveldModel::Sprint
    );
    assert_eq!(
        HeusinkveldModel::from_vid_pid(
            HEUSINKVELD_LEGACY_VENDOR_ID,
            HEUSINKVELD_LEGACY_ULTIMATE_PID
        ),
        HeusinkveldModel::Ultimate
    );
    assert_eq!(
        HeusinkveldModel::from_vid_pid(HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_PRO_PID),
        HeusinkveldModel::Pro
    );
}

#[test]
fn model_from_vid_pid_peripherals() {
    assert_eq!(
        HeusinkveldModel::from_vid_pid(HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID, HEUSINKVELD_HANDBRAKE_V1_PID),
        HeusinkveldModel::HandbrakeV1
    );
    assert_eq!(
        HeusinkveldModel::from_vid_pid(HEUSINKVELD_SHIFTER_VENDOR_ID, HEUSINKVELD_SHIFTER_PID),
        HeusinkveldModel::SequentialShifter
    );
}

#[test]
fn model_cross_vid_pid_returns_unknown() {
    // Sprint PID with wrong VID should be Unknown
    assert_eq!(
        HeusinkveldModel::from_vid_pid(HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_SPRINT_PID),
        HeusinkveldModel::Unknown
    );
    // Legacy Sprint PID with current VID should be Unknown
    assert_eq!(
        HeusinkveldModel::from_vid_pid(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_LEGACY_SPRINT_PID),
        HeusinkveldModel::Unknown
    );
}

// ---------------------------------------------------------------------------
// Model metadata consistency
// ---------------------------------------------------------------------------

#[test]
fn pedal_models_have_positive_load() {
    for model in [
        HeusinkveldModel::Sprint,
        HeusinkveldModel::Ultimate,
        HeusinkveldModel::Pro,
    ] {
        assert!(
            model.max_load_kg() > 0.0,
            "{model:?} must have positive max_load_kg"
        );
    }
}

#[test]
fn non_pedal_models_have_zero_load() {
    for model in [
        HeusinkveldModel::HandbrakeV1,
        HeusinkveldModel::HandbrakeV2,
        HeusinkveldModel::SequentialShifter,
    ] {
        assert!(
            (model.max_load_kg() - 0.0).abs() < f32::EPSILON,
            "{model:?} must have zero max_load_kg"
        );
    }
}

#[test]
fn non_pedal_models_have_zero_pedal_count() {
    for model in [
        HeusinkveldModel::HandbrakeV1,
        HeusinkveldModel::HandbrakeV2,
        HeusinkveldModel::SequentialShifter,
    ] {
        assert_eq!(model.pedal_count(), 0, "{model:?} must have zero pedal_count");
    }
}

#[test]
fn all_display_names_contain_heusinkveld() {
    let all_models = [
        HeusinkveldModel::Sprint,
        HeusinkveldModel::Ultimate,
        HeusinkveldModel::Pro,
        HeusinkveldModel::HandbrakeV1,
        HeusinkveldModel::HandbrakeV2,
        HeusinkveldModel::SequentialShifter,
        HeusinkveldModel::Unknown,
    ];
    for model in &all_models {
        assert!(
            model.display_name().contains("Heusinkveld")
                || model.display_name().contains("Unknown"),
            "{model:?} display name must contain 'Heusinkveld' or 'Unknown'"
        );
    }
}

#[test]
fn display_names_all_unique() {
    let all_models = [
        HeusinkveldModel::Sprint,
        HeusinkveldModel::Ultimate,
        HeusinkveldModel::Pro,
        HeusinkveldModel::HandbrakeV1,
        HeusinkveldModel::HandbrakeV2,
        HeusinkveldModel::SequentialShifter,
        HeusinkveldModel::Unknown,
    ];
    let mut names: Vec<&str> = all_models.iter().map(|m| m.display_name()).collect();
    let len_before = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), len_before, "all display names must be unique");
}

// ---------------------------------------------------------------------------
// PedalCapabilities consistency
// ---------------------------------------------------------------------------

#[test]
fn pedal_capabilities_for_model_matches_model_metadata() {
    let sprint_caps = PedalCapabilities::for_model(PedalModel::Sprint);
    assert_eq!(sprint_caps.max_load_kg, 55.0);
    assert_eq!(sprint_caps.pedal_count, 2);
    assert!(sprint_caps.has_load_cell);

    let ultimate_caps = PedalCapabilities::for_model(PedalModel::Ultimate);
    assert_eq!(ultimate_caps.max_load_kg, 140.0);
    assert_eq!(ultimate_caps.pedal_count, 3);

    let pro_caps = PedalCapabilities::for_model(PedalModel::Pro);
    assert_eq!(pro_caps.max_load_kg, 200.0);
    assert_eq!(pro_caps.pedal_count, 3);
}

#[test]
fn pedal_capabilities_unknown_uses_defaults() {
    let unknown = PedalCapabilities::for_model(PedalModel::Unknown);
    let default = PedalCapabilities::default();
    assert_eq!(unknown.max_load_kg, default.max_load_kg);
    assert_eq!(unknown.pedal_count, default.pedal_count);
}

#[test]
fn pedal_model_default_is_unknown() {
    assert_eq!(PedalModel::default(), PedalModel::Unknown);
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[test]
fn error_display_messages() {
    let err = HeusinkveldError::InvalidReportSize {
        expected: 8,
        actual: 3,
    };
    let msg = err.to_string();
    assert!(msg.contains("8") && msg.contains("3"));

    let err = HeusinkveldError::InvalidPedalValue(999);
    assert!(err.to_string().contains("999"));

    let err = HeusinkveldError::DeviceNotFound("missing".to_string());
    assert!(err.to_string().contains("missing"));
}

#[test]
fn error_from_hid_common() {
    let hid = openracing_hid_common::HidCommonError::Disconnected;
    let err: HeusinkveldError = hid.into();
    assert!(matches!(err, HeusinkveldError::DeviceNotFound(_)));
}

// ---------------------------------------------------------------------------
// Default report
// ---------------------------------------------------------------------------

#[test]
fn default_report_is_connected_and_calibrated() {
    let report = HeusinkveldInputReport::default();
    assert_eq!(report.status, 0x03);
    assert!(report.is_connected());
    assert!(report.is_calibrated());
    assert!(!report.has_fault());
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Normalized values must be in [0.0, 1.0] for any u16.
    #[test]
    fn prop_normalized_in_range(throttle in any::<u16>(), brake in any::<u16>(), clutch in any::<u16>()) {
        let report = HeusinkveldInputReport {
            throttle,
            brake,
            clutch,
            status: 0,
        };
        let tn = report.throttle_normalized();
        let bn = report.brake_normalized();
        let cn = report.clutch_normalized();
        prop_assert!((0.0..=1.0).contains(&tn), "throttle_normalized={tn} out of range");
        prop_assert!((0.0..=1.0).contains(&bn), "brake_normalized={bn} out of range");
        prop_assert!((0.0..=1.0).contains(&cn), "clutch_normalized={cn} out of range");
    }

    /// parse must succeed for any 8-byte buffer.
    #[test]
    fn prop_parse_succeeds_for_8_bytes(data in proptest::collection::vec(any::<u8>(), 8..=64)) {
        let result = HeusinkveldInputReport::parse(&data);
        prop_assert!(result.is_ok(), "parse must succeed for {}-byte buffer", data.len());
    }

    /// parse must fail for buffers shorter than 8 bytes.
    #[test]
    fn prop_parse_fails_for_short_buffer(data in proptest::collection::vec(any::<u8>(), 0..8)) {
        let result = HeusinkveldInputReport::parse(&data);
        prop_assert!(result.is_err(), "parse must fail for {}-byte buffer", data.len());
    }

    /// Wire format: throttle comes from bytes 0-1 (LE u16).
    #[test]
    fn prop_wire_format_throttle(val in any::<u16>()) {
        let mut data = [0u8; 8];
        let bytes = val.to_le_bytes();
        data[0] = bytes[0];
        data[1] = bytes[1];
        let report = HeusinkveldInputReport::parse(&data)
            .map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(report.throttle, val);
    }

    /// Wire format: brake comes from bytes 2-3 (LE u16).
    #[test]
    fn prop_wire_format_brake(val in any::<u16>()) {
        let mut data = [0u8; 8];
        let bytes = val.to_le_bytes();
        data[2] = bytes[0];
        data[3] = bytes[1];
        let report = HeusinkveldInputReport::parse(&data)
            .map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(report.brake, val);
    }

    /// Wire format: clutch comes from bytes 4-5 (LE u16).
    #[test]
    fn prop_wire_format_clutch(val in any::<u16>()) {
        let mut data = [0u8; 8];
        let bytes = val.to_le_bytes();
        data[4] = bytes[0];
        data[5] = bytes[1];
        let report = HeusinkveldInputReport::parse(&data)
            .map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(report.clutch, val);
    }

    /// Wire format: status is byte 6.
    #[test]
    fn prop_wire_format_status(val in any::<u8>()) {
        let mut data = [0u8; 8];
        data[6] = val;
        let report = HeusinkveldInputReport::parse(&data)
            .map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(report.status, val);
    }

    /// PedalStatus::from_flags is deterministic.
    #[test]
    fn prop_pedal_status_deterministic(flags in any::<u8>()) {
        let s1 = PedalStatus::from_flags(flags);
        let s2 = PedalStatus::from_flags(flags);
        prop_assert_eq!(s1, s2);
    }

    /// is_heusinkveld_device must be true for all known VIDs.
    #[test]
    fn prop_known_vids_recognised(_dummy in 0u8..1u8) {
        prop_assert!(is_heusinkveld_device(HEUSINKVELD_VENDOR_ID));
        prop_assert!(is_heusinkveld_device(HEUSINKVELD_LEGACY_VENDOR_ID));
        prop_assert!(is_heusinkveld_device(HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID));
        prop_assert!(is_heusinkveld_device(HEUSINKVELD_SHIFTER_VENDOR_ID));
    }

    /// heusinkveld_model_from_info must agree with HeusinkveldModel::from_vid_pid.
    #[test]
    fn prop_model_from_info_agrees(vid in any::<u16>(), pid in any::<u16>()) {
        let a = heusinkveld_model_from_info(vid, pid);
        let b = HeusinkveldModel::from_vid_pid(vid, pid);
        prop_assert_eq!(a, b);
    }
}
