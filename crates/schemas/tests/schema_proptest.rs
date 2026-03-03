#![allow(clippy::redundant_closure)]
//! Property-based tests for schema types: domain value object roundtrips,
//! config type serde roundtrips, numeric conversions, and error invariants.

use proptest::prelude::*;
use racing_wheel_schemas::config::{
    BumpstopConfig, CurvePoint as CfgCurvePoint, FilterConfig, HandsOffConfig, NotchFilter,
};
use racing_wheel_schemas::domain::{
    CurvePoint, Degrees, DeviceId, DomainError, FrequencyHz, Gain, ProfileId, TorqueNm,
    validate_curve_monotonic,
};
use racing_wheel_schemas::telemetry::{TelemetryFlags, TelemetryValue};

// ── Strategies ──────────────────────────────────────────────────────────────

fn valid_torque() -> impl Strategy<Value = f32> {
    0.0f32..=TorqueNm::MAX_TORQUE
}

fn valid_dor() -> impl Strategy<Value = f32> {
    Degrees::MIN_DOR..=Degrees::MAX_DOR
}

fn valid_gain() -> impl Strategy<Value = f32> {
    0.0f32..=1.0
}

fn valid_frequency() -> impl Strategy<Value = f32> {
    0.001f32..=100_000.0
}

fn valid_device_id() -> impl Strategy<Value = String> {
    "[a-z0-9][a-z0-9_-]{0,19}".prop_map(|s| s)
}

fn valid_profile_id() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9._-]{0,19}".prop_map(|s| s)
}

fn telemetry_value_strategy() -> impl Strategy<Value = TelemetryValue> {
    prop_oneof![
        (-1e6f32..1e6f32)
            .prop_filter("finite", |v| v.is_finite())
            .prop_map(TelemetryValue::Float),
        any::<i32>().prop_map(TelemetryValue::Integer),
        any::<bool>().prop_map(TelemetryValue::Boolean),
        "[a-zA-Z0-9_]{0,20}".prop_map(|s| TelemetryValue::String(s)),
    ]
}

// ── Domain roundtrip tests ──────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    // === TorqueNm: new → value roundtrip ===

    #[test]
    fn prop_torque_nm_roundtrip(v in valid_torque()) {
        let t = TorqueNm::new(v);
        prop_assert!(t.is_ok(), "valid torque {} should succeed", v);
        let t = t.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert!((t.value() - v).abs() < f32::EPSILON);
    }

    // === TorqueNm: cNm conversion roundtrip ===

    #[test]
    fn prop_torque_cnm_roundtrip(v in valid_torque()) {
        let t = TorqueNm::new(v)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let cnm = t.to_cnm();
        let back = TorqueNm::from_cnm(cnm)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        // Rounding tolerance: 0.01 Nm (1 cNm)
        prop_assert!(
            (t.value() - back.value()).abs() < 0.01,
            "cNm roundtrip: {} -> {} -> {}", t.value(), cnm, back.value()
        );
    }

    // === TorqueNm: addition stays within bounds ===

    #[test]
    fn prop_torque_add_bounded(
        a in 0.0f32..=TorqueNm::MAX_TORQUE,
        b in 0.0f32..=TorqueNm::MAX_TORQUE,
    ) {
        let ta = TorqueNm::new(a).map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let tb = TorqueNm::new(b).map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let sum = ta + tb;
        prop_assert!(sum.value() >= 0.0, "sum must be non-negative");
        prop_assert!(sum.value() <= TorqueNm::MAX_TORQUE, "sum must be <= MAX_TORQUE");
    }

    // === TorqueNm: subtraction stays within bounds ===

    #[test]
    fn prop_torque_sub_bounded(
        a in 0.0f32..=TorqueNm::MAX_TORQUE,
        b in 0.0f32..=TorqueNm::MAX_TORQUE,
    ) {
        let ta = TorqueNm::new(a).map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let tb = TorqueNm::new(b).map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let diff = ta - tb;
        prop_assert!(diff.value() >= 0.0, "diff must be non-negative");
        prop_assert!(diff.value() <= TorqueNm::MAX_TORQUE);
    }

    // === TorqueNm: multiplication stays within bounds ===

    #[test]
    fn prop_torque_mul_bounded(
        v in 0.0f32..=TorqueNm::MAX_TORQUE,
        factor in -10.0f32..=10.0,
    ) {
        let t = TorqueNm::new(v).map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let scaled = t * factor;
        prop_assert!(scaled.value() >= 0.0, "scaled must be non-negative: {}", scaled.value());
        prop_assert!(scaled.value() <= TorqueNm::MAX_TORQUE, "scaled exceeds MAX_TORQUE");
    }

    // === TorqueNm: rejects non-finite ===

    #[test]
    fn prop_torque_rejects_non_finite(_seed in 0u32..256) {
        prop_assert!(TorqueNm::new(f32::NAN).is_err());
        prop_assert!(TorqueNm::new(f32::INFINITY).is_err());
        prop_assert!(TorqueNm::new(f32::NEG_INFINITY).is_err());
    }

    // === Degrees: DOR roundtrip ===

    #[test]
    fn prop_degrees_dor_roundtrip(v in valid_dor()) {
        let d = Degrees::new_dor(v)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert!((d.value() - v).abs() < f32::EPSILON);
    }

    // === Degrees: millidegrees roundtrip ===

    #[test]
    fn prop_degrees_millidegrees_roundtrip(v in -1000.0f32..=1000.0) {
        let d = Degrees::new_angle(v)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let mdeg = d.to_millidegrees();
        let back = Degrees::from_millidegrees(mdeg);
        prop_assert!(
            (d.value() - back.value()).abs() < 0.001,
            "mdeg roundtrip: {} -> {} -> {}", d.value(), mdeg, back.value()
        );
    }

    // === Degrees: normalize keeps [-180, 180] ===

    #[test]
    fn prop_degrees_normalize_range(v in -3600.0f32..=3600.0) {
        let d = Degrees::new_angle(v)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let n = d.normalize();
        prop_assert!(
            n.value() >= -180.0 && n.value() <= 180.0,
            "normalized {} out of [-180, 180]", n.value()
        );
    }

    // === Gain: value roundtrip ===

    #[test]
    fn prop_gain_roundtrip(v in valid_gain()) {
        let g = Gain::new(v).map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert!((g.value() - v).abs() < f32::EPSILON);
    }

    // === Gain: mul preserves sign ===

    #[test]
    fn prop_gain_mul_preserves_sign(
        g in valid_gain(),
        val in -1000.0f32..=1000.0,
    ) {
        let gain = Gain::new(g).map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let result = gain * val;
        if val > 0.0 && g > 0.0 {
            prop_assert!(result >= 0.0);
        } else if val < 0.0 && g > 0.0 {
            prop_assert!(result <= 0.0);
        }
    }

    // === Gain: rejects out-of-range ===

    #[test]
    fn prop_gain_rejects_out_of_range(v in 1.01f32..=100.0) {
        prop_assert!(Gain::new(v).is_err());
        prop_assert!(Gain::new(-v).is_err());
    }

    // === FrequencyHz: roundtrip ===

    #[test]
    fn prop_frequency_roundtrip(v in valid_frequency()) {
        let f = FrequencyHz::new(v)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert!((f.value() - v).abs() < f32::EPSILON);
    }

    // === FrequencyHz: rejects zero and negative ===

    #[test]
    fn prop_frequency_rejects_non_positive(v in -1000.0f32..=0.0) {
        prop_assert!(FrequencyHz::new(v).is_err());
    }

    // === CurvePoint: roundtrip ===

    #[test]
    fn prop_curve_point_roundtrip(
        input in 0.0f32..=1.0,
        output in 0.0f32..=1.0,
    ) {
        let cp = CurvePoint::new(input, output)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert!((cp.input - input).abs() < f32::EPSILON);
        prop_assert!((cp.output - output).abs() < f32::EPSILON);
    }

    // === CurvePoint: rejects out-of-range ===

    #[test]
    fn prop_curve_point_rejects_oob(input in 1.01f32..=10.0) {
        prop_assert!(CurvePoint::new(input, 0.5).is_err());
        prop_assert!(CurvePoint::new(0.5, input).is_err());
    }

    // === validate_curve_monotonic: sorted inputs pass ===

    #[test]
    fn prop_monotonic_sorted_inputs_pass(
        mid in 0.01f32..=0.99,
    ) {
        let points_result = (|| -> Result<Vec<CurvePoint>, DomainError> {
            Ok(vec![
                CurvePoint::new(0.0, 0.0)?,
                CurvePoint::new(mid, mid)?,
                CurvePoint::new(1.0, 1.0)?,
            ])
        })();
        if let Ok(points) = points_result {
            prop_assert!(validate_curve_monotonic(&points).is_ok());
        }
    }

    // === DeviceId: parse roundtrip (normalized) ===

    #[test]
    fn prop_device_id_normalized(s in valid_device_id()) {
        let id: Result<DeviceId, _> = s.parse();
        prop_assert!(id.is_ok(), "valid device id '{}' should parse", s);
        let id = id.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        // Should be lowercase
        prop_assert_eq!(id.as_str(), s.trim().to_lowercase());
    }

    // === DeviceId: serde roundtrip ===

    #[test]
    fn prop_device_id_serde_roundtrip(s in valid_device_id()) {
        let id: DeviceId = s.parse()
            .map_err(|e: DomainError| TestCaseError::Fail(format!("{e}").into()))?;
        let json = serde_json::to_string(&id)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: DeviceId = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(id.as_str(), decoded.as_str());
    }

    // === ProfileId: parse roundtrip (normalized) ===

    #[test]
    fn prop_profile_id_normalized(s in valid_profile_id()) {
        let id: Result<ProfileId, _> = s.parse();
        prop_assert!(id.is_ok(), "valid profile id '{}' should parse", s);
        let id = id.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(id.as_str(), s.trim().to_lowercase());
    }

    // === ProfileId: serde roundtrip ===

    #[test]
    fn prop_profile_id_serde_roundtrip(s in valid_profile_id()) {
        let id: ProfileId = s.parse()
            .map_err(|e: DomainError| TestCaseError::Fail(format!("{e}").into()))?;
        let json = serde_json::to_string(&id)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: ProfileId = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(id.as_str(), decoded.as_str());
    }

    // === TelemetryValue: serde roundtrip ===

    #[test]
    fn prop_telemetry_value_serde_roundtrip(val in telemetry_value_strategy()) {
        let json = serde_json::to_string(&val)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: TelemetryValue = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(&val, &decoded);
    }

    // === TelemetryFlags: serde roundtrip ===

    #[test]
    fn prop_telemetry_flags_serde_roundtrip(
        yellow in any::<bool>(),
        blue in any::<bool>(),
        green in any::<bool>(),
        pit_limiter in any::<bool>(),
    ) {
        let flags = TelemetryFlags {
            yellow_flag: yellow,
            blue_flag: blue,
            green_flag: green,
            pit_limiter,
            ..TelemetryFlags::default()
        };
        let json = serde_json::to_string(&flags)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: TelemetryFlags = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(flags.yellow_flag, decoded.yellow_flag);
        prop_assert_eq!(flags.blue_flag, decoded.blue_flag);
        prop_assert_eq!(flags.green_flag, decoded.green_flag);
        prop_assert_eq!(flags.pit_limiter, decoded.pit_limiter);
    }

    // === Config FilterConfig: serde roundtrip ===

    #[test]
    fn prop_config_filter_config_serde_roundtrip(
        reconstruction in 0u8..=8,
        friction in 0.0f32..=1.0,
        damper in 0.0f32..=1.0,
        inertia in 0.0f32..=1.0,
        slew_rate in 0.0f32..=1.0,
    ) {
        let config = FilterConfig {
            reconstruction,
            friction,
            damper,
            inertia,
            bumpstop: BumpstopConfig::default(),
            hands_off: HandsOffConfig::default(),
            torque_cap: None,
            notch_filters: vec![],
            slew_rate,
            curve_points: vec![
                CfgCurvePoint { input: 0.0, output: 0.0 },
                CfgCurvePoint { input: 1.0, output: 1.0 },
            ],
        };
        let json = serde_json::to_string(&config)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: FilterConfig = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(config.reconstruction, decoded.reconstruction);
        prop_assert!((config.friction - decoded.friction).abs() < f32::EPSILON);
        prop_assert!((config.damper - decoded.damper).abs() < f32::EPSILON);
        prop_assert!((config.inertia - decoded.inertia).abs() < f32::EPSILON);
        prop_assert!((config.slew_rate - decoded.slew_rate).abs() < f32::EPSILON);
    }

    // === Config NotchFilter: serde roundtrip ===

    #[test]
    fn prop_config_notch_filter_serde_roundtrip(
        hz in 0.1f32..=500.0,
        q in 0.1f32..=20.0,
        gain_db in -60.0f32..=0.0,
    ) {
        let nf = NotchFilter { hz, q, gain_db };
        let json = serde_json::to_string(&nf)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: NotchFilter = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert!((nf.hz - decoded.hz).abs() < f32::EPSILON);
        prop_assert!((nf.q - decoded.q).abs() < f32::EPSILON);
        prop_assert!((nf.gain_db - decoded.gain_db).abs() < f32::EPSILON);
    }

    // === Config BumpstopConfig: serde roundtrip ===

    #[test]
    fn prop_config_bumpstop_serde_roundtrip(
        enabled in any::<bool>(),
        strength in 0.0f32..=1.0,
    ) {
        let bs = BumpstopConfig { enabled, strength };
        let json = serde_json::to_string(&bs)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: BumpstopConfig = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(bs.enabled, decoded.enabled);
        prop_assert!((bs.strength - decoded.strength).abs() < f32::EPSILON);
    }

    // === Config HandsOffConfig: serde roundtrip ===

    #[test]
    fn prop_config_handsoff_serde_roundtrip(
        enabled in any::<bool>(),
        sensitivity in 0.0f32..=1.0,
    ) {
        let ho = HandsOffConfig { enabled, sensitivity };
        let json = serde_json::to_string(&ho)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: HandsOffConfig = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(ho.enabled, decoded.enabled);
        prop_assert!((ho.sensitivity - decoded.sensitivity).abs() < f32::EPSILON);
    }

    // === DomainError: display contains context ===

    #[test]
    fn prop_domain_error_display_torque(v in -100.0f32..=100.0) {
        let err = DomainError::InvalidTorque(v, TorqueNm::MAX_TORQUE);
        let display = format!("{err}");
        prop_assert!(
            display.contains(&format!("{}", v)),
            "error display '{}' should contain value '{}'", display, v
        );
    }

    // === DomainError: gain error shows value ===

    #[test]
    fn prop_domain_error_display_gain(v in -10.0f32..=10.0) {
        let err = DomainError::InvalidGain(v);
        let display = format!("{err}");
        prop_assert!(
            display.contains(&format!("{}", v)),
            "error display '{}' should contain value", display
        );
    }
}
