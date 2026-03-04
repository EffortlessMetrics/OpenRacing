//! Extended write → validate round-trip coverage for game config writers.
//!
//! The existing `comprehensive.rs` tests cover iracing, acc, eawrc, rfactor2,
//! dirt5, and ams2. This file adds round-trip tests for the remaining writer
//! families to ensure every major protocol category is exercised.

use racing_wheel_telemetry_config_writers::{
    ConfigDiff, ConfigWriter, DiffOperation, TelemetryConfig, config_writer_factories,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn default_config() -> TelemetryConfig {
    TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    }
}

fn writer_for(
    game_id: &str,
) -> Result<Box<dyn ConfigWriter + Send + Sync>, Box<dyn std::error::Error>> {
    config_writer_factories()
        .iter()
        .find(|(id, _)| *id == game_id)
        .map(|(_, f)| f())
        .ok_or_else(|| format!("{game_id} factory not found").into())
}

fn write_validate_round_trip(game_id: &str) -> TestResult {
    let writer = writer_for(game_id)?;
    let temp_dir = tempfile::tempdir()?;
    let config = default_config();
    let diffs = writer.write_config(temp_dir.path(), &config)?;
    assert!(
        !diffs.is_empty(),
        "{game_id}: write_config produced no diffs"
    );
    assert!(
        writer.validate_config(temp_dir.path())?,
        "{game_id}: validate_config returned false after write"
    );
    Ok(())
}

// ── Codemasters UDP bridge writers ──────────────────────────────────────

#[test]
fn f1_write_and_validate() -> TestResult {
    write_validate_round_trip("f1")
}

#[test]
fn f1_25_write_and_validate() -> TestResult {
    write_validate_round_trip("f1_25")
}

#[test]
fn f1_native_write_and_validate() -> TestResult {
    write_validate_round_trip("f1_native")
}

#[test]
fn dirt_rally_2_write_and_validate() -> TestResult {
    write_validate_round_trip("dirt_rally_2")
}

#[test]
fn dirt4_write_and_validate() -> TestResult {
    write_validate_round_trip("dirt4")
}

#[test]
fn dirt3_write_and_validate() -> TestResult {
    write_validate_round_trip("dirt3")
}

#[test]
fn wrc_generations_write_and_validate() -> TestResult {
    write_validate_round_trip("wrc_generations")
}

#[test]
fn wrc_9_write_and_validate() -> TestResult {
    write_validate_round_trip("wrc_9")
}

#[test]
fn wrc_10_write_and_validate() -> TestResult {
    write_validate_round_trip("wrc_10")
}

#[test]
fn grid_autosport_write_and_validate() -> TestResult {
    write_validate_round_trip("grid_autosport")
}

#[test]
fn grid_2019_write_and_validate() -> TestResult {
    write_validate_round_trip("grid_2019")
}

#[test]
fn grid_legends_write_and_validate() -> TestResult {
    write_validate_round_trip("grid_legends")
}

#[test]
fn race_driver_grid_write_and_validate() -> TestResult {
    write_validate_round_trip("race_driver_grid")
}

// ── Forza data-out writers ──────────────────────────────────────────────

#[test]
fn forza_motorsport_write_and_validate() -> TestResult {
    write_validate_round_trip("forza_motorsport")
}

#[test]
fn forza_horizon_4_write_and_validate() -> TestResult {
    write_validate_round_trip("forza_horizon_4")
}

#[test]
fn forza_horizon_5_write_and_validate() -> TestResult {
    write_validate_round_trip("forza_horizon_5")
}

// ── Console / encrypted protocol writers ────────────────────────────────

#[test]
fn gran_turismo_7_write_and_validate() -> TestResult {
    write_validate_round_trip("gran_turismo_7")
}

#[test]
fn gran_turismo_sport_write_and_validate() -> TestResult {
    write_validate_round_trip("gran_turismo_sport")
}

// ── Outgauge / unique protocol writers ──────────────────────────────────

#[test]
fn assetto_corsa_write_and_validate() -> TestResult {
    write_validate_round_trip("assetto_corsa")
}

#[test]
fn beamng_drive_write_and_validate() -> TestResult {
    write_validate_round_trip("beamng_drive")
}

#[test]
fn project_cars_2_write_and_validate() -> TestResult {
    write_validate_round_trip("project_cars_2")
}

#[test]
fn project_cars_3_write_and_validate() -> TestResult {
    write_validate_round_trip("project_cars_3")
}

#[test]
fn live_for_speed_write_and_validate() -> TestResult {
    write_validate_round_trip("live_for_speed")
}

#[test]
fn rbr_write_and_validate() -> TestResult {
    write_validate_round_trip("rbr")
}

// ── Truck sim / shared memory writers ───────────────────────────────────

#[test]
fn ets2_write_and_validate() -> TestResult {
    write_validate_round_trip("ets2")
}

#[test]
fn ats_write_and_validate() -> TestResult {
    write_validate_round_trip("ats")
}

#[test]
fn automobilista_write_and_validate() -> TestResult {
    write_validate_round_trip("automobilista")
}

#[test]
fn raceroom_write_and_validate() -> TestResult {
    write_validate_round_trip("raceroom")
}

// ── Miscellaneous writers ───────────────────────────────────────────────

#[test]
fn wreckfest_write_and_validate() -> TestResult {
    write_validate_round_trip("wreckfest")
}

#[test]
fn flatout_write_and_validate() -> TestResult {
    write_validate_round_trip("flatout")
}

#[test]
fn dakar_desert_rally_write_and_validate() -> TestResult {
    write_validate_round_trip("dakar_desert_rally")
}

#[test]
fn rennsport_write_and_validate() -> TestResult {
    write_validate_round_trip("rennsport")
}

#[test]
fn kartkraft_write_and_validate() -> TestResult {
    write_validate_round_trip("kartkraft")
}

#[test]
fn nascar_write_and_validate() -> TestResult {
    write_validate_round_trip("nascar")
}

#[test]
fn nascar_21_write_and_validate() -> TestResult {
    write_validate_round_trip("nascar_21")
}

#[test]
fn le_mans_ultimate_write_and_validate() -> TestResult {
    write_validate_round_trip("le_mans_ultimate")
}

#[test]
fn wtcr_write_and_validate() -> TestResult {
    write_validate_round_trip("wtcr")
}

#[test]
fn trackmania_write_and_validate() -> TestResult {
    write_validate_round_trip("trackmania")
}

#[test]
fn simhub_write_and_validate() -> TestResult {
    write_validate_round_trip("simhub")
}

#[test]
fn mudrunner_write_and_validate() -> TestResult {
    write_validate_round_trip("mudrunner")
}

#[test]
fn snowrunner_write_and_validate() -> TestResult {
    write_validate_round_trip("snowrunner")
}

#[test]
fn motogp_write_and_validate() -> TestResult {
    write_validate_round_trip("motogp")
}

#[test]
fn ride5_write_and_validate() -> TestResult {
    write_validate_round_trip("ride5")
}

#[test]
fn rfactor1_write_and_validate() -> TestResult {
    write_validate_round_trip("rfactor1")
}

#[test]
fn gtr2_write_and_validate() -> TestResult {
    write_validate_round_trip("gtr2")
}

#[test]
fn race_07_write_and_validate() -> TestResult {
    write_validate_round_trip("race_07")
}

#[test]
fn gsc_write_and_validate() -> TestResult {
    write_validate_round_trip("gsc")
}

#[test]
fn v_rally_4_write_and_validate() -> TestResult {
    write_validate_round_trip("v_rally_4")
}

#[test]
fn gravel_write_and_validate() -> TestResult {
    write_validate_round_trip("gravel")
}

#[test]
fn seb_loeb_rally_write_and_validate() -> TestResult {
    write_validate_round_trip("seb_loeb_rally")
}

#[test]
fn acc2_write_and_validate() -> TestResult {
    write_validate_round_trip("acc2")
}

#[test]
fn ac_evo_write_and_validate() -> TestResult {
    write_validate_round_trip("ac_evo")
}

#[test]
fn ac_rally_write_and_validate() -> TestResult {
    write_validate_round_trip("ac_rally")
}

#[test]
fn dirt_showdown_write_and_validate() -> TestResult {
    write_validate_round_trip("dirt_showdown")
}

#[test]
fn f1_manager_write_and_validate() -> TestResult {
    write_validate_round_trip("f1_manager")
}

// ── Overwrite (Modify) round-trip ───────────────────────────────────────

#[test]
fn overwrite_produces_modify_operation() -> TestResult {
    let writer = writer_for("f1")?;
    let temp_dir = tempfile::tempdir()?;
    let config = default_config();

    // First write → Add
    let diffs1 = writer.write_config(temp_dir.path(), &config)?;
    assert_eq!(diffs1[0].operation, DiffOperation::Add);

    // Second write → Modify
    let diffs2 = writer.write_config(temp_dir.path(), &config)?;
    assert_eq!(diffs2[0].operation, DiffOperation::Modify);
    assert!(diffs2[0].old_value.is_some());
    Ok(())
}

// ── get_expected_diffs consistency ──────────────────────────────────────

#[test]
fn expected_diffs_count_matches_write_for_all_writers() -> TestResult {
    let config = default_config();
    for (id, factory) in config_writer_factories() {
        let writer = factory();
        let temp_dir = tempfile::tempdir()?;
        let write_diffs = writer.write_config(temp_dir.path(), &config)?;
        let expected_diffs = writer.get_expected_diffs(&config)?;
        assert_eq!(
            write_diffs.len(),
            expected_diffs.len(),
            "{id}: write vs expected diff count mismatch"
        );
    }
    Ok(())
}

// ── DiffOperation serde ─────────────────────────────────────────────────

#[test]
fn diff_operation_serde_round_trip_all_variants() -> TestResult {
    for op in [
        DiffOperation::Add,
        DiffOperation::Modify,
        DiffOperation::Remove,
    ] {
        let json = serde_json::to_string(&op)?;
        let decoded: DiffOperation = serde_json::from_str(&json)?;
        assert_eq!(decoded, op);
    }
    Ok(())
}

#[test]
fn config_diff_debug_is_not_empty() -> TestResult {
    let diff = ConfigDiff {
        file_path: "test.json".to_string(),
        section: None,
        key: "key".to_string(),
        old_value: None,
        new_value: "val".to_string(),
        operation: DiffOperation::Add,
    };
    let debug = format!("{diff:?}");
    assert!(debug.contains("ConfigDiff"));
    Ok(())
}

// ── TelemetryConfig edge cases ──────────────────────────────────────────

#[test]
fn disabled_config_still_writes() -> TestResult {
    let writer = writer_for("f1")?;
    let temp_dir = tempfile::tempdir()?;
    let config = TelemetryConfig {
        enabled: false,
        update_rate_hz: 0,
        output_method: "none".to_string(),
        output_target: "".to_string(),
        fields: vec![],
        enable_high_rate_iracing_360hz: false,
    };
    let diffs = writer.write_config(temp_dir.path(), &config)?;
    assert!(!diffs.is_empty());
    Ok(())
}

#[test]
fn empty_fields_config_writes_successfully() -> TestResult {
    let writer = writer_for("acc")?;
    let temp_dir = tempfile::tempdir()?;
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 120,
        output_method: "udp_broadcast".to_string(),
        output_target: "127.0.0.1:9000".to_string(),
        fields: vec![],
        enable_high_rate_iracing_360hz: false,
    };
    let diffs = writer.write_config(temp_dir.path(), &config)?;
    assert!(!diffs.is_empty());
    assert!(writer.validate_config(temp_dir.path())?);
    Ok(())
}
