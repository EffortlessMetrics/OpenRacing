//! Snapshot tests for SimpleMotion V2 HID protocol.

use insta::assert_snapshot;
use racing_wheel_simplemotion_v2 as sm;

#[test]
fn test_snapshot_torque_command_positive() {
    let mut enc = sm::TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; sm::TORQUE_COMMAND_LEN];
    enc.encode(10.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_torque_command_negative() {
    let mut enc = sm::TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; sm::TORQUE_COMMAND_LEN];
    enc.encode(-15.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_torque_command_zero() {
    let mut enc = sm::TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; sm::TORQUE_COMMAND_LEN];
    enc.encode_zero(&mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_torque_encoder_ioni_premium() {
    let mut enc = sm::TorqueCommandEncoder::new(35.0);
    let mut out = [0u8; sm::TORQUE_COMMAND_LEN];
    enc.encode(17.5, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_torque_encoder_argon() {
    let mut enc = sm::TorqueCommandEncoder::new(10.0);
    let mut out = [0u8; sm::TORQUE_COMMAND_LEN];
    enc.encode(5.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_torque_with_velocity() {
    let mut enc = sm::TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; sm::TORQUE_COMMAND_LEN];
    enc.encode_with_velocity(5.0, 100.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_set_zero_position() {
    let out = sm::build_set_zero_position(0);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_device_enable() {
    let out = sm::build_device_enable(true, 0);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_device_disable() {
    let out = sm::build_device_enable(false, 0);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_feedback_report_center() {
    let mut data = vec![0u8; 64];
    data[0] = 0x02;
    data[1] = 0x05;
    data[2] = 0x00;
    let state = sm::parse_feedback_report(&data).expect("parse should succeed");
    assert_snapshot!(format!(
        "seq={}, status={:?}, pos={}, vel={}, torque={}",
        state.seq, state.status, state.motor.position, state.motor.velocity, state.motor.torque
    ));
}

#[test]
fn test_snapshot_feedback_report_full_torque() {
    let mut data = vec![0u8; 64];
    data[0] = 0x02;
    data[1] = 0x10;
    data[2] = 0x00;
    data[12] = 0x00;
    data[13] = 0x80;
    let state = sm::parse_feedback_report(&data).expect("parse should succeed");
    assert_snapshot!(format!(
        "torque={:.4}, torque_nm={:.4}",
        state.motor.torque as f32,
        state.torque_nm(0.1)
    ));
}

#[test]
fn test_snapshot_feedback_report_position() {
    let mut data = vec![0u8; 64];
    data[0] = 0x02;
    data[1] = 0x01;
    data[2] = 0x00;
    data[4] = 0x00;
    data[5] = 0x10;
    data[6] = 0x00;
    data[7] = 0x00;
    let state = sm::parse_feedback_report(&data).expect("parse should succeed");
    assert_snapshot!(format!(
        "position={}, position_degrees={:.4}",
        state.motor.position,
        state.position_degrees(4)
    ));
}

#[test]
fn test_snapshot_feedback_report_velocity() {
    let mut data = vec![0u8; 64];
    data[0] = 0x02;
    data[1] = 0x01;
    data[2] = 0x00;
    data[8] = 0x00;
    data[9] = 0x20;
    data[10] = 0x00;
    data[11] = 0x00;
    let state = sm::parse_feedback_report(&data).expect("parse should succeed");
    assert_snapshot!(format!(
        "velocity={}, velocity_rpm={:.4}",
        state.motor.velocity,
        state.velocity_rpm(4)
    ));
}

#[test]
fn test_snapshot_get_parameter() {
    let out = sm::build_get_parameter(0x1001, 0);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_set_parameter() {
    let out = sm::build_set_parameter(0x1001, 1000, 0);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_get_status() {
    let out = sm::build_get_status(0);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_device_ioni() {
    let identity = sm::identify_device(0x6050);
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}, max_torque={:?}",
        identity.name, identity.category, identity.supports_ffb, identity.max_torque_nm
    ));
}

#[test]
fn test_snapshot_device_ioni_premium() {
    let identity = sm::identify_device(0x6051);
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}, max_torque={:?}",
        identity.name, identity.category, identity.supports_ffb, identity.max_torque_nm
    ));
}

#[test]
fn test_snapshot_device_argon() {
    let identity = sm::identify_device(0x6052);
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}, max_torque={:?}",
        identity.name, identity.category, identity.supports_ffb, identity.max_torque_nm
    ));
}

#[test]
fn test_snapshot_device_simucube_1() {
    let identity = sm::identify_device(0x6050);
    assert_snapshot!(format!(
        "name={}, category={:?}, max_rpm={:?}",
        identity.name, identity.category, identity.max_rpm
    ));
}

#[test]
fn test_snapshot_device_simucube_2() {
    let identity = sm::identify_device(0x6051);
    assert_snapshot!(format!(
        "name={}, category={:?}, max_rpm={:?}",
        identity.name, identity.category, identity.max_rpm
    ));
}

#[test]
fn test_snapshot_device_simucube_sport() {
    let identity = sm::identify_device(0x6052);
    assert_snapshot!(format!(
        "name={}, category={:?}, max_rpm={:?}",
        identity.name, identity.category, identity.max_rpm
    ));
}

#[test]
fn test_snapshot_device_unknown() {
    let identity = sm::identify_device(0xFFFF);
    assert_snapshot!(format!(
        "name={}, category={:?}, supports_ffb={}",
        identity.name, identity.category, identity.supports_ffb
    ));
}

#[test]
fn test_snapshot_is_wheelbase_product() {
    let results = [
        ("IONI", sm::is_wheelbase_product(0x6050)),
        ("IONI_PREMIUM", sm::is_wheelbase_product(0x6051)),
        ("ARGON", sm::is_wheelbase_product(0x6052)),
        ("UNKNOWN", sm::is_wheelbase_product(0xFFFF)),
    ];
    assert_snapshot!(format!("{:?}", results));
}
