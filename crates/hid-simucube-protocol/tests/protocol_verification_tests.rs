//! Protocol verification tests for Simucube HID protocol.
//!
//! Cross-verifies VID/PID constants, HID report structure, and protocol
//! invariants against authoritative external sources.
//!
//! ## Authoritative sources (verified July 2025)
//!
//! 1. **Official Simucube developer docs** (Granite Devices):
//!    `Simucube/simucube-docs.github.io` → `docs/Simucube 2/Developers.md`
//!    <https://github.com/Simucube/simucube-docs.github.io>
//!
//! 2. **Granite Devices wiki — USB interface documentation**:
//!    <https://granitedevices.com/wiki/Simucube_product_USB_interface_documentation>
//!
//! 3. **Granite Devices wiki — Linux udev rules**:
//!    <https://granitedevices.com/wiki/Using_Simucube_wheel_base_in_Linux>
//!
//! 4. **JacKeTUs/linux-steering-wheels** compatibility table:
//!    <https://github.com/JacKeTUs/linux-steering-wheels>
//!
//! 5. **USB HID PID 1.01 specification**:
//!    <https://www.usb.org/sites/default/files/documents/pid1_01.pdf>

use hid_simucube_protocol::{
    SimucubeHidReport, SimucubeOutputReport,
    // IDs from ids.rs
    SIMUCUBE_1_BOOTLOADER_PID, SIMUCUBE_1_PID, SIMUCUBE_2_BOOTLOADER_PID, SIMUCUBE_2_PRO_PID,
    SIMUCUBE_2_SPORT_PID, SIMUCUBE_2_ULTIMATE_PID, SIMUCUBE_ACTIVE_PEDAL_PID, SIMUCUBE_VENDOR_ID,
    SIMUCUBE_WIRELESS_WHEEL_PID, SimucubeModel,
    // Constants from lib.rs
    VENDOR_ID, PRODUCT_ID_SPORT, PRODUCT_ID_PRO, PRODUCT_ID_ULTIMATE,
    REPORT_SIZE_INPUT, REPORT_SIZE_OUTPUT,
    MAX_TORQUE_SPORT, MAX_TORQUE_PRO, MAX_TORQUE_ULTIMATE,
    ANGLE_SENSOR_BITS, ANGLE_SENSOR_MAX,
    HID_ADDITIONAL_AXES, HID_BUTTON_COUNT, HID_BUTTON_BYTES, HID_JOYSTICK_REPORT_MIN_BYTES,
    // Output
    EffectType, SimucubeError,
};

// ═══════════════════════════════════════════════════════════════════════════
// § 1  VID/PID cross-verification against authoritative sources
// ═══════════════════════════════════════════════════════════════════════════

/// Simucube USB Vendor ID must be 0x16D0.
///
/// Source: Official Simucube developer docs (`Developers.md` PID table),
///         Granite Devices wiki USB interface documentation,
///         Granite Devices wiki udev rules,
///         JacKeTUs/linux-steering-wheels (VID `16d0`).
#[test]
fn vid_matches_granite_devices_official_docs() {
    assert_eq!(SIMUCUBE_VENDOR_ID, 0x16D0);
}

/// `ids::SIMUCUBE_VENDOR_ID` and `lib::VENDOR_ID` must agree.
#[test]
fn vid_constants_are_consistent() {
    assert_eq!(
        SIMUCUBE_VENDOR_ID, VENDOR_ID,
        "ids.rs SIMUCUBE_VENDOR_ID and lib.rs VENDOR_ID must match"
    );
}

/// Simucube 1 PID = 0x0D5A.
///
/// Source: Official Simucube developer docs — row "Simucube 1 | 0x0d5a".
///         Granite Devices wiki udev: ATTRS{{idProduct}}=="0d5a".
///         JacKeTUs/linux-steering-wheels: VID 16d0, PID 0d5a.
#[test]
fn sc1_pid_matches_official_docs() {
    assert_eq!(SIMUCUBE_1_PID, 0x0D5A);
}

/// Simucube 2 Sport PID = 0x0D61.
///
/// Source: Official Simucube developer docs — row "Simucube 2 Sport | 0x0d61".
///         Granite Devices wiki udev: ATTRS{{idProduct}}=="0d61".
///         JacKeTUs/linux-steering-wheels: VID 16d0, PID 0d61.
#[test]
fn sc2_sport_pid_matches_official_docs() {
    assert_eq!(SIMUCUBE_2_SPORT_PID, 0x0D61);
    assert_eq!(PRODUCT_ID_SPORT, 0x0D61, "lib.rs PRODUCT_ID_SPORT must match ids.rs");
}

/// Simucube 2 Pro PID = 0x0D60.
///
/// Source: Official Simucube developer docs — row "Simucube 2 Pro | 0x0d60".
///         Granite Devices wiki udev: ATTRS{{idProduct}}=="0d60".
///         JacKeTUs/linux-steering-wheels: VID 16d0, PID 0d60.
#[test]
fn sc2_pro_pid_matches_official_docs() {
    assert_eq!(SIMUCUBE_2_PRO_PID, 0x0D60);
    assert_eq!(PRODUCT_ID_PRO, 0x0D60, "lib.rs PRODUCT_ID_PRO must match ids.rs");
}

/// Simucube 2 Ultimate PID = 0x0D5F.
///
/// Source: Official Simucube developer docs — row "Simucube 2 Ultimate | 0x0d5f".
///         Granite Devices wiki udev: ATTRS{{idProduct}}=="0d5f".
///         JacKeTUs/linux-steering-wheels: VID 16d0, PID 0d5f.
#[test]
fn sc2_ultimate_pid_matches_official_docs() {
    assert_eq!(SIMUCUBE_2_ULTIMATE_PID, 0x0D5F);
    assert_eq!(PRODUCT_ID_ULTIMATE, 0x0D5F, "lib.rs PRODUCT_ID_ULTIMATE must match ids.rs");
}

/// Simucube SC-Link Hub (ActivePedal) PID = 0x0D66.
///
/// Source: Official Simucube developer docs — row "Simucube Link Hub | 0x0d66".
///         Windows guidProduct ID: `{{0D6616D0-0000-0000-0000-504944564944}}`.
#[test]
fn sc_link_hub_pid_matches_official_docs() {
    assert_eq!(SIMUCUBE_ACTIVE_PEDAL_PID, 0x0D66);
}

/// Simucube 2 bootloader PID = 0x0D5E.
///
/// Source: Granite Devices wiki udev rules for firmware flashing:
///         ATTRS{{idProduct}}=="0d5e" (needed for firmware upgrade, all SC2 models).
#[test]
fn sc2_bootloader_pid_matches_udev_rules() {
    assert_eq!(SIMUCUBE_2_BOOTLOADER_PID, 0x0D5E);
}

/// Simucube 1 bootloader PID = 0x0D5B.
///
/// Source: Granite Devices wiki udev rules for firmware flashing:
///         ATTRS{{idProduct}}=="0d5b" (needed for Simucube 1 firmware upgrade).
#[test]
fn sc1_bootloader_pid_matches_udev_rules() {
    assert_eq!(SIMUCUBE_1_BOOTLOADER_PID, 0x0D5B);
}

/// SimuCube Wireless Wheel PID = 0x0D63 (speculative).
///
/// NOTE: This PID is **not present** in the official Simucube developer PID
/// table (verified July 2025). It is retained as an estimate only.
#[test]
fn wireless_wheel_pid_is_speculative_0d63() {
    assert_eq!(SIMUCUBE_WIRELESS_WHEEL_PID, 0x0D63);
}

// ── Windows guidProduct ID format verification ──────────────────────────

/// Verify the Windows guidProduct ID format matches the pattern from the
/// official Simucube developer docs.
///
/// The guidProduct ID is formed as: `{PPPPVVVV-0000-0000-0000-504944564944}`
/// where PPPP is the product ID (big-endian hex) and VVVV is the vendor ID.
///
/// Source: Official Simucube developer docs, Microsoft DirectInput docs.
#[test]
fn windows_guid_product_id_format() {
    // Verify the pattern: PID bytes then VID bytes (big-endian hex)
    // SC2 Sport: PID=0x0D61, VID=0x16D0 → {0D6116D0-...}
    let sport_guid_prefix = format!("{:04X}{:04X}", SIMUCUBE_2_SPORT_PID, SIMUCUBE_VENDOR_ID);
    assert_eq!(sport_guid_prefix, "0D6116D0");

    let pro_guid_prefix = format!("{:04X}{:04X}", SIMUCUBE_2_PRO_PID, SIMUCUBE_VENDOR_ID);
    assert_eq!(pro_guid_prefix, "0D6016D0");

    let ultimate_guid_prefix = format!("{:04X}{:04X}", SIMUCUBE_2_ULTIMATE_PID, SIMUCUBE_VENDOR_ID);
    assert_eq!(ultimate_guid_prefix, "0D5F16D0");

    let sc1_guid_prefix = format!("{:04X}{:04X}", SIMUCUBE_1_PID, SIMUCUBE_VENDOR_ID);
    assert_eq!(sc1_guid_prefix, "0D5A16D0");

    let pedal_guid_prefix = format!("{:04X}{:04X}", SIMUCUBE_ACTIVE_PEDAL_PID, SIMUCUBE_VENDOR_ID);
    assert_eq!(pedal_guid_prefix, "0D6616D0");
}

// ═══════════════════════════════════════════════════════════════════════════
// § 2  PID-to-model mapping verification
// ═══════════════════════════════════════════════════════════════════════════

/// All known PIDs must map to the correct `SimucubeModel` variant.
///
/// Source: Official Simucube developer docs product table.
#[test]
fn pid_to_model_mapping_matches_official_table() {
    assert_eq!(SimucubeModel::from_product_id(0x0D5A), SimucubeModel::Simucube1);
    assert_eq!(SimucubeModel::from_product_id(0x0D61), SimucubeModel::Sport);
    assert_eq!(SimucubeModel::from_product_id(0x0D60), SimucubeModel::Pro);
    assert_eq!(SimucubeModel::from_product_id(0x0D5F), SimucubeModel::Ultimate);
    assert_eq!(SimucubeModel::from_product_id(0x0D66), SimucubeModel::ActivePedal);
    assert_eq!(SimucubeModel::from_product_id(0x0D63), SimucubeModel::WirelessWheel);
}

/// Bootloader PIDs must **not** map to any real device model — they should
/// return `Unknown` so the driver never attempts FFB on a bootloader device.
#[test]
fn bootloader_pids_map_to_unknown() {
    assert_eq!(
        SimucubeModel::from_product_id(SIMUCUBE_2_BOOTLOADER_PID),
        SimucubeModel::Unknown
    );
    assert_eq!(
        SimucubeModel::from_product_id(SIMUCUBE_1_BOOTLOADER_PID),
        SimucubeModel::Unknown
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// § 3  Torque specification verification
// ═══════════════════════════════════════════════════════════════════════════

/// Peak torque values must match the published product specifications.
///
/// Source: Granite Devices product pages; community consensus values.
///   - Simucube 2 Sport:    17 Nm
///   - Simucube 2 Pro:      25 Nm
///   - Simucube 2 Ultimate: 32 Nm
#[test]
fn max_torque_matches_product_specs() {
    assert_eq!(MAX_TORQUE_SPORT, 17.0_f32);
    assert_eq!(MAX_TORQUE_PRO, 25.0_f32);
    assert_eq!(MAX_TORQUE_ULTIMATE, 32.0_f32);
}

/// `SimucubeModel::max_torque_nm()` must agree with the top-level constants.
#[test]
fn model_torque_matches_constants() {
    assert_eq!(SimucubeModel::Sport.max_torque_nm(), MAX_TORQUE_SPORT);
    assert_eq!(SimucubeModel::Pro.max_torque_nm(), MAX_TORQUE_PRO);
    assert_eq!(SimucubeModel::Ultimate.max_torque_nm(), MAX_TORQUE_ULTIMATE);
}

/// ActivePedal and WirelessWheel are not torque-output devices.
#[test]
fn non_wheelbase_models_have_zero_torque() {
    assert_eq!(SimucubeModel::ActivePedal.max_torque_nm(), 0.0_f32);
    assert_eq!(SimucubeModel::WirelessWheel.max_torque_nm(), 0.0_f32);
}

// ═══════════════════════════════════════════════════════════════════════════
// § 4  HID joystick input report structure verification
// ═══════════════════════════════════════════════════════════════════════════

/// The documented Simucube HID joystick input report has 8 axes and 128 buttons.
///
/// Source: Official Simucube developer docs (Developers.md):
///   - "Wheel axis: X axis, Unsigned 16 bit field, 0-65535 value"
///   - "Y axis: Unsigned 16 bit field"
///   - "6 additional axises, unsigned 16 bit values"
///   - "128 buttons"
///
/// Layout: 8 axes × 2 bytes + 128 buttons / 8 = 32 bytes minimum.
#[test]
fn hid_report_constants_match_documentation() {
    // Official docs: X + Y + 6 additional = 8 axes total
    assert_eq!(HID_ADDITIONAL_AXES, 6, "6 additional axes per official docs");
    assert_eq!(HID_BUTTON_COUNT, 128, "128 buttons per official docs");
    assert_eq!(HID_BUTTON_BYTES, 16, "128 buttons = 16 bytes");
    // 8 axes × 2 bytes + 16 button bytes = 32 bytes
    assert_eq!(HID_JOYSTICK_REPORT_MIN_BYTES, 32);
}

/// Steering axis is unsigned 16-bit (0–65535) per the official docs.
/// The internal 22-bit encoder resolution is NOT exposed over USB.
///
/// Source: Granite Devices wiki USB interface documentation:
///         "Wheel axis: X axis, Unsigned 16 bit field, 0-65535 value"
#[test]
fn steering_axis_is_u16_range() {
    assert_eq!(ANGLE_SENSOR_BITS, 22, "internal encoder is 22-bit");
    assert_eq!(ANGLE_SENSOR_MAX, (1u32 << 22) - 1, "2^22 - 1 = 4194303");

    // USB axis maximum is u16::MAX (65535), not ANGLE_SENSOR_MAX
    let report = SimucubeHidReport {
        steering: u16::MAX,
        ..SimucubeHidReport::default()
    };
    let norm = report.steering_normalized();
    assert!((norm - 1.0).abs() < 0.001, "u16::MAX should normalise to ~1.0");
}

/// The default HID report should have steering and Y at center (0x8000).
///
/// Source: Official docs state "Y axis will idle at center position".
#[test]
fn hid_report_default_center_position() {
    let report = SimucubeHidReport::default();
    assert_eq!(report.steering, 0x8000, "steering defaults to center");
    assert_eq!(report.y_axis, 0x8000, "Y axis defaults to center/idle");
    assert_eq!(report.axes, [0u16; HID_ADDITIONAL_AXES]);
    assert_eq!(report.buttons, [0u8; HID_BUTTON_BYTES]);
}

/// Parsing a known-good 32-byte joystick report must produce correct values.
///
/// This is a synthetic test vector, not captured from hardware. The byte
/// ordering follows standard HID little-endian conventions.
#[test]
fn hid_report_parse_known_good_bytes() -> Result<(), SimucubeError> {
    // Known-good byte sequence: steering=0x7FFF, y=0x8000, axes all zero,
    // buttons: button 0 set, button 127 set
    let mut data = [0u8; 32];
    // Steering = 0x7FFF (little-endian)
    data[0] = 0xFF;
    data[1] = 0x7F;
    // Y axis = 0x8000 (little-endian)
    data[2] = 0x00;
    data[3] = 0x80;
    // Axes 1-6: all zero (bytes 4-15)
    // Buttons: byte 16 = 0x01 (button 0 pressed)
    data[16] = 0x01;
    // Button 127: byte 31, bit 7
    data[31] = 0x80;

    let report = SimucubeHidReport::parse(&data)?;
    assert_eq!(report.steering, 0x7FFF);
    assert_eq!(report.y_axis, 0x8000);
    assert!(report.button_pressed(0), "button 0 should be pressed");
    assert!(!report.button_pressed(1), "button 1 should not be pressed");
    assert!(report.button_pressed(127), "button 127 should be pressed");
    assert_eq!(report.pressed_count(), 2);
    Ok(())
}

/// Report shorter than 32 bytes must be rejected.
#[test]
fn hid_report_rejects_short_input() {
    let data = [0u8; 31];
    let result = SimucubeHidReport::parse(&data);
    assert!(result.is_err());
    if let Err(SimucubeError::InvalidReportSize { expected, actual }) = result {
        assert_eq!(expected, 32);
        assert_eq!(actual, 31);
    }
}

/// Extra bytes beyond 32 are silently ignored (padded reports are common).
#[test]
fn hid_report_accepts_padded_input() -> Result<(), SimucubeError> {
    let data = [0u8; 64];
    let report = SimucubeHidReport::parse(&data)?;
    assert_eq!(report.steering, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// § 5  Output report (placeholder) format verification
// ═══════════════════════════════════════════════════════════════════════════

/// The output report size must be 64 bytes.
#[test]
fn output_report_is_64_bytes() {
    assert_eq!(REPORT_SIZE_OUTPUT, 64);
    assert_eq!(REPORT_SIZE_INPUT, 64);
}

/// Verify the placeholder output report byte layout.
///
/// NOTE: This is a **placeholder** wire format used for internal testing.
/// Real Simucube FFB uses USB HID PID effect descriptors per the PID 1.01
/// specification. The placeholder layout is:
///
/// ```text
/// Byte 0:    report ID (0x01)
/// Bytes 1-2: sequence (u16 LE)
/// Bytes 3-4: torque_cNm (i16 LE)
/// Byte 5:    LED red
/// Byte 6:    LED green
/// Byte 7:    LED blue
/// Byte 8:    effect type (u8)
/// Bytes 9-10: effect parameter (u16 LE)
/// Bytes 11-63: zero padding
/// ```
#[test]
fn output_report_placeholder_byte_layout() -> Result<(), SimucubeError> {
    let report = SimucubeOutputReport::new(0x0102)
        .with_torque(10.0)   // 10.0 Nm → 1000 cNm
        .with_rgb(0xAA, 0xBB, 0xCC)
        .with_effect(EffectType::Constant, 500);

    let data = report.build()?;

    assert_eq!(data.len(), 64, "output report must be 64 bytes");

    // Report ID
    assert_eq!(data[0], 0x01, "report ID");

    // Sequence (u16 LE)
    assert_eq!(u16::from_le_bytes([data[1], data[2]]), 0x0102, "sequence");

    // Torque in cNm (i16 LE): 10.0 Nm × 100 = 1000 cNm
    assert_eq!(i16::from_le_bytes([data[3], data[4]]), 1000, "torque_cNm");

    // RGB LEDs
    assert_eq!(data[5], 0xAA, "LED red");
    assert_eq!(data[6], 0xBB, "LED green");
    assert_eq!(data[7], 0xCC, "LED blue");

    // Effect type
    assert_eq!(data[8], EffectType::Constant as u8, "effect type");

    // Effect parameter (u16 LE)
    assert_eq!(u16::from_le_bytes([data[9], data[10]]), 500, "effect parameter");

    // Remaining bytes are zero padding
    for (i, &byte) in data[11..].iter().enumerate() {
        assert_eq!(byte, 0, "padding byte at offset {} should be zero", i + 11);
    }

    Ok(())
}

/// Zero-torque output report should have zero torque bytes.
#[test]
fn output_report_zero_torque_known_bytes() -> Result<(), SimucubeError> {
    let report = SimucubeOutputReport::new(0);
    let data = report.build()?;

    assert_eq!(data[0], 0x01, "report ID");
    assert_eq!(data[1], 0x00, "sequence lo");
    assert_eq!(data[2], 0x00, "sequence hi");
    assert_eq!(data[3], 0x00, "torque lo");
    assert_eq!(data[4], 0x00, "torque hi");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// § 6  USB HID PID effect type verification
// ═══════════════════════════════════════════════════════════════════════════

/// Effect type enum values must match the USB HID PID 1.01 effect IDs.
///
/// Source: USB HID PID 1.01 specification, table of effect types.
///         Cross-verified against OpenFFBoard `ffb_defs.h` effect type
///         defines (FFB_EFFECT_CONSTANT=0x01, etc.).
#[test]
fn effect_type_ids_match_pid_spec() {
    assert_eq!(EffectType::None as u8, 0x00);
    assert_eq!(EffectType::Constant as u8, 0x01);
    assert_eq!(EffectType::Ramp as u8, 0x02);
    assert_eq!(EffectType::Square as u8, 0x03);
    assert_eq!(EffectType::Sine as u8, 0x04);
    assert_eq!(EffectType::Triangle as u8, 0x05);
    assert_eq!(EffectType::SawtoothUp as u8, 0x06);
    assert_eq!(EffectType::SawtoothDown as u8, 0x07);
    assert_eq!(EffectType::Spring as u8, 0x08);
    assert_eq!(EffectType::Damper as u8, 0x09);
    assert_eq!(EffectType::Friction as u8, 0x0A);
}

// ═══════════════════════════════════════════════════════════════════════════
// § 7  Torque clamping and validation
// ═══════════════════════════════════════════════════════════════════════════

/// Torque values beyond the MAX_TORQUE_NM (25 Nm) must be clamped.
#[test]
fn torque_clamping_at_max() {
    let report = SimucubeOutputReport::new(0).with_torque(100.0);
    // Clamped to 25.0 Nm → 2500 cNm
    assert_eq!(report.torque_cNm, 2500);

    let report_neg = SimucubeOutputReport::new(0).with_torque(-100.0);
    assert_eq!(report_neg.torque_cNm, -2500);
}

/// Torque validation catches out-of-range values set directly.
#[test]
fn torque_validation_rejects_extreme_values() {
    let report = SimucubeOutputReport {
        torque_cNm: i16::MAX, // Way beyond 25 Nm
        ..SimucubeOutputReport::default()
    };
    assert!(report.validate_torque().is_err());
}

// ═══════════════════════════════════════════════════════════════════════════
// § 8  Cross-crate consistency with simplemotion-v2
// ═══════════════════════════════════════════════════════════════════════════

/// The Simucube HID VID (0x16D0) must be distinct from the SimpleMotion V2
/// VID (0x1D50). These serve different USB interfaces on the device.
///
/// - 0x16D0: HID joystick/PID interface (used by games)
/// - 0x1D50: SimpleMotion V2 servo protocol (used by Simucube Tuner/True Drive)
#[test]
fn simucube_hid_vid_distinct_from_simplemotion_vid() {
    // SimpleMotion V2 VID is 0x1D50 (Openmoko Inc. / open hardware)
    let sm_vid: u16 = 0x1D50;
    assert_ne!(
        SIMUCUBE_VENDOR_ID, sm_vid,
        "HID VID (0x16D0) must differ from SimpleMotion VID (0x1D50)"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// § 9  Linux driver compatibility notes
// ═══════════════════════════════════════════════════════════════════════════

/// The Linux `hid-pidff` driver (Silver support since Linux 6.15) handles
/// Simucube devices. Early firmware omitted the 0xA7 (effect delay) HID
/// descriptor, fixed in firmware 1.0.49. Verify our effect type set includes
/// the standard PID effects supported by hid-pidff.
///
/// Source: JacKeTUs/linux-steering-wheels README.md
#[test]
fn all_standard_pid_effects_present() {
    // linux-steering-wheels rates Simucube as Silver with hid-pidff driver.
    // The driver requires these standard PID effect types:
    let expected_effects: &[(u8, &str)] = &[
        (0x01, "Constant"),
        (0x02, "Ramp"),
        (0x03, "Square"),
        (0x04, "Sine"),
        (0x05, "Triangle"),
        (0x06, "SawtoothUp"),
        (0x07, "SawtoothDown"),
        (0x08, "Spring"),
        (0x09, "Damper"),
        (0x0A, "Friction"),
    ];

    for &(id, name) in expected_effects {
        // Verify the EffectType enum covers this ID
        let has_effect = matches!(
            id,
            x if x == EffectType::Constant as u8
                || x == EffectType::Ramp as u8
                || x == EffectType::Square as u8
                || x == EffectType::Sine as u8
                || x == EffectType::Triangle as u8
                || x == EffectType::SawtoothUp as u8
                || x == EffectType::SawtoothDown as u8
                || x == EffectType::Spring as u8
                || x == EffectType::Damper as u8
                || x == EffectType::Friction as u8
        );
        assert!(has_effect, "Missing PID effect type: {name} (0x{id:02X})");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § 10  Known-good byte sequence tests
// ═══════════════════════════════════════════════════════════════════════════

/// Known-good HID input report: full-left steering, no buttons.
#[test]
fn known_good_full_left_steering() -> Result<(), SimucubeError> {
    let mut data = [0u8; 32];
    // Steering = 0x0000 (full left)
    data[0] = 0x00;
    data[1] = 0x00;
    // Y axis = 0x8000 (center)
    data[2] = 0x00;
    data[3] = 0x80;

    let report = SimucubeHidReport::parse(&data)?;
    assert_eq!(report.steering, 0x0000);
    let signed = report.steering_signed();
    assert!((signed - (-1.0)).abs() < 0.001, "full left ≈ -1.0, got {signed}");
    Ok(())
}

/// Known-good HID input report: full-right steering, no buttons.
#[test]
fn known_good_full_right_steering() -> Result<(), SimucubeError> {
    let mut data = [0u8; 32];
    // Steering = 0xFFFF (full right)
    data[0] = 0xFF;
    data[1] = 0xFF;
    data[2] = 0x00;
    data[3] = 0x80;

    let report = SimucubeHidReport::parse(&data)?;
    assert_eq!(report.steering, 0xFFFF);
    let signed = report.steering_signed();
    assert!((signed - 1.0).abs() < 0.001, "full right ≈ 1.0, got {signed}");
    Ok(())
}

/// Known-good HID input report: center steering with all 128 buttons pressed.
#[test]
fn known_good_all_buttons_pressed() -> Result<(), SimucubeError> {
    let mut data = [0u8; 32];
    // Steering = center
    data[0] = 0x00;
    data[1] = 0x80;
    data[2] = 0x00;
    data[3] = 0x80;
    // All button bytes set to 0xFF
    for byte in &mut data[16..32] {
        *byte = 0xFF;
    }

    let report = SimucubeHidReport::parse(&data)?;
    assert_eq!(report.pressed_count(), 128, "all 128 buttons should be pressed");
    for i in 0..128 {
        assert!(report.button_pressed(i), "button {i} should be pressed");
    }
    Ok(())
}

/// Known-good output report: maximum positive torque (25 Nm) on Ultimate.
#[test]
fn known_good_output_max_torque() -> Result<(), SimucubeError> {
    let report = SimucubeOutputReport::new(1).with_torque(MAX_TORQUE_ULTIMATE);
    // Clamped to MAX_TORQUE_NM (25 Nm) since with_torque clamps to MAX_TORQUE_NM
    let data = report.build()?;

    // Expected: torque_cNm = 2500 (25.0 Nm × 100)
    let torque = i16::from_le_bytes([data[3], data[4]]);
    assert_eq!(torque, 2500, "25.0 Nm × 100 = 2500 cNm");
    Ok(())
}

/// Known-good output report: negative torque (-15.5 Nm).
#[test]
fn known_good_output_negative_torque() -> Result<(), SimucubeError> {
    let report = SimucubeOutputReport::new(0).with_torque(-15.5);
    let data = report.build()?;

    let torque = i16::from_le_bytes([data[3], data[4]]);
    assert_eq!(torque, -1550, "-15.5 Nm × 100 = -1550 cNm");
    Ok(())
}
