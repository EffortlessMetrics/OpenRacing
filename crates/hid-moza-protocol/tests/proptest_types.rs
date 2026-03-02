//! Property-based tests for Moza device types, enum parsing, and normalization.
//!
//! Covers:
//! - `MozaHatDirection::from_hid_hat_value` valid/invalid ranges
//! - `MozaEsJoystickMode::from_config_value` valid/invalid ranges
//! - `MozaPedalAxesRaw::normalize` output bounds and `None` preservation
//! - `MozaModel::from_pid` returns `Unknown` for unrecognised PIDs
//! - `identify_device` determinism and PID echo-back
//! - `es_compatibility` wheelbase/non-wheelbase consistency
//! - V2 PID derivation pattern (V1 | 0x0010)

use proptest::prelude::*;
use racing_wheel_hid_moza_protocol::{
    MozaEsCompatibility, MozaEsJoystickMode, MozaHatDirection, MozaModel, MozaPedalAxesRaw,
    es_compatibility, identify_device, is_wheelbase_product, product_ids,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // ── Hat direction parsing ─────────────────────────────────────────────────

    /// Valid hat values (0..=8) must parse to a direction.
    #[test]
    fn prop_hat_direction_valid_range(value in 0u8..=8u8) {
        prop_assert!(
            MozaHatDirection::from_hid_hat_value(value).is_some(),
            "hat value {value} in 0..=8 must return Some"
        );
    }

    /// Invalid hat values (9..=255) must return None.
    #[test]
    fn prop_hat_direction_invalid_range(value in 9u8..=255u8) {
        prop_assert!(
            MozaHatDirection::from_hid_hat_value(value).is_none(),
            "hat value {value} > 8 must return None"
        );
    }

    // ── ES joystick mode parsing ─────────────────────────────────────────────

    /// Valid config values (0, 1) must parse to a joystick mode.
    #[test]
    fn prop_joystick_mode_valid_range(value in 0u8..=1u8) {
        prop_assert!(
            MozaEsJoystickMode::from_config_value(value).is_some(),
            "config value {value} in 0..=1 must return Some"
        );
    }

    /// Invalid config values (2..=255) must return None.
    #[test]
    fn prop_joystick_mode_invalid_range(value in 2u8..=255u8) {
        prop_assert!(
            MozaEsJoystickMode::from_config_value(value).is_none(),
            "config value {value} > 1 must return None"
        );
    }

    // ── Pedal normalization ──────────────────────────────────────────────────

    /// Normalized pedal axes must be in [0.0, 1.0] for all u16 inputs.
    #[test]
    fn prop_pedal_normalize_bounds(
        throttle: u16,
        brake: u16,
        clutch: u16,
        handbrake: u16,
    ) {
        let raw = MozaPedalAxesRaw {
            throttle,
            brake,
            clutch: Some(clutch),
            handbrake: Some(handbrake),
        };
        let norm = raw.normalize();

        prop_assert!(
            (0.0..=1.0).contains(&norm.throttle),
            "throttle {throttle} → {} out of [0.0, 1.0]", norm.throttle
        );
        prop_assert!(
            (0.0..=1.0).contains(&norm.brake),
            "brake {brake} → {} out of [0.0, 1.0]", norm.brake
        );
        if let Some(c) = norm.clutch {
            prop_assert!(
                (0.0..=1.0).contains(&c),
                "clutch {clutch} → {c} out of [0.0, 1.0]"
            );
        }
        if let Some(h) = norm.handbrake {
            prop_assert!(
                (0.0..=1.0).contains(&h),
                "handbrake {handbrake} → {h} out of [0.0, 1.0]"
            );
        }
    }

    /// `None` pedal axes are preserved through normalization.
    #[test]
    fn prop_pedal_normalize_none_preserved(throttle: u16, brake: u16) {
        let raw = MozaPedalAxesRaw {
            throttle,
            brake,
            clutch: None,
            handbrake: None,
        };
        let norm = raw.normalize();
        prop_assert!(norm.clutch.is_none(), "None clutch must remain None");
        prop_assert!(norm.handbrake.is_none(), "None handbrake must remain None");
    }

    // ── Model classification ─────────────────────────────────────────────────

    /// Unrecognised PIDs (non-wheelbase, non-SRP) must resolve to `MozaModel::Unknown`.
    #[test]
    fn prop_unrecognised_pid_is_unknown_model(pid in any::<u16>()) {
        if !is_wheelbase_product(pid) && pid != product_ids::SR_P_PEDALS {
            prop_assert_eq!(
                MozaModel::from_pid(pid),
                MozaModel::Unknown,
                "non-wheelbase, non-SRP PID 0x{:04X} must be Unknown", pid
            );
        }
    }

    // ── Device identity ──────────────────────────────────────────────────────

    /// `identify_device` always echoes back the input PID in the result.
    #[test]
    fn prop_identity_echoes_pid(pid: u16) {
        let identity = identify_device(pid);
        prop_assert_eq!(
            identity.product_id, pid,
            "identify_device must echo back the input PID"
        );
    }

    /// `identify_device` is deterministic across repeated calls.
    #[test]
    fn prop_identify_device_deterministic(pid: u16) {
        let a = identify_device(pid);
        let b = identify_device(pid);
        prop_assert_eq!(a.name, b.name);
        prop_assert_eq!(a.category, b.category);
        prop_assert_eq!(a.topology_hint, b.topology_hint);
        prop_assert_eq!(a.supports_ffb, b.supports_ffb);
    }

    // ── ES compatibility ─────────────────────────────────────────────────────

    /// Wheelbase PIDs never have `NotWheelbase` ES compatibility.
    #[test]
    fn prop_es_compat_wheelbase_never_not_wheelbase(pid: u16) {
        if is_wheelbase_product(pid) {
            let compat = es_compatibility(pid);
            prop_assert_ne!(
                compat,
                MozaEsCompatibility::NotWheelbase,
                "wheelbase PID 0x{:04X} must not have NotWheelbase compatibility", pid
            );
        }
    }

    /// `es_compatibility` is deterministic.
    #[test]
    fn prop_es_compatibility_deterministic(pid: u16) {
        let a = es_compatibility(pid);
        let b = es_compatibility(pid);
        prop_assert_eq!(a, b, "es_compatibility must be deterministic for PID 0x{:04X}", pid);
    }
}

// ── V2 PID derivation pattern ────────────────────────────────────────────────

/// V2 PIDs must equal V1 | 0x0010 for all wheelbase models.
#[test]
fn v2_pids_are_v1_or_0x0010() {
    let pairs = [
        (product_ids::R16_R21_V1, product_ids::R16_R21_V2),
        (product_ids::R9_V1, product_ids::R9_V2),
        (product_ids::R5_V1, product_ids::R5_V2),
        (product_ids::R3_V1, product_ids::R3_V2),
        (product_ids::R12_V1, product_ids::R12_V2),
    ];
    for (v1, v2) in pairs {
        assert_eq!(
            v2,
            v1 | 0x0010,
            "V2 PID 0x{v2:04X} must equal V1 PID 0x{v1:04X} | 0x0010"
        );
    }
}

/// All peripheral PIDs are non-zero.
#[test]
fn peripheral_pids_are_nonzero() {
    let pids = [
        product_ids::SR_P_PEDALS,
        product_ids::HGP_SHIFTER,
        product_ids::SGP_SHIFTER,
        product_ids::HBP_HANDBRAKE,
    ];
    for pid in pids {
        assert_ne!(pid, 0, "peripheral PID 0x{pid:04X} must be non-zero");
    }
}
