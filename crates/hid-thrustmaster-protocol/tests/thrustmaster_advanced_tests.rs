//! Advanced tests for Thrustmaster protocol crate.
//!
//! Covers PID recognition, FFB effect encoding, LED/display commands,
//! wheel base pairing protocol, bootloader mode detection, and proptest
//! roundtrip verification.

use racing_wheel_hid_thrustmaster_protocol::{
    Model, ProtocolFamily, THRUSTMASTER_VENDOR_ID,
    ThrustmasterConstantForceEncoder, ThrustmasterProtocol, ThrustmasterInitState,
    build_damper_effect, build_device_gain, build_friction_effect, build_set_range_report,
    build_spring_effect, build_actuator_enable,
    build_kernel_range_command, build_kernel_gain_command,
    build_kernel_open_command, build_kernel_close_command,
    build_kernel_autocenter_commands,
    identify_device, is_wheel_product, is_pedal_product,
    parse_input_report,
    T150EffectType, encode_range_t150, encode_gain_t150,
    encode_play_effect_t150, encode_stop_effect_t150,
    product_ids, init_protocol,
    EFFECT_REPORT_LEN,
    ThrustmasterDeviceCategory,
};

// ─── PID recognition ─────────────────────────────────────────────────────

#[test]
fn test_t300rs_recognition() {
    assert_eq!(Model::from_product_id(product_ids::T300_RS), Model::T300RS);
    assert_eq!(Model::T300RS.name(), "Thrustmaster T300 RS");
    assert!(Model::T300RS.supports_ffb());
    assert_eq!(Model::T300RS.protocol_family(), ProtocolFamily::T300);
    assert_eq!(Model::T300RS.max_rotation_deg(), 1080);
}

#[test]
fn test_t300rs_ps4_recognition() {
    assert_eq!(Model::from_product_id(product_ids::T300_RS_PS4), Model::T300RSPS4);
    assert_eq!(Model::T300RSPS4.protocol_family(), ProtocolFamily::T300);
}

#[test]
fn test_t500rs_recognition() {
    assert_eq!(Model::from_product_id(product_ids::T500_RS), Model::T500RS);
    assert!((Model::T500RS.max_torque_nm() - 5.0).abs() < f32::EPSILON);
    assert_eq!(Model::T500RS.protocol_family(), ProtocolFamily::T500);
    assert_eq!(Model::T500RS.max_rotation_deg(), 1080);
}

#[test]
fn test_tx_racing_recognition() {
    assert_eq!(Model::from_product_id(product_ids::TX_RACING), Model::TXRacing);
    assert_eq!(Model::from_product_id(product_ids::TX_RACING_ORIG), Model::TXRacing);
    assert_eq!(Model::TXRacing.protocol_family(), ProtocolFamily::T300);
    assert!(Model::TXRacing.supports_ffb());
}

#[test]
fn test_tgt_ii_recognition() {
    assert_eq!(Model::from_product_id(product_ids::T_GT_II_GT), Model::TGTII);
    assert_eq!(Model::TGTII.name(), "Thrustmaster T-GT II");
    assert_eq!(Model::TGTII.protocol_family(), ProtocolFamily::T300);
    assert!((Model::TGTII.max_torque_nm() - 6.0).abs() < f32::EPSILON);
}

#[test]
fn test_t248_recognition() {
    assert_eq!(Model::from_product_id(product_ids::T248), Model::T248);
    assert_eq!(Model::from_product_id(product_ids::T248X), Model::T248X);
    assert_eq!(Model::T248.protocol_family(), ProtocolFamily::T300);
    assert_eq!(Model::T248.max_rotation_deg(), 900);
}

#[test]
fn test_t818_recognition() {
    assert_eq!(Model::from_product_id(product_ids::T818), Model::T818);
    assert!((Model::T818.max_torque_nm() - 10.0).abs() < f32::EPSILON);
    assert_eq!(Model::T818.name(), "Thrustmaster T818");
}

#[test]
fn test_tspc_racer_recognition() {
    assert_eq!(Model::from_product_id(product_ids::TS_PC_RACER), Model::TSPCRacer);
    assert_eq!(Model::TSPCRacer.protocol_family(), ProtocolFamily::T300);
    assert!((Model::TSPCRacer.max_torque_nm() - 6.0).abs() < f32::EPSILON);
}

#[test]
fn test_tsxw_recognition() {
    assert_eq!(Model::from_product_id(product_ids::TS_XW), Model::TSXW);
    assert_eq!(Model::from_product_id(product_ids::TS_XW_GIP), Model::TSXW);
    assert_eq!(Model::TSXW.protocol_family(), ProtocolFamily::T300);
}

#[test]
fn test_t150_tmx_recognition() {
    assert_eq!(Model::from_product_id(product_ids::T150), Model::T150);
    assert_eq!(Model::from_product_id(product_ids::TMX), Model::TMX);
    assert_eq!(Model::T150.protocol_family(), ProtocolFamily::T150);
    assert_eq!(Model::TMX.protocol_family(), ProtocolFamily::T150);
    assert!((Model::T150.max_torque_nm() - 2.5).abs() < f32::EPSILON);
}

#[test]
fn test_t80_no_ffb() {
    assert_eq!(Model::from_product_id(product_ids::T80), Model::T80);
    assert_eq!(Model::from_product_id(product_ids::T80_FERRARI_488), Model::T80);
    assert!(!Model::T80.supports_ffb());
    assert!((Model::T80.max_torque_nm() - 0.0).abs() < f32::EPSILON);
}

#[test]
fn test_legacy_wheel_recognition() {
    assert_eq!(Model::from_product_id(product_ids::NASCAR_PRO_FF2), Model::NascarProFF2);
    assert_eq!(Model::from_product_id(product_ids::FGT_RUMBLE_FORCE), Model::FGTRumbleForce);
    assert_eq!(Model::from_product_id(product_ids::RGT_FF_CLUTCH), Model::RGTFF);
    assert_eq!(Model::from_product_id(product_ids::FGT_FORCE_FEEDBACK), Model::FGTForceFeedback);
    assert_eq!(Model::from_product_id(product_ids::F430_FORCE_FEEDBACK), Model::F430ForceFeedback);
    // All legacy wheels share same torque
    assert!((Model::NascarProFF2.max_torque_nm() - 1.5).abs() < f32::EPSILON);
}

#[test]
fn test_pedal_pids_are_unknown_category() {
    // Pedal PIDs are not explicitly matched in identify_device, so they
    // fall through to Unknown (no FFB). This means is_pedal_product is false.
    let tlcm = identify_device(product_ids::T_LCM);
    assert_eq!(tlcm.category, ThrustmasterDeviceCategory::Unknown);
    assert!(!tlcm.supports_ffb);
    assert!(!is_pedal_product(product_ids::T_LCM));
    assert!(!is_pedal_product(product_ids::TPR_PEDALS));
    assert!(!is_wheel_product(product_ids::T_LCM));
}

#[test]
fn test_unknown_pid_defaults() {
    assert_eq!(Model::from_product_id(0xFFFF), Model::Unknown);
    assert!(!Model::Unknown.supports_ffb());
    assert_eq!(Model::Unknown.protocol_family(), ProtocolFamily::Unknown);
}

#[test]
fn test_vendor_id_constant() {
    assert_eq!(THRUSTMASTER_VENDOR_ID, 0x044F);
}

// ─── FFB effect encoding ─────────────────────────────────────────────────

#[test]
fn test_spring_effect_encoding_center_and_stiffness() {
    let report = build_spring_effect(100, 750);
    assert_eq!(report[0], 0x22); // EFFECT_OP
    assert_eq!(report[1], 0x40); // EFFECT_TYPE_SPRING
    let center = i16::from_le_bytes([report[3], report[4]]);
    let stiffness = u16::from_le_bytes([report[5], report[6]]);
    assert_eq!(center, 100);
    assert_eq!(stiffness, 750);
}

#[test]
fn test_damper_effect_encoding() {
    let report = build_damper_effect(500);
    assert_eq!(report[0], 0x22);
    assert_eq!(report[1], 0x41); // EFFECT_TYPE_DAMPER
    let damping = u16::from_le_bytes([report[3], report[4]]);
    assert_eq!(damping, 500);
}

#[test]
fn test_friction_effect_encoding_min_max() {
    let report = build_friction_effect(200, 900);
    assert_eq!(report[0], 0x22);
    assert_eq!(report[1], 0x43); // EFFECT_TYPE_FRICTION
    let min_val = u16::from_le_bytes([report[3], report[4]]);
    let max_val = u16::from_le_bytes([report[5], report[6]]);
    assert_eq!(min_val, 200);
    assert_eq!(max_val, 900);
}

#[test]
fn test_constant_force_encoder_half_torque() {
    let enc = ThrustmasterConstantForceEncoder::new(4.0);
    let mut out = [0u8; EFFECT_REPORT_LEN];
    enc.encode(2.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 5000);
}

#[test]
fn test_constant_force_encoder_clamps_beyond_max() {
    let enc = ThrustmasterConstantForceEncoder::new(4.0);
    let mut out = [0u8; EFFECT_REPORT_LEN];
    enc.encode(50.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 10000);
    enc.encode(-50.0, &mut out);
    let neg_mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(neg_mag, -10000);
}

// ─── LED / display commands (gain/range report encoding) ─────────────────

#[test]
fn test_device_gain_report_format() {
    let report = build_device_gain(0x80);
    assert_eq!(report[0], 0x81);
    assert_eq!(report[1], 0x80);
}

#[test]
fn test_set_range_report_encodes_degrees_le() {
    let report = build_set_range_report(900);
    assert_eq!(report[0], 0x80); // VENDOR_SET_RANGE
    let decoded = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(decoded, 900);
}

#[test]
fn test_actuator_enable_disable() {
    let enable = build_actuator_enable(true);
    assert_eq!(enable[0], 0x82);
    assert_eq!(enable[1], 0x01);
    let disable = build_actuator_enable(false);
    assert_eq!(disable[1], 0x00);
}

// ─── Wheel base pairing / init protocol ──────────────────────────────────

#[test]
fn test_init_switch_values_per_family() {
    // T150 family uses 0x0006
    assert_eq!(Model::T150.init_switch_value(), Some(0x0006));
    assert_eq!(Model::TMX.init_switch_value(), Some(0x0006));
    // T300 family uses 0x0005
    assert_eq!(Model::T300RS.init_switch_value(), Some(0x0005));
    assert_eq!(Model::T248.init_switch_value(), Some(0x0005));
    assert_eq!(Model::TGTII.init_switch_value(), Some(0x0005));
    // T500 uses 0x0002
    assert_eq!(Model::T500RS.init_switch_value(), Some(0x0002));
    // Unknown has no init value
    assert_eq!(Model::Unknown.init_switch_value(), None);
}

#[test]
fn test_init_protocol_constants() {
    assert_eq!(init_protocol::MODEL_QUERY_REQUEST, 73);
    assert_eq!(init_protocol::MODE_SWITCH_REQUEST, 83);
    assert_eq!(init_protocol::MODEL_QUERY_REQUEST_TYPE, 0xC1);
    assert_eq!(init_protocol::MODE_SWITCH_REQUEST_TYPE, 0x41);
    assert_eq!(init_protocol::MODEL_RESPONSE_LEN, 0x0010);
}

#[test]
fn test_setup_interrupts_count_and_sizes() {
    assert_eq!(init_protocol::SETUP_INTERRUPTS.len(), 5);
    // First interrupt is 9 bytes, rest are 8
    assert_eq!(init_protocol::SETUP_INTERRUPTS[0].len(), 9);
    for interrupt in &init_protocol::SETUP_INTERRUPTS[1..] {
        assert_eq!(interrupt.len(), 8);
    }
}

#[test]
fn test_known_models_mapping() {
    // Verify each known model has a valid switch value
    for &(model_code, switch_value, name) in init_protocol::KNOWN_MODELS {
        assert!(model_code > 0, "model code must be nonzero for {name}");
        assert!(switch_value > 0, "switch value must be nonzero for {name}");
        assert!(!name.is_empty(), "name must not be empty");
    }
    // At least T150, T300, T500 entries
    assert!(init_protocol::KNOWN_MODELS.len() >= 3);
}

// ─── Bootloader / generic mode detection ─────────────────────────────────

#[test]
fn test_generic_ffb_wheel_pid_not_a_model() {
    // The generic pre-init PID should resolve to Unknown
    let model = Model::from_product_id(product_ids::FFB_WHEEL_GENERIC);
    assert_eq!(model, Model::Unknown);
    assert!(!model.supports_ffb());
}

// ─── Protocol handler ────────────────────────────────────────────────────

#[test]
fn test_protocol_init_and_reset() {
    let mut proto = ThrustmasterProtocol::new(product_ids::T300_RS);
    assert_eq!(proto.init_state(), ThrustmasterInitState::Uninitialized);
    proto.init();
    assert_eq!(proto.init_state(), ThrustmasterInitState::Ready);
    proto.reset();
    assert_eq!(proto.init_state(), ThrustmasterInitState::Uninitialized);
}

#[test]
fn test_protocol_build_init_sequence_length() {
    let proto = ThrustmasterProtocol::new(product_ids::T300_RS);
    let seq = proto.build_init_sequence();
    assert_eq!(seq.len(), 4);
}

// ─── T150 protocol encoding ─────────────────────────────────────────────

#[test]
fn test_t150_effect_type_roundtrip_all_variants() {
    let all_types = [
        T150EffectType::Constant,
        T150EffectType::Sine,
        T150EffectType::SawtoothUp,
        T150EffectType::SawtoothDown,
        T150EffectType::Spring,
        T150EffectType::Damper,
    ];
    for ty in all_types {
        let wire = ty.as_u16();
        let decoded = T150EffectType::from_u16(wire);
        assert_eq!(decoded, Some(ty));
    }
}

#[test]
fn test_t150_range_encode_max() {
    let cmd = encode_range_t150(0xFFFF);
    assert_eq!(cmd[0], 0x40);
    assert_eq!(cmd[1], 0x11);
    let decoded = u16::from_le_bytes([cmd[2], cmd[3]]);
    assert_eq!(decoded, 0xFFFF);
}

#[test]
fn test_t150_gain_encode_half() {
    let cmd = encode_gain_t150(0x80);
    assert_eq!(cmd, [0x43, 0x80]);
}

#[test]
fn test_t150_play_and_stop_equivalence() {
    let stop = encode_stop_effect_t150(5);
    let play_zero = encode_play_effect_t150(5, 0x00, 0x00);
    assert_eq!(stop, play_zero);
}

// ─── Kernel-level wire commands ──────────────────────────────────────────

#[test]
fn test_kernel_range_scaling() {
    // 900 * 0x3C = 54000 = 0xD2F0
    let cmd = build_kernel_range_command(900);
    assert_eq!(cmd, [0x08, 0x11, 0xF0, 0xD2]);
}

#[test]
fn test_kernel_range_clamping() {
    // Below min (40) clamps
    let cmd_lo = build_kernel_range_command(0);
    let cmd_40 = build_kernel_range_command(40);
    assert_eq!(cmd_lo, cmd_40);
    // Above max (1080) clamps
    let cmd_hi = build_kernel_range_command(9999);
    let cmd_1080 = build_kernel_range_command(1080);
    assert_eq!(cmd_hi, cmd_1080);
}

#[test]
fn test_kernel_open_close_commands() {
    assert_eq!(build_kernel_open_command(), [0x01, 0x05]);
    assert_eq!(build_kernel_close_command(), [0x01, 0x00]);
}

#[test]
fn test_kernel_autocenter_two_step() {
    let cmds = build_kernel_autocenter_commands(0xABCD);
    assert_eq!(cmds[0], [0x08, 0x04, 0x01, 0x00]);
    assert_eq!(cmds[1], [0x08, 0x03, 0xCD, 0xAB]);
}

// ─── Input parsing ───────────────────────────────────────────────────────

#[test]
fn test_parse_input_report_too_short() {
    let short_data = [0u8; 4];
    assert!(parse_input_report(&short_data).is_none());
}

// ─── Proptest: effect parameter roundtrip ────────────────────────────────

mod proptest_advanced {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(300))]

        #[test]
        fn prop_spring_center_roundtrip(center in i16::MIN..=i16::MAX, stiffness in 0u16..=10000u16) {
            let report = build_spring_effect(center, stiffness);
            let decoded_center = i16::from_le_bytes([report[3], report[4]]);
            let decoded_stiffness = u16::from_le_bytes([report[5], report[6]]);
            prop_assert_eq!(decoded_center, center);
            prop_assert_eq!(decoded_stiffness, stiffness);
        }

        #[test]
        fn prop_damper_roundtrip(damping in 0u16..=10000u16) {
            let report = build_damper_effect(damping);
            let decoded = u16::from_le_bytes([report[3], report[4]]);
            prop_assert_eq!(decoded, damping);
        }

        #[test]
        fn prop_friction_roundtrip(min_val in 0u16..=10000u16, max_val in 0u16..=10000u16) {
            let report = build_friction_effect(min_val, max_val);
            let decoded_min = u16::from_le_bytes([report[3], report[4]]);
            let decoded_max = u16::from_le_bytes([report[5], report[6]]);
            prop_assert_eq!(decoded_min, min_val);
            prop_assert_eq!(decoded_max, max_val);
        }

        #[test]
        fn prop_set_range_roundtrip(degrees in 0u16..=10000u16) {
            let report = build_set_range_report(degrees);
            let decoded = u16::from_le_bytes([report[2], report[3]]);
            prop_assert_eq!(decoded, degrees);
        }

        #[test]
        fn prop_t150_range_roundtrip(value: u16) {
            let cmd = encode_range_t150(value);
            let decoded = u16::from_le_bytes([cmd[2], cmd[3]]);
            prop_assert_eq!(decoded, value);
        }

        #[test]
        fn prop_constant_force_sign_preserved(
            max_torque in 0.1_f32..=20.0_f32,
            fraction in -1.0_f32..=1.0_f32,
        ) {
            let torque = max_torque * fraction;
            let enc = ThrustmasterConstantForceEncoder::new(max_torque);
            let mut out = [0u8; EFFECT_REPORT_LEN];
            enc.encode(torque, &mut out);
            let raw = i16::from_le_bytes([out[2], out[3]]);
            if torque > 0.01 {
                prop_assert!(raw >= 0, "positive torque {torque} yielded negative raw {raw}");
            } else if torque < -0.01 {
                prop_assert!(raw <= 0, "negative torque {torque} yielded positive raw {raw}");
            }
        }

        #[test]
        fn prop_kernel_gain_high_byte(gain: u16) {
            let cmd = build_kernel_gain_command(gain);
            prop_assert_eq!(cmd[0], 0x02);
            prop_assert_eq!(cmd[1], (gain >> 8) as u8);
        }
    }
}
