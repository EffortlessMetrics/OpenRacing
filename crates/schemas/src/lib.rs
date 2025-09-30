//! Racing Wheel Software - Schema Definitions
//!
//! This crate contains all the schema definitions for IPC communication,
//! configuration files, and data interchange formats.

#![deny(static_mut_refs)]

pub mod domain;
pub mod entities;

#[cfg(test)]
mod validation_tests;

// Temporarily disable protobuf types to focus on profile repository
// #[cfg(has_protoc)]
// pub mod wheel {
//     //! Generated protobuf types for wheel service
//     tonic::include_proto!("wheel.v1");
// }

// #[cfg(not(has_protoc))]
// pub mod wheel {
//     //! Stub protobuf types when protoc is not available
//     include!("generated/wheel.v1.rs");
// }

// Re-export commonly used types
pub use domain::{
    DeviceId, ProfileId, TorqueNm, Degrees, Gain, FrequencyHz, CurvePoint,
    DomainError, validate_curve_monotonic,
};

pub use entities::{
    Device, DeviceCapabilities, DeviceState, DeviceType,
    Profile, ProfileScope, ProfileMetadata,
    BaseSettings, FilterConfig, NotchFilter,
    LedConfig, HapticsConfig,
    CalibrationData, PedalCalibrationData, CalibrationType,
};

pub mod config {
    //! Configuration schema types and validation
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use jsonschema::JSONSchema;
    use thiserror::Error;

    /// Schema validation errors
    #[derive(Error, Debug)]
    pub enum SchemaError {
        #[error("JSON parsing error: {0}")]
        JsonError(#[from] serde_json::Error),
        
        #[error("Schema compilation error: {0}")]
        SchemaCompilationError(String),
        
        #[error("Validation error at {path}: {message}")]
        ValidationError { path: String, message: String },
        
        #[error("Multiple validation errors: {0:?}")]
        MultipleValidationErrors(Vec<SchemaError>),
        
        #[error("Unsupported schema version: {0}")]
        UnsupportedSchemaVersion(String),
        
        #[error("Curve points are not monotonic")]
        NonMonotonicCurve,
    }

    /// Profile schema for JSON serialization/deserialization
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ProfileSchema {
        pub schema: String,
        pub scope: ProfileScope,
        pub base: BaseSettings,
        pub leds: Option<LedConfig>,
        pub haptics: Option<HapticsConfig>,
        pub signature: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ProfileScope {
        pub game: Option<String>,
        pub car: Option<String>,
        pub track: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BaseSettings {
        #[serde(rename = "ffbGain")]
        pub ffb_gain: f32,
        #[serde(rename = "dorDeg")]
        pub dor_deg: u16,
        #[serde(rename = "torqueCapNm")]
        pub torque_cap_nm: f32,
        pub filters: FilterConfig,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FilterConfig {
        pub reconstruction: u8,
        pub friction: f32,
        pub damper: f32,
        pub inertia: f32,
        #[serde(rename = "notchFilters")]
        pub notch_filters: Vec<NotchFilter>,
        #[serde(rename = "slewRate")]
        pub slew_rate: f32,
        #[serde(rename = "curvePoints")]
        pub curve_points: Vec<CurvePoint>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct NotchFilter {
        pub hz: f32,
        pub q: f32,
        #[serde(rename = "gainDb")]
        pub gain_db: f32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CurvePoint {
        pub input: f32,
        pub output: f32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LedConfig {
        #[serde(rename = "rpmBands")]
        pub rpm_bands: Vec<f32>,
        pub pattern: String,
        pub brightness: f32,
        pub colors: Option<std::collections::HashMap<String, [u8; 3]>>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct HapticsConfig {
        pub enabled: bool,
        pub intensity: f32,
        #[serde(rename = "frequencyHz")]
        pub frequency_hz: f32,
        pub effects: Option<std::collections::HashMap<String, bool>>,
    }

    /// Profile validator with JSON Schema support
    pub struct ProfileValidator {
        schema: JSONSchema,
    }

    impl ProfileValidator {
        /// Create a new profile validator
        pub fn new() -> Result<Self, SchemaError> {
            let schema_json = include_str!("../schemas/profile.schema.json");
            let schema_value: Value = serde_json::from_str(schema_json)?;
            
            let schema = JSONSchema::compile(&schema_value)
                .map_err(|e| SchemaError::SchemaCompilationError(e.to_string()))?;
            
            Ok(Self { schema })
        }
        
        /// Validate a profile JSON string
        pub fn validate_json(&self, json: &str) -> Result<ProfileSchema, SchemaError> {
            // Parse JSON
            let value: Value = serde_json::from_str(json)?;
            
            // Validate against schema
            if let Err(errors) = self.schema.validate(&value) {
                let schema_errors: Vec<SchemaError> = errors
                    .map(|error| SchemaError::ValidationError {
                        path: error.instance_path.to_string(),
                        message: error.to_string(),
                    })
                    .collect();
                
                if schema_errors.len() == 1 {
                    return Err(schema_errors.into_iter().next().unwrap());
                } else {
                    return Err(SchemaError::MultipleValidationErrors(schema_errors));
                }
            }
            
            // Deserialize to typed structure
            let profile: ProfileSchema = serde_json::from_value(value)?;
            
            // Additional business logic validation
            self.validate_business_rules(&profile)?;
            
            Ok(profile)
        }
        
        /// Validate a profile struct
        pub fn validate_profile(&self, profile: &ProfileSchema) -> Result<(), SchemaError> {
            // Serialize to JSON and validate
            let json = serde_json::to_string(profile)?;
            self.validate_json(&json)?;
            Ok(())
        }
        
        /// Additional business logic validation beyond JSON Schema
        fn validate_business_rules(&self, profile: &ProfileSchema) -> Result<(), SchemaError> {
            // Check schema version
            if profile.schema != "wheel.profile/1" {
                return Err(SchemaError::UnsupportedSchemaVersion(profile.schema.clone()));
            }
            
            // Validate curve points are monotonic
            let curve_points = &profile.base.filters.curve_points;
            for window in curve_points.windows(2) {
                if window[1].input <= window[0].input {
                    return Err(SchemaError::NonMonotonicCurve);
                }
            }
            
            // Validate RPM bands are sorted (if LED config exists)
            if let Some(ref leds) = profile.leds {
                for window in leds.rpm_bands.windows(2) {
                    if window[1] <= window[0] {
                        return Err(SchemaError::ValidationError {
                            path: "leds.rpmBands".to_string(),
                            message: "RPM bands must be in ascending order".to_string(),
                        });
                    }
                }
            }
            
            Ok(())
        }
    }

    impl Default for ProfileValidator {
        fn default() -> Self {
            Self::new().expect("Failed to create default ProfileValidator")
        }
    }

    /// Migration support for profile schemas
    pub struct ProfileMigrator;

    impl ProfileMigrator {
        /// Migrate a profile from an older schema version
        pub fn migrate_profile(json: &str) -> Result<ProfileSchema, SchemaError> {
            let value: Value = serde_json::from_str(json)?;
            
            // Check schema version
            let schema_version = value
                .get("schema")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            
            match schema_version {
                "wheel.profile/1" => {
                    // Current version, no migration needed
                    let validator = ProfileValidator::new()?;
                    validator.validate_json(json)
                }
                _ => {
                    // Future: Add migration logic for older versions
                    Err(SchemaError::UnsupportedSchemaVersion(schema_version.to_string()))
                }
            }
        }
    }
}