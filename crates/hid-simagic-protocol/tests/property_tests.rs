//! Property-based tests for the Simagic HID protocol encoding.
//!
//! Uses proptest with 500 cases to verify invariants on FFB torque encoding,
//! model detection determinism, and EVO vs legacy model discrimination.

use proptest::prelude::*;
use racing_wheel_hid_simagic_protocol::{
    SimagicConstantForceEncoder, CONSTANT_FORCE_REPORT_LEN, identify_device,
    is_wheelbase_product, ids::product_ids,
};
use racing_wheel_hid_simagic_protocol::types::SimagicModel;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    // ── Torque encoding: report ID ────────────────────────────────────────────

    /// Byte 0 must always be the CONSTANT_FORCE report ID (0x11).
    #[test]
    fn prop_report_id_always_0x11(
        torque in -200.0f32..200.0f32,
        max_torque in 0.01f32..50.0f32,
    ) {
        let enc = SimagicConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        prop_assert_eq!(out[0], 0x11, "byte 0 must be CONSTANT_FORCE report ID 0x11");
    }

    // ── Torque encoding: magnitude range ─────────────────────────────────────

    /// The encoded magnitude (bytes 3–4, i16 LE) must stay within ±10000.
    #[test]
    fn prop_magnitude_within_10000(
        torque in -200.0f32..200.0f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let enc = SimagicConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[3], out[4]]);
        prop_assert!(
            raw >= -10_000 && raw <= 10_000,
            "magnitude {} must be within ±10000",
            raw
        );
    }

    // ── Torque encoding: sign preservation ───────────────────────────────────

    /// A positive torque must produce a non-negative raw magnitude.
    #[test]
    fn prop_positive_torque_nonneg_magnitude(
        torque in 0.01f32..200.0f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let enc = SimagicConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[3], out[4]]);
        prop_assert!(
            raw >= 0,
            "positive torque {torque} (max {max_torque}) must give raw >= 0, got {raw}"
        );
    }

    /// A negative torque must produce a non-positive raw magnitude.
    #[test]
    fn prop_negative_torque_nonpos_magnitude(
        torque in -200.0f32..-0.01f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let enc = SimagicConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[3], out[4]]);
        prop_assert!(
            raw <= 0,
            "negative torque {torque} (max {max_torque}) must give raw <= 0, got {raw}"
        );
    }

    // ── Torque encoding: overflow / saturation ────────────────────────────────

    /// Torque well beyond max_torque must saturate at ±10000.
    #[test]
    fn prop_overflow_saturates_at_10000(max_torque in 0.1f32..50.0f32) {
        let enc = SimagicConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

        enc.encode(max_torque * 100.0, &mut out);
        let raw_pos = i16::from_le_bytes([out[3], out[4]]);
        prop_assert_eq!(raw_pos, 10_000, "over-positive must saturate to 10000");

        enc.encode(-max_torque * 100.0, &mut out);
        let raw_neg = i16::from_le_bytes([out[3], out[4]]);
        prop_assert_eq!(raw_neg, -10_000, "over-negative must saturate to -10000");
    }

    /// encode_zero must always produce zero in the magnitude bytes (3–4).
    #[test]
    fn prop_encode_zero_clears_magnitude(max_torque in 0.1f32..50.0f32) {
        let enc = SimagicConstantForceEncoder::new(max_torque);
        let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode_zero(&mut out);
        prop_assert_eq!(out[3], 0, "encode_zero must clear magnitude low byte");
        prop_assert_eq!(out[4], 0, "encode_zero must clear magnitude high byte");
    }

    // ── Model detection: determinism ─────────────────────────────────────────

    /// identify_device must be deterministic: the same PID always returns the same identity.
    #[test]
    fn prop_identify_device_deterministic(pid: u16) {
        let a = identify_device(pid);
        let b = identify_device(pid);
        prop_assert_eq!(a.name, b.name, "name must be stable for pid={:#06x}", pid);
        prop_assert_eq!(
            a.category, b.category,
            "category must be stable for pid={:#06x}", pid
        );
        prop_assert_eq!(
            a.supports_ffb, b.supports_ffb,
            "supports_ffb must be stable for pid={:#06x}", pid
        );
    }

    /// SimagicModel::from_pid must be deterministic.
    #[test]
    fn prop_model_from_pid_deterministic(pid: u16) {
        let a = SimagicModel::from_pid(pid);
        let b = SimagicModel::from_pid(pid);
        prop_assert_eq!(a, b, "model must be stable for pid={:#06x}", pid);
    }

    // ── Model detection: EVO vs legacy discrimination ─────────────────────────

    /// Known EVO-generation wheelbase PIDs must be detected as wheelbases with FFB.
    #[test]
    fn prop_evo_pids_are_wheelbases(idx in 0usize..6usize) {
        let pids = [
            product_ids::EVO_SPORT,
            product_ids::EVO,
            product_ids::EVO_PRO,
            product_ids::ALPHA_EVO,
            product_ids::NEO,
            product_ids::NEO_MINI,
        ];
        let pid = pids[idx];
        prop_assert!(
            is_wheelbase_product(pid),
            "EVO/Neo PID {pid:#06x} must be a wheelbase"
        );
        let identity = identify_device(pid);
        prop_assert!(
            identity.supports_ffb,
            "EVO/Neo PID {pid:#06x} must support FFB"
        );
        prop_assert!(
            identity.max_torque_nm.is_some(),
            "EVO/Neo PID {pid:#06x} must have a max torque value"
        );
    }

    /// Non-wheelbase Simagic product PIDs must not be detected as wheelbases.
    #[test]
    fn prop_accessory_pids_not_wheelbases(idx in 0usize..6usize) {
        let pids = [
            product_ids::P1000_PEDALS,
            product_ids::P2000_PEDALS,
            product_ids::P1000A_PEDALS,
            product_ids::SHIFTER_H,
            product_ids::SHIFTER_SEQ,
            product_ids::HANDBRAKE,
        ];
        let pid = pids[idx];
        prop_assert!(
            !is_wheelbase_product(pid),
            "accessory PID {pid:#06x} must not be a wheelbase"
        );
        let identity = identify_device(pid);
        prop_assert!(
            !identity.supports_ffb,
            "accessory PID {pid:#06x} must not support FFB"
        );
    }
}

/// The legacy Simagic PID (shared by Alpha/M10/Alpha Ultimate) must not be
/// classified as an EVO-generation wheelbase by this crate.
#[test]
fn test_legacy_pid_not_evo_wheelbase() -> Result<(), Box<dyn std::error::Error>> {
    // 0x0522 = SIMAGIC_LEGACY_PID used by Alpha, Alpha Mini, M10, Alpha Ultimate.
    let legacy_pid = racing_wheel_hid_simagic_protocol::ids::SIMAGIC_LEGACY_PID;
    assert!(
        !is_wheelbase_product(legacy_pid),
        "legacy PID {legacy_pid:#06x} must not be classified as an EVO wheelbase"
    );
    let identity = identify_device(legacy_pid);
    assert!(
        !identity.supports_ffb,
        "legacy PID must not report FFB support via EVO protocol"
    );
    Ok(())
}
