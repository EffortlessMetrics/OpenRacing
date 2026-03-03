//! Advanced tests for Moza protocol crate — device PID recognition, HBP parsing,
//! wheelbase report fields, torque encoding, and setting commands.

use racing_wheel_hid_moza_protocol::{
    DeviceSignature, FfbMode, MOZA_VENDOR_ID, MozaDeviceCategory, MozaDirectTorqueEncoder,
    MozaEsCompatibility, MozaHatDirection, MozaInputState, MozaModel, MozaProtocol,
    MozaTopologyHint, REPORT_LEN, SignatureVerdict, identify_device, is_wheelbase_product,
    product_ids, report_ids, rim_ids, verify_signature,
};

use proptest::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Device PID Recognition — every V1 and V2 wheelbase, peripherals, and rims
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_r5_v1_identity() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::R5_V1);
    assert_eq!(id.name, "Moza R5");
    assert_eq!(id.category, MozaDeviceCategory::Wheelbase);
    assert_eq!(id.topology_hint, MozaTopologyHint::WheelbaseAggregated);
    assert!(id.supports_ffb);
    Ok(())
}

#[test]
fn test_r5_v2_identity() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::R5_V2);
    assert_eq!(id.name, "Moza R5");
    assert!(id.supports_ffb);
    Ok(())
}

#[test]
fn test_r9_v1_identity() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::R9_V1);
    assert_eq!(id.name, "Moza R9");
    assert_eq!(id.category, MozaDeviceCategory::Wheelbase);
    assert!(id.supports_ffb);
    Ok(())
}

#[test]
fn test_r9_v2_identity() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::R9_V2);
    assert_eq!(id.name, "Moza R9");
    assert!(id.supports_ffb);
    Ok(())
}

#[test]
fn test_r12_v1_identity() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::R12_V1);
    assert_eq!(id.name, "Moza R12");
    assert_eq!(id.category, MozaDeviceCategory::Wheelbase);
    Ok(())
}

#[test]
fn test_r16_r21_v1_identity() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::R16_R21_V1);
    assert_eq!(id.name, "Moza R16/R21");
    assert_eq!(id.category, MozaDeviceCategory::Wheelbase);
    assert!(id.supports_ffb);
    Ok(())
}

#[test]
fn test_r16_r21_v2_identity() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::R16_R21_V2);
    assert_eq!(id.name, "Moza R16/R21");
    assert!(id.supports_ffb);
    Ok(())
}

#[test]
fn test_r3_v1_v2_identity() -> Result<(), Box<dyn std::error::Error>> {
    let v1 = identify_device(product_ids::R3_V1);
    let v2 = identify_device(product_ids::R3_V2);
    assert_eq!(v1.name, "Moza R3");
    assert_eq!(v2.name, "Moza R3");
    assert_eq!(v1.category, MozaDeviceCategory::Wheelbase);
    assert_eq!(v2.category, MozaDeviceCategory::Wheelbase);
    Ok(())
}

#[test]
fn test_srp_pedals_identity() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::SR_P_PEDALS);
    assert_eq!(id.name, "Moza SR-P Pedals");
    assert_eq!(id.category, MozaDeviceCategory::Pedals);
    assert_eq!(id.topology_hint, MozaTopologyHint::StandaloneUsb);
    assert!(!id.supports_ffb);
    Ok(())
}

#[test]
fn test_shifter_identities() -> Result<(), Box<dyn std::error::Error>> {
    let hgp = identify_device(product_ids::HGP_SHIFTER);
    let sgp = identify_device(product_ids::SGP_SHIFTER);
    assert_eq!(hgp.category, MozaDeviceCategory::Shifter);
    assert_eq!(sgp.category, MozaDeviceCategory::Shifter);
    assert!(!hgp.supports_ffb);
    assert!(!sgp.supports_ffb);
    Ok(())
}

#[test]
fn test_hbp_handbrake_identity() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::HBP_HANDBRAKE);
    assert_eq!(id.name, "Moza HBP Handbrake");
    assert_eq!(id.category, MozaDeviceCategory::Handbrake);
    assert!(!id.supports_ffb);
    Ok(())
}

#[test]
fn test_unknown_pid_identity() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(0xFFFF);
    assert_eq!(id.category, MozaDeviceCategory::Unknown);
    assert!(!id.supports_ffb);
    Ok(())
}

#[test]
fn test_all_wheelbases_are_wheelbase_products() -> Result<(), Box<dyn std::error::Error>> {
    let wheelbase_pids = [
        product_ids::R3_V1,
        product_ids::R3_V2,
        product_ids::R5_V1,
        product_ids::R5_V2,
        product_ids::R9_V1,
        product_ids::R9_V2,
        product_ids::R12_V1,
        product_ids::R12_V2,
        product_ids::R16_R21_V1,
        product_ids::R16_R21_V2,
    ];
    for pid in wheelbase_pids {
        assert!(
            is_wheelbase_product(pid),
            "PID 0x{pid:04X} must be a wheelbase product"
        );
    }
    Ok(())
}

#[test]
fn test_peripherals_not_wheelbase_products() -> Result<(), Box<dyn std::error::Error>> {
    let periph_pids = [
        product_ids::SR_P_PEDALS,
        product_ids::HGP_SHIFTER,
        product_ids::SGP_SHIFTER,
        product_ids::HBP_HANDBRAKE,
    ];
    for pid in periph_pids {
        assert!(
            !is_wheelbase_product(pid),
            "PID 0x{pid:04X} must not be a wheelbase product"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. V2 PID pattern verification (V2 = V1 | 0x0010)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_v2_pid_pattern() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_ids::R16_R21_V2, product_ids::R16_R21_V1 | 0x0010);
    assert_eq!(product_ids::R9_V2, product_ids::R9_V1 | 0x0010);
    assert_eq!(product_ids::R5_V2, product_ids::R5_V1 | 0x0010);
    assert_eq!(product_ids::R3_V2, product_ids::R3_V1 | 0x0010);
    assert_eq!(product_ids::R12_V2, product_ids::R12_V1 | 0x0010);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Rim ID constants
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_rim_id_constants_unique() -> Result<(), Box<dyn std::error::Error>> {
    let all_rims = [
        rim_ids::CS_V2,
        rim_ids::GS_V2,
        rim_ids::RS_V2,
        rim_ids::FSR,
        rim_ids::KS,
        rim_ids::ES,
    ];
    for (i, &a) in all_rims.iter().enumerate() {
        for (j, &b) in all_rims.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "rim IDs at index {i} and {j} must differ");
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Model max torque per device
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_model_max_torque_values() -> Result<(), Box<dyn std::error::Error>> {
    assert!((MozaModel::R3.max_torque_nm() - 3.9).abs() < 0.01);
    assert!((MozaModel::R5.max_torque_nm() - 5.5).abs() < 0.01);
    assert!((MozaModel::R9.max_torque_nm() - 9.0).abs() < 0.01);
    assert!((MozaModel::R12.max_torque_nm() - 12.0).abs() < 0.01);
    assert!((MozaModel::R16.max_torque_nm() - 16.0).abs() < 0.01);
    assert!((MozaModel::R21.max_torque_nm() - 21.0).abs() < 0.01);
    assert!((MozaModel::SrpPedals.max_torque_nm()).abs() < 0.01);
    assert!(MozaModel::Unknown.max_torque_nm() > 0.0);
    Ok(())
}

#[test]
fn test_model_from_pid_all_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(MozaModel::from_pid(product_ids::R3_V1), MozaModel::R3);
    assert_eq!(MozaModel::from_pid(product_ids::R3_V2), MozaModel::R3);
    assert_eq!(MozaModel::from_pid(product_ids::R5_V1), MozaModel::R5);
    assert_eq!(MozaModel::from_pid(product_ids::R5_V2), MozaModel::R5);
    assert_eq!(MozaModel::from_pid(product_ids::R9_V1), MozaModel::R9);
    assert_eq!(MozaModel::from_pid(product_ids::R9_V2), MozaModel::R9);
    assert_eq!(MozaModel::from_pid(product_ids::R12_V1), MozaModel::R12);
    assert_eq!(MozaModel::from_pid(product_ids::R12_V2), MozaModel::R12);
    assert_eq!(MozaModel::from_pid(product_ids::R16_R21_V1), MozaModel::R16);
    assert_eq!(MozaModel::from_pid(product_ids::R16_R21_V2), MozaModel::R16);
    assert_eq!(
        MozaModel::from_pid(product_ids::SR_P_PEDALS),
        MozaModel::SrpPedals
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Torque encoding at maximum rated torque per device
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_torque_encoding_at_max_per_model() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        MozaModel::R3,
        MozaModel::R5,
        MozaModel::R9,
        MozaModel::R12,
        MozaModel::R16,
        MozaModel::R21,
    ];
    for model in models {
        let max_nm = model.max_torque_nm();
        let enc = MozaDirectTorqueEncoder::new(max_nm);
        let mut out = [0u8; REPORT_LEN];
        let len = enc.encode(max_nm, 0, &mut out);
        assert_eq!(len, REPORT_LEN);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        assert_eq!(
            raw,
            i16::MAX,
            "model {:?} at max torque {max_nm} must produce i16::MAX, got {raw}",
            model
        );
        assert_eq!(out[3] & 0x01, 0x01, "motor enable must be set");
    }
    Ok(())
}

#[test]
fn test_torque_encoding_negative_max_per_model() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        MozaModel::R3,
        MozaModel::R5,
        MozaModel::R9,
        MozaModel::R12,
        MozaModel::R16,
        MozaModel::R21,
    ];
    for model in models {
        let max_nm = model.max_torque_nm();
        let enc = MozaDirectTorqueEncoder::new(max_nm);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(-max_nm, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        assert_eq!(
            raw,
            i16::MIN,
            "model {:?} at -max torque must produce i16::MIN, got {raw}",
            model
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Signature verification
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_signature_all_wheelbases_known() -> Result<(), Box<dyn std::error::Error>> {
    let wb_pids = [
        product_ids::R3_V1,
        product_ids::R3_V2,
        product_ids::R5_V1,
        product_ids::R5_V2,
        product_ids::R9_V1,
        product_ids::R9_V2,
        product_ids::R12_V1,
        product_ids::R12_V2,
        product_ids::R16_R21_V1,
        product_ids::R16_R21_V2,
    ];
    for pid in wb_pids {
        let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, pid);
        assert_eq!(
            verify_signature(&sig),
            SignatureVerdict::KnownWheelbase,
            "PID 0x{pid:04X} must be KnownWheelbase"
        );
    }
    Ok(())
}

#[test]
fn test_signature_peripherals_known() -> Result<(), Box<dyn std::error::Error>> {
    let periph_pids = [
        product_ids::SR_P_PEDALS,
        product_ids::HGP_SHIFTER,
        product_ids::SGP_SHIFTER,
        product_ids::HBP_HANDBRAKE,
    ];
    for pid in periph_pids {
        let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, pid);
        assert_eq!(
            verify_signature(&sig),
            SignatureVerdict::KnownPeripheral,
            "PID 0x{pid:04X} must be KnownPeripheral"
        );
    }
    Ok(())
}

#[test]
fn test_signature_non_moza_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let sig = DeviceSignature::from_vid_pid(0x046D, product_ids::R5_V1);
    assert_eq!(verify_signature(&sig), SignatureVerdict::Rejected);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. ES compatibility per wheelbase
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_es_compatibility_r9_v1_unsupported() -> Result<(), Box<dyn std::error::Error>> {
    let compat = racing_wheel_hid_moza_protocol::es_compatibility(product_ids::R9_V1);
    assert_eq!(compat, MozaEsCompatibility::UnsupportedHardwareRevision);
    assert!(!compat.is_supported());
    assert!(compat.diagnostic_message().is_some());
    Ok(())
}

#[test]
fn test_es_compatibility_r5_v1_supported() -> Result<(), Box<dyn std::error::Error>> {
    let compat = racing_wheel_hid_moza_protocol::es_compatibility(product_ids::R5_V1);
    assert_eq!(compat, MozaEsCompatibility::Supported);
    assert!(compat.is_supported());
    Ok(())
}

#[test]
fn test_es_compatibility_r9_v2_supported() -> Result<(), Box<dyn std::error::Error>> {
    let compat = racing_wheel_hid_moza_protocol::es_compatibility(product_ids::R9_V2);
    assert_eq!(compat, MozaEsCompatibility::Supported);
    Ok(())
}

#[test]
fn test_es_compatibility_pedals_not_wheelbase() -> Result<(), Box<dyn std::error::Error>> {
    let compat = racing_wheel_hid_moza_protocol::es_compatibility(product_ids::SR_P_PEDALS);
    assert_eq!(compat, MozaEsCompatibility::NotWheelbase);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Hat direction parsing
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_hat_direction_all_values() -> Result<(), Box<dyn std::error::Error>> {
    let expected = [
        (0, MozaHatDirection::Up),
        (1, MozaHatDirection::UpRight),
        (2, MozaHatDirection::Right),
        (3, MozaHatDirection::DownRight),
        (4, MozaHatDirection::Down),
        (5, MozaHatDirection::DownLeft),
        (6, MozaHatDirection::Left),
        (7, MozaHatDirection::UpLeft),
        (8, MozaHatDirection::Center),
    ];
    for (val, dir) in expected {
        assert_eq!(
            MozaHatDirection::from_hid_hat_value(val),
            Some(dir),
            "hat value {val} must map to {dir:?}"
        );
    }
    assert!(MozaHatDirection::from_hid_hat_value(9).is_none());
    assert!(MozaHatDirection::from_hid_hat_value(0xFF).is_none());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Protocol init state and FFB mode
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_protocol_new_starts_uninitialized() -> Result<(), Box<dyn std::error::Error>> {
    let proto = MozaProtocol::new_with_config(product_ids::R9_V2, FfbMode::Standard, false);
    assert_eq!(
        proto.init_state(),
        racing_wheel_hid_moza_protocol::MozaInitState::Uninitialized
    );
    assert!(!proto.is_ffb_ready());
    assert_eq!(proto.retry_count(), 0);
    assert_eq!(proto.model(), MozaModel::R9);
    Ok(())
}

#[test]
fn test_protocol_ffb_mode_direct() -> Result<(), Box<dyn std::error::Error>> {
    let proto = MozaProtocol::new_with_ffb_mode(product_ids::R5_V1, FfbMode::Direct);
    assert_eq!(proto.ffb_mode(), FfbMode::Direct);
    Ok(())
}

#[test]
fn test_protocol_is_v2_hardware() -> Result<(), Box<dyn std::error::Error>> {
    let v1 = MozaProtocol::new(product_ids::R5_V1);
    let v2 = MozaProtocol::new(product_ids::R5_V2);
    assert!(!racing_wheel_hid_moza_protocol::writer::VendorProtocol::is_v2_hardware(&v1));
    assert!(racing_wheel_hid_moza_protocol::writer::VendorProtocol::is_v2_hardware(&v2));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Direct torque encoder — slew rate, zero max, report structure
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_encoder_slew_rate_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(9.0).with_slew_rate(1000);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(4.5, 0, &mut out);
    assert_eq!(out[3] & 0x02, 0x02, "slew-rate flag must be set");
    assert_eq!(u16::from_le_bytes([out[4], out[5]]), 1000);
    Ok(())
}

#[test]
fn test_encoder_zero_torque_no_slew_flag() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(9.0).with_slew_rate(500);
    let mut out = [0u8; REPORT_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(out[3] & 0x01, 0x00, "motor must be disabled");
    assert_eq!(i16::from_le_bytes([out[1], out[2]]), 0);
    Ok(())
}

#[test]
fn test_encoder_report_id_always_direct_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(5.5);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(2.0, 0, &mut out);
    assert_eq!(out[0], report_ids::DIRECT_TORQUE);
    enc.encode_zero(&mut out);
    assert_eq!(out[0], report_ids::DIRECT_TORQUE);
    Ok(())
}

#[test]
fn test_encoder_reserved_bytes_always_zero() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(12.0);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(6.0, 0, &mut out);
    assert_eq!(out[6], 0);
    assert_eq!(out[7], 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. MozaInputState empty constructor
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_input_state_empty_zeroed() -> Result<(), Box<dyn std::error::Error>> {
    let state = MozaInputState::empty(42);
    assert_eq!(state.tick, 42);
    assert_eq!(state.steering_u16, 0);
    assert_eq!(state.throttle_u16, 0);
    assert_eq!(state.brake_u16, 0);
    assert_eq!(state.clutch_u16, 0);
    assert_eq!(state.handbrake_u16, 0);
    assert_eq!(state.hat, 0);
    assert_eq!(state.funky, 0);
    assert_eq!(state.buttons, [0u8; 16]);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. Proptest: torque round-trip accuracy and boundary testing
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    #[test]
    fn prop_torque_round_trip_accuracy(
        max in 0.1_f32..=21.0_f32,
        frac in -1.0_f32..=1.0_f32,
    ) {
        let torque_nm = max * frac;
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(torque_nm, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        let recovered = if raw >= 0 {
            (raw as f32 / i16::MAX as f32) * max
        } else {
            (raw as f32 / (-(i16::MIN as f32))) * max
        };
        let error = (recovered - torque_nm).abs();
        let tolerance = max / (i16::MAX as f32) + 0.001;
        prop_assert!(
            error <= tolerance,
            "round-trip error {error} > tolerance {tolerance} for torque={torque_nm}, max={max}"
        );
    }

    #[test]
    fn prop_sign_symmetry(
        max in 0.1_f32..=21.0_f32,
        frac in 0.001_f32..=1.0_f32,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out_pos = [0u8; REPORT_LEN];
        let mut out_neg = [0u8; REPORT_LEN];
        enc.encode(max * frac, 0, &mut out_pos);
        enc.encode(-max * frac, 0, &mut out_neg);
        let raw_pos = i16::from_le_bytes([out_pos[1], out_pos[2]]);
        let raw_neg = i16::from_le_bytes([out_neg[1], out_neg[2]]);
        // |+raw| ≈ |-raw| within 1 LSB
        prop_assert!(
            ((raw_pos as i32) + (raw_neg as i32)).abs() <= 1,
            "sign asymmetry: +raw={raw_pos}, -raw={raw_neg}"
        );
    }

    #[test]
    fn prop_identify_device_never_panics(pid in proptest::num::u16::ANY) {
        let _id = identify_device(pid);
    }

    #[test]
    fn prop_hat_out_of_range_returns_none(val in 9u8..=255u8) {
        prop_assert!(MozaHatDirection::from_hid_hat_value(val).is_none());
    }
}
