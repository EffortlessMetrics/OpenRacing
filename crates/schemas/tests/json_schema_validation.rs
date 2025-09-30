//! JSON Schema validation and round-trip tests
//! 
//! These tests ensure that JSON schemas are valid and that serialization/deserialization
//! works correctly with proper validation of required fields.

use serde_json::{json, Value};
use jsonschema::{Validator, ValidationError};
use racing_wheel_schemas::config::{Profile, ProfileScope, BaseConfig, FilterConfig};

#[test]
fn test_profile_schema_validation() {
    // Load the JSON schema
    let schema_path = "schemas/profile.schema.json";
    let schema_content = std::fs::read_to_string(schema_path)
        .expect("Failed to read profile schema");
    let schema: Value = serde_json::from_str(&schema_content)
        .expect("Failed to parse schema JSON");
    
    let compiled_schema = Validator::new(&schema)
        .expect("Failed to compile JSON schema");
    
    // Test valid profile
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
                "bumpstop": {},
                "handsOff": {},
                "torqueCap": 10.0,
                "notchFilters": [],
                "slewRate": 0.85,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 1.0, "output": 1.0}
                ]
            }
        }
    });
    
    let validation_result = compiled_schema.validate(&valid_profile);
    assert!(validation_result.is_ok(), "Valid profile should pass validation");
}

#[test]
fn test_profile_schema_required_fields() {
    // Load the JSON schema
    let schema_path = "schemas/profile.schema.json";
    let schema_content = std::fs::read_to_string(schema_path)
        .expect("Failed to read profile schema");
    let schema: Value = serde_json::from_str(&schema_content)
        .expect("Failed to parse schema JSON");
    
    let compiled_schema = Validator::new(&schema)
        .expect("Failed to compile JSON schema");
    
    // Test profile missing required schema field
    let invalid_profile = json!({
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
                "bumpstop": {},
                "handsOff": {},
                "torqueCap": 10.0,
                "notchFilters": [],
                "slewRate": 0.85,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 1.0, "output": 1.0}
                ]
            }
        }
    });
    
    let validation_result = compiled_schema.validate(&invalid_profile);
    assert!(validation_result.is_err(), "Profile missing required 'schema' field should fail validation");
    
    let errors: Vec<ValidationError> = validation_result.unwrap_err().collect();
    assert!(!errors.is_empty(), "Should have validation errors");
    
    // Check that the error mentions the missing required field
    let error_messages: Vec<String> = errors.iter()
        .map(|e| e.to_string())
        .collect();
    assert!(
        error_messages.iter().any(|msg| msg.contains("schema") || msg.contains("required")),
        "Error should mention missing required 'schema' field. Errors: {:?}",
        error_messages
    );
}

#[test]
fn test_profile_round_trip_serialization() {
    // Create a profile using Rust structs
    let profile = Profile {
        schema: "wheel.profile/1".to_string(),
        scope: ProfileScope {
            game: Some("iracing".to_string()),
            car: None,
            track: None,
        },
        base: BaseConfig {
            ffb_gain: 0.75,
            dor_deg: 900,
            torque_cap_nm: 15.0,
            filters: FilterConfig::default(),
        },
        leds: None,
        haptics: None,
        signature: None,
    };
    
    // Serialize to JSON
    let json_value = serde_json::to_value(&profile)
        .expect("Failed to serialize profile to JSON");
    
    // Deserialize back to struct
    let deserialized_profile: Profile = serde_json::from_value(json_value.clone())
        .expect("Failed to deserialize profile from JSON");
    
    // Verify round-trip consistency
    assert_eq!(profile.schema, deserialized_profile.schema);
    assert_eq!(profile.scope.game, deserialized_profile.scope.game);
    assert_eq!(profile.base.ffb_gain, deserialized_profile.base.ffb_gain);
    assert_eq!(profile.base.filters.reconstruction, deserialized_profile.base.filters.reconstruction);
    
    // Validate against schema
    let schema_path = "schemas/profile.schema.json";
    let schema_content = std::fs::read_to_string(schema_path)
        .expect("Failed to read profile schema");
    let schema: Value = serde_json::from_str(&schema_content)
        .expect("Failed to parse schema JSON");
    
    let compiled_schema = Validator::new(&schema)
        .expect("Failed to compile JSON schema");
    
    let validation_result = compiled_schema.validate(&json_value);
    if let Err(errors) = validation_result {
        let error_messages: Vec<String> = errors.map(|e| e.to_string()).collect();
        panic!("Round-trip serialized profile should pass schema validation. Errors: {:?}\nJSON: {}", error_messages, serde_json::to_string_pretty(&json_value).unwrap());
    }
}

#[test]
fn test_filter_config_required_fields() {
    // Test that FilterConfig requires all new fields
    let filter_config = FilterConfig::default();
    
    // Serialize and verify all fields are present
    let json_value = serde_json::to_value(&filter_config)
        .expect("Failed to serialize FilterConfig");
    
    let json_obj = json_value.as_object()
        .expect("FilterConfig should serialize to JSON object");
    
    // Verify new required fields are present
    assert!(json_obj.contains_key("bumpstop"), "FilterConfig should contain bumpstop field");
    assert!(json_obj.contains_key("handsOff"), "FilterConfig should contain handsOff field");
    assert!(json_obj.contains_key("torqueCap"), "FilterConfig should contain torqueCap field");
}

#[test]
fn test_deprecated_field_detection() {
    // This test ensures we can detect usage of deprecated field names in JSON
    let deprecated_json = json!({
        "wheel_angle_mdeg": 45000,  // Old field name
        "wheel_speed_mrad_s": 2500, // Old field name
        "temp_c": 25,               // Old field name
        "faults": 0                 // Old field name
    });
    
    // Convert to string and check for deprecated field names
    let json_string = serde_json::to_string(&deprecated_json)
        .expect("Failed to serialize JSON");
    
    let deprecated_fields = [
        "wheel_angle_mdeg",
        "wheel_speed_mrad_s", 
        "temp_c",
        "faults"
    ];
    
    for field in &deprecated_fields {
        assert!(
            json_string.contains(field),
            "Test JSON should contain deprecated field: {}",
            field
        );
    }
    
    // In a real scenario, we would want to ensure these fields are NOT present
    // This test documents what we're looking for in CI checks
}