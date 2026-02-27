use insta::assert_snapshot;
use racing_wheel_hid_thrustmaster_protocol as tm;

#[test]
fn test_snapshot_tgt_encoder() {
    let encoder = tm::ThrustmasterConstantForceEncoder::new(6.0);
    let mut out = [0u8; 8];
    encoder.encode(3.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_t818_encoder() {
    let encoder = tm::ThrustmasterConstantForceEncoder::new(10.0);
    let mut out = [0u8; 8];
    encoder.encode(-5.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_input_report_center() {
    let data = vec![
        0x01, 0x00, 0x80, // steering center
        0x00, 0x00, 0x00, // pedals
        0x00, 0x00, // buttons
        0x08, // hat center
        0x00, // paddles
    ];
    let state = tm::parse_input_report(&data).expect("parse should succeed");
    assert_snapshot!(format!(
        "steering={}, throttle={}, brake={}, clutch={}, buttons={}, hat={}, paddles={}/{}",
        state.steering,
        state.throttle,
        state.brake,
        state.clutch,
        state.buttons,
        state.hat,
        state.paddle_right as u8,
        state.paddle_left as u8
    ));
}

#[test]
fn test_snapshot_input_report_full_throttle() {
    let data = vec![
        0x01, 0x00, 0x80, // steering center
        0xFF, 0x00, 0x00, // throttle full, others zero
        0x00, 0x00, // buttons
        0x00, // hat up
        0x03, // both paddles
    ];
    let state = tm::parse_input_report(&data).expect("parse should succeed");
    assert_snapshot!(format!(
        "steering={}, throttle={}, brake={}, clutch={}, buttons={}, hat={}, paddles={}/{}",
        state.steering,
        state.throttle,
        state.brake,
        state.clutch,
        state.buttons,
        state.hat,
        state.paddle_right as u8,
        state.paddle_left as u8
    ));
}

#[test]
fn test_snapshot_set_range_900() {
    let report = tm::build_set_range_report(900);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_set_range_1080() {
    let report = tm::build_set_range_report(1080);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_device_gain() {
    let report = tm::build_device_gain(0x80);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_actuator_enable() {
    let report = tm::build_actuator_enable(true);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_spring_effect() {
    let effect = tm::build_spring_effect(0, 500);
    assert_snapshot!(format!("{:?}", effect));
}

#[test]
fn test_snapshot_damper_effect() {
    let effect = tm::build_damper_effect(300);
    assert_snapshot!(format!("{:?}", effect));
}

#[test]
fn test_snapshot_friction_effect() {
    let effect = tm::build_friction_effect(100, 800);
    assert_snapshot!(format!("{:?}", effect));
}

#[test]
fn test_snapshot_protocol_tgt() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T_GT);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_protocol_t818() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T818);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_protocol_t_lcm() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T_LCM);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, pedals={}, wheelbase={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.is_pedals(),
        proto.is_wheelbase()
    ));
}
