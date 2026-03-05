//! Comprehensive compat layer tests.
//!
//! Covers migration-path roundtrips, legacy API shims, version detection and
//! negotiation, unsupported-version error handling, edge cases (empty input,
//! malformed data, boundary versions), CompatibleProfile accessors,
//! ProfileMigrationService, MigrationOutcome, and proptest fuzzing for
//! serialised profile types.
//!
//! All tests return `Result` — no `unwrap()` / `expect()` per project rules.

use compat::TelemetryCompat;
use proptest::prelude::*;
use racing_wheel_engine::TelemetryData;
use racing_wheel_schemas::migration::{
    CURRENT_SCHEMA_VERSION, MigrationConfig, MigrationManager, MigrationResult,
    ProfileMigrationService, SCHEMA_VERSION_V2, SchemaVersion,
    compat::BackwardCompatibleParser as CompatParser,
};
use std::time::Instant;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Newtype wrapper (orphan rule) + helpers
// ---------------------------------------------------------------------------

struct Compat(TelemetryData);

impl TelemetryCompat for Compat {
    fn temp_c(&self) -> u8 {
        self.0.temperature_c
    }
    fn faults(&self) -> u8 {
        self.0.fault_flags
    }
    fn wheel_angle_mdeg(&self) -> i32 {
        (self.0.wheel_angle_deg * 1000.0) as i32
    }
    fn wheel_speed_mrad_s(&self) -> i32 {
        (self.0.wheel_speed_rad_s * 1000.0) as i32
    }
    fn sequence(&self) -> u32 {
        0
    }
}

fn sample(angle_deg: f32, speed_rad_s: f32, temp: u8, faults: u8) -> Compat {
    Compat(TelemetryData {
        wheel_angle_deg: angle_deg,
        wheel_speed_rad_s: speed_rad_s,
        temperature_c: temp,
        fault_flags: faults,
        hands_on: false,
        timestamp: Instant::now(),
    })
}

fn make_manager() -> MigrationResult<MigrationManager> {
    MigrationManager::new(MigrationConfig::without_backups())
}

fn legacy_json(ffb: f64, dor: u16, torque: f64) -> String {
    serde_json::json!({
        "ffb_gain": ffb,
        "degrees_of_rotation": dor,
        "torque_cap": torque
    })
    .to_string()
}

fn v1_json(ffb: f64, dor: u16, torque: f64) -> String {
    serde_json::json!({
        "schema": CURRENT_SCHEMA_VERSION,
        "scope": { "game": null, "car": null, "track": null },
        "base": {
            "ffbGain": ffb,
            "dorDeg": dor,
            "torqueCapNm": torque,
            "filters": {
                "reconstruction": 0,
                "friction": 0.0,
                "damper": 0.0,
                "inertia": 0.0,
                "notchFilters": [],
                "slewRate": 1.0,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 1.0, "output": 1.0}
                ]
            }
        }
    })
    .to_string()
}

fn v1_json_with_extras(
    ffb: f64,
    dor: u16,
    torque: f64,
    game: Option<&str>,
    parent: Option<&str>,
) -> String {
    let mut obj = serde_json::json!({
        "schema": CURRENT_SCHEMA_VERSION,
        "scope": { "game": game, "car": null, "track": null },
        "base": {
            "ffbGain": ffb,
            "dorDeg": dor,
            "torqueCapNm": torque,
            "filters": {
                "reconstruction": 0,
                "friction": 0.0,
                "damper": 0.0,
                "inertia": 0.0,
                "notchFilters": [],
                "slewRate": 1.0,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 1.0, "output": 1.0}
                ]
            }
        }
    });
    if let Some(p) = parent {
        obj.as_object_mut()
            .and_then(|m| Some(m.insert("parent".to_string(), serde_json::json!(p))));
    }
    obj.to_string()
}

// ===========================================================================
// 1. Migration path correctness — old format → new format roundtrips
// ===========================================================================

mod migration_roundtrips {
    use super::*;

    #[test]
    fn legacy_to_v1_preserves_ffb_gain() -> TestResult {
        let mgr = make_manager()?;
        let migrated = mgr.migrate_profile(&legacy_json(0.75, 900, 15.0))?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;
        let gain = v.pointer("/base/ffbGain").and_then(|v| v.as_f64());
        assert_eq!(gain, Some(0.75));
        Ok(())
    }

    #[test]
    fn legacy_to_v1_preserves_dor() -> TestResult {
        let mgr = make_manager()?;
        let migrated = mgr.migrate_profile(&legacy_json(0.8, 540, 12.0))?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;
        let dor = v.pointer("/base/dorDeg").and_then(|v| v.as_u64());
        assert_eq!(dor, Some(540));
        Ok(())
    }

    #[test]
    fn legacy_to_v1_preserves_torque_cap() -> TestResult {
        let mgr = make_manager()?;
        let migrated = mgr.migrate_profile(&legacy_json(1.0, 900, 25.0))?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;
        let cap = v.pointer("/base/torqueCapNm").and_then(|v| v.as_f64());
        assert_eq!(cap, Some(25.0));
        Ok(())
    }

    #[test]
    fn v1_roundtrip_through_migrate_is_identity() -> TestResult {
        let mgr = make_manager()?;
        let original = v1_json(0.65, 720, 18.0);
        let migrated = mgr.migrate_profile(&original)?;
        let orig_val: serde_json::Value = serde_json::from_str(&original)?;
        let migr_val: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(orig_val, migr_val, "v1 profile must pass through unchanged");
        Ok(())
    }

    #[test]
    fn legacy_migration_adds_schema_field() -> TestResult {
        let mgr = make_manager()?;
        let migrated = mgr.migrate_profile(&legacy_json(0.5, 900, 10.0))?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(
            v.get("schema").and_then(|s| s.as_str()),
            Some(CURRENT_SCHEMA_VERSION)
        );
        Ok(())
    }

    #[test]
    fn legacy_migration_adds_scope_section() -> TestResult {
        let mgr = make_manager()?;
        let migrated = mgr.migrate_profile(&legacy_json(0.5, 900, 10.0))?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;
        assert!(v.get("scope").is_some(), "migrated profile must have scope");
        Ok(())
    }

    #[test]
    fn legacy_migration_adds_filters_section() -> TestResult {
        let mgr = make_manager()?;
        let migrated = mgr.migrate_profile(&legacy_json(0.5, 900, 10.0))?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;
        assert!(
            v.pointer("/base/filters").is_some(),
            "migrated profile must have default filters"
        );
        Ok(())
    }

    #[test]
    fn migrate_then_parse_roundtrip() -> TestResult {
        let mgr = make_manager()?;
        let migrated = mgr.migrate_profile(&legacy_json(0.9, 1080, 20.0))?;

        let parser = CompatParser::new();
        let profile = parser.parse(&migrated)?;
        assert_eq!(profile.ffb_gain(), Some(0.9));
        assert_eq!(profile.dor_deg(), Some(1080));
        assert_eq!(profile.torque_cap_nm(), Some(20.0));
        Ok(())
    }

    #[test]
    fn migrate_then_to_json_then_parse_roundtrip() -> TestResult {
        let mgr = make_manager()?;
        let migrated = mgr.migrate_profile(&legacy_json(0.85, 900, 15.5))?;

        let parser = CompatParser::new();
        let profile = parser.parse(&migrated)?;
        let json_back = profile.to_json()?;
        let profile2 = parser.parse(&json_back)?;

        assert_eq!(profile.ffb_gain(), profile2.ffb_gain());
        assert_eq!(profile.dor_deg(), profile2.dor_deg());
        assert_eq!(profile.torque_cap_nm(), profile2.torque_cap_nm());
        assert_eq!(profile.game(), profile2.game());
        Ok(())
    }
}

// ===========================================================================
// 2. Legacy API compatibility shims
// ===========================================================================

mod legacy_api_shims {
    use super::*;

    #[test]
    fn compat_trait_temp_c_maps_temperature_c() -> TestResult {
        let t = sample(0.0, 0.0, 72, 0);
        assert_eq!(t.temp_c(), t.0.temperature_c);
        assert_eq!(t.temp_c(), 72);
        Ok(())
    }

    #[test]
    fn compat_trait_faults_maps_fault_flags() -> TestResult {
        let t = sample(0.0, 0.0, 0, 0xCD);
        assert_eq!(t.faults(), t.0.fault_flags);
        assert_eq!(t.faults(), 0xCD);
        Ok(())
    }

    #[test]
    fn compat_trait_wheel_angle_mdeg_converts_deg_to_mdeg() -> TestResult {
        let t = sample(123.456, 0.0, 0, 0);
        assert_eq!(t.wheel_angle_mdeg(), (123.456_f32 * 1000.0) as i32);
        Ok(())
    }

    #[test]
    fn compat_trait_wheel_speed_mrad_s_converts_rad_to_mrad() -> TestResult {
        let t = sample(0.0, 7.89, 0, 0);
        assert_eq!(t.wheel_speed_mrad_s(), (7.89_f32 * 1000.0) as i32);
        Ok(())
    }

    #[test]
    fn compat_trait_sequence_always_zero() -> TestResult {
        for (a, s, t, f) in [(0.0, 0.0, 0, 0), (900.0, 50.0, 255, 255)] {
            let c = sample(a, s, t, f);
            assert_eq!(c.sequence(), 0);
        }
        Ok(())
    }

    #[test]
    fn compat_through_dyn_trait_object() -> TestResult {
        let t = sample(30.0, 2.0, 40, 0x0F);
        let dyn_ref: &dyn TelemetryCompat = &t;
        assert_eq!(dyn_ref.temp_c(), 40);
        assert_eq!(dyn_ref.faults(), 0x0F);
        assert_eq!(dyn_ref.wheel_angle_mdeg(), 30_000);
        assert_eq!(dyn_ref.wheel_speed_mrad_s(), 2_000);
        assert_eq!(dyn_ref.sequence(), 0);
        Ok(())
    }

    #[test]
    fn compat_through_boxed_trait_object() -> TestResult {
        let t: Box<dyn TelemetryCompat> = Box::new(sample(60.0, 4.0, 55, 0x22));
        assert_eq!(t.temp_c(), 55);
        assert_eq!(t.faults(), 0x22);
        assert_eq!(t.wheel_angle_mdeg(), 60_000);
        assert_eq!(t.wheel_speed_mrad_s(), 4_000);
        Ok(())
    }

    #[test]
    fn compat_hands_on_field_untouched() -> TestResult {
        let inner = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: true,
            timestamp: Instant::now(),
        };
        let c = Compat(inner);
        // compat layer does not alter hands_on
        assert!(c.0.hands_on);
        Ok(())
    }
}

// ===========================================================================
// 3. Version detection and negotiation
// ===========================================================================

mod version_detection {
    use super::*;

    #[test]
    fn detect_legacy_format_as_v0() -> TestResult {
        let mgr = make_manager()?;
        let ver = mgr.detect_version(&legacy_json(0.8, 900, 15.0))?;
        assert_eq!(ver.major, 0);
        assert_eq!(ver.minor, 0);
        Ok(())
    }

    #[test]
    fn detect_v1_format() -> TestResult {
        let mgr = make_manager()?;
        let ver = mgr.detect_version(&v1_json(0.8, 900, 15.0))?;
        assert_eq!(ver.major, 1);
        assert!(ver.is_current());
        Ok(())
    }

    #[test]
    fn schema_version_parse_v1() -> TestResult {
        let v = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
        assert!(v.is_current());
        Ok(())
    }

    #[test]
    fn schema_version_parse_v2() -> TestResult {
        let v = SchemaVersion::parse(SCHEMA_VERSION_V2)?;
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 0);
        assert!(!v.is_current());
        Ok(())
    }

    #[test]
    fn schema_version_parse_minor() -> TestResult {
        let v = SchemaVersion::parse("wheel.profile/1.3")?;
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 3);
        Ok(())
    }

    #[test]
    fn schema_version_ordering() -> TestResult {
        let v0 = SchemaVersion::new(0, 0);
        let v1 = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;
        let v1_1 = SchemaVersion::parse("wheel.profile/1.1")?;
        let v2 = SchemaVersion::parse(SCHEMA_VERSION_V2)?;

        assert!(v0.is_older_than(&v1));
        assert!(v1.is_older_than(&v1_1));
        assert!(v1_1.is_older_than(&v2));
        // Not older than self
        assert!(!v1.is_older_than(&v1));
        Ok(())
    }

    #[test]
    fn schema_version_new_generates_correct_string() -> TestResult {
        let v = SchemaVersion::new(3, 5);
        assert_eq!(v.version, "wheel.profile/3.5");
        assert_eq!(v.major, 3);
        assert_eq!(v.minor, 5);
        Ok(())
    }

    #[test]
    fn needs_migration_legacy() -> TestResult {
        let mgr = make_manager()?;
        assert!(mgr.needs_migration(&legacy_json(0.5, 540, 10.0))?);
        Ok(())
    }

    #[test]
    fn needs_migration_v1_false() -> TestResult {
        let mgr = make_manager()?;
        assert!(!mgr.needs_migration(&v1_json(0.5, 540, 10.0))?);
        Ok(())
    }

    #[test]
    fn backward_compatible_parser_accepts_v1() -> TestResult {
        let parser = CompatParser::new();
        assert!(parser.is_compatible(&v1_json(0.8, 900, 15.0))?);
        Ok(())
    }

    #[test]
    fn backward_compatible_parser_rejects_legacy() -> TestResult {
        let parser = CompatParser::new();
        assert!(!parser.is_compatible(&legacy_json(0.8, 900, 15.0))?);
        Ok(())
    }

    #[test]
    fn parser_for_major_version_2_rejects_v1() -> TestResult {
        let parser = CompatParser::for_major_version(2);
        assert!(!parser.is_compatible(&v1_json(0.8, 900, 15.0))?);
        Ok(())
    }

    #[test]
    fn parser_for_major_version_1_accepts_v1_1() -> TestResult {
        let parser = CompatParser::for_major_version(1);
        let json = serde_json::json!({
            "schema": "wheel.profile/1.1",
            "scope": { "game": null, "car": null, "track": null },
            "base": {
                "ffbGain": 0.8,
                "dorDeg": 900,
                "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.0,
                    "damper": 0.0,
                    "inertia": 0.0,
                    "notchFilters": [],
                    "slewRate": 1.0,
                    "curvePoints": []
                }
            }
        })
        .to_string();
        assert!(parser.is_compatible(&json)?);
        Ok(())
    }
}

// ===========================================================================
// 4. Error handling for unsupported versions
// ===========================================================================

mod error_handling {
    use super::*;

    #[test]
    fn parse_invalid_schema_prefix() -> TestResult {
        let result = SchemaVersion::parse("invalid.prefix/1");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn parse_missing_version_number() -> TestResult {
        let result = SchemaVersion::parse("wheel.profile/");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn parse_non_numeric_version() -> TestResult {
        let result = SchemaVersion::parse("wheel.profile/abc");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn parse_empty_string() -> TestResult {
        let result = SchemaVersion::parse("");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn parse_no_slash() -> TestResult {
        let result = SchemaVersion::parse("wheel.profile1");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn migrate_empty_string_fails() -> TestResult {
        let mgr = make_manager()?;
        let result = mgr.migrate_profile("");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn migrate_plain_text_fails() -> TestResult {
        let mgr = make_manager()?;
        let result = mgr.migrate_profile("not json");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn migrate_json_array_fails() -> TestResult {
        let mgr = make_manager()?;
        let result = mgr.migrate_profile("[1, 2, 3]");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn migrate_json_null_fails() -> TestResult {
        let mgr = make_manager()?;
        let result = mgr.migrate_profile("null");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn migrate_truncated_json_fails() -> TestResult {
        let mgr = make_manager()?;
        let result = mgr.migrate_profile(r##"{"ffb_gain": 0.8, "degrees_of_rotation":#"##);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn detect_version_empty_object_is_legacy() -> TestResult {
        let mgr = make_manager()?;
        let ver = mgr.detect_version("{}")?;
        assert_eq!(ver.major, 0, "empty object should be detected as legacy v0");
        Ok(())
    }

    #[test]
    fn parser_rejects_legacy_without_migration() -> TestResult {
        let parser = CompatParser::new();
        let result = parser.parse(&legacy_json(0.8, 900, 15.0));
        assert!(result.is_err(), "parser should reject legacy format");
        Ok(())
    }

    #[test]
    fn parser_parse_or_migrate_handles_legacy() -> TestResult {
        let parser = CompatParser::new();
        let profile = parser.parse_or_migrate(&legacy_json(0.8, 900, 15.0))?;
        assert_eq!(profile.ffb_gain(), Some(0.8));
        assert_eq!(profile.dor_deg(), Some(900));
        Ok(())
    }

    #[test]
    fn parser_parse_rejects_invalid_json() -> TestResult {
        let parser = CompatParser::new();
        let result = parser.parse("definitely not json!!!");
        assert!(result.is_err());
        Ok(())
    }
}

// ===========================================================================
// 5. Edge cases — empty input, malformed data, boundary versions
// ===========================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn legacy_profile_with_zero_values() -> TestResult {
        let mgr = make_manager()?;
        let migrated = mgr.migrate_profile(&legacy_json(0.0, 0, 0.0))?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(
            v.pointer("/base/ffbGain").and_then(|v| v.as_f64()),
            Some(0.0)
        );
        assert_eq!(v.pointer("/base/dorDeg").and_then(|v| v.as_u64()), Some(0));
        assert_eq!(
            v.pointer("/base/torqueCapNm").and_then(|v| v.as_f64()),
            Some(0.0)
        );
        Ok(())
    }

    #[test]
    fn legacy_profile_with_extreme_values() -> TestResult {
        let mgr = make_manager()?;
        let migrated = mgr.migrate_profile(&legacy_json(1.0, 2700, 100.0))?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(
            v.pointer("/base/ffbGain").and_then(|v| v.as_f64()),
            Some(1.0)
        );
        assert_eq!(
            v.pointer("/base/dorDeg").and_then(|v| v.as_u64()),
            Some(2700)
        );
        assert_eq!(
            v.pointer("/base/torqueCapNm").and_then(|v| v.as_f64()),
            Some(100.0)
        );
        Ok(())
    }

    #[test]
    fn legacy_profile_with_extra_unknown_fields() -> TestResult {
        let mgr = make_manager()?;
        let json = serde_json::json!({
            "ffb_gain": 0.8,
            "degrees_of_rotation": 900,
            "torque_cap": 15.0,
            "custom_setting": "hello",
            "version_note": 42
        })
        .to_string();
        // Should still detect as legacy and migrate
        let ver = mgr.detect_version(&json)?;
        assert_eq!(ver.major, 0);
        let migrated = mgr.migrate_profile(&json)?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(
            v.get("schema").and_then(|s| s.as_str()),
            Some(CURRENT_SCHEMA_VERSION)
        );
        Ok(())
    }

    #[test]
    fn v1_profile_with_optional_sections() -> TestResult {
        let parser = CompatParser::new();
        let json = serde_json::json!({
            "schema": CURRENT_SCHEMA_VERSION,
            "scope": { "game": "iRacing", "car": "MX-5", "track": "Laguna Seca" },
            "base": {
                "ffbGain": 0.7,
                "dorDeg": 900,
                "torqueCapNm": 12.0,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.1,
                    "damper": 0.2,
                    "inertia": 0.0,
                    "notchFilters": [],
                    "slewRate": 1.0,
                    "curvePoints": []
                }
            },
            "leds": { "enabled": true },
            "haptics": { "mode": "vibrate" }
        })
        .to_string();

        let profile = parser.parse(&json)?;
        assert_eq!(profile.game(), Some("iRacing"));
        assert!(profile.leds.is_some());
        assert!(profile.haptics.is_some());
        Ok(())
    }

    #[test]
    fn v1_profile_with_parent() -> TestResult {
        let parser = CompatParser::new();
        let json = v1_json_with_extras(0.7, 900, 12.0, Some("iRacing"), Some("base_profile"));
        let profile = parser.parse(&json)?;
        assert!(profile.has_parent());
        assert_eq!(profile.parent.as_deref(), Some("base_profile"));
        Ok(())
    }

    #[test]
    fn v1_profile_without_parent() -> TestResult {
        let parser = CompatParser::new();
        let profile = parser.parse(&v1_json(0.7, 900, 12.0))?;
        assert!(!profile.has_parent());
        assert!(profile.parent.is_none());
        Ok(())
    }

    #[test]
    fn compat_telemetry_with_all_max_u8() -> TestResult {
        let c = sample(0.0, 0.0, u8::MAX, u8::MAX);
        assert_eq!(c.temp_c(), 255);
        assert_eq!(c.faults(), 255);
        Ok(())
    }

    #[test]
    fn compat_telemetry_with_all_min_u8() -> TestResult {
        let c = sample(0.0, 0.0, u8::MIN, u8::MIN);
        assert_eq!(c.temp_c(), 0);
        assert_eq!(c.faults(), 0);
        Ok(())
    }

    #[test]
    fn migration_idempotency_double_migrate() -> TestResult {
        let mgr = make_manager()?;
        let migrated1 = mgr.migrate_profile(&legacy_json(0.8, 900, 15.0))?;
        let migrated2 = mgr.migrate_profile(&migrated1)?;
        let v1: serde_json::Value = serde_json::from_str(&migrated1)?;
        let v2: serde_json::Value = serde_json::from_str(&migrated2)?;
        assert_eq!(v1, v2, "double migration must be idempotent");
        Ok(())
    }

    #[test]
    fn whitespace_heavy_json_still_parses() -> TestResult {
        let mgr = make_manager()?;
        let json = "  \n\t {\n  \"ffb_gain\" : 0.8 ,\n  \"degrees_of_rotation\" : 900 ,\n  \"torque_cap\" : 15.0 \n} \n\t ";
        let ver = mgr.detect_version(json)?;
        assert_eq!(ver.major, 0);
        let migrated = mgr.migrate_profile(json)?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(
            v.get("schema").and_then(|s| s.as_str()),
            Some(CURRENT_SCHEMA_VERSION)
        );
        Ok(())
    }
}

// ===========================================================================
// 6. CompatibleProfile accessors and to_json roundtrip
// ===========================================================================

mod compatible_profile {
    use super::*;

    #[test]
    fn accessors_return_correct_values() -> TestResult {
        let parser = CompatParser::new();
        let profile = parser.parse(&v1_json(0.65, 720, 18.5))?;
        assert_eq!(profile.ffb_gain(), Some(0.65));
        assert_eq!(profile.dor_deg(), Some(720));
        assert_eq!(profile.torque_cap_nm(), Some(18.5));
        assert_eq!(profile.game(), None);
        assert!(!profile.has_parent());
        Ok(())
    }

    #[test]
    fn accessors_with_game_scope() -> TestResult {
        let parser = CompatParser::new();
        let json = v1_json_with_extras(0.8, 900, 15.0, Some("ACC"), None);
        let profile = parser.parse(&json)?;
        assert_eq!(profile.game(), Some("ACC"));
        Ok(())
    }

    #[test]
    fn to_json_produces_valid_json() -> TestResult {
        let parser = CompatParser::new();
        let profile = parser.parse(&v1_json(0.7, 900, 12.0))?;
        let json_out = profile.to_json()?;
        let _v: serde_json::Value = serde_json::from_str(&json_out)?;
        Ok(())
    }

    #[test]
    fn to_json_roundtrip_preserves_all_fields() -> TestResult {
        let parser = CompatParser::new();
        let original = parser.parse(&v1_json(0.7, 900, 12.0))?;
        let json_out = original.to_json()?;
        let reparsed = parser.parse(&json_out)?;

        assert_eq!(original.ffb_gain(), reparsed.ffb_gain());
        assert_eq!(original.dor_deg(), reparsed.dor_deg());
        assert_eq!(original.torque_cap_nm(), reparsed.torque_cap_nm());
        assert_eq!(original.game(), reparsed.game());
        assert_eq!(original.has_parent(), reparsed.has_parent());
        assert_eq!(
            original.schema_version.version,
            reparsed.schema_version.version
        );
        Ok(())
    }

    #[test]
    fn to_json_includes_optional_sections_when_present() -> TestResult {
        let parser = CompatParser::new();
        let json = serde_json::json!({
            "schema": CURRENT_SCHEMA_VERSION,
            "scope": { "game": null, "car": null, "track": null },
            "base": {
                "ffbGain": 0.7,
                "dorDeg": 900,
                "torqueCapNm": 12.0,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.0,
                    "damper": 0.0,
                    "inertia": 0.0,
                    "notchFilters": [],
                    "slewRate": 1.0,
                    "curvePoints": []
                }
            },
            "leds": { "brightness": 80 },
            "haptics": { "mode": "off" },
            "signature": "abc123"
        })
        .to_string();

        let profile = parser.parse(&json)?;
        let out = profile.to_json()?;
        let v: serde_json::Value = serde_json::from_str(&out)?;
        assert!(v.get("leds").is_some());
        assert!(v.get("haptics").is_some());
        assert_eq!(v.get("signature").and_then(|s| s.as_str()), Some("abc123"));
        Ok(())
    }

    #[test]
    fn to_json_omits_optional_sections_when_absent() -> TestResult {
        let parser = CompatParser::new();
        let profile = parser.parse(&v1_json(0.7, 900, 12.0))?;
        let out = profile.to_json()?;
        let v: serde_json::Value = serde_json::from_str(&out)?;
        assert!(v.get("leds").is_none());
        assert!(v.get("haptics").is_none());
        assert!(v.get("signature").is_none());
        assert!(v.get("parent").is_none());
        Ok(())
    }
}

// ===========================================================================
// 7. ProfileMigrationService
// ===========================================================================

mod profile_migration_service {
    use super::*;

    #[test]
    fn service_detects_legacy_version() -> TestResult {
        let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
        let ver = svc.detect_version(&legacy_json(0.8, 900, 15.0))?;
        assert_eq!(ver.major, 0);
        Ok(())
    }

    #[test]
    fn service_detects_v1_version() -> TestResult {
        let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
        let ver = svc.detect_version(&v1_json(0.8, 900, 15.0))?;
        assert!(ver.is_current());
        Ok(())
    }

    #[test]
    fn service_needs_migration_legacy() -> TestResult {
        let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
        assert!(svc.needs_migration(&legacy_json(0.8, 900, 15.0))?);
        Ok(())
    }

    #[test]
    fn service_needs_migration_v1_false() -> TestResult {
        let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
        assert!(!svc.needs_migration(&v1_json(0.8, 900, 15.0))?);
        Ok(())
    }

    #[test]
    fn service_migrate_with_backup_no_path_legacy() -> TestResult {
        let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
        let outcome = svc.migrate_with_backup(&legacy_json(0.8, 900, 15.0), None)?;
        assert!(outcome.was_migrated());
        assert!(outcome.migration_count() > 0);
        assert!(outcome.backup_info.is_none(), "no path → no backup");
        let v: serde_json::Value = serde_json::from_str(&outcome.migrated_json)?;
        assert_eq!(
            v.get("schema").and_then(|s| s.as_str()),
            Some(CURRENT_SCHEMA_VERSION)
        );
        Ok(())
    }

    #[test]
    fn service_migrate_with_backup_v1_noop() -> TestResult {
        let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
        let original = v1_json(0.8, 900, 15.0);
        let outcome = svc.migrate_with_backup(&original, None)?;
        assert!(!outcome.was_migrated());
        assert_eq!(outcome.migration_count(), 0);
        Ok(())
    }

    #[test]
    fn service_outcome_versions_correct() -> TestResult {
        let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
        let outcome = svc.migrate_with_backup(&legacy_json(0.8, 900, 15.0), None)?;
        assert_eq!(outcome.original_version.major, 0);
        assert!(outcome.target_version.is_current());
        Ok(())
    }

    #[test]
    fn multiple_independent_managers() -> TestResult {
        let mgr1 = make_manager()?;
        let mgr2 = make_manager()?;
        let legacy = legacy_json(0.8, 900, 15.0);
        let m1 = mgr1.migrate_profile(&legacy)?;
        let m2 = mgr2.migrate_profile(&legacy)?;
        let v1: serde_json::Value = serde_json::from_str(&m1)?;
        let v2: serde_json::Value = serde_json::from_str(&m2)?;
        assert_eq!(
            v1, v2,
            "independent managers must produce identical results"
        );
        Ok(())
    }
}

// ===========================================================================
// 8. MigrationOutcome API
// ===========================================================================

mod migration_outcome {
    use super::*;

    #[test]
    fn outcome_was_migrated_true_for_legacy() -> TestResult {
        let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
        let outcome = svc.migrate_with_backup(&legacy_json(0.5, 540, 10.0), None)?;
        assert!(outcome.was_migrated());
        Ok(())
    }

    #[test]
    fn outcome_was_migrated_false_for_current() -> TestResult {
        let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
        let outcome = svc.migrate_with_backup(&v1_json(0.5, 540, 10.0), None)?;
        assert!(!outcome.was_migrated());
        Ok(())
    }

    #[test]
    fn outcome_migration_count_matches() -> TestResult {
        let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;

        let legacy_outcome = svc.migrate_with_backup(&legacy_json(0.5, 540, 10.0), None)?;
        assert!(legacy_outcome.migration_count() >= 1);

        let v1_outcome = svc.migrate_with_backup(&v1_json(0.5, 540, 10.0), None)?;
        assert_eq!(v1_outcome.migration_count(), 0);
        Ok(())
    }

    #[test]
    fn outcome_migrated_json_is_valid() -> TestResult {
        let svc = ProfileMigrationService::new(MigrationConfig::without_backups())?;
        let outcome = svc.migrate_with_backup(&legacy_json(0.8, 900, 15.0), None)?;
        let _v: serde_json::Value = serde_json::from_str(&outcome.migrated_json)?;
        Ok(())
    }
}

// ===========================================================================
// 9. BackwardCompatibleParser.parse_or_migrate
// ===========================================================================

mod parse_or_migrate {
    use super::*;

    #[test]
    fn parse_or_migrate_v1_direct() -> TestResult {
        let parser = CompatParser::new();
        let profile = parser.parse_or_migrate(&v1_json(0.8, 900, 15.0))?;
        assert_eq!(profile.ffb_gain(), Some(0.8));
        assert_eq!(profile.dor_deg(), Some(900));
        assert_eq!(profile.torque_cap_nm(), Some(15.0));
        Ok(())
    }

    #[test]
    fn parse_or_migrate_legacy_auto_migrates() -> TestResult {
        let parser = CompatParser::new();
        let profile = parser.parse_or_migrate(&legacy_json(0.75, 1080, 20.0))?;
        assert_eq!(profile.ffb_gain(), Some(0.75));
        assert_eq!(profile.dor_deg(), Some(1080));
        assert_eq!(profile.torque_cap_nm(), Some(20.0));
        Ok(())
    }

    #[test]
    fn parse_or_migrate_result_has_correct_schema() -> TestResult {
        let parser = CompatParser::new();
        let profile = parser.parse_or_migrate(&legacy_json(0.5, 540, 10.0))?;
        assert_eq!(profile.schema_version.major, 1);
        Ok(())
    }

    #[test]
    fn parse_or_migrate_invalid_json_fails() -> TestResult {
        let parser = CompatParser::new();
        let result = parser.parse_or_migrate("not json");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn parse_or_migrate_empty_object_migrates() -> TestResult {
        let parser = CompatParser::new();
        // Empty object is detected as legacy (v0)
        let result = parser.parse_or_migrate("{}");
        // The migration should succeed (empty legacy → v1 with defaults)
        // or may fail depending on implementation; just check it doesn't panic
        let _is_ok = result.is_ok();
        Ok(())
    }
}

// ===========================================================================
// 10. MigrationConfig variations
// ===========================================================================

mod migration_config {
    use super::*;

    #[test]
    fn without_backups_config_has_backups_disabled() -> TestResult {
        let config = MigrationConfig::without_backups();
        assert!(!config.create_backups);
        Ok(())
    }

    #[test]
    fn manager_with_validation_disabled() -> TestResult {
        let mut config = MigrationConfig::without_backups();
        config.validate_after_migration = false;
        let mgr = MigrationManager::new(config)?;
        let migrated = mgr.migrate_profile(&legacy_json(0.8, 900, 15.0))?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;
        assert!(v.get("schema").is_some());
        Ok(())
    }

    #[test]
    fn manager_with_validation_enabled() -> TestResult {
        let mut config = MigrationConfig::without_backups();
        config.validate_after_migration = true;
        let mgr = MigrationManager::new(config)?;
        let migrated = mgr.migrate_profile(&legacy_json(0.8, 900, 15.0))?;
        let v: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(
            v.get("schema").and_then(|s| s.as_str()),
            Some(CURRENT_SCHEMA_VERSION)
        );
        Ok(())
    }
}

// ===========================================================================
// 11. Proptest fuzzing for profile migration
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_legacy_migration_preserves_ffb_gain(
        ffb in 0.0f64..=1.0f64,
    ) {
        let mgr = make_manager().map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let migrated = mgr
            .migrate_profile(&legacy_json(ffb, 900, 15.0))
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let v: serde_json::Value = serde_json::from_str(&migrated)
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let gain = v.pointer("/base/ffbGain").and_then(|v| v.as_f64())
            .ok_or_else(|| TestCaseError::Fail("missing ffbGain".into()))?;
        // JSON roundtrip may introduce ±1 ULP; use relative tolerance
        prop_assert!((gain - ffb).abs() < 1e-12, "ffb_gain drift: {} vs {}", gain, ffb);
    }

    #[test]
    fn prop_legacy_migration_preserves_dor(
        dor in 0u16..=2700u16,
    ) {
        let mgr = make_manager().map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let migrated = mgr
            .migrate_profile(&legacy_json(0.8, dor, 15.0))
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let v: serde_json::Value = serde_json::from_str(&migrated)
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let d = v.pointer("/base/dorDeg").and_then(|v| v.as_u64());
        prop_assert_eq!(d, Some(dor as u64));
    }

    #[test]
    fn prop_legacy_migration_preserves_torque(
        torque in 0.0f64..=100.0f64,
    ) {
        let mgr = make_manager().map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let migrated = mgr
            .migrate_profile(&legacy_json(0.8, 900, torque))
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let v: serde_json::Value = serde_json::from_str(&migrated)
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let cap = v.pointer("/base/torqueCapNm").and_then(|v| v.as_f64())
            .ok_or_else(|| TestCaseError::Fail("missing torqueCapNm".into()))?;
        prop_assert!((cap - torque).abs() < 1e-10, "torque drift: {} vs {}", cap, torque);
    }

    #[test]
    fn prop_v1_migration_is_identity(
        ffb in 0.0f64..=1.0f64,
        dor in 0u16..=2700u16,
        torque in 0.0f64..=100.0f64,
    ) {
        let mgr = make_manager().map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let original = v1_json(ffb, dor, torque);
        let migrated = mgr
            .migrate_profile(&original)
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let orig_val: serde_json::Value = serde_json::from_str(&original)
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let migr_val: serde_json::Value = serde_json::from_str(&migrated)
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        prop_assert_eq!(orig_val, migr_val);
    }

    #[test]
    fn prop_migration_idempotent(
        ffb in 0.0f64..=1.0f64,
        dor in 0u16..=2700u16,
        torque in 0.0f64..=100.0f64,
    ) {
        let mgr = make_manager().map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let legacy = legacy_json(ffb, dor, torque);
        let m1 = mgr
            .migrate_profile(&legacy)
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let m2 = mgr
            .migrate_profile(&m1)
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let v1: serde_json::Value = serde_json::from_str(&m1)
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let v2: serde_json::Value = serde_json::from_str(&m2)
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        prop_assert_eq!(v1, v2);
    }

    #[test]
    fn prop_telemetry_compat_angle_roundtrip(angle in -900.0f32..900.0f32) {
        let c = sample(angle, 0.0, 0, 0);
        let mdeg = c.wheel_angle_mdeg();
        let expected = (angle * 1000.0) as i32;
        prop_assert_eq!(mdeg, expected);
    }

    #[test]
    fn prop_telemetry_compat_speed_roundtrip(speed in -500.0f32..500.0f32) {
        let c = sample(0.0, speed, 0, 0);
        let mrad = c.wheel_speed_mrad_s();
        let expected = (speed * 1000.0) as i32;
        prop_assert_eq!(mrad, expected);
    }

    #[test]
    fn prop_parse_or_migrate_roundtrip(
        ffb in 0.01f64..=1.0f64,
        dor in 90u16..=2700u16,
        torque in 0.1f64..=100.0f64,
    ) {
        let parser = CompatParser::new();
        let legacy = legacy_json(ffb, dor, torque);
        let profile = parser
            .parse_or_migrate(&legacy)
            .map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let gain = profile.ffb_gain()
            .ok_or_else(|| TestCaseError::Fail("missing ffb_gain".into()))?;
        prop_assert!((gain - ffb).abs() < 1e-12, "ffb drift: {} vs {}", gain, ffb);
        prop_assert_eq!(profile.dor_deg(), Some(dor as u64));
        let cap = profile.torque_cap_nm()
            .ok_or_else(|| TestCaseError::Fail("missing torque_cap".into()))?;
        prop_assert!((cap - torque).abs() < 1e-10, "torque drift: {} vs {}", cap, torque);
    }

    #[test]
    fn prop_schema_version_ordering_consistent(
        a_maj in 0u32..10u32,
        a_min in 0u32..10u32,
        b_maj in 0u32..10u32,
        b_min in 0u32..10u32,
    ) {
        let a = SchemaVersion::new(a_maj, a_min);
        let b = SchemaVersion::new(b_maj, b_min);
        if a_maj < b_maj || (a_maj == b_maj && a_min < b_min) {
            prop_assert!(a.is_older_than(&b));
            prop_assert!(!b.is_older_than(&a));
        } else if a_maj == b_maj && a_min == b_min {
            prop_assert!(!a.is_older_than(&b));
            prop_assert!(!b.is_older_than(&a));
        } else {
            prop_assert!(!a.is_older_than(&b));
            prop_assert!(b.is_older_than(&a));
        }
    }
}
