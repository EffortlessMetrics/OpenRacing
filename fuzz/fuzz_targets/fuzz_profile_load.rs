//! Fuzzes profile JSON deserialization and validation.
//!
//! Exercises two profile formats:
//! 1. `WheelProfile` (openracing-profile) — the internal settings model.
//! 2. `ProfileSchema` (racing-wheel-schemas) — the JSON-schema-validated format
//!    parsed by `ProfileValidator::validate_json()` and
//!    `ProfileMigrator::migrate_profile()`.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_profile_load

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_profile::WheelProfile;
use racing_wheel_schemas::config::{ProfileMigrator, ProfileValidator};

fuzz_target!(|data: &[u8]| {
    // Treat the input as a UTF-8 string; non-UTF-8 data is still interesting
    // because the JSON parsers must reject it cleanly.
    let Ok(json) = core::str::from_utf8(data) else {
        return;
    };

    // --- WheelProfile (serde_json) ---
    let _ = serde_json::from_str::<WheelProfile>(json);

    // --- ProfileSchema (JSON-schema validated) ---
    if let Ok(validator) = ProfileValidator::new() {
        let _ = validator.validate_json(json);
    }

    // --- Profile migration path ---
    let _ = ProfileMigrator::migrate_profile(json);
});
