//! Comprehensive integration tests for racing-wheel-telemetry-support.
//!
//! Covers: game support matrix loading, helper functions, ID normalization,
//! status filtering, and serde round-trips.

use racing_wheel_telemetry_support::{
    GameSupportMatrix, GameSupportStatus, load_default_matrix, matrix_game_id_set, matrix_game_ids,
    normalize_game_id,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Matrix loading ──────────────────────────────────────────────────────

#[test]
fn load_default_matrix_succeeds() -> TestResult {
    let matrix = load_default_matrix()?;
    assert!(!matrix.games.is_empty());
    Ok(())
}

#[test]
fn matrix_game_ids_non_empty_and_sorted() -> TestResult {
    let ids = matrix_game_ids()?;
    assert!(!ids.is_empty());
    for pair in ids.windows(2) {
        assert!(
            pair[0] <= pair[1],
            "{} should come before {}",
            pair[0],
            pair[1]
        );
    }
    Ok(())
}

#[test]
fn matrix_game_id_set_matches_vector() -> TestResult {
    let ids = matrix_game_ids()?;
    let set = matrix_game_id_set()?;
    assert_eq!(ids.len(), set.len());
    for id in &ids {
        assert!(set.contains(id), "set missing {id}");
    }
    Ok(())
}

#[test]
fn matrix_game_ids_method_matches_free_fn() -> TestResult {
    let matrix = load_default_matrix()?;
    assert_eq!(matrix.game_ids(), matrix_game_ids()?);
    Ok(())
}

// ── has_game_id ─────────────────────────────────────────────────────────

#[test]
fn has_game_id_positive_and_negative() -> TestResult {
    let matrix = load_default_matrix()?;
    assert!(matrix.has_game_id("iracing"));
    assert!(!matrix.has_game_id("nonexistent_game_xyz"));
    Ok(())
}

// ── Status filtering ────────────────────────────────────────────────────

#[test]
fn stable_and_experimental_are_disjoint() -> TestResult {
    let matrix = load_default_matrix()?;
    let stable = matrix.stable_games();
    let experimental = matrix.experimental_games();
    for game in &stable {
        assert!(
            !experimental.contains(game),
            "{game} appears in both stable and experimental"
        );
    }
    Ok(())
}

#[test]
fn stable_and_experimental_cover_all_games() -> TestResult {
    let matrix = load_default_matrix()?;
    let total = matrix.stable_games().len() + matrix.experimental_games().len();
    assert_eq!(total, matrix.games.len());
    Ok(())
}

#[test]
fn game_ids_by_status_returns_sorted() -> TestResult {
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
fn game_support_status_default_is_stable() {
    assert_eq!(GameSupportStatus::default(), GameSupportStatus::Stable);
}

// ── normalize_game_id ───────────────────────────────────────────────────

#[test]
fn normalize_ea_wrc_alias() {
    assert_eq!(normalize_game_id("ea_wrc"), "eawrc");
    assert_eq!(normalize_game_id("EA_WRC"), "eawrc");
    assert_eq!(normalize_game_id("Ea_Wrc"), "eawrc");
}

#[test]
fn normalize_f1_2025_alias() {
    assert_eq!(normalize_game_id("f1_2025"), "f1_25");
    assert_eq!(normalize_game_id("F1_2025"), "f1_25");
}

#[test]
fn normalize_passthrough_for_unknown() {
    assert_eq!(normalize_game_id("iracing"), "iracing");
    assert_eq!(normalize_game_id("acc"), "acc");
    assert_eq!(normalize_game_id("unknown"), "unknown");
    assert_eq!(normalize_game_id(""), "");
}

// ── Game data integrity ─────────────────────────────────────────────────

#[test]
fn each_game_has_non_empty_name_and_versions() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        assert!(!game.name.is_empty(), "game {id} has empty name");
        assert!(!game.versions.is_empty(), "game {id} has no versions");
    }
    Ok(())
}

#[test]
fn each_game_has_config_writer() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        assert!(
            !game.config_writer.is_empty(),
            "game {id} has empty config_writer"
        );
    }
    Ok(())
}

#[test]
fn each_version_has_non_empty_version_string() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        for ver in &game.versions {
            assert!(
                !ver.version.is_empty(),
                "game {id} has empty version string"
            );
            assert!(
                !ver.telemetry_method.is_empty(),
                "game {id} version {} has empty telemetry_method",
                ver.version
            );
        }
    }
    Ok(())
}

#[test]
fn telemetry_rates_are_positive_for_active_games() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        if game.telemetry.method != "none" {
            assert!(
                game.telemetry.update_rate_hz > 0,
                "game {id} has zero update_rate_hz"
            );
        }
    }
    Ok(())
}

#[test]
fn game_names_are_unique() -> TestResult {
    let matrix = load_default_matrix()?;
    let mut seen = std::collections::HashSet::new();
    for (id, game) in &matrix.games {
        assert!(
            seen.insert(game.name.clone()),
            "duplicate name '{}' on game {id}",
            game.name
        );
    }
    Ok(())
}

// ── High-rate telemetry ─────────────────────────────────────────────────

#[test]
fn high_rate_option_implies_higher_hz() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        if game.telemetry.supports_360hz_option {
            assert!(
                game.telemetry.high_rate_update_rate_hz.is_some(),
                "game {id} has 360hz option but no high rate hz"
            );
            let high_hz = game.telemetry.high_rate_update_rate_hz.unwrap_or(0);
            assert!(
                high_hz > game.telemetry.update_rate_hz,
                "game {id} high rate should exceed base rate"
            );
        }
    }
    Ok(())
}

// ── YAML constant ───────────────────────────────────────────────────────

#[test]
fn embedded_yaml_is_non_empty() {
    assert!(!racing_wheel_telemetry_support::TELEMETRY_SUPPORT_MATRIX_YAML.is_empty());
}

// ── Serde round-trip ────────────────────────────────────────────────────

#[test]
fn matrix_serde_round_trip_preserves_game_ids() -> TestResult {
    let matrix = load_default_matrix()?;
    let yaml = serde_yaml::to_string(&matrix)?;
    let round_tripped: GameSupportMatrix = serde_yaml::from_str(&yaml)?;
    assert_eq!(matrix.game_ids(), round_tripped.game_ids());
    for id in matrix.game_ids() {
        let orig = &matrix.games[&id];
        let rt = &round_tripped.games[&id];
        assert_eq!(orig.name, rt.name, "name mismatch for {id}");
        assert_eq!(
            orig.telemetry.method, rt.telemetry.method,
            "method mismatch for {id}"
        );
    }
    Ok(())
}

// ── Minimum game count regression ───────────────────────────────────────

#[test]
fn minimum_game_count_regression() -> TestResult {
    let ids = matrix_game_ids()?;
    assert!(ids.len() >= 15, "got {} games", ids.len());
    Ok(())
}

// ── Expected games present ──────────────────────────────────────────────

#[test]
fn expected_games_present() -> TestResult {
    let ids = matrix_game_ids()?;
    for game in ["iracing", "acc", "f1_25", "eawrc", "ams2", "rfactor2"] {
        assert!(ids.contains(&game.to_string()), "missing: {game}");
    }
    Ok(())
}

// ── Custom YAML deserialization ─────────────────────────────────────────

#[test]
fn custom_yaml_minimal_deserialization() -> TestResult {
    let yaml = r#"
games:
  test_game:
    name: "Test Game"
    versions:
      - version: "1.0"
        config_paths: []
        executable_patterns: ["test.exe"]
        telemetry_method: "udp"
        supported_fields: ["rpm"]
    telemetry:
      method: "udp"
      update_rate_hz: 60
      fields:
        rpm: "RPM"
    config_writer: "test"
    auto_detect:
      process_names: ["test.exe"]
      install_registry_keys: []
      install_paths: []
"#;
    let matrix: GameSupportMatrix = serde_yaml::from_str(yaml)?;
    assert_eq!(matrix.game_ids(), vec!["test_game".to_string()]);
    assert!(matrix.has_game_id("test_game"));
    assert!(!matrix.has_game_id("missing"));
    let game = &matrix.games["test_game"];
    assert_eq!(game.name, "Test Game");
    assert_eq!(game.telemetry.update_rate_hz, 60);
    assert_eq!(game.status, GameSupportStatus::Stable);
    assert!(!game.telemetry.supports_360hz_option);
    assert!(game.telemetry.high_rate_update_rate_hz.is_none());
    Ok(())
}

#[test]
fn custom_yaml_experimental_status() -> TestResult {
    let yaml = r#"
games:
  alpha_sim:
    name: "Alpha Sim"
    versions:
      - version: "0.1"
        config_paths: []
        executable_patterns: []
        telemetry_method: "none"
        supported_fields: []
    telemetry:
      method: "none"
      update_rate_hz: 0
      fields: {}
    status: "experimental"
    config_writer: "alpha"
    auto_detect:
      process_names: []
      install_registry_keys: []
      install_paths: []
"#;
    let matrix: GameSupportMatrix = serde_yaml::from_str(yaml)?;
    let game = &matrix.games["alpha_sim"];
    assert_eq!(game.status, GameSupportStatus::Experimental);
    assert!(matrix.stable_games().is_empty());
    assert_eq!(matrix.experimental_games(), vec!["alpha_sim".to_string()]);
    Ok(())
}
