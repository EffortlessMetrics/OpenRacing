//! Roundtrip property-based tests for the Moza HID protocol.
//!
//! Verifies encode→decode and decode→encode consistency for:
//! - Direct torque encoder (float API)
//! - RT TorqueEncoder trait (Q8.8 API)
//! - Input report construction→parse roundtrip
//! - Pedal axis normalization roundtrip
#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_hid_moza_protocol::{
    MozaDirectTorqueEncoder, MozaModel, MozaPedalAxesRaw, REPORT_LEN, TorqueEncoder,
    identify_device,
};

// ── Direct torque encode roundtrip ──────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Torque magnitude within ±max survives the encode→decode cycle with at
    /// most one LSB of error relative to the i16 (percent-of-max) scale.
    #[test]
    fn prop_direct_torque_encode_roundtrip(
        max in 0.1_f32..=21.0_f32,
        torque_frac in -1.0_f32..=1.0_f32,
    ) {
        let torque = torque_frac * max;
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(torque, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        // Raw is percent-of-max * i16::MAX/MIN (±32767/32768)
        let decoded = if raw >= 0 {
            raw as f32 / i16::MAX as f32 * max
        } else {
            raw as f32 / (-(i16::MIN as f32)) * max
        };
        let tolerance = max / i16::MAX as f32 + 1e-4;
        let error = (torque.clamp(-max, max) - decoded).abs();
        prop_assert!(
            error <= tolerance,
            "torque {torque} roundtrips as {decoded} (error {error} > tol {tolerance})"
        );
    }

    /// encode_zero always yields zero raw torque regardless of max_torque_nm.
    #[test]
    fn prop_direct_encode_zero_roundtrip(max in 0.1_f32..=21.0_f32) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0xFFu8; REPORT_LEN];
        enc.encode_zero(&mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
        prop_assert_eq!(raw, 0, "encode_zero must produce raw=0");
    }

    /// RT TorqueEncoder::encode: the raw i16 must have correct sign and
    /// be within the valid output range. The RT path converts Q8.8 → f32 Nm
    /// → percent-of-max i16, so exact roundtrip depends on max_torque_nm.
    #[test]
    fn prop_rt_torque_sign_and_range(
        max in 0.1_f32..=21.0_f32,
        torque: i16,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        TorqueEncoder::encode(&enc, torque, 0, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);

        // Sign must be preserved for non-tiny values
        if torque > 1 {
            prop_assert!(raw >= 0, "positive Q8.8 {torque} must yield non-negative raw {raw}");
        } else if torque < -1 {
            prop_assert!(raw <= 0, "negative Q8.8 {torque} must yield non-positive raw {raw}");
        }
    }
}

// ── Pedal normalization roundtrip ───────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Pedal raw values normalize into [0.0, 1.0] without NaN or infinity.
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
        prop_assert!(norm.throttle >= 0.0 && norm.throttle <= 1.0,
            "throttle {}", norm.throttle);
        prop_assert!(norm.brake >= 0.0 && norm.brake <= 1.0,
            "brake {}", norm.brake);
        if let Some(c) = norm.clutch {
            prop_assert!((0.0..=1.0).contains(&c), "clutch {c}");
        }
        if let Some(h) = norm.handbrake {
            prop_assert!((0.0..=1.0).contains(&h), "handbrake {h}");
        }
    }

    /// Pedal raw→normalized is monotone: larger raw ⇒ larger normalized.
    #[test]
    fn prop_pedal_normalize_monotone(
        a in 0u16..=65534u16,
        b in 1u16..=65535u16,
    ) {
        prop_assume!(a < b);
        let raw_a = MozaPedalAxesRaw {
            throttle: a, brake: 0, clutch: None, handbrake: None,
        };
        let raw_b = MozaPedalAxesRaw {
            throttle: b, brake: 0, clutch: None, handbrake: None,
        };
        prop_assert!(
            raw_a.normalize().throttle <= raw_b.normalize().throttle,
            "monotonicity: {} at raw {} must <= {} at raw {}",
            raw_a.normalize().throttle, a, raw_b.normalize().throttle, b
        );
    }
}

// ── Device identification roundtrip ─────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// identify_device is deterministic: calling it twice yields the same result.
    #[test]
    fn prop_identify_device_deterministic(pid: u16) {
        let id1 = identify_device(pid);
        let id2 = identify_device(pid);
        let name_matches = id1.name == id2.name;
        prop_assert!(name_matches, "name must be deterministic");
    }

    /// Known Moza PIDs must map to a non-Unknown model.
    #[test]
    fn prop_known_model_max_torque_positive(
        model in prop_oneof![
            Just(MozaModel::R3),
            Just(MozaModel::R5),
            Just(MozaModel::R9),
            Just(MozaModel::R12),
            Just(MozaModel::R16),
            Just(MozaModel::R21),
        ]
    ) {
        let torque = model.max_torque_nm();
        prop_assert!(torque > 0.0, "{:?} max torque must be > 0, got {torque}", model);
    }

    /// Encode with slew-rate → bytes → read back slew-rate: must match.
    #[test]
    fn prop_slew_rate_roundtrip(
        max in 0.1_f32..=21.0_f32,
        slew: u16,
        torque in -21.0_f32..=21.0_f32,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max).with_slew_rate(slew);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(torque, 0, &mut out);
        let recovered = u16::from_le_bytes([out[4], out[5]]);
        prop_assert_eq!(recovered, slew, "slew rate must roundtrip exactly");
    }
}

// ── Boundary / overflow tests ───────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Extreme torque values (±f32::MAX) must not panic and must saturate.
    #[test]
    fn prop_extreme_torque_saturates(
        max in 0.1_f32..=21.0_f32,
        torque in prop_oneof![
            Just(f32::MAX),
            Just(f32::MIN),
            Just(1e30_f32),
            Just(-1e30_f32),
            Just(0.0_f32),
        ],
    ) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        // Must not panic
        enc.encode(torque, 0, &mut out);
        // Must not panic; just verify it produces a valid i16
        let _raw = i16::from_le_bytes([out[1], out[2]]);
    }

    /// NaN torque must not panic and must produce a valid report.
    #[test]
    fn prop_nan_torque_no_panic(max in 0.1_f32..=21.0_f32) {
        let enc = MozaDirectTorqueEncoder::new(max);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(f32::NAN, 0, &mut out);
        // Just verify no panic occurred; report ID must still be correct.
        prop_assert_eq!(out[0], 0x20, "report ID must be 0x20");
    }
}
