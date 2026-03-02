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
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn matrix_metadata_game_ids_is_sorted_and_non_empty() -> TestResult {
        let game_ids = matrix_game_ids()?;

        assert!(
            game_ids.windows(2).all(|pair| pair[0] <= pair[1]),
            "matrix game ids should be sorted"
        );
        assert!(!game_ids.is_empty());

        Ok(())
    }

    #[test]
    fn matrix_metadata_has_expected_status_coverage() -> TestResult {
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
        assert_eq!(normalize_game_id("f1_25"), "f1_25");
        assert_eq!(normalize_game_id("f1_2025"), "f1_25");
        assert_eq!(normalize_game_id("f1"), "f1");
    }

    #[test]
    fn game_count_meets_minimum_regression_threshold() -> TestResult {
        let game_ids = matrix_game_ids()?;
        assert!(game_ids.len() >= 15, "got {}", game_ids.len());
        Ok(())
    }

    #[test]
    fn each_game_entry_has_required_fields() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            assert!(!game.name.is_empty(), "game {} empty name", id);
            assert!(!game.versions.is_empty(), "game {} no versions", id);
            assert!(
                !game.config_writer.is_empty(),
                "game {} no config_writer",
                id
            );
        }
        Ok(())
    }

    #[test]
    fn expected_games_are_present_in_matrix_config() -> TestResult {
        let game_ids = matrix_game_ids()?;
        for game in [
            "iracing", "acc", "f1_25", "eawrc", "ams2", "rfactor2", "dirt5",
        ] {
            assert!(game_ids.contains(&game.to_string()), "missing: {}", game);
        }
        Ok(())
    }

    // --- New tests below ---

    #[test]
    fn embedded_yaml_is_valid_utf8_and_non_empty() {
        assert!(
            !TELEMETRY_SUPPORT_MATRIX_YAML.is_empty(),
            "embedded YAML should not be empty"
        );
        assert!(
            TELEMETRY_SUPPORT_MATRIX_YAML.contains("games:"),
            "embedded YAML should contain top-level 'games:' key"
        );
    }

    #[test]
    fn load_default_matrix_succeeds() -> TestResult {
        let matrix = load_default_matrix()?;
        assert!(!matrix.games.is_empty());
        Ok(())
    }

    #[test]
    fn matrix_game_id_set_returns_correct_count() -> TestResult {
        let ids_vec = matrix_game_ids()?;
        let ids_set = matrix_game_id_set()?;
        assert_eq!(
            ids_vec.len(),
            ids_set.len(),
            "game id set and vec should have the same length (no duplicates)"
        );
        for id in &ids_vec {
            assert!(ids_set.contains(id), "set missing id from vec: {}", id);
        }
        Ok(())
    }

    #[test]
    fn game_support_matrix_has_game_id_returns_true_for_known() -> TestResult {
        let matrix = load_default_matrix()?;
        assert!(matrix.has_game_id("iracing"));
        assert!(matrix.has_game_id("acc"));
        assert!(!matrix.has_game_id("__nonexistent_game__"));
        Ok(())
    }

    #[test]
    fn game_support_matrix_game_ids_sorted() -> TestResult {
        let matrix = load_default_matrix()?;
        let ids = matrix.game_ids();
        assert!(
            ids.windows(2).all(|pair| pair[0] <= pair[1]),
            "game_ids() should return sorted ids"
        );
        Ok(())
    }

    #[test]
    fn game_ids_by_status_returns_subset_of_all_ids() -> TestResult {
        let matrix = load_default_matrix()?;
        let all_ids: HashSet<String> = matrix.games.keys().cloned().collect();
        for id in matrix.game_ids_by_status(GameSupportStatus::Stable) {
            assert!(all_ids.contains(&id), "stable id {} not in all ids", id);
        }
        for id in matrix.game_ids_by_status(GameSupportStatus::Experimental) {
            assert!(
                all_ids.contains(&id),
                "experimental id {} not in all ids",
                id
            );
        }
        Ok(())
    }

    #[test]
    fn each_game_has_valid_telemetry_method() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            assert!(
                !game.telemetry.method.is_empty(),
                "game {} has empty telemetry method",
                id
            );
        }
        Ok(())
    }

    #[test]
    fn stable_games_with_telemetry_have_positive_update_rate() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if game.status == GameSupportStatus::Stable && game.telemetry.method != "none" {
                assert!(
                    game.telemetry.update_rate_hz > 0,
                    "stable game {} with telemetry method '{}' has zero update_rate_hz",
                    id,
                    game.telemetry.method
                );
            }
        }
        Ok(())
    }

    #[test]
    fn each_game_version_has_valid_version_string() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            for ver in &game.versions {
                assert!(
                    !ver.version.is_empty(),
                    "game {} has empty version string",
                    id
                );
            }
        }
        Ok(())
    }

    #[test]
    fn each_game_version_has_telemetry_method() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            for ver in &game.versions {
                assert!(
                    !ver.telemetry_method.is_empty(),
                    "game {} version {} has empty telemetry_method",
                    id,
                    ver.version
                );
            }
        }
        Ok(())
    }

    #[test]
    fn games_with_360hz_option_have_high_rate_set() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if game.telemetry.supports_360hz_option {
                assert!(
                    game.telemetry.high_rate_update_rate_hz.is_some(),
                    "game {} supports 360hz but has no high_rate_update_rate_hz",
                    id
                );
            }
        }
        Ok(())
    }

    #[test]
    fn config_writer_ids_are_non_empty() -> TestResult {
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
    fn normalize_game_id_case_insensitive_aliases() {
        assert_eq!(normalize_game_id("EA_WRC"), "eawrc");
        assert_eq!(normalize_game_id("Ea_Wrc"), "eawrc");
        assert_eq!(normalize_game_id("F1_2025"), "f1_25");
        assert_eq!(normalize_game_id("F1_2025"), "f1_25");
    }

    #[test]
    fn normalize_game_id_passthrough_for_unknown() {
        assert_eq!(normalize_game_id("iracing"), "iracing");
        assert_eq!(normalize_game_id("acc"), "acc");
        assert_eq!(normalize_game_id("some_random_id"), "some_random_id");
        assert_eq!(normalize_game_id(""), "");
    }

    #[test]
    fn game_support_status_default_is_stable() {
        let status = GameSupportStatus::default();
        assert_eq!(status, GameSupportStatus::Stable);
    }

    #[test]
    fn game_support_status_serde_round_trip_stable() -> TestResult {
        let status = GameSupportStatus::Stable;
        let json = serde_json::to_string(&status)?;
        assert_eq!(json, r#""stable""#);
        let decoded: GameSupportStatus = serde_json::from_str(&json)?;
        assert_eq!(decoded, GameSupportStatus::Stable);
        Ok(())
    }

    #[test]
    fn game_support_status_serde_round_trip_experimental() -> TestResult {
        let status = GameSupportStatus::Experimental;
        let json = serde_json::to_string(&status)?;
        assert_eq!(json, r#""experimental""#);
        let decoded: GameSupportStatus = serde_json::from_str(&json)?;
        assert_eq!(decoded, GameSupportStatus::Experimental);
        Ok(())
    }

    #[test]
    fn game_support_matrix_serde_yaml_round_trip() -> TestResult {
        let matrix = load_default_matrix()?;
        let yaml_str = serde_yaml::to_string(&matrix)?;
        let decoded: GameSupportMatrix = serde_yaml::from_str(&yaml_str)?;
        assert_eq!(matrix.games.len(), decoded.games.len());
        for key in matrix.games.keys() {
            assert!(
                decoded.games.contains_key(key),
                "round-trip lost game key: {}",
                key
            );
        }
        Ok(())
    }

    #[test]
    fn game_support_matrix_serde_json_round_trip() -> TestResult {
        let matrix = load_default_matrix()?;
        let json_str = serde_json::to_string(&matrix)?;
        let decoded: GameSupportMatrix = serde_json::from_str(&json_str)?;
        assert_eq!(matrix.games.len(), decoded.games.len());
        Ok(())
    }

    #[test]
    fn telemetry_field_mapping_serde_round_trip() -> TestResult {
        let mapping = TelemetryFieldMapping {
            ffb_scalar: Some("SteeringWheelPctTorqueSign".to_string()),
            rpm: Some("RPM".to_string()),
            speed_ms: Some("Speed".to_string()),
            slip_ratio: Some("LFSlipRatio".to_string()),
            gear: Some("Gear".to_string()),
            flags: Some("SessionFlags".to_string()),
            car_id: Some("CarPath".to_string()),
            track_id: Some("TrackName".to_string()),
        };
        let json = serde_json::to_string(&mapping)?;
        let decoded: TelemetryFieldMapping = serde_json::from_str(&json)?;
        assert_eq!(decoded.ffb_scalar, mapping.ffb_scalar);
        assert_eq!(decoded.rpm, mapping.rpm);
        assert_eq!(decoded.speed_ms, mapping.speed_ms);
        assert_eq!(decoded.gear, mapping.gear);
        Ok(())
    }

    #[test]
    fn telemetry_field_mapping_all_none_round_trip() -> TestResult {
        let mapping = TelemetryFieldMapping {
            ffb_scalar: None,
            rpm: None,
            speed_ms: None,
            slip_ratio: None,
            gear: None,
            flags: None,
            car_id: None,
            track_id: None,
        };
        let json = serde_json::to_string(&mapping)?;
        let decoded: TelemetryFieldMapping = serde_json::from_str(&json)?;
        assert!(decoded.ffb_scalar.is_none());
        assert!(decoded.rpm.is_none());
        Ok(())
    }

    #[test]
    fn auto_detect_config_serde_round_trip() -> TestResult {
        let config = AutoDetectConfig {
            process_names: vec!["iRacingSim64DX11.exe".to_string()],
            install_registry_keys: vec!["HKCU\\Software\\iRacing".to_string()],
            install_paths: vec!["Program Files (x86)/iRacing".to_string()],
        };
        let json = serde_json::to_string(&config)?;
        let decoded: AutoDetectConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.process_names, config.process_names);
        assert_eq!(decoded.install_registry_keys, config.install_registry_keys);
        assert_eq!(decoded.install_paths, config.install_paths);
        Ok(())
    }

    #[test]
    fn game_version_serde_round_trip() -> TestResult {
        let version = GameVersion {
            version: "2024.x".to_string(),
            config_paths: vec!["Documents/iRacing/app.ini".to_string()],
            executable_patterns: vec!["iRacingSim64DX11.exe".to_string()],
            telemetry_method: "shared_memory".to_string(),
            supported_fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
        };
        let json = serde_json::to_string(&version)?;
        let decoded: GameVersion = serde_json::from_str(&json)?;
        assert_eq!(decoded.version, version.version);
        assert_eq!(decoded.config_paths, version.config_paths);
        assert_eq!(decoded.executable_patterns, version.executable_patterns);
        assert_eq!(decoded.telemetry_method, version.telemetry_method);
        assert_eq!(decoded.supported_fields, version.supported_fields);
        Ok(())
    }

    #[test]
    fn telemetry_support_serde_round_trip() -> TestResult {
        let support = TelemetrySupport {
            method: "shared_memory".to_string(),
            update_rate_hz: 60,
            supports_360hz_option: true,
            high_rate_update_rate_hz: Some(360),
            output_target: Some("127.0.0.1:12345".to_string()),
            fields: TelemetryFieldMapping {
                ffb_scalar: Some("SteeringWheelPctTorqueSign".to_string()),
                rpm: None,
                speed_ms: None,
                slip_ratio: None,
                gear: None,
                flags: None,
                car_id: None,
                track_id: None,
            },
        };
        let json = serde_json::to_string(&support)?;
        let decoded: TelemetrySupport = serde_json::from_str(&json)?;
        assert_eq!(decoded.method, support.method);
        assert_eq!(decoded.update_rate_hz, support.update_rate_hz);
        assert_eq!(decoded.supports_360hz_option, support.supports_360hz_option);
        assert_eq!(
            decoded.high_rate_update_rate_hz,
            support.high_rate_update_rate_hz
        );
        assert_eq!(decoded.output_target, support.output_target);
        Ok(())
    }

    #[test]
    fn telemetry_support_defaults_for_optional_fields() -> TestResult {
        let json = r#"{
            "method": "udp",
            "update_rate_hz": 60,
            "output_target": null,
            "fields": {
                "ffb_scalar": null,
                "rpm": null,
                "speed_ms": null,
                "slip_ratio": null,
                "gear": null,
                "flags": null,
                "car_id": null,
                "track_id": null
            }
        }"#;
        let decoded: TelemetrySupport = serde_json::from_str(json)?;
        assert!(!decoded.supports_360hz_option);
        assert!(decoded.high_rate_update_rate_hz.is_none());
        Ok(())
    }

    #[test]
    fn invalid_yaml_returns_error() {
        let bad_yaml = "games:\n  - this is not valid: [";
        let result = serde_yaml::from_str::<GameSupportMatrix>(bad_yaml);
        assert!(result.is_err());
    }

    #[test]
    fn iracing_game_has_expected_telemetry_properties() -> TestResult {
        let matrix = load_default_matrix()?;
        let iracing = matrix
            .games
            .get("iracing")
            .ok_or("iracing not found in matrix")?;
        assert_eq!(iracing.telemetry.method, "shared_memory");
        assert!(iracing.telemetry.supports_360hz_option);
        assert_eq!(iracing.telemetry.high_rate_update_rate_hz, Some(360));
        assert!(iracing.telemetry.fields.ffb_scalar.is_some());
        assert!(iracing.telemetry.fields.rpm.is_some());
        Ok(())
    }

    #[test]
    fn iracing_auto_detect_has_process_names() -> TestResult {
        let matrix = load_default_matrix()?;
        let iracing = matrix
            .games
            .get("iracing")
            .ok_or("iracing not found in matrix")?;
        assert!(
            !iracing.auto_detect.process_names.is_empty(),
            "iRacing should have auto-detect process names"
        );
        Ok(())
    }

    #[test]
    fn stable_games_with_telemetry_have_at_least_one_field_mapped() -> TestResult {
        let matrix = load_default_matrix()?;
        for (id, game) in &matrix.games {
            if game.status != GameSupportStatus::Stable || game.telemetry.method == "none" {
                continue;
            }
            let fields = &game.telemetry.fields;
            let has_any = fields.ffb_scalar.is_some()
                || fields.rpm.is_some()
                || fields.speed_ms.is_some()
                || fields.slip_ratio.is_some()
                || fields.gear.is_some()
                || fields.flags.is_some()
                || fields.car_id.is_some()
                || fields.track_id.is_some();
            assert!(
                has_any,
                "stable game {} with telemetry should have at least one field mapped",
                id
            );
        }
        Ok(())
    }

    #[test]
    fn game_support_matrix_game_ids_matches_keys() -> TestResult {
        let matrix = load_default_matrix()?;
        let ids = matrix.game_ids();
        let mut keys: Vec<String> = matrix.games.keys().cloned().collect();
        keys.sort_unstable();
        assert_eq!(ids, keys);
        Ok(())
    }

    #[test]
    fn stable_and_experimental_partition_covers_all_games() -> TestResult {
        let matrix = load_default_matrix()?;
        let stable: HashSet<String> = matrix.stable_games().into_iter().collect();
        let experimental: HashSet<String> = matrix.experimental_games().into_iter().collect();
        let all: HashSet<String> = matrix.games.keys().cloned().collect();
        let union: HashSet<String> = stable.union(&experimental).cloned().collect();
        assert_eq!(union, all, "stable + experimental should cover all games");
        let intersection: HashSet<String> = stable.intersection(&experimental).cloned().collect();
        assert!(
            intersection.is_empty(),
            "no game should be both stable and experimental: {:?}",
            intersection
        );
        Ok(())
    }
}
