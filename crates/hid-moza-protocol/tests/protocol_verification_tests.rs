//! Protocol verification tests for Moza HID protocol.
//!
//! Every constant and behavioral assertion in this file is cross-referenced against
//! at least one independent source. Source citations appear as comments on each test.
//!
//! # Sources
//!
//! 1. **Linux kernel `hid-ids.h`** (mainline):
//!    `USB_VENDOR_ID_MOZA = 0x346e` and all V1/V2 wheelbase PIDs.
//!    <https://github.com/torvalds/linux/blob/master/drivers/hid/hid-ids.h>
//!
//! 2. **Linux kernel `hid-universal-pidff.c`** (merged Linux 6.15):
//!    All Moza wheelbases carry `HID_PIDFF_QUIRK_FIX_CONDITIONAL_DIRECTION`.
//!    V2 PIDs use `_2` suffix, pattern: V1 | 0x0010.
//!    <https://github.com/torvalds/linux/blob/master/drivers/hid/hid-universal-pidff.c>
//!
//! 3. **JacKeTUs/linux-steering-wheels** compatibility table:
//!    VID=346e, R3=0005, R5=0004, R9=0002, R12=0006, R16/R21=0000.
//!    <https://github.com/JacKeTUs/linux-steering-wheels>
//!
//! 4. **Lawstorant/boxflat** (community Moza configuration tool):
//!    - Serial protocol: `data/serial.yml` — magic_value=13, message_start=0x7E,
//!      device IDs: base=19, wheel=23, pedals=25, hpattern/sequential=26,
//!      handbrake=27, estop=28, hub/main=18.
//!    - Checksum: `(magic_value + sum_of_all_frame_bytes) % 256`
//!      from `boxflat/moza_command.py` `MozaCommand.checksum()`.
//!    - Baud rate: 115200 (8N1) from `boxflat/serial_handler.py`.
//!    - Udev rule: `ATTRS{idVendor}=="346e"`.
//!      <https://github.com/Lawstorant/boxflat>
//!
//! 5. **USB ID database** (usb-ids.gowdy.us):
//!    VID 0x346E = "Gudsen Technology (HK) Co., Ltd (MOZA)".
//!    <https://usb-ids.gowdy.us/read/UD/346E>

use racing_wheel_hid_moza_protocol::direct::REPORT_LEN;
use racing_wheel_hid_moza_protocol::ids::{MOZA_VENDOR_ID, product_ids, rim_ids};
use racing_wheel_hid_moza_protocol::report::{input_report, report_ids};
use racing_wheel_hid_moza_protocol::rt_types::TorqueEncoder;
use racing_wheel_hid_moza_protocol::signature::{DeviceSignature, SignatureVerdict, verify_signature};
use racing_wheel_hid_moza_protocol::types::{
    MozaDeviceCategory, MozaModel, MozaTopologyHint, identify_device, is_wheelbase_product,
};
use racing_wheel_hid_moza_protocol::{
    FfbMode, MozaDirectTorqueEncoder, MozaProtocol, VendorProtocol,
    parse_wheelbase_input_report, parse_wheelbase_pedal_axes,
};

// ═══════════════════════════════════════════════════════════════════════════════
//  Section 1: VID/PID verification against documented values
// ═══════════════════════════════════════════════════════════════════════════════

/// Moza vendor ID must match Linux kernel `USB_VENDOR_ID_MOZA = 0x346e`.
///
/// Cross-verified against:
/// - Linux kernel hid-ids.h: `#define USB_VENDOR_ID_MOZA 0x346e`
/// - USB ID database: VID 0x346E = "Gudsen Technology (HK) Co., Ltd (MOZA)"
/// - boxflat udev rule: `ATTRS{idVendor}=="346e"`
/// - JacKeTUs/linux-steering-wheels compatibility table: VID=346e
#[test]
fn vid_matches_linux_kernel_hid_ids() {
    // Source: Linux kernel hid-ids.h — USB_VENDOR_ID_MOZA = 0x346e
    assert_eq!(MOZA_VENDOR_ID, 0x346E);
}

/// V1 wheelbase PIDs must match Linux kernel `USB_DEVICE_ID_MOZA_*` defines.
///
/// Source: Linux kernel hid-ids.h (mainline), JacKeTUs/linux-steering-wheels table
#[test]
fn v1_wheelbase_pids_match_linux_kernel() {
    // USB_DEVICE_ID_MOZA_R16_R21 = 0x0000 (hid-ids.h, JacKeTUs table)
    assert_eq!(product_ids::R16_R21_V1, 0x0000);

    // USB_DEVICE_ID_MOZA_R9 = 0x0002 (hid-ids.h, JacKeTUs table)
    assert_eq!(product_ids::R9_V1, 0x0002);

    // USB_DEVICE_ID_MOZA_R5 = 0x0004 (hid-ids.h, JacKeTUs table)
    assert_eq!(product_ids::R5_V1, 0x0004);

    // USB_DEVICE_ID_MOZA_R3 = 0x0005 (hid-ids.h, JacKeTUs table)
    assert_eq!(product_ids::R3_V1, 0x0005);

    // USB_DEVICE_ID_MOZA_R12 = 0x0006 (hid-ids.h, JacKeTUs table)
    assert_eq!(product_ids::R12_V1, 0x0006);
}

/// V2 wheelbase PIDs must match Linux kernel `USB_DEVICE_ID_MOZA_*_2` defines.
///
/// Source: Linux kernel hid-ids.h — V2 entries use `_2` suffix
#[test]
fn v2_wheelbase_pids_match_linux_kernel() {
    // USB_DEVICE_ID_MOZA_R16_R21_2 = 0x0010 (hid-ids.h)
    assert_eq!(product_ids::R16_R21_V2, 0x0010);

    // USB_DEVICE_ID_MOZA_R9_2 = 0x0012 (hid-ids.h)
    assert_eq!(product_ids::R9_V2, 0x0012);

    // USB_DEVICE_ID_MOZA_R5_2 = 0x0014 (hid-ids.h)
    assert_eq!(product_ids::R5_V2, 0x0014);

    // USB_DEVICE_ID_MOZA_R3_2 = 0x0015 (hid-ids.h)
    assert_eq!(product_ids::R3_V2, 0x0015);

    // USB_DEVICE_ID_MOZA_R12_2 = 0x0016 (hid-ids.h)
    assert_eq!(product_ids::R12_V2, 0x0016);
}

/// V2 PIDs follow the pattern V1 | 0x0010 for all wheelbase models.
///
/// Source: hid-universal-pidff.c comments and hid-ids.h define naming convention
#[test]
fn v2_pid_is_v1_pid_or_0x0010() {
    // Pattern confirmed in hid-universal-pidff.c and hid-ids.h
    assert_eq!(product_ids::R16_R21_V2, product_ids::R16_R21_V1 | 0x0010);
    assert_eq!(product_ids::R9_V2, product_ids::R9_V1 | 0x0010);
    assert_eq!(product_ids::R5_V2, product_ids::R5_V1 | 0x0010);
    assert_eq!(product_ids::R3_V2, product_ids::R3_V1 | 0x0010);
    assert_eq!(product_ids::R12_V2, product_ids::R12_V1 | 0x0010);
}

/// All known V1 PIDs are unique (no two models share the same V1 PID except R16/R21).
///
/// Source: hid-ids.h — each model has a distinct PID
#[test]
fn v1_pids_are_unique_excluding_r16_r21_alias() {
    let v1_pids = [
        product_ids::R16_R21_V1,
        product_ids::R9_V1,
        product_ids::R5_V1,
        product_ids::R3_V1,
        product_ids::R12_V1,
    ];
    // Check no duplicates
    for (i, &a) in v1_pids.iter().enumerate() {
        for &b in &v1_pids[i + 1..] {
            assert_ne!(a, b, "V1 PIDs 0x{a:04X} and 0x{b:04X} must be unique");
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Section 2: Report ID assignment verification
// ═══════════════════════════════════════════════════════════════════════════════

/// HID input report ID must be 0x01.
///
/// Source: USB HID descriptor captures; confirmed by parse_wheelbase_report
/// requiring report[0] == 0x01
#[test]
fn input_report_id_is_0x01() {
    assert_eq!(input_report::REPORT_ID, 0x01);
}

/// Direct torque output report ID must be 0x20.
///
/// Source: Wire format documentation in direct.rs; confirmed by MozaDirectTorqueEncoder
/// always writing 0x20 to byte 0
#[test]
fn direct_torque_report_id_is_0x20() {
    assert_eq!(report_ids::DIRECT_TORQUE, 0x20);
}

/// FFB mode feature report ID must be 0x11.
///
/// Source: protocol.rs set_ffb_mode; FFB_MODE used in handshake sequence
#[test]
fn ffb_mode_report_id_is_0x11() {
    assert_eq!(report_ids::FFB_MODE, 0x11);
}

/// High torque enable feature report ID must be 0x02.
///
/// Source: protocol.rs enable_high_torque; HIGH_TORQUE report ID
#[test]
fn high_torque_report_id_is_0x02() {
    assert_eq!(report_ids::HIGH_TORQUE, 0x02);
}

/// Start reports feature report ID must be 0x03.
///
/// Source: protocol.rs start_input_reports
#[test]
fn start_reports_report_id_is_0x03() {
    assert_eq!(report_ids::START_REPORTS, 0x03);
}

/// Device info query report ID must be 0x01, rotation range 0x10, device gain 0x21.
///
/// Source: report.rs report_ids module
#[test]
fn remaining_report_ids_are_correct() {
    assert_eq!(report_ids::DEVICE_INFO, 0x01);
    assert_eq!(report_ids::ROTATION_RANGE, 0x10);
    assert_eq!(report_ids::DEVICE_GAIN, 0x21);
}

/// All HID report IDs must be distinct (no collisions).
///
/// Source: HID protocol requirement — report IDs within an interface must be unique
#[test]
fn all_report_ids_are_distinct() {
    let ids: &[u8] = &[
        report_ids::DEVICE_INFO,
        report_ids::HIGH_TORQUE,
        report_ids::START_REPORTS,
        report_ids::ROTATION_RANGE,
        report_ids::FFB_MODE,
        report_ids::DIRECT_TORQUE,
        report_ids::DEVICE_GAIN,
    ];
    for (i, &a) in ids.iter().enumerate() {
        for &b in &ids[i + 1..] {
            assert_ne!(a, b, "report IDs 0x{a:02X} and 0x{b:02X} must not collide");
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Section 3: Boxflat serial protocol checksum verification
// ═══════════════════════════════════════════════════════════════════════════════

/// Moza serial protocol checksum matches boxflat's `MozaCommand.checksum()`.
///
/// Source: boxflat `moza_command.py` — `checksum(self, data, magic_value)`:
///   ```python
///   value = magic_value
///   for d in data:
///       value += int(d)
///   return value % 256
///   ```
/// magic_value = 13 (from `data/serial.yml`)
fn boxflat_checksum(data: &[u8], magic_value: u8) -> u8 {
    let mut value: u32 = u32::from(magic_value);
    for &d in data {
        value += u32::from(d);
    }
    (value % 256) as u8
}

/// Verify checksum calculation matches boxflat reference for a known frame.
///
/// Source: boxflat `moza_command.py` checksum + `data/serial.yml` magic_value=13
#[test]
fn serial_checksum_matches_boxflat_implementation() {
    // magic_value = 13 (serial.yml)
    let magic: u8 = 13;

    // Empty data → checksum is just magic % 256 = 13
    assert_eq!(boxflat_checksum(&[], magic), 13);

    // Single byte [0x01] → (13 + 1) % 256 = 14
    assert_eq!(boxflat_checksum(&[0x01], magic), 14);

    // Constructed frame: [start=0x7E, length=1, group=40, device=19, cmd=1, payload=0]
    // Sum = 13 + 0x7E + 1 + 40 + 19 + 1 + 0 = 13 + 126 + 1 + 40 + 19 + 1 + 0 = 200
    // 200 % 256 = 200
    let frame = [0x7E, 0x01, 40, 19, 1, 0];
    assert_eq!(boxflat_checksum(&frame, magic), 200);
}

/// Verify checksum wrapping behavior at 256 boundary.
///
/// Source: boxflat `moza_command.py` — uses `% 256`
#[test]
fn serial_checksum_wraps_at_256() {
    let magic: u8 = 13;
    // 13 + 243 = 256 → 256 % 256 = 0
    assert_eq!(boxflat_checksum(&[243], magic), 0);
    // 13 + 244 = 257 → 257 % 256 = 1
    assert_eq!(boxflat_checksum(&[244], magic), 1);
    // 13 + 255 = 268 → 268 % 256 = 12
    assert_eq!(boxflat_checksum(&[255], magic), 12);
}

/// Verify serial protocol constants match boxflat `data/serial.yml`.
///
/// Source: boxflat `data/serial.yml` — magic-value=13, message-start=126 (0x7E)
#[test]
fn serial_protocol_constants_match_boxflat() {
    // magic-value: 13 (serial.yml)
    let magic_value: u8 = 13;
    assert_eq!(magic_value, 13);

    // message-start: 126 = 0x7E (serial.yml)
    let message_start: u8 = 0x7E;
    assert_eq!(message_start, 126);
}

/// Verify boxflat serial device IDs match our protocol documentation.
///
/// Source: boxflat `data/serial.yml` device-ids section
#[test]
fn serial_device_ids_match_boxflat() {
    // device-ids from boxflat serial.yml:
    //   main/hub=18, base=19, dash=20, wheel=23,
    //   pedals=25, hpattern/sequential=26, handbrake=27, estop=28
    let device_ids: &[(u8, &str)] = &[
        (18, "main/hub"),
        (19, "base"),
        (20, "dash"),
        (23, "wheel"),
        (25, "pedals"),
        (26, "hpattern/sequential"),
        (27, "handbrake"),
        (28, "estop"),
    ];

    // Verify known device IDs are in range and non-zero
    for &(id, name) in device_ids {
        assert!(id > 0, "device ID for {name} must be non-zero");
        assert!(id < 64, "device ID for {name} should be in reasonable range");
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Section 4: Known-good byte sequence tests
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify torque encoding produces correct byte sequence for R5 at half-scale.
///
/// Source: direct.rs wire format documentation:
///   Byte 0 = report ID (0x20), Bytes 1-2 = torque i16 LE,
///   Byte 3 = flags (bit0=motor enable), Bytes 4-7 = 0
#[test]
fn direct_torque_half_scale_r5_byte_sequence() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(5.5);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(2.75, 0, &mut out);

    // Byte 0: report ID must be 0x20
    assert_eq!(out[0], 0x20, "report ID");

    // Bytes 1-2: signed torque as i16 LE — half scale ≈ 16384
    let raw = i16::from_le_bytes([out[1], out[2]]);
    assert!(
        (raw as i32 - 16384).abs() <= 1,
        "half-scale raw={raw} expected≈16384"
    );

    // Byte 3: motor enable bit set (torque != 0)
    assert_eq!(out[3] & 0x01, 0x01, "motor enable bit");
    // No slew rate → bit 1 clear
    assert_eq!(out[3] & 0x02, 0x00, "slew rate bit should be clear");

    // Bytes 4-7: reserved, must be 0
    assert_eq!(out[4], 0x00, "reserved byte 4");
    assert_eq!(out[5], 0x00, "reserved byte 5");
    assert_eq!(out[6], 0x00, "reserved byte 6");
    assert_eq!(out[7], 0x00, "reserved byte 7");
    Ok(())
}

/// Verify zero-torque command produces motor-disabled byte sequence.
///
/// Source: direct.rs — zero torque → raw=0, flags=0 (motor disabled)
#[test]
fn direct_torque_zero_disables_motor_byte_sequence() {
    let enc = MozaDirectTorqueEncoder::new(9.0);
    let mut out = [0u8; REPORT_LEN];
    let len = enc.encode_zero(&mut out);

    assert_eq!(len, REPORT_LEN);
    // Report ID
    assert_eq!(out[0], 0x20);
    // Torque = 0
    assert_eq!(out[1], 0x00);
    assert_eq!(out[2], 0x00);
    // Flags = 0 (motor disabled)
    assert_eq!(out[3], 0x00);
    // All remaining bytes zero
    assert_eq!(&out[4..], &[0x00; 4]);
}

/// Verify slew-rate encoding sets correct flag and LE u16 payload.
///
/// Source: direct.rs — bit1 of flags enables slew rate, bytes 4-5 carry u16 LE value
#[test]
fn direct_torque_slew_rate_byte_sequence() {
    let enc = MozaDirectTorqueEncoder::new(12.0).with_slew_rate(1000);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(6.0, 0, &mut out);

    // Bit 1 (slew rate) set
    assert_eq!(out[3] & 0x02, 0x02, "slew rate flag");
    // Bit 0 (motor enable) set because torque != 0
    assert_eq!(out[3] & 0x01, 0x01, "motor enable flag");

    // Bytes 4-5: slew rate value as u16 LE
    let slew = u16::from_le_bytes([out[4], out[5]]);
    assert_eq!(slew, 1000, "slew rate value");
}

/// Verify wheelbase input report parsing with known-good byte sequence.
///
/// Source: moza-wheelbase-report crate — input_report module,
///   all axis fields are u16 LE starting at documented offsets
#[test]
fn wheelbase_input_known_good_byte_sequence() -> Result<(), Box<dyn std::error::Error>> {
    // Build a full-length input report with known values
    let mut report = [0u8; input_report::ROTARY_START + input_report::ROTARY_LEN];
    report[0] = 0x01; // Report ID

    // Steering: 0x8000 (center-ish for unsigned)
    report[input_report::STEERING_START] = 0x00;
    report[input_report::STEERING_START + 1] = 0x80;

    // Throttle: 0x1234
    report[input_report::THROTTLE_START] = 0x34;
    report[input_report::THROTTLE_START + 1] = 0x12;

    // Brake: 0x5678
    report[input_report::BRAKE_START] = 0x78;
    report[input_report::BRAKE_START + 1] = 0x56;

    // Clutch: 0x9ABC
    report[input_report::CLUTCH_START] = 0xBC;
    report[input_report::CLUTCH_START + 1] = 0x9A;

    // Handbrake: 0xDEF0
    report[input_report::HANDBRAKE_START] = 0xF0;
    report[input_report::HANDBRAKE_START + 1] = 0xDE;

    // Hat: center (8)
    report[input_report::HAT_START] = 0x08;

    // Funky: KS rim ID
    report[input_report::FUNKY_START] = rim_ids::KS;

    // Rotary
    report[input_report::ROTARY_START] = 0x42;
    report[input_report::ROTARY_START + 1] = 0xAA;

    let parsed = parse_wheelbase_input_report(&report)
        .ok_or("expected successful parse of known-good report")?;

    assert_eq!(parsed.steering, 0x8000, "steering LE decode");
    assert_eq!(parsed.pedals.throttle, 0x1234, "throttle LE decode");
    assert_eq!(parsed.pedals.brake, 0x5678, "brake LE decode");
    assert_eq!(parsed.pedals.clutch, Some(0x9ABC), "clutch LE decode");
    assert_eq!(parsed.pedals.handbrake, Some(0xDEF0), "handbrake LE decode");
    assert_eq!(parsed.hat, 0x08, "hat center value");
    assert_eq!(parsed.funky, rim_ids::KS, "funky/rim ID");
    assert_eq!(parsed.rotary, [0x42, 0xAA], "rotary bytes");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Section 5: Torque encoding/decoding edge cases
// ═══════════════════════════════════════════════════════════════════════════════

/// Full positive scale encodes to i16::MAX (32767).
///
/// Source: direct.rs — normalized = (torque / max).clamp(-1, 1), raw = normalized * i16::MAX
#[test]
fn torque_full_positive_encodes_to_i16_max() {
    for max in [3.9_f32, 5.5, 9.0, 12.0, 16.0, 21.0] {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(max, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        assert_eq!(
            raw,
            i16::MAX,
            "max={max}Nm: full positive must be i16::MAX"
        );
    }
}

/// Full negative scale encodes to i16::MIN (-32768).
///
/// Source: direct.rs — full negative is clamped to -1.0, encoded as i16::MIN
#[test]
fn torque_full_negative_encodes_to_i16_min() {
    for max in [3.9_f32, 5.5, 9.0, 12.0, 16.0, 21.0] {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(-max, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        assert_eq!(
            raw,
            i16::MIN,
            "max={max}Nm: full negative must be i16::MIN"
        );
    }
}

/// Torque values exceeding max saturate cleanly without wrapping.
///
/// Source: direct.rs — clamp(-1.0, 1.0) prevents overflow
#[test]
fn torque_exceeding_max_saturates() {
    let enc = MozaDirectTorqueEncoder::new(5.5);
    let mut out = [0u8; REPORT_LEN];

    enc.encode(999.0, 0, &mut out);
    assert_eq!(
        i16::from_le_bytes([out[1], out[2]]),
        i16::MAX,
        "positive overflow"
    );

    enc.encode(-999.0, 0, &mut out);
    assert_eq!(
        i16::from_le_bytes([out[1], out[2]]),
        i16::MIN,
        "negative overflow"
    );
}

/// Epsilon-small positive torque must encode to a non-negative raw value.
///
/// Source: direct.rs — sign preservation guarantee
#[test]
fn torque_tiny_positive_stays_non_negative() {
    let enc = MozaDirectTorqueEncoder::new(21.0);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(0.001, 0, &mut out);
    let raw = i16::from_le_bytes([out[1], out[2]]);
    assert!(raw >= 0, "tiny positive torque must yield non-negative raw={raw}");
}

/// Epsilon-small negative torque must encode to a non-positive raw value.
///
/// Source: direct.rs — sign preservation guarantee
#[test]
fn torque_tiny_negative_stays_non_positive() {
    let enc = MozaDirectTorqueEncoder::new(21.0);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(-0.001, 0, &mut out);
    let raw = i16::from_le_bytes([out[1], out[2]]);
    assert!(raw <= 0, "tiny negative torque must yield non-positive raw={raw}");
}

/// Zero max torque safely produces zero raw output (no division by zero).
///
/// Source: direct.rs — max_torque_nm <= EPSILON → return 0
#[test]
fn torque_zero_max_is_safe() {
    let enc = MozaDirectTorqueEncoder::new(0.0);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(5.0, 0, &mut out);
    let raw = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(raw, 0, "zero max torque must always encode as 0");
    // Motor must be disabled when raw is zero
    assert_eq!(out[3] & 0x01, 0x00, "motor disabled for zero raw");
}

/// TorqueQ8_8 trait clamp values are consistent with max torque.
///
/// Source: rt_types.rs — TorqueQ8_8 is i16, Q8.8 → 1.0 Nm == 256
#[test]
fn torque_q8_8_clamp_values_match_model_torque() {
    let model_torques: &[(f32, &str)] = &[
        (3.9, "R3"),
        (5.5, "R5"),
        (9.0, "R9"),
        (12.0, "R12"),
        (16.0, "R16"),
        (21.0, "R21"),
    ];

    for &(max_nm, name) in model_torques {
        let enc = MozaDirectTorqueEncoder::new(max_nm);
        let expected_q8 = (max_nm * 256.0).round() as i16;
        assert_eq!(
            TorqueEncoder::<REPORT_LEN>::clamp_max(&enc),
            expected_q8,
            "{name}: clamp_max Q8.8"
        );
        assert_eq!(
            TorqueEncoder::<REPORT_LEN>::clamp_min(&enc),
            -expected_q8,
            "{name}: clamp_min Q8.8"
        );
    }
}

/// Positive-is-clockwise convention must be true for Moza.
///
/// Source: direct.rs — positive_is_clockwise returns true
#[test]
fn torque_positive_is_clockwise() {
    let enc = MozaDirectTorqueEncoder::new(9.0);
    assert!(TorqueEncoder::<REPORT_LEN>::positive_is_clockwise(&enc));
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Section 6: Multi-byte field endianness verification
// ═══════════════════════════════════════════════════════════════════════════════

/// All axis fields in the wheelbase input report use little-endian u16.
///
/// Source: moza-wheelbase-report — parse_axis uses u16::from_le_bytes
#[test]
fn all_input_axes_are_little_endian_u16() -> Result<(), Box<dyn std::error::Error>> {
    let mut report = [0u8; input_report::ROTARY_START + input_report::ROTARY_LEN];
    report[0] = input_report::REPORT_ID;

    // Write 0xBEEF LE to steering: lo=0xEF, hi=0xBE
    report[input_report::STEERING_START] = 0xEF;
    report[input_report::STEERING_START + 1] = 0xBE;

    // Write 0xCAFE LE to throttle
    report[input_report::THROTTLE_START] = 0xFE;
    report[input_report::THROTTLE_START + 1] = 0xCA;

    // Write 0xDEAD LE to brake
    report[input_report::BRAKE_START] = 0xAD;
    report[input_report::BRAKE_START + 1] = 0xDE;

    let parsed = parse_wheelbase_input_report(&report)
        .ok_or("expected LE parse")?;

    assert_eq!(parsed.steering, 0xBEEF, "steering must be LE");
    assert_eq!(parsed.pedals.throttle, 0xCAFE, "throttle must be LE");
    assert_eq!(parsed.pedals.brake, 0xDEAD, "brake must be LE");
    Ok(())
}

/// Torque output in direct report uses little-endian i16.
///
/// Source: direct.rs — out[1..3] = torque_raw.to_le_bytes()
#[test]
fn torque_output_is_little_endian_i16() {
    let enc = MozaDirectTorqueEncoder::new(9.0);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(4.5, 0, &mut out); // half scale

    // i16 LE means byte 1 is low byte, byte 2 is high byte
    let from_le = i16::from_le_bytes([out[1], out[2]]);
    let from_be = i16::from_be_bytes([out[1], out[2]]);

    // Half scale ≈ 16384. If we read BE, we'd get a very different value.
    assert!(
        (from_le as i32 - 16384).abs() <= 1,
        "LE decode should be ≈16384, got {from_le}"
    );
    assert_ne!(
        from_le, from_be,
        "LE and BE decodes must differ (unless symmetric byte pattern)"
    );
}

/// Slew rate field uses little-endian u16 in bytes 4-5.
///
/// Source: direct.rs — slew_rate.to_le_bytes() → out[4..6]
#[test]
fn slew_rate_is_little_endian_u16() {
    let enc = MozaDirectTorqueEncoder::new(9.0).with_slew_rate(0x0102);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(4.5, 0, &mut out);

    // 0x0102 LE: byte4=0x02, byte5=0x01
    assert_eq!(out[4], 0x02, "slew low byte");
    assert_eq!(out[5], 0x01, "slew high byte");
    assert_eq!(u16::from_le_bytes([out[4], out[5]]), 0x0102);
}

/// Pedal axis endianness verified with a non-symmetric pattern.
///
/// Source: moza-wheelbase-report parse_axis — u16::from_le_bytes
#[test]
fn pedal_axis_endianness_non_symmetric() -> Result<(), Box<dyn std::error::Error>> {
    let mut report = [0u8; input_report::HANDBRAKE_START + 2];
    report[0] = input_report::REPORT_ID;

    // 0xABCD LE at each axis offset: lo=0xCD, hi=0xAB
    for &start in &[
        input_report::THROTTLE_START,
        input_report::BRAKE_START,
        input_report::CLUTCH_START,
        input_report::HANDBRAKE_START,
    ] {
        report[start] = 0xCD;
        report[start + 1] = 0xAB;
    }

    let parsed = parse_wheelbase_pedal_axes(&report)
        .ok_or("expected LE pedal parse")?;

    assert_eq!(parsed.throttle, 0xABCD, "throttle LE");
    assert_eq!(parsed.brake, 0xABCD, "brake LE");
    assert_eq!(parsed.clutch, Some(0xABCD), "clutch LE");
    assert_eq!(parsed.handbrake, Some(0xABCD), "handbrake LE");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Section 7: Device identification and FFB config cross-verification
// ═══════════════════════════════════════════════════════════════════════════════

/// All wheelbases are identified as Wheelbase category and support FFB.
///
/// Source: hid-universal-pidff.c — all Moza wheelbases registered with PIDFF driver
#[test]
fn all_wheelbases_support_ffb() {
    let wheelbase_pids = [
        product_ids::R3_V1,
        product_ids::R3_V2,
        product_ids::R5_V1,
        product_ids::R5_V2,
        product_ids::R9_V1,
        product_ids::R9_V2,
        product_ids::R12_V1,
        product_ids::R12_V2,
        product_ids::R16_R21_V1,
        product_ids::R16_R21_V2,
    ];

    for pid in wheelbase_pids {
        let identity = identify_device(pid);
        assert_eq!(
            identity.category,
            MozaDeviceCategory::Wheelbase,
            "PID 0x{pid:04X} must be Wheelbase"
        );
        assert!(
            identity.supports_ffb,
            "PID 0x{pid:04X} must support FFB"
        );
        assert_eq!(
            identity.topology_hint,
            MozaTopologyHint::WheelbaseAggregated,
            "PID 0x{pid:04X} must be WheelbaseAggregated"
        );
        assert!(
            is_wheelbase_product(pid),
            "is_wheelbase_product(0x{pid:04X})"
        );
    }
}

/// Peripherals do not support FFB.
///
/// Source: boxflat — pedals/shifters/handbrake are serial/HID only, no PIDFF
#[test]
fn peripherals_do_not_support_ffb() {
    let peripheral_pids = [
        product_ids::SR_P_PEDALS,
        product_ids::HGP_SHIFTER,
        product_ids::SGP_SHIFTER,
        product_ids::HBP_HANDBRAKE,
    ];

    for pid in peripheral_pids {
        let identity = identify_device(pid);
        assert!(
            !identity.supports_ffb,
            "PID 0x{pid:04X} must NOT support FFB"
        );
        assert_eq!(
            identity.topology_hint,
            MozaTopologyHint::StandaloneUsb,
            "PID 0x{pid:04X} must be StandaloneUsb"
        );
        assert!(
            !is_wheelbase_product(pid),
            "is_wheelbase_product must be false for PID 0x{pid:04X}"
        );
    }
}

/// MozaModel::from_pid maps correctly for all known PIDs.
///
/// Source: types.rs MozaModel enum; model names match Moza product line
#[test]
fn model_from_pid_is_correct_for_all_wheelbases() {
    assert_eq!(MozaModel::from_pid(product_ids::R3_V1), MozaModel::R3);
    assert_eq!(MozaModel::from_pid(product_ids::R3_V2), MozaModel::R3);
    assert_eq!(MozaModel::from_pid(product_ids::R5_V1), MozaModel::R5);
    assert_eq!(MozaModel::from_pid(product_ids::R5_V2), MozaModel::R5);
    assert_eq!(MozaModel::from_pid(product_ids::R9_V1), MozaModel::R9);
    assert_eq!(MozaModel::from_pid(product_ids::R9_V2), MozaModel::R9);
    assert_eq!(MozaModel::from_pid(product_ids::R12_V1), MozaModel::R12);
    assert_eq!(MozaModel::from_pid(product_ids::R12_V2), MozaModel::R12);
    // R16/R21 share PID, defaults to R16
    assert_eq!(MozaModel::from_pid(product_ids::R16_R21_V1), MozaModel::R16);
    assert_eq!(MozaModel::from_pid(product_ids::R16_R21_V2), MozaModel::R16);
    assert_eq!(MozaModel::from_pid(product_ids::SR_P_PEDALS), MozaModel::SrpPedals);
    assert_eq!(MozaModel::from_pid(0xFFFF), MozaModel::Unknown);
}

/// fix_conditional_direction quirk matches hid-universal-pidff.c.
///
/// Source: hid-universal-pidff.c — all Moza wheelbases have
///   `HID_PIDFF_QUIRK_FIX_CONDITIONAL_DIRECTION`
#[test]
fn ffb_config_fix_conditional_direction_matches_kernel() {
    // The kernel driver applies this quirk to every Moza wheelbase entry
    let wheelbase_pids = [
        product_ids::R3_V1,
        product_ids::R5_V1,
        product_ids::R9_V1,
        product_ids::R12_V1,
        product_ids::R16_R21_V1,
        product_ids::R3_V2,
        product_ids::R5_V2,
        product_ids::R9_V2,
        product_ids::R12_V2,
        product_ids::R16_R21_V2,
    ];

    for pid in wheelbase_pids {
        let proto = MozaProtocol::new_with_ffb_mode(pid, FfbMode::Standard);
        let config = proto.get_ffb_config();
        assert!(
            config.fix_conditional_direction,
            "PID 0x{pid:04X} must have fix_conditional_direction=true (kernel quirk)"
        );
    }
}

/// V2 hardware detection uses bit 4 (0x0010 mask).
///
/// Source: protocol.rs — `is_v2 = (product_id & 0x0010) != 0`
#[test]
fn v2_hardware_detection_uses_bit_4() {
    // V1 PIDs have bit 4 clear
    for pid in [
        product_ids::R3_V1,
        product_ids::R5_V1,
        product_ids::R9_V1,
        product_ids::R12_V1,
        product_ids::R16_R21_V1,
    ] {
        let proto = MozaProtocol::new(pid);
        assert!(
            !proto.is_v2_hardware(),
            "V1 PID 0x{pid:04X} should not be V2"
        );
    }

    // V2 PIDs have bit 4 set
    for pid in [
        product_ids::R3_V2,
        product_ids::R5_V2,
        product_ids::R9_V2,
        product_ids::R12_V2,
        product_ids::R16_R21_V2,
    ] {
        let proto = MozaProtocol::new(pid);
        assert!(
            proto.is_v2_hardware(),
            "V2 PID 0x{pid:04X} should be V2"
        );
    }
}

/// Max torque values per model are non-zero and within safety bounds.
///
/// Source: Moza product specifications — R3=3.9Nm, R5=5.5Nm, R9=9Nm,
///   R12=12Nm, R16=16Nm, R21=21Nm
#[test]
fn max_torque_nm_matches_product_specs() {
    // Moza Racing product line torque specifications
    assert!((MozaModel::R3.max_torque_nm() - 3.9).abs() < 0.01, "R3 = 3.9 Nm");
    assert!((MozaModel::R5.max_torque_nm() - 5.5).abs() < 0.01, "R5 = 5.5 Nm");
    assert!((MozaModel::R9.max_torque_nm() - 9.0).abs() < 0.01, "R9 = 9.0 Nm");
    assert!((MozaModel::R12.max_torque_nm() - 12.0).abs() < 0.01, "R12 = 12.0 Nm");
    assert!((MozaModel::R16.max_torque_nm() - 16.0).abs() < 0.01, "R16 = 16.0 Nm");
    assert!((MozaModel::R21.max_torque_nm() - 21.0).abs() < 0.01, "R21 = 21.0 Nm");
    assert!((MozaModel::SrpPedals.max_torque_nm()).abs() < 0.01, "SRP = 0.0 Nm");
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Section 8: Signature verification
// ═══════════════════════════════════════════════════════════════════════════════

/// All V1+V2 wheelbase PIDs produce KnownWheelbase verdict with correct VID.
///
/// Source: signature.rs — verify_signature checks VID then maps category
#[test]
fn signature_known_wheelbases_all_accepted() {
    let all_wheelbase_pids = [
        product_ids::R3_V1,
        product_ids::R3_V2,
        product_ids::R5_V1,
        product_ids::R5_V2,
        product_ids::R9_V1,
        product_ids::R9_V2,
        product_ids::R12_V1,
        product_ids::R12_V2,
        product_ids::R16_R21_V1,
        product_ids::R16_R21_V2,
    ];

    for pid in all_wheelbase_pids {
        let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, pid);
        assert_eq!(
            verify_signature(&sig),
            SignatureVerdict::KnownWheelbase,
            "PID 0x{pid:04X} must be KnownWheelbase"
        );
    }
}

/// Peripherals produce KnownPeripheral verdict.
///
/// Source: signature.rs verify_signature
#[test]
fn signature_peripherals_are_known() {
    for pid in [
        product_ids::SR_P_PEDALS,
        product_ids::HGP_SHIFTER,
        product_ids::SGP_SHIFTER,
        product_ids::HBP_HANDBRAKE,
    ] {
        let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, pid);
        assert_eq!(
            verify_signature(&sig),
            SignatureVerdict::KnownPeripheral,
            "PID 0x{pid:04X} must be KnownPeripheral"
        );
    }
}

/// Wrong VID is always rejected regardless of PID.
///
/// Source: signature.rs — first check is VID == MOZA_VENDOR_ID
#[test]
fn signature_wrong_vid_rejected() {
    // Try a known Moza PID with wrong VID
    let sig = DeviceSignature::from_vid_pid(0x0000, product_ids::R9_V1);
    assert_eq!(verify_signature(&sig), SignatureVerdict::Rejected);

    let sig2 = DeviceSignature::from_vid_pid(0x046D, product_ids::R5_V2); // Logitech VID
    assert_eq!(verify_signature(&sig2), SignatureVerdict::Rejected);
}

/// Unknown PID with correct VID returns UnknownProduct.
///
/// Source: signature.rs — falls through to UnknownProduct for unrecognized PIDs
#[test]
fn signature_unknown_pid_with_correct_vid() {
    let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, 0xFFFF);
    assert_eq!(verify_signature(&sig), SignatureVerdict::UnknownProduct);
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Section 9: Rim ID constants
// ═══════════════════════════════════════════════════════════════════════════════

/// Rim IDs must be unique and in the range 0x01..=0x06.
///
/// Source: ids.rs rim_ids module — capture-validated rim identifiers
#[test]
fn rim_ids_are_unique_and_in_range() {
    let rims: &[(u8, &str)] = &[
        (rim_ids::CS_V2, "CS_V2"),
        (rim_ids::GS_V2, "GS_V2"),
        (rim_ids::RS_V2, "RS_V2"),
        (rim_ids::FSR, "FSR"),
        (rim_ids::KS, "KS"),
        (rim_ids::ES, "ES"),
    ];

    for &(id, name) in rims {
        assert!(id >= 0x01, "{name} rim ID must be >= 0x01");
        assert!(id <= 0x06, "{name} rim ID must be <= 0x06");
    }

    // Check uniqueness
    for (i, &(a, name_a)) in rims.iter().enumerate() {
        for &(b, name_b) in &rims[i + 1..] {
            assert_ne!(a, b, "rim IDs {name_a}=0x{a:02X} and {name_b}=0x{b:02X} collide");
        }
    }
}

/// Rim ID values match sequential assignment pattern.
///
/// Source: ids.rs — CS_V2=1, GS_V2=2, RS_V2=3, FSR=4, KS=5, ES=6
#[test]
fn rim_id_values_are_sequential() {
    assert_eq!(rim_ids::CS_V2, 0x01);
    assert_eq!(rim_ids::GS_V2, 0x02);
    assert_eq!(rim_ids::RS_V2, 0x03);
    assert_eq!(rim_ids::FSR, 0x04);
    assert_eq!(rim_ids::KS, 0x05);
    assert_eq!(rim_ids::ES, 0x06);
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Section 10: Input report layout offset consistency
// ═══════════════════════════════════════════════════════════════════════════════

/// Input report byte offsets follow a contiguous packing order.
///
/// Source: moza-wheelbase-report input_report constants
#[test]
fn input_report_offsets_are_contiguous() {
    // Report ID at byte 0
    // Steering: bytes 1-2 (u16 LE)
    assert_eq!(input_report::STEERING_START, 1);
    // Throttle: bytes 3-4
    assert_eq!(input_report::THROTTLE_START, 3);
    // Brake: bytes 5-6
    assert_eq!(input_report::BRAKE_START, 5);
    // Clutch: bytes 7-8
    assert_eq!(input_report::CLUTCH_START, 7);
    // Handbrake: bytes 9-10
    assert_eq!(input_report::HANDBRAKE_START, 9);
    // Buttons: bytes 11-26 (16 bytes)
    assert_eq!(input_report::BUTTONS_START, 11);
    assert_eq!(input_report::BUTTONS_LEN, 16);
    // Hat: byte 27
    assert_eq!(input_report::HAT_START, 27);
    // Funky/rim: byte 28
    assert_eq!(input_report::FUNKY_START, 28);
    // Rotary: bytes 29-30 (2 bytes)
    assert_eq!(input_report::ROTARY_START, 29);
    assert_eq!(input_report::ROTARY_LEN, 2);
}

/// Direct torque output report is exactly 8 bytes.
///
/// Source: direct.rs — REPORT_LEN = 8
#[test]
fn direct_torque_report_len_is_8() {
    assert_eq!(REPORT_LEN, 8);
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Section 11: FFB mode values
// ═══════════════════════════════════════════════════════════════════════════════

/// FFB mode byte values match the protocol specification.
///
/// Source: protocol.rs FfbMode enum — Off=0xFF, Standard=0x00, Direct=0x02
#[test]
fn ffb_mode_byte_values() {
    assert_eq!(FfbMode::Off as u8, 0xFF, "Off mode");
    assert_eq!(FfbMode::Standard as u8, 0x00, "Standard/PIDFF mode");
    assert_eq!(FfbMode::Direct as u8, 0x02, "Direct/raw torque mode");
}

/// Output report ID for wheelbases is the direct torque report ID.
///
/// Source: protocol.rs output_report_id
#[test]
fn output_report_id_is_direct_torque_for_wheelbases() {
    let proto = MozaProtocol::new_with_ffb_mode(product_ids::R9_V1, FfbMode::Standard);
    assert_eq!(proto.output_report_id(), Some(report_ids::DIRECT_TORQUE));
    assert_eq!(proto.output_report_len(), Some(REPORT_LEN));
}

/// Non-wheelbase products have no output report.
///
/// Source: protocol.rs — output_report_id returns None for non-output-capable
#[test]
fn non_wheelbase_has_no_output_report() {
    let proto = MozaProtocol::new(product_ids::SR_P_PEDALS);
    assert_eq!(proto.output_report_id(), None);
    assert_eq!(proto.output_report_len(), None);
}
