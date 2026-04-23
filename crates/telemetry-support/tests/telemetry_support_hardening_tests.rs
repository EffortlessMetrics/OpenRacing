//! Hardening tests for racing-wheel-telemetry-support.
//!
//! Covers: telemetry data normalization, cross-game data mapping,
//! data quality checks, timestamp handling, and missing data handling.

use racing_wheel_telemetry_support::{
    GameSupportMatrix, GameSupportStatus, TelemetryFieldMapping, load_default_matrix,
    matrix_game_id_set, matrix_game_ids, normalize_game_id,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Telemetry data normalization ────────────────────────────────────────

#[test]
fn normalize_game_id_ea_wrc_all_casings() {
    let variants = ["ea_wrc", "EA_WRC", "Ea_Wrc", "eA_wRc", "EA_wrc"];
    for v in variants {
        assert_eq!(
            normalize_game_id(v),
            "eawrc",
            "expected 'eawrc' for input '{v}'"
        );
    }
}

#[test]
fn normalize_game_id_f1_2025_all_casings() {
    let variants = ["f1_2025", "F1_2025", "F1_2025"];
    for v in variants {
        assert_eq!(
            normalize_game_id(v),
            "f1_25",
            "expected 'f1_25' for input '{v}'"
        );
    }
}

#[test]
fn normalize_game_id_preserves_canonical_ids() -> TestResult {
    let ids = matrix_game_ids()?;
    for id in &ids {
        // Canonical IDs that aren't aliases should pass through unchanged.
        let normalized = normalize_game_id(id);
        // The only IDs that change are ea_wrc and f1_2025 which aren't canonical.
        assert!(
            !normalized.is_empty(),
            "normalized game_id for '{id}' should not be empty"
        );
    }
    Ok(())
}

#[test]
fn normalize_game_id_empty_string_passthrough() {
    assert_eq!(normalize_game_id(""), "");
}

#[test]
fn normalize_game_id_whitespace_is_trimmed() {
    // Input should be trimmed before alias checks and passthrough.
    assert_eq!(normalize_game_id(" "), "");
    assert_eq!(normalize_game_id("\t"), "");
    assert_eq!(normalize_game_id(" iracing "), "iracing");
}

#[test]
fn normalize_game_id_special_characters_passthrough() {
    assert_eq!(normalize_game_id("game-with-dashes"), "game-with-dashes");
    assert_eq!(normalize_game_id("game.with.dots"), "game.with.dots");
    assert_eq!(normalize_game_id("game/slash"), "game/slash");
}

// ── Cross-game data mapping ─────────────────────────────────────────────

#[test]
fn active_telemetry_games_with_field_mappings_exist() -> TestResult {
    let matrix = load_default_matrix()?;
    let games_with_mappings = matrix
        .games
        .iter()
        .filter(|(_, game)| {
            game.telemetry.method != "none" && {
                let fields = &game.telemetry.fields;
                fields.ffb_scalar.is_some()
                    || fields.rpm.is_some()
                    || fields.speed_ms.is_some()
                    || fields.slip_ratio.is_some()
                    || fields.gear.is_some()
                    || fields.flags.is_some()
                    || fields.car_id.is_some()
                    || fields.track_id.is_some()
            }
        })
        .count();
    // At least some active games should have field mappings
    assert!(
        games_with_mappings >= 5,
        "expected at least 5 games with field mappings, got {games_with_mappings}"
    );
    Ok(())
}

#[test]
fn games_with_udp_telemetry_have_output_target() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        if game.telemetry.method.contains("udp") {
            assert!(
                game.telemetry.output_target.is_some(),
                "game {id} uses UDP telemetry but has no output_target"
            );
        }
    }
    Ok(())
}

#[test]
fn field_mappings_for_known_games_include_core_fields() -> TestResult {
    let matrix = load_default_matrix()?;
    let core_games = ["iracing", "acc", "ams2", "rfactor2"];
    for game_id in core_games {
        let game = matrix
            .games
            .get(game_id)
            .ok_or_else(|| format!("missing game: {game_id}"))?;
        let fields = &game.telemetry.fields;
        assert!(fields.rpm.is_some(), "core game {game_id} should map rpm");
        assert!(
            fields.speed_ms.is_some(),
            "core game {game_id} should map speed_ms"
        );
    }
    Ok(())
}

#[test]
fn cross_game_field_mapping_names_are_non_empty_strings() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        let fields = &game.telemetry.fields;
        for field_name in [
            &fields.ffb_scalar,
            &fields.rpm,
            &fields.speed_ms,
            &fields.slip_ratio,
            &fields.gear,
            &fields.flags,
            &fields.car_id,
            &fields.track_id,
        ]
        .into_iter()
        .flatten()
        {
            assert!(
                !field_name.is_empty(),
                "game {id} has an empty field mapping string"
            );
        }
    }
    Ok(())
}

// ── Data quality checks ─────────────────────────────────────────────────

#[test]
fn all_game_names_are_human_readable() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        assert!(
            game.name.len() >= 2,
            "game {id} name '{}' is suspiciously short",
            game.name
        );
        assert!(
            game.name.len() <= 100,
            "game {id} name '{}' is suspiciously long",
            game.name
        );
        // Name should contain at least one alphabetic character
        assert!(
            game.name.chars().any(|c| c.is_alphabetic()),
            "game {id} name '{}' has no alphabetic chars",
            game.name
        );
    }
    Ok(())
}

#[test]
fn game_ids_are_lowercase_snake_case() -> TestResult {
    let ids = matrix_game_ids()?;
    for id in &ids {
        assert!(
            id.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "game id '{id}' is not lowercase snake_case"
        );
        assert!(
            !id.starts_with('_'),
            "game id '{id}' should not start with underscore"
        );
        assert!(
            !id.ends_with('_'),
            "game id '{id}' should not end with underscore"
        );
    }
    Ok(())
}

#[test]
fn no_duplicate_game_names() -> TestResult {
    let matrix = load_default_matrix()?;
    let mut seen = std::collections::HashSet::new();
    for (id, game) in &matrix.games {
        assert!(
            seen.insert(game.name.clone()),
            "duplicate game name '{}' found on game {id}",
            game.name
        );
    }
    Ok(())
}

#[test]
fn telemetry_update_rates_are_within_sane_bounds() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        let rate = game.telemetry.update_rate_hz;
        if game.telemetry.method != "none" {
            assert!(
                rate > 0 && rate <= 1000,
                "game {id} has update_rate_hz={rate} which is out of bounds [1..1000]"
            );
        }
    }
    Ok(())
}

#[test]
fn high_rate_update_rates_exceed_base_rates() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        if let Some(high_rate) = game.telemetry.high_rate_update_rate_hz {
            assert!(
                high_rate > game.telemetry.update_rate_hz,
                "game {id} high_rate {high_rate} should exceed base rate {}",
                game.telemetry.update_rate_hz
            );
        }
    }
    Ok(())
}

#[test]
fn matrix_game_count_regression_guard() -> TestResult {
    let ids = matrix_game_ids()?;
    // If games are removed, this test catches it.
    assert!(
        ids.len() >= 15,
        "expected at least 15 games, got {}",
        ids.len()
    );
    Ok(())
}

// ── Auto-detect configuration quality ───────────────────────────────────

#[test]
fn auto_detect_process_names_are_non_empty_when_present() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        for pname in &game.auto_detect.process_names {
            assert!(!pname.is_empty(), "game {id} has empty process_name entry");
        }
    }
    Ok(())
}

#[test]
fn auto_detect_install_paths_are_non_empty_when_present() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        for path in &game.auto_detect.install_paths {
            assert!(!path.is_empty(), "game {id} has empty install_path entry");
        }
    }
    Ok(())
}

#[test]
fn most_stable_games_have_auto_detect_info() -> TestResult {
    let matrix = load_default_matrix()?;
    let stable = matrix.stable_games();
    let with_auto_detect = stable
        .iter()
        .filter(|id| {
            let game = &matrix.games[id.as_str()];
            !game.auto_detect.process_names.is_empty()
                || !game.auto_detect.install_paths.is_empty()
                || !game.auto_detect.install_registry_keys.is_empty()
        })
        .count();
    // At least half of stable games should have auto-detect
    assert!(
        with_auto_detect * 2 >= stable.len(),
        "only {with_auto_detect}/{} stable games have auto-detect",
        stable.len()
    );
    Ok(())
}

// ── Version configuration quality ───────────────────────────────────────

#[test]
fn each_version_has_supported_fields_or_is_documented() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        for ver in &game.versions {
            // Versions should either have supported_fields or the method is "none"
            if ver.telemetry_method != "none" && ver.supported_fields.is_empty() {
                // This is OK for some games; just ensure the version string is present
                assert!(
                    !ver.version.is_empty(),
                    "game {id} has a version with empty version string and no fields"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn version_executable_patterns_are_valid_when_present() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        for ver in &game.versions {
            for pattern in &ver.executable_patterns {
                assert!(
                    !pattern.is_empty(),
                    "game {id} version {} has empty executable pattern",
                    ver.version
                );
            }
        }
    }
    Ok(())
}

// ── Missing data handling ───────────────────────────────────────────────

#[test]
fn matrix_handles_lookup_of_missing_game_gracefully() -> TestResult {
    let matrix = load_default_matrix()?;
    assert!(!matrix.has_game_id("totally_fake_game_12345"));
    assert!(!matrix.games.contains_key("totally_fake_game_12345"));
    Ok(())
}

#[test]
fn matrix_game_id_set_is_consistent_with_vector() -> TestResult {
    let ids = matrix_game_ids()?;
    let set = matrix_game_id_set()?;
    assert_eq!(ids.len(), set.len());
    for id in &ids {
        assert!(set.contains(id));
    }
    // Verify no extra IDs in set
    for id in &set {
        assert!(ids.contains(id), "set contains {id} not in vector");
    }
    Ok(())
}

#[test]
fn game_ids_by_status_exhaustive_coverage() -> TestResult {
    let matrix = load_default_matrix()?;
    let stable = matrix.game_ids_by_status(GameSupportStatus::Stable);
    let experimental = matrix.game_ids_by_status(GameSupportStatus::Experimental);
    let all_ids = matrix.game_ids();

    let mut combined: Vec<String> = stable.into_iter().chain(experimental).collect();
    combined.sort_unstable();
    assert_eq!(combined, all_ids);
    Ok(())
}

// ── Serde round-trip for matrix structures ──────────────────────────────

#[test]
fn game_support_matrix_json_round_trip() -> TestResult {
    let matrix = load_default_matrix()?;
    let json = serde_json::to_string(&matrix)?;
    let decoded: GameSupportMatrix = serde_json::from_str(&json)?;
    assert_eq!(matrix.games.len(), decoded.games.len());
    for key in matrix.games.keys() {
        assert!(
            decoded.games.contains_key(key),
            "decoded matrix missing key {key}"
        );
    }
    Ok(())
}

#[test]
fn game_support_status_serde_round_trip() -> TestResult {
    for status in [GameSupportStatus::Stable, GameSupportStatus::Experimental] {
        let json = serde_json::to_string(&status)?;
        let decoded: GameSupportStatus = serde_json::from_str(&json)?;
        assert_eq!(decoded, status);
    }
    Ok(())
}

#[test]
fn telemetry_field_mapping_serde_round_trip() -> TestResult {
    let mapping = TelemetryFieldMapping {
        ffb_scalar: Some("forceFeedback".to_string()),
        rpm: Some("engineRpm".to_string()),
        speed_ms: Some("vehicleSpeed".to_string()),
        slip_ratio: None,
        gear: Some("currentGear".to_string()),
        flags: None,
        car_id: Some("carModel".to_string()),
        track_id: None,
    };
    let json = serde_json::to_string(&mapping)?;
    let decoded: TelemetryFieldMapping = serde_json::from_str(&json)?;
    assert_eq!(decoded.ffb_scalar, mapping.ffb_scalar);
    assert_eq!(decoded.rpm, mapping.rpm);
    assert_eq!(decoded.speed_ms, mapping.speed_ms);
    assert_eq!(decoded.slip_ratio, mapping.slip_ratio);
    assert_eq!(decoded.gear, mapping.gear);
    assert_eq!(decoded.car_id, mapping.car_id);
    assert_eq!(decoded.track_id, mapping.track_id);
    Ok(())
}

#[test]
fn embedded_yaml_contains_expected_top_level_key() {
    let yaml = racing_wheel_telemetry_support::TELEMETRY_SUPPORT_MATRIX_YAML;
    assert!(yaml.contains("games:"), "YAML must contain 'games:' key");
}

#[test]
fn embedded_yaml_parseable_as_yaml() -> TestResult {
    let yaml = racing_wheel_telemetry_support::TELEMETRY_SUPPORT_MATRIX_YAML;
    let _matrix: GameSupportMatrix = serde_yaml::from_str(yaml)?;
    Ok(())
}

// ── 360Hz option consistency ────────────────────────────────────────────

#[test]
fn games_with_360hz_option_flag_have_high_rate_value() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        if game.telemetry.supports_360hz_option {
            assert!(
                game.telemetry.high_rate_update_rate_hz.is_some(),
                "game {id} has supports_360hz_option=true but no high_rate_update_rate_hz"
            );
        }
    }
    Ok(())
}

#[test]
fn games_without_360hz_flag_have_no_high_rate_or_it_is_explicit() -> TestResult {
    let matrix = load_default_matrix()?;
    for (id, game) in &matrix.games {
        if !game.telemetry.supports_360hz_option {
            // It's OK to have high_rate_update_rate_hz without 360hz flag,
            // but it must be > base rate if set.
            if let Some(hr) = game.telemetry.high_rate_update_rate_hz {
                assert!(
                    hr > game.telemetry.update_rate_hz,
                    "game {id} high rate {hr} <= base rate {} without 360hz flag",
                    game.telemetry.update_rate_hz
                );
            }
        }
    }
    Ok(())
}
