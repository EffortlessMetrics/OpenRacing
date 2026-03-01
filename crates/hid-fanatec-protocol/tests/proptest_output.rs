//! Property-based tests for Fanatec output report encoding.
//!
//! Uses proptest with 500 cases to verify correctness properties of the
//! constant-force FFB encoder independent of specific numeric values.

use proptest::prelude::*;
use racing_wheel_hid_fanatec_protocol::{CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Sign of the input torque must be preserved in the encoded raw i16 value.
    #[test]
    fn prop_sign_preserved(
        torque in -50.0f32..=50.0f32,
        max_torque in 0.1f32..=50.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(torque, 0, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        if torque > 0.01 {
            prop_assert!(raw > 0,
                "positive torque {torque} (max {max_torque}) encoded as non-positive {raw}");
        } else if torque < -0.01 {
            prop_assert!(raw < 0,
                "negative torque {torque} (max {max_torque}) encoded as non-negative {raw}");
        }
    }

    /// Encoded report length must always equal CONSTANT_FORCE_REPORT_LEN (8).
    #[test]
    fn prop_report_length(
        torque in -1000.0f32..=1000.0f32,
        max_torque in 0.0f32..=100.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let len = encoder.encode(torque, 0, &mut out);
        prop_assert_eq!(len, CONSTANT_FORCE_REPORT_LEN,
            "encode() must return CONSTANT_FORCE_REPORT_LEN={}, got {}", CONSTANT_FORCE_REPORT_LEN, len);
    }

    /// Report byte 0 must always be the FFB output report ID (0x01) and
    /// byte 1 must always be the CONSTANT_FORCE command (0x01).
    #[test]
    fn prop_report_header(
        torque in -1000.0f32..=1000.0f32,
        max_torque in 0.0f32..=100.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(torque, 0, &mut out);
        prop_assert_eq!(out[0], 0x01, "byte 0 must be FFB report ID 0x01");
        prop_assert_eq!(out[1], 0x01, "byte 1 must be CONSTANT_FORCE command 0x01");
    }

    /// Torque within ±max_torque must round-trip through the i16 encoding
    /// with at most (max_torque / 32767) Nm of error (1-LSB tolerance).
    #[test]
    fn prop_round_trip_accuracy(
        torque in -50.0f32..=50.0f32,
        max_torque in 0.1f32..=50.0f32,
    ) {
        // Only test torques within the valid range.
        let clamped = torque.clamp(-max_torque, max_torque);
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(clamped, 0, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        // Reconstruct: raw / i16::MAX * max_torque (positive side).
        let decoded = if raw >= 0 {
            raw as f32 / i16::MAX as f32 * max_torque
        } else {
            raw as f32 / (-(i16::MIN as f32)) * max_torque
        };
        let tolerance = max_torque / i16::MAX as f32 + 1e-4;
        let error = (clamped - decoded).abs();
        prop_assert!(
            error <= tolerance,
            "torque {clamped} round-trips as {decoded} (error {error} > tolerance {tolerance})"
        );
    }

    /// Larger absolute torque values must produce larger absolute raw values
    /// (monotonicity), within the in-range region.
    #[test]
    fn prop_monotone_positive(
        t1 in 0.0f32..=50.0f32,
        t2 in 0.0f32..=50.0f32,
        max_torque in 0.1f32..=50.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out1 = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let mut out2 = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(t1.min(max_torque), 0, &mut out1);
        encoder.encode(t2.min(max_torque), 0, &mut out2);
        let r1 = i16::from_le_bytes([out1[2], out1[3]]);
        let r2 = i16::from_le_bytes([out2[2], out2[3]]);
        if t1 < t2 - 0.01 {
            prop_assert!(
                r1 <= r2,
                "t1={t1} → {r1} should be ≤ t2={t2} → {r2} (max_torque={max_torque})"
            );
        }
    }

    /// Positive and negative torques of equal magnitude must produce raw values
    /// that are mirror images (|pos_raw| ≈ |neg_raw|, within 1 LSB).
    #[test]
    fn prop_sign_symmetry(
        torque in 0.01f32..=50.0f32,
        max_torque in 0.1f32..=50.0f32,
    ) {
        let clamped = torque.min(max_torque);
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut pos_out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let mut neg_out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(clamped, 0, &mut pos_out);
        encoder.encode(-clamped, 0, &mut neg_out);
        let pos_raw = i16::from_le_bytes([pos_out[2], pos_out[3]]);
        let neg_raw = i16::from_le_bytes([neg_out[2], neg_out[3]]);
        let diff = (pos_raw as i32 + neg_raw as i32).unsigned_abs();
        prop_assert!(
            diff <= 1,
            "pos_raw={pos_raw} and neg_raw={neg_raw} should be symmetric (diff={diff})"
        );
    }

    /// Reserved bytes 4–7 must always be zero.
    #[test]
    fn prop_reserved_bytes_zero(
        torque in -1000.0f32..=1000.0f32,
        max_torque in 0.0f32..=100.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(torque, 0, &mut out);
        prop_assert_eq!(out[4], 0x00, "reserved byte 4 must be zero");
        prop_assert_eq!(out[5], 0x00, "reserved byte 5 must be zero");
        prop_assert_eq!(out[6], 0x00, "reserved byte 6 must be zero");
        prop_assert_eq!(out[7], 0x00, "reserved byte 7 must be zero");
    }
}

// ── Kernel-verified protocol property tests ──────────────────────────────

use racing_wheel_hid_fanatec_protocol::{
    FanatecModel, MAX_ROTATION_DEGREES, MIN_ROTATION_DEGREES, build_kernel_range_sequence,
    fix_report_values,
};

/// Strategy that produces every `FanatecModel` variant uniformly.
fn arb_fanatec_model() -> impl Strategy<Value = FanatecModel> {
    prop_oneof![
        Just(FanatecModel::Dd1),
        Just(FanatecModel::Dd2),
        Just(FanatecModel::CslElite),
        Just(FanatecModel::CslDd),
        Just(FanatecModel::GtDdPro),
        Just(FanatecModel::ClubSportDd),
        Just(FanatecModel::ClubSportV2),
        Just(FanatecModel::ClubSportV25),
        Just(FanatecModel::CsrElite),
        Just(FanatecModel::Unknown),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    // ── build_kernel_range_sequence: always 3 reports of 7 bytes ─────────

    /// The kernel range sequence must always produce exactly 3 reports,
    /// each 7 bytes long.
    #[test]
    fn prop_kernel_range_sequence_shape(degrees in 0u16..=4000u16) {
        let seq = build_kernel_range_sequence(degrees);
        prop_assert_eq!(seq.len(), 3, "must produce exactly 3 reports");
        for report in &seq {
            prop_assert_eq!(report.len(), 7, "each report must be 7 bytes");
        }
    }

    /// The 3rd report of the kernel range sequence must encode the
    /// clamped degree value as a little-endian u16 at bytes [2,3].
    #[test]
    fn prop_kernel_range_sequence_encodes_degrees(degrees in 0u16..=4000u16) {
        let seq = build_kernel_range_sequence(degrees);
        let clamped = degrees.clamp(MIN_ROTATION_DEGREES, MAX_ROTATION_DEGREES);
        let decoded = u16::from_le_bytes([seq[2][2], seq[2][3]]);
        prop_assert_eq!(decoded, clamped,
            "3rd report bytes [2,3] must encode clamped degrees");
    }

    /// Step 1 must always be the reset command, step 2 the prepare command.
    #[test]
    fn prop_kernel_range_sequence_fixed_steps(degrees in 0u16..=4000u16) {
        let seq = build_kernel_range_sequence(degrees);
        prop_assert_eq!(seq[0], [0xF5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            "step 1 must be the reset command");
        prop_assert_eq!(seq[1], [0xF8, 0x09, 0x01, 0x06, 0x01, 0x00, 0x00],
            "step 2 must be the prepare command");
        prop_assert_eq!(seq[2][0], 0xF8, "step 3 byte 0 must be 0xF8");
        prop_assert_eq!(seq[2][1], 0x81, "step 3 byte 1 must be 0x81");
    }

    // ── fix_report_values: idempotence ───────────────────────────────────

    /// Applying fix_report_values twice must give the same result as once.
    #[test]
    fn prop_fix_report_values_idempotent(
        v0 in 0i16..=255i16,
        v1 in 0i16..=255i16,
        v2 in 0i16..=255i16,
        v3 in 0i16..=255i16,
        v4 in 0i16..=255i16,
        v5 in 0i16..=255i16,
        v6 in 0i16..=255i16,
    ) {
        let mut once = [v0, v1, v2, v3, v4, v5, v6];
        fix_report_values(&mut once);
        let mut twice = once;
        fix_report_values(&mut twice);
        prop_assert_eq!(once, twice,
            "fix_report_values must be idempotent");
    }

    /// fix_report_values must only modify values >= 0x80 by subtracting 0x100.
    #[test]
    fn prop_fix_report_values_only_modifies_high(
        v0 in 0i16..=255i16,
        v1 in 0i16..=255i16,
        v2 in 0i16..=255i16,
        v3 in 0i16..=255i16,
        v4 in 0i16..=255i16,
        v5 in 0i16..=255i16,
        v6 in 0i16..=255i16,
    ) {
        let original = [v0, v1, v2, v3, v4, v5, v6];
        let mut fixed = original;
        fix_report_values(&mut fixed);
        for (i, (&orig, &fix)) in original.iter().zip(fixed.iter()).enumerate() {
            if orig < 0x80 {
                prop_assert!(fix == orig,
                    "value 0x{:04X} at index {} is < 0x80 and must be unchanged, got 0x{:04X}",
                    orig, i, fix);
            } else {
                prop_assert!(fix == orig - 0x100,
                    "value 0x{:04X} at index {} is >= 0x80 and must become {}, got {}",
                    orig, i, orig - 0x100, fix);
            }
        }
    }

    // ── FanatecModel: max_rotation_degrees bounds ────────────────────────

    /// max_rotation_degrees must always be > 0 and <= 2520 for any model.
    #[test]
    fn prop_max_rotation_degrees_bounds(model in arb_fanatec_model()) {
        let deg = model.max_rotation_degrees();
        prop_assert!(deg > 0, "max_rotation_degrees must be > 0, got {deg}");
        prop_assert!(deg <= 2520, "max_rotation_degrees must be <= 2520, got {deg}");
    }

    // ── FanatecModel: needs_sign_fix ─────────────────────────────────────

    /// needs_sign_fix must return false only for CsrElite and Unknown.
    #[test]
    fn prop_needs_sign_fix_csr_elite_is_false(model in arb_fanatec_model()) {
        let needs = model.needs_sign_fix();
        if model == FanatecModel::CsrElite || model == FanatecModel::Unknown {
            prop_assert!(!needs,
                "needs_sign_fix must be false for {:?}", model);
        } else {
            prop_assert!(needs,
                "needs_sign_fix must be true for {:?}", model);
        }
    }

    // ── FanatecModel: is_highres ─────────────────────────────────────────

    /// is_highres must be true only for DD1, DD2, CslDd, GtDdPro, ClubSportDd.
    #[test]
    fn prop_is_highres_dd_only(model in arb_fanatec_model()) {
        let hr = model.is_highres();
        let expected = matches!(
            model,
            FanatecModel::Dd1
                | FanatecModel::Dd2
                | FanatecModel::CslDd
                | FanatecModel::GtDdPro
                | FanatecModel::ClubSportDd
        );
        prop_assert_eq!(hr, expected,
            "is_highres for {:?} should be {}, got {}", model, expected, hr);
    }
}
