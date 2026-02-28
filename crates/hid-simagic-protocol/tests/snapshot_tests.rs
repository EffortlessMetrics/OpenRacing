//! Snapshot tests for Simagic HID protocol.

use insta::assert_snapshot;
use racing_wheel_hid_simagic_protocol as simagic;

#[test]
fn test_snapshot_encoder_alpha() {
    let encoder = simagic::SimagicConstantForceEncoder::new(15.0);
    let mut out = [0u8; simagic::CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(7.5, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_encoder_alpha_negative() {
    let encoder = simagic::SimagicConstantForceEncoder::new(15.0);
    let mut out = [0u8; simagic::CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(-10.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_encoder_m10() {
    let encoder = simagic::SimagicConstantForceEncoder::new(10.0);
    let mut out = [0u8; simagic::CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(5.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_spring_effect() {
    let encoder = simagic::SimagicSpringEncoder::new(15.0);
    let mut out = [0u8; simagic::SPRING_REPORT_LEN];
    encoder.encode(500, 0, 0, 50, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_damper_effect() {
    let encoder = simagic::SimagicDamperEncoder::new(15.0);
    let mut out = [0u8; simagic::DAMPER_REPORT_LEN];
    encoder.encode(750, 5000, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_friction_effect() {
    let encoder = simagic::SimagicFrictionEncoder::new(15.0);
    let mut out = [0u8; simagic::FRICTION_REPORT_LEN];
    encoder.encode(300, 2000, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_input_report_center() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![
        0x00, 0x80, // steering center
        0x00, 0x00, 0x00, 0x00, // pedals released
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // buttons
        0x08, // hat center
        0x00, 0x00, // rotary
        0xFF, // neutral gear
        0x00, // flags
        0x00, 0x00, 0x00, // quick release
    ];
    let state = simagic::parse_input_report(&data).ok_or("parse_input_report returned None")?;
    assert_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, handbrake={:.4}, buttons={}, hat={}, rotary1={}, rotary2={}, gear={:?}",
        state.steering,
        state.throttle,
        state.brake,
        state.clutch,
        state.handbrake,
        state.buttons,
        state.hat,
        state.rotary1,
        state.rotary2,
        state.shifter.gear
    ));
    Ok(())
}

#[test]
fn test_snapshot_input_report_full_throttle() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![
        0x00, 0x80, // steering center
        0xFF, 0xFF, // throttle full
        0x00, 0x00, // brake released
        0x00, 0x00, // clutch released
        0x00, 0x00, // handbrake released
        0x00, 0x00, // buttons
        0x00, // hat up
        0x00, 0x00, // rotary
        0x00, // first gear
        0x01, // clutch in range
        0x00, 0x00, 0x00, // quick release attached
    ];
    let state = simagic::parse_input_report(&data).ok_or("parse_input_report returned None")?;
    assert_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, gear={:?}, clutch_in_range={}",
        state.steering,
        state.throttle,
        state.brake,
        state.clutch,
        state.shifter.gear,
        state.shifter.clutch_in_range
    ));
    Ok(())
}

#[test]
fn test_snapshot_input_report_full_right() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![
        0xFF, 0xFF, // steering full right
        0x00, 0x00, 0x00, 0x00, // pedals
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // buttons
        0x04, // hat right
        0x00, 0x00, // rotary
        0xFF, // neutral
        0x00, // flags
        0x00, 0x00, 0x01, // quick release detached
    ];
    let state = simagic::parse_input_report(&data).ok_or("parse_input_report returned None")?;
    assert_snapshot!(format!(
        "steering={:.4}, hat={}, quick_release={:?}",
        state.steering, state.hat, state.quick_release
    ));
    Ok(())
}

#[test]
fn test_snapshot_rotation_range_900() {
    let report = simagic::build_rotation_range(900);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_rotation_range_1080() {
    let report = simagic::build_rotation_range(1080);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_rotation_range_1440() {
    let report = simagic::build_rotation_range(1440);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_device_gain_full() {
    let report = simagic::build_device_gain(0xFF);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_device_gain_half() {
    let report = simagic::build_device_gain(0x80);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_device_gain_zero() {
    let report = simagic::build_device_gain(0x00);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_led_report() {
    let report = simagic::build_led_report(0b00001111);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_sine_effect() {
    let report = simagic::build_sine_effect(500, 2.0, 90);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_square_effect() {
    let report = simagic::build_square_effect(750, 5.0, 50);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_triangle_effect() {
    let report = simagic::build_triangle_effect(300, 1.5);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_device_alpha() {
    let identity = simagic::identify_device(0x0500); // EVO Sport (correct VID 0x3670 product)
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}, max_torque={:?}",
        identity.name, identity.category, identity.supports_ffb, identity.max_torque_nm
    ));
}

#[test]
fn test_snapshot_device_alpha_mini() {
    let identity = simagic::identify_device(0x0501); // EVO
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}, max_torque={:?}",
        identity.name, identity.category, identity.supports_ffb, identity.max_torque_nm
    ));
}

#[test]
fn test_snapshot_device_alpha_evo() {
    let identity = simagic::identify_device(0x0502); // EVO Pro
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}, max_torque={:?}",
        identity.name, identity.category, identity.supports_ffb, identity.max_torque_nm
    ));
}

#[test]
fn test_snapshot_device_m10() {
    let identity = simagic::identify_device(0x0600); // Alpha EVO (estimated)
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}, max_torque={:?}",
        identity.name, identity.category, identity.supports_ffb, identity.max_torque_nm
    ));
}

#[test]
fn test_snapshot_device_neo() {
    let identity = simagic::identify_device(0x0700); // Neo
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}, max_torque={:?}",
        identity.name, identity.category, identity.supports_ffb, identity.max_torque_nm
    ));
}

#[test]
fn test_snapshot_device_pedals() {
    let identity = simagic::identify_device(0x1001);
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}, max_torque={:?}",
        identity.name, identity.category, identity.supports_ffb, identity.max_torque_nm
    ));
}

#[test]
fn test_snapshot_device_unknown() {
    let identity = simagic::identify_device(0xFFFF);
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}, max_torque={:?}",
        identity.name, identity.category, identity.supports_ffb, identity.max_torque_nm
    ));
}

#[test]
fn test_snapshot_is_wheelbase_product() {
    let results = [
        ("EVO_SPORT", simagic::is_wheelbase_product(0x0500)),
        ("EVO", simagic::is_wheelbase_product(0x0501)),
        ("EVO_PRO", simagic::is_wheelbase_product(0x0502)),
        ("ALPHA_EVO", simagic::is_wheelbase_product(0x0600)),
        ("NEO", simagic::is_wheelbase_product(0x0700)),
        ("NEO_MINI", simagic::is_wheelbase_product(0x0701)),
        ("P1000_PEDALS", simagic::is_wheelbase_product(0x1001)),
        ("UNKNOWN", simagic::is_wheelbase_product(0xFFFF)),
    ];
    assert_snapshot!(format!("{:?}", results));
}
