//! Protocol verification tests for the FFBeast HID protocol implementation.
//!
//! These tests cross-reference our constants, encoding, and report formats
//! against the Linux kernel mainline source, the FFBeast official project site,
//! and community driver references.
//!
//! ## Sources cited
//!
//! | # | Source | What it confirms |
//! |---|--------|------------------|
//! | 1 | Linux kernel `hid-ids.h` (mainline ≥6.15) | `USB_VENDOR_ID_FFBEAST = 0x045b`, joystick `0x58f9`, rudder `0x5968`, wheel `0x59d7` |
//! | 2 | Linux kernel `hid-universal-pidff.c` | All three PIDs in device table |
//! | 3 | JacKeTUs/linux-steering-wheels | FFBeast Wheel `045b:59d7`, Platinum rating |
//! | 4 | the-sz.com / usb-ids.gowdy.us | VID `0x045B` = Renesas Electronics Corp. (formerly Hitachi) |
//! | 5 | FFBeast official site (ffbeast.github.io) | Project documentation, VID/PID references |
//! | 6 | shubham0x13/ffbeast-wheel-webhid-api | VID `0x045B`, wheel PID `0x59D7`, torque range ±10000 |

use racing_wheel_hid_ffbeast_protocol::{
    CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN, FFBEAST_PRODUCT_ID_JOYSTICK,
    FFBEAST_PRODUCT_ID_RUDDER, FFBEAST_PRODUCT_ID_WHEEL, FFBEAST_VENDOR_ID, FFBeastTorqueEncoder,
    GAIN_REPORT_ID, build_enable_ffb, build_set_gain, is_ffbeast_product,
};

// ════════════════════════════════════════════════════════════════════════════
// § 1. VID / PID verification against Linux kernel mainline
// ════════════════════════════════════════════════════════════════════════════

/// VID `0x045B` = Renesas Electronics Corp. (FFBeast reuses via Renesas MCUs).
/// Source [1]: `#define USB_VENDOR_ID_FFBEAST 0x045b`
/// Source [4]: the-sz.com → "Renesas Electronics Corp."
/// Source [6]: ffbeast-wheel-webhid-api → VID `0x045B`
#[test]
fn vid_matches_kernel_and_vendor_databases() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        FFBEAST_VENDOR_ID, 0x045B,
        "FFBeast VID must be 0x045B (Renesas Electronics, confirmed in kernel hid-ids.h)"
    );
    Ok(())
}

/// Joystick PID `0x58F9` — confirmed in Linux kernel mainline.
/// Source [1]: `#define USB_DEVICE_ID_FFBEAST_JOYSTICK 0x58f9`
/// Source [2]: `hid-universal-pidff.c` device table entry
#[test]
fn joystick_pid_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        FFBEAST_PRODUCT_ID_JOYSTICK, 0x58F9,
        "FFBeast joystick PID must be 0x58F9"
    );
    Ok(())
}

/// Rudder PID `0x5968` — confirmed in Linux kernel mainline.
/// Source [1]: `#define USB_DEVICE_ID_FFBEAST_RUDDER 0x5968`
/// Source [2]: `hid-universal-pidff.c` device table entry
#[test]
fn rudder_pid_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        FFBEAST_PRODUCT_ID_RUDDER, 0x5968,
        "FFBeast rudder PID must be 0x5968"
    );
    Ok(())
}

/// Wheel PID `0x59D7` — confirmed in Linux kernel mainline.
/// Source [1]: `#define USB_DEVICE_ID_FFBEAST_WHEEL 0x59d7`
/// Source [2]: `hid-universal-pidff.c` device table entry
/// Source [3]: linux-steering-wheels → `045b:59d7`, Platinum
/// Source [6]: ffbeast-wheel-webhid-api → PID `0x59D7`
#[test]
fn wheel_pid_matches_kernel_and_community() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        FFBEAST_PRODUCT_ID_WHEEL, 0x59D7,
        "FFBeast wheel PID must be 0x59D7"
    );
    Ok(())
}

/// All three PIDs must be recognised by `is_ffbeast_product`.
#[test]
fn all_confirmed_pids_recognised() -> Result<(), Box<dyn std::error::Error>> {
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_JOYSTICK));
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_RUDDER));
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_WHEEL));
    Ok(())
}

/// Unknown PIDs must not be recognised.
#[test]
fn unknown_pids_rejected() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!is_ffbeast_product(0x0000));
    assert!(!is_ffbeast_product(0xFFFF));
    // VRS DFP shares nothing with FFBeast
    assert!(!is_ffbeast_product(0xA355));
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 2. Constant force report format
// ════════════════════════════════════════════════════════════════════════════

/// Report ID for constant force is 0x01 (standard HID PID constant force).
#[test]
fn constant_force_report_id() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(CONSTANT_FORCE_REPORT_ID, 0x01, "constant force report ID must be 0x01");
    Ok(())
}

/// Constant force report is 5 bytes: report ID (1) + torque i16 LE (2) + reserved (2).
#[test]
fn constant_force_report_len() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        CONSTANT_FORCE_REPORT_LEN, 5,
        "constant force report must be 5 bytes"
    );
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 3. Torque encoding verification
// ════════════════════════════════════════════════════════════════════════════

/// Zero torque → [0x01, 0x00, 0x00, 0x00, 0x00].
/// Source [6]: ffbeast-wheel-webhid-api — torque range ±10000.
#[test]
fn encode_zero_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(0.0);
    assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 0, "zero torque must encode as 0");
    assert_eq!(report[3], 0x00, "reserved byte 3 must be 0");
    assert_eq!(report[4], 0x00, "reserved byte 4 must be 0");
    Ok(())
}

/// Full positive torque (1.0) → raw = 10000.
/// Source [6]: MAX_TORQUE_SCALE = 10000
#[test]
fn encode_full_positive_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 10_000, "full positive must encode as 10000");
    Ok(())
}

/// Full negative torque (-1.0) → raw = -10000.
#[test]
fn encode_full_negative_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(-1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -10_000, "full negative must encode as -10000");
    Ok(())
}

/// Half torque (0.5) → raw = 5000.
#[test]
fn encode_half_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(0.5);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 5000, "half torque must encode as 5000");
    Ok(())
}

/// Clamping: values > 1.0 saturate to 10000; < -1.0 saturate to -10000.
#[test]
fn torque_clamping() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FFBeastTorqueEncoder;
    let over = enc.encode(2.0);
    let normal = enc.encode(1.0);
    assert_eq!(over, normal, "over-range must clamp to 1.0");

    let under = enc.encode(-2.0);
    let neg = enc.encode(-1.0);
    assert_eq!(under, neg, "under-range must clamp to -1.0");
    Ok(())
}

/// Sign preservation: positive input → positive raw, negative input → negative raw.
#[test]
fn torque_sign_preservation() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FFBeastTorqueEncoder;
    let pos = enc.encode(0.3);
    assert!(i16::from_le_bytes([pos[1], pos[2]]) > 0, "positive torque must yield positive raw");

    let neg = enc.encode(-0.3);
    assert!(i16::from_le_bytes([neg[1], neg[2]]) < 0, "negative torque must yield negative raw");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 4. Feature report format verification
// ════════════════════════════════════════════════════════════════════════════

/// Enable FFB feature report: [0x60, 0x01, 0x00] (enable) / [0x60, 0x00, 0x00] (disable).
#[test]
fn enable_ffb_report() -> Result<(), Box<dyn std::error::Error>> {
    let on = build_enable_ffb(true);
    assert_eq!(on[0], 0x60, "enable FFB report ID must be 0x60");
    assert_eq!(on[1], 0x01, "enabled flag must be 0x01");
    assert_eq!(on[2], 0x00, "padding must be 0x00");

    let off = build_enable_ffb(false);
    assert_eq!(off[0], 0x60, "disable FFB report ID must be 0x60");
    assert_eq!(off[1], 0x00, "disabled flag must be 0x00");
    assert_eq!(off[2], 0x00, "padding must be 0x00");
    Ok(())
}

/// Gain report: [0x61, gain, 0x00].
#[test]
fn gain_report() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(GAIN_REPORT_ID, 0x61, "gain report ID must be 0x61");

    let report = build_set_gain(128);
    assert_eq!(report[0], GAIN_REPORT_ID);
    assert_eq!(report[1], 128, "gain value must be 128");
    assert_eq!(report[2], 0x00, "padding must be 0x00");

    // Boundary: full gain
    let full = build_set_gain(255);
    assert_eq!(full[1], 255, "full gain must be 255");

    // Boundary: zero gain
    let zero = build_set_gain(0);
    assert_eq!(zero[1], 0, "zero gain must be 0");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 5. Report length consistency
// ════════════════════════════════════════════════════════════════════════════

/// Encoded reports must always be exactly CONSTANT_FORCE_REPORT_LEN bytes.
#[test]
fn all_encoded_reports_correct_length() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FFBeastTorqueEncoder;
    let values = [-1.0f32, -0.5, 0.0, 0.5, 1.0, 2.0, -2.0];
    for &v in &values {
        let report = enc.encode(v);
        assert_eq!(
            report.len(),
            CONSTANT_FORCE_REPORT_LEN,
            "encode({v}) must produce {CONSTANT_FORCE_REPORT_LEN}-byte report"
        );
    }
    Ok(())
}
