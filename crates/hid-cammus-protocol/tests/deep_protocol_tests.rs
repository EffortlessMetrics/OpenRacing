//! Deep protocol tests for Cammus HID protocol crate.

use racing_wheel_hid_cammus_protocol::{
    CammusModel, FFB_REPORT_ID, FFB_REPORT_LEN, MODE_CONFIG, MODE_GAME,
    PRODUCT_C12, PRODUCT_C5, PRODUCT_CP5_PEDALS, PRODUCT_LC100_PEDALS, REPORT_ID, REPORT_LEN,
    STEERING_RANGE_DEG, VENDOR_ID, encode_stop, encode_torque, is_cammus,
    parse, ParseError, product_name,
};

// ── Device identification ────────────────────────────────────────────────────

#[test]
fn vendor_id_matches_kernel_constant() {
    assert_eq!(VENDOR_ID, 0x3416);
}

#[test]
fn all_product_ids_are_nonzero_and_unique() {
    let pids = [PRODUCT_C5, PRODUCT_C12, PRODUCT_CP5_PEDALS, PRODUCT_LC100_PEDALS];
    for pid in pids {
        assert_ne!(pid, 0, "PID must not be zero");
    }
    for (i, &a) in pids.iter().enumerate() {
        for (j, &b) in pids.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "PIDs at index {i} and {j} must be unique");
            }
        }
    }
}

#[test]
fn is_cammus_recognises_all_known_pids() {
    assert!(is_cammus(VENDOR_ID, PRODUCT_C5));
    assert!(is_cammus(VENDOR_ID, PRODUCT_C12));
    assert!(is_cammus(VENDOR_ID, PRODUCT_CP5_PEDALS));
    assert!(is_cammus(VENDOR_ID, PRODUCT_LC100_PEDALS));
}

#[test]
fn is_cammus_rejects_wrong_vid() {
    assert!(!is_cammus(0x0000, PRODUCT_C5));
    assert!(!is_cammus(0xFFFF, PRODUCT_C12));
    assert!(!is_cammus(0x0483, PRODUCT_C5)); // VRS VID, not Cammus
}

#[test]
fn is_cammus_rejects_unknown_pid() {
    assert!(!is_cammus(VENDOR_ID, 0x0000));
    assert!(!is_cammus(VENDOR_ID, 0xFFFF));
    assert!(!is_cammus(VENDOR_ID, 0x0303));
}

#[test]
fn product_name_returns_correct_strings() {
    assert_eq!(product_name(PRODUCT_C5), Some("Cammus C5"));
    assert_eq!(product_name(PRODUCT_C12), Some("Cammus C12"));
    assert_eq!(product_name(PRODUCT_CP5_PEDALS), Some("Cammus CP5 Pedals"));
    assert_eq!(
        product_name(PRODUCT_LC100_PEDALS),
        Some("Cammus LC100 Pedals")
    );
}

#[test]
fn product_name_unknown_returns_none() {
    assert_eq!(product_name(0x0000), None);
    assert_eq!(product_name(0xFFFF), None);
}

// ── CammusModel classification ───────────────────────────────────────────────

#[test]
fn model_from_pid_maps_correctly() -> Result<(), String> {
    let cases: &[(u16, CammusModel)] = &[
        (PRODUCT_C5, CammusModel::C5),
        (PRODUCT_C12, CammusModel::C12),
        (PRODUCT_CP5_PEDALS, CammusModel::Cp5Pedals),
        (PRODUCT_LC100_PEDALS, CammusModel::Lc100Pedals),
    ];
    for &(pid, expected) in cases {
        let model = CammusModel::from_pid(pid)
            .ok_or_else(|| format!("PID 0x{pid:04X} should resolve"))?;
        assert_eq!(model, expected);
    }
    Ok(())
}

#[test]
fn model_from_unknown_pid_returns_none() {
    assert!(CammusModel::from_pid(0x0000).is_none());
    assert!(CammusModel::from_pid(0xFFFF).is_none());
}

#[test]
fn model_torque_values_correct() {
    assert!((CammusModel::C5.max_torque_nm() - 5.0).abs() < 0.01);
    assert!((CammusModel::C12.max_torque_nm() - 12.0).abs() < 0.01);
    assert!((CammusModel::Cp5Pedals.max_torque_nm()).abs() < 0.01);
    assert!((CammusModel::Lc100Pedals.max_torque_nm()).abs() < 0.01);
}

#[test]
fn model_names_match_product_names() {
    assert_eq!(CammusModel::C5.name(), "Cammus C5");
    assert_eq!(CammusModel::C12.name(), "Cammus C12");
    assert_eq!(CammusModel::Cp5Pedals.name(), "Cammus CP5 Pedals");
    assert_eq!(CammusModel::Lc100Pedals.name(), "Cammus LC100 Pedals");
}

// ── Torque command encoding ──────────────────────────────────────────────────

#[test]
fn encode_torque_zero_has_correct_structure() {
    let r = encode_torque(0.0);
    assert_eq!(r.len(), FFB_REPORT_LEN);
    assert_eq!(r[0], FFB_REPORT_ID);
    let raw = i16::from_le_bytes([r[1], r[2]]);
    assert_eq!(raw, 0);
    assert_eq!(r[3], MODE_GAME);
    assert_eq!(&r[4..], &[0, 0, 0, 0]);
}

#[test]
fn encode_torque_full_positive() {
    let r = encode_torque(1.0);
    let raw = i16::from_le_bytes([r[1], r[2]]);
    assert_eq!(raw, i16::MAX);
}

#[test]
fn encode_torque_full_negative() {
    let r = encode_torque(-1.0);
    let raw = i16::from_le_bytes([r[1], r[2]]);
    assert_eq!(raw, -i16::MAX);
}

#[test]
fn encode_torque_clamps_above_1() {
    assert_eq!(encode_torque(2.0), encode_torque(1.0));
    assert_eq!(encode_torque(100.0), encode_torque(1.0));
}

#[test]
fn encode_torque_clamps_below_neg1() {
    assert_eq!(encode_torque(-2.0), encode_torque(-1.0));
    assert_eq!(encode_torque(-100.0), encode_torque(-1.0));
}

#[test]
fn encode_torque_preserves_sign() {
    let pos = encode_torque(0.5);
    let neg = encode_torque(-0.5);
    assert!(i16::from_le_bytes([pos[1], pos[2]]) > 0);
    assert!(i16::from_le_bytes([neg[1], neg[2]]) < 0);
}

#[test]
fn encode_torque_is_monotonic() {
    let v = |t: f32| {
        let r = encode_torque(t);
        i16::from_le_bytes([r[1], r[2]])
    };
    assert!(v(-0.75) < v(-0.25));
    assert!(v(-0.25) < v(0.0));
    assert!(v(0.0) < v(0.25));
    assert!(v(0.25) < v(0.75));
}

// ── Configuration command encoding ───────────────────────────────────────────

#[test]
fn encode_stop_equals_zero_torque() {
    assert_eq!(encode_stop(), encode_torque(0.0));
}

#[test]
fn mode_constants_are_distinct() {
    assert_ne!(MODE_GAME, MODE_CONFIG);
    assert_eq!(MODE_GAME, 0x01);
    assert_eq!(MODE_CONFIG, 0x00);
}

// ── Report parsing ───────────────────────────────────────────────────────────

#[test]
fn parse_too_short_returns_error() {
    let result = parse(&[0u8; 5]);
    assert!(result.is_err());
    let err = match result {
        Err(e) => e,
        Ok(_) => return,
    };
    assert_eq!(err, ParseError::TooShort { got: 5, need: 12 });
}

#[test]
fn parse_minimum_length_succeeds() -> Result<(), ParseError> {
    let data = [0u8; 12];
    let _report = parse(&data)?;
    Ok(())
}

#[test]
fn parse_center_position() -> Result<(), ParseError> {
    let data = [0u8; 64];
    let report = parse(&data)?;
    assert!(report.steering.abs() < 0.01);
    assert!(report.throttle.abs() < 0.01);
    assert!(report.brake.abs() < 0.01);
    assert!(report.clutch.abs() < 0.01);
    assert!(report.handbrake.abs() < 0.01);
    assert_eq!(report.buttons, 0);
    Ok(())
}

#[test]
fn parse_full_throttle() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    data[2] = 0xFF;
    data[3] = 0xFF;
    let report = parse(&data)?;
    assert!((report.throttle - 1.0).abs() < 0.01);
    Ok(())
}

#[test]
fn parse_full_brake() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    data[4] = 0xFF;
    data[5] = 0xFF;
    let report = parse(&data)?;
    assert!((report.brake - 1.0).abs() < 0.01);
    Ok(())
}

#[test]
fn parse_buttons_encoding() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    data[6] = 0xAB;
    data[7] = 0xCD;
    let report = parse(&data)?;
    assert_eq!(report.buttons, 0xCDAB);
    Ok(())
}

#[test]
fn parse_steering_full_positive() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    let bytes = i16::MAX.to_le_bytes();
    data[0] = bytes[0];
    data[1] = bytes[1];
    let report = parse(&data)?;
    assert!((report.steering - 1.0).abs() < 0.01);
    Ok(())
}

#[test]
fn parse_steering_full_negative() -> Result<(), ParseError> {
    let mut data = [0u8; 64];
    let bytes = (-i16::MAX).to_le_bytes();
    data[0] = bytes[0];
    data[1] = bytes[1];
    let report = parse(&data)?;
    assert!((report.steering + 1.0).abs() < 0.01);
    Ok(())
}

// ── Report constants ─────────────────────────────────────────────────────────

#[test]
fn report_constants_are_correct() {
    assert_eq!(REPORT_LEN, 64);
    assert_eq!(REPORT_ID, 0x01);
    assert_eq!(FFB_REPORT_ID, 0x01);
    assert_eq!(FFB_REPORT_LEN, 8);
    assert!((STEERING_RANGE_DEG - 1080.0).abs() < 0.01);
}
