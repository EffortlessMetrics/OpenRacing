//! Schema validation tests
//!
//! This module contains comprehensive tests for JSON Schema validation,
//! protobuf contract compatibility, and business rule validation.

#[cfg(test)]
mod tests {
    // Test helper functions to replace unwrap
    #[track_caller]
    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("unexpected Err: {e:?}"),
        }
    }

    use crate::config::{ProfileMigrator, ProfileValidator, SchemaError};
    use serde_json::json;

    #[test]
    fn test_valid_profile_schema() {
        let validator = must(ProfileValidator::new());

        let valid_profile = json!({
            "schema": "wheel.profile/1",
            "scope": {
                "game": "iracing"
            },
            "base": {
                "ffbGain": 0.75,
                "dorDeg": 900,
                "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 4,
                    "friction": 0.12,
                    "damper": 0.18,
                    "inertia": 0.08,
                    "notchFilters": [
                        {
                            "hz": 60.0,
                            "q": 2.0,
                            "gainDb": -12.0
                        }
                    ],
                    "slewRate": 0.85,
                    "curvePoints": [
                        {"input": 0.0, "output": 0.0},
                        {"input": 0.5, "output": 0.6},
                        {"input": 1.0, "output": 1.0}
                    ]
                }
            },
            "leds": {
                "rpmBands": [0.75, 0.82, 0.88, 0.92, 0.96],
                "pattern": "progressive",
                "brightness": 0.8,
                "colors": {
                    "green": [0, 255, 0],
                    "yellow": [255, 255, 0],
                    "red": [255, 0, 0]
                }
            },
            "haptics": {
                "enabled": true,
                "intensity": 0.6,
                "frequencyHz": 80.0,
                "effects": {
                    "kerb": true,
                    "slip": true,
                    "gear_shift": false
                }
            }
        });

        let json_str = must(serde_json::to_string(&valid_profile));
        let result = validator.validate_json(&json_str);

        assert!(
            result.is_ok(),
            "Valid profile should pass validation: {:?}",
            result.err()
        );

        let profile = must(result);
        assert_eq!(profile.schema, "wheel.profile/1");
        assert_eq!(profile.scope.game, Some("iracing".to_string()));
        assert_eq!(profile.base.ffb_gain, 0.75);
        assert_eq!(profile.base.dor_deg, 900);
        assert_eq!(profile.base.torque_cap_nm, 15.0);
    }

    #[test]
    fn test_minimal_valid_profile() {
        let validator = must(ProfileValidator::new());

        let minimal_profile = json!({
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
        });

        let json_str = must(serde_json::to_string(&minimal_profile));
        let result = validator.validate_json(&json_str);

        assert!(
            result.is_ok(),
            "Minimal valid profile should pass validation: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_missing_required_fields() {
        let validator = must(ProfileValidator::new());

        // Missing schema field
        let invalid_profile = json!({
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
        });

        let json_str = must(serde_json::to_string(&invalid_profile));
        let result = validator.validate_json(&json_str);

        assert!(
            result.is_err(),
            "Profile missing required fields should fail validation"
        );

        if let Err(SchemaError::ValidationError { path: _, message }) = result {
            assert!(message.contains("required"));
        } else {
            panic!("Expected ValidationError for missing required field");
        }
    }

    #[test]
    fn test_invalid_value_ranges() {
        let validator = must(ProfileValidator::new());

        // FFB gain out of range
        let invalid_profile = json!({
            "schema": "wheel.profile/1",
            "scope": {},
            "base": {
                "ffbGain": 1.5, // Invalid: > 1.0
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
        });

        let json_str = must(serde_json::to_string(&invalid_profile));
        let result = validator.validate_json(&json_str);

        assert!(
            result.is_err(),
            "Profile with out-of-range values should fail validation"
        );
    }

    #[test]
    fn test_invalid_dor_range() {
        let validator = must(ProfileValidator::new());

        // DOR below minimum
        let invalid_profile = json!({
            "schema": "wheel.profile/1",
            "scope": {},
            "base": {
                "ffbGain": 0.5,
                "dorDeg": 90, // Invalid: < 180
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
        });

        let json_str = must(serde_json::to_string(&invalid_profile));
        let result = validator.validate_json(&json_str);

        assert!(
            result.is_err(),
            "Profile with invalid DOR should fail validation"
        );
    }

    #[test]
    fn test_invalid_reconstruction_level() {
        let validator = must(ProfileValidator::new());

        // Reconstruction level too high
        let invalid_profile = json!({
            "schema": "wheel.profile/1",
            "scope": {},
            "base": {
                "ffbGain": 0.5,
                "dorDeg": 540,
                "torqueCapNm": 10.0,
                "filters": {
                    "reconstruction": 10, // Invalid: > 8
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
        });

        let json_str = must(serde_json::to_string(&invalid_profile));
        let result = validator.validate_json(&json_str);

        assert!(
            result.is_err(),
            "Profile with invalid reconstruction level should fail validation"
        );
    }

    #[test]
    fn test_non_monotonic_curve_points() {
        let validator = ProfileValidator::new().expect("Failed to create validator");

        // Non-monotonic curve points
        let invalid_profile = json!({
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
                        {"input": 0.8, "output": 0.6},
                        {"input": 0.5, "output": 1.0} // Invalid: input goes backwards
                    ]
                }
            }
        });

        let json_str = must(serde_json::to_string(&invalid_profile));
        let result = validator.validate_json(&json_str);

        assert!(
            result.is_err(),
            "Profile with non-monotonic curve should fail validation"
        );

        if let Err(SchemaError::NonMonotonicCurve) = result {
            // Expected error type
        } else {
            panic!("Expected NonMonotonicCurve error, got: {:?}", result.err());
        }
    }

    #[test]
    fn test_invalid_notch_filter() {
        let validator = must(ProfileValidator::new());

        // Invalid notch filter frequency
        let invalid_profile = json!({
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
                    "notchFilters": [
                        {
                            "hz": 0.05, // Invalid: < 0.1
                            "q": 2.0,
                            "gainDb": -12.0
                        }
                    ],
                    "slewRate": 0.8,
                    "curvePoints": [
                        {"input": 0.0, "output": 0.0},
                        {"input": 1.0, "output": 1.0}
                    ]
                }
            }
        });

        let json_str = must(serde_json::to_string(&invalid_profile));
        let result = validator.validate_json(&json_str);

        assert!(
            result.is_err(),
            "Profile with invalid notch filter should fail validation"
        );
    }

    #[test]
    fn test_invalid_led_config() {
        let validator = must(ProfileValidator::new());

        // Non-sorted RPM bands
        let invalid_profile = json!({
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
            },
            "leds": {
                "rpmBands": [0.75, 0.92, 0.82], // Invalid: not sorted
                "pattern": "progressive",
                "brightness": 0.8
            }
        });

        let json_str = must(serde_json::to_string(&invalid_profile));
        let result = validator.validate_json(&json_str);

        assert!(
            result.is_err(),
            "Profile with non-sorted RPM bands should fail validation"
        );
    }

    #[test]
    fn test_invalid_led_pattern() {
        let validator = must(ProfileValidator::new());

        // Invalid LED pattern
        let invalid_profile = json!({
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
            },
            "leds": {
                "rpmBands": [0.75, 0.82, 0.88],
                "pattern": "invalid_pattern", // Invalid: not in enum
                "brightness": 0.8
            }
        });

        let json_str = must(serde_json::to_string(&invalid_profile));
        let result = validator.validate_json(&json_str);

        assert!(
            result.is_err(),
            "Profile with invalid LED pattern should fail validation"
        );
    }

    #[test]
    fn test_invalid_haptics_frequency() {
        let validator = must(ProfileValidator::new());

        // Haptics frequency out of range
        let invalid_profile = json!({
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
            },
            "haptics": {
                "enabled": true,
                "intensity": 0.6,
                "frequencyHz": 5.0 // Invalid: < 10.0
            }
        });

        let json_str = must(serde_json::to_string(&invalid_profile));
        let result = validator.validate_json(&json_str);

        assert!(
            result.is_err(),
            "Profile with invalid haptics frequency should fail validation"
        );
    }

    #[test]
    fn test_unsupported_schema_version() {
        let validator = must(ProfileValidator::new());

        // Unsupported schema version - this will fail JSON Schema validation first
        let invalid_profile = json!({
            "schema": "wheel.profile/2", // Unsupported version
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
        });

        let json_str = must(serde_json::to_string(&invalid_profile));
        let result = validator.validate_json(&json_str);

        assert!(
            result.is_err(),
            "Profile with unsupported schema version should fail validation"
        );

        // The JSON Schema validation will catch this first with a ValidationError
        match result {
            Err(SchemaError::ValidationError { path, message }) => {
                assert!(path.contains("schema") || message.contains("wheel.profile/1"));
            }
            Err(SchemaError::UnsupportedSchemaVersion(version)) => {
                assert_eq!(version, "wheel.profile/2");
            }
            _ => panic!(
                "Expected ValidationError or UnsupportedSchemaVersion error, got: {:?}",
                result.err()
            ),
        }
    }

    #[test]
    fn test_profile_migration() {
        // Test current version (no migration needed)
        let current_profile = json!({
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
        });

        let json_str = must(serde_json::to_string(&current_profile));
        let result = ProfileMigrator::migrate_profile(&json_str);

        assert!(
            result.is_ok(),
            "Current version profile should migrate successfully"
        );

        // Test unsupported version
        let old_profile = json!({
            "schema": "wheel.profile/0",
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
        });

        let json_str = must(serde_json::to_string(&old_profile));
        let result = ProfileMigrator::migrate_profile(&json_str);

        assert!(result.is_err(), "Unsupported version should fail migration");
    }

    #[test]
    fn test_additional_properties_rejected() {
        let validator = must(ProfileValidator::new());

        // Profile with additional properties
        let invalid_profile = json!({
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
            },
            "extraField": "should not be allowed" // Additional property
        });

        let json_str = must(serde_json::to_string(&invalid_profile));
        let result = validator.validate_json(&json_str);

        assert!(
            result.is_err(),
            "Profile with additional properties should fail validation"
        );
    }

    #[test]
    fn test_curve_points_boundary_conditions() {
        let validator = must(ProfileValidator::new());

        // Test minimum curve points (2)
        let valid_profile = json!({
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
        });

        let json_str = must(serde_json::to_string(&valid_profile));
        let result = validator.validate_json(&json_str);
        assert!(
            result.is_ok(),
            "Profile with minimum curve points should be valid"
        );

        // Test too few curve points (1)
        let invalid_profile = json!({
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
                        {"input": 0.0, "output": 0.0}
                    ]
                }
            }
        });

        let json_str = must(serde_json::to_string(&invalid_profile));
        let result = validator.validate_json(&json_str);
        assert!(
            result.is_err(),
            "Profile with too few curve points should fail validation"
        );
    }
}
