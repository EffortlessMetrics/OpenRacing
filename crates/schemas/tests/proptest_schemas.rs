//! Property-based tests for the racing-wheel-schemas crate.
//!
//! These tests verify critical schema invariants:
//! - Config serialization roundtrips for arbitrary valid configs
//! - Profile names can contain any valid identifier characters
//! - Device IDs are always valid format after construction
//! - Version comparison is transitive and antisymmetric

use proptest::prelude::*;
use racing_wheel_schemas::config::{
    BaseConfig, BumpstopConfig, CurvePoint, FilterConfig, HandsOffConfig, ProfileSchema,
    ProfileScope,
};
use racing_wheel_schemas::domain::{
    self, Degrees, DeviceId, FrequencyHz, Gain, ProfileId, TorqueNm,
};
use racing_wheel_schemas::migration::SchemaVersion;

/// proptest config with 200 cases per test
fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 200,
        ..Default::default()
    }
}

/// Strategy producing a valid ProfileSchema for roundtrip tests
fn arb_profile_schema() -> impl Strategy<Value = ProfileSchema> {
    (arb_profile_scope(), arb_base_config()).prop_map(|(scope, base)| ProfileSchema {
        schema: "wheel.profile/1".to_string(),
        scope,
        base,
        leds: None,
        haptics: None,
        signature: None,
    })
}

fn arb_profile_scope() -> impl Strategy<Value = ProfileScope> {
    (
        prop::option::of("[a-z0-9_-]{1,20}".prop_map(String::from)),
        prop::option::of("[a-z0-9_-]{1,20}".prop_map(String::from)),
        prop::option::of("[a-z0-9_-]{1,20}".prop_map(String::from)),
    )
        .prop_map(|(game, car, track)| ProfileScope { game, car, track })
}

fn arb_base_config() -> impl Strategy<Value = BaseConfig> {
    (
        0.0f32..=1.0,  // ffb_gain
        180u16..=2160, // dor_deg
        0.0f32..=50.0, // torque_cap_nm
        arb_filter_config(),
    )
        .prop_map(|(ffb_gain, dor_deg, torque_cap_nm, filters)| BaseConfig {
            ffb_gain,
            dor_deg,
            torque_cap_nm,
            filters,
        })
}

fn arb_filter_config() -> impl Strategy<Value = FilterConfig> {
    (
        0u8..=8,      // reconstruction
        0.0f32..=1.0, // friction
        0.0f32..=1.0, // damper
        0.0f32..=1.0, // inertia
        0.0f32..=1.0, // slew_rate
    )
        .prop_map(
            |(reconstruction, friction, damper, inertia, slew_rate)| FilterConfig {
                reconstruction,
                friction,
                damper,
                inertia,
                bumpstop: BumpstopConfig::default(),
                hands_off: HandsOffConfig::default(),
                torque_cap: Some(10.0),
                notch_filters: Vec::new(),
                slew_rate,
                curve_points: vec![
                    CurvePoint {
                        input: 0.0,
                        output: 0.0,
                    },
                    CurvePoint {
                        input: 1.0,
                        output: 1.0,
                    },
                ],
            },
        )
}

// ---------------------------------------------------------------------------
// ProfileSchema serialization roundtrip
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// ProfileSchema serializes to JSON and deserializes back identically.
    #[test]
    fn profile_schema_json_roundtrip(profile in arb_profile_schema()) {
        let json = serde_json::to_string(&profile);
        prop_assert!(json.is_ok(), "Serialization failed: {:?}", json.err());
        let json = json.map_err(|e| TestCaseError::fail(format!("{}", e)))?;

        let parsed: Result<ProfileSchema, _> = serde_json::from_str(&json);
        prop_assert!(parsed.is_ok(), "Deserialization failed: {:?}", parsed.err());
        let parsed = parsed.map_err(|e| TestCaseError::fail(format!("{}", e)))?;

        prop_assert_eq!(&*profile.schema, &*parsed.schema);
        prop_assert!((profile.base.ffb_gain - parsed.base.ffb_gain).abs() < f32::EPSILON);
        prop_assert_eq!(profile.base.dor_deg, parsed.base.dor_deg);
    }

    /// ProfileSchema roundtrip preserves scope fields.
    #[test]
    fn profile_schema_scope_roundtrip(profile in arb_profile_schema()) {
        let json = serde_json::to_string(&profile)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        let parsed: ProfileSchema = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;

        prop_assert_eq!(profile.scope.game, parsed.scope.game);
        prop_assert_eq!(profile.scope.car, parsed.scope.car);
        prop_assert_eq!(profile.scope.track, parsed.scope.track);
    }

    /// ProfileSchema roundtrip preserves filter config.
    #[test]
    fn profile_schema_filters_roundtrip(profile in arb_profile_schema()) {
        let json = serde_json::to_string(&profile)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        let parsed: ProfileSchema = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;

        prop_assert_eq!(profile.base.filters.reconstruction, parsed.base.filters.reconstruction);
        prop_assert!((profile.base.filters.friction - parsed.base.filters.friction).abs() < f32::EPSILON);
        prop_assert!((profile.base.filters.damper - parsed.base.filters.damper).abs() < f32::EPSILON);
        prop_assert!((profile.base.filters.inertia - parsed.base.filters.inertia).abs() < f32::EPSILON);
    }
}

// ---------------------------------------------------------------------------
// DeviceId invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// Valid device IDs always parse successfully.
    #[test]
    fn device_id_valid_always_parses(s in "[a-z0-9][a-z0-9_-]{0,29}") {
        let result: Result<DeviceId, _> = s.parse();
        prop_assert!(result.is_ok(), "Failed to parse valid device ID: {}", s);
    }

    /// DeviceId is always lowercased and trimmed.
    #[test]
    fn device_id_normalized(s in "[A-Za-z0-9][A-Za-z0-9_-]{0,19}") {
        let result: Result<DeviceId, _> = s.parse();
        if let Ok(id) = result {
            let id_str = id.as_str();
            prop_assert_eq!(id_str, id_str.to_lowercase(), "DeviceId not lowercase");
            prop_assert_eq!(id_str, id_str.trim(), "DeviceId not trimmed");
        }
    }

    /// Empty strings are rejected as DeviceId.
    #[test]
    fn device_id_empty_rejected(padding in "\\s{0,5}") {
        let result: Result<DeviceId, _> = padding.parse();
        // Whitespace-only or empty strings should be rejected
        if padding.trim().is_empty() {
            prop_assert!(result.is_err(), "Empty/whitespace-only DeviceId accepted");
        }
    }

    /// DeviceId contains only valid characters after construction.
    #[test]
    fn device_id_valid_chars_after_parse(s in "[A-Za-z0-9_-]{1,20}") {
        let result: Result<DeviceId, _> = s.parse();
        if let Ok(id) = result {
            let id_str = id.as_str();
            prop_assert!(
                id_str.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_'),
                "DeviceId contains invalid chars: {}",
                id_str
            );
        }
    }

    /// DeviceId Display roundtrips through parse.
    #[test]
    fn device_id_display_roundtrip(s in "[a-z0-9][a-z0-9_-]{0,19}") {
        let id: DeviceId = s.parse()
            .map_err(|e: domain::DomainError| TestCaseError::fail(format!("{}", e)))?;
        let display = id.to_string();
        let reparsed: DeviceId = display.parse()
            .map_err(|e: domain::DomainError| TestCaseError::fail(format!("{}", e)))?;
        prop_assert_eq!(id, reparsed);
    }
}

// ---------------------------------------------------------------------------
// ProfileId invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// Valid profile IDs always parse successfully.
    #[test]
    fn profile_id_valid_always_parses(s in "[a-z0-9][a-z0-9._-]{0,29}") {
        let result: Result<ProfileId, _> = s.parse();
        prop_assert!(result.is_ok(), "Failed to parse valid profile ID: {}", s);
    }

    /// ProfileId is always lowercased and trimmed.
    #[test]
    fn profile_id_normalized(s in "[A-Za-z0-9][A-Za-z0-9._-]{0,19}") {
        let result: Result<ProfileId, _> = s.parse();
        if let Ok(id) = result {
            let id_str = id.as_str();
            prop_assert_eq!(id_str, id_str.to_lowercase());
            prop_assert_eq!(id_str, id_str.trim());
        }
    }

    /// ProfileId Display roundtrips through parse.
    #[test]
    fn profile_id_display_roundtrip(s in "[a-z0-9][a-z0-9._-]{0,19}") {
        let id: ProfileId = s.parse()
            .map_err(|e: domain::DomainError| TestCaseError::fail(format!("{}", e)))?;
        let display = id.to_string();
        let reparsed: ProfileId = display.parse()
            .map_err(|e: domain::DomainError| TestCaseError::fail(format!("{}", e)))?;
        prop_assert_eq!(id, reparsed);
    }
}

// ---------------------------------------------------------------------------
// Domain value object invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// TorqueNm in valid range always constructs successfully.
    #[test]
    fn torque_valid_range_constructs(v in 0.0f32..=50.0) {
        let result = TorqueNm::new(v);
        prop_assert!(result.is_ok(), "Valid torque {} rejected", v);
    }

    /// TorqueNm roundtrips through to_cnm/from_cnm within 0.01 Nm.
    #[test]
    fn torque_cnm_roundtrip(v in 0.0f32..=50.0) {
        let t = TorqueNm::new(v)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        let cnm = t.to_cnm();
        let back = TorqueNm::from_cnm(cnm);
        prop_assert!(back.is_ok());
        let back = back.map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        prop_assert!((t.value() - back.value()).abs() < 0.01);
    }

    /// Gain in valid range always constructs successfully.
    #[test]
    fn gain_valid_range_constructs(v in 0.0f32..=1.0) {
        let result = Gain::new(v);
        prop_assert!(result.is_ok(), "Valid gain {} rejected", v);
    }

    /// FrequencyHz above zero always constructs successfully.
    #[test]
    fn frequency_valid_range_constructs(v in 0.01f32..=100_000.0) {
        let result = FrequencyHz::new(v);
        prop_assert!(result.is_ok(), "Valid frequency {} rejected", v);
    }

    /// Degrees DOR in valid range always constructs successfully.
    #[test]
    fn degrees_dor_valid_range(v in 180.0f32..=2160.0) {
        let result = Degrees::new_dor(v);
        prop_assert!(result.is_ok(), "Valid DOR {} rejected", v);
    }

    /// CurvePoint in valid range always constructs successfully.
    #[test]
    fn curve_point_valid_range(
        input in 0.0f32..=1.0,
        output in 0.0f32..=1.0,
    ) {
        let result = domain::CurvePoint::new(input, output);
        prop_assert!(result.is_ok(), "Valid curve point ({}, {}) rejected", input, output);
    }

    /// NaN and Infinity are always rejected for TorqueNm.
    #[test]
    fn torque_rejects_nan_inf(v in prop::num::f32::ANY.prop_filter("non-finite", |v| !v.is_finite())) {
        let result = TorqueNm::new(v);
        prop_assert!(result.is_err(), "Non-finite torque {} accepted", v);
    }

    /// NaN and Infinity are always rejected for Gain.
    #[test]
    fn gain_rejects_nan_inf(v in prop::num::f32::ANY.prop_filter("non-finite", |v| !v.is_finite())) {
        let result = Gain::new(v);
        prop_assert!(result.is_err(), "Non-finite gain {} accepted", v);
    }
}

// ---------------------------------------------------------------------------
// SchemaVersion invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// SchemaVersion comparison is transitive: if a < b and b < c then a < c.
    #[test]
    fn schema_version_comparison_transitive(
        a_major in 0u32..=10,
        a_minor in 0u32..=10,
        b_major in 0u32..=10,
        b_minor in 0u32..=10,
        c_major in 0u32..=10,
        c_minor in 0u32..=10,
    ) {
        let a = SchemaVersion::new(a_major, a_minor);
        let b = SchemaVersion::new(b_major, b_minor);
        let c = SchemaVersion::new(c_major, c_minor);

        if a.is_older_than(&b) && b.is_older_than(&c) {
            prop_assert!(
                a.is_older_than(&c),
                "Transitivity violated: {:?} < {:?} < {:?} but not {:?} < {:?}",
                a, b, c, a, c
            );
        }
    }

    /// SchemaVersion comparison is antisymmetric: if a < b then !(b < a).
    #[test]
    fn schema_version_comparison_antisymmetric(
        a_major in 0u32..=10,
        a_minor in 0u32..=10,
        b_major in 0u32..=10,
        b_minor in 0u32..=10,
    ) {
        let a = SchemaVersion::new(a_major, a_minor);
        let b = SchemaVersion::new(b_major, b_minor);

        if a.is_older_than(&b) {
            prop_assert!(
                !b.is_older_than(&a),
                "Antisymmetry violated: {:?} < {:?} AND {:?} < {:?}",
                a, b, b, a
            );
        }
    }

    /// SchemaVersion is never older than itself (irreflexivity).
    #[test]
    fn schema_version_not_older_than_self(
        major in 0u32..=100,
        minor in 0u32..=100,
    ) {
        let v = SchemaVersion::new(major, minor);
        prop_assert!(
            !v.is_older_than(&v),
            "Version {:?} is older than itself",
            v
        );
    }

    /// SchemaVersion parse roundtrips for well-formed version strings.
    #[test]
    fn schema_version_parse_roundtrip(
        major in 0u32..=100,
        minor in 0u32..=100,
    ) {
        let version_str = format!("wheel.profile/{}.{}", major, minor);
        let parsed = SchemaVersion::parse(&version_str);
        prop_assert!(parsed.is_ok(), "Failed to parse: {}", version_str);
        let parsed = parsed.map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        prop_assert_eq!(parsed.major, major);
        prop_assert_eq!(parsed.minor, minor);
    }

    /// SchemaVersion Display is never empty.
    #[test]
    fn schema_version_display_never_empty(
        major in 0u32..=100,
        minor in 0u32..=100,
    ) {
        let v = SchemaVersion::new(major, minor);
        prop_assert!(!v.to_string().is_empty());
    }
}
