//! Deep tests for the telemetry-support crate.
//!
//! Covers helper utilities, byte-level validation, format conversion helpers,
//! and extended matrix validation.

use racing_wheel_telemetry_support::{
    GameSupportMatrix, GameSupportStatus, load_default_matrix, matrix_game_id_set,
    matrix_game_ids, normalize_game_id,
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
        assert_eq!(
            orig.status, copy.status,
            "status mismatch for {id}"
        );
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
        .filter(|g| !g.auto_detect.process_names.is_empty() || !g.auto_detect.install_paths.is_empty())
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
