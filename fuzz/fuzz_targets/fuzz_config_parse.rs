//! Fuzzes configuration file parsing: JSON profile deserialization,
//! JSON-schema validation, YAML deserialization, and profile migration.
//!
//! Exercises every code path that accepts untrusted configuration text,
//! including malformed JSON, invalid schema versions, and edge-case YAML.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_config_parse

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_profile::WheelProfile;
use racing_wheel_schemas::config::{ProfileMigrator, ProfileValidator};

fuzz_target!(|data: &[u8]| {
    // Only process valid UTF-8 — non-UTF-8 is rejected by all JSON/YAML parsers.
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };

    // --- JSON deserialization of WheelProfile ---
    if let Ok(mut profile) = serde_json::from_str::<WheelProfile>(text) {
        // If we managed to parse, validate and migrate the profile.
        let _ = openracing_profile::validate_profile(&profile);
        let _ = openracing_profile::migrate_profile(&mut profile);
    }

    // --- JSON-schema validated profile parsing ---
    if let Ok(validator) = ProfileValidator::new() {
        let _ = validator.validate_json(text);
    }

    // --- Profile migration path ---
    let _ = ProfileMigrator::migrate_profile(text);

    // --- YAML deserialization of WheelProfile ---
    let _ = serde_yaml::from_str::<WheelProfile>(text);
});
