//! Example demonstrating schema validation functionality
//!
//! This example shows how to use the ProfileValidator to validate
//! racing wheel profile configurations against the JSON Schema.

use racing_wheel_schemas::config::{ProfileMigrator, ProfileValidator, SchemaError};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Racing Wheel Schema Validation Example");
    println!("=====================================\n");

    // Create a validator
    let validator = ProfileValidator::new()?;
    println!("✓ ProfileValidator created successfully\n");

    // Example 1: Valid profile
    println!("1. Testing valid profile...");
    let valid_profile = json!({
        "schema": "wheel.profile/1",
        "scope": {
            "game": "iracing",
            "car": "gt3"
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

    let json_str = serde_json::to_string_pretty(&valid_profile)?;
    match validator.validate_json(&json_str) {
        Ok(profile) => {
            println!("✓ Valid profile passed validation");
            println!("  - Schema: {}", profile.schema);
            println!("  - Game: {:?}", profile.scope.game);
            println!("  - Car: {:?}", profile.scope.car);
            println!("  - FFB Gain: {}", profile.base.ffb_gain);
            println!("  - DOR: {}°", profile.base.dor_deg);
            println!("  - Torque Cap: {} Nm", profile.base.torque_cap_nm);
        }
        Err(e) => {
            println!("✗ Unexpected validation error: {}", e);
        }
    }
    println!();

    // Example 2: Invalid profile - missing required field
    println!("2. Testing invalid profile (missing required field)...");
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

    let json_str = serde_json::to_string(&invalid_profile)?;
    match validator.validate_json(&json_str) {
        Ok(_) => {
            println!("✗ Invalid profile unexpectedly passed validation");
        }
        Err(SchemaError::ValidationError { path, message }) => {
            println!("✓ Invalid profile correctly failed validation");
            println!("  - Path: {}", path);
            println!("  - Message: {}", message);
        }
        Err(e) => {
            println!("✓ Invalid profile failed with error: {}", e);
        }
    }
    println!();

    // Example 3: Invalid profile - out of range values
    println!("3. Testing invalid profile (out of range values)...");
    let invalid_range_profile = json!({
        "schema": "wheel.profile/1",
        "scope": {},
        "base": {
            "ffbGain": 1.5, // Invalid: > 1.0
            "dorDeg": 90,   // Invalid: < 180
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

    let json_str = serde_json::to_string(&invalid_range_profile)?;
    match validator.validate_json(&json_str) {
        Ok(_) => {
            println!("✗ Invalid profile unexpectedly passed validation");
        }
        Err(e) => {
            println!("✓ Invalid profile correctly failed validation");
            println!("  - Error: {}", e);
        }
    }
    println!();

    // Example 4: Invalid profile - non-monotonic curve
    println!("4. Testing invalid profile (non-monotonic curve)...");
    let non_monotonic_profile = json!({
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

    let json_str = serde_json::to_string(&non_monotonic_profile)?;
    match validator.validate_json(&json_str) {
        Ok(_) => {
            println!("✗ Invalid profile unexpectedly passed validation");
        }
        Err(SchemaError::NonMonotonicCurve) => {
            println!("✓ Non-monotonic curve correctly detected");
        }
        Err(e) => {
            println!("✓ Invalid profile failed with error: {}", e);
        }
    }
    println!();

    // Example 5: Profile migration
    println!("5. Testing profile migration...");
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

    let json_str = serde_json::to_string(&current_profile)?;
    match ProfileMigrator::migrate_profile(&json_str) {
        Ok(profile) => {
            println!("✓ Profile migration successful");
            println!("  - Schema: {}", profile.schema);
        }
        Err(e) => {
            println!("✗ Profile migration failed: {}", e);
        }
    }
    println!();

    // Example 6: Complex profile with all features
    println!("6. Testing complex profile with all features...");
    let complex_profile = json!({
        "schema": "wheel.profile/1",
        "scope": {
            "game": "assetto_corsa_competizione",
            "car": "porsche_991_gt3_r",
            "track": "spa_francorchamps"
        },
        "base": {
            "ffbGain": 0.68,
            "dorDeg": 540,
            "torqueCapNm": 12.5,
            "filters": {
                "reconstruction": 6,
                "friction": 0.15,
                "damper": 0.22,
                "inertia": 0.12,
                "notchFilters": [
                    {
                        "hz": 7.5,
                        "q": 3.0,
                        "gainDb": -10.0
                    },
                    {
                        "hz": 15.0,
                        "q": 2.5,
                        "gainDb": -8.0
                    }
                ],
                "slewRate": 0.75,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 0.2, "output": 0.15},
                    {"input": 0.4, "output": 0.35},
                    {"input": 0.6, "output": 0.58},
                    {"input": 0.8, "output": 0.78},
                    {"input": 1.0, "output": 1.0}
                ]
            }
        },
        "leds": {
            "rpmBands": [0.70, 0.78, 0.84, 0.88, 0.92, 0.95, 0.98],
            "pattern": "wipe",
            "brightness": 0.9,
            "colors": {
                "green": [0, 255, 0],
                "yellow": [255, 255, 0],
                "orange": [255, 165, 0],
                "red": [255, 0, 0],
                "blue": [0, 0, 255]
            }
        },
        "haptics": {
            "enabled": true,
            "intensity": 0.7,
            "frequencyHz": 120.0,
            "effects": {
                "kerb": true,
                "slip": true,
                "gear_shift": true,
                "collision": true,
                "abs": false,
                "tc": false
            }
        },
        "signature": "base64encodedSignatureWouldGoHere=="
    });

    let json_str = serde_json::to_string_pretty(&complex_profile)?;
    match validator.validate_json(&json_str) {
        Ok(profile) => {
            println!("✓ Complex profile passed validation");
            println!("  - Schema: {}", profile.schema);
            println!(
                "  - Scope: Game={:?}, Car={:?}, Track={:?}",
                profile.scope.game, profile.scope.car, profile.scope.track
            );
            println!(
                "  - Notch filters: {}",
                profile.base.filters.notch_filters.len()
            );
            println!(
                "  - Curve points: {}",
                profile.base.filters.curve_points.len()
            );
            println!(
                "  - LED RPM bands: {}",
                profile.leds.as_ref().map_or(0, |l| l.rpm_bands.len())
            );
            println!(
                "  - Haptic effects: {}",
                profile
                    .haptics
                    .as_ref()
                    .and_then(|h| h.effects.as_ref())
                    .map_or(0, |e| e.len())
            );
            println!("  - Has signature: {}", profile.signature.is_some());
        }
        Err(e) => {
            println!("✗ Complex profile validation failed: {}", e);
        }
    }

    println!("\nSchema validation examples completed successfully!");
    Ok(())
}
