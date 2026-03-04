//! Protocol verification tests for the Cammus HID protocol implementation.
//!
//! These tests cross-reference our constants, encoding, and report formats
//! against the Linux kernel mainline source, community hardware databases,
//! and the USB HID PID specification.
//!
//! ## Sources cited
//!
//! | # | Source | What it confirms |
//! |---|--------|------------------|
//! | 1 | Linux kernel `hid-ids.h` (mainline ≥6.15) | `USB_VENDOR_ID_CAMMUS = 0x3416`, `USB_DEVICE_ID_CAMMUS_C5 = 0x0301`, `USB_DEVICE_ID_CAMMUS_C12 = 0x0302` |
//! | 2 | Linux kernel `hid-universal-pidff.c` | C5 and C12 in device table, standard PIDFF |
//! | 3 | JacKeTUs/linux-steering-wheels | VID `3416`, C5 `0301` Platinum, C12 `0302` Platinum |
//! | 4 | JacKeTUs/simracing-hwdb `90-cammus.hwdb` | `v3416p0301`, `v3416p0302`, `v3416p1018`, `v3416p1019` |
//! | 5 | the-sz.com USB vendor database | VID `0x3416` = "Shenzhen Cammus Electronic Technology Com. Ltd." |

use racing_wheel_hid_cammus_protocol::{
    CammusModel, FFB_REPORT_ID, FFB_REPORT_LEN, MODE_CONFIG, MODE_GAME, PRODUCT_C5, PRODUCT_C12,
    PRODUCT_CP5_PEDALS, PRODUCT_LC100_PEDALS, REPORT_ID, REPORT_LEN, STEERING_RANGE_DEG, VENDOR_ID,
    encode_stop, encode_torque, is_cammus, parse, product_name,
};

// ════════════════════════════════════════════════════════════════════════════
// § 1. VID / PID verification against Linux kernel mainline
// ════════════════════════════════════════════════════════════════════════════

/// VID `0x3416` = Shenzhen Cammus Electronic Technology Co., Ltd.
/// Source [1]: Linux kernel `hid-ids.h` → `#define USB_VENDOR_ID_CAMMUS 0x3416`
/// Source [5]: the-sz.com → "Shenzhen Cammus Electronic Technology Com. Ltd."
#[test]
fn vid_matches_kernel_mainline() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        VENDOR_ID, 0x3416,
        "Cammus VID must be 0x3416 (confirmed in Linux kernel hid-ids.h)"
    );
    Ok(())
}

/// C5 PID `0x0301` — confirmed in Linux kernel mainline.
/// Source [1]: `#define USB_DEVICE_ID_CAMMUS_C5 0x0301`
/// Source [3]: linux-steering-wheels → Platinum rating
/// Source [4]: simracing-hwdb → `v3416p0301`
#[test]
fn c5_pid_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PRODUCT_C5, 0x0301, "C5 PID must be 0x0301");
    Ok(())
}

/// C12 PID `0x0302` — confirmed in Linux kernel mainline.
/// Source [1]: `#define USB_DEVICE_ID_CAMMUS_C12 0x0302`
/// Source [3]: linux-steering-wheels → Platinum rating
/// Source [4]: simracing-hwdb → `v3416p0302`
#[test]
fn c12_pid_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PRODUCT_C12, 0x0302, "C12 PID must be 0x0302");
    Ok(())
}

/// CP5 Pedals PID `0x1018` — confirmed in simracing-hwdb.
/// Source [4]: `v3416p1018` labeled "Cammus CP5 Pedals"
#[test]
fn cp5_pedals_pid_matches_hwdb() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PRODUCT_CP5_PEDALS, 0x1018, "CP5 Pedals PID must be 0x1018");
    Ok(())
}

/// LC100 Pedals PID `0x1019` — confirmed in simracing-hwdb.
/// Source [4]: `v3416p1019` labeled "Cammus LC100 Pedals"
#[test]
fn lc100_pedals_pid_matches_hwdb() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        PRODUCT_LC100_PEDALS, 0x1019,
        "LC100 Pedals PID must be 0x1019"
    );
    Ok(())
}

/// `is_cammus()` must accept all four confirmed VID/PID pairs.
#[test]
fn is_cammus_accepts_all_confirmed_pairs() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        PRODUCT_C5,
        PRODUCT_C12,
        PRODUCT_CP5_PEDALS,
        PRODUCT_LC100_PEDALS,
    ];
    for &pid in &pids {
        assert!(
            is_cammus(VENDOR_ID, pid),
            "is_cammus must return true for PID 0x{pid:04X}"
        );
    }
    Ok(())
}

/// Wrong VID must be rejected.
#[test]
fn wrong_vid_rejected() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        !is_cammus(0x0000, PRODUCT_C5),
        "VID 0x0000 must be rejected"
    );
    assert!(!is_cammus(0x0483, PRODUCT_C5), "STM VID must be rejected");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 2. Product name mapping
// ════════════════════════════════════════════════════════════════════════════

/// All known PIDs must have a product name.
#[test]
fn product_names_for_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_name(PRODUCT_C5), Some("Cammus C5"));
    assert_eq!(product_name(PRODUCT_C12), Some("Cammus C12"));
    assert_eq!(product_name(PRODUCT_CP5_PEDALS), Some("Cammus CP5 Pedals"));
    assert_eq!(
        product_name(PRODUCT_LC100_PEDALS),
        Some("Cammus LC100 Pedals")
    );
    assert_eq!(product_name(0xFFFF), None, "unknown PID must return None");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 3. Device model classification and torque ratings
// ════════════════════════════════════════════════════════════════════════════

/// C5 is a 5 Nm desktop direct drive wheel.
#[test]
fn c5_model_torque() -> Result<(), Box<dyn std::error::Error>> {
    let model = CammusModel::from_pid(PRODUCT_C5);
    assert_eq!(model, Some(CammusModel::C5));
    if let Some(m) = model {
        assert!(
            (m.max_torque_nm() - 5.0).abs() < f32::EPSILON,
            "C5 should be 5 Nm"
        );
    }
    Ok(())
}

/// C12 is a 12 Nm desktop direct drive wheel.
#[test]
fn c12_model_torque() -> Result<(), Box<dyn std::error::Error>> {
    let model = CammusModel::from_pid(PRODUCT_C12);
    assert_eq!(model, Some(CammusModel::C12));
    if let Some(m) = model {
        assert!(
            (m.max_torque_nm() - 12.0).abs() < f32::EPSILON,
            "C12 should be 12 Nm"
        );
    }
    Ok(())
}

/// Pedals have zero torque (input-only devices).
#[test]
fn pedal_models_zero_torque() -> Result<(), Box<dyn std::error::Error>> {
    for &pid in &[PRODUCT_CP5_PEDALS, PRODUCT_LC100_PEDALS] {
        let model = CammusModel::from_pid(pid);
        assert!(model.is_some(), "PID 0x{pid:04X} must resolve to a model");
        if let Some(m) = model {
            assert!(
                m.max_torque_nm().abs() < f32::EPSILON,
                "pedal model must have 0 Nm torque"
            );
        }
    }
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 4. Input report format verification (estimated layout)
// ════════════════════════════════════════════════════════════════════════════

/// Input report constants: 64 bytes, report ID 0x01.
#[test]
fn input_report_constants() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(REPORT_LEN, 64, "Cammus input report must be 64 bytes");
    assert_eq!(REPORT_ID, 0x01, "Cammus input report ID must be 0x01");
    Ok(())
}

/// Steering range = 1080° (±540°).
#[test]
fn steering_range_is_1080_degrees() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        (STEERING_RANGE_DEG - 1080.0).abs() < f32::EPSILON,
        "steering range must be 1080°"
    );
    Ok(())
}

/// All-zero 64-byte report must parse to center/idle state.
#[test]
fn parse_zero_report_is_center() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0u8; 64];
    let report = parse(&data).map_err(|e| e.to_string())?;
    assert!(
        report.steering.abs() < 0.01,
        "zero input must yield ~0 steering"
    );
    assert!(
        report.throttle.abs() < 0.01,
        "zero input must yield ~0 throttle"
    );
    assert!(report.brake.abs() < 0.01, "zero input must yield ~0 brake");
    assert_eq!(report.buttons, 0, "zero input must yield 0 buttons");
    Ok(())
}

/// Full-scale throttle: bytes 2–3 = 0xFFFF → throttle ≈ 1.0.
#[test]
fn parse_full_throttle() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[2] = 0xFF;
    data[3] = 0xFF;
    let report = parse(&data).map_err(|e| e.to_string())?;
    assert!(
        (report.throttle - 1.0).abs() < 0.01,
        "0xFFFF must yield throttle ≈ 1.0"
    );
    Ok(())
}

/// Full-scale brake: bytes 4–5 = 0xFFFF → brake ≈ 1.0.
#[test]
fn parse_full_brake() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[4] = 0xFF;
    data[5] = 0xFF;
    let report = parse(&data).map_err(|e| e.to_string())?;
    assert!(
        (report.brake - 1.0).abs() < 0.01,
        "0xFFFF must yield brake ≈ 1.0"
    );
    Ok(())
}

/// Minimum parseable length is 12 bytes (11 must fail).
#[test]
fn parse_minimum_length() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse(&[0u8; 12]).is_ok(), "12 bytes must succeed");
    assert!(parse(&[0u8; 11]).is_err(), "11 bytes must fail");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 5. FFB output report format verification (estimated layout)
// ════════════════════════════════════════════════════════════════════════════

/// FFB output report constants: 8 bytes, report ID 0x01.
#[test]
fn ffb_report_constants() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(FFB_REPORT_LEN, 8, "FFB report must be 8 bytes");
    assert_eq!(FFB_REPORT_ID, 0x01, "FFB report ID must be 0x01");
    Ok(())
}

/// Mode byte constants.
#[test]
fn mode_constants() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(MODE_GAME, 0x01, "game mode must be 0x01");
    assert_eq!(MODE_CONFIG, 0x00, "config mode must be 0x00");
    Ok(())
}

/// Zero torque encoding: report ID + 0x0000 torque + game mode + zeros.
#[test]
fn encode_zero_torque_byte_layout() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_torque(0.0);
    assert_eq!(report.len(), FFB_REPORT_LEN, "report length mismatch");
    assert_eq!(report[0], FFB_REPORT_ID, "byte 0 must be report ID");
    assert_eq!(report[1], 0x00, "torque low byte must be 0");
    assert_eq!(report[2], 0x00, "torque high byte must be 0");
    assert_eq!(report[3], MODE_GAME, "byte 3 must be game mode");
    assert_eq!(
        &report[4..],
        &[0x00, 0x00, 0x00, 0x00],
        "reserved bytes must be 0"
    );
    Ok(())
}

/// Full positive torque → i16::MAX (0x7FFF).
#[test]
fn encode_full_positive_torque() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_torque(1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, i16::MAX, "full positive must encode as i16::MAX");
    Ok(())
}

/// Full negative torque → -i16::MAX (-0x7FFF).
#[test]
fn encode_full_negative_torque() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_torque(-1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -i16::MAX, "full negative must encode as -i16::MAX");
    Ok(())
}

/// `encode_stop()` must produce the same bytes as `encode_torque(0.0)`.
#[test]
fn encode_stop_equals_zero_torque() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        encode_stop(),
        encode_torque(0.0),
        "encode_stop must equal encode_torque(0.0)"
    );
    Ok(())
}

/// Clamping: values > 1.0 must saturate to 1.0.
#[test]
fn torque_clamping() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        encode_torque(2.0),
        encode_torque(1.0),
        "over-range must clamp to 1.0"
    );
    assert_eq!(
        encode_torque(-2.0),
        encode_torque(-1.0),
        "under-range must clamp to -1.0"
    );
    Ok(())
}
