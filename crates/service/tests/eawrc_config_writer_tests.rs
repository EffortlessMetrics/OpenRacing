//! Integration tests for the EA WRC config writer.

use racing_wheel_service::config_writers::EAWRCConfigWriter;
use racing_wheel_service::game_service::{ConfigWriter, TelemetryConfig};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[track_caller]
fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected Err: {error:?}"),
    }
}

#[test]
fn test_eawrc_writer_creates_structure_and_patches_config() -> TestResult {
    let writer = EAWRCConfigWriter;
    let temp_dir = must(tempfile::tempdir());
    let telemetry_root = temp_dir.path().join("Documents/My Games/WRC/telemetry");
    let udp_dir = telemetry_root.join("udp");
    must(std::fs::create_dir_all(&udp_dir));

    let existing_config = serde_json::json!({
        "udp": {
            "packetAssignments": [
                {
                    "packetId": "session_update",
                    "structureId": "openracing",
                    "ip": "127.0.0.1",
                    "port": 20778,
                    "frequencyHz": 60,
                    "bEnabled": false
                }
            ]
        }
    });
    must(std::fs::write(
        telemetry_root.join("config.json"),
        serde_json::to_vec_pretty(&existing_config)?,
    ));

    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 120,
        output_method: "udp_schema".to_string(),
        output_target: "127.0.0.1:20790".to_string(),
        fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    };

    let diffs = writer.write_config(temp_dir.path(), &config)?;
    assert_eq!(diffs.len(), 2);
    assert!(writer.validate_config(temp_dir.path())?);

    let structure_path = telemetry_root.join("udp/openracing.json");
    assert!(structure_path.exists());

    let config_value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
        telemetry_root.join("config.json"),
    )?)?;
    let assignments = config_value
        .get("udp")
        .and_then(serde_json::Value::as_object)
        .and_then(|udp| udp.get("packetAssignments"))
        .and_then(serde_json::Value::as_array)
        .ok_or("missing packetAssignments array")?;

    let updated = assignments
        .iter()
        .find(|entry| {
            entry
                .get("packetId")
                .and_then(serde_json::Value::as_str)
                .map(|value| value == "session_update")
                .unwrap_or(false)
        })
        .ok_or("missing session_update assignment")?;

    assert_eq!(
        updated.get("structureId"),
        Some(&serde_json::Value::String("openracing".to_string()))
    );
    assert_eq!(
        updated.get("ip"),
        Some(&serde_json::Value::String("127.0.0.1".to_string()))
    );
    assert_eq!(
        updated.get("port"),
        Some(&serde_json::Value::from(20790u16))
    );
    assert_eq!(
        updated.get("frequencyHz"),
        Some(&serde_json::Value::from(120u32))
    );
    assert_eq!(
        updated.get("bEnabled"),
        Some(&serde_json::Value::Bool(true))
    );

    Ok(())
}
