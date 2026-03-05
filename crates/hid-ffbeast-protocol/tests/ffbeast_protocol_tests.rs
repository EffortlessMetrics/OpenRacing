//! Comprehensive FFBeast protocol hardening tests.
//!
//! Covers VID/PID validation, state report parsing, torque encoder,
//! settings parsers, command builders, and proptest fuzzing.

use racing_wheel_hid_ffbeast_protocol::*;

// ─── VID / PID golden values ────────────────────────────────────────────

#[test]
fn vid_golden_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        FFBEAST_VENDOR_ID, 0x045B,
        "FFBeast VID must be Renesas 0x045B"
    );
    Ok(())
}

#[test]
fn pid_golden_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(FFBEAST_PRODUCT_ID_JOYSTICK, 0x58F9);
    assert_eq!(FFBEAST_PRODUCT_ID_RUDDER, 0x5968);
    assert_eq!(FFBEAST_PRODUCT_ID_WHEEL, 0x59D7);
    Ok(())
}

#[test]
fn all_pids_distinct() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        FFBEAST_PRODUCT_ID_JOYSTICK,
        FFBEAST_PRODUCT_ID_RUDDER,
        FFBEAST_PRODUCT_ID_WHEEL,
    ];
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(pids[i], pids[j]);
        }
    }
    Ok(())
}

#[test]
fn is_ffbeast_product_known() -> Result<(), Box<dyn std::error::Error>> {
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_JOYSTICK));
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_RUDDER));
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_WHEEL));
    Ok(())
}

#[test]
fn is_ffbeast_product_rejects_unknown() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!is_ffbeast_product(0x0000));
    assert!(!is_ffbeast_product(0xFFFF));
    assert!(!is_ffbeast_product(0x0001));
    assert!(!is_ffbeast_product(FFBEAST_VENDOR_ID)); // VID is not a PID
    Ok(())
}

// ─── State report parsing ───────────────────────────────────────────────

fn make_state_report(fw: [u8; 4], registered: u8, position: i16, torque: i16) -> Vec<u8> {
    let mut data = vec![0u8; 9];
    data[0] = fw[0];
    data[1] = fw[1];
    data[2] = fw[2];
    data[3] = fw[3];
    data[4] = registered;
    let pos = position.to_le_bytes();
    data[5] = pos[0];
    data[6] = pos[1];
    let trq = torque.to_le_bytes();
    data[7] = trq[0];
    data[8] = trq[1];
    data
}

#[test]
fn state_report_parse_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_state_report([0, 24, 6, 1], 1, 5000, -3000);
    let report = FFBeastStateReport::parse(&data).ok_or("parse failed")?;
    assert_eq!(report.firmware_version.release_type, 0);
    assert_eq!(report.firmware_version.major, 24);
    assert_eq!(report.firmware_version.minor, 6);
    assert_eq!(report.firmware_version.patch, 1);
    assert_eq!(report.is_registered, 1);
    assert_eq!(report.position, 5000);
    assert_eq!(report.torque, -3000);
    Ok(())
}

#[test]
fn state_report_parse_extremes() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_state_report([0xFF, 0xFF, 0xFF, 0xFF], 0, 10000, -10000);
    let report = FFBeastStateReport::parse(&data).ok_or("parse failed")?;
    assert_eq!(report.position, 10000);
    assert_eq!(report.torque, -10000);
    assert!((report.position_normalized() - 1.0).abs() < 0.001);
    assert!((report.torque_normalized() + 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn state_report_parse_zero() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_state_report([0, 0, 0, 0], 0, 0, 0);
    let report = FFBeastStateReport::parse(&data).ok_or("parse failed")?;
    assert_eq!(report.position, 0);
    assert_eq!(report.torque, 0);
    assert!(report.position_normalized().abs() < 0.001);
    assert!(report.torque_normalized().abs() < 0.001);
    Ok(())
}

#[test]
fn state_report_position_degrees() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_state_report([0, 1, 0, 0], 1, 10000, 0);
    let report = FFBeastStateReport::parse(&data).ok_or("parse failed")?;
    let degrees = report.position_degrees(900.0);
    assert!(
        (degrees - 450.0).abs() < 0.1,
        "full positive at 900° range = 450°"
    );
    Ok(())
}

#[test]
fn state_report_rejects_short_data() -> Result<(), Box<dyn std::error::Error>> {
    assert!(FFBeastStateReport::parse(&[]).is_none());
    assert!(FFBeastStateReport::parse(&[0; 8]).is_none());
    assert!(FFBeastStateReport::parse(&[0; 9]).is_some()); // exactly min
    Ok(())
}

#[test]
fn state_report_accepts_extra_data() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = make_state_report([0, 1, 2, 3], 1, 100, 200);
    data.extend_from_slice(&[0xAB; 55]);
    let report = FFBeastStateReport::parse(&data).ok_or("parse failed")?;
    assert_eq!(report.position, 100);
    assert_eq!(report.torque, 200);
    Ok(())
}

#[test]
fn state_report_parse_with_id() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![STATE_REPORT_ID];
    data.extend_from_slice(&make_state_report([0, 1, 0, 0], 1, 42, 84));
    let report = FFBeastStateReport::parse_with_id(&data).ok_or("parse with ID failed")?;
    assert_eq!(report.position, 42);
    assert_eq!(report.torque, 84);
    Ok(())
}

#[test]
fn state_report_parse_with_id_wrong_id() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0x01]; // wrong report ID
    data.extend_from_slice(&make_state_report([0, 1, 0, 0], 1, 0, 0));
    assert!(FFBeastStateReport::parse_with_id(&data).is_none());
    Ok(())
}

#[test]
fn state_report_id_constant() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(STATE_REPORT_ID, 0xA3);
    Ok(())
}

// ─── Torque encoder ─────────────────────────────────────────────────────

#[test]
fn torque_encoder_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(0.5);
    assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 5000, "0.5 * 10000 = 5000");
    assert_eq!(report[3], 0, "reserved");
    assert_eq!(report[4], 0, "reserved");
    Ok(())
}

#[test]
fn torque_encoder_full_range() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FFBeastTorqueEncoder;

    let pos = enc.encode(1.0);
    assert_eq!(i16::from_le_bytes([pos[1], pos[2]]), 10000);

    let neg = enc.encode(-1.0);
    assert_eq!(i16::from_le_bytes([neg[1], neg[2]]), -10000);

    let zero = enc.encode(0.0);
    assert_eq!(i16::from_le_bytes([zero[1], zero[2]]), 0);
    Ok(())
}

#[test]
fn torque_encoder_clamps() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FFBeastTorqueEncoder;

    let over = enc.encode(5.0);
    let max = enc.encode(1.0);
    assert_eq!(over, max, "values > 1.0 clamp to 1.0");

    let under = enc.encode(-5.0);
    let min = enc.encode(-1.0);
    assert_eq!(under, min, "values < -1.0 clamp to -1.0");
    Ok(())
}

#[test]
fn torque_encoder_report_constants() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(CONSTANT_FORCE_REPORT_ID, 0x01);
    assert_eq!(CONSTANT_FORCE_REPORT_LEN, 5);
    assert_eq!(GAIN_REPORT_ID, 0x61);
    Ok(())
}

// ─── FFB enable/gain ────────────────────────────────────────────────────

#[test]
fn enable_ffb_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let on = build_enable_ffb(true);
    assert_eq!(on, [0x60, 0x01, 0x00]);

    let off = build_enable_ffb(false);
    assert_eq!(off, [0x60, 0x00, 0x00]);
    Ok(())
}

#[test]
fn set_gain_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_gain(128);
    assert_eq!(report, [0x61, 128, 0x00]);

    let zero = build_set_gain(0);
    assert_eq!(zero, [0x61, 0, 0x00]);

    let max = build_set_gain(255);
    assert_eq!(max, [0x61, 255, 0x00]);
    Ok(())
}

// ─── Settings command builders ──────────────────────────────────────────

#[test]
fn dfu_mode_command_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_dfu_mode();
    assert_eq!(buf.len(), REPORT_SIZE);
    assert_eq!(buf[0], ReportCmd::DfuMode as u8);
    assert_eq!(buf[0], 0x03);
    for &b in &buf[1..] {
        assert_eq!(b, 0, "rest must be zero-padded");
    }
    Ok(())
}

#[test]
fn reboot_command_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_reboot_command();
    assert_eq!(buf.len(), REPORT_SIZE);
    assert_eq!(buf[0], 0x01);
    Ok(())
}

#[test]
fn save_and_reboot_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_save_and_reboot();
    assert_eq!(buf.len(), REPORT_SIZE);
    assert_eq!(buf[0], 0x02);
    Ok(())
}

#[test]
fn reset_center_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_reset_center();
    assert_eq!(buf.len(), REPORT_SIZE);
    assert_eq!(buf[0], 0x04);
    Ok(())
}

#[test]
fn all_commands_64_bytes() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(REPORT_SIZE, 64);
    assert_eq!(build_dfu_mode().len(), 64);
    assert_eq!(build_reboot_command().len(), 64);
    assert_eq!(build_save_and_reboot().len(), 64);
    assert_eq!(build_reset_center().len(), 64);
    Ok(())
}

// ─── Direct control ─────────────────────────────────────────────────────

#[test]
fn direct_control_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let ctrl = DirectControl {
        spring_force: 5000,
        constant_force: -3000,
        periodic_force: 1000,
        force_drop: 50,
    };
    let buf = build_direct_control(&ctrl);
    assert_eq!(buf[0], 0x10, "OverrideData command");
    assert_eq!(i16::from_le_bytes([buf[1], buf[2]]), 5000);
    assert_eq!(i16::from_le_bytes([buf[3], buf[4]]), -3000);
    assert_eq!(i16::from_le_bytes([buf[5], buf[6]]), 1000);
    assert_eq!(buf[7], 50);
    Ok(())
}

#[test]
fn direct_control_clamps_forces() -> Result<(), Box<dyn std::error::Error>> {
    let ctrl = DirectControl {
        spring_force: 20000,
        constant_force: -20000,
        periodic_force: 20000,
        force_drop: 200,
    };
    let buf = build_direct_control(&ctrl);
    assert_eq!(i16::from_le_bytes([buf[1], buf[2]]), 10000);
    assert_eq!(i16::from_le_bytes([buf[3], buf[4]]), -10000);
    assert_eq!(i16::from_le_bytes([buf[5], buf[6]]), 10000);
    assert_eq!(buf[7], 100);
    Ok(())
}

#[test]
fn direct_control_zero() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_direct_control(&DirectControl::default());
    assert_eq!(buf[0], 0x10);
    for &b in &buf[1..8] {
        assert_eq!(b, 0);
    }
    Ok(())
}

// ─── Settings write ─────────────────────────────────────────────────────

#[test]
fn settings_write_u16_motion_range() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_settings_write(SettingField::MotionRange, 0, 900);
    assert_eq!(buf[0], 0x14, "SettingsField command");
    assert_eq!(buf[1], SettingField::MotionRange as u8);
    assert_eq!(buf[2], 0, "index");
    let val = u16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(val, 900);
    Ok(())
}

#[test]
fn settings_write_i8_direction() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_settings_write(SettingField::EncoderDirection, 0, -1);
    assert_eq!(buf[3] as i8, -1);
    Ok(())
}

#[test]
fn settings_write_u8_power_limit() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_settings_write(SettingField::PowerLimit, 0, 80);
    assert_eq!(buf[3], 80);
    Ok(())
}

#[test]
fn settings_write_overflow_clamping() -> Result<(), Box<dyn std::error::Error>> {
    // U16 overflow
    let buf = build_settings_write(SettingField::MotionRange, 0, 70000);
    let val = u16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(val, 65535);

    // U8 overflow
    let buf = build_settings_write(SettingField::PowerLimit, 0, 999);
    assert_eq!(buf[3], 255);

    // I8 underflow
    let buf = build_settings_write(SettingField::DirectXConstantDirection, 0, -200);
    assert_eq!(buf[3] as i8, -128);
    Ok(())
}

// ─── Feature report parsers ─────────────────────────────────────────────

#[test]
fn parse_effect_settings_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; 14];
    buf[0..2].copy_from_slice(&900u16.to_le_bytes()); // motion_range
    buf[2..4].copy_from_slice(&500u16.to_le_bytes()); // static_dampening
    buf[4..6].copy_from_slice(&300u16.to_le_bytes()); // soft_stop_dampening
    buf[6] = 80; // total_effect_strength
    buf[7] = 40; // integrated_spring
    buf[8] = 10; // soft_stop_range
    buf[9] = 60; // soft_stop_strength
    buf[10] = (-1i8) as u8; // dx_constant_direction
    buf[11] = 100; // dx_spring
    buf[12] = 90; // dx_constant
    buf[13] = 80; // dx_periodic

    let settings = parse_effect_settings(&buf).ok_or("parse failed")?;
    assert_eq!(settings.motion_range, 900);
    assert_eq!(settings.static_dampening_strength, 500);
    assert_eq!(settings.soft_stop_dampening_strength, 300);
    assert_eq!(settings.total_effect_strength, 80);
    assert_eq!(settings.integrated_spring_strength, 40);
    assert_eq!(settings.soft_stop_range, 10);
    assert_eq!(settings.soft_stop_strength, 60);
    assert_eq!(settings.dx_constant_direction, -1);
    assert_eq!(settings.dx_spring_strength, 100);
    assert_eq!(settings.dx_constant_strength, 90);
    assert_eq!(settings.dx_periodic_strength, 80);
    Ok(())
}

#[test]
fn parse_effect_settings_min_length() -> Result<(), Box<dyn std::error::Error>> {
    // Exactly 13 bytes - dx_periodic defaults to 0
    let buf = [0u8; 13];
    let settings = parse_effect_settings(&buf).ok_or("should parse 13 bytes")?;
    assert_eq!(settings.dx_periodic_strength, 0);
    Ok(())
}

#[test]
fn parse_effect_settings_too_short() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_effect_settings(&[0u8; 12]).is_none());
    assert!(parse_effect_settings(&[]).is_none());
    Ok(())
}

#[test]
fn parse_hardware_settings_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; 17];
    buf[0..2].copy_from_slice(&4096u16.to_le_bytes()); // encoder_cpr
    buf[2..4].copy_from_slice(&200u16.to_le_bytes()); // integral_gain
    buf[4] = 50; // proportional_gain
    buf[5] = 1; // force_enabled
    buf[6] = 0; // debug_torque
    buf[7] = 2; // amplifier_gain (20V)
    buf[8] = 30; // cal_magnitude
    buf[9] = 10; // cal_speed
    buf[10] = 100; // power_limit
    buf[11] = 80; // braking_limit
    buf[12] = 5; // position_smoothing
    buf[13] = 8; // speed_buffer_size
    buf[14] = (-1i8) as u8; // encoder_direction
    buf[15] = 1; // force_direction
    buf[16] = 7; // pole_pairs

    let settings = parse_hardware_settings(&buf).ok_or("parse failed")?;
    assert_eq!(settings.encoder_cpr, 4096);
    assert_eq!(settings.integral_gain, 200);
    assert_eq!(settings.proportional_gain, 50);
    assert_eq!(settings.force_enabled, 1);
    assert_eq!(settings.amplifier_gain, 2);
    assert_eq!(settings.encoder_direction, -1);
    assert_eq!(settings.force_direction, 1);
    assert_eq!(settings.pole_pairs, 7);
    Ok(())
}

#[test]
fn parse_hardware_settings_too_short() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_hardware_settings(&[0u8; 14]).is_none());
    Ok(())
}

#[test]
fn parse_firmware_license_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; 29];
    buf[0] = 0;
    buf[1] = 24;
    buf[2] = 6;
    buf[3] = 1; // version
    buf[4..8].copy_from_slice(&0xAABBCCDDu32.to_le_bytes());
    buf[8..12].copy_from_slice(&0x11223344u32.to_le_bytes());
    buf[12..16].copy_from_slice(&0x55667788u32.to_le_bytes());
    buf[16..20].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
    buf[20..24].copy_from_slice(&0xCAFEBABEu32.to_le_bytes());
    buf[24..28].copy_from_slice(&0x12345678u32.to_le_bytes());
    buf[28] = 1; // registered

    let lic = parse_firmware_license(&buf).ok_or("parse failed")?;
    assert_eq!(lic.major, 24);
    assert_eq!(lic.minor, 6);
    assert_eq!(lic.patch, 1);
    assert_eq!(lic.serial_key[0], 0xAABBCCDD);
    assert_eq!(lic.device_id[0], 0xDEADBEEF);
    assert_eq!(lic.is_registered, 1);
    Ok(())
}

#[test]
fn parse_firmware_license_too_short() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_firmware_license(&[0u8; 28]).is_none());
    Ok(())
}

#[test]
fn generic_io_report_id_constant() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(GENERIC_IO_REPORT_ID, 0xA3);
    Ok(())
}

// ─── Proptest fuzzing ───────────────────────────────────────────────────

mod proptest_ffbeast {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_state_report_parse_never_panics(data in proptest::collection::vec(any::<u8>(), 0..128)) {
            let _ = FFBeastStateReport::parse(&data);
        }

        #[test]
        fn prop_state_report_valid_length_always_some(data in proptest::collection::vec(any::<u8>(), 9..128)) {
            let result = FFBeastStateReport::parse(&data);
            prop_assert!(result.is_some());
        }

        #[test]
        fn prop_torque_encoder_always_valid_report_id(torque in any::<f32>()) {
            let enc = FFBeastTorqueEncoder;
            let report = enc.encode(torque);
            prop_assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
            prop_assert_eq!(report[3], 0);
            prop_assert_eq!(report[4], 0);
        }

        #[test]
        fn prop_torque_encoder_bounded(torque in -1.0f32..=1.0) {
            let enc = FFBeastTorqueEncoder;
            let report = enc.encode(torque);
            let raw = i16::from_le_bytes([report[1], report[2]]);
            prop_assert!(raw >= -10000 && raw <= 10000);
        }

        #[test]
        fn prop_is_ffbeast_product_only_known(pid in 0u16..=0xFFFF) {
            if is_ffbeast_product(pid) {
                let known = [FFBEAST_PRODUCT_ID_JOYSTICK, FFBEAST_PRODUCT_ID_RUDDER, FFBEAST_PRODUCT_ID_WHEEL];
                prop_assert!(known.contains(&pid));
            }
        }

        #[test]
        fn prop_direct_control_clamps_all_forces(
            spring in -20000i16..=20000,
            constant in -20000i16..=20000,
            periodic in -20000i16..=20000,
            drop_pct in 0u8..=255,
        ) {
            let ctrl = DirectControl {
                spring_force: spring,
                constant_force: constant,
                periodic_force: periodic,
                force_drop: drop_pct,
            };
            let buf = build_direct_control(&ctrl);
            let s = i16::from_le_bytes([buf[1], buf[2]]);
            let c = i16::from_le_bytes([buf[3], buf[4]]);
            let p = i16::from_le_bytes([buf[5], buf[6]]);
            prop_assert!(s >= -10000 && s <= 10000);
            prop_assert!(c >= -10000 && c <= 10000);
            prop_assert!(p >= -10000 && p <= 10000);
            prop_assert!(buf[7] <= 100);
        }

        #[test]
        fn prop_parse_effect_settings_never_panics(data in proptest::collection::vec(any::<u8>(), 0..128)) {
            let _ = parse_effect_settings(&data);
        }

        #[test]
        fn prop_parse_hardware_settings_never_panics(data in proptest::collection::vec(any::<u8>(), 0..128)) {
            let _ = parse_hardware_settings(&data);
        }

        #[test]
        fn prop_parse_firmware_license_never_panics(data in proptest::collection::vec(any::<u8>(), 0..128)) {
            let _ = parse_firmware_license(&data);
        }

        #[test]
        fn prop_position_normalized_bounded(position in -10000i16..=10000) {
            let data = make_state_report([0, 1, 0, 0], 1, position, 0);
            if let Some(report) = FFBeastStateReport::parse(&data) {
                let n = report.position_normalized();
                prop_assert!(n >= -1.0 && n <= 1.0, "normalized {n} out of [-1, 1]");
            }
        }

        #[test]
        fn prop_settings_write_report_structure(
            value in -70000i32..=70000,
            index in 0u8..=255,
        ) {
            let buf = build_settings_write(SettingField::MotionRange, index, value);
            prop_assert_eq!(buf[0], 0x14);
            prop_assert_eq!(buf[1], SettingField::MotionRange as u8);
            prop_assert_eq!(buf[2], index);
            prop_assert_eq!(buf.len(), 64);
        }
    }

    fn make_state_report(fw: [u8; 4], registered: u8, position: i16, torque: i16) -> Vec<u8> {
        let mut data = vec![0u8; 9];
        data[0] = fw[0];
        data[1] = fw[1];
        data[2] = fw[2];
        data[3] = fw[3];
        data[4] = registered;
        let pos = position.to_le_bytes();
        data[5] = pos[0];
        data[6] = pos[1];
        let trq = torque.to_le_bytes();
        data[7] = trq[0];
        data[8] = trq[1];
        data
    }
}
