//! Game support matrix shim backed by shared telemetry metadata.
//!
//! This module is intentionally lightweight and delegates schema ownership to
//! `racing-wheel-telemetry-config`.

use std::collections::HashMap;

use racing_wheel_telemetry_config::support::load_default_matrix;
pub use racing_wheel_telemetry_config::support::{
    AutoDetectConfig, GameSupport, GameSupportMatrix, GameVersion, TelemetryFieldMapping,
    TelemetrySupport,
};
use tracing::warn;

/// Create the canonical default matrix from shared telemetry metadata.
pub fn create_default_matrix() -> GameSupportMatrix {
    load_default_matrix().unwrap_or_else(|err| {
        warn!(
            error = %err,
            "Failed to load default telemetry support matrix; falling back to empty matrix"
        );

        GameSupportMatrix {
            games: HashMap::new(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_create_default_matrix_returns_non_empty() -> Result<()> {
        let matrix = create_default_matrix();
        assert!(
            !matrix.games.is_empty(),
            "Default matrix should contain at least one game"
        );
        Ok(())
    }

    #[test]
    fn test_matrix_contains_iracing() -> Result<()> {
        let matrix = create_default_matrix();
        assert!(
            matrix.games.contains_key("iracing"),
            "Matrix should contain iRacing"
        );
        let iracing = matrix
            .games
            .get("iracing")
            .ok_or_else(|| anyhow::anyhow!("iracing missing"))?;
        assert_eq!(iracing.name, "iRacing");
        assert!(!iracing.versions.is_empty(), "iRacing should have versions");
        Ok(())
    }

    #[test]
    fn test_matrix_contains_acc() -> Result<()> {
        let matrix = create_default_matrix();
        let acc = matrix
            .games
            .get("acc")
            .ok_or_else(|| anyhow::anyhow!("acc missing"))?;
        assert_eq!(acc.name, "Assetto Corsa Competizione");
        assert!(
            acc.telemetry.update_rate_hz > 0,
            "ACC should have a positive update rate"
        );
        Ok(())
    }

    #[test]
    fn test_matrix_game_lookup_nonexistent() -> Result<()> {
        let matrix = create_default_matrix();
        assert!(
            !matrix.games.contains_key("nonexistent_game_xyz"),
            "Nonexistent game should return None"
        );
        Ok(())
    }

    #[test]
    fn test_all_games_have_required_fields() -> Result<()> {
        let matrix = create_default_matrix();
        for (game_id, game) in &matrix.games {
            assert!(
                !game.name.is_empty(),
                "Game '{}' should have a non-empty name",
                game_id
            );
            assert!(
                !game.versions.is_empty(),
                "Game '{}' should have at least one version",
                game_id
            );
            assert!(
                !game.telemetry.method.is_empty(),
                "Game '{}' should have a telemetry method",
                game_id
            );
        }
        Ok(())
    }

    #[test]
    fn test_auto_detect_config_populated() -> Result<()> {
        let matrix = create_default_matrix();
        let iracing = matrix
            .games
            .get("iracing")
            .ok_or_else(|| anyhow::anyhow!("iracing missing"))?;
        assert!(
            !iracing.auto_detect.process_names.is_empty(),
            "iRacing should have auto-detect process names"
        );
        Ok(())
    }

    #[test]
    fn test_telemetry_field_mapping_present() -> Result<()> {
        let matrix = create_default_matrix();
        let iracing = matrix
            .games
            .get("iracing")
            .ok_or_else(|| anyhow::anyhow!("iracing missing"))?;
        assert!(
            iracing.telemetry.fields.ffb_scalar.is_some(),
            "iRacing should map the ffb_scalar field"
        );
        assert!(
            iracing.telemetry.fields.rpm.is_some(),
            "iRacing should map the rpm field"
        );
        Ok(())
    }
}
