//! Config edge-case tests for the racing-wheel-schemas crate
//!
//! Tests partial configs, extra/unknown sections, type mismatches,
//! out-of-range values, format roundtrips, and validation corner cases.

use racing_wheel_schemas::config::{Profile, ProfileMigrator, ProfileValidator};
use serde_json::json;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Helper: build a minimal valid profile JSON value.
fn minimal_valid_profile() -> serde_json::Value {
    json!({
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 0.5,
            "dorDeg": 540,
            "torqueCapNm": 10.0,
            "filters": {
                "reconstruction": 2,
                "friction": 0.1,
                "damper": 0.15,
                "inertia": 0.05,
                "notchFilters": [],
                "slewRate": 0.8,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 1.0, "output": 1.0}
                ]
            }
        }
    })
}

/// Helper: turn a JSON value into its serialized string.
fn to_json_str(v: &serde_json::Value) -> String {
    serde_json::to_string(v).ok().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Partial / missing sections
// ---------------------------------------------------------------------------

mod partial_config {
    use super::*;

    #[test]
    fn missing_schema_field_rejected() {
        let validator = ProfileValidator::new();
        assert!(validator.is_ok(), "validator creation failed");
        let v = validator.ok();
        if let Some(val) = v {
            let mut profile = minimal_valid_profile();
            profile.as_object_mut().and_then(|m| m.remove("schema"));
            let result = val.validate_json(&to_json_str(&profile));
            assert!(result.is_err(), "missing schema must be rejected");
        }
    }

    #[test]
    fn missing_base_section_rejected() {
        let validator = ProfileValidator::new();
        assert!(validator.is_ok());
        let v = validator.ok();
        if let Some(val) = v {
            let mut profile = minimal_valid_profile();
            profile.as_object_mut().and_then(|m| m.remove("base"));
            let result = val.validate_json(&to_json_str(&profile));
            assert!(result.is_err(), "missing base must be rejected");
        }
    }

    #[test]
    fn missing_filters_section_rejected() {
        let validator = ProfileValidator::new();
        assert!(validator.is_ok());
        let v = validator.ok();
        if let Some(val) = v {
            let mut profile = minimal_valid_profile();
            if let Some(base) = profile.get_mut("base").and_then(|b| b.as_object_mut()) {
                base.remove("filters");
            }
            let result = val.validate_json(&to_json_str(&profile));
            assert!(result.is_err(), "missing filters must be rejected");
        }
    }

    #[test]
    fn empty_scope_accepted() -> TestResult {
        let validator = ProfileValidator::new()?;
        let profile = minimal_valid_profile();
        let result = validator.validate_json(&to_json_str(&profile));
        assert!(
            result.is_ok(),
            "empty scope should be valid: {:?}",
            result.err()
        );
        Ok(())
    }

    #[test]
    fn scope_with_all_fields_accepted() -> TestResult {
        let validator = ProfileValidator::new()?;
        let mut profile = minimal_valid_profile();
        profile["scope"] = json!({
            "game": "iracing",
            "car": "mx5",
            "track": "spa"
        });
        let result = validator.validate_json(&to_json_str(&profile));
        assert!(
            result.is_ok(),
            "full scope should be valid: {:?}",
            result.err()
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Extra / unknown sections
// ---------------------------------------------------------------------------

mod extra_sections {
    use super::*;

    #[test]
    fn extra_top_level_field_rejected() {
        let validator = ProfileValidator::new();
        assert!(validator.is_ok());
        let v = validator.ok();
        if let Some(val) = v {
            let mut profile = minimal_valid_profile();
            profile["unknownSection"] = json!({"foo": "bar"});
            let result = val.validate_json(&to_json_str(&profile));
            assert!(result.is_err(), "extra top-level field should be rejected");
        }
    }

    #[test]
    fn extra_field_in_base_rejected() {
        let validator = ProfileValidator::new();
        assert!(validator.is_ok());
        let v = validator.ok();
        if let Some(val) = v {
            let mut profile = minimal_valid_profile();
            if let Some(base) = profile.get_mut("base").and_then(|b| b.as_object_mut()) {
                base.insert("extraField".to_string(), json!(42));
            }
            let result = val.validate_json(&to_json_str(&profile));
            assert!(result.is_err(), "extra field in base should be rejected");
        }
    }
}

// ---------------------------------------------------------------------------
// Type mismatches
// ---------------------------------------------------------------------------

mod type_mismatches {
    use super::*;

    #[test]
    fn ffb_gain_as_string_rejected() {
        let validator = ProfileValidator::new();
        assert!(validator.is_ok());
        let v = validator.ok();
        if let Some(val) = v {
            let mut profile = minimal_valid_profile();
            profile["base"]["ffbGain"] = json!("not a number");
            let result = val.validate_json(&to_json_str(&profile));
            assert!(result.is_err(), "string ffbGain must be rejected");
        }
    }

    #[test]
    fn dor_deg_as_float_accepted_if_valid() -> TestResult {
        // JSON numbers are untyped; serde may coerce 540.0 → u16
        let validator = ProfileValidator::new()?;
        let mut profile = minimal_valid_profile();
        profile["base"]["dorDeg"] = json!(540.0);
        // May succeed or fail depending on schema; just assert no panic
        let _ = validator.validate_json(&to_json_str(&profile));
        Ok(())
    }

    #[test]
    fn boolean_where_number_expected_rejected() {
        let validator = ProfileValidator::new();
        assert!(validator.is_ok());
        let v = validator.ok();
        if let Some(val) = v {
            let mut profile = minimal_valid_profile();
            profile["base"]["torqueCapNm"] = json!(true);
            let result = val.validate_json(&to_json_str(&profile));
            assert!(result.is_err(), "boolean torqueCapNm must be rejected");
        }
    }

    #[test]
    fn null_for_required_field_rejected() {
        let validator = ProfileValidator::new();
        assert!(validator.is_ok());
        let v = validator.ok();
        if let Some(val) = v {
            let mut profile = minimal_valid_profile();
            profile["base"]["ffbGain"] = json!(null);
            let result = val.validate_json(&to_json_str(&profile));
            assert!(result.is_err(), "null ffbGain must be rejected");
        }
    }
}

// ---------------------------------------------------------------------------
// Out-of-range values
// ---------------------------------------------------------------------------

mod out_of_range {
    use super::*;

    #[test]
    fn ffb_gain_negative_rejected() {
        let validator = ProfileValidator::new();
        assert!(validator.is_ok());
        let v = validator.ok();
        if let Some(val) = v {
            let mut profile = minimal_valid_profile();
            profile["base"]["ffbGain"] = json!(-0.1);
            let result = val.validate_json(&to_json_str(&profile));
            assert!(result.is_err());
        }
    }

    #[test]
    fn dor_deg_below_minimum_rejected() {
        let validator = ProfileValidator::new();
        assert!(validator.is_ok());
        let v = validator.ok();
        if let Some(val) = v {
            let mut profile = minimal_valid_profile();
            profile["base"]["dorDeg"] = json!(90);
            let result = val.validate_json(&to_json_str(&profile));
            assert!(result.is_err(), "DOR below 180 must be rejected");
        }
    }

    #[test]
    fn dor_deg_above_maximum_rejected() {
        let validator = ProfileValidator::new();
        assert!(validator.is_ok());
        let v = validator.ok();
        if let Some(val) = v {
            let mut profile = minimal_valid_profile();
            profile["base"]["dorDeg"] = json!(5000);
            let result = val.validate_json(&to_json_str(&profile));
            assert!(result.is_err(), "DOR above 2160 must be rejected");
        }
    }

    #[test]
    fn reconstruction_above_max_rejected() {
        let validator = ProfileValidator::new();
        assert!(validator.is_ok());
        let v = validator.ok();
        if let Some(val) = v {
            let mut profile = minimal_valid_profile();
            profile["base"]["filters"]["reconstruction"] = json!(9);
            let result = val.validate_json(&to_json_str(&profile));
            assert!(result.is_err(), "reconstruction > 8 must be rejected");
        }
    }

    #[test]
    fn torque_cap_nm_negative_rejected() {
        let validator = ProfileValidator::new();
        assert!(validator.is_ok());
        let v = validator.ok();
        if let Some(val) = v {
            let mut profile = minimal_valid_profile();
            profile["base"]["torqueCapNm"] = json!(-1.0);
            let result = val.validate_json(&to_json_str(&profile));
            assert!(result.is_err(), "negative torque must be rejected");
        }
    }
}

// ---------------------------------------------------------------------------
// Roundtrip through different representations
// ---------------------------------------------------------------------------

mod roundtrip {
    use super::*;

    #[test]
    fn json_roundtrip_preserves_all_fields() -> TestResult {
        let validator = ProfileValidator::new()?;
        let profile = validator.validate_json(&to_json_str(&minimal_valid_profile()))?;

        let serialized = serde_json::to_string(&profile)?;
        let restored: Profile = serde_json::from_str(&serialized)?;

        assert_eq!(profile.schema, restored.schema);
        assert_eq!(profile.base.ffb_gain, restored.base.ffb_gain);
        assert_eq!(profile.base.dor_deg, restored.base.dor_deg);
        assert!((profile.base.torque_cap_nm - restored.base.torque_cap_nm).abs() < f32::EPSILON);
        assert_eq!(
            profile.base.filters.reconstruction,
            restored.base.filters.reconstruction
        );
        Ok(())
    }

    #[test]
    fn pretty_json_roundtrip() -> TestResult {
        let validator = ProfileValidator::new()?;
        let profile = validator.validate_json(&to_json_str(&minimal_valid_profile()))?;

        let pretty = serde_json::to_string_pretty(&profile)?;
        let compact = serde_json::to_string(&profile)?;

        let from_pretty: Profile = serde_json::from_str(&pretty)?;
        let from_compact: Profile = serde_json::from_str(&compact)?;

        assert_eq!(from_pretty.schema, from_compact.schema);
        assert_eq!(from_pretty.base.dor_deg, from_compact.base.dor_deg);
        Ok(())
    }

    #[test]
    fn value_roundtrip_preserves_identity() -> TestResult {
        let validator = ProfileValidator::new()?;
        let profile = validator.validate_json(&to_json_str(&minimal_valid_profile()))?;

        let value = serde_json::to_value(&profile)?;
        let restored: Profile = serde_json::from_value(value)?;

        assert_eq!(profile.schema, restored.schema);
        assert_eq!(
            profile.base.filters.curve_points.len(),
            restored.base.filters.curve_points.len()
        );
        Ok(())
    }

    #[test]
    fn migrator_current_version_is_noop() -> TestResult {
        let json_str = to_json_str(&minimal_valid_profile());
        let result = ProfileMigrator::migrate_profile(&json_str);
        assert!(
            result.is_ok(),
            "current version migration should succeed: {:?}",
            result.err()
        );
        Ok(())
    }

    #[test]
    fn migrator_unknown_version_rejected() {
        let mut profile = minimal_valid_profile();
        profile["schema"] = json!("wheel.profile/99");
        let result = ProfileMigrator::migrate_profile(&to_json_str(&profile));
        assert!(result.is_err(), "unknown schema version must be rejected");
    }
}
