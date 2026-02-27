pub use racing_wheel_telemetry_config_writers::*;

#[cfg(test)]
mod tests {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    #[test]
    fn config_writer_factories_is_non_empty() { assert!(!config_writer_factories().is_empty()); }
    #[test]
    fn config_writer_factories_contains_known_game_ids() {
        let ids: Vec<&str> = config_writer_factories().iter().map(|(id,_)| *id).collect();
        for expected in ["iracing","acc","ams2","rfactor2","eawrc"] {
            assert!(ids.contains(&expected), "missing: {}", expected);
        }
    }
    #[test]
    fn config_writer_factories_does_not_contain_unknown() {
        let ids: Vec<&str> = config_writer_factories().iter().map(|(id,_)| *id).collect();
        assert!(!ids.contains(&"__no_such_game__"));
    }
    #[test]
    fn telemetry_config_serde_round_trip() -> TestResult {
        let config = TelemetryConfig { enabled: true, update_rate_hz: 60,
            output_method: "udp".to_string(), output_target: "127.0.0.1:9999".to_string(),
            fields: vec!["rpm".to_string()], enable_high_rate_iracing_360hz: false };
        let json = serde_json::to_string(&config)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.enabled, config.enabled);
        assert_eq!(decoded.update_rate_hz, config.update_rate_hz);
        Ok(())
    }
}
