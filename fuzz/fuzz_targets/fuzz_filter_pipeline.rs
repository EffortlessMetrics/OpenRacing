//! Fuzzes filter pipeline configuration parsing and validation.
//!
//! Exercises:
//! - JSON deserialization of the entities-level `FilterConfig` (with `Gain`
//!   newtypes, `NotchFilter`, `CurvePoint`, `BumpstopConfig`, `HandsOffConfig`).
//! - JSON deserialization of the config-level `FilterConfig` (profile schema).
//! - `PipelineValidator::validate_config()` on both default and fuzzed configs.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_filter_pipeline

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_pipeline::PipelineValidator;
use racing_wheel_schemas::entities::FilterConfig;

fuzz_target!(|data: &[u8]| {
    let validator = PipelineValidator::new();

    // --- JSON deserialization of domain FilterConfig ---
    if let Ok(json) = core::str::from_utf8(data) {
        if let Ok(config) = serde_json::from_str::<FilterConfig>(json) {
            // Validate: must never panic regardless of field values.
            let _ = validator.validate_config(&config);
            let _ = config.is_linear();
        }

        // Also try the profile-schema-level FilterConfig.
        let _ = serde_json::from_str::<racing_wheel_schemas::config::FilterConfig>(json);
    }

    // --- Default config validation (baseline sanity) ---
    let default_config = FilterConfig::default();
    let _ = validator.validate_config(&default_config);
});
