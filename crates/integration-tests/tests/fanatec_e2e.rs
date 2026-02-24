//! BDD end-to-end tests for the Fanatec protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable hardware-ready
//! behaviors without real USB hardware.

use racing_wheel_hid_fanatec_protocol::{
    ids::report_ids, product_ids, FanatecConstantForceEncoder, CONSTANT_FORCE_REPORT_LEN,
};
use racing_wheel_integration_tests::fanatec_virtual::FanatecScenario;

// ─── Scenario 1: wheelbase sends mode-switch on initialize ───────────────────

#[test]
fn scenario_wheelbase_sends_mode_switch_on_initialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: GT DD Pro wheelbase
    let mut s = FanatecScenario::wheelbase(product_ids::GT_DD_PRO);

    // When: initialized
    s.initialize()?;

    // Then: exactly one feature report was sent
    assert_eq!(s.device.feature_reports().len(), 1);

    // Then: the report carries the MODE_SWITCH report ID
    assert!(s.device.sent_feature_report_id(report_ids::MODE_SWITCH));

    Ok(())
}

// ─── Scenario 2: non-wheelbase skips mode-switch ─────────────────────────────

#[test]
fn scenario_non_wheelbase_skips_mode_switch() -> Result<(), Box<dyn std::error::Error>> {
    // Given: unknown/accessory product ID that is not a wheelbase
    let mut s = FanatecScenario::wheelbase(0xFF00);

    // When: initialized
    s.initialize()?;

    // Then: no feature reports are sent
    assert_eq!(
        s.device.feature_reports().len(),
        0,
        "non-wheelbase must not send a mode-switch report"
    );

    Ok(())
}

// ─── Scenario 3: mode-switch report has correct wire bytes ───────────────────

#[test]
fn scenario_mode_switch_report_bytes_are_correct() -> Result<(), Box<dyn std::error::Error>> {
    // Given: CSL DD wheelbase
    let mut s = FanatecScenario::wheelbase(product_ids::CSL_DD);

    // When: initialized
    s.initialize()?;

    // Then: mode-switch payload is exactly [0x01, 0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00]
    let reports = s.device.feature_reports_with_id(report_ids::MODE_SWITCH);
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].as_slice(),
        &[0x01u8, 0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00],
        "mode-switch payload must match Fanatec Advanced/PC mode command"
    );

    Ok(())
}

// ─── Scenario 4: initialize returns error on I/O failure ─────────────────────

#[test]
fn scenario_initialize_returns_error_on_io_failure() {
    // Given: always-failing device
    let mut s = FanatecScenario::wheelbase_failing(product_ids::GT_DD_PRO);

    // When: initialized
    let result = s.initialize();

    // Then: returns Err (write failure propagated)
    assert!(result.is_err(), "I/O failure must propagate from initialize");
}

// ─── Scenario 5: disconnect, reconnect, and reinitialize ─────────────────────

#[test]
fn scenario_disconnect_reconnect_and_reinitialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: GT DD Pro after successful init
    let mut s = FanatecScenario::wheelbase(product_ids::GT_DD_PRO);
    s.initialize()?;
    assert_eq!(s.device.feature_reports().len(), 1);

    // When: device disconnects
    s.device.disconnect();
    assert!(!s.device.is_connected());

    // Then: reinitialize while disconnected returns Err
    assert!(
        s.initialize().is_err(),
        "initialize must fail when device is disconnected"
    );

    // When: device reconnects
    s.device.reconnect();
    s.device.clear_records();

    // Then: reinitialize succeeds and sends exactly one mode-switch report again
    s.initialize()?;
    assert_eq!(
        s.device.feature_reports().len(),
        1,
        "reinitialize after reconnect must send mode-switch again"
    );

    Ok(())
}

// ─── Scenario 6: constant-force encoder encodes max torque correctly ─────────

#[test]
fn scenario_constant_force_max_torque_encoding() {
    // Given: GT DD Pro encoder at 8 Nm
    let max_nm: f32 = 8.0;
    let encoder = FanatecConstantForceEncoder::new(max_nm);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // When: encoding maximum positive torque
    let written = encoder.encode(max_nm, 0, &mut out);

    // Then: report ID and command byte are correct
    assert_eq!(written, CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(out[0], report_ids::FFB_OUTPUT, "byte 0 must be FFB_OUTPUT report ID");
    assert_eq!(out[1], 0x01, "byte 1 must be CONSTANT_FORCE command");

    // Then: signed i16 LE at bytes 2–3 must be +32767 (maximum positive)
    let force = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(force, i16::MAX, "max torque must encode to i16::MAX");
}

// ─── Scenario 7: zero-torque produces a neutral output report ────────────────

#[test]
fn scenario_constant_force_zero_torque_encoding() {
    // Given: CSL DD encoder at 8 Nm
    let encoder = FanatecConstantForceEncoder::new(8.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // When: encoding zero torque
    encoder.encode(0.0, 0, &mut out);

    // Then: force bytes are zero
    let force = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(force, 0, "zero torque must encode to 0");

    // Then: report ID and command still set
    assert_eq!(out[0], report_ids::FFB_OUTPUT);
    assert_eq!(out[1], 0x01);
}

// ─── Scenario 8: DD1 also sends mode-switch ───────────────────────────────────

#[test]
fn scenario_dd1_sends_mode_switch_on_initialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: Podium DD1 (20 Nm direct drive)
    let mut s = FanatecScenario::wheelbase(product_ids::DD1);

    // When: initialized
    s.initialize()?;

    // Then: exactly one mode-switch report sent
    assert_eq!(s.device.feature_reports().len(), 1);
    assert!(s.device.sent_feature_report_id(report_ids::MODE_SWITCH));

    Ok(())
}

// ─── Scenario 9: graceful shutdown sends stop-all ─────────────────────────────

#[test]
fn scenario_shutdown_sends_stop_all_output_report() -> Result<(), Box<dyn std::error::Error>> {
    // Given: initialized GT DD Pro wheelbase
    let mut s = FanatecScenario::wheelbase(product_ids::GT_DD_PRO);
    s.initialize()?;
    s.device.clear_records();

    // When: graceful shutdown
    s.shutdown()?;

    // Then: exactly one output report sent (stop-all)
    assert_eq!(
        s.device.output_reports().len(),
        1,
        "shutdown must send exactly one stop-all output report"
    );

    let report = s.device.last_output_report().expect("report must be present");
    // stop-all layout: [FFB_OUTPUT=0x01, STOP_ALL=0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    assert_eq!(report[0], report_ids::FFB_OUTPUT, "byte 0 must be FFB_OUTPUT");
    assert_eq!(report[1], 0x0F, "byte 1 must be STOP_ALL command (0x0F)");
    assert_eq!(&report[2..], &[0x00u8; 6], "trailing bytes must be zero");

    // Then: no feature reports during shutdown
    assert_eq!(
        s.device.feature_reports().len(),
        0,
        "shutdown must not send any feature reports"
    );

    Ok(())
}

// ─── Scenario 10: non-wheelbase shutdown is a no-op ───────────────────────────

#[test]
fn scenario_non_wheelbase_shutdown_is_noop() -> Result<(), Box<dyn std::error::Error>> {
    // Given: unknown/accessory PID
    let mut s = FanatecScenario::wheelbase(0xFF00);

    // When: shutdown
    s.shutdown()?;

    // Then: no reports sent at all
    assert_eq!(s.device.output_reports().len(), 0);
    assert_eq!(s.device.feature_reports().len(), 0);

    Ok(())
}

// ─── Scenario 11: extended telemetry report parses temperature and faults ─────

#[test]
fn scenario_extended_report_parses_telemetry() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a raw 64-byte extended telemetry report (ID 0x02)
    let mut raw = [0u8; 64];
    raw[0] = report_ids::EXTENDED_INPUT; // report ID 0x02
    // Steering velocity (bytes 3–4): ignored in this test
    raw[5] = 82;   // motor temperature: 82 °C
    raw[6] = 41;   // board temperature: 41 °C
    raw[7] = 15;   // current draw: 1.5 A (in 0.1 A units)
    raw[10] = 0x03; // fault_flags: over-temp (bit 0) + over-current (bit 1)

    // When: parsed
    use racing_wheel_hid_fanatec_protocol::parse_extended_report;
    let state = parse_extended_report(&raw).ok_or("parse failed")?;

    // Then: fields match the raw bytes
    assert_eq!(state.motor_temp_c, 82, "motor temperature must match");
    assert_eq!(state.board_temp_c, 41, "board temperature must match");
    assert_eq!(state.current_raw, 15, "current draw must match");
    assert_eq!(
        state.fault_flags & 0x03,
        0x03,
        "over-temp and over-current bits must be set"
    );

    Ok(())
}

// ─── Scenario 12: LED report encodes bitmask and brightness correctly ─────────

#[test]
fn scenario_led_report_encoding() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::build_led_report;

    // Given: 8 rev-lights lit (low byte = 0xFF) at 75 % brightness (≈ 191)
    let report = build_led_report(0x00FF, 191);

    // Then: wire layout is [0x08, 0x80, bitmask_lo, bitmask_hi, brightness, 0, 0, 0]
    assert_eq!(report[0], 0x08, "byte 0 must be LED_DISPLAY report ID");
    assert_eq!(report[1], 0x80, "byte 1 must be REV_LIGHTS command");
    assert_eq!(report[2], 0xFF, "byte 2 must be bitmask low byte");
    assert_eq!(report[3], 0x00, "byte 3 must be bitmask high byte");
    assert_eq!(report[4], 191, "byte 4 must be brightness");
    assert_eq!(&report[5..], &[0u8; 3], "trailing bytes must be zero");

    Ok(())
}

// ─── Scenario 13: display report carries gear digit and brightness ─────────────

#[test]
fn scenario_display_report_encoding() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::build_display_report;

    // Given: display gear "3" at full brightness
    let report = build_display_report(0x00, [b'3', b' ', b' '], 255);

    // Then: layout is [0x08, 0x81, mode, d0, d1, d2, brightness, 0]
    assert_eq!(report[0], 0x08, "byte 0 must be LED_DISPLAY report ID");
    assert_eq!(report[1], 0x81, "byte 1 must be DISPLAY command");
    assert_eq!(report[2], 0x00, "byte 2 must be mode");
    assert_eq!(report[3], b'3', "byte 3 must be digit 0");
    assert_eq!(report[6], 255, "byte 6 must be brightness");
    assert_eq!(report[7], 0, "byte 7 must be reserved zero");

    Ok(())
}

// ─── Scenario 14: rumble report encodes left/right intensity and duration ──────

#[test]
fn scenario_rumble_report_encoding() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::build_rumble_report;

    // Given: both motors at max for 200 ms (= 20 × 10 ms units)
    let report = build_rumble_report(255, 255, 20);

    // Then: layout is [0x08, 0x82, left, right, duration, 0, 0, 0]
    assert_eq!(report[0], 0x08, "byte 0 must be LED_DISPLAY report ID");
    assert_eq!(report[1], 0x82, "byte 1 must be RUMBLE command");
    assert_eq!(report[2], 255, "byte 2 must be left motor intensity");
    assert_eq!(report[3], 255, "byte 3 must be right motor intensity");
    assert_eq!(report[4], 20, "byte 4 must be duration (×10 ms)");
    assert_eq!(&report[5..], &[0u8; 3], "trailing bytes must be zero");

    Ok(())
}

// ─── Scenario 15: CSL DD sends mode-switch on initialize ──────────────────────

#[test]
fn scenario_csl_dd_sends_mode_switch_on_initialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: CSL DD wheelbase (primary PID 0x0020)
    let mut s = FanatecScenario::wheelbase(product_ids::CSL_DD);

    // When: initialized
    s.initialize()?;

    // Then: exactly one mode-switch feature report sent
    assert_eq!(
        s.device.feature_reports().len(),
        1,
        "CSL DD must send exactly one mode-switch on init"
    );
    assert!(
        s.device.sent_feature_report_id(report_ids::MODE_SWITCH),
        "CSL DD init report must carry MODE_SWITCH report ID"
    );

    Ok(())
}

// ─── Scenario 16: McLaren funky switch center direction is 0x00 ───────────────

#[test]
fn scenario_mclaren_funky_switch_center() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::parse_standard_report;

    // Given: a 64-byte standard input report with funky switch byte 10 = center (0x00)
    let mut raw = [0u8; 64];
    raw[0] = report_ids::STANDARD_INPUT;
    raw[10] = 0x00; // center

    // When: parsed
    let state = parse_standard_report(&raw).ok_or("parse failed")?;

    // Then: funky_dir is 0x00
    assert_eq!(state.funky_dir, 0x00, "funky switch center must be 0x00");

    Ok(())
}

// ─── Scenario 17: McLaren funky switch up direction is 0x01 ──────────────────

#[test]
fn scenario_mclaren_funky_switch_up() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::parse_standard_report;

    // Given: a standard input report with funky switch up (0x01)
    let mut raw = [0u8; 64];
    raw[0] = report_ids::STANDARD_INPUT;
    raw[10] = 0x01; // up

    // When: parsed
    let state = parse_standard_report(&raw).ok_or("parse failed")?;

    // Then: funky_dir is 0x01
    assert_eq!(state.funky_dir, 0x01, "funky switch up must be 0x01");

    Ok(())
}

// ─── Scenario 18: McLaren rotary encoder values round-trip correctly ──────────

#[test]
fn scenario_mclaren_rotary_encoder_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::parse_standard_report;

    // Given: rotary1 = 512, rotary2 = -256 encoded as LE i16
    let mut raw = [0u8; 64];
    raw[0] = report_ids::STANDARD_INPUT;
    let r1 = 512i16.to_le_bytes();
    raw[11] = r1[0];
    raw[12] = r1[1];
    let r2 = (-256i16).to_le_bytes();
    raw[13] = r2[0];
    raw[14] = r2[1];

    // When: parsed
    let state = parse_standard_report(&raw).ok_or("parse failed")?;

    // Then: rotary values match
    assert_eq!(state.rotary1, 512, "rotary1 must round-trip");
    assert_eq!(state.rotary2, -256, "rotary2 must round-trip");

    Ok(())
}

// ─── Scenario 19: McLaren dual clutch paddles are correctly normalized ────────

#[test]
fn scenario_mclaren_dual_clutch_paddles_normalized() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::parse_standard_report;

    // Given: left clutch fully pressed (byte 15 = 0x00 inverted → 1.0),
    //        right clutch released   (byte 16 = 0xFF inverted → 0.0)
    let mut raw = [0u8; 64];
    raw[0] = report_ids::STANDARD_INPUT;
    raw[15] = 0x00; // left pressed
    raw[16] = 0xFF; // right released

    // When: parsed
    let state = parse_standard_report(&raw).ok_or("parse failed")?;

    // Then: clutch_left is ~1.0 and clutch_right is ~0.0
    assert!(
        (state.clutch_left - 1.0).abs() < 1e-4,
        "left clutch fully pressed must be ~1.0"
    );
    assert!(
        state.clutch_right.abs() < 1e-4,
        "right clutch released must be ~0.0"
    );

    Ok(())
}

// ─── Scenario 20: pedal report parses throttle and brake ─────────────────────

#[test]
fn scenario_pedal_report_parses_throttle_and_brake() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::parse_pedal_report;

    // Given: a 2-axis pedal report with throttle half, brake full
    let mut raw = [0u8; 5];
    raw[0] = report_ids::STANDARD_INPUT; // 0x01
    // throttle = 0x0800 (half)
    raw[1] = 0x00;
    raw[2] = 0x08;
    // brake = 0x0FFF (full)
    raw[3] = 0xFF;
    raw[4] = 0x0F;

    // When: parsed
    let state = parse_pedal_report(&raw).ok_or("parse failed")?;

    // Then: raw values and axis count are correct
    assert_eq!(state.throttle_raw, 0x0800, "throttle must be half-pressed");
    assert_eq!(state.brake_raw, 0x0FFF, "brake must be fully pressed");
    assert_eq!(state.axis_count, 2, "5-byte report means 2 axes");

    Ok(())
}

// ─── Scenario 21: pedal report parses clutch on 3-axis set ───────────────────

#[test]
fn scenario_pedal_report_parses_clutch_axis() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::parse_pedal_report;

    // Given: a 3-axis ClubSport Pedals V3 report
    let mut raw = [0u8; 7];
    raw[0] = report_ids::STANDARD_INPUT;
    raw[1] = 0x00;
    raw[2] = 0x04; // throttle = 0x0400
    raw[3] = 0x00;
    raw[4] = 0x08; // brake   = 0x0800
    raw[5] = 0xFF;
    raw[6] = 0x0F; // clutch  = 0x0FFF

    // When: parsed
    let state = parse_pedal_report(&raw).ok_or("parse failed")?;

    // Then: clutch axis is present
    assert_eq!(state.clutch_raw, 0x0FFF, "clutch must be fully pressed");
    assert_eq!(state.axis_count, 3, "7-byte report means 3 axes");

    Ok(())
}

// ─── Scenario 22: ClubSport Pedals V3 PID is not a wheelbase ─────────────────

#[test]
fn scenario_clubsport_pedals_v3_pid_is_not_a_wheelbase() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::{is_pedal_product, is_wheelbase_product, product_ids};

    // Given: the ClubSport Pedals V3 product ID
    let pid = product_ids::CLUBSPORT_PEDALS_V3;

    // Then: recognised as a pedal, not a wheelbase
    assert!(is_pedal_product(pid), "CLUBSPORT_PEDALS_V3 must be a pedal product");
    assert!(!is_wheelbase_product(pid), "CLUBSPORT_PEDALS_V3 must not be a wheelbase");

    Ok(())
}

// ─── Scenario 23: Podium DD2 torque capacity is 25 Nm ────────────────────────

#[test]
fn scenario_podium_dd2_torque_capacity_is_25nm() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::{FanatecModel, product_ids};

    // Given: Podium DD2 product ID
    let model = FanatecModel::from_product_id(product_ids::DD2);

    // Then: maximum torque is 25 Nm
    assert!(
        (model.max_torque_nm() - 25.0).abs() < 0.1,
        "Podium DD2 max torque must be 25 Nm, got {}",
        model.max_torque_nm()
    );

    Ok(())
}

// ─── Scenario 24: rim ID byte 0x04 is McLaren GT3 V2 with funky switch ────────

#[test]
fn scenario_rim_id_mclaren_has_funky_switch() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::{FanatecRimId, rim_ids};

    // Given: rim ID byte from feature report 0x02
    let rim = FanatecRimId::from_byte(rim_ids::MCLAREN_GT3_V2);

    // Then: classified as McLaren GT3 V2 with all rim extras
    assert_eq!(rim, FanatecRimId::McLarenGt3V2);
    assert!(rim.has_funky_switch(), "McLaren GT3 V2 must have funky switch");
    assert!(rim.has_dual_clutch(), "McLaren GT3 V2 must have dual clutch paddles");
    assert!(rim.has_rotary_encoders(), "McLaren GT3 V2 must have rotary encoders");

    Ok(())
}
