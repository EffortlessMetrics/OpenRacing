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

// ── T150/TMX wire-format snapshot tests ──────────────────────────────────────

#[test]
fn test_snapshot_t150_range_max() {
    let report = tm::encode_range_t150(0xFFFF);
    assert_snapshot!(format!("{:02X?}", report));
}

#[test]
fn test_snapshot_t150_range_zero() {
    let report = tm::encode_range_t150(0x0000);
    assert_snapshot!(format!("{:02X?}", report));
}

#[test]
fn test_snapshot_t150_range_midpoint() {
    let report = tm::encode_range_t150(0x8000);
    assert_snapshot!(format!("{:02X?}", report));
}

#[test]
fn test_snapshot_t150_gain_full() {
    let report = tm::encode_gain_t150(0xFF);
    assert_snapshot!(format!("{:02X?}", report));
}

#[test]
fn test_snapshot_t150_gain_zero() {
    let report = tm::encode_gain_t150(0x00);
    assert_snapshot!(format!("{:02X?}", report));
}

#[test]
fn test_snapshot_t150_gain_half() {
    let report = tm::encode_gain_t150(0x80);
    assert_snapshot!(format!("{:02X?}", report));
}

#[test]
fn test_snapshot_t150_play_effect() {
    let report = tm::encode_play_effect_t150(0, 0x01, 1);
    assert_snapshot!(format!("{:02X?}", report));
}

#[test]
fn test_snapshot_t150_play_effect_infinite() {
    let report = tm::encode_play_effect_t150(3, 0x01, 0);
    assert_snapshot!(format!("{:02X?}", report));
}

#[test]
fn test_snapshot_t150_stop_effect() {
    let report = tm::encode_stop_effect_t150(0);
    assert_snapshot!(format!("{:02X?}", report));
}

#[test]
fn test_snapshot_t150_stop_effect_id5() {
    let report = tm::encode_stop_effect_t150(5);
    assert_snapshot!(format!("{:02X?}", report));
}

#[test]
fn test_snapshot_t150_effect_types() {
    let types = [
        tm::T150EffectType::Constant,
        tm::T150EffectType::Sine,
        tm::T150EffectType::SawtoothUp,
        tm::T150EffectType::SawtoothDown,
        tm::T150EffectType::Spring,
        tm::T150EffectType::Damper,
    ];
    let results: Vec<String> = types
        .iter()
        .map(|ty| format!("{:?}=0x{:04X}", ty, ty.as_u16()))
        .collect();
    assert_snapshot!(results.join(", "));
}

// ── Missing model protocol snapshots ─────────────────────────────────────────

#[test]
fn test_snapshot_model_t500rs_proper() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T500_RS);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_model_t248() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T248);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_model_t248x() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T248X);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_model_tgt2_gt_mode() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T_GT_II_GT);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_model_t80() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T80);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

#[test]
fn test_snapshot_model_t300rs_gt() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T300_RS_GT);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}

// ── All-models summary snapshot ──────────────────────────────────────────────

#[test]
fn test_snapshot_all_model_names() {
    let models = [
        tm::Model::T150,
        tm::Model::TMX,
        tm::Model::T300RS,
        tm::Model::T300RSPS4,
        tm::Model::T300RSGT,
        tm::Model::TXRacing,
        tm::Model::T500RS,
        tm::Model::T248,
        tm::Model::T248X,
        tm::Model::TGT,
        tm::Model::TGTII,
        tm::Model::TSPCRacer,
        tm::Model::TSXW,
        tm::Model::T818,
        tm::Model::T80,
        tm::Model::NascarProFF2,
        tm::Model::FGTRumbleForce,
        tm::Model::RGTFF,
        tm::Model::FGTForceFeedback,
        tm::Model::F430ForceFeedback,
        tm::Model::T3PA,
        tm::Model::T3PAPro,
        tm::Model::TLCM,
        tm::Model::TLCMPro,
        tm::Model::Unknown,
    ];
    let results: Vec<String> = models
        .iter()
        .map(|m| format!("{:?}: name={}, torque={}, rot={}, ffb={}", m, m.name(), m.max_torque_nm(), m.max_rotation_deg(), m.supports_ffb()))
        .collect();
    assert_snapshot!(results.join("\n"));
}

// ── Protocol constants snapshot ──────────────────────────────────────────────

#[test]
fn test_snapshot_protocol_constants() {
    let constants = format!(
        "VID=0x{:04X}\nFFB_WHEEL_GENERIC=0x{:04X}\nEFFECT_REPORT_LEN={}\nSTANDARD_INPUT_REPORT_ID=0x{:02X}",
        tm::THRUSTMASTER_VENDOR_ID,
        tm::product_ids::FFB_WHEEL_GENERIC,
        tm::EFFECT_REPORT_LEN,
        tm::input::STANDARD_INPUT_REPORT_ID,
    );
    assert_snapshot!(constants);
}

// ── Legacy wheel capability snapshots ────────────────────────────────────────

#[test]
fn test_snapshot_capability_legacy_wheels() {
    let legacy_pids = [
        tm::product_ids::NASCAR_PRO_FF2,
        tm::product_ids::FGT_RUMBLE_FORCE,
        tm::product_ids::RGT_FF_CLUTCH,
        tm::product_ids::FGT_FORCE_FEEDBACK,
        tm::product_ids::F430_FORCE_FEEDBACK,
    ];
    let results: Vec<String> = legacy_pids
        .iter()
        .map(|&pid| {
            let ident = tm::identify_device(pid);
            format!(
                "pid=0x{:04X}, name={}, category={:?}, ffb={}",
                ident.product_id, ident.name, ident.category, ident.supports_ffb
            )
        })
        .collect();
    assert_snapshot!(results.join("\n"));
}

// ── Init protocol constants snapshot ─────────────────────────────────────────

#[test]
fn test_snapshot_init_protocol_known_models() {
    let results: Vec<String> = tm::init_protocol::KNOWN_MODELS
        .iter()
        .map(|(model, switch, name)| {
            format!("model=0x{model:04X}, switch=0x{switch:04X}, name={name}")
        })
        .collect();
    assert_snapshot!(results.join("\n"));
}
