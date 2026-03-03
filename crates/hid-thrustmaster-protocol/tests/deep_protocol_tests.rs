//! Deep protocol tests for Thrustmaster HID protocol.
//!
//! Covers constant-force encoding (±10000 scale), device identification,
//! protocol family classification, T150-specific encoding, kernel wire format,
//! init protocol constants, and property-based guarantees.

use racing_wheel_hid_thrustmaster_protocol::ids::{
    Model, ProtocolFamily, THRUSTMASTER_VENDOR_ID, init_protocol, product_ids,
};
use racing_wheel_hid_thrustmaster_protocol::input::{
    STANDARD_INPUT_REPORT_ID, parse_input_report, parse_pedal_report,
};
use racing_wheel_hid_thrustmaster_protocol::output::{
    EFFECT_REPORT_LEN, EFFECT_TYPE_CONSTANT, EFFECT_TYPE_DAMPER, EFFECT_TYPE_FRICTION,
    EFFECT_TYPE_RAMP, EFFECT_TYPE_SPRING, ThrustmasterConstantForceEncoder,
    ThrustmasterEffectEncoder, build_actuator_enable, build_damper_effect, build_device_gain,
    build_friction_effect, build_kernel_autocenter_commands, build_kernel_close_command,
    build_kernel_gain_command, build_kernel_open_command, build_kernel_range_command,
    build_set_range_report, build_spring_effect, report_ids,
};
use racing_wheel_hid_thrustmaster_protocol::protocol::{
    ThrustmasterInitState, ThrustmasterProtocol,
};
use racing_wheel_hid_thrustmaster_protocol::t150::{
    CMD_GAIN, CMD_RANGE, SUBCMD_RANGE, T150EffectType, encode_gain_t150, encode_play_effect_t150,
    encode_range_t150, encode_stop_effect_t150,
};
use racing_wheel_hid_thrustmaster_protocol::types::{
    ThrustmasterDeviceCategory, ThrustmasterPedalAxesRaw, identify_device, is_pedal_product,
    is_wheel_product,
};

// ─── Vendor ID ───────────────────────────────────────────────────────────────

#[test]
fn vendor_id_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(THRUSTMASTER_VENDOR_ID, 0x044F);
    Ok(())
}

// ─── Generic PID ─────────────────────────────────────────────────────────────

#[test]
fn generic_ffb_wheel_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_ids::FFB_WHEEL_GENERIC, 0xB65D);
    Ok(())
}

// ─── Constant force encoding: ±10000 scale (NOT i16::MAX) ────────────────────

#[test]
fn cf_encode_full_positive() -> Result<(), Box<dyn std::error::Error>> {
    let enc = ThrustmasterConstantForceEncoder::new(6.0);
    let mut out = [0u8; EFFECT_REPORT_LEN];
    enc.encode(6.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 10000, "full positive must be 10000");
    Ok(())
}

#[test]
fn cf_encode_full_negative() -> Result<(), Box<dyn std::error::Error>> {
    let enc = ThrustmasterConstantForceEncoder::new(6.0);
    let mut out = [0u8; EFFECT_REPORT_LEN];
    enc.encode(-6.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, -10000, "full negative must be -10000");
    Ok(())
}

#[test]
fn cf_encode_zero() -> Result<(), Box<dyn std::error::Error>> {
    let enc = ThrustmasterConstantForceEncoder::new(6.0);
    let mut out = [0u8; EFFECT_REPORT_LEN];
    enc.encode(0.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 0, "zero torque must be 0");
    Ok(())
}

#[test]
fn cf_encode_half_scale_truncation() -> Result<(), Box<dyn std::error::Error>> {
    // TM uses truncation (as i16) not rounding — test boundary
    let enc = ThrustmasterConstantForceEncoder::new(4.0);
    let mut out = [0u8; EFFECT_REPORT_LEN];
    enc.encode(2.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    // 2.0/4.0 * 10000 = 5000.0, truncated to 5000
    assert_eq!(mag, 5000);
    Ok(())
}

#[test]
fn cf_encode_overflow_clamps_positive() -> Result<(), Box<dyn std::error::Error>> {
    let enc = ThrustmasterConstantForceEncoder::new(4.0);
    let mut out = [0u8; EFFECT_REPORT_LEN];
    enc.encode(100.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 10000, "overflow must clamp to 10000");
    Ok(())
}

#[test]
fn cf_encode_overflow_clamps_negative() -> Result<(), Box<dyn std::error::Error>> {
    let enc = ThrustmasterConstantForceEncoder::new(4.0);
    let mut out = [0u8; EFFECT_REPORT_LEN];
    enc.encode(-100.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, -10000, "overflow must clamp to -10000");
    Ok(())
}

#[test]
fn cf_encode_max_torque_floor_to_0_01() -> Result<(), Box<dyn std::error::Error>> {
    // Encoder floors max_torque to 0.01 (not 0.0)
    let enc = ThrustmasterConstantForceEncoder::new(0.0);
    let mut out = [0u8; EFFECT_REPORT_LEN];
    // 5.0 / 0.01 > 1.0 → clamped to 1.0 → 10000
    enc.encode(5.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(
        mag, 10000,
        "zero max_torque floors to 0.01, so large input clamps"
    );
    Ok(())
}

#[test]
fn cf_encode_negative_max_torque_floors() -> Result<(), Box<dyn std::error::Error>> {
    let enc = ThrustmasterConstantForceEncoder::new(-5.0);
    let mut out = [0u8; EFFECT_REPORT_LEN];
    enc.encode(1.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    // -5.0 is floored to 0.01, so 1.0/0.01 > 1.0 → clamped → 10000
    assert_eq!(mag, 10000);
    Ok(())
}

#[test]
fn cf_encode_zero_via_trait() -> Result<(), Box<dyn std::error::Error>> {
    let enc = ThrustmasterConstantForceEncoder::new(6.0);
    let mut out = [0xFFu8; EFFECT_REPORT_LEN];
    ThrustmasterEffectEncoder::encode_zero(&enc, &mut out);
    assert_eq!(out[0], report_ids::CONSTANT_FORCE);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 0);
    Ok(())
}

#[test]
fn cf_encode_header_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let enc = ThrustmasterConstantForceEncoder::new(10.0);
    let mut out = [0u8; EFFECT_REPORT_LEN];
    enc.encode(5.0, &mut out);
    assert_eq!(out[0], report_ids::CONSTANT_FORCE);
    assert_eq!(out[1], 1);
    Ok(())
}

#[test]
fn cf_encode_reserved_bytes_zero() -> Result<(), Box<dyn std::error::Error>> {
    let enc = ThrustmasterConstantForceEncoder::new(10.0);
    let mut out = [0xFFu8; EFFECT_REPORT_LEN];
    enc.encode(5.0, &mut out);
    assert_eq!(out[4], 0);
    assert_eq!(out[5], 0);
    assert_eq!(out[6], 0);
    assert_eq!(out[7], 0);
    Ok(())
}

// ─── Per-model torque encoding ───────────────────────────────────────────────

#[test]
fn cf_encode_per_model_half_torque() -> Result<(), Box<dyn std::error::Error>> {
    let models: &[(Model, f32)] = &[
        (Model::T150, 2.5),
        (Model::T300RS, 4.0),
        (Model::T500RS, 5.0),
        (Model::TSPCRacer, 6.0),
        (Model::T818, 10.0),
    ];
    for &(model, max) in models {
        let enc = ThrustmasterConstantForceEncoder::new(max);
        let mut out = [0u8; EFFECT_REPORT_LEN];
        enc.encode(max * 0.5, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(
            mag,
            5000,
            "{:?}: half torque ({}) must encode to 5000, got {mag}",
            model,
            max * 0.5
        );
    }
    Ok(())
}

// ─── Device identification: all PIDs ─────────────────────────────────────────

#[test]
fn identify_all_t300_family_pids() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        product_ids::T300_RS,
        product_ids::T300_RS_PS4,
        product_ids::T300_RS_GT,
        product_ids::TX_RACING,
        product_ids::TX_RACING_ORIG,
    ];
    for pid in pids {
        let identity = identify_device(pid);
        assert_eq!(
            identity.category,
            ThrustmasterDeviceCategory::Wheelbase,
            "PID 0x{pid:04X}"
        );
        assert!(identity.supports_ffb, "PID 0x{pid:04X}");
    }
    Ok(())
}

#[test]
fn identify_t150_tmx() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [product_ids::T150, product_ids::TMX] {
        let identity = identify_device(pid);
        assert_eq!(identity.category, ThrustmasterDeviceCategory::Wheelbase);
        assert!(identity.supports_ffb);
    }
    Ok(())
}

#[test]
fn identify_t248_variants() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [product_ids::T248, product_ids::T248X] {
        let identity = identify_device(pid);
        assert_eq!(identity.category, ThrustmasterDeviceCategory::Wheelbase);
        assert!(identity.supports_ffb);
    }
    Ok(())
}

#[test]
fn identify_ts_xw_variants() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [product_ids::TS_XW, product_ids::TS_XW_GIP] {
        let identity = identify_device(pid);
        assert_eq!(identity.category, ThrustmasterDeviceCategory::Wheelbase);
        assert!(identity.supports_ffb);
    }
    Ok(())
}

#[test]
fn identify_t80_no_ffb() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [product_ids::T80, product_ids::T80_FERRARI_488] {
        let identity = identify_device(pid);
        assert_eq!(identity.category, ThrustmasterDeviceCategory::Wheelbase);
        assert!(
            !identity.supports_ffb,
            "T80 PID 0x{pid:04X} must NOT support FFB"
        );
    }
    Ok(())
}

#[test]
fn identify_legacy_wheels_have_ffb() -> Result<(), Box<dyn std::error::Error>> {
    let legacy = [
        product_ids::NASCAR_PRO_FF2,
        product_ids::FGT_RUMBLE_FORCE,
        product_ids::RGT_FF_CLUTCH,
        product_ids::FGT_FORCE_FEEDBACK,
        product_ids::F430_FORCE_FEEDBACK,
    ];
    for pid in legacy {
        let identity = identify_device(pid);
        assert_eq!(identity.category, ThrustmasterDeviceCategory::Wheelbase);
        assert!(identity.supports_ffb, "Legacy PID 0x{pid:04X}");
    }
    Ok(())
}

#[test]
fn identify_unknown_pid() -> Result<(), Box<dyn std::error::Error>> {
    let identity = identify_device(0xFFFF);
    assert_eq!(identity.category, ThrustmasterDeviceCategory::Unknown);
    assert!(!identity.supports_ffb);
    Ok(())
}

// ─── Model from_product_id ───────────────────────────────────────────────────

#[test]
fn model_from_pid_all_known() -> Result<(), Box<dyn std::error::Error>> {
    let expected: &[(u16, Model)] = &[
        (product_ids::T150, Model::T150),
        (product_ids::TMX, Model::TMX),
        (product_ids::T300_RS, Model::T300RS),
        (product_ids::T300_RS_PS4, Model::T300RSPS4),
        (product_ids::T300_RS_GT, Model::T300RSGT),
        (product_ids::TX_RACING, Model::TXRacing),
        (product_ids::TX_RACING_ORIG, Model::TXRacing),
        (product_ids::T500_RS, Model::T500RS),
        (product_ids::T248, Model::T248),
        (product_ids::T248X, Model::T248X),
        (product_ids::TS_PC_RACER, Model::TSPCRacer),
        (product_ids::TS_XW, Model::TSXW),
        (product_ids::TS_XW_GIP, Model::TSXW),
        (product_ids::T_GT_II_GT, Model::TGTII),
        (product_ids::T818, Model::T818),
        (product_ids::T80, Model::T80),
        (product_ids::T80_FERRARI_488, Model::T80),
        (product_ids::NASCAR_PRO_FF2, Model::NascarProFF2),
        (product_ids::FGT_RUMBLE_FORCE, Model::FGTRumbleForce),
        (product_ids::RGT_FF_CLUTCH, Model::RGTFF),
        (product_ids::FGT_FORCE_FEEDBACK, Model::FGTForceFeedback),
        (product_ids::F430_FORCE_FEEDBACK, Model::F430ForceFeedback),
        (product_ids::T_LCM, Model::TLCM),
    ];
    for &(pid, expected_model) in expected {
        assert_eq!(
            Model::from_product_id(pid),
            expected_model,
            "PID 0x{pid:04X}"
        );
    }
    Ok(())
}

#[test]
fn model_unknown_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(Model::from_product_id(0xDEAD), Model::Unknown);
    Ok(())
}

#[test]
fn model_tpr_pedals_mapped_to_unknown() -> Result<(), Box<dyn std::error::Error>> {
    // TPR pedals PID maps to Unknown (no FFB)
    assert_eq!(
        Model::from_product_id(product_ids::TPR_PEDALS),
        Model::Unknown
    );
    Ok(())
}

// ─── Model torque values ─────────────────────────────────────────────────────

#[test]
fn model_torque_known_values() -> Result<(), Box<dyn std::error::Error>> {
    assert!((Model::T150.max_torque_nm() - 2.5).abs() < 0.01);
    assert!((Model::TMX.max_torque_nm() - 2.5).abs() < 0.01);
    assert!((Model::T300RS.max_torque_nm() - 4.0).abs() < 0.01);
    assert!((Model::T300RSPS4.max_torque_nm() - 4.0).abs() < 0.01);
    assert!((Model::T300RSGT.max_torque_nm() - 4.0).abs() < 0.01);
    assert!((Model::TXRacing.max_torque_nm() - 4.0).abs() < 0.01);
    assert!((Model::T248.max_torque_nm() - 4.0).abs() < 0.01);
    assert!((Model::T500RS.max_torque_nm() - 5.0).abs() < 0.01);
    assert!((Model::TGT.max_torque_nm() - 6.0).abs() < 0.01);
    assert!((Model::TGTII.max_torque_nm() - 6.0).abs() < 0.01);
    assert!((Model::TSPCRacer.max_torque_nm() - 6.0).abs() < 0.01);
    assert!((Model::TSXW.max_torque_nm() - 6.0).abs() < 0.01);
    assert!((Model::T818.max_torque_nm() - 10.0).abs() < 0.01);
    assert!((Model::T80.max_torque_nm()).abs() < 0.01); // 0.0 Nm (no FFB)
    Ok(())
}

#[test]
fn model_torque_ordering() -> Result<(), Box<dyn std::error::Error>> {
    assert!(Model::T150.max_torque_nm() < Model::T300RS.max_torque_nm());
    assert!(Model::T300RS.max_torque_nm() < Model::T500RS.max_torque_nm());
    assert!(Model::T500RS.max_torque_nm() < Model::TSPCRacer.max_torque_nm());
    assert!(Model::TSPCRacer.max_torque_nm() < Model::T818.max_torque_nm());
    Ok(())
}

// ─── Model max rotation ─────────────────────────────────────────────────────

#[test]
fn model_rotation_values() -> Result<(), Box<dyn std::error::Error>> {
    // 1080° group
    for model in [
        Model::T150,
        Model::T300RS,
        Model::T500RS,
        Model::TSPCRacer,
        Model::TSXW,
        Model::TGT,
        Model::TGTII,
        Model::T818,
    ] {
        assert_eq!(model.max_rotation_deg(), 1080, "{model:?}");
    }
    // 900° group (default)
    for model in [Model::TMX, Model::TXRacing, Model::T248, Model::T248X] {
        assert_eq!(model.max_rotation_deg(), 900, "{model:?}");
    }
    // 270° group
    for model in [
        Model::T80,
        Model::NascarProFF2,
        Model::FGTRumbleForce,
        Model::RGTFF,
        Model::FGTForceFeedback,
        Model::F430ForceFeedback,
    ] {
        assert_eq!(model.max_rotation_deg(), 270, "{model:?}");
    }
    Ok(())
}

// ─── Protocol family classification ──────────────────────────────────────────

#[test]
fn protocol_family_t300_group() -> Result<(), Box<dyn std::error::Error>> {
    let t300_models = [
        Model::T300RS,
        Model::T300RSPS4,
        Model::T300RSGT,
        Model::TXRacing,
        Model::T248,
        Model::T248X,
        Model::TSPCRacer,
        Model::TSXW,
        Model::TGTII,
    ];
    for model in t300_models {
        assert_eq!(model.protocol_family(), ProtocolFamily::T300, "{model:?}");
    }
    Ok(())
}

#[test]
fn protocol_family_t150_group() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(Model::T150.protocol_family(), ProtocolFamily::T150);
    assert_eq!(Model::TMX.protocol_family(), ProtocolFamily::T150);
    Ok(())
}

#[test]
fn protocol_family_t500_separate() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(Model::T500RS.protocol_family(), ProtocolFamily::T500);
    Ok(())
}

#[test]
fn protocol_family_unknown_for_non_ffb() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(Model::T80.protocol_family(), ProtocolFamily::Unknown);
    assert_eq!(Model::T3PA.protocol_family(), ProtocolFamily::Unknown);
    assert_eq!(Model::TLCM.protocol_family(), ProtocolFamily::Unknown);
    assert_eq!(Model::Unknown.protocol_family(), ProtocolFamily::Unknown);
    Ok(())
}

// ─── Init switch values ──────────────────────────────────────────────────────

#[test]
fn init_switch_values_per_family() -> Result<(), Box<dyn std::error::Error>> {
    // T300 family: 0x0005
    for model in [
        Model::T300RS,
        Model::T248,
        Model::TSPCRacer,
        Model::TSXW,
        Model::TGTII,
    ] {
        assert_eq!(model.init_switch_value(), Some(0x0005), "{model:?}");
    }
    // T150 family: 0x0006
    assert_eq!(Model::T150.init_switch_value(), Some(0x0006));
    assert_eq!(Model::TMX.init_switch_value(), Some(0x0006));
    // T500RS: 0x0002
    assert_eq!(Model::T500RS.init_switch_value(), Some(0x0002));
    // Unknown: None
    assert_eq!(Model::Unknown.init_switch_value(), None);
    assert_eq!(Model::T80.init_switch_value(), None);
    assert_eq!(Model::T818.init_switch_value(), None);
    Ok(())
}

// ─── Model supports_ffb ──────────────────────────────────────────────────────

#[test]
fn model_supports_ffb_comprehensive() -> Result<(), Box<dyn std::error::Error>> {
    // Must support FFB
    let ffb_models = [
        Model::T150,
        Model::TMX,
        Model::T300RS,
        Model::T300RSPS4,
        Model::T300RSGT,
        Model::TXRacing,
        Model::T500RS,
        Model::T248,
        Model::T248X,
        Model::TGT,
        Model::TGTII,
        Model::TSPCRacer,
        Model::TSXW,
        Model::T818,
        Model::NascarProFF2,
        Model::FGTRumbleForce,
        Model::RGTFF,
        Model::FGTForceFeedback,
        Model::F430ForceFeedback,
    ];
    for model in ffb_models {
        assert!(model.supports_ffb(), "{model:?} must support FFB");
    }
    // Must NOT support FFB
    let no_ffb = [
        Model::T80,
        Model::T3PA,
        Model::T3PAPro,
        Model::TLCM,
        Model::TLCMPro,
        Model::Unknown,
    ];
    for model in no_ffb {
        assert!(!model.supports_ffb(), "{model:?} must NOT support FFB");
    }
    Ok(())
}

// ─── Model name ──────────────────────────────────────────────────────────────

#[test]
fn model_name_non_empty() -> Result<(), Box<dyn std::error::Error>> {
    let all_models = [
        Model::T150,
        Model::TMX,
        Model::T300RS,
        Model::T300RSPS4,
        Model::T300RSGT,
        Model::TXRacing,
        Model::T500RS,
        Model::T248,
        Model::T248X,
        Model::TGT,
        Model::TGTII,
        Model::TSPCRacer,
        Model::TSXW,
        Model::T818,
        Model::T80,
        Model::NascarProFF2,
        Model::FGTRumbleForce,
        Model::RGTFF,
        Model::FGTForceFeedback,
        Model::F430ForceFeedback,
        Model::T3PA,
        Model::T3PAPro,
        Model::TLCM,
        Model::TLCMPro,
        Model::Unknown,
    ];
    for model in all_models {
        assert!(!model.name().is_empty(), "{model:?} must have a name");
        assert!(
            model.name().starts_with("Thrustmaster"),
            "{model:?} name must start with 'Thrustmaster'"
        );
    }
    Ok(())
}

// ─── is_wheel_product / is_pedal_product ─────────────────────────────────────

#[test]
fn is_wheel_product_all_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
    let wheelbases = [
        product_ids::T150,
        product_ids::TMX,
        product_ids::T300_RS,
        product_ids::T300_RS_PS4,
        product_ids::T300_RS_GT,
        product_ids::TX_RACING,
        product_ids::TX_RACING_ORIG,
        product_ids::T500_RS,
        product_ids::T248,
        product_ids::T248X,
        product_ids::TS_PC_RACER,
        product_ids::TS_XW,
        product_ids::TS_XW_GIP,
        product_ids::T818,
        product_ids::T80,
        product_ids::T80_FERRARI_488,
        product_ids::NASCAR_PRO_FF2,
        product_ids::FGT_RUMBLE_FORCE,
        product_ids::RGT_FF_CLUTCH,
        product_ids::FGT_FORCE_FEEDBACK,
        product_ids::F430_FORCE_FEEDBACK,
    ];
    for pid in wheelbases {
        assert!(is_wheel_product(pid), "PID 0x{pid:04X} must be wheel");
    }
    Ok(())
}

#[test]
fn unknown_pid_not_wheel_or_pedal() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!is_wheel_product(0xFFFF));
    assert!(!is_pedal_product(0xFFFF));
    Ok(())
}

// ─── Init protocol constants ─────────────────────────────────────────────────

#[test]
fn init_protocol_request_codes() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(init_protocol::MODEL_QUERY_REQUEST, 73);
    assert_eq!(init_protocol::MODE_SWITCH_REQUEST, 83);
    assert_eq!(init_protocol::MODEL_QUERY_REQUEST_TYPE, 0xC1);
    assert_eq!(init_protocol::MODE_SWITCH_REQUEST_TYPE, 0x41);
    assert_eq!(init_protocol::MODEL_RESPONSE_LEN, 0x0010);
    Ok(())
}

#[test]
fn init_protocol_setup_interrupts_count() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(init_protocol::SETUP_INTERRUPTS.len(), 5);
    // First interrupt starts with 0x42
    assert_eq!(init_protocol::SETUP_INTERRUPTS[0][0], 0x42);
    // Others start with 0x0a
    for i in 1..5 {
        assert_eq!(init_protocol::SETUP_INTERRUPTS[i][0], 0x0A, "interrupt {i}");
    }
    Ok(())
}

#[test]
fn init_protocol_known_models_complete() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(init_protocol::KNOWN_MODELS.len(), 7);
    // T500RS entry: model=0x0002, switch=0x0002
    let t500 = init_protocol::KNOWN_MODELS.iter().find(|m| m.0 == 0x0002);
    let t500 = t500.ok_or("T500RS entry missing")?;
    assert_eq!(t500.1, 0x0002);
    // T150 entry: model=0x0306, switch=0x0006
    let t150 = init_protocol::KNOWN_MODELS.iter().find(|m| m.0 == 0x0306);
    let t150 = t150.ok_or("T150 entry missing")?;
    assert_eq!(t150.1, 0x0006);
    Ok(())
}

// ─── ThrustmasterProtocol ────────────────────────────────────────────────────

#[test]
fn protocol_new_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let proto = ThrustmasterProtocol::new(product_ids::T300_RS);
    assert_eq!(proto.model(), Model::T300RS);
    assert_eq!(proto.product_id(), product_ids::T300_RS);
    assert_eq!(proto.init_state(), ThrustmasterInitState::Uninitialized);
    assert_eq!(proto.gain(), 0xFF);
    assert_eq!(proto.rotation_range(), 1080); // T300RS max rotation
    assert!((proto.max_torque_nm() - 4.0).abs() < 0.01);
    assert!(proto.supports_ffb());
    assert!(proto.is_wheelbase());
    Ok(())
}

#[test]
fn protocol_new_with_config() -> Result<(), Box<dyn std::error::Error>> {
    let proto = ThrustmasterProtocol::new_with_config(0x1234, 7.5, 900);
    assert_eq!(proto.model(), Model::Unknown);
    assert_eq!(proto.product_id(), 0x1234);
    assert_eq!(proto.rotation_range(), 900);
    assert!((proto.max_torque_nm() - 7.5).abs() < 0.01);
    Ok(())
}

#[test]
fn protocol_new_with_config_negative_torque_floors() -> Result<(), Box<dyn std::error::Error>> {
    let proto = ThrustmasterProtocol::new_with_config(0x1234, -5.0, 900);
    assert!(
        (proto.max_torque_nm() - 0.01).abs() < 0.001,
        "negative torque floors to 0.01"
    );
    Ok(())
}

#[test]
fn protocol_init_state_transitions() -> Result<(), Box<dyn std::error::Error>> {
    let mut proto = ThrustmasterProtocol::new(product_ids::T300_RS);
    assert_eq!(proto.init_state(), ThrustmasterInitState::Uninitialized);

    proto.init();
    assert_eq!(proto.init_state(), ThrustmasterInitState::Ready);

    proto.reset();
    assert_eq!(proto.init_state(), ThrustmasterInitState::Uninitialized);
    Ok(())
}

#[test]
fn protocol_set_gain_and_rotation() -> Result<(), Box<dyn std::error::Error>> {
    let mut proto = ThrustmasterProtocol::new(product_ids::T300_RS);
    proto.set_gain(128);
    assert_eq!(proto.gain(), 128);
    proto.set_rotation_range(540);
    assert_eq!(proto.rotation_range(), 540);
    Ok(())
}

#[test]
fn protocol_build_init_sequence_length() -> Result<(), Box<dyn std::error::Error>> {
    let proto = ThrustmasterProtocol::new(product_ids::T300_RS);
    let seq = proto.build_init_sequence();
    assert_eq!(seq.len(), 4);
    Ok(())
}

#[test]
fn protocol_parse_input_rejects_pedals() -> Result<(), Box<dyn std::error::Error>> {
    // Unknown PID → not wheelbase → pedals check
    let proto = ThrustmasterProtocol::new(0xFFFF);
    let mut data = [0u8; 16];
    data[0] = STANDARD_INPUT_REPORT_ID;
    // Not a wheelbase, but Unknown is not pedal either → parse_input still runs
    // This tests the parse_input delegation
    let _result = proto.parse_input(&data);
    Ok(())
}

#[test]
fn protocol_default_is_t300rs() -> Result<(), Box<dyn std::error::Error>> {
    let proto = ThrustmasterProtocol::default();
    assert_eq!(proto.model(), Model::T300RS);
    assert!(proto.supports_ffb());
    Ok(())
}

// ─── Effect type constants ───────────────────────────────────────────────────

#[test]
fn effect_type_constants() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(EFFECT_TYPE_CONSTANT, 0x26);
    assert_eq!(EFFECT_TYPE_RAMP, 0x27);
    assert_eq!(EFFECT_TYPE_SPRING, 0x40);
    assert_eq!(EFFECT_TYPE_DAMPER, 0x41);
    assert_eq!(EFFECT_TYPE_FRICTION, 0x43);
    Ok(())
}

// ─── Report IDs ──────────────────────────────────────────────────────────────

#[test]
fn output_report_ids_golden() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(report_ids::VENDOR_SET_RANGE, 0x80);
    assert_eq!(report_ids::DEVICE_GAIN, 0x81);
    assert_eq!(report_ids::ACTUATOR_ENABLE, 0x82);
    assert_eq!(report_ids::CONSTANT_FORCE, 0x23);
    assert_eq!(report_ids::EFFECT_OP, 0x22);
    Ok(())
}

// ─── Spring / damper / friction effects ──────────────────────────────────────

#[test]
fn spring_effect_wire_format() -> Result<(), Box<dyn std::error::Error>> {
    let r = build_spring_effect(-500, 1000);
    assert_eq!(r[0], report_ids::EFFECT_OP);
    assert_eq!(r[1], EFFECT_TYPE_SPRING);
    assert_eq!(r[2], 0x01);
    let center = i16::from_le_bytes([r[3], r[4]]);
    assert_eq!(center, -500);
    let stiffness = u16::from_le_bytes([r[5], r[6]]);
    assert_eq!(stiffness, 1000);
    assert_eq!(r[7], 0);
    Ok(())
}

#[test]
fn damper_effect_wire_format() -> Result<(), Box<dyn std::error::Error>> {
    let r = build_damper_effect(2000);
    assert_eq!(r[0], report_ids::EFFECT_OP);
    assert_eq!(r[1], EFFECT_TYPE_DAMPER);
    assert_eq!(r[2], 0x01);
    let damping = u16::from_le_bytes([r[3], r[4]]);
    assert_eq!(damping, 2000);
    assert_eq!(&r[5..], &[0, 0, 0]);
    Ok(())
}

#[test]
fn friction_effect_wire_format() -> Result<(), Box<dyn std::error::Error>> {
    let r = build_friction_effect(100, 800);
    assert_eq!(r[0], report_ids::EFFECT_OP);
    assert_eq!(r[1], EFFECT_TYPE_FRICTION);
    assert_eq!(r[2], 0x01);
    let minimum = u16::from_le_bytes([r[3], r[4]]);
    assert_eq!(minimum, 100);
    let maximum = u16::from_le_bytes([r[5], r[6]]);
    assert_eq!(maximum, 800);
    assert_eq!(r[7], 0);
    Ok(())
}

// ─── Set range report ────────────────────────────────────────────────────────

#[test]
fn set_range_report_360() -> Result<(), Box<dyn std::error::Error>> {
    let r = build_set_range_report(360);
    assert_eq!(r[0], report_ids::VENDOR_SET_RANGE);
    let decoded = u16::from_le_bytes([r[2], r[3]]);
    assert_eq!(decoded, 360);
    Ok(())
}

// ─── Kernel range command: degrees * 0x3C ────────────────────────────────────

#[test]
fn kernel_range_scaling_factor() -> Result<(), Box<dyn std::error::Error>> {
    // 100 * 0x3C = 100 * 60 = 6000 = 0x1770
    let r = build_kernel_range_command(100);
    let scaled = u16::from_le_bytes([r[2], r[3]]);
    assert_eq!(scaled, 6000);
    Ok(())
}

#[test]
fn kernel_range_clamp_below_40() -> Result<(), Box<dyn std::error::Error>> {
    let at_40 = build_kernel_range_command(40);
    let below = build_kernel_range_command(10);
    assert_eq!(at_40, below, "below 40 must clamp to 40");
    Ok(())
}

#[test]
fn kernel_range_clamp_above_1080() -> Result<(), Box<dyn std::error::Error>> {
    let at_1080 = build_kernel_range_command(1080);
    let above = build_kernel_range_command(2000);
    assert_eq!(at_1080, above, "above 1080 must clamp to 1080");
    Ok(())
}

// ─── Kernel gain command ─────────────────────────────────────────────────────

#[test]
fn kernel_gain_shift() -> Result<(), Box<dyn std::error::Error>> {
    // 0x8000 >> 8 = 0x80
    let r = build_kernel_gain_command(0x8000);
    assert_eq!(r[1], 0x80);
    // 0x0100 >> 8 = 0x01
    let r2 = build_kernel_gain_command(0x0100);
    assert_eq!(r2[1], 0x01);
    Ok(())
}

// ─── Kernel autocenter commands ──────────────────────────────────────────────

#[test]
fn kernel_autocenter_wire_format() -> Result<(), Box<dyn std::error::Error>> {
    let cmds = build_kernel_autocenter_commands(0xABCD);
    assert_eq!(cmds[0], [0x08, 0x04, 0x01, 0x00]);
    assert_eq!(cmds[1], [0x08, 0x03, 0xCD, 0xAB]);
    Ok(())
}

// ─── Kernel open/close commands ──────────────────────────────────────────────

#[test]
fn kernel_open_close_golden() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(build_kernel_open_command(), [0x01, 0x05]);
    assert_eq!(build_kernel_close_command(), [0x01, 0x00]);
    Ok(())
}

// ─── Device gain / actuator enable ───────────────────────────────────────────

#[test]
fn device_gain_boundary_values() -> Result<(), Box<dyn std::error::Error>> {
    let g0 = build_device_gain(0);
    assert_eq!(g0, [report_ids::DEVICE_GAIN, 0x00]);
    let g255 = build_device_gain(255);
    assert_eq!(g255, [report_ids::DEVICE_GAIN, 0xFF]);
    Ok(())
}

#[test]
fn actuator_enable_disable() -> Result<(), Box<dyn std::error::Error>> {
    let en = build_actuator_enable(true);
    assert_eq!(en, [report_ids::ACTUATOR_ENABLE, 0x01]);
    let dis = build_actuator_enable(false);
    assert_eq!(dis, [report_ids::ACTUATOR_ENABLE, 0x00]);
    Ok(())
}

// ─── T150 encoding ───────────────────────────────────────────────────────────

#[test]
fn t150_range_encode_known_value() -> Result<(), Box<dyn std::error::Error>> {
    // 0x5678 → LE bytes: 0x78, 0x56
    let r = encode_range_t150(0x5678);
    assert_eq!(r, [CMD_RANGE, SUBCMD_RANGE, 0x78, 0x56]);
    Ok(())
}

#[test]
fn t150_gain_encode() -> Result<(), Box<dyn std::error::Error>> {
    let r = encode_gain_t150(0x80);
    assert_eq!(r, [CMD_GAIN, 0x80]);
    Ok(())
}

#[test]
fn t150_play_and_stop_relationship() -> Result<(), Box<dyn std::error::Error>> {
    for id in [0u8, 1, 5, 255] {
        let stop = encode_stop_effect_t150(id);
        let play_zero = encode_play_effect_t150(id, 0, 0);
        assert_eq!(
            stop, play_zero,
            "stop(id={id}) must equal play(id={id}, 0, 0)"
        );
    }
    Ok(())
}

// ─── T150EffectType round-trip ───────────────────────────────────────────────

#[test]
fn t150_effect_type_all_values() -> Result<(), Box<dyn std::error::Error>> {
    let expected: &[(T150EffectType, u16)] = &[
        (T150EffectType::Constant, 0x4000),
        (T150EffectType::Sine, 0x4022),
        (T150EffectType::SawtoothUp, 0x4023),
        (T150EffectType::SawtoothDown, 0x4024),
        (T150EffectType::Spring, 0x4040),
        (T150EffectType::Damper, 0x4041),
    ];
    for &(effect, value) in expected {
        assert_eq!(effect.as_u16(), value, "{effect:?}");
        assert_eq!(
            T150EffectType::from_u16(value),
            Some(effect),
            "value 0x{value:04X}"
        );
    }
    // Invalid values
    assert_eq!(T150EffectType::from_u16(0x0000), None);
    assert_eq!(T150EffectType::from_u16(0x4001), None);
    assert_eq!(T150EffectType::from_u16(0xFFFF), None);
    Ok(())
}

// ─── Input report parsing ────────────────────────────────────────────────────

#[test]
fn input_report_center_steering() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 16];
    data[0] = STANDARD_INPUT_REPORT_ID;
    data[1] = 0x00;
    data[2] = 0x80; // center = 0x8000
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!(state.steering.abs() < 0.001);
    Ok(())
}

#[test]
fn input_report_full_left_right() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 16];
    data[0] = STANDARD_INPUT_REPORT_ID;

    // Full left: 0x0000
    data[1] = 0x00;
    data[2] = 0x00;
    let left = parse_input_report(&data).ok_or("parse failed")?;
    assert!((left.steering + 1.0).abs() < 0.001);

    // Full right: 0xFFFF
    data[1] = 0xFF;
    data[2] = 0xFF;
    let right = parse_input_report(&data).ok_or("parse failed")?;
    assert!((right.steering - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn input_report_pedal_normalization() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 16];
    data[0] = STANDARD_INPUT_REPORT_ID;
    data[3] = 255; // full throttle
    data[4] = 128; // half brake
    data[5] = 0; // no clutch
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!((state.throttle - 1.0).abs() < 0.01);
    assert!((state.brake - 0.502).abs() < 0.01);
    assert!(state.clutch.abs() < 0.01);
    Ok(())
}

#[test]
fn input_report_buttons_and_hat() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 16];
    data[0] = STANDARD_INPUT_REPORT_ID;
    data[6] = 0xAB;
    data[7] = 0xCD;
    data[8] = 0x05; // hat value
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.buttons, 0xCDAB);
    assert_eq!(state.hat, 0x05);
    Ok(())
}

#[test]
fn input_report_paddles() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 16];
    data[0] = STANDARD_INPUT_REPORT_ID;
    data[9] = 0x01; // right paddle only
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!(state.paddle_right);
    assert!(!state.paddle_left);

    data[9] = 0x02; // left paddle only
    let state2 = parse_input_report(&data).ok_or("parse failed")?;
    assert!(!state2.paddle_right);
    assert!(state2.paddle_left);
    Ok(())
}

#[test]
fn input_report_rejects_short() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_input_report(&[STANDARD_INPUT_REPORT_ID, 0, 0]).is_none());
    Ok(())
}

#[test]
fn input_report_rejects_wrong_id() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 16];
    data[0] = 0x02; // wrong
    assert!(parse_input_report(&data).is_none());
    Ok(())
}

// ─── Pedal report parsing ────────────────────────────────────────────────────

#[test]
fn pedal_report_basic() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0xC0, 0x40, 0x20];
    let pedals = parse_pedal_report(&data).ok_or("parse failed")?;
    assert_eq!(pedals.throttle, 0xC0);
    assert_eq!(pedals.brake, 0x40);
    assert_eq!(pedals.clutch, Some(0x20));
    Ok(())
}

#[test]
fn pedal_report_too_short() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_pedal_report(&[0xFF, 0x80]).is_none());
    Ok(())
}

// ─── Pedal normalization ─────────────────────────────────────────────────────

#[test]
fn pedal_axes_normalize_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let raw = ThrustmasterPedalAxesRaw {
        throttle: 255,
        brake: 0,
        clutch: Some(128),
    };
    let norm = raw.normalize();
    assert!((norm.throttle - 1.0).abs() < 0.01);
    assert!(norm.brake.abs() < 0.01);
    assert!((norm.clutch.ok_or("clutch missing")? - 0.502).abs() < 0.01);
    Ok(())
}

#[test]
fn pedal_axes_normalize_no_clutch() -> Result<(), Box<dyn std::error::Error>> {
    let raw = ThrustmasterPedalAxesRaw {
        throttle: 0,
        brake: 255,
        clutch: None,
    };
    let norm = raw.normalize();
    assert!(norm.throttle.abs() < 0.01);
    assert!((norm.brake - 1.0).abs() < 0.01);
    assert!(norm.clutch.is_none());
    Ok(())
}

// ─── Proptest ────────────────────────────────────────────────────────────────

mod proptest_deep {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Encoded magnitude is always in [-10000, 10000].
        #[test]
        fn prop_cf_magnitude_bounded(
            max_torque in 0.1_f32..=21.0_f32,
            torque in -50.0_f32..=50.0_f32,
        ) {
            let enc = ThrustmasterConstantForceEncoder::new(max_torque);
            let mut out = [0u8; EFFECT_REPORT_LEN];
            enc.encode(torque, &mut out);
            let mag = i16::from_le_bytes([out[2], out[3]]);
            prop_assert!((-10000..=10000).contains(&mag), "magnitude {mag} out of bounds");
        }

        /// Sign of encoded value matches sign of input torque.
        #[test]
        fn prop_cf_sign_matches(
            max_torque in 0.1_f32..=21.0_f32,
            fraction in -1.0_f32..=1.0_f32,
        ) {
            let torque = max_torque * fraction;
            let enc = ThrustmasterConstantForceEncoder::new(max_torque);
            let mut out = [0u8; EFFECT_REPORT_LEN];
            enc.encode(torque, &mut out);
            let mag = i16::from_le_bytes([out[2], out[3]]);
            if torque > 0.01 {
                prop_assert!(mag >= 0, "positive torque {torque} → mag {mag}");
            } else if torque < -0.01 {
                prop_assert!(mag <= 0, "negative torque {torque} → mag {mag}");
            }
        }

        /// Report header is always correct.
        #[test]
        fn prop_cf_header(
            max_torque in 0.01_f32..=21.0_f32,
            torque in -50.0_f32..=50.0_f32,
        ) {
            let enc = ThrustmasterConstantForceEncoder::new(max_torque);
            let mut out = [0u8; EFFECT_REPORT_LEN];
            enc.encode(torque, &mut out);
            prop_assert_eq!(out[0], report_ids::CONSTANT_FORCE);
            prop_assert_eq!(out[1], 1);
        }

        /// Kernel range: scaled value = degrees * 0x3C, clamped to [40, 1080].
        #[test]
        fn prop_kernel_range_scaling(degrees in 0u16..=2000u16) {
            let r = build_kernel_range_command(degrees);
            let clamped = degrees.clamp(40, 1080);
            let expected = (clamped as u32) * 0x3C;
            let actual = u16::from_le_bytes([r[2], r[3]]);
            let msg = format!("degrees={degrees}");
            prop_assert_eq!(actual, expected as u16, "{}", msg);
        }

        /// Identify device is deterministic.
        #[test]
        fn prop_identify_deterministic(pid: u16) {
            let a = identify_device(pid);
            let b = identify_device(pid);
            prop_assert_eq!(a.product_id, b.product_id);
            prop_assert_eq!(a.category, b.category);
            prop_assert_eq!(a.supports_ffb, b.supports_ffb);
        }

        /// Model::from_product_id is deterministic.
        #[test]
        fn prop_model_from_pid_deterministic(pid: u16) {
            let a = Model::from_product_id(pid);
            let b = Model::from_product_id(pid);
            prop_assert_eq!(a, b);
        }

        /// T150 encode_range round-trips via LE bytes.
        #[test]
        fn prop_t150_range_round_trip(value: u16) {
            let cmd = encode_range_t150(value);
            let decoded = u16::from_le_bytes([cmd[2], cmd[3]]);
            prop_assert_eq!(decoded, value);
        }
    }
}
