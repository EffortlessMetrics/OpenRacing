//! Deep tests for the telemetry-support crate.
//!
//! Covers helper utilities, byte-level validation, format conversion helpers,
//! and extended matrix validation.

use racing_wheel_telemetry_support::{
    GameSupportMatrix, GameSupportStatus, load_default_matrix, matrix_game_id_set, matrix_game_ids,
    normalize_game_id,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helper utilities testing
// ---------------------------------------------------------------------------

#[test]
fn normalize_game_id_handles_all_known_aliases() {
    assert_eq!(normalize_game_id("ea_wrc"), "eawrc");
    assert_eq!(normalize_game_id("EA_WRC"), "eawrc");
    assert_eq!(normalize_game_id("Ea_Wrc"), "eawrc");
    assert_eq!(normalize_game_id("f1_2025"), "f1_25");
    assert_eq!(normalize_game_id("F1_2025"), "f1_25");
}

#[test]
fn normalize_game_id_passthrough_for_non_aliased() {
    assert_eq!(normalize_game_id("iracing"), "iracing");
    assert_eq!(normalize_game_id("acc"), "acc");
    assert_eq!(normalize_game_id("f1_25"), "f1_25");
    assert_eq!(normalize_game_id(""), "");
    assert_eq!(normalize_game_id("unknown_game_xyz"), "unknown_game_xyz");
}

#[test]
fn game_id_set_matches_game_ids_vector() -> TestResult {
    let ids_vec = matrix_game_ids()?;
    let ids_set = matrix_game_id_set()?;

    assert_eq!(ids_vec.len(), ids_set.len());
    for id in &ids_vec {
        assert!(ids_set.contains(id), "set missing id: {id}");
    }
    Ok(())
}

#[test]
fn game_ids_are_always_sorted() -> TestResult {
    let ids = matrix_game_ids()?;
    for pair in ids.windows(2) {
        assert!(pair[0] <= pair[1], "{} should precede {}", pair[0], pair[1]);
    }
    Ok(())
}

#[test]
fn matrix_game_ids_method_consistent_with_free_fn() -> TestResult {
    let matrix = load_default_matrix()?;
    assert_eq!(matrix.game_ids(), matrix_game_ids()?);
    Ok(())
}

// ---------------------------------------------------------------------------
// Byte parsing helpers — field mapping validation
// ---------------------------------------------------------------------------

#[test]
fn every_game_has_non_empty_name_and_config_writer() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        assert!(!game.name.is_empty(), "game {id} has empty name");
        assert!(
            !game.config_writer.is_empty(),
            "game {id} has empty config_writer"
        );
    }
    Ok(())
}

#[test]
fn telemetry_method_is_non_empty_for_all_games() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        assert!(
            !game.telemetry.method.is_empty(),
            "game {id} has empty telemetry method"
        );
    }
    Ok(())
}

#[test]
fn active_telemetry_games_have_positive_update_rate() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        if game.telemetry.method != "none" {
            assert!(
                game.telemetry.update_rate_hz > 0,
                "game {id} has zero update_rate_hz with active telemetry"
            );
        }
    }
    Ok(())
}

#[test]
fn field_mapping_coverage_for_active_games() -> TestResult {
    let matrix = load_default_matrix()?;
    let active: Vec<_> = matrix
        .games
        .iter()
        .filter(|(_, g)| g.telemetry.method != "none")
        .collect();

    let with_any_field = active
        .iter()
        .filter(|(_, g)| {
            let f = &g.telemetry.fields;
            f.ffb_scalar.is_some()
                || f.rpm.is_some()
                || f.speed_ms.is_some()
                || f.slip_ratio.is_some()
                || f.gear.is_some()
                || f.flags.is_some()
                || f.car_id.is_some()
                || f.track_id.is_some()
        })
        .count();

    // At least 75% of active games should have field mappings
    assert!(
        with_any_field * 4 >= active.len() * 3,
        "field mapping coverage too low: {with_any_field}/{}",
        active.len()
    );
    Ok(())
}

#[test]
fn version_telemetry_methods_are_non_empty() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        for ver in &game.versions {
            assert!(
                !ver.telemetry_method.is_empty(),
                "game {id} version {} has empty telemetry_method",
                ver.version
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Format conversion helpers — serde round-trip and YAML deserialization
// ---------------------------------------------------------------------------

#[test]
fn serde_yaml_round_trip_preserves_all_game_ids() -> TestResult {
    let matrix = load_default_matrix()?;
    let yaml = serde_yaml::to_string(&matrix)?;
    let round_tripped: GameSupportMatrix = serde_yaml::from_str(&yaml)?;

    assert_eq!(matrix.game_ids(), round_tripped.game_ids());
    Ok(())
}

#[test]
fn serde_round_trip_preserves_telemetry_details() -> TestResult {
    let matrix = load_default_matrix()?;
    let yaml = serde_yaml::to_string(&matrix)?;
    let rt: GameSupportMatrix = serde_yaml::from_str(&yaml)?;

    for id in matrix.game_ids() {
        let orig = &matrix.games[&id];
        let copy = &rt.games[&id];
        assert_eq!(orig.name, copy.name, "name mismatch for {id}");
        assert_eq!(
            orig.telemetry.method, copy.telemetry.method,
            "method mismatch for {id}"
        );
        assert_eq!(
            orig.telemetry.update_rate_hz, copy.telemetry.update_rate_hz,
            "rate mismatch for {id}"
        );
        assert_eq!(orig.status, copy.status, "status mismatch for {id}");
    }
    Ok(())
}

#[test]
fn custom_yaml_with_all_optional_fields() -> TestResult {
    let yaml = r#"
games:
  test_complete:
    name: "Test Complete"
    versions:
      - version: "2.0"
        config_paths: ["/path/a"]
        executable_patterns: ["test.exe"]
        telemetry_method: "shared_memory"
        supported_fields: ["rpm", "speed"]
    telemetry:
      method: "shared_memory"
      update_rate_hz: 120
      supports_360hz_option: true
      high_rate_update_rate_hz: 360
      output_target: "localhost:20777"
      fields:
        ffb_scalar: "FFBScalar"
        rpm: "EngineRPM"
        speed_ms: "SpeedMS"
        slip_ratio: "SlipRatio"
        gear: "Gear"
        flags: "Flags"
        car_id: "CarID"
        track_id: "TrackID"
    status: "stable"
    config_writer: "test_writer"
    auto_detect:
      process_names: ["test.exe"]
      install_registry_keys: ["HKLM\\Software\\Test"]
      install_paths: ["C:\\Games\\Test"]
"#;
    let matrix: GameSupportMatrix = serde_yaml::from_str(yaml)?;
    let game = &matrix.games["test_complete"];

    assert_eq!(game.name, "Test Complete");
    assert!(game.telemetry.supports_360hz_option);
    assert_eq!(game.telemetry.high_rate_update_rate_hz, Some(360));
    assert_eq!(
        game.telemetry.output_target,
        Some("localhost:20777".to_string())
    );
    assert_eq!(game.telemetry.fields.ffb_scalar, Some("FFBScalar".into()));
    assert_eq!(game.telemetry.fields.track_id, Some("TrackID".into()));
    assert_eq!(game.auto_detect.process_names.len(), 1);
    assert_eq!(game.auto_detect.install_registry_keys.len(), 1);
    assert_eq!(game.auto_detect.install_paths.len(), 1);
    Ok(())
}

#[test]
fn custom_yaml_minimal_with_defaults() -> TestResult {
    let yaml = r#"
games:
  minimal_game:
    name: "Minimal"
    versions:
      - version: "1.0"
        config_paths: []
        executable_patterns: []
        telemetry_method: "udp"
        supported_fields: []
    telemetry:
      method: "udp"
      update_rate_hz: 30
      fields: {}
    config_writer: "minimal"
    auto_detect:
      process_names: []
      install_registry_keys: []
      install_paths: []
"#;
    let matrix: GameSupportMatrix = serde_yaml::from_str(yaml)?;
    let game = &matrix.games["minimal_game"];

    // Defaults should be applied
    assert_eq!(game.status, GameSupportStatus::Stable);
    assert!(!game.telemetry.supports_360hz_option);
    assert!(game.telemetry.high_rate_update_rate_hz.is_none());
    assert!(game.telemetry.output_target.is_none());
    assert!(game.telemetry.fields.ffb_scalar.is_none());
    Ok(())
}

// ---------------------------------------------------------------------------
// Status filtering
// ---------------------------------------------------------------------------

#[test]
fn stable_and_experimental_are_disjoint_and_exhaustive() -> TestResult {
    let matrix = load_default_matrix()?;
    let stable = matrix.stable_games();
    let experimental = matrix.experimental_games();

    // Disjoint
    for game in &stable {
        assert!(
            !experimental.contains(game),
            "{game} appears in both stable and experimental"
        );
    }

    // Exhaustive
    assert_eq!(
        stable.len() + experimental.len(),
        matrix.games.len(),
        "stable + experimental should cover all games"
    );
    Ok(())
}

#[test]
fn game_ids_by_status_returns_sorted_results() -> TestResult {
    let matrix = load_default_matrix()?;

    let stable = matrix.game_ids_by_status(GameSupportStatus::Stable);
    for pair in stable.windows(2) {
        assert!(pair[0] <= pair[1]);
    }

    let experimental = matrix.game_ids_by_status(GameSupportStatus::Experimental);
    for pair in experimental.windows(2) {
        assert!(pair[0] <= pair[1]);
    }
    Ok(())
}

#[test]
fn has_game_id_returns_false_for_missing() -> TestResult {
    let matrix = load_default_matrix()?;
    assert!(!matrix.has_game_id("nonexistent_game_xyz_12345"));
    assert!(!matrix.has_game_id(""));
    Ok(())
}

// ---------------------------------------------------------------------------
// High-rate telemetry validation
// ---------------------------------------------------------------------------

#[test]
fn high_rate_games_have_valid_configuration() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        if game.telemetry.supports_360hz_option {
            let high_hz = game.telemetry.high_rate_update_rate_hz;
            assert!(
                high_hz.is_some(),
                "game {id} supports 360hz but has no high_rate_update_rate_hz"
            );
            assert!(
                high_hz.is_some_and(|hz| hz > game.telemetry.update_rate_hz),
                "game {id} high rate should exceed base rate"
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Version structure validation
// ---------------------------------------------------------------------------

#[test]
fn every_game_has_at_least_one_version_with_non_empty_string() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        assert!(
            !game.versions.is_empty(),
            "game {id} has no version entries"
        );
        for ver in &game.versions {
            assert!(
                !ver.version.is_empty(),
                "game {id} has a version entry with empty version string"
            );
        }
    }
    Ok(())
}

#[test]
fn auto_detect_coverage_is_reasonable() -> TestResult {
    let matrix = load_default_matrix()?;
    let with_auto_detect = matrix
        .games
        .values()
        .filter(|g| {
            !g.auto_detect.process_names.is_empty() || !g.auto_detect.install_paths.is_empty()
        })
        .count();

    assert!(
        with_auto_detect >= 10,
        "expected >= 10 games with auto-detect, got {with_auto_detect}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Embedded YAML constant
// ---------------------------------------------------------------------------

#[test]
fn embedded_yaml_constant_is_valid() -> TestResult {
    assert!(
        !racing_wheel_telemetry_support::TELEMETRY_SUPPORT_MATRIX_YAML.is_empty(),
        "embedded YAML constant must not be empty"
    );
    // Verify it can be parsed
    let _matrix: GameSupportMatrix =
        serde_yaml::from_str(racing_wheel_telemetry_support::TELEMETRY_SUPPORT_MATRIX_YAML)?;
    Ok(())
}

#[test]
fn game_names_are_unique_across_matrix() -> TestResult {
    let matrix = load_default_matrix()?;
    let mut seen = std::collections::HashSet::new();
    for (id, game) in &matrix.games {
        assert!(
            seen.insert(game.name.clone()),
            "duplicate display name '{}' on game id '{id}'",
            game.name
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Deserialization error handling
// ---------------------------------------------------------------------------

#[test]
fn invalid_yaml_returns_error() {
    let bad_yaml = "{{{{ not valid yaml at all ::::";
    let result: Result<GameSupportMatrix, _> = serde_yaml::from_str(bad_yaml);
    assert!(result.is_err());
}

#[test]
fn missing_required_name_field_returns_error() {
    let yaml = r#"
games:
  broken_game:
    versions:
      - version: "1.0"
        config_paths: []
        executable_patterns: []
        telemetry_method: "udp"
        supported_fields: []
    telemetry:
      method: "udp"
      update_rate_hz: 60
      fields: {}
    config_writer: "test"
    auto_detect:
      process_names: []
      install_registry_keys: []
      install_paths: []
"#;
    let result: Result<GameSupportMatrix, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "missing 'name' should be an error");
}

#[test]
fn missing_required_versions_field_returns_error() {
    let yaml = r#"
games:
  broken_game:
    name: "Broken"
    telemetry:
      method: "udp"
      update_rate_hz: 60
      fields: {}
    config_writer: "test"
    auto_detect:
      process_names: []
      install_registry_keys: []
      install_paths: []
"#;
    let result: Result<GameSupportMatrix, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "missing 'versions' should be an error");
}

#[test]
fn missing_required_telemetry_field_returns_error() {
    let yaml = r#"
games:
  broken_game:
    name: "Broken"
    versions:
      - version: "1.0"
        config_paths: []
        executable_patterns: []
        telemetry_method: "udp"
        supported_fields: []
    config_writer: "test"
    auto_detect:
      process_names: []
      install_registry_keys: []
      install_paths: []
"#;
    let result: Result<GameSupportMatrix, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "missing 'telemetry' should be an error");
}

#[test]
fn wrong_type_for_update_rate_returns_error() {
    let yaml = r#"
games:
  broken_game:
    name: "Broken"
    versions:
      - version: "1.0"
        config_paths: []
        executable_patterns: []
        telemetry_method: "udp"
        supported_fields: []
    telemetry:
      method: "udp"
      update_rate_hz: "not_a_number"
      fields: {}
    config_writer: "test"
    auto_detect:
      process_names: []
      install_registry_keys: []
      install_paths: []
"#;
    let result: Result<GameSupportMatrix, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "string for update_rate_hz should be an error"
    );
}

#[test]
fn empty_yaml_string_returns_error() {
    let result: Result<GameSupportMatrix, _> = serde_yaml::from_str("");
    assert!(result.is_err());
}

#[test]
fn null_yaml_returns_error() {
    let result: Result<GameSupportMatrix, _> = serde_yaml::from_str("null");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Empty and edge case matrices
// ---------------------------------------------------------------------------

#[test]
fn empty_games_map_deserializes() -> TestResult {
    let yaml = "games: {}";
    let matrix: GameSupportMatrix = serde_yaml::from_str(yaml)?;
    assert!(matrix.games.is_empty());
    assert!(matrix.game_ids().is_empty());
    assert!(matrix.stable_games().is_empty());
    assert!(matrix.experimental_games().is_empty());
    assert!(!matrix.has_game_id("anything"));
    Ok(())
}

#[test]
fn game_with_multiple_versions() -> TestResult {
    let yaml = r#"
games:
  multi_ver:
    name: "Multi Version Game"
    versions:
      - version: "1.0"
        config_paths: ["/v1/config"]
        executable_patterns: ["game_v1.exe"]
        telemetry_method: "udp"
        supported_fields: ["rpm"]
      - version: "2.0"
        config_paths: ["/v2/config"]
        executable_patterns: ["game_v2.exe"]
        telemetry_method: "shared_memory"
        supported_fields: ["rpm", "speed"]
      - version: "3.0-beta"
        config_paths: []
        executable_patterns: ["game_v3.exe"]
        telemetry_method: "udp"
        supported_fields: ["rpm", "speed", "gear"]
    telemetry:
      method: "udp"
      update_rate_hz: 60
      fields:
        rpm: "RPM"
    config_writer: "multi"
    auto_detect:
      process_names: ["game.exe"]
      install_registry_keys: []
      install_paths: []
"#;
    let matrix: GameSupportMatrix = serde_yaml::from_str(yaml)?;
    let game = &matrix.games["multi_ver"];
    assert_eq!(game.versions.len(), 3);
    assert_eq!(game.versions[0].version, "1.0");
    assert_eq!(game.versions[1].version, "2.0");
    assert_eq!(game.versions[2].version, "3.0-beta");
    assert_eq!(game.versions[1].telemetry_method, "shared_memory");
    assert_eq!(game.versions[2].supported_fields.len(), 3);
    Ok(())
}

#[test]
fn game_with_rich_auto_detect() -> TestResult {
    let yaml = r#"
games:
  rich_detect:
    name: "Rich Detect"
    versions:
      - version: "1.0"
        config_paths: ["/path/a", "/path/b", "/path/c"]
        executable_patterns: ["game*.exe", "launcher.exe"]
        telemetry_method: "udp"
        supported_fields: ["rpm"]
    telemetry:
      method: "udp"
      update_rate_hz: 60
      fields: {}
    config_writer: "rich"
    auto_detect:
      process_names: ["game.exe", "game_launcher.exe", "game64.exe"]
      install_registry_keys: ["HKLM\\Software\\Game", "HKCU\\Software\\Game"]
      install_paths: ["C:\\Games\\Rich", "D:\\Steam\\Rich"]
"#;
    let matrix: GameSupportMatrix = serde_yaml::from_str(yaml)?;
    let game = &matrix.games["rich_detect"];
    assert_eq!(game.auto_detect.process_names.len(), 3);
    assert_eq!(game.auto_detect.install_registry_keys.len(), 2);
    assert_eq!(game.auto_detect.install_paths.len(), 2);
    assert_eq!(game.versions[0].config_paths.len(), 3);
    assert_eq!(game.versions[0].executable_patterns.len(), 2);
    Ok(())
}

// ---------------------------------------------------------------------------
// TelemetryFieldMapping edge cases
// ---------------------------------------------------------------------------

#[test]
fn field_mapping_all_none() -> TestResult {
    let yaml = r#"
games:
  no_fields:
    name: "No Fields"
    versions:
      - version: "1.0"
        config_paths: []
        executable_patterns: []
        telemetry_method: "none"
        supported_fields: []
    telemetry:
      method: "none"
      update_rate_hz: 0
      fields: {}
    config_writer: "none"
    auto_detect:
      process_names: []
      install_registry_keys: []
      install_paths: []
"#;
    let matrix: GameSupportMatrix = serde_yaml::from_str(yaml)?;
    let fields = &matrix.games["no_fields"].telemetry.fields;
    assert!(fields.ffb_scalar.is_none());
    assert!(fields.rpm.is_none());
    assert!(fields.speed_ms.is_none());
    assert!(fields.slip_ratio.is_none());
    assert!(fields.gear.is_none());
    assert!(fields.flags.is_none());
    assert!(fields.car_id.is_none());
    assert!(fields.track_id.is_none());
    Ok(())
}

#[test]
fn field_mapping_all_some() -> TestResult {
    let yaml = r#"
games:
  all_fields:
    name: "All Fields"
    versions:
      - version: "1.0"
        config_paths: []
        executable_patterns: []
        telemetry_method: "udp"
        supported_fields: []
    telemetry:
      method: "udp"
      update_rate_hz: 60
      fields:
        ffb_scalar: "FFB"
        rpm: "RPM"
        speed_ms: "Speed"
        slip_ratio: "Slip"
        gear: "Gear"
        flags: "Flags"
        car_id: "Car"
        track_id: "Track"
    config_writer: "all"
    auto_detect:
      process_names: []
      install_registry_keys: []
      install_paths: []
"#;
    let matrix: GameSupportMatrix = serde_yaml::from_str(yaml)?;
    let fields = &matrix.games["all_fields"].telemetry.fields;
    assert_eq!(fields.ffb_scalar, Some("FFB".to_string()));
    assert_eq!(fields.rpm, Some("RPM".to_string()));
    assert_eq!(fields.speed_ms, Some("Speed".to_string()));
    assert_eq!(fields.slip_ratio, Some("Slip".to_string()));
    assert_eq!(fields.gear, Some("Gear".to_string()));
    assert_eq!(fields.flags, Some("Flags".to_string()));
    assert_eq!(fields.car_id, Some("Car".to_string()));
    assert_eq!(fields.track_id, Some("Track".to_string()));
    Ok(())
}

// ---------------------------------------------------------------------------
// Clone and Debug trait validation
// ---------------------------------------------------------------------------

#[test]
fn matrix_clone_preserves_game_ids() -> TestResult {
    let matrix = load_default_matrix()?;
    let cloned = matrix.clone();
    assert_eq!(matrix.game_ids(), cloned.game_ids());
    for id in matrix.game_ids() {
        assert_eq!(
            matrix.games[&id].name, cloned.games[&id].name,
            "name mismatch for {id}"
        );
    }
    Ok(())
}

#[test]
fn matrix_debug_is_non_empty() -> TestResult {
    let matrix = load_default_matrix()?;
    let debug = format!("{matrix:?}");
    assert!(!debug.is_empty());
    assert!(debug.contains("GameSupportMatrix"));
    Ok(())
}

#[test]
fn game_support_status_debug_and_clone() {
    let status = GameSupportStatus::Experimental;
    let cloned = status;
    let debug = format!("{status:?}");
    assert_eq!(cloned, GameSupportStatus::Experimental);
    assert!(debug.contains("Experimental"));
}

// ---------------------------------------------------------------------------
// normalize_game_id edge cases
// ---------------------------------------------------------------------------

#[test]
fn normalize_game_id_very_long_input() {
    let long_id = "a".repeat(1000);
    assert_eq!(normalize_game_id(&long_id), long_id.as_str());
}

#[test]
fn normalize_game_id_with_special_chars() {
    assert_eq!(normalize_game_id("game-with-dashes"), "game-with-dashes");
    assert_eq!(normalize_game_id("game.with.dots"), "game.with.dots");
    assert_eq!(normalize_game_id("game 123"), "game 123");
}

#[test]
fn normalize_game_id_mixed_case_non_aliased() {
    // Only ea_wrc and f1_2025 have case-insensitive matching
    assert_eq!(normalize_game_id("IRACING"), "IRACING");
    assert_eq!(normalize_game_id("ACC"), "ACC");
}

// ---------------------------------------------------------------------------
// Serde round-trip preserves status and field mappings
// ---------------------------------------------------------------------------

#[test]
fn serde_round_trip_preserves_status_and_field_mappings() -> TestResult {
    let matrix = load_default_matrix()?;
    let yaml = serde_yaml::to_string(&matrix)?;
    let rt: GameSupportMatrix = serde_yaml::from_str(&yaml)?;

    for id in matrix.game_ids() {
        let orig = &matrix.games[&id];
        let copy = &rt.games[&id];
        assert_eq!(orig.status, copy.status, "status mismatch for {id}");
        assert_eq!(
            orig.telemetry.fields.ffb_scalar, copy.telemetry.fields.ffb_scalar,
            "ffb_scalar mismatch for {id}"
        );
        assert_eq!(
            orig.telemetry.fields.rpm, copy.telemetry.fields.rpm,
            "rpm field mismatch for {id}"
        );
        assert_eq!(
            orig.telemetry.supports_360hz_option, copy.telemetry.supports_360hz_option,
            "360hz mismatch for {id}"
        );
        assert_eq!(
            orig.telemetry.high_rate_update_rate_hz, copy.telemetry.high_rate_update_rate_hz,
            "high_rate_hz mismatch for {id}"
        );
        assert_eq!(
            orig.telemetry.output_target, copy.telemetry.output_target,
            "output_target mismatch for {id}"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// JSON serde round-trip (structs derive Serialize + Deserialize)
// ---------------------------------------------------------------------------

#[test]
fn json_serde_round_trip_preserves_game_ids() -> TestResult {
    let matrix = load_default_matrix()?;
    let json = serde_json::to_string(&matrix)?;
    let rt: GameSupportMatrix = serde_json::from_str(&json)?;
    assert_eq!(matrix.game_ids(), rt.game_ids());
    for id in matrix.game_ids() {
        assert_eq!(
            matrix.games[&id].name, rt.games[&id].name,
            "JSON name mismatch for {id}"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Matrix invariant: auto_detect lists have no empty strings
// ---------------------------------------------------------------------------

#[test]
fn auto_detect_entries_are_non_empty_strings() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        for name in &game.auto_detect.process_names {
            assert!(!name.is_empty(), "game {id} has empty process_name");
        }
        for key in &game.auto_detect.install_registry_keys {
            assert!(!key.is_empty(), "game {id} has empty registry key");
        }
        for path in &game.auto_detect.install_paths {
            assert!(!path.is_empty(), "game {id} has empty install path");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Matrix invariant: version supported_fields have no empty strings
// ---------------------------------------------------------------------------

#[test]
fn version_supported_fields_are_non_empty_strings() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        for ver in &game.versions {
            for field in &ver.supported_fields {
                assert!(
                    !field.is_empty(),
                    "game {id} version {} has empty supported_field",
                    ver.version
                );
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Invalid status value in YAML
// ---------------------------------------------------------------------------

#[test]
fn invalid_status_value_returns_error() {
    let yaml = r#"
games:
  bad_status:
    name: "Bad Status"
    versions:
      - version: "1.0"
        config_paths: []
        executable_patterns: []
        telemetry_method: "udp"
        supported_fields: []
    telemetry:
      method: "udp"
      update_rate_hz: 60
      fields: {}
    status: "deprecated"
    config_writer: "bad"
    auto_detect:
      process_names: []
      install_registry_keys: []
      install_paths: []
"#;
    let result: Result<GameSupportMatrix, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "unrecognized status should be an error");
}

// ---------------------------------------------------------------------------
// Telemetry output_target: verify nullable field
// ---------------------------------------------------------------------------

#[test]
fn output_target_null_and_present() -> TestResult {
    let yaml = r#"
games:
  with_target:
    name: "With Target"
    versions:
      - version: "1.0"
        config_paths: []
        executable_patterns: []
        telemetry_method: "udp"
        supported_fields: []
    telemetry:
      method: "udp"
      update_rate_hz: 60
      output_target: "127.0.0.1:20777"
      fields: {}
    config_writer: "wt"
    auto_detect:
      process_names: []
      install_registry_keys: []
      install_paths: []
  without_target:
    name: "Without Target"
    versions:
      - version: "1.0"
        config_paths: []
        executable_patterns: []
        telemetry_method: "shared_memory"
        supported_fields: []
    telemetry:
      method: "shared_memory"
      update_rate_hz: 60
      fields: {}
    config_writer: "wot"
    auto_detect:
      process_names: []
      install_registry_keys: []
      install_paths: []
"#;
    let matrix: GameSupportMatrix = serde_yaml::from_str(yaml)?;
    assert_eq!(
        matrix.games["with_target"].telemetry.output_target,
        Some("127.0.0.1:20777".to_string())
    );
    assert!(
        matrix.games["without_target"]
            .telemetry
            .output_target
            .is_none()
    );
    Ok(())
}
