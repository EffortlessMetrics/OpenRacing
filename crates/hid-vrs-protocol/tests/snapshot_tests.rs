//! Snapshot tests for VRS DirectForce Pro HID protocol.

use insta::assert_snapshot;
use racing_wheel_hid_vrs_protocol as vrs;

#[test]
fn test_snapshot_encoder_pro() {
    let encoder = vrs::VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; vrs::CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(10.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_encoder_pro_negative() {
    let encoder = vrs::VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; vrs::CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(-15.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_encoder_pro_v2() {
    let encoder = vrs::VrsConstantForceEncoder::new(25.0);
    let mut out = [0u8; vrs::CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(12.5, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_spring_effect() {
    let encoder = vrs::VrsSpringEncoder::new(20.0);
    let mut out = [0u8; vrs::SPRING_REPORT_LEN];
    encoder.encode(5000, 0, 0, 500, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_damper_effect() {
    let encoder = vrs::VrsDamperEncoder::new(20.0);
    let mut out = [0u8; vrs::DAMPER_REPORT_LEN];
    encoder.encode(7500, 5000, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_friction_effect() {
    let encoder = vrs::VrsFrictionEncoder::new(20.0);
    let mut out = [0u8; vrs::FRICTION_REPORT_LEN];
    encoder.encode(3000, 2000, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_input_report_center() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![
        0x00, 0x00, // steering center (0)
        0x00, 0x00, // throttle (0)
        0x00, 0x00, // brake (0)
        0x00, 0x00, // clutch (0)
        0x00, 0x00, // buttons
        0x00, 0x00, // buttons
        0x0F, // hat center
        0x00, // encoder1
        0x00, // padding
        0x00, // encoder2
        0x00, 0x00, 0x00, // padding to reach 17 bytes
    ];
    let state =
        vrs::parse_input_report(&data).ok_or("parse should succeed for valid 17-byte data")?;
    assert_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, buttons={}, hat={}, encoder1={}, encoder2={}",
        state.steering,
        state.throttle,
        state.brake,
        state.clutch,
        state.buttons,
        state.hat,
        state.encoder1,
        state.encoder2
    ));
    Ok(())
}

#[test]
fn test_snapshot_input_report_full_throttle() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![
        0x00, 0x00, // steering center
        0xFF, 0xFF, // throttle full
        0x00, 0x00, // brake released
        0x00, 0x00, // clutch released
        0x00, 0x00, // buttons
        0x00, 0x00, // buttons
        0x00, // hat up
        0x00, // encoder1
        0x00, // padding
        0x00, // encoder2
        0x00, 0x00, // padding to reach 17 bytes
    ];
    let state =
        vrs::parse_input_report(&data).ok_or("parse should succeed for valid 17-byte data")?;
    assert_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}",
        state.steering, state.throttle, state.brake, state.clutch
    ));
    Ok(())
}

#[test]
fn test_snapshot_input_report_full_left() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 17];
    data[0] = 0x00; // steering low byte (-32768 = 0x8000)
    data[1] = 0x80;
    data[12] = 0x07; // hat left
    let state =
        vrs::parse_input_report(&data).ok_or("parse should succeed for valid 17-byte data")?;
    assert_snapshot!(format!("steering={:.4}, hat={}", state.steering, state.hat));
    Ok(())
}

#[test]
fn test_snapshot_input_report_full_right() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 17];
    data[0] = 0xFF; // steering high byte (32767 = 0x7FFF)
    data[1] = 0x7F;
    data[12] = 0x03; // hat right
    let state =
        vrs::parse_input_report(&data).ok_or("parse should succeed for valid 17-byte data")?;
    assert_snapshot!(format!("steering={:.4}, hat={}", state.steering, state.hat));
    Ok(())
}

#[test]
fn test_snapshot_input_report_pedals_full() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![
        0x00, 0x00, // steering center
        0xFF, 0xFF, // throttle full
        0xFF, 0xFF, // brake full
        0xFF, 0xFF, // clutch full
        0x00, 0x00, // buttons
        0x00, 0x00, // buttons
        0x00, // hat
        0x00, // encoder1
        0x00, // padding
        0x00, // encoder2
        0x00, 0x00, // padding to reach 17 bytes
    ];
    let state =
        vrs::parse_input_report(&data).ok_or("parse should succeed for valid 17-byte data")?;
    assert_snapshot!(format!(
        "throttle={:.4}, brake={:.4}, clutch={:.4}",
        state.throttle, state.brake, state.clutch
    ));
    Ok(())
}

#[test]
fn test_snapshot_input_report_encoders() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 17];
    data[13] = 0x2A; // encoder1 = 42
    data[15] = 0x80; // encoder2 = -128
    let state =
        vrs::parse_input_report(&data).ok_or("parse should succeed for valid 17-byte data")?;
    assert_snapshot!(format!(
        "encoder1={}, encoder2={}",
        state.encoder1, state.encoder2
    ));
    Ok(())
}

#[test]
fn test_snapshot_rotation_range_900() {
    let report = vrs::build_rotation_range(900);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_rotation_range_1080() {
    let report = vrs::build_rotation_range(1080);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_rotation_range_1440() {
    let report = vrs::build_rotation_range(1440);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_device_gain_full() {
    let report = vrs::build_device_gain(0xFF);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_device_gain_half() {
    let report = vrs::build_device_gain(0x80);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_device_gain_zero() {
    let report = vrs::build_device_gain(0x00);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_ffb_enable() {
    let report = vrs::build_ffb_enable(true);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_ffb_disable() {
    let report = vrs::build_ffb_enable(false);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_constant_force_setpoint_min() {
    let encoder = vrs::VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; vrs::CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(-20.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_constant_force_setpoint_max() {
    let encoder = vrs::VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; vrs::CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(20.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_constant_force_setpoint_midpoint() {
    let encoder = vrs::VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; vrs::CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(0.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_device_directforce_pro() {
    let identity = vrs::identify_device(0xA355);
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}, max_torque={:?}",
        identity.name, identity.category, identity.supports_ffb, identity.max_torque_nm
    ));
}

#[test]
fn test_snapshot_device_directforce_pro_v2() {
    let identity = vrs::identify_device(0xA356);
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}, max_torque={:?}",
        identity.name, identity.category, identity.supports_ffb, identity.max_torque_nm
    ));
}

#[test]
fn test_snapshot_device_pedals_v1() {
    let identity = vrs::identify_device(0xA357);
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}",
        identity.name, identity.category, identity.supports_ffb
    ));
}

#[test]
fn test_snapshot_device_pedals_v2() {
    let identity = vrs::identify_device(0xA358);
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}",
        identity.name, identity.category, identity.supports_ffb
    ));
}

#[test]
fn test_snapshot_device_unknown() {
    let identity = vrs::identify_device(0xFFFF);
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}, max_torque={:?}",
        identity.name, identity.category, identity.supports_ffb, identity.max_torque_nm
    ));
}

#[test]
fn test_snapshot_is_wheelbase_product() {
    let results = [
        ("DIRECTFORCE_PRO", vrs::is_wheelbase_product(0xA355)),
        ("DIRECTFORCE_PRO_V2", vrs::is_wheelbase_product(0xA356)),
        ("PEDALS_V1", vrs::is_wheelbase_product(0xA357)),
        ("PEDALS_V2", vrs::is_wheelbase_product(0xA358)),
        ("HANDBRAKE", vrs::is_wheelbase_product(0xA359)),
        ("SHIFTER", vrs::is_wheelbase_product(0xA35A)),
        ("UNKNOWN", vrs::is_wheelbase_product(0xFFFF)),
    ];
    assert_snapshot!(format!("{:?}", results));
}
