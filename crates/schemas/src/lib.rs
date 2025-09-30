//! Racing Wheel Software - Schema Definitions
//!
//! This crate contains all the schema definitions for IPC communication,
//! configuration files, and data interchange formats.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]

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

/// Public prelude module for explicit imports
/// 
/// Consumers must use `racing_wheel_schemas::prelude::*` explicitly
/// to import commonly used types.
pub mod prelude {
    // Domain types
    pub use crate::domain::{
        DeviceId, ProfileId, TorqueNm, Degrees, Gain, FrequencyHz, CurvePoint,
        DomainError, validate_curve_monotonic,
    };

    // Entity types
    pub use crate::entities::{
        Device, DeviceCapabilities, DeviceState, DeviceType,
        Profile, ProfileScope, ProfileMetadata,
        BaseSettings, FilterConfig, NotchFilter,
        LedConfig, HapticsConfig,
        CalibrationData, PedalCalibrationData, CalibrationType,
    };

    // Telemetry types
    pub use crate::telemetry::TelemetryData;

    // Configuration types
    pub use crate::config::{
        ProfileSchema, ProfileValidator, ProfileMigrator,
        BumpstopConfig, HandsOffConfig,
    };
}

pub mod profile {
    //! Profile types for JSON serialization
    pub use crate::entities::{Profile, ProfileScope, ProfileMetadata, BaseSettings, FilterConfig};
    pub use crate::config::{ProfileSchema, ProfileValidator, ProfileMigrator};
}

pub mod telemetry {
    //! Telemetry data types
    use serde::{Deserialize, Serialize};
    
    /// Telemetry data with new field names
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
    pub struct TelemetryData {
        /// Wheel angle in degrees (new field name)
        pub wheel_angle_deg: f32,
        
        /// Wheel speed in radians per second (new field name)
        pub wheel_speed_rad_s: f32,
        
        /// Temperature in Celsius (new field name)
        pub temperature_c: u8,
        
        /// Fault flags (new field name)
        pub fault_flags: u8,
        
        /// Hands on wheel detection
        pub hands_on: bool,
        
        /// Timestamp in milliseconds
        pub timestamp: u64,
    }
}

pub mod device {
    //! Device types
    pub use crate::entities::{Device, DeviceCapabilities, DeviceState, DeviceType};
    pub use crate::domain::DeviceId;
}

pub mod config {
    //! Configuration schema types and validation
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use jsonschema::Validator;
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
        pub base: BaseConfig,
        pub leds: Option<LedConfig>,
        pub haptics: Option<HapticsConfig>,
        pub signature: Option<String>,
    }
    
    /// Alias for compatibility with tests
    pub type Profile = ProfileSchema;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ProfileScope {
        pub game: Option<String>,
        pub car: Option<String>,
        pub track: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BaseConfig {
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
        pub bumpstop: BumpstopConfig,
        #[serde(rename = "handsOff")]
        pub hands_off: HandsOffConfig,
        #[serde(rename = "torqueCap")]
        pub torque_cap: Option<f32>,
        #[serde(rename = "notchFilters")]
        pub notch_filters: Vec<NotchFilter>,
        #[serde(rename = "slewRate")]
        pub slew_rate: f32,
        #[serde(rename = "curvePoints")]
        pub curve_points: Vec<CurvePoint>,
    }
    
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BumpstopConfig {
        #[serde(default = "default_true")]
        pub enabled: bool,
        #[serde(default = "default_bumpstop_strength")]
        pub strength: f32,
    }
    
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct HandsOffConfig {
        #[serde(default = "default_true")]
        pub enabled: bool,
        #[serde(default = "default_hands_off_sensitivity")]
        pub sensitivity: f32,
    }
    
    fn default_true() -> bool { true }
    fn default_bumpstop_strength() -> f32 { 0.5 }
    fn default_hands_off_sensitivity() -> f32 { 0.3 }
    
    impl Default for BumpstopConfig {
        fn default() -> Self {
            Self {
                enabled: true,
                strength: 0.5,
            }
        }
    }
    
    impl Default for HandsOffConfig {
        fn default() -> Self {
            Self {
                enabled: true,
                sensitivity: 0.3,
            }
        }
    }
    
    impl Default for FilterConfig {
        /// Create FilterConfig with stable 1kHz-safe defaults
        /// 
        /// These defaults are designed to be stable at 1kHz update rates
        /// with no oscillation or instability.
        fn default() -> Self {
            Self {
                // Stable values - no reconstruction filtering
                reconstruction: 0,
                friction: 0.0,
                damper: 0.0,
                inertia: 0.0,
                bumpstop: BumpstopConfig::default(),
                hands_off: HandsOffConfig::default(),
                torque_cap: Some(10.0), // Explicit for test predictability
                notch_filters: Vec::new(),
                slew_rate: 1.0, // No slew rate limiting
                curve_points: vec![
                    CurvePoint { input: 0.0, output: 0.0 },
                    CurvePoint { input: 1.0, output: 1.0 },
                ],
            }
        }
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
        schema: Validator,
    }

    impl ProfileValidator {
        /// Create a new profile validator
        pub fn new() -> Result<Self, SchemaError> {
            let schema_json = include_str!("../schemas/profile.schema.json");
            let schema_value: Value = serde_json::from_str(schema_json)?;
            
            let schema = Validator::new(&schema_value)
                .map_err(|e| SchemaError::SchemaCompilationError(e.to_string()))?;
            
            Ok(Self { schema })
        }
        
        /// Validate a profile JSON string
        pub fn validate_json(&self, json: &str) -> Result<Profile, SchemaError> {
            // Parse JSON
            let value: Value = serde_json::from_str(json)?;
            
            // Validate against schema
            if let Err(error) = self.schema.validate(&value) {
                return Err(SchemaError::ValidationError {
                    path: "root".to_string(),
                    message: error.to_string(),
                });
            }
            
            // Deserialize to typed structure
            let profile: Profile = serde_json::from_value(value)?;
            
            // Additional business logic validation
            self.validate_business_rules(&profile)?;
            
            Ok(profile)
        }
        
        /// Validate a profile struct
        pub fn validate_profile(&self, profile: &Profile) -> Result<(), SchemaError> {
            // Serialize to JSON and validate
            let json = serde_json::to_string(profile)?;
            self.validate_json(&json)?;
            Ok(())
        }
        
        /// Additional business logic validation beyond JSON Schema
        fn validate_business_rules(&self, profile: &Profile) -> Result<(), SchemaError> {
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
        pub fn migrate_profile(json: &str) -> Result<Profile, SchemaError> {
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