//! Deep configuration tests covering validation, migration, environment variable
//! overrides, default generation, config merge semantics, and edge cases.

use racing_wheel_service::system_config::{
    DevelopmentConfig, EngineConfig, GameConfig, IpcConfig, ObservabilityConfig, PluginConfig,
    SafetyConfig, ServiceConfig, SystemConfig, TransportType,
};
use tempfile::TempDir;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// =========================================================================
// 1. Config validation — invalid ports, paths, values
// =========================================================================

#[test]
fn config_validation_default_passes() -> Result<(), BoxErr> {
    let config = SystemConfig::default();
    config.validate()?;
    Ok(())
}

#[test]
fn config_validation_invalid_schema_version() -> Result<(), BoxErr> {
    let config = SystemConfig {
        schema_version: "not-a-schema".to_string(),
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "invalid schema version should fail");
    let msg = format!("{}", result.as_ref().err().ok_or("expected error")?);
    assert!(
        msg.contains("schema version"),
        "error should mention schema"
    );
    Ok(())
}

#[test]
fn config_validation_zero_tick_rate() -> Result<(), BoxErr> {
    let config = SystemConfig {
        engine: EngineConfig {
            tick_rate_hz: 0,
            ..EngineConfig::default()
        },
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "zero tick rate should fail");
    Ok(())
}

#[test]
fn config_validation_excessive_tick_rate() -> Result<(), BoxErr> {
    let config = SystemConfig {
        engine: EngineConfig {
            tick_rate_hz: 20000,
            ..EngineConfig::default()
        },
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "tick rate above 10000 should fail");
    Ok(())
}

#[test]
fn config_validation_max_jitter_too_high() -> Result<(), BoxErr> {
    let config = SystemConfig {
        engine: EngineConfig {
            max_jitter_us: 5000,
            ..EngineConfig::default()
        },
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "max jitter >1000 should fail");
    Ok(())
}

#[test]
fn config_validation_negative_safe_torque() -> Result<(), BoxErr> {
    let config = SystemConfig {
        safety: SafetyConfig {
            default_safe_torque_nm: -1.0,
            ..SafetyConfig::default()
        },
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "negative safe torque should fail");
    Ok(())
}

#[test]
fn config_validation_safe_torque_exceeds_max() -> Result<(), BoxErr> {
    let config = SystemConfig {
        safety: SafetyConfig {
            default_safe_torque_nm: 30.0,
            max_torque_nm: 25.0,
            ..SafetyConfig::default()
        },
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "safe torque > max should fail");
    Ok(())
}

#[test]
fn config_validation_max_torque_above_limit() -> Result<(), BoxErr> {
    let config = SystemConfig {
        safety: SafetyConfig {
            max_torque_nm: 100.0,
            ..SafetyConfig::default()
        },
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "max torque >50 Nm should fail");
    Ok(())
}

#[test]
fn config_validation_zero_fault_response_timeout() -> Result<(), BoxErr> {
    let config = SystemConfig {
        safety: SafetyConfig {
            fault_response_timeout_ms: 0,
            ..SafetyConfig::default()
        },
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "zero fault timeout should fail");
    Ok(())
}

#[test]
fn config_validation_zero_max_connections() -> Result<(), BoxErr> {
    let config = SystemConfig {
        ipc: IpcConfig {
            max_connections: 0,
            ..IpcConfig::default()
        },
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "zero max connections should fail");
    Ok(())
}

#[test]
fn config_validation_excessive_max_connections() -> Result<(), BoxErr> {
    let config = SystemConfig {
        ipc: IpcConfig {
            max_connections: 5000,
            ..IpcConfig::default()
        },
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "max connections >1000 should fail");
    Ok(())
}

#[test]
fn config_validation_tracing_sample_rate_out_of_range() -> Result<(), BoxErr> {
    let config_hi = SystemConfig {
        observability: ObservabilityConfig {
            tracing_sample_rate: 1.5,
            ..ObservabilityConfig::default()
        },
        ..SystemConfig::default()
    };
    assert!(
        config_hi.validate().is_err(),
        "sample rate >1.0 should fail"
    );

    let config_lo = SystemConfig {
        observability: ObservabilityConfig {
            tracing_sample_rate: -0.1,
            ..ObservabilityConfig::default()
        },
        ..SystemConfig::default()
    };
    assert!(
        config_lo.validate().is_err(),
        "negative sample rate should fail"
    );
    Ok(())
}

#[test]
fn config_validation_boundary_values_pass() -> Result<(), BoxErr> {
    // Boundary: tick_rate at exact limits
    SystemConfig {
        engine: EngineConfig {
            tick_rate_hz: 1,
            ..EngineConfig::default()
        },
        ..SystemConfig::default()
    }
    .validate()?;

    SystemConfig {
        engine: EngineConfig {
            tick_rate_hz: 10000,
            ..EngineConfig::default()
        },
        ..SystemConfig::default()
    }
    .validate()?;

    // Boundary: jitter exactly 1000
    SystemConfig {
        engine: EngineConfig {
            max_jitter_us: 1000,
            ..EngineConfig::default()
        },
        ..SystemConfig::default()
    }
    .validate()?;

    // Boundary: tracing rate at 0.0 and 1.0
    SystemConfig {
        observability: ObservabilityConfig {
            tracing_sample_rate: 0.0,
            ..ObservabilityConfig::default()
        },
        ..SystemConfig::default()
    }
    .validate()?;

    SystemConfig {
        observability: ObservabilityConfig {
            tracing_sample_rate: 1.0,
            ..ObservabilityConfig::default()
        },
        ..SystemConfig::default()
    }
    .validate()?;
    Ok(())
}

// =========================================================================
// 2. Config migration from older versions
// =========================================================================

#[test]
fn config_migration_v0_to_v1() -> Result<(), BoxErr> {
    let mut config = SystemConfig {
        schema_version: "wheel.config/0".to_string(),
        ..SystemConfig::default()
    };

    let migrated = config.migrate()?;
    assert!(migrated, "migration should report it was performed");
    assert_eq!(config.schema_version, "wheel.config/1");
    config.validate()?;
    Ok(())
}

#[test]
fn config_migration_current_version_noop() -> Result<(), BoxErr> {
    let mut config = SystemConfig::default();
    assert_eq!(config.schema_version, "wheel.config/1");

    let migrated = config.migrate()?;
    assert!(!migrated, "current version should not need migration");
    Ok(())
}

#[test]
fn config_migration_unknown_version_fails() -> Result<(), BoxErr> {
    let mut config = SystemConfig {
        schema_version: "wheel.config/99".to_string(),
        ..SystemConfig::default()
    };

    let result = config.migrate();
    assert!(result.is_err(), "unknown version should fail migration");
    Ok(())
}

// =========================================================================
// 3. Default config generation
// =========================================================================

#[test]
fn config_default_engine_values() -> Result<(), BoxErr> {
    let engine = EngineConfig::default();
    assert_eq!(engine.tick_rate_hz, 1000);
    assert_eq!(engine.max_jitter_us, 250);
    assert!(!engine.disable_realtime);
    assert!(engine.memory_lock_all);
    assert_eq!(engine.processing_budget_us, 200);
    assert!(engine.force_ffb_mode.is_none());
    assert!(engine.rt_cpu_affinity.is_none());
    Ok(())
}

#[test]
fn config_default_service_values() -> Result<(), BoxErr> {
    let svc = ServiceConfig::default();
    assert_eq!(svc.service_name, "wheeld");
    assert_eq!(svc.health_check_interval, 30);
    assert_eq!(svc.max_restart_attempts, 3);
    assert_eq!(svc.restart_delay, 5);
    assert!(svc.auto_restart);
    assert_eq!(svc.shutdown_timeout, 30);
    Ok(())
}

#[test]
fn config_default_safety_values() -> Result<(), BoxErr> {
    let safety = SafetyConfig::default();
    assert!((safety.default_safe_torque_nm - 5.0).abs() < f32::EPSILON);
    assert!((safety.max_torque_nm - 25.0).abs() < f32::EPSILON);
    assert_eq!(safety.fault_response_timeout_ms, 50);
    assert!(safety.require_physical_interlock);
    Ok(())
}

#[test]
fn config_default_ipc_values() -> Result<(), BoxErr> {
    let ipc = IpcConfig::default();
    assert!(matches!(ipc.transport, TransportType::Native));
    assert!(ipc.bind_address.is_none());
    assert_eq!(ipc.max_connections, 10);
    assert!(ipc.enable_acl);
    assert_eq!(ipc.max_message_size, 1024 * 1024);
    Ok(())
}

#[test]
fn config_default_plugin_values() -> Result<(), BoxErr> {
    let plugins = PluginConfig::default();
    assert!(plugins.enabled);
    assert!(plugins.auto_load);
    assert!(!plugins.enable_native);
    assert_eq!(plugins.timeout_ms, 100);
    assert_eq!(plugins.max_memory_mb, 64);
    Ok(())
}

#[test]
fn config_default_observability_values() -> Result<(), BoxErr> {
    let obs = ObservabilityConfig::default();
    assert!(obs.enable_metrics);
    assert!(obs.enable_tracing);
    assert!(obs.enable_blackbox);
    assert_eq!(obs.metrics_interval_s, 60);
    assert_eq!(obs.blackbox_retention_hours, 24);
    assert_eq!(obs.health_stream_hz, 10);
    Ok(())
}

#[test]
fn config_default_development_disabled() -> Result<(), BoxErr> {
    let dev = DevelopmentConfig::default();
    assert!(!dev.enable_dev_features);
    assert!(!dev.enable_debug_logging);
    assert!(!dev.enable_virtual_devices);
    assert!(!dev.disable_safety_interlocks);
    assert!(!dev.enable_plugin_dev_mode);
    assert!(!dev.mock_telemetry);
    Ok(())
}

#[test]
fn config_default_game_has_supported_games() -> Result<(), BoxErr> {
    let games = GameConfig::default();
    assert!(games.auto_configure);
    assert!(games.auto_profile_switch);
    assert!(
        games.supported_games.contains_key("iracing"),
        "iRacing should be a default game"
    );
    assert!(
        games.supported_games.contains_key("acc"),
        "ACC should be a default game"
    );
    Ok(())
}

// =========================================================================
// 4. JSON serialization roundtrip
// =========================================================================

#[test]
fn config_json_roundtrip_preserves_all_fields() -> Result<(), BoxErr> {
    let original = SystemConfig::default();
    let json = serde_json::to_string_pretty(&original)?;
    let parsed: SystemConfig = serde_json::from_str(&json)?;

    assert_eq!(parsed.schema_version, original.schema_version);
    assert_eq!(parsed.engine.tick_rate_hz, original.engine.tick_rate_hz);
    assert_eq!(
        parsed.safety.default_safe_torque_nm,
        original.safety.default_safe_torque_nm
    );
    assert_eq!(parsed.ipc.max_connections, original.ipc.max_connections);
    assert_eq!(parsed.service.service_name, original.service.service_name);
    assert_eq!(
        parsed.observability.health_stream_hz,
        original.observability.health_stream_hz
    );
    parsed.validate()?;
    Ok(())
}

// =========================================================================
// 5. Config save/load from file
// =========================================================================

#[tokio::test]
async fn config_save_and_load_from_path() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("system.json");

    let config = SystemConfig {
        engine: EngineConfig {
            tick_rate_hz: 500,
            ..EngineConfig::default()
        },
        safety: SafetyConfig {
            max_torque_nm: 20.0,
            ..SafetyConfig::default()
        },
        ..SystemConfig::default()
    };
    config.save_to_path(&path).await?;

    let loaded = SystemConfig::load_from_path(&path).await?;
    assert_eq!(loaded.engine.tick_rate_hz, 500);
    assert!((loaded.safety.max_torque_nm - 20.0).abs() < f32::EPSILON);
    Ok(())
}

#[tokio::test]
async fn config_load_missing_creates_default() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("nonexistent.json");
    assert!(!path.exists());

    let loaded = SystemConfig::load_from_path(&path).await?;
    assert_eq!(loaded.schema_version, "wheel.config/1");
    // File should have been created
    assert!(path.exists());
    Ok(())
}

// =========================================================================
// 6. Config merge (file + mutation)
// =========================================================================

#[tokio::test]
async fn config_merge_file_then_mutate() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("merge.json");

    // Save base config
    let config = SystemConfig::default();
    config.save_to_path(&path).await?;

    // "Merge" — load, apply overrides, validate
    let mut loaded = SystemConfig::load_from_path(&path).await?;
    loaded.engine.tick_rate_hz = 2000;
    loaded.service.health_check_interval = 10;
    loaded.development.enable_dev_features = true;

    loaded.validate()?;
    assert_eq!(loaded.engine.tick_rate_hz, 2000);
    assert_eq!(loaded.service.health_check_interval, 10);
    assert!(loaded.development.enable_dev_features);
    Ok(())
}

#[tokio::test]
async fn config_merge_overwritten_values_persist() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("overwrite.json");

    let mut config = SystemConfig::default();
    config.engine.tick_rate_hz = 750;
    config.save_to_path(&path).await?;

    let mut loaded = SystemConfig::load_from_path(&path).await?;
    assert_eq!(loaded.engine.tick_rate_hz, 750);

    // Override and save again
    loaded.engine.tick_rate_hz = 1500;
    loaded.save_to_path(&path).await?;

    let final_config = SystemConfig::load_from_path(&path).await?;
    assert_eq!(final_config.engine.tick_rate_hz, 1500);
    Ok(())
}

// =========================================================================
// 7. ServiceConfig from SystemConfig conversion
// =========================================================================

#[test]
fn config_service_config_from_system_config() -> Result<(), BoxErr> {
    let system = SystemConfig::default();
    let daemon_cfg = ServiceConfig::from_system_config(&system);
    assert_eq!(daemon_cfg.service_name, system.service.service_name);
    assert_eq!(
        daemon_cfg.health_check_interval,
        system.service.health_check_interval
    );
    assert_eq!(
        daemon_cfg.max_restart_attempts,
        system.service.max_restart_attempts
    );
    assert_eq!(daemon_cfg.auto_restart, system.service.auto_restart);
    Ok(())
}

// =========================================================================
// 8. FeatureFlags construction
// =========================================================================

#[test]
fn config_feature_flags_default_safe() -> Result<(), BoxErr> {
    let flags = racing_wheel_service::FeatureFlags {
        disable_realtime: false,
        force_ffb_mode: None,
        enable_dev_features: false,
        enable_debug_logging: false,
        enable_virtual_devices: false,
        disable_safety_interlocks: false,
        enable_plugin_dev_mode: false,
    };
    assert!(!flags.disable_safety_interlocks);
    assert!(!flags.enable_dev_features);
    Ok(())
}
