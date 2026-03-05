//! Deep protocol tests for Moza HID protocol.
//!
//! Covers torque encoding/decoding, device identification, signature verification,
//! HBP integration, ES compatibility, and property-based round-trip guarantees.

use racing_wheel_hid_moza_protocol::direct::REPORT_LEN;
use racing_wheel_hid_moza_protocol::ids::{MOZA_VENDOR_ID, product_ids, rim_ids};
use racing_wheel_hid_moza_protocol::report::report_ids;
use racing_wheel_hid_moza_protocol::rt_types::{TorqueEncoder, TorqueQ8_8};
use racing_wheel_hid_moza_protocol::signature::{DeviceSignature, SignatureVerdict};
use racing_wheel_hid_moza_protocol::types::{
    ES_BUTTON_COUNT, ES_LED_COUNT, MozaDeviceCategory, MozaEsCompatibility, MozaEsJoystickMode,
    MozaHatDirection, MozaInputState, MozaModel, MozaPedalAxesRaw, MozaTopologyHint,
};
use racing_wheel_hid_moza_protocol::{
    FfbMode, MozaDirectTorqueEncoder, MozaInitState, MozaProtocol, MozaRetryPolicy,
    es_compatibility, identify_device, is_wheelbase_product, verify_signature,
};

// ─── Torque encoding: boundary values ────────────────────────────────────────

#[test]
fn torque_encode_i16_min_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(9.0);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(-9.0, 0, &mut out);
    let raw = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(raw, i16::MIN, "full negative must saturate to i16::MIN");
    Ok(())
}

#[test]
fn torque_encode_i16_max_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(9.0);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(9.0, 0, &mut out);
    let raw = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(raw, i16::MAX, "full positive must saturate to i16::MAX");
    Ok(())
}

#[test]
fn torque_encode_sign_preserved_small_values() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(21.0);
    let mut out = [0u8; REPORT_LEN];

    // Small positive
    enc.encode(0.01, 0, &mut out);
    let raw_pos = i16::from_le_bytes([out[1], out[2]]);
    assert!(
        raw_pos >= 0,
        "small positive torque must encode as non-negative raw={raw_pos}"
    );

    // Small negative
    enc.encode(-0.01, 0, &mut out);
    let raw_neg = i16::from_le_bytes([out[1], out[2]]);
    assert!(
        raw_neg <= 0,
        "small negative torque must encode as non-positive raw={raw_neg}"
    );
    Ok(())
}

#[test]
fn torque_encode_overflow_clamps_positive() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(5.5);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(100.0, 0, &mut out);
    let raw = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(raw, i16::MAX, "overflow positive must clamp to i16::MAX");
    Ok(())
}

#[test]
fn torque_encode_overflow_clamps_negative() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(5.5);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(-100.0, 0, &mut out);
    let raw = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(raw, i16::MIN, "overflow negative must clamp to i16::MIN");
    Ok(())
}

#[test]
fn torque_encode_zero_max_torque_is_safe() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(0.0);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(5.0, 0, &mut out);
    let raw = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(raw, 0, "zero max_torque must always produce raw=0");
    assert_eq!(
        out[3] & 0x01,
        0x00,
        "motor must be disabled for zero output"
    );
    Ok(())
}

#[test]
fn torque_encode_negative_max_torque_floors_to_zero() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(-5.0);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(3.0, 0, &mut out);
    let raw = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(raw, 0, "negative max_torque floors to 0 → raw must be 0");
    Ok(())
}

// ─── Torque encoding: slew rate ──────────────────────────────────────────────

#[test]
fn torque_encode_slew_rate_zero_value() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(12.0).with_slew_rate(0);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(6.0, 0, &mut out);
    assert_eq!(out[3] & 0x02, 0x02, "slew-rate flag must be set");
    assert_eq!(
        u16::from_le_bytes([out[4], out[5]]),
        0,
        "slew-rate value must be 0"
    );
    Ok(())
}

#[test]
fn torque_encode_slew_rate_max_u16() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(12.0).with_slew_rate(u16::MAX);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(6.0, 0, &mut out);
    assert_eq!(out[3] & 0x02, 0x02, "slew-rate flag must be set");
    assert_eq!(
        u16::from_le_bytes([out[4], out[5]]),
        u16::MAX,
        "slew-rate must preserve u16::MAX"
    );
    Ok(())
}

#[test]
fn torque_encode_without_slew_rate_has_zero_bytes_4_5() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(12.0);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(6.0, 0, &mut out);
    assert_eq!(out[3] & 0x02, 0x00, "slew-rate flag must not be set");
    assert_eq!(out[4], 0);
    assert_eq!(out[5], 0);
    Ok(())
}

// ─── TorqueEncoder trait (Q8.8 path) ─────────────────────────────────────────

#[test]
fn q8_8_trait_clamp_values_per_model() -> Result<(), Box<dyn std::error::Error>> {
    let models: &[(f32, TorqueQ8_8)] = &[
        (3.9, (3.9_f64 * 256.0).round() as TorqueQ8_8),
        (5.5, (5.5_f64 * 256.0).round() as TorqueQ8_8),
        (9.0, (9.0_f64 * 256.0).round() as TorqueQ8_8),
        (12.0, (12.0_f64 * 256.0).round() as TorqueQ8_8),
        (16.0, (16.0_f64 * 256.0).round() as TorqueQ8_8),
        (21.0, (21.0_f64 * 256.0).round() as TorqueQ8_8),
    ];
    for &(max_nm, expected_clamp) in models {
        let enc = MozaDirectTorqueEncoder::new(max_nm);
        assert_eq!(
            TorqueEncoder::<REPORT_LEN>::clamp_max(&enc),
            expected_clamp,
            "clamp_max for {max_nm} Nm"
        );
        assert_eq!(
            TorqueEncoder::<REPORT_LEN>::clamp_min(&enc),
            -expected_clamp,
            "clamp_min for {max_nm} Nm"
        );
    }
    Ok(())
}

#[test]
fn q8_8_trait_positive_is_clockwise() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(9.0);
    assert!(TorqueEncoder::<REPORT_LEN>::positive_is_clockwise(&enc));
    Ok(())
}

#[test]
fn q8_8_trait_flags_passthrough() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(5.5);
    let mut out = [0u8; REPORT_LEN];
    // Encode with extra flags (0xAA) — motor enable (bit 0) should be OR'd
    TorqueEncoder::<REPORT_LEN>::encode(&enc, 256, 0, 0xAA, &mut out);
    // 256 Q8.8 = 1.0 Nm, which is nonzero → motor enable bit set
    assert_eq!(out[3] & 0xAA, 0xAA, "extra flags must be preserved");
    assert_eq!(out[3] & 0x01, 0x01, "motor enable must be OR'd with flags");
    Ok(())
}

#[test]
fn q8_8_trait_encode_zero_disables_motor() -> Result<(), Box<dyn std::error::Error>> {
    let enc = MozaDirectTorqueEncoder::new(9.0);
    let mut out = [0xFFu8; REPORT_LEN];
    TorqueEncoder::<REPORT_LEN>::encode_zero(&enc, &mut out);
    assert_eq!(out[0], report_ids::DIRECT_TORQUE);
    let raw = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(raw, 0, "encode_zero must produce raw=0");
    assert_eq!(out[3] & 0x01, 0x00, "motor must be disabled");
    Ok(())
}

// ─── Device identification: all known Moza PIDs ──────────────────────────────

#[test]
fn identify_all_v1_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
    let v1_pids = [
        (product_ids::R3_V1, "R3"),
        (product_ids::R5_V1, "R5"),
        (product_ids::R9_V1, "R9"),
        (product_ids::R12_V1, "R12"),
        (product_ids::R16_R21_V1, "R16/R21"),
    ];
    for (pid, label) in v1_pids {
        let identity = identify_device(pid);
        assert_eq!(
            identity.category,
            MozaDeviceCategory::Wheelbase,
            "{label} V1 (0x{pid:04X}) must be Wheelbase"
        );
        assert!(identity.supports_ffb, "{label} V1 must support FFB");
        assert_eq!(
            identity.topology_hint,
            MozaTopologyHint::WheelbaseAggregated,
            "{label} V1 must be WheelbaseAggregated"
        );
    }
    Ok(())
}

#[test]
fn identify_all_v2_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
    let v2_pids = [
        (product_ids::R3_V2, "R3"),
        (product_ids::R5_V2, "R5"),
        (product_ids::R9_V2, "R9"),
        (product_ids::R12_V2, "R12"),
        (product_ids::R16_R21_V2, "R16/R21"),
    ];
    for (pid, label) in v2_pids {
        let identity = identify_device(pid);
        assert_eq!(
            identity.category,
            MozaDeviceCategory::Wheelbase,
            "{label} V2 (0x{pid:04X}) must be Wheelbase"
        );
        assert!(identity.supports_ffb, "{label} V2 must support FFB");
    }
    Ok(())
}

#[test]
fn identify_all_peripherals() -> Result<(), Box<dyn std::error::Error>> {
    let peripherals = [
        (
            product_ids::SR_P_PEDALS,
            MozaDeviceCategory::Pedals,
            "SR-P Pedals",
        ),
        (
            product_ids::HGP_SHIFTER,
            MozaDeviceCategory::Shifter,
            "HGP Shifter",
        ),
        (
            product_ids::SGP_SHIFTER,
            MozaDeviceCategory::Shifter,
            "SGP Shifter",
        ),
        (
            product_ids::HBP_HANDBRAKE,
            MozaDeviceCategory::Handbrake,
            "HBP Handbrake",
        ),
    ];
    for (pid, expected_cat, label) in peripherals {
        let identity = identify_device(pid);
        assert_eq!(
            identity.category, expected_cat,
            "{label} (0x{pid:04X}) must be {expected_cat:?}"
        );
        assert!(!identity.supports_ffb, "{label} must not support FFB");
        assert_eq!(
            identity.topology_hint,
            MozaTopologyHint::StandaloneUsb,
            "{label} must be StandaloneUsb"
        );
    }
    Ok(())
}

#[test]
fn identify_unknown_pid_returns_unknown() -> Result<(), Box<dyn std::error::Error>> {
    let identity = identify_device(0xFFFF);
    assert_eq!(identity.category, MozaDeviceCategory::Unknown);
    assert!(!identity.supports_ffb);
    assert_eq!(identity.topology_hint, MozaTopologyHint::Unknown);
    Ok(())
}

#[test]
fn is_wheelbase_product_covers_all_known_pids() -> Result<(), Box<dyn std::error::Error>> {
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
        assert!(is_wheelbase_product(pid), "0x{pid:04X} must be a wheelbase");
    }
    // Non-wheelbase
    assert!(!is_wheelbase_product(product_ids::SR_P_PEDALS));
    assert!(!is_wheelbase_product(product_ids::HGP_SHIFTER));
    assert!(!is_wheelbase_product(product_ids::HBP_HANDBRAKE));
    assert!(!is_wheelbase_product(0xFFFF));
    Ok(())
}

// ─── V2 PID pattern: V1 | 0x0010 ────────────────────────────────────────────

#[test]
fn v2_pids_are_v1_or_0x0010() -> Result<(), Box<dyn std::error::Error>> {
    let pairs = [
        (product_ids::R3_V1, product_ids::R3_V2),
        (product_ids::R5_V1, product_ids::R5_V2),
        (product_ids::R9_V1, product_ids::R9_V2),
        (product_ids::R12_V1, product_ids::R12_V2),
        (product_ids::R16_R21_V1, product_ids::R16_R21_V2),
    ];
    for (v1, v2) in pairs {
        assert_eq!(
            v1 | 0x0010,
            v2,
            "V2 PID 0x{v2:04X} must equal V1 PID 0x{v1:04X} | 0x0010"
        );
    }
    Ok(())
}

// ─── MozaModel ───────────────────────────────────────────────────────────────

#[test]
fn model_from_pid_all_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
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
    assert_eq!(MozaModel::from_pid(0xDEAD), MozaModel::Unknown);
    Ok(())
}

#[test]
fn model_max_torque_nm_ordering() -> Result<(), Box<dyn std::error::Error>> {
    // R3 < R5 < R9 < R12 < R16 < R21
    assert!(MozaModel::R3.max_torque_nm() < MozaModel::R5.max_torque_nm());
    assert!(MozaModel::R5.max_torque_nm() < MozaModel::R9.max_torque_nm());
    assert!(MozaModel::R9.max_torque_nm() < MozaModel::R12.max_torque_nm());
    assert!(MozaModel::R12.max_torque_nm() < MozaModel::R16.max_torque_nm());
    assert!(MozaModel::R16.max_torque_nm() < MozaModel::R21.max_torque_nm());
    Ok(())
}

#[test]
fn model_max_torque_known_values() -> Result<(), Box<dyn std::error::Error>> {
    assert!((MozaModel::R3.max_torque_nm() - 3.9).abs() < 0.01);
    assert!((MozaModel::R5.max_torque_nm() - 5.5).abs() < 0.01);
    assert!((MozaModel::R9.max_torque_nm() - 9.0).abs() < 0.01);
    assert!((MozaModel::R12.max_torque_nm() - 12.0).abs() < 0.01);
    assert!((MozaModel::R16.max_torque_nm() - 16.0).abs() < 0.01);
    assert!((MozaModel::R21.max_torque_nm() - 21.0).abs() < 0.01);
    assert!((MozaModel::SrpPedals.max_torque_nm()).abs() < 0.01);
    Ok(())
}

// ─── Signature verification ──────────────────────────────────────────────────

#[test]
fn signature_all_wheelbase_pids() -> Result<(), Box<dyn std::error::Error>> {
    let all_wheelbase_pids = [
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
    for pid in all_wheelbase_pids {
        let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, pid);
        assert_eq!(
            verify_signature(&sig),
            SignatureVerdict::KnownWheelbase,
            "PID 0x{pid:04X} must verify as KnownWheelbase"
        );
    }
    Ok(())
}

#[test]
fn signature_all_peripheral_pids() -> Result<(), Box<dyn std::error::Error>> {
    let peripheral_pids = [
        product_ids::SR_P_PEDALS,
        product_ids::HGP_SHIFTER,
        product_ids::SGP_SHIFTER,
        product_ids::HBP_HANDBRAKE,
    ];
    for pid in peripheral_pids {
        let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, pid);
        assert_eq!(
            verify_signature(&sig),
            SignatureVerdict::KnownPeripheral,
            "PID 0x{pid:04X} must verify as KnownPeripheral"
        );
    }
    Ok(())
}

#[test]
fn signature_wrong_vid_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let sig = DeviceSignature::from_vid_pid(0x0001, product_ids::R5_V1);
    assert_eq!(verify_signature(&sig), SignatureVerdict::Rejected);
    Ok(())
}

#[test]
fn signature_unknown_moza_pid() -> Result<(), Box<dyn std::error::Error>> {
    let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, 0xBEEF);
    assert_eq!(verify_signature(&sig), SignatureVerdict::UnknownProduct);
    Ok(())
}

#[test]
fn signature_optional_fields_do_not_affect_verdict() -> Result<(), Box<dyn std::error::Error>> {
    let sig = DeviceSignature {
        vendor_id: MOZA_VENDOR_ID,
        product_id: product_ids::R9_V2,
        interface_number: Some(0),
        descriptor_len: Some(512),
        descriptor_crc32: Some(0xDEADBEEF),
    };
    assert_eq!(verify_signature(&sig), SignatureVerdict::KnownWheelbase);
    Ok(())
}

// ─── ES compatibility ────────────────────────────────────────────────────────

#[test]
fn es_compatibility_r9_v1_unsupported() -> Result<(), Box<dyn std::error::Error>> {
    let compat = es_compatibility(product_ids::R9_V1);
    assert_eq!(compat, MozaEsCompatibility::UnsupportedHardwareRevision);
    assert!(!compat.is_supported());
    assert!(compat.diagnostic_message().is_some());
    Ok(())
}

#[test]
fn es_compatibility_r5_r9v2_supported() -> Result<(), Box<dyn std::error::Error>> {
    let supported = [product_ids::R5_V1, product_ids::R5_V2, product_ids::R9_V2];
    for pid in supported {
        let compat = es_compatibility(pid);
        assert_eq!(compat, MozaEsCompatibility::Supported, "0x{pid:04X}");
        assert!(compat.is_supported());
    }
    Ok(())
}

#[test]
fn es_compatibility_unknown_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
    let unknown_bases = [
        product_ids::R3_V1,
        product_ids::R3_V2,
        product_ids::R12_V1,
        product_ids::R12_V2,
        product_ids::R16_R21_V1,
        product_ids::R16_R21_V2,
    ];
    for pid in unknown_bases {
        assert_eq!(
            es_compatibility(pid),
            MozaEsCompatibility::UnknownWheelbase,
            "0x{pid:04X}"
        );
    }
    Ok(())
}

#[test]
fn es_compatibility_non_wheelbase() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        es_compatibility(product_ids::SR_P_PEDALS),
        MozaEsCompatibility::NotWheelbase
    );
    assert_eq!(
        es_compatibility(product_ids::HGP_SHIFTER),
        MozaEsCompatibility::NotWheelbase
    );
    assert_eq!(es_compatibility(0xFFFF), MozaEsCompatibility::NotWheelbase);
    Ok(())
}

// ─── MozaEsJoystickMode ─────────────────────────────────────────────────────

#[test]
fn es_joystick_mode_from_config_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        MozaEsJoystickMode::from_config_value(0),
        Some(MozaEsJoystickMode::Buttons)
    );
    assert_eq!(
        MozaEsJoystickMode::from_config_value(1),
        Some(MozaEsJoystickMode::DPad)
    );
    assert_eq!(MozaEsJoystickMode::from_config_value(2), None);
    assert_eq!(MozaEsJoystickMode::from_config_value(255), None);
    Ok(())
}

// ─── MozaHatDirection ────────────────────────────────────────────────────────

#[test]
fn hat_direction_all_valid_values() -> Result<(), Box<dyn std::error::Error>> {
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
    for (value, direction) in expected {
        assert_eq!(
            MozaHatDirection::from_hid_hat_value(value),
            Some(direction),
            "hat value {value}"
        );
    }
    Ok(())
}

#[test]
fn hat_direction_invalid_values_return_none() -> Result<(), Box<dyn std::error::Error>> {
    for value in [9, 10, 15, 128, 255] {
        assert_eq!(
            MozaHatDirection::from_hid_hat_value(value),
            None,
            "hat value {value} must be None"
        );
    }
    Ok(())
}

// ─── Rim IDs ─────────────────────────────────────────────────────────────────

#[test]
fn rim_ids_are_unique_and_sequential() -> Result<(), Box<dyn std::error::Error>> {
    let all_rim_ids = [
        rim_ids::CS_V2,
        rim_ids::GS_V2,
        rim_ids::RS_V2,
        rim_ids::FSR,
        rim_ids::KS,
        rim_ids::ES,
    ];
    // All unique
    for i in 0..all_rim_ids.len() {
        for j in (i + 1)..all_rim_ids.len() {
            assert_ne!(
                all_rim_ids[i], all_rim_ids[j],
                "rim IDs at index {i} and {j} must be unique"
            );
        }
    }
    // Verify known values
    assert_eq!(rim_ids::CS_V2, 0x01);
    assert_eq!(rim_ids::GS_V2, 0x02);
    assert_eq!(rim_ids::RS_V2, 0x03);
    assert_eq!(rim_ids::FSR, 0x04);
    assert_eq!(rim_ids::KS, 0x05);
    assert_eq!(rim_ids::ES, 0x06);
    Ok(())
}

// ─── ES constants ────────────────────────────────────────────────────────────

#[test]
fn es_constants_documented_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(ES_BUTTON_COUNT, 22);
    assert_eq!(ES_LED_COUNT, 10);
    Ok(())
}

// ─── MozaPedalAxesRaw normalization ──────────────────────────────────────────

#[test]
fn pedal_axes_normalize_full_range() -> Result<(), Box<dyn std::error::Error>> {
    let raw = MozaPedalAxesRaw {
        throttle: u16::MAX,
        brake: 0,
        clutch: Some(u16::MAX / 2),
        handbrake: None,
    };
    let norm = raw.normalize();
    assert!(
        (norm.throttle - 1.0).abs() < 0.001,
        "max throttle must normalize to 1.0"
    );
    assert!(norm.brake.abs() < 0.001, "zero brake must normalize to 0.0");
    assert!((norm.clutch.ok_or("clutch missing")? - 0.5).abs() < 0.01);
    assert!(norm.handbrake.is_none());
    Ok(())
}

#[test]
fn pedal_axes_normalize_zero() -> Result<(), Box<dyn std::error::Error>> {
    let raw = MozaPedalAxesRaw {
        throttle: 0,
        brake: 0,
        clutch: Some(0),
        handbrake: Some(0),
    };
    let norm = raw.normalize();
    assert!(norm.throttle.abs() < 0.001);
    assert!(norm.brake.abs() < 0.001);
    assert!(norm.clutch.ok_or("clutch missing")?.abs() < 0.001);
    assert!(norm.handbrake.ok_or("handbrake missing")?.abs() < 0.001);
    Ok(())
}

// ─── MozaInputState ──────────────────────────────────────────────────────────

#[test]
fn input_state_empty_has_correct_tick() -> Result<(), Box<dyn std::error::Error>> {
    let state = MozaInputState::empty(42);
    assert_eq!(state.tick, 42);
    assert_eq!(state.steering_u16, 0);
    assert_eq!(state.throttle_u16, 0);
    assert_eq!(state.brake_u16, 0);
    assert_eq!(state.buttons, [0u8; 16]);
    Ok(())
}

#[test]
fn input_state_both_clutches_pressed() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = MozaInputState::empty(0);
    state.clutch_u16 = 50000;
    state.handbrake_u16 = 50000;
    assert!(state.both_clutches_pressed(40000));
    assert!(!state.both_clutches_pressed(60000));
    Ok(())
}

// ─── MozaProtocol ────────────────────────────────────────────────────────────

#[test]
fn protocol_new_creates_correct_model() -> Result<(), Box<dyn std::error::Error>> {
    let proto = MozaProtocol::new(product_ids::R9_V2);
    assert_eq!(proto.model(), MozaModel::R9);
    assert_eq!(proto.product_id(), product_ids::R9_V2);
    assert_eq!(proto.init_state(), MozaInitState::Uninitialized);
    Ok(())
}

#[test]
fn protocol_ffb_mode_configurations() -> Result<(), Box<dyn std::error::Error>> {
    let standard = MozaProtocol::new_with_ffb_mode(product_ids::R5_V1, FfbMode::Standard);
    assert_eq!(standard.ffb_mode(), FfbMode::Standard);

    let direct = MozaProtocol::new_with_ffb_mode(product_ids::R5_V1, FfbMode::Direct);
    assert_eq!(direct.ffb_mode(), FfbMode::Direct);

    let off = MozaProtocol::new_with_ffb_mode(product_ids::R5_V1, FfbMode::Off);
    assert_eq!(off.ffb_mode(), FfbMode::Off);
    Ok(())
}

#[test]
fn protocol_es_compatibility_delegates() -> Result<(), Box<dyn std::error::Error>> {
    let proto_r9v1 = MozaProtocol::new(product_ids::R9_V1);
    assert_eq!(
        proto_r9v1.es_compatibility(),
        MozaEsCompatibility::UnsupportedHardwareRevision
    );

    let proto_r5v2 = MozaProtocol::new(product_ids::R5_V2);
    assert_eq!(
        proto_r5v2.es_compatibility(),
        MozaEsCompatibility::Supported
    );
    Ok(())
}

// ─── MozaRetryPolicy ─────────────────────────────────────────────────────────

#[test]
fn retry_policy_exponential_backoff_sequence() -> Result<(), Box<dyn std::error::Error>> {
    let policy = MozaRetryPolicy {
        max_retries: 5,
        base_delay_ms: 100,
    };
    let d0 = policy.delay_ms_for(0);
    let d1 = policy.delay_ms_for(1);
    let d2 = policy.delay_ms_for(2);
    // Each delay should increase
    assert!(d1 > d0, "delay must increase: d1={d1} > d0={d0}");
    assert!(d2 > d1, "delay must increase: d2={d2} > d1={d1}");
    Ok(())
}

// ─── MozaInitState ───────────────────────────────────────────────────────────

#[test]
fn init_state_to_u8_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let states = [
        MozaInitState::Uninitialized,
        MozaInitState::Initializing,
        MozaInitState::Ready,
        MozaInitState::Failed,
        MozaInitState::PermanentFailure,
    ];
    let mut seen_values = std::collections::HashSet::new();
    for state in states {
        let value = state.to_u8();
        assert!(
            seen_values.insert(value),
            "init state {state:?} must have unique u8 value"
        );
    }
    Ok(())
}

// ─── Report IDs golden values ────────────────────────────────────────────────

#[test]
fn report_ids_golden() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(report_ids::DEVICE_INFO, 0x01);
    assert_eq!(report_ids::HIGH_TORQUE, 0x02);
    assert_eq!(report_ids::START_REPORTS, 0x03);
    assert_eq!(report_ids::ROTATION_RANGE, 0x10);
    assert_eq!(report_ids::FFB_MODE, 0x11);
    assert_eq!(report_ids::DIRECT_TORQUE, 0x20);
    assert_eq!(report_ids::DEVICE_GAIN, 0x21);
    Ok(())
}

// ─── FfbMode discriminant values ─────────────────────────────────────────────

#[test]
fn ffb_mode_discriminants() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(FfbMode::Off as u8, 0xFF);
    assert_eq!(FfbMode::Standard as u8, 0x00);
    assert_eq!(FfbMode::Direct as u8, 0x02);
    Ok(())
}

// ─── Vendor ID ───────────────────────────────────────────────────────────────

#[test]
fn vendor_id_correct() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(MOZA_VENDOR_ID, 0x346E);
    Ok(())
}

// ─── Torque encoding per-model round-trip ────────────────────────────────────

#[test]
fn torque_round_trip_all_models() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        (MozaModel::R3, 3.9_f32),
        (MozaModel::R5, 5.5),
        (MozaModel::R9, 9.0),
        (MozaModel::R12, 12.0),
        (MozaModel::R16, 16.0),
        (MozaModel::R21, 21.0),
    ];
    for (model, max_torque) in models {
        let enc = MozaDirectTorqueEncoder::new(max_torque);
        let mut out = [0u8; REPORT_LEN];
        let half = max_torque * 0.5;
        enc.encode(half, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        // Decode back
        let decoded = (raw as f32 / i16::MAX as f32) * max_torque;
        assert!(
            (decoded - half).abs() < 0.01,
            "{model:?}: round-trip error too large: encoded {half} → raw {raw} → decoded {decoded}"
        );
    }
    Ok(())
}

// ─── Proptest: torque round-trip ─────────────────────────────────────────────

mod proptest_deep {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config { cases: 1000, timeout: 60_000, ..proptest::test_runner::Config::default() })]

        /// Any valid torque value within [-max, max] round-trips correctly
        /// through encode → decode with bounded error.
        #[test]
        fn prop_torque_round_trip(
            max_torque in 0.5_f32..=21.0_f32,
            fraction in -1.0_f32..=1.0_f32,
        ) {
            let torque_nm = max_torque * fraction;
            let enc = MozaDirectTorqueEncoder::new(max_torque);
            let mut out = [0u8; REPORT_LEN];
            enc.encode(torque_nm, 0, &mut out);
            let raw = i16::from_le_bytes([out[1], out[2]]);

            // Decode: raw / i16::MAX * max_torque for positive,
            //         raw / -i16::MIN * max_torque for negative
            let decoded = if raw >= 0 {
                (raw as f32 / i16::MAX as f32) * max_torque
            } else {
                (raw as f32 / (-(i16::MIN as f32))) * max_torque
            };

            // Error bound: at most 1 LSB of quantization
            let lsb = max_torque / i16::MAX as f32;
            let error = (decoded - torque_nm).abs();
            prop_assert!(
                error < lsb + 0.001,
                "round-trip error {error} exceeds 1 LSB ({lsb}) for torque={torque_nm}, max={max_torque}, raw={raw}"
            );
        }

        /// Reserved bytes (6-7) are always zero regardless of input.
        #[test]
        fn prop_reserved_bytes_always_zero(
            max_torque in 0.1_f32..=21.0_f32,
            torque in -50.0_f32..=50.0_f32,
        ) {
            let enc = MozaDirectTorqueEncoder::new(max_torque);
            let mut out = [0u8; REPORT_LEN];
            enc.encode(torque, 0, &mut out);
            prop_assert_eq!(out[6], 0, "byte 6 must be zero");
            prop_assert_eq!(out[7], 0, "byte 7 must be zero");
        }

        /// Verify all Moza device PIDs are uniquely identified.
        #[test]
        fn prop_identify_device_deterministic(pid: u16) {
            let a = identify_device(pid);
            let b = identify_device(pid);
            prop_assert_eq!(a.product_id, b.product_id);
            prop_assert_eq!(a.category, b.category);
            prop_assert_eq!(a.supports_ffb, b.supports_ffb);
        }
    }
}
