//! OpenRacing shared game support matrix metadata.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const TELEMETRY_SUPPORT_MATRIX_YAML: &str = include_str!("game_support_matrix.yaml");

/// Supported game matrix loaded from a static configuration source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSupportMatrix {
    pub games: HashMap<String, GameSupport>,
}

/// Support lifecycle status for a game integration.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum GameSupportStatus {
    #[default]
    Stable,
    Experimental,
}

/// Support information for a specific game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSupport {
    pub name: String,
    pub versions: Vec<GameVersion>,
    pub telemetry: TelemetrySupport,
    #[serde(default)]
    pub status: GameSupportStatus,
    pub config_writer: String,
    pub auto_detect: AutoDetectConfig,
}

/// Version-specific game support details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameVersion {
    pub version: String,
    pub config_paths: Vec<String>,
    pub executable_patterns: Vec<String>,
    pub telemetry_method: String,
    pub supported_fields: Vec<String>,
}

/// Telemetry settings for the given game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySupport {
    pub method: String,
    pub update_rate_hz: u32,
    #[serde(default)]
    pub supports_360hz_option: bool,
    #[serde(default)]
    pub high_rate_update_rate_hz: Option<u32>,
    pub output_target: Option<String>,
    pub fields: TelemetryFieldMapping,
}

/// Mapping of normalized telemetry fields to game-specific fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFieldMapping {
    pub ffb_scalar: Option<String>,
    pub rpm: Option<String>,
    pub speed_ms: Option<String>,
    pub slip_ratio: Option<String>,
    pub gear: Option<String>,
    pub flags: Option<String>,
    pub car_id: Option<String>,
    pub track_id: Option<String>,
}

/// Process/path auto-detection metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoDetectConfig {
    pub process_names: Vec<String>,
    pub install_registry_keys: Vec<String>,
    pub install_paths: Vec<String>,
}

/// Normalize game IDs at the boundary (historical alias support).
pub fn normalize_game_id(game_id: &str) -> &str {
    if game_id.eq_ignore_ascii_case("ea_wrc") {
        "eawrc"
    } else if game_id.eq_ignore_ascii_case("f1_2025") {
        // f1_2025 is an alias for the native EA protocol adapter (f1_25)
        "f1_25"
    } else {
        game_id
    }
}

/// Load the canonical game support matrix.
pub fn load_default_matrix() -> Result<GameSupportMatrix, serde_yaml::Error> {
    serde_yaml::from_str(TELEMETRY_SUPPORT_MATRIX_YAML)
}

/// Load game identifiers from the canonical telemetry matrix.
pub fn matrix_game_ids() -> Result<Vec<String>, serde_yaml::Error> {
    let matrix = load_default_matrix()?;

    let mut game_ids: Vec<String> = matrix.games.keys().cloned().collect();
    game_ids.sort_unstable();
    Ok(game_ids)
}

/// Load game identifiers from the canonical telemetry matrix as a set.
pub fn matrix_game_id_set() -> Result<HashSet<String>, serde_yaml::Error> {
    Ok(matrix_game_ids()?.into_iter().collect())
}

impl GameSupportMatrix {
    /// Return all configured game ids as a vector.
    pub fn game_ids(&self) -> Vec<String> {
        let mut game_ids: Vec<String> = self.games.keys().cloned().collect();
        game_ids.sort_unstable();
        game_ids
    }

    /// Check whether a game id is present in this matrix.
    pub fn has_game_id(&self, game_id: &str) -> bool {
        self.games.contains_key(game_id)
    }

    /// Return game ids by status, sorted alphabetically.
    pub fn game_ids_by_status(&self, status: GameSupportStatus) -> Vec<String> {
        let mut game_ids: Vec<String> = self
            .games
            .iter()
            .filter_map(|(game_id, support)| (support.status == status).then_some(game_id.clone()))
            .collect();
        game_ids.sort_unstable();
        game_ids
    }

    /// Return stable integrations from the matrix, sorted alphabetically.
    pub fn stable_games(&self) -> Vec<String> {
        self.game_ids_by_status(GameSupportStatus::Stable)
    }

    /// Return experimental integrations from the matrix, sorted alphabetically.
    pub fn experimental_games(&self) -> Vec<String> {
        self.game_ids_by_status(GameSupportStatus::Experimental)
    }
}

#[cfg(test)]
mod tests {
    use super::{GameSupportStatus, load_default_matrix, matrix_game_ids, normalize_game_id};

    #[test]
    fn matrix_metadata_game_ids_is_sorted_and_non_empty() -> Result<(), Box<dyn std::error::Error>>
    {
        let game_ids = matrix_game_ids()?;

        assert!(
            game_ids.windows(2).all(|pair| pair[0] <= pair[1]),
            "matrix game ids should be sorted"
        );
        assert!(!game_ids.is_empty());

        Ok(())
    }

    #[test]
    fn matrix_metadata_has_expected_status_coverage() -> Result<(), Box<dyn std::error::Error>> {
        let matrix = load_default_matrix()?;
        let stable = matrix.stable_games();
        let experimental = matrix.experimental_games();
        let total_covered = stable.len() + experimental.len();
        let mut covered = stable;
        covered.extend(experimental);
        covered.sort_unstable();

        let expected_count = matrix.games.len();
        let mut unique_covered = covered.clone();
        unique_covered.dedup();
        assert_eq!(unique_covered.len(), expected_count);

        assert_eq!(total_covered, expected_count);
        assert!(!matrix.stable_games().is_empty());
        assert!(matrix.stable_games().contains(&"iracing".to_string()));
        assert!(
            matrix
                .experimental_games()
                .contains(&"ac_rally".to_string())
        );

        Ok(())
    }

    #[test]
    fn normalize_game_id_supports_historical_aliases() {
        assert_eq!(normalize_game_id("ea_wrc"), "eawrc");
        // f1_25 is now a first-class game_id (native EA UDP protocol)
        assert_eq!(normalize_game_id("f1_25"), "f1_25");
        // f1_2025 aliases to the native EA protocol adapter
        assert_eq!(normalize_game_id("f1_2025"), "f1_25");
        // legacy Codemasters bridge adapter
        assert_eq!(normalize_game_id("f1"), "f1");
    }

    #[test]
    fn game_count_meets_minimum_regression_threshold() -> Result<(), Box<dyn std::error::Error>> {
        let game_ids = matrix_game_ids()?;
        assert!(game_ids.len() >= 15, "got {}", game_ids.len());
        Ok(())
    }
    #[test]
    fn each_game_has_valid_telemetry_rate_and_non_empty_name()
    -> Result<(), Box<dyn std::error::Error>> {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            assert!(!game.name.is_empty(), "game {} has empty name", id);
            // Only check update_rate_hz for games that actually support telemetry
            if game.telemetry.method != "none" {
                assert!(game.telemetry.update_rate_hz > 0, "game {} zero hz", id);
            }
        }
        Ok(())
    }
    #[test]
    fn most_games_have_auto_detect_identifiers() -> Result<(), Box<dyn std::error::Error>> {
        let matrix = load_default_matrix()?;
        let count = matrix
            .games
            .values()
            .filter(|g| {
                !g.auto_detect.process_names.is_empty() || !g.auto_detect.install_paths.is_empty()
            })
            .count();
        assert!(count >= 10, "expected >= 10, got {}", count);
        Ok(())
    }
    #[test]
    fn expected_games_are_present_in_matrix() -> Result<(), Box<dyn std::error::Error>> {
        let game_ids = matrix_game_ids()?;
        for game in [
            "iracing", "acc", "f1_25", "eawrc", "ams2", "rfactor2", "dirt5",
        ] {
            assert!(game_ids.contains(&game.to_string()), "missing: {}", game);
        }
        Ok(())
    }

    #[test]
    fn normalize_game_id_case_insensitive_ea_wrc() {
        assert_eq!(normalize_game_id("EA_WRC"), "eawrc");
        assert_eq!(normalize_game_id("Ea_Wrc"), "eawrc");
    }

    #[test]
    fn normalize_game_id_case_insensitive_f1_2025() {
        assert_eq!(normalize_game_id("F1_2025"), "f1_25");
        assert_eq!(normalize_game_id("f1_2025"), "f1_25");
    }

    #[test]
    fn normalize_game_id_passthrough_unknown() {
        assert_eq!(normalize_game_id("unknown_game"), "unknown_game");
        assert_eq!(normalize_game_id(""), "");
    }

    #[test]
    fn matrix_game_ids_returns_sorted() -> Result<(), Box<dyn std::error::Error>> {
        let ids = matrix_game_ids()?;
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
    fn matrix_game_id_set_contains_same_ids() -> Result<(), Box<dyn std::error::Error>> {
        let ids = matrix_game_ids()?;
        let id_set = super::matrix_game_id_set()?;
        assert_eq!(ids.len(), id_set.len());
        for id in &ids {
            assert!(id_set.contains(id), "set missing {}", id);
        }
        Ok(())
    }

    #[test]
    fn matrix_has_game_id_check() -> Result<(), Box<dyn std::error::Error>> {
        let matrix = load_default_matrix()?;
        assert!(matrix.has_game_id("iracing"));
        assert!(!matrix.has_game_id("nonexistent_game_xyz"));
        Ok(())
    }

    #[test]
    fn matrix_game_ids_method_matches_free_fn() -> Result<(), Box<dyn std::error::Error>> {
        let matrix = load_default_matrix()?;
        let method_ids = matrix.game_ids();
        let fn_ids = matrix_game_ids()?;
        assert_eq!(method_ids, fn_ids);
        Ok(())
    }

    #[test]
    fn each_game_has_at_least_one_version() -> Result<(), Box<dyn std::error::Error>> {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            assert!(!game.versions.is_empty(), "game {} has no versions", id);
        }
        Ok(())
    }

    #[test]
    fn each_game_has_config_writer() -> Result<(), Box<dyn std::error::Error>> {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            assert!(
                !game.config_writer.is_empty(),
                "game {} has empty config_writer",
                id
            );
        }
        Ok(())
    }

    #[test]
    fn game_support_status_default() {
        let status = GameSupportStatus::default();
        assert_eq!(status, GameSupportStatus::Stable);
    }

    #[test]
    fn telemetry_support_matrix_yaml_is_non_empty() {
        assert!(
            !super::TELEMETRY_SUPPORT_MATRIX_YAML.is_empty(),
            "embedded YAML must not be empty"
        );
    }

    #[test]
    fn stable_and_experimental_are_disjoint() -> Result<(), Box<dyn std::error::Error>> {
        let matrix = load_default_matrix()?;
        let stable = matrix.stable_games();
        let experimental = matrix.experimental_games();
        for game in &stable {
            assert!(
                !experimental.contains(game),
                "game {} appears in both stable and experimental",
                game
            );
        }
        Ok(())
    }
}
