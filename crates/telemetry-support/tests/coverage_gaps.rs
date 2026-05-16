//! Additional coverage tests for `racing-wheel-telemetry-support`.
//!
//! Pins behaviour the existing suite did not assert:
//!
//! * `normalize_game_id` mixed-case, dash, and space aliases for `ea_wrc`
//!   and `f1_2025`.
//! * `normalize_game_id` trim + alias interaction (whitespace around a
//!   mixed-case alias).
//! * `GameSupportStatus` YAML decoding: uppercase / mixed-case rejected
//!   (under `rename_all = "lowercase"`), unknown values rejected.
//! * `GameSupportMatrix::game_ids_by_status` returns a sorted list even
//!   when entries were inserted out of order.
//! * `matrix_game_ids` produces no duplicates.
//! * `GameSupportStatus` `Copy` semantics and `Default == Stable`.
//! * `TelemetryFieldMapping::clone` preserves all eight `Option<String>`
//!   fields verbatim.
//! * `TelemetrySupport.supports_360hz_option = true` with no
//!   `high_rate_update_rate_hz` value still parses (invariant lives in
//!   tests, not the type).
//! * `output_target: ""` decodes to `Some("".to_string())` (distinct from
//!   `None`).

use racing_wheel_telemetry_support::{
    AutoDetectConfig, GameSupport, GameSupportMatrix, GameSupportStatus, GameVersion,
    TelemetryFieldMapping, TelemetrySupport, load_default_matrix, matrix_game_ids,
    normalize_game_id,
};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// normalize_game_id alias coverage
// ---------------------------------------------------------------------------

#[test]
fn normalize_game_id_ea_wrc_dash_space_and_underscore_variants() {
    for variant in ["ea_wrc", "ea-wrc", "ea wrc"] {
        assert_eq!(normalize_game_id(variant), "eawrc", "variant {variant}");
    }
}

#[test]
fn normalize_game_id_ea_wrc_mixed_case() {
    for variant in ["EA-WRC", "Ea Wrc", "eA_wRc", "EA_WRC"] {
        assert_eq!(normalize_game_id(variant), "eawrc", "variant {variant}");
    }
}

#[test]
fn normalize_game_id_f1_2025_variants() {
    for variant in ["f1_2025", "f1-2025", "f1 2025", "F1-2025", "F1 2025"] {
        assert_eq!(normalize_game_id(variant), "f1_25", "variant {variant}");
    }
}

#[test]
fn normalize_game_id_trim_combined_with_mixed_case_alias() {
    assert_eq!(normalize_game_id("  Ea-Wrc\t"), "eawrc");
    assert_eq!(normalize_game_id(" F1-2025 "), "f1_25");
}

#[test]
fn normalize_game_id_returns_input_unchanged_when_not_alias() {
    assert_eq!(normalize_game_id("iracing"), "iracing");
    assert_eq!(normalize_game_id("ACC"), "ACC");
}

#[test]
fn normalize_game_id_trims_whitespace_for_non_aliases() {
    assert_eq!(normalize_game_id(" iracing "), "iracing");
    assert_eq!(normalize_game_id("\tACC\t"), "ACC");
}

// ---------------------------------------------------------------------------
// GameSupportStatus YAML decoding (rename_all = "lowercase")
// ---------------------------------------------------------------------------

#[test]
fn game_support_status_yaml_decodes_lowercase() -> Result<(), Box<dyn std::error::Error>> {
    let stable: GameSupportStatus = serde_yaml::from_str("stable")?;
    let experimental: GameSupportStatus = serde_yaml::from_str("experimental")?;
    assert_eq!(stable, GameSupportStatus::Stable);
    assert_eq!(experimental, GameSupportStatus::Experimental);
    Ok(())
}

#[test]
fn game_support_status_yaml_rejects_capitalized() {
    let result: Result<GameSupportStatus, _> = serde_yaml::from_str("Stable");
    assert!(
        result.is_err(),
        "rename_all=\"lowercase\" must reject 'Stable'"
    );

    let result: Result<GameSupportStatus, _> = serde_yaml::from_str("STABLE");
    assert!(result.is_err());
}

#[test]
fn game_support_status_yaml_rejects_unknown_value() {
    let result: Result<GameSupportStatus, _> = serde_yaml::from_str("deprecated");
    assert!(result.is_err());

    let result: Result<GameSupportStatus, _> = serde_yaml::from_str("retired");
    assert!(result.is_err());
}

#[test]
fn game_support_status_default_is_stable() {
    assert_eq!(GameSupportStatus::default(), GameSupportStatus::Stable);
}

#[test]
fn game_support_status_copy_semantics() {
    let a = GameSupportStatus::Experimental;
    let b = a; // Copy, no move
    assert_eq!(a, b);
    let dbg = format!("{a:?}");
    assert!(dbg.contains("Experimental"));
}

// ---------------------------------------------------------------------------
// GameSupportMatrix::game_ids_by_status sorting with out-of-order inserts
// ---------------------------------------------------------------------------

fn empty_telemetry() -> TelemetrySupport {
    TelemetrySupport {
        method: "udp".to_string(),
        update_rate_hz: 60,
        supports_360hz_option: false,
        high_rate_update_rate_hz: None,
        output_target: None,
        fields: TelemetryFieldMapping {
            ffb_scalar: None,
            rpm: None,
            speed_ms: None,
            slip_ratio: None,
            gear: None,
            flags: None,
            car_id: None,
            track_id: None,
        },
    }
}

fn empty_auto_detect() -> AutoDetectConfig {
    AutoDetectConfig {
        process_names: vec![],
        install_registry_keys: vec![],
        install_paths: vec![],
    }
}

fn fake_game(status: GameSupportStatus) -> GameSupport {
    GameSupport {
        name: "fake".to_string(),
        versions: vec![GameVersion {
            version: "1".to_string(),
            config_paths: vec![],
            executable_patterns: vec![],
            telemetry_method: "udp".to_string(),
            supported_fields: vec![],
        }],
        telemetry: empty_telemetry(),
        status,
        config_writer: "noop".to_string(),
        auto_detect: empty_auto_detect(),
    }
}

#[test]
fn game_ids_by_status_returns_sorted_for_out_of_order_inserts() {
    let mut games: HashMap<String, GameSupport> = HashMap::new();
    // Insert in non-sorted order; HashMap iteration order is unspecified, so
    // the only contract we can pin is that the returned vec is sorted.
    games.insert("zzz".to_string(), fake_game(GameSupportStatus::Stable));
    games.insert("aaa".to_string(), fake_game(GameSupportStatus::Stable));
    games.insert("mmm".to_string(), fake_game(GameSupportStatus::Experimental));
    let matrix = GameSupportMatrix { games };

    let stable = matrix.game_ids_by_status(GameSupportStatus::Stable);
    assert_eq!(stable, vec!["aaa".to_string(), "zzz".to_string()]);

    let experimental = matrix.game_ids_by_status(GameSupportStatus::Experimental);
    assert_eq!(experimental, vec!["mmm".to_string()]);

    let all_ids = matrix.game_ids();
    assert_eq!(
        all_ids,
        vec!["aaa".to_string(), "mmm".to_string(), "zzz".to_string()]
    );

    assert!(matrix.has_game_id("aaa"));
    assert!(!matrix.has_game_id("nope"));
}

// ---------------------------------------------------------------------------
// matrix_game_ids: no duplicates
// ---------------------------------------------------------------------------

#[test]
fn matrix_game_ids_has_no_duplicates() -> Result<(), Box<dyn std::error::Error>> {
    let ids = matrix_game_ids()?;
    let set: HashSet<&String> = ids.iter().collect();
    assert_eq!(
        set.len(),
        ids.len(),
        "matrix_game_ids must be unique; vec={ids:?}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// TelemetryFieldMapping clone fidelity
// ---------------------------------------------------------------------------

#[test]
fn telemetry_field_mapping_clone_preserves_mixed_options() {
    let m = TelemetryFieldMapping {
        ffb_scalar: Some("ffb".to_string()),
        rpm: None,
        speed_ms: Some("v".to_string()),
        slip_ratio: None,
        gear: Some("g".to_string()),
        flags: None,
        car_id: Some("car".to_string()),
        track_id: None,
    };
    let c = m.clone();
    assert_eq!(c.ffb_scalar, m.ffb_scalar);
    assert_eq!(c.rpm, m.rpm);
    assert_eq!(c.speed_ms, m.speed_ms);
    assert_eq!(c.slip_ratio, m.slip_ratio);
    assert_eq!(c.gear, m.gear);
    assert_eq!(c.flags, m.flags);
    assert_eq!(c.car_id, m.car_id);
    assert_eq!(c.track_id, m.track_id);
}

// ---------------------------------------------------------------------------
// 360 Hz invariant lives in tests, not the type
// ---------------------------------------------------------------------------

#[test]
fn telemetry_support_360hz_option_without_high_rate_parses_and_returns_false_check() -> Result<(), Box<dyn std::error::Error>> {
    let yaml = r#"
method: "udp"
update_rate_hz: 60
supports_360hz_option: true
output_target: null
fields:
  ffb_scalar: null
  rpm: null
  speed_ms: null
  slip_ratio: null
  gear: null
  flags: null
  car_id: null
  track_id: null
"#;
    let ts: TelemetrySupport = serde_yaml::from_str(yaml)?;
    // The crate intentionally allows this combination: the invariant
    // (option implies a rate) is enforced by tests, not by the type. The
    // companion helper using `is_some_and(...)` therefore returns `false`.
    assert!(ts.supports_360hz_option);
    assert!(ts.high_rate_update_rate_hz.is_none());
    // `is_some_and` on `None` must return `false`; equivalent to
    // `is_none_or` of the negated predicate but expressed positively here
    // so the assertion reads naturally.
    let observed_hz = ts.high_rate_update_rate_hz.is_some_and(|hz| hz > 0);
    assert!(!observed_hz);
    Ok(())
}

#[test]
fn telemetry_support_high_rate_value_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let yaml = r#"
method: "udp"
update_rate_hz: 60
supports_360hz_option: true
high_rate_update_rate_hz: 360
output_target: "to-some-stream"
fields:
  ffb_scalar: "f"
  rpm: null
  speed_ms: null
  slip_ratio: null
  gear: null
  flags: null
  car_id: null
  track_id: null
"#;
    let ts: TelemetrySupport = serde_yaml::from_str(yaml)?;
    assert_eq!(ts.high_rate_update_rate_hz, Some(360));
    assert_eq!(ts.output_target.as_deref(), Some("to-some-stream"));
    Ok(())
}

// ---------------------------------------------------------------------------
// output_target empty string vs None
// ---------------------------------------------------------------------------

#[test]
fn output_target_empty_string_is_distinct_from_none() -> Result<(), Box<dyn std::error::Error>> {
    let yaml_empty = r#"
method: "udp"
update_rate_hz: 60
output_target: ""
fields:
  ffb_scalar: null
  rpm: null
  speed_ms: null
  slip_ratio: null
  gear: null
  flags: null
  car_id: null
  track_id: null
"#;
    let ts_empty: TelemetrySupport = serde_yaml::from_str(yaml_empty)?;
    assert_eq!(ts_empty.output_target.as_deref(), Some(""));

    let yaml_null = r#"
method: "udp"
update_rate_hz: 60
output_target: null
fields:
  ffb_scalar: null
  rpm: null
  speed_ms: null
  slip_ratio: null
  gear: null
  flags: null
  car_id: null
  track_id: null
"#;
    let ts_null: TelemetrySupport = serde_yaml::from_str(yaml_null)?;
    assert!(ts_null.output_target.is_none());
    Ok(())
}

// ---------------------------------------------------------------------------
// load_default_matrix is internally consistent
// ---------------------------------------------------------------------------

#[test]
fn load_default_matrix_keys_equal_game_ids() -> Result<(), Box<dyn std::error::Error>> {
    let matrix = load_default_matrix()?;
    let mut from_keys: Vec<String> = matrix.games.keys().cloned().collect();
    from_keys.sort_unstable();
    let from_method = matrix.game_ids();
    assert_eq!(from_keys, from_method);
    Ok(())
}
