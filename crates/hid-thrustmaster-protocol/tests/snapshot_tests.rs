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
fn test_snapshot_input_report_center() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![
        0x01, 0x00, 0x80, // steering center
        0x00, 0x00, 0x00, // pedals
        0x00, 0x00, // buttons
        0x08, // hat center
        0x00, // paddles
    ];
    let state = tm::parse_input_report(&data).ok_or("parse_input_report returned None")?;
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
    Ok(())
}

#[test]
fn test_snapshot_input_report_full_throttle() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![
        0x01, 0x00, 0x80, // steering center
        0xFF, 0x00, 0x00, // throttle full, others zero
        0x00, 0x00, // buttons
        0x00, // hat up
        0x03, // both paddles
    ];
    let state = tm::parse_input_report(&data).ok_or("parse_input_report returned None")?;
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
    Ok(())
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
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::TS_XW);
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
fn test_snapshot_protocol_unknown_pid() {
    let proto = tm::ThrustmasterProtocol::new(0xFFFF);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, pedals={}, wheelbase={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.is_pedals(),
        proto.is_wheelbase()
    ));
}

// ── Per-model protocol snapshots ─────────────────────────────────────────────

#[test]
fn test_snapshot_model_t300rs() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T300_RS);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_model_t300rs_ps4() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T300_RS_PS4);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_model_t500rs() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T150);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_model_tmx() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::TMX);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_model_tx_racing() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::TX_RACING);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_model_ts_xw() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::TS_XW);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_model_ts_pc_racer() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::TS_PC_RACER);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_model_tgt2() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T300_RS_PS4);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

// ── Device capability lookup snapshots ───────────────────────────────────────

#[test]
fn test_snapshot_capability_t300rs() {
    let ident = tm::identify_device(tm::product_ids::T300_RS);
    assert_snapshot!(format!(
        "pid=0x{:04X}, name={}, category={:?}, ffb={}",
        ident.product_id, ident.name, ident.category, ident.supports_ffb
    ));
}

#[test]
fn test_snapshot_capability_t500rs() {
    let ident = tm::identify_device(tm::product_ids::T150);
    assert_snapshot!(format!(
        "pid=0x{:04X}, name={}, category={:?}, ffb={}",
        ident.product_id, ident.name, ident.category, ident.supports_ffb
    ));
}

#[test]
fn test_snapshot_capability_ts_xw() {
    let ident = tm::identify_device(tm::product_ids::TS_XW);
    assert_snapshot!(format!(
        "pid=0x{:04X}, name={}, category={:?}, ffb={}",
        ident.product_id, ident.name, ident.category, ident.supports_ffb
    ));
}

// ── is_wheelbase_product checks ───────────────────────────────────────────────

#[test]
fn test_snapshot_is_wheelbase_known_pids() {
    let wheelbase_pids = [
        tm::product_ids::T150,
        tm::product_ids::T300_RS,
        tm::product_ids::T300_RS_PS4,
        tm::product_ids::T300_RS_GT,
        tm::product_ids::TMX,
        tm::product_ids::TX_RACING,
        tm::product_ids::TS_XW,
        tm::product_ids::TS_XW_GIP,
        tm::product_ids::TS_PC_RACER,
        tm::product_ids::T818,
        tm::product_ids::T248,
    ];
    let results: Vec<String> = wheelbase_pids
        .iter()
        .map(|&pid| format!("0x{pid:04X}={}", tm::is_wheel_product(pid)))
        .collect();
    assert_snapshot!(results.join(", "));
}
