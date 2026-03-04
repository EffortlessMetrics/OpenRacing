//! Protocol verification tests for the VRS DirectForce Pro HID protocol implementation.
//!
//! These tests cross-reference our constants, encoding, and report formats
//! against the Linux kernel mainline source, community hardware databases,
//! and the USB HID PID specification.
#![allow(deprecated)] // Tests intentionally exercise deprecated PEDALS_V1 constant
//!
//! ## Sources cited
//!
//! | # | Source | What it confirms |
//! |---|--------|------------------|
//! | 1 | Linux kernel `hid-ids.h` (mainline) | `USB_VENDOR_ID_VRS` missing (uses `0x0483` STM), `USB_DEVICE_ID_VRS_DFP = 0xa355`, `USB_DEVICE_ID_VRS_R295 = 0xa44c` |
//! | 2 | Linux kernel `hid-universal-pidff.c` | DFP in device table with `HID_PIDFF_QUIRK_PERMISSIVE_CONTROL` |
//! | 3 | JacKeTUs/linux-steering-wheels | VRS DFP `0483:a355`, Platinum rating |
//! | 4 | JacKeTUs/simracing-hwdb `90-vrs.hwdb` | `v0483pA355` (DirectForce), `v0483pA3BE` (Pedals) |
//! | 5 | USB HID PID specification (`pid1_01.pdf`) | PIDFF report IDs: constant force 0x11, spring 0x19, damper 0x1A, friction 0x1B |
//! | 6 | the-sz.com / usb-ids.gowdy.us | VID `0x0483` = STMicroelectronics |

use racing_wheel_hid_vrs_protocol::{
    CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SPRING_REPORT_LEN,
    VRS_PRODUCT_ID, VRS_VENDOR_ID, VrsConstantForceEncoder, VrsDamperEncoder, VrsFrictionEncoder,
    VrsSpringEncoder, build_device_gain, build_ffb_enable, build_rotation_range, identify_device,
    is_wheelbase_product, product_ids,
};

// ════════════════════════════════════════════════════════════════════════════
// § 1. VID / PID verification
// ════════════════════════════════════════════════════════════════════════════

/// VID `0x0483` = STMicroelectronics (generic shared VID for STM32 devices).
/// Source [6]: the-sz.com → "STMicroelectronics"
/// Note: shared with legacy Simagic (PID `0x0522`) and Cube Controls (provisional).
#[test]
fn vid_is_stmicroelectronics() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        VRS_VENDOR_ID, 0x0483,
        "VRS VID must be 0x0483 (STMicroelectronics)"
    );
    Ok(())
}

/// DFP PID `0xA355` — confirmed in Linux kernel mainline.
/// Source [1]: `#define USB_DEVICE_ID_VRS_DFP 0xa355`
/// Source [2]: `hid-universal-pidff.c` device table
/// Source [3]: linux-steering-wheels → Platinum
/// Source [4]: simracing-hwdb → `v0483pA355`
#[test]
fn dfp_pid_matches_kernel_and_community() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(VRS_PRODUCT_ID, 0xA355, "DFP PID must be 0xA355");
    assert_eq!(
        product_ids::DIRECTFORCE_PRO,
        0xA355,
        "product_ids::DIRECTFORCE_PRO must be 0xA355"
    );
    Ok(())
}

/// R295 PID `0xA44C` — confirmed in Linux kernel `hid-ids.h` and `hid-quirks.c`.
/// Source [1]: `#define USB_DEVICE_ID_VRS_R295 0xa44c`
#[test]
fn r295_pid_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_ids::R295, 0xA44C, "R295 PID must be 0xA44C");
    Ok(())
}

/// Pedals PID `0xA3BE` — confirmed via simracing-hwdb.
/// Source [4]: `v0483pA3BE` labeled "VRS DirectForce Pro Pedals"
#[test]
fn pedals_pid_matches_hwdb() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_ids::PEDALS, 0xA3BE, "Pedals PID must be 0xA3BE");
    Ok(())
}

/// DFP V2 PID `0xA356` — unverified (not in kernel, not in community sources).
#[test]
fn dfp_v2_pid_is_sequential_estimate() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        product_ids::DIRECTFORCE_PRO_V2,
        0xA356,
        "DFP V2 PID must be 0xA356 (unverified sequential estimate)"
    );
    Ok(())
}

/// Unverified accessory PIDs exist but are sequential estimates.
#[test]
fn unverified_accessory_pids() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_ids::PEDALS_V2, 0xA358);
    assert_eq!(product_ids::HANDBRAKE, 0xA359);
    assert_eq!(product_ids::SHIFTER, 0xA35A);
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 2. Device identification and classification
// ════════════════════════════════════════════════════════════════════════════

/// DFP is classified as a wheelbase with FFB support and 20 Nm torque.
#[test]
fn dfp_device_identity() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::DIRECTFORCE_PRO);
    assert_eq!(id.product_id, 0xA355);
    assert!(id.supports_ffb, "DFP must support FFB");
    assert_eq!(id.max_torque_nm, Some(20.0), "DFP torque must be 20 Nm");
    Ok(())
}

/// R295 is classified as a wheelbase with FFB support.
#[test]
fn r295_is_wheelbase() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        is_wheelbase_product(product_ids::R295),
        "R295 must be classified as a wheelbase"
    );
    let id = identify_device(product_ids::R295);
    assert!(id.supports_ffb, "R295 must support FFB");
    Ok(())
}

/// Wheelbases: DFP, DFP V2, R295.
#[test]
fn wheelbase_pids_classified_correctly() -> Result<(), Box<dyn std::error::Error>> {
    assert!(is_wheelbase_product(product_ids::DIRECTFORCE_PRO));
    assert!(is_wheelbase_product(product_ids::DIRECTFORCE_PRO_V2));
    assert!(is_wheelbase_product(product_ids::R295));
    Ok(())
}

/// Non-wheelbases: pedals, handbrake, shifter.
#[test]
fn non_wheelbase_pids_classified_correctly() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!is_wheelbase_product(product_ids::PEDALS));
    assert!(!is_wheelbase_product(product_ids::PEDALS_V1));
    assert!(!is_wheelbase_product(product_ids::PEDALS_V2));
    assert!(!is_wheelbase_product(product_ids::HANDBRAKE));
    assert!(!is_wheelbase_product(product_ids::SHIFTER));
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 3. PIDFF report format constants
// ════════════════════════════════════════════════════════════════════════════

/// Report sizes match the PIDFF specification expectations.
#[test]
fn report_sizes() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        CONSTANT_FORCE_REPORT_LEN, 8,
        "constant force report = 8 bytes"
    );
    assert_eq!(SPRING_REPORT_LEN, 10, "spring report = 10 bytes");
    assert_eq!(DAMPER_REPORT_LEN, 8, "damper report = 8 bytes");
    assert_eq!(FRICTION_REPORT_LEN, 10, "friction report = 10 bytes");
    Ok(())
}

/// PIDFF report IDs match the USB HID PID specification.
/// Source [5]: `pid1_01.pdf` — standard report ID assignments.
#[test]
fn pidff_report_ids() -> Result<(), Box<dyn std::error::Error>> {
    // These report IDs are from ids.rs::report_ids
    // Constant Force = 0x11 per HID PID spec
    // Spring = 0x19, Damper = 0x1A, Friction = 0x1B
    // We verify by encoding and checking byte 0 of the output.
    let cf_enc = VrsConstantForceEncoder::new(20.0);
    let mut cf_out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    cf_enc.encode(0.0, &mut cf_out);
    assert_eq!(cf_out[0], 0x11, "constant force report ID must be 0x11");

    let sp_enc = VrsSpringEncoder::new(20.0);
    let mut sp_out = [0u8; SPRING_REPORT_LEN];
    sp_enc.encode(0, 0, 0, 0, &mut sp_out);
    assert_eq!(sp_out[0], 0x19, "spring report ID must be 0x19");

    let dm_enc = VrsDamperEncoder::new(20.0);
    let mut dm_out = [0u8; DAMPER_REPORT_LEN];
    dm_enc.encode(0, 0, &mut dm_out);
    assert_eq!(dm_out[0], 0x1A, "damper report ID must be 0x1A");

    let fr_enc = VrsFrictionEncoder::new(20.0);
    let mut fr_out = [0u8; FRICTION_REPORT_LEN];
    fr_enc.encode(0, 0, &mut fr_out);
    assert_eq!(fr_out[0], 0x1B, "friction report ID must be 0x1B");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 4. Constant force encoding verification
// ════════════════════════════════════════════════════════════════════════════

/// Zero torque must encode as magnitude 0.
#[test]
fn encode_zero_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode_zero(&mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 0, "zero torque must encode as magnitude 0");
    Ok(())
}

/// Half torque (10 Nm out of 20 Nm max) → magnitude = 5000.
#[test]
fn encode_half_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(10.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 5000, "10 Nm / 20 Nm max = 0.5 × 10000 = 5000");
    Ok(())
}

/// Full torque (20 Nm out of 20 Nm max) → magnitude = 10000.
#[test]
fn encode_full_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(20.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 10000, "20 Nm / 20 Nm max = 1.0 × 10000 = 10000");
    Ok(())
}

/// Over-range torque must saturate at ±10000.
#[test]
fn encode_torque_saturates() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(100.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 10000, "over-range must saturate to 10000");

    enc.encode(-100.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, -10000, "under-range must saturate to -10000");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 5. Control report format verification
// ════════════════════════════════════════════════════════════════════════════

/// Rotation range report: byte 0 = 0x0C (SET_REPORT), bytes 2–3 = degrees LE.
#[test]
fn rotation_range_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_rotation_range(900);
    assert_eq!(report[0], 0x0C, "rotation range report ID must be 0x0C");
    let degrees = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(degrees, 900, "900° must encode correctly");

    let report = build_rotation_range(1080);
    let degrees = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(degrees, 1080, "1080° must encode correctly");
    Ok(())
}

/// Device gain report: byte 0 = 0x0C, byte 1 = 0x01, byte 2 = gain.
#[test]
fn device_gain_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_device_gain(0x80);
    assert_eq!(report[0], 0x0C, "gain report ID must be 0x0C");
    assert_eq!(report[1], 0x01, "gain sub-command must be 0x01");
    assert_eq!(report[2], 0x80, "gain value must be 0x80");
    Ok(())
}

/// FFB enable report: byte 0 = 0x0B (DEVICE_CONTROL), byte 1 = 0x01/0x00.
#[test]
fn ffb_enable_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let on = build_ffb_enable(true);
    assert_eq!(on[0], 0x0B, "FFB enable report ID must be 0x0B");
    assert_eq!(on[1], 0x01, "enable flag must be 0x01");

    let off = build_ffb_enable(false);
    assert_eq!(off[0], 0x0B, "FFB disable report ID must be 0x0B");
    assert_eq!(off[1], 0x00, "disable flag must be 0x00");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 6. Spring, damper, friction encoding
// ════════════════════════════════════════════════════════════════════════════

/// Spring encoder zero output: all coefficients zero, report ID 0x19.
#[test]
fn spring_zero_output() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsSpringEncoder::new(20.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(out[0], 0x19, "spring report ID");
    assert_eq!(out[1], 1, "effect block index");
    // All data bytes should be 0
    for (i, &byte) in out.iter().enumerate().skip(2) {
        assert_eq!(byte, 0, "byte {i} must be 0 for zero spring");
    }
    Ok(())
}

/// Damper encoder zero output: report ID 0x1A.
#[test]
fn damper_zero_output() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsDamperEncoder::new(20.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(out[0], 0x1A, "damper report ID");
    Ok(())
}

/// Friction encoder zero output: report ID 0x1B.
#[test]
fn friction_zero_output() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsFrictionEncoder::new(20.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(out[0], 0x1B, "friction report ID");
    Ok(())
}
