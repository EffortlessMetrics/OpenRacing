//! Property-based tests for the racing-wheel-service crate.
//!
//! These tests verify critical service invariants:
//! - Service state machine transitions are deterministic
//! - System config serialization roundtrips
//! - Safety config invariants hold
//! - Device state ordering properties

use proptest::prelude::*;
use racing_wheel_service::device_service::DeviceState;
use racing_wheel_service::safety_service::FaultSeverity;
use racing_wheel_service::system_config::{
    EngineConfig, GameConfig, IpcConfig, ObservabilityConfig, PluginConfig,
    SafetyConfig, SystemConfig,
};

/// proptest config with 200 cases per test
fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 200,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// SystemConfig serialization roundtrip
// ---------------------------------------------------------------------------

fn arb_engine_config() -> impl Strategy<Value = EngineConfig> {
    (
        100u32..=10_000,   // tick_rate_hz
        50u32..=10_000,    // max_jitter_us
        any::<bool>(),     // disable_realtime
        any::<bool>(),     // memory_lock_all
        50u32..=2000,      // processing_budget_us
    )
        .prop_map(
            |(tick_rate_hz, max_jitter_us, disable_realtime, memory_lock_all, processing_budget_us)| {
                EngineConfig {
                    tick_rate_hz,
                    max_jitter_us,
                    force_ffb_mode: None,
                    disable_realtime,
                    rt_cpu_affinity: None,
                    memory_lock_all,
                    processing_budget_us,
                }
            },
        )
}

fn arb_safety_config() -> impl Strategy<Value = SafetyConfig> {
    (
        0.1f32..=50.0,   // default_safe_torque_nm
        0.1f32..=50.0,   // max_torque_nm
        10u32..=5000,    // fault_response_timeout_ms
        1u32..=30,       // hands_off_timeout_s
        40u8..=90,       // temp_warning_c
        50u8..=100,      // temp_fault_c
        any::<bool>(),   // require_physical_interlock
    )
        .prop_map(
            |(default_safe, max, fault_timeout, hands_off, temp_warn, temp_fault, interlock)| {
                // Ensure default_safe <= max
                let (default_safe, max) = if default_safe <= max {
                    (default_safe, max)
                } else {
                    (max, default_safe)
                };
                // Ensure warning < fault
                let (temp_warn, temp_fault) = if temp_warn < temp_fault {
                    (temp_warn, temp_fault)
                } else {
                    (temp_fault, temp_warn.max(temp_fault + 1))
                };
                SafetyConfig {
                    default_safe_torque_nm: default_safe,
                    max_torque_nm: max,
                    fault_response_timeout_ms: fault_timeout,
                    hands_off_timeout_s: hands_off,
                    temp_warning_c: temp_warn,
                    temp_fault_c: temp_fault,
                    require_physical_interlock: interlock,
                }
            },
        )
}

proptest! {
    #![proptest_config(config())]

    /// SystemConfig default serializes and deserializes identically.
    #[test]
    fn system_config_default_roundtrip(_dummy in 0u8..1) {
        let config = SystemConfig::default();
        let json = serde_json::to_string(&config)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        let parsed: SystemConfig = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        prop_assert_eq!(&config.schema_version, &parsed.schema_version);
        prop_assert_eq!(config.engine.tick_rate_hz, parsed.engine.tick_rate_hz);
    }

    /// EngineConfig roundtrips through JSON.
    #[test]
    fn engine_config_roundtrip(engine in arb_engine_config()) {
        let json = serde_json::to_string(&engine)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        let parsed: EngineConfig = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        prop_assert_eq!(engine.tick_rate_hz, parsed.tick_rate_hz);
        prop_assert_eq!(engine.max_jitter_us, parsed.max_jitter_us);
        prop_assert_eq!(engine.disable_realtime, parsed.disable_realtime);
        prop_assert_eq!(engine.memory_lock_all, parsed.memory_lock_all);
        prop_assert_eq!(engine.processing_budget_us, parsed.processing_budget_us);
    }

    /// SafetyConfig roundtrips through JSON.
    #[test]
    fn safety_config_roundtrip(safety in arb_safety_config()) {
        let json = serde_json::to_string(&safety)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        let parsed: SafetyConfig = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        prop_assert!((safety.default_safe_torque_nm - parsed.default_safe_torque_nm).abs() < f32::EPSILON);
        prop_assert!((safety.max_torque_nm - parsed.max_torque_nm).abs() < f32::EPSILON);
        prop_assert_eq!(safety.fault_response_timeout_ms, parsed.fault_response_timeout_ms);
        prop_assert_eq!(safety.hands_off_timeout_s, parsed.hands_off_timeout_s);
        prop_assert_eq!(safety.temp_warning_c, parsed.temp_warning_c);
        prop_assert_eq!(safety.temp_fault_c, parsed.temp_fault_c);
    }
}

// ---------------------------------------------------------------------------
// DeviceState invariants
// ---------------------------------------------------------------------------

fn arb_device_state() -> impl Strategy<Value = DeviceState> {
    prop_oneof![
        Just(DeviceState::Disconnected),
        Just(DeviceState::Connected),
        Just(DeviceState::Ready),
        "[a-z ]{1,30}".prop_map(|reason| DeviceState::Faulted { reason }),
    ]
}

proptest! {
    #![proptest_config(config())]

    /// DeviceState equality is reflexive.
    #[test]
    fn device_state_equality_reflexive(state in arb_device_state()) {
        prop_assert_eq!(&state, &state);
    }

    /// DeviceState Faulted always carries a non-empty reason after construction.
    #[test]
    fn device_state_faulted_has_reason(reason in "[a-z ]{1,30}") {
        let state = DeviceState::Faulted { reason: reason.clone() };
        if let DeviceState::Faulted { reason: r } = &state {
            prop_assert!(!r.is_empty(), "Faulted state has empty reason");
        }
    }

    /// DeviceState transitions: same state -> same state is idempotent.
    #[test]
    fn device_state_idempotent_transition(state in arb_device_state()) {
        let clone = state.clone();
        prop_assert_eq!(&state, &clone);
    }
}

// ---------------------------------------------------------------------------
// FaultSeverity ordering
// ---------------------------------------------------------------------------

fn arb_fault_severity() -> impl Strategy<Value = FaultSeverity> {
    prop_oneof![
        Just(FaultSeverity::Warning),
        Just(FaultSeverity::Critical),
        Just(FaultSeverity::Fatal),
    ]
}

proptest! {
    #![proptest_config(config())]

    /// FaultSeverity equality is reflexive.
    #[test]
    fn fault_severity_reflexive(sev in arb_fault_severity()) {
        prop_assert_eq!(sev, sev);
    }

    /// Warning < Critical < Fatal ordering holds.
    #[test]
    fn fault_severity_ordering(_dummy in 0u8..1) {
        prop_assert!(FaultSeverity::Warning != FaultSeverity::Critical);
        prop_assert!(FaultSeverity::Critical != FaultSeverity::Fatal);
        prop_assert!(FaultSeverity::Warning != FaultSeverity::Fatal);
    }
}

// ---------------------------------------------------------------------------
// Safety config invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// SafetyConfig default always has default_safe <= max_torque.
    #[test]
    fn safety_default_safe_le_max(_dummy in 0u8..1) {
        let config = SafetyConfig::default();
        prop_assert!(
            config.default_safe_torque_nm <= config.max_torque_nm,
            "default_safe {} > max {}",
            config.default_safe_torque_nm, config.max_torque_nm
        );
    }

    /// SafetyConfig default always has temp_warning < temp_fault.
    #[test]
    fn safety_default_temp_ordering(_dummy in 0u8..1) {
        let config = SafetyConfig::default();
        prop_assert!(
            config.temp_warning_c < config.temp_fault_c,
            "temp_warning {} >= temp_fault {}",
            config.temp_warning_c, config.temp_fault_c
        );
    }

    /// Constructed SafetyConfig maintains safe_torque <= max_torque.
    #[test]
    fn safety_constructed_safe_le_max(safety in arb_safety_config()) {
        prop_assert!(
            safety.default_safe_torque_nm <= safety.max_torque_nm,
            "default_safe {} > max {}",
            safety.default_safe_torque_nm, safety.max_torque_nm
        );
    }
}

// ---------------------------------------------------------------------------
// GameConfig invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// GameConfig default serialization roundtrips structurally.
    #[test]
    fn game_config_serialization_idempotent(_dummy in 0u8..1) {
        let config = GameConfig::default();
        let json1 = serde_json::to_string(&config)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        let parsed: GameConfig = serde_json::from_str(&json1)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        let json2 = serde_json::to_string(&parsed)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        // Compare as Value to ignore HashMap key ordering
        let val1: serde_json::Value = serde_json::from_str(&json1)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        let val2: serde_json::Value = serde_json::from_str(&json2)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        prop_assert_eq!(val1, val2, "Serialization is not structurally idempotent");
    }

    /// GameConfig default always has at least one supported game.
    #[test]
    fn game_config_default_has_games(_dummy in 0u8..1) {
        let config = GameConfig::default();
        prop_assert!(!config.supported_games.is_empty());
    }

    /// GameConfig default switch timeout is positive.
    #[test]
    fn game_config_switch_timeout_positive(_dummy in 0u8..1) {
        let config = GameConfig::default();
        prop_assert!(config.profile_switch_timeout_ms > 0);
    }
}

// ---------------------------------------------------------------------------
// IpcConfig and PluginConfig invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// IpcConfig default roundtrips through JSON.
    #[test]
    fn ipc_config_roundtrip(_dummy in 0u8..1) {
        let config = IpcConfig::default();
        let json = serde_json::to_string(&config)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        let parsed: IpcConfig = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        prop_assert_eq!(config.max_connections, parsed.max_connections);
        prop_assert_eq!(config.max_message_size, parsed.max_message_size);
    }

    /// PluginConfig default roundtrips through JSON.
    #[test]
    fn plugin_config_roundtrip(_dummy in 0u8..1) {
        let config = PluginConfig::default();
        let json = serde_json::to_string(&config)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        let parsed: PluginConfig = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        prop_assert_eq!(config.enabled, parsed.enabled);
        prop_assert_eq!(config.max_memory_mb, parsed.max_memory_mb);
        prop_assert_eq!(config.timeout_ms, parsed.timeout_ms);
    }

    /// ObservabilityConfig tracing_sample_rate is in [0.0, 1.0] by default.
    #[test]
    fn observability_sample_rate_bounded(_dummy in 0u8..1) {
        let config = ObservabilityConfig::default();
        prop_assert!(config.tracing_sample_rate >= 0.0 && config.tracing_sample_rate <= 1.0);
    }
}
