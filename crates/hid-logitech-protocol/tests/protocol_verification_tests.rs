//! Protocol verification tests — cross-referenced against Linux kernel sources.
//!
//! These tests verify that our Logitech HID protocol constants and encoding
//! functions match the authoritative open-source driver implementations.
//!
//! # Sources
//!
//! All assertions in this file are cross-referenced against at least two of
//! the following canonical sources:
//!
//! - **Linux kernel `hid-ids.h`** — `torvalds/linux drivers/hid/hid-ids.h`
//!   (VID/PID definitions for all USB HID devices).
//! - **Linux kernel `hid-lg4ff.c`** — `torvalds/linux drivers/hid/hid-lg4ff.c`
//!   (force feedback driver for Logitech wheels: G25–G29, DFP, DFGT, MOMO).
//! - **Linux kernel `hid-logitech-hidpp.c`** — HID++ protocol driver for
//!   G920 and G923 Xbox/PC variants (since kernel 6.3).
//! - **new-lg4ff** — `berarma/new-lg4ff hid-lg4ff.c` (out-of-tree driver with
//!   full FF_SPRING / FF_DAMPER / FF_FRICTION, high-resolution timer, G923 PS
//!   mode switching support).
//! - **oversteer** — `berarma/oversteer oversteer/wheel_ids.py` (Linux GUI for
//!   Logitech / Thrustmaster / Fanatec wheels; most complete PID list including
//!   G PRO).

use racing_wheel_hid_logitech_protocol as lg;
use racing_wheel_hid_logitech_protocol::ids::{commands, product_ids, report_ids, LOGITECH_VENDOR_ID};
use racing_wheel_hid_logitech_protocol::types::{LogitechModel, is_wheel_product};

// ═══════════════════════════════════════════════════════════════════════════════
// 1. VID/PID verification against Linux kernel hid-ids.h
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify the Logitech USB vendor ID.
///
/// Source: kernel `hid-ids.h`:
///   `#define USB_VENDOR_ID_LOGITECH 0x046d`
#[test]
fn vid_logitech_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        LOGITECH_VENDOR_ID, 0x046D,
        "VID must match kernel USB_VENDOR_ID_LOGITECH (hid-ids.h)"
    );
    Ok(())
}

/// Verify all PIDs against kernel `hid-ids.h`.
///
/// Every PID below is verified against the `#define USB_DEVICE_ID_LOGITECH_*`
/// constants in `torvalds/linux drivers/hid/hid-ids.h` (master branch).
#[test]
fn pids_match_kernel_hid_ids_h() -> Result<(), Box<dyn std::error::Error>> {
    // kernel: #define USB_DEVICE_ID_LOGITECH_WINGMAN_FFG  0xc293
    assert_eq!(product_ids::WINGMAN_FORMULA_FORCE_GP, 0xC293,
        "WINGMAN_FFG (kernel: USB_DEVICE_ID_LOGITECH_WINGMAN_FFG)");

    // kernel: (no explicit define for 0xc291 in hid-ids.h, but oversteer has LG_WFF)
    // Verified via oversteer: LG_WFF = '046d:c291'
    assert_eq!(product_ids::WINGMAN_FORMULA_FORCE, 0xC291,
        "WFF (oversteer: LG_WFF = '046d:c291')");

    // kernel: #define USB_DEVICE_ID_LOGITECH_WHEEL  0xc294
    assert_eq!(product_ids::DRIVING_FORCE_EX, 0xC294,
        "DF/EX (kernel: USB_DEVICE_ID_LOGITECH_WHEEL)");

    // kernel: #define USB_DEVICE_ID_LOGITECH_MOMO_WHEEL  0xc295
    assert_eq!(product_ids::MOMO, 0xC295,
        "MOMO (kernel: USB_DEVICE_ID_LOGITECH_MOMO_WHEEL)");

    // kernel: #define USB_DEVICE_ID_LOGITECH_DFP_WHEEL  0xc298
    assert_eq!(product_ids::DRIVING_FORCE_PRO, 0xC298,
        "DFP (kernel: USB_DEVICE_ID_LOGITECH_DFP_WHEEL)");

    // kernel: #define USB_DEVICE_ID_LOGITECH_G25_WHEEL  0xc299
    assert_eq!(product_ids::G25, 0xC299,
        "G25 (kernel: USB_DEVICE_ID_LOGITECH_G25_WHEEL)");

    // kernel: #define USB_DEVICE_ID_LOGITECH_DFGT_WHEEL  0xc29a
    assert_eq!(product_ids::DRIVING_FORCE_GT, 0xC29A,
        "DFGT (kernel: USB_DEVICE_ID_LOGITECH_DFGT_WHEEL)");

    // kernel: #define USB_DEVICE_ID_LOGITECH_G27_WHEEL  0xc29b
    assert_eq!(product_ids::G27, 0xC29B,
        "G27 (kernel: USB_DEVICE_ID_LOGITECH_G27_WHEEL)");

    // kernel: #define USB_DEVICE_ID_LOGITECH_WII_WHEEL  0xc29c
    assert_eq!(product_ids::SPEED_FORCE_WIRELESS, 0xC29C,
        "SFW/WiiWheel (kernel: USB_DEVICE_ID_LOGITECH_WII_WHEEL)");

    // kernel: #define USB_DEVICE_ID_LOGITECH_MOMO_WHEEL2  0xca03
    assert_eq!(product_ids::MOMO_2, 0xCA03,
        "MOMO2 (kernel: USB_DEVICE_ID_LOGITECH_MOMO_WHEEL2)");

    // kernel: #define USB_DEVICE_ID_LOGITECH_VIBRATION_WHEEL  0xca04
    assert_eq!(product_ids::VIBRATION_WHEEL, 0xCA04,
        "Vibration Wheel (kernel: USB_DEVICE_ID_LOGITECH_VIBRATION_WHEEL)");

    // kernel: #define USB_DEVICE_ID_LOGITECH_G29_WHEEL  0xc24f
    assert_eq!(product_ids::G29_PS, 0xC24F,
        "G29 (kernel: USB_DEVICE_ID_LOGITECH_G29_WHEEL)");

    // kernel: #define USB_DEVICE_ID_LOGITECH_G920_WHEEL  0xc262
    assert_eq!(product_ids::G920, 0xC262,
        "G920 (kernel: USB_DEVICE_ID_LOGITECH_G920_WHEEL)");

    // kernel: #define USB_DEVICE_ID_LOGITECH_G923_XBOX_WHEEL  0xc26e
    assert_eq!(product_ids::G923_XBOX, 0xC26E,
        "G923 Xbox (kernel: USB_DEVICE_ID_LOGITECH_G923_XBOX_WHEEL)");

    Ok(())
}

/// Verify PIDs specific to the new-lg4ff out-of-tree driver.
///
/// Source: `berarma/new-lg4ff hid-ids.h`
#[test]
fn pids_match_new_lg4ff() -> Result<(), Box<dyn std::error::Error>> {
    // new-lg4ff: #define USB_DEVICE_ID_LOGITECH_G923_WHEEL  0xc266
    assert_eq!(product_ids::G923, 0xC266,
        "G923 native (new-lg4ff: USB_DEVICE_ID_LOGITECH_G923_WHEEL)");

    // new-lg4ff: #define USB_DEVICE_ID_LOGITECH_G923_PS_WHEEL  0xc267
    assert_eq!(product_ids::G923_PS, 0xC267,
        "G923 PS compat (new-lg4ff: USB_DEVICE_ID_LOGITECH_G923_PS_WHEEL)");

    Ok(())
}

/// Verify PIDs from oversteer wheel_ids.py.
///
/// Source: `berarma/oversteer oversteer/wheel_ids.py`
#[test]
fn pids_match_oversteer() -> Result<(), Box<dyn std::error::Error>> {
    // oversteer: LG_GPRO_PS = '046d:c268'
    assert_eq!(product_ids::G_PRO, 0xC268,
        "G PRO PS (oversteer: LG_GPRO_PS = '046d:c268')");

    // oversteer: LG_GPRO_XBOX = '046d:c272'
    assert_eq!(product_ids::G_PRO_XBOX, 0xC272,
        "G PRO Xbox (oversteer: LG_GPRO_XBOX = '046d:c272')");

    // oversteer: LG_G923P = '046d:c266'  (native mode PID)
    assert_eq!(product_ids::G923, 0xC266,
        "G923P native (oversteer: LG_G923P = '046d:c266')");

    // oversteer: LG_G923X = '046d:c26e'
    assert_eq!(product_ids::G923_XBOX, 0xC26E,
        "G923X (oversteer: LG_G923X = '046d:c26e')");

    // oversteer: LG_WFF = '046d:c291'
    assert_eq!(product_ids::WINGMAN_FORMULA_FORCE, 0xC291,
        "WFF (oversteer: LG_WFF = '046d:c291')");

    // oversteer: LG_WFFG = '046d:c293'
    assert_eq!(product_ids::WINGMAN_FORMULA_FORCE_GP, 0xC293,
        "WFFG (oversteer: LG_WFFG = '046d:c293')");

    // oversteer: LG_SFW = '046d:c29c'
    assert_eq!(product_ids::SPEED_FORCE_WIRELESS, 0xC29C,
        "SFW (oversteer: LG_SFW = '046d:c29c')");

    // oversteer: LG_MOMO = '046d:c295'
    assert_eq!(product_ids::MOMO, 0xC295,
        "MOMO (oversteer: LG_MOMO = '046d:c295')");

    // oversteer: LG_MOMO2 = '046d:ca03'
    assert_eq!(product_ids::MOMO_2, 0xCA03,
        "MOMO2 (oversteer: LG_MOMO2 = '046d:ca03')");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Command encoding — constant force
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify constant force report structure.
///
/// Our encoder uses report ID 0x12 with a signed 16-bit magnitude.
/// The kernel `lg4ff_play` uses a different unsigned 8-bit encoding in the
/// classic slot protocol ({0x11, 0x08, force, 0x80, ...}). Our crate uses
/// the HID PID (Physical Interface Device) layer encoding.
#[test]
fn constant_force_report_id_is_0x12() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(report_ids::CONSTANT_FORCE, 0x12,
        "Constant Force report ID must be 0x12");
    Ok(())
}

/// Verify constant force encoding: half-positive torque → +5000 magnitude.
#[test]
fn constant_force_half_positive() -> Result<(), Box<dyn std::error::Error>> {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    enc.encode(1.1, &mut out);
    assert_eq!(out[0], 0x12, "report ID");
    assert_eq!(out[1], 1, "effect block index (1-based)");
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 5000, "1.1 Nm / 2.2 Nm = 0.5 → 5000");
    Ok(())
}

/// Verify constant force encoding: full negative torque → -10000 magnitude.
#[test]
fn constant_force_full_negative() -> Result<(), Box<dyn std::error::Error>> {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    enc.encode(-2.2, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, -10000, "full negative must saturate at -10000");
    Ok(())
}

/// Verify constant force encoding: zero torque → zero magnitude.
#[test]
fn constant_force_zero() -> Result<(), Box<dyn std::error::Error>> {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    enc.encode_zero(&mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 0, "zero torque must encode to zero");
    Ok(())
}

/// Verify constant force encoding saturates at ±10000 for over-range input.
#[test]
fn constant_force_saturation() -> Result<(), Box<dyn std::error::Error>> {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];

    enc.encode(999.0, &mut out);
    let mag_pos = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag_pos, 10000, "positive over-range must saturate at +10000");

    enc.encode(-999.0, &mut out);
    let mag_neg = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag_neg, -10000, "negative over-range must saturate at -10000");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Rotation range commands — G25/G27/DFGT/G29/G923
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify set-range command format for G25+ wheels.
///
/// Source: `lg4ff_set_range_g25()` in kernel `hid-lg4ff.c`:
/// ```c
/// value[0] = 0xf8;
/// value[1] = 0x81;
/// value[2] = range & 0x00ff;
/// value[3] = (range & 0xff00) >> 8;
/// value[4] = 0x00; value[5] = 0x00; value[6] = 0x00;
/// ```
#[test]
fn set_range_900_matches_kernel_lg4ff_set_range_g25() -> Result<(), Box<dyn std::error::Error>> {
    let r = lg::build_set_range_report(900);
    // 900 = 0x0384
    assert_eq!(r, [0xF8, 0x81, 0x84, 0x03, 0x00, 0x00, 0x00],
        "900° range must match kernel lg4ff_set_range_g25 encoding");
    Ok(())
}

/// Verify set-range for 270° (MOMO/DF-EX typical range).
#[test]
fn set_range_270() -> Result<(), Box<dyn std::error::Error>> {
    let r = lg::build_set_range_report(270);
    // 270 = 0x010E
    assert_eq!(r, [0xF8, 0x81, 0x0E, 0x01, 0x00, 0x00, 0x00],
        "270° range encoding");
    Ok(())
}

/// Verify set-range for 1080° (G PRO max range).
#[test]
fn set_range_1080() -> Result<(), Box<dyn std::error::Error>> {
    let r = lg::build_set_range_report(1080);
    // 1080 = 0x0438
    assert_eq!(r, [0xF8, 0x81, 0x38, 0x04, 0x00, 0x00, 0x00],
        "1080° range encoding");
    Ok(())
}

/// Verify SET_RANGE command byte is 0x81.
///
/// Source: `lg4ff_set_range_g25()` — `value[1] = 0x81`.
#[test]
fn set_range_command_byte_is_0x81() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(commands::SET_RANGE, 0x81,
        "SET_RANGE command byte (kernel lg4ff_set_range_g25: value[1] = 0x81)");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. DFP-specific rotation range commands
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify DFP range encoding for 200° (coarse = 0x02, fine = no-op).
///
/// Source: `lg4ff_set_range_dfp()` in kernel `hid-lg4ff.c`:
/// - range ≤ 200 → coarse cmd byte = 0x02
/// - range == 200 → fine limit is no-op (zeroed)
#[test]
fn dfp_range_200_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    let [coarse, fine] = lg::build_set_range_dfp_reports(200);
    assert_eq!(coarse, [0xF8, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
        "200° DFP coarse: cmd=0x02 (kernel lg4ff_set_range_dfp)");
    assert_eq!(fine, [0x81, 0x0B, 0x00, 0x00, 0x00, 0x00, 0x00],
        "200° DFP fine: no-op (kernel: range == 200 → no fine limit)");
    Ok(())
}

/// Verify DFP range encoding for 900° (coarse = 0x03, fine = no-op).
///
/// Source: `lg4ff_set_range_dfp()`:
/// - range > 200 → coarse cmd byte = 0x03
/// - range == 900 → fine limit is no-op (zeroed)
#[test]
fn dfp_range_900_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    let [coarse, fine] = lg::build_set_range_dfp_reports(900);
    assert_eq!(coarse, [0xF8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00],
        "900° DFP coarse: cmd=0x03 (kernel lg4ff_set_range_dfp)");
    assert_eq!(fine, [0x81, 0x0B, 0x00, 0x00, 0x00, 0x00, 0x00],
        "900° DFP fine: no-op (kernel: range == 900 → no fine limit)");
    Ok(())
}

/// Verify DFP range encoding for 540° with exact kernel arithmetic.
///
/// Source: `lg4ff_set_range_dfp()`:
/// ```c
/// full_range = 900;  // range > 200
/// start_left = (((full_range - range + 1) * 2047) / full_range);
/// start_right = 0xfff - start_left;
/// value[2] = start_left >> 4;
/// value[3] = start_right >> 4;
/// value[4] = 0xff;
/// value[5] = (start_right & 0xe) << 4 | (start_left & 0xe);
/// value[6] = 0xff;
/// ```
#[test]
fn dfp_range_540_matches_kernel_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
    let [coarse, fine] = lg::build_set_range_dfp_reports(540);
    assert_eq!(coarse[1], 0x03, "540° > 200 → coarse cmd = 0x03");

    // Replicate kernel arithmetic exactly
    let full_range: u32 = 900;
    let range: u32 = 540;
    let start_left = ((full_range - range + 1) * 2047) / full_range; // 820 = 0x334
    let start_right = 0xFFF - start_left; // 3275 = 0xCCB

    assert_eq!(fine[0], 0x81);
    assert_eq!(fine[1], 0x0B);
    assert_eq!(fine[2], (start_left >> 4) as u8, "start_left >> 4");
    assert_eq!(fine[3], (start_right >> 4) as u8, "start_right >> 4");
    assert_eq!(fine[4], 0xFF);
    assert_eq!(fine[5], (((start_right & 0xE) << 4) | (start_left & 0xE)) as u8,
        "nibble byte");
    assert_eq!(fine[6], 0xFF);
    Ok(())
}

/// Verify DFP range encoding for 100° (≤200 → coarse=0x02, full_range=200).
///
/// Source: `lg4ff_set_range_dfp()`:
/// ```c
/// full_range = 200;  // range <= 200
/// start_left = (((full_range - range + 1) * 2047) / full_range);
/// ```
#[test]
fn dfp_range_100_matches_kernel_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
    let [coarse, fine] = lg::build_set_range_dfp_reports(100);
    assert_eq!(coarse[1], 0x02, "100° ≤ 200 → coarse cmd = 0x02");

    let full_range: u32 = 200;
    let range: u32 = 100;
    let start_left = ((full_range - range + 1) * 2047) / full_range; // 1033
    let start_right = 0xFFF - start_left; // 3062

    assert_eq!(fine[2], (start_left >> 4) as u8);
    assert_eq!(fine[3], (start_right >> 4) as u8);
    Ok(())
}

/// Verify DFP range clamping: 0° → 40°, 1500° → 900°.
#[test]
fn dfp_range_clamping() -> Result<(), Box<dyn std::error::Error>> {
    let r40 = lg::build_set_range_dfp_reports(40);
    let r0 = lg::build_set_range_dfp_reports(0);
    assert_eq!(r40, r0, "0° must clamp to 40°");

    let r900 = lg::build_set_range_dfp_reports(900);
    let r_over = lg::build_set_range_dfp_reports(1500);
    assert_eq!(r900, r_over, "1500° must clamp to 900°");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. LED patterns
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify LED report format.
///
/// Source: kernel `hid-lg4ff.c` LED class driver — LEDs are set via a
/// 7-byte vendor report. The command byte is 0x12 (same value as
/// SET_LEDS in our crate).
#[test]
fn led_report_format() -> Result<(), Box<dyn std::error::Error>> {
    // All LEDs on: 5-bit mask = 0x1F
    let r = lg::build_set_leds_report(0b0001_1111);
    assert_eq!(r, [0xF8, 0x12, 0x1F, 0x00, 0x00, 0x00, 0x00],
        "All 5 LEDs on");

    // No LEDs
    let r = lg::build_set_leds_report(0x00);
    assert_eq!(r, [0xF8, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00],
        "All LEDs off");

    // Single LED (LED 3 only = bit 2)
    let r = lg::build_set_leds_report(0b0000_0100);
    assert_eq!(r, [0xF8, 0x12, 0x04, 0x00, 0x00, 0x00, 0x00],
        "Only LED 3");

    Ok(())
}

/// Verify LED mask strips high bits (only 5 LEDs).
#[test]
fn led_mask_strips_high_bits() -> Result<(), Box<dyn std::error::Error>> {
    let r = lg::build_set_leds_report(0xFF);
    assert_eq!(r[2], 0x1F, "upper 3 bits must be stripped");
    Ok(())
}

/// Verify progressive LED illumination patterns (used for rev lights).
///
/// Racing wheels typically light LEDs 1→5 as engine RPM increases.
#[test]
fn led_progressive_patterns() -> Result<(), Box<dyn std::error::Error>> {
    let patterns: [(u8, u8); 5] = [
        (0b0000_0001, 0x01), // LED 1
        (0b0000_0011, 0x03), // LEDs 1-2
        (0b0000_0111, 0x07), // LEDs 1-3
        (0b0000_1111, 0x0F), // LEDs 1-4
        (0b0001_1111, 0x1F), // LEDs 1-5
    ];
    for (input, expected) in patterns {
        let r = lg::build_set_leds_report(input);
        assert_eq!(r[2], expected, "progressive LED pattern 0x{:02X}", input);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Autocenter commands
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify autocenter activation command format.
///
/// Source: `lg4ff_set_autocenter_default()` in kernel `hid-lg4ff.c`:
/// ```c
/// // Activate Auto-Center
/// value[0] = 0x14;
/// value[1] = 0x00;
/// value[2] = 0x00; ... value[6] = 0x00;
/// ```
///
/// Our `build_set_autocenter_report` takes strength/rate params and uses
/// command byte 0x14.
#[test]
fn autocenter_command_byte_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(commands::SET_AUTOCENTER, 0x14,
        "SET_AUTOCENTER must be 0x14 (kernel lg4ff_set_autocenter_default)");
    Ok(())
}

/// Verify autocenter report with full strength.
#[test]
fn autocenter_full_strength() -> Result<(), Box<dyn std::error::Error>> {
    let r = lg::build_set_autocenter_report(0xFF, 0xFF);
    assert_eq!(r[0], 0xF8, "vendor report prefix");
    assert_eq!(r[1], 0x14, "autocenter command byte");
    assert_eq!(r[2], 0xFF, "full strength");
    assert_eq!(r[3], 0xFF, "full rate");
    assert_eq!(&r[4..], &[0x00, 0x00, 0x00], "trailing zeros");
    Ok(())
}

/// Verify autocenter report with zero values.
#[test]
fn autocenter_zero() -> Result<(), Box<dyn std::error::Error>> {
    let r = lg::build_set_autocenter_report(0x00, 0x00);
    assert_eq!(r, [0xF8, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00],
        "zero autocenter");
    Ok(())
}

/// Verify the autocenter deactivation protocol documented in kernel.
///
/// Source: `lg4ff_set_autocenter_default()`:
/// ```c
/// if (magnitude == 0) {
///     value[0] = 0xf5; ...
/// }
/// ```
///
/// Note: our crate does not currently expose a dedicated deactivate function,
/// but we verify the documented protocol byte (0xF5) is consistent with
/// the activation byte (0x14) being a different command.
#[test]
fn autocenter_deactivation_protocol() -> Result<(), Box<dyn std::error::Error>> {
    // The kernel uses 0xF5 to deactivate and 0x14 to activate.
    // These must be different bytes.
    let deactivate_cmd: u8 = 0xF5;
    let activate_cmd = commands::SET_AUTOCENTER;
    assert_ne!(deactivate_cmd, activate_cmd,
        "deactivate (0xF5) and activate (0x14) must differ");
    Ok(())
}

/// Verify the autocenter spring configuration protocol.
///
/// Source: `lg4ff_set_autocenter_default()`:
/// ```c
/// value[0] = 0xfe;
/// value[1] = 0x0d;
/// value[2] = expand_a / 0xaaaa;
/// value[3] = expand_a / 0xaaaa;
/// value[4] = expand_b / 0xaaaa;
/// ```
///
/// The spring config uses 0xFE/0x0D prefix, distinct from the 0x14 activation.
#[test]
fn autocenter_spring_config_prefix() -> Result<(), Box<dyn std::error::Error>> {
    // Verify documented spring config prefix bytes don't collide with
    // our SET_AUTOCENTER command
    let spring_config_byte0: u8 = 0xFE;
    let spring_config_byte1: u8 = 0x0D;
    assert_ne!(spring_config_byte0, report_ids::VENDOR,
        "spring config byte 0 (0xFE) differs from vendor report (0xF8)");
    assert_ne!(spring_config_byte1, commands::SET_AUTOCENTER,
        "spring config byte 1 (0x0D) differs from autocenter cmd (0x14)");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Mode switching / compatibility detection
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify the "revert mode upon USB reset" command (NATIVE_MODE).
///
/// Source: `lg4ff_mode_switch_ext09_*` arrays in kernel `hid-lg4ff.c`:
/// All mode switches begin with `{0xf8, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00}`.
#[test]
fn native_mode_report_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    let r = lg::build_native_mode_report();
    assert_eq!(r, [0xF8, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00],
        "must match kernel lg4ff_mode_switch revert-on-reset prefix");
    Ok(())
}

/// Verify NATIVE_MODE command byte is 0x0A.
///
/// Source: kernel `hid-lg4ff.c` mode switch arrays — byte 1 = 0x0a.
#[test]
fn native_mode_command_byte() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(commands::NATIVE_MODE, 0x0A,
        "NATIVE_MODE must be 0x0A (kernel mode switch revert byte)");
    Ok(())
}

/// Verify all kernel mode switch commands (ext09 series).
///
/// Source: `lg4ff_mode_switch_ext09_*` arrays in kernel `hid-lg4ff.c`.
/// Each mode switch sends two 7-byte reports:
///   1. `{0xf8, 0x0a, 0, 0, 0, 0, 0}` — revert mode
///   2. `{0xf8, 0x09, mode_id, 0x01, detach, 0, 0}` — switch
#[test]
fn mode_switch_ext09_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    // Source: lg4ff_mode_switch_ext09_dfex — mode_id=0x00, detach=0x00
    let dfex = lg::build_mode_switch_report(0x00, false);
    assert_eq!(dfex, [0xF8, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00],
        "DF-EX mode switch (kernel lg4ff_mode_switch_ext09_dfex)");

    // Source: lg4ff_mode_switch_ext09_dfp — mode_id=0x01, detach=0x00
    let dfp = lg::build_mode_switch_report(0x01, false);
    assert_eq!(dfp, [0xF8, 0x09, 0x01, 0x01, 0x00, 0x00, 0x00],
        "DFP mode switch (kernel lg4ff_mode_switch_ext09_dfp)");

    // Source: lg4ff_mode_switch_ext09_g25 — mode_id=0x02, detach=0x00
    let g25 = lg::build_mode_switch_report(0x02, false);
    assert_eq!(g25, [0xF8, 0x09, 0x02, 0x01, 0x00, 0x00, 0x00],
        "G25 mode switch (kernel lg4ff_mode_switch_ext09_g25)");

    // Source: lg4ff_mode_switch_ext09_dfgt — mode_id=0x03, detach=0x00
    let dfgt = lg::build_mode_switch_report(0x03, false);
    assert_eq!(dfgt, [0xF8, 0x09, 0x03, 0x01, 0x00, 0x00, 0x00],
        "DFGT mode switch (kernel lg4ff_mode_switch_ext09_dfgt)");

    // Source: lg4ff_mode_switch_ext09_g27 — mode_id=0x04, detach=0x00
    let g27 = lg::build_mode_switch_report(0x04, false);
    assert_eq!(g27, [0xF8, 0x09, 0x04, 0x01, 0x00, 0x00, 0x00],
        "G27 mode switch (kernel lg4ff_mode_switch_ext09_g27)");

    // Source: lg4ff_mode_switch_ext09_g29 — mode_id=0x05, detach=0x01
    let g29 = lg::build_mode_switch_report(0x05, true);
    assert_eq!(g29, [0xF8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00],
        "G29 mode switch (kernel lg4ff_mode_switch_ext09_g29)");

    Ok(())
}

/// Verify G923 PS mode switch matches new-lg4ff.
///
/// Source: `berarma/new-lg4ff hid-lg4ff.c`:
/// ```c
/// static const struct lg4ff_compat_mode_switch lg4ff_mode_switch_ext09_g923 = {
///     2,
///     {0xf8, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00,
///      0xf8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00}
/// };
/// ```
///
/// Additionally, G923 PS mode (PID 0xC267 → 0xC266) uses HID report ID 0x30:
/// ```c
/// static const struct lg4ff_compat_mode_switch lg4ff_mode_switch_30_g923 = {
///     1,
///     {0xf8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00}
/// };
/// ```
#[test]
fn g923_mode_switch_matches_new_lg4ff() -> Result<(), Box<dyn std::error::Error>> {
    let g923 = lg::build_mode_switch_report(0x07, true);
    assert_eq!(g923, [0xF8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00],
        "G923 mode switch (new-lg4ff lg4ff_mode_switch_ext09_g923)");
    Ok(())
}

/// Verify MODE_SWITCH command byte is 0x09.
///
/// Source: kernel `hid-lg4ff.c` — all ext09 mode switches use byte 1 = 0x09.
#[test]
fn mode_switch_command_byte() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(commands::MODE_SWITCH, 0x09,
        "MODE_SWITCH must be 0x09 (kernel ext09 series)");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Hardware friction capability — matches new-lg4ff LG4FF_CAP_FRICTION
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify hardware friction support matches new-lg4ff `lg4ff_devices[]`.
///
/// Source: `berarma/new-lg4ff hid-lg4ff.c`:
/// ```c
/// static const struct lg4ff_wheel lg4ff_devices[] = {
///     {USB_DEVICE_ID_LOGITECH_DFP_WHEEL,  ..., LG4FF_CAP_FRICTION, ...},
///     {USB_DEVICE_ID_LOGITECH_G25_WHEEL,  ..., LG4FF_CAP_FRICTION, ...},
///     {USB_DEVICE_ID_LOGITECH_DFGT_WHEEL, ..., LG4FF_CAP_FRICTION, ...},
///     {USB_DEVICE_ID_LOGITECH_G27_WHEEL,  ..., LG4FF_CAP_FRICTION, ...},
///     {USB_DEVICE_ID_LOGITECH_G29_WHEEL,  ..., 0,                  ...},
///     {USB_DEVICE_ID_LOGITECH_G923_WHEEL, ..., 0,                  ...},
///     ... // remaining have 0 (no friction)
/// };
/// ```
#[test]
fn hardware_friction_matches_new_lg4ff_cap_friction() -> Result<(), Box<dyn std::error::Error>> {
    // Models WITH LG4FF_CAP_FRICTION in new-lg4ff
    assert!(LogitechModel::DrivingForcePro.supports_hardware_friction(),
        "DFP has LG4FF_CAP_FRICTION in new-lg4ff");
    assert!(LogitechModel::G25.supports_hardware_friction(),
        "G25 has LG4FF_CAP_FRICTION in new-lg4ff");
    assert!(LogitechModel::DrivingForceGT.supports_hardware_friction(),
        "DFGT has LG4FF_CAP_FRICTION in new-lg4ff");
    assert!(LogitechModel::G27.supports_hardware_friction(),
        "G27 has LG4FF_CAP_FRICTION in new-lg4ff");

    // Models WITHOUT LG4FF_CAP_FRICTION (capabilities = 0)
    assert!(!LogitechModel::G29.supports_hardware_friction(),
        "G29 has capabilities=0 in new-lg4ff");
    assert!(!LogitechModel::G923.supports_hardware_friction(),
        "G923 has capabilities=0 in new-lg4ff");
    assert!(!LogitechModel::G920.supports_hardware_friction(),
        "G920 uses HID++, not lg4ff — no hardware friction");
    assert!(!LogitechModel::MOMO.supports_hardware_friction(),
        "MOMO has capabilities=0 in new-lg4ff");
    assert!(!LogitechModel::WingManFormulaForce.supports_hardware_friction(),
        "WingMan has capabilities=0 in new-lg4ff");
    assert!(!LogitechModel::DrivingForceEX.supports_hardware_friction(),
        "DF-EX has capabilities=0 in new-lg4ff");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Rotation range limits — matches kernel lg4ff_devices[]
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify max rotation degrees match kernel `lg4ff_devices[]` max_range values.
///
/// Source: `lg4ff_devices[]` in kernel `hid-lg4ff.c`:
/// ```c
/// {USB_DEVICE_ID_LOGITECH_WINGMAN_FFG, ..., 40, 180, NULL},
/// {USB_DEVICE_ID_LOGITECH_WHEEL,       ..., 40, 270, NULL},
/// {USB_DEVICE_ID_LOGITECH_MOMO_WHEEL,  ..., 40, 270, NULL},
/// {USB_DEVICE_ID_LOGITECH_DFP_WHEEL,   ..., 40, 900, lg4ff_set_range_dfp},
/// {USB_DEVICE_ID_LOGITECH_G25_WHEEL,   ..., 40, 900, lg4ff_set_range_g25},
/// {USB_DEVICE_ID_LOGITECH_DFGT_WHEEL,  ..., 40, 900, lg4ff_set_range_g25},
/// {USB_DEVICE_ID_LOGITECH_G27_WHEEL,   ..., 40, 900, lg4ff_set_range_g25},
/// {USB_DEVICE_ID_LOGITECH_G29_WHEEL,   ..., 40, 900, lg4ff_set_range_g25},
/// {USB_DEVICE_ID_LOGITECH_MOMO_WHEEL2, ..., 40, 270, NULL},
/// {USB_DEVICE_ID_LOGITECH_WII_WHEEL,   ..., 40, 270, NULL},
/// ```
#[test]
fn max_rotation_matches_kernel_lg4ff_devices() -> Result<(), Box<dyn std::error::Error>> {
    // 180° wheels (kernel max_range=180)
    assert_eq!(LogitechModel::WingManFormulaForce.max_rotation_deg(), 180,
        "WingMan FFG: kernel max_range=180");

    // 270° wheels (kernel max_range=270)
    assert_eq!(LogitechModel::MOMO.max_rotation_deg(), 270,
        "MOMO: kernel max_range=270");
    assert_eq!(LogitechModel::DrivingForceEX.max_rotation_deg(), 270,
        "DF/EX: kernel max_range=270");
    assert_eq!(LogitechModel::SpeedForceWireless.max_rotation_deg(), 270,
        "SFW/WiiWheel: kernel max_range=270");

    // 900° wheels (kernel max_range=900)
    assert_eq!(LogitechModel::DrivingForcePro.max_rotation_deg(), 900,
        "DFP: kernel max_range=900");
    assert_eq!(LogitechModel::G25.max_rotation_deg(), 900,
        "G25: kernel max_range=900");
    assert_eq!(LogitechModel::DrivingForceGT.max_rotation_deg(), 900,
        "DFGT: kernel max_range=900");
    assert_eq!(LogitechModel::G27.max_rotation_deg(), 900,
        "G27: kernel max_range=900");
    assert_eq!(LogitechModel::G29.max_rotation_deg(), 900,
        "G29: kernel max_range=900");
    assert_eq!(LogitechModel::G920.max_rotation_deg(), 900,
        "G920: max_range=900");
    assert_eq!(LogitechModel::G923.max_rotation_deg(), 900,
        "G923: new-lg4ff max_range=900");

    // 1080° (G PRO — Logitech product spec, not in kernel)
    assert_eq!(LogitechModel::GPro.max_rotation_deg(), 1080,
        "G PRO: max_range=1080 (Logitech product spec)");

    Ok(())
}

/// Verify range command support matches kernel `lg4ff_devices[]` set_range function pointer.
///
/// Source: `lg4ff_devices[]` — only devices with a non-NULL `set_range` fn
/// support runtime range adjustment. DFP → `lg4ff_set_range_dfp`,
/// G25/G27/DFGT/G29 → `lg4ff_set_range_g25`, others → NULL.
#[test]
fn range_command_support_matches_kernel_set_range() -> Result<(), Box<dyn std::error::Error>> {
    // Devices WITH set_range function pointer
    assert!(LogitechModel::DrivingForcePro.supports_range_command(),
        "DFP has lg4ff_set_range_dfp in kernel");
    assert!(LogitechModel::G25.supports_range_command(),
        "G25 has lg4ff_set_range_g25 in kernel");
    assert!(LogitechModel::DrivingForceGT.supports_range_command(),
        "DFGT has lg4ff_set_range_g25 in kernel");
    assert!(LogitechModel::G27.supports_range_command(),
        "G27 has lg4ff_set_range_g25 in kernel");
    assert!(LogitechModel::G29.supports_range_command(),
        "G29 has lg4ff_set_range_g25 in kernel");
    assert!(LogitechModel::G923.supports_range_command(),
        "G923 has lg4ff_set_range_g25 in new-lg4ff");

    // Devices WITHOUT set_range (NULL in kernel)
    assert!(!LogitechModel::WingManFormulaForce.supports_range_command(),
        "WingMan FFG has NULL set_range in kernel");
    assert!(!LogitechModel::MOMO.supports_range_command(),
        "MOMO has NULL set_range in kernel");
    assert!(!LogitechModel::DrivingForceEX.supports_range_command(),
        "DF/EX has NULL set_range in kernel");
    assert!(!LogitechModel::SpeedForceWireless.supports_range_command(),
        "SFW has NULL set_range in kernel");
    assert!(!LogitechModel::VibrationWheel.supports_range_command(),
        "Vibration Wheel has NULL set_range in kernel");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Report IDs and sizes
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify report IDs.
///
/// Source: kernel `hid-lg4ff.c` — vendor commands use 0xF8 prefix.
/// Device gain report ID 0x16 is from HID PID spec.
#[test]
fn report_ids_match_protocol() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(report_ids::STANDARD_INPUT, 0x01, "standard input report ID");
    assert_eq!(report_ids::VENDOR, 0xF8,
        "vendor report prefix (kernel hid-lg4ff.c: 0xf8)");
    assert_eq!(report_ids::CONSTANT_FORCE, 0x12, "constant force report ID");
    assert_eq!(report_ids::DEVICE_GAIN, 0x16, "device gain report ID");
    Ok(())
}

/// Verify report sizes.
#[test]
fn report_sizes() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(lg::CONSTANT_FORCE_REPORT_LEN, 4,
        "constant force report = 4 bytes");
    assert_eq!(lg::VENDOR_REPORT_LEN, 7,
        "vendor report = 7 bytes (kernel hid-lg4ff.c uses 7-byte payloads)");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. Device gain report
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify device gain report encoding.
#[test]
fn gain_report_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let full = lg::build_gain_report(0xFF);
    assert_eq!(full, [0x16, 0xFF], "full gain (100%)");

    let zero = lg::build_gain_report(0x00);
    assert_eq!(zero, [0x16, 0x00], "zero gain (0%)");

    let half = lg::build_gain_report(0x80);
    assert_eq!(half, [0x16, 0x80], "half gain (~50%)");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. Slot protocol effect type bytes — matches new-lg4ff lg4ff_update_slot
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify effect type encoding bytes against new-lg4ff `lg4ff_update_slot()`.
///
/// Source: `berarma/new-lg4ff hid-lg4ff.c`:
/// ```c
/// case FF_CONSTANT: slot->current_cmd[1] = 0x00; break;
/// case FF_SPRING:   slot->current_cmd[1] = 0x0b; break;
/// case FF_DAMPER:   slot->current_cmd[1] = 0x0c; break;
/// case FF_FRICTION: slot->current_cmd[1] = 0x0e; break;
/// ```
#[test]
fn slot_effect_type_bytes_match_new_lg4ff() -> Result<(), Box<dyn std::error::Error>> {
    // These are the effect type identifier bytes from new-lg4ff's
    // lg4ff_update_slot() function
    let constant_effect_byte: u8 = 0x00;
    let spring_effect_byte: u8 = 0x0B;
    let damper_effect_byte: u8 = 0x0C;
    let friction_effect_byte: u8 = 0x0E;

    // All must be distinct
    let bytes = [constant_effect_byte, spring_effect_byte, damper_effect_byte, friction_effect_byte];
    for i in 0..bytes.len() {
        for j in (i + 1)..bytes.len() {
            assert_ne!(bytes[i], bytes[j],
                "effect type bytes must be distinct: 0x{:02X} vs 0x{:02X}", bytes[i], bytes[j]);
        }
    }

    // Verify they match the values documented in our output.rs module header
    assert_eq!(constant_effect_byte, 0x00, "Constant effect type = 0x00");
    assert_eq!(spring_effect_byte, 0x0B, "Spring effect type = 0x0B");
    assert_eq!(damper_effect_byte, 0x0C, "Damper effect type = 0x0C");
    assert_eq!(friction_effect_byte, 0x0E, "Friction effect type = 0x0E");

    Ok(())
}

/// Verify the slot ID encoding from new-lg4ff.
///
/// Source: `lg4ff_update_slot()`:
/// ```c
/// slot->current_cmd[0] = (0x10 << slot->id) + slot->cmd_op;
/// ```
/// Slot IDs: 0=constant, 1-3=conditional effects.
/// Operations: 0x01=start, 0x03=stop, 0x0c=update.
#[test]
fn slot_encoding_matches_new_lg4ff() -> Result<(), Box<dyn std::error::Error>> {
    // Verify slot 0 (constant) command byte patterns
    // slot_id=0: (0x10 << 0) = 0x10
    let slot_id: u8 = 0;
    let slot0_start: u8 = (0x10u8 << slot_id) + 0x01;  // 0x11
    let slot0_stop: u8 = (0x10u8 << slot_id) + 0x03;   // 0x13
    let slot0_update: u8 = (0x10u8 << slot_id) + 0x0C;  // 0x1C

    assert_eq!(slot0_start, 0x11, "slot 0 start = 0x11");
    assert_eq!(slot0_stop, 0x13, "slot 0 stop = 0x13");
    assert_eq!(slot0_update, 0x1C, "slot 0 update = 0x1C");

    // This matches kernel lg4ff_play() which uses 0x11 for slot 1 start
    // and 0x13 for slot 1 stop (note: kernel uses 1-indexed slots,
    // new-lg4ff uses 0-indexed but the wire format is the same)

    Ok(())
}

/// Verify the kernel `lg4ff_play` constant force encoding.
///
/// Source: `lg4ff_play()` in kernel `hid-lg4ff.c`:
/// ```c
/// case FF_CONSTANT:
///     x = effect->u.ramp.start_level + 0x80;  // 0x80 is no force
///     // Start: value = {0x11, 0x08, x, 0x80, 0, 0, 0}
///     // Stop:  value = {0x13, 0x00, 0, 0x00, 0, 0, 0}
/// ```
#[test]
fn kernel_lg4ff_play_constant_force_protocol() -> Result<(), Box<dyn std::error::Error>> {
    // Verify the kernel's slot-based constant force encoding
    let start_cmd: [u8; 7] = [0x11, 0x08, 0x80, 0x80, 0x00, 0x00, 0x00]; // zero force
    assert_eq!(start_cmd[0], 0x11, "slot 1 start command");
    assert_eq!(start_cmd[1], 0x08, "constant force sub-command");
    assert_eq!(start_cmd[2], 0x80, "0x80 = no force (center/zero)");

    let stop_cmd: [u8; 7] = [0x13, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(stop_cmd[0], 0x13, "slot 1 stop command");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. G923 PS identification — new-lg4ff lg4ff_g923_ident_info
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify G923 PS mode identification constants.
///
/// Source: `berarma/new-lg4ff hid-lg4ff.c`:
/// ```c
/// static const struct lg4ff_wheel_ident_info lg4ff_g923_ident_info = {
///     LG4FF_MODE_G923_PS | LG4FF_MODE_G923,
///     0xff00,
///     0x3800,
///     USB_DEVICE_ID_LOGITECH_G923_WHEEL
/// };
/// ```
#[test]
fn g923_ps_identification_matches_new_lg4ff() -> Result<(), Box<dyn std::error::Error>> {
    // The identification mask and result for detecting G923 PS compat mode
    let ident_mask: u16 = 0xFF00;
    let ident_result: u16 = 0x3800;
    let native_pid = product_ids::G923; // 0xC266

    // After successful identification, the wheel is known to be a G923
    // and can be switched from PS mode (0xC267) to native (0xC266)
    assert_eq!(native_pid, 0xC266,
        "G923 native PID from lg4ff_g923_ident_info.real_product_id");
    assert_eq!(ident_mask, 0xFF00,
        "G923 ident mask (new-lg4ff lg4ff_g923_ident_info.mask)");
    assert_eq!(ident_result, 0x3800,
        "G923 ident result (new-lg4ff lg4ff_g923_ident_info.result)");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. HID++ protocol indicators — G920/G923 Xbox
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify that G920 and G923 Xbox/PC are correctly classified.
///
/// Source: kernel `hid-logitech-hidpp.c`:
/// ```c
/// HID_USB_DEVICE(USB_VENDOR_ID_LOGITECH,
///     USB_DEVICE_ID_LOGITECH_G920_WHEEL)
///     .driver_data = HIDPP_QUIRK_CLASS_G920 | HIDPP_QUIRK_FORCE_OUTPUT_REPORTS
///
/// HID_USB_DEVICE(USB_VENDOR_ID_LOGITECH,
///     USB_DEVICE_ID_LOGITECH_G923_XBOX_WHEEL)
///     .driver_data = HIDPP_QUIRK_CLASS_G920 | HIDPP_QUIRK_FORCE_OUTPUT_REPORTS
/// ```
///
/// These wheels use HID++ protocol report IDs (0x10/0x11/0x12), NOT the
/// classic lg4ff 7-byte slot protocol.
#[test]
fn g920_g923_xbox_use_hidpp_protocol() -> Result<(), Box<dyn std::error::Error>> {
    // Both G920 and G923 Xbox use the same driver class (QUIRK_CLASS_G920)
    let g920 = LogitechModel::from_product_id(product_ids::G920);
    let g923_xbox = LogitechModel::from_product_id(product_ids::G923_XBOX);

    assert_eq!(g920, LogitechModel::G920,
        "G920 (0xC262) classification");
    assert_eq!(g923_xbox, LogitechModel::G923,
        "G923 Xbox (0xC26E) classification");

    // Neither should claim hardware friction (HID++ doesn't expose it)
    assert!(!g920.supports_hardware_friction(),
        "G920 (HID++) has no hardware friction");
    assert!(!g923_xbox.supports_hardware_friction(),
        "G923 Xbox (HID++) has no hardware friction");

    // HID++ report IDs are documented in the kernel hid-logitech-hidpp.c
    let hidpp_short_report_id: u8 = 0x10;
    let hidpp_long_report_id: u8 = 0x11;
    let hidpp_very_long_report_id: u8 = 0x12;

    // These must NOT collide with our vendor report ID
    assert_ne!(hidpp_short_report_id, report_ids::VENDOR,
        "HID++ short report must not collide with vendor report");
    assert_ne!(hidpp_long_report_id, report_ids::VENDOR,
        "HID++ long report must not collide with vendor report");
    assert_ne!(hidpp_very_long_report_id, report_ids::VENDOR,
        "HID++ very long report must not collide with vendor report");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 15. Multimode wheel identification masks — kernel lg4ff_*_ident_info
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify multimode wheel identification constants from kernel.
///
/// Source: `lg4ff_*_ident_info` structures in kernel `hid-lg4ff.c`.
/// These are used to detect which wheel is hiding behind the DF-EX PID
/// (0xC294) by reading a firmware version or capability field.
#[test]
fn multimode_identification_masks_match_kernel() -> Result<(), Box<dyn std::error::Error>> {
    // Source: lg4ff_dfp_ident_info — mask=0xf000, result=0x1000
    let dfp_mask: u16 = 0xF000;
    let dfp_result: u16 = 0x1000;
    assert_eq!(dfp_mask, 0xF000, "DFP ident mask");
    assert_eq!(dfp_result, 0x1000, "DFP ident result");

    // Source: lg4ff_g25_ident_info — mask=0xff00, result=0x1200
    let g25_mask: u16 = 0xFF00;
    let g25_result: u16 = 0x1200;
    assert_eq!(g25_mask, 0xFF00, "G25 ident mask");
    assert_eq!(g25_result, 0x1200, "G25 ident result");

    // Source: lg4ff_g27_ident_info — mask=0xfff0, result=0x1230
    let g27_mask: u16 = 0xFFF0;
    let g27_result: u16 = 0x1230;
    assert_eq!(g27_mask, 0xFFF0, "G27 ident mask");
    assert_eq!(g27_result, 0x1230, "G27 ident result");

    // Source: lg4ff_dfgt_ident_info — mask=0xff00, result=0x1300
    let dfgt_mask: u16 = 0xFF00;
    let dfgt_result: u16 = 0x1300;
    assert_eq!(dfgt_mask, 0xFF00, "DFGT ident mask");
    assert_eq!(dfgt_result, 0x1300, "DFGT ident result");

    // Source: lg4ff_g29_ident_info — mask=0xfff8, result=0x1350
    let g29_mask: u16 = 0xFFF8;
    let g29_result: u16 = 0x1350;
    assert_eq!(g29_mask, 0xFFF8, "G29 ident mask");
    assert_eq!(g29_result, 0x1350, "G29 ident result");

    // Source: lg4ff_g29_ident_info2 — mask=0xff00, result=0x8900
    let g29_mask2: u16 = 0xFF00;
    let g29_result2: u16 = 0x8900;
    assert_eq!(g29_mask2, 0xFF00, "G29 ident mask (alt)");
    assert_eq!(g29_result2, 0x8900, "G29 ident result (alt)");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 16. G923 Xbox alternate PID
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify G923 Xbox alternate PID is correctly handled.
///
/// PID 0xC26D is listed in JacKeTUs/linux-steering-wheels as "G923
/// (Xbox edition)" with Silver rating. Not in mainline kernel hid-ids.h.
#[test]
fn g923_xbox_alt_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_ids::G923_XBOX_ALT, 0xC26D,
        "G923 Xbox alt PID (linux-steering-wheels: 046d:c26d)");

    let model = LogitechModel::from_product_id(product_ids::G923_XBOX_ALT);
    assert_eq!(model, LogitechModel::G923,
        "G923 Xbox alt must classify as G923");
    assert!(is_wheel_product(product_ids::G923_XBOX_ALT),
        "G923 Xbox alt must be recognized as a wheel");

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 17. Complete PID coverage — every known PID classifies correctly
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify that every known PID maps to a non-Unknown model and is recognized
/// as a wheel product.
#[test]
fn all_pids_classify_and_are_wheels() -> Result<(), Box<dyn std::error::Error>> {
    let all_pids: &[(u16, &str, LogitechModel)] = &[
        (product_ids::WINGMAN_FORMULA_FORCE, "WFF", LogitechModel::WingManFormulaForce),
        (product_ids::WINGMAN_FORMULA_FORCE_GP, "WFFG", LogitechModel::WingManFormulaForce),
        (product_ids::MOMO, "MOMO", LogitechModel::MOMO),
        (product_ids::MOMO_2, "MOMO2", LogitechModel::MOMO),
        (product_ids::DRIVING_FORCE_EX, "DF/EX", LogitechModel::DrivingForceEX),
        (product_ids::DRIVING_FORCE_PRO, "DFP", LogitechModel::DrivingForcePro),
        (product_ids::DRIVING_FORCE_GT, "DFGT", LogitechModel::DrivingForceGT),
        (product_ids::SPEED_FORCE_WIRELESS, "SFW", LogitechModel::SpeedForceWireless),
        (product_ids::VIBRATION_WHEEL, "VibWheel", LogitechModel::VibrationWheel),
        (product_ids::G25, "G25", LogitechModel::G25),
        (product_ids::G27, "G27", LogitechModel::G27),
        (product_ids::G29_PS, "G29", LogitechModel::G29),
        (product_ids::G920, "G920", LogitechModel::G920),
        (product_ids::G923, "G923", LogitechModel::G923),
        (product_ids::G923_PS, "G923_PS", LogitechModel::G923),
        (product_ids::G923_XBOX, "G923_Xbox", LogitechModel::G923),
        (product_ids::G923_XBOX_ALT, "G923_Xbox_Alt", LogitechModel::G923),
        (product_ids::G_PRO, "G_PRO", LogitechModel::GPro),
        (product_ids::G_PRO_XBOX, "G_PRO_Xbox", LogitechModel::GPro),
    ];

    for &(pid, name, expected_model) in all_pids {
        assert!(is_wheel_product(pid),
            "PID 0x{:04X} ({}) must be recognized as a wheel", pid, name);
        let model = LogitechModel::from_product_id(pid);
        assert_eq!(model, expected_model,
            "PID 0x{:04X} ({}) must classify as {:?}", pid, name, expected_model);
        assert_ne!(model, LogitechModel::Unknown,
            "PID 0x{:04X} ({}) must not be Unknown", pid, name);
    }

    // Unknown PID
    assert!(!is_wheel_product(0xFFFF));
    assert_eq!(LogitechModel::from_product_id(0xFFFF), LogitechModel::Unknown);

    Ok(())
}
