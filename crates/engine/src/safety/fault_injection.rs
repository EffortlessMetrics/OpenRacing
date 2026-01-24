//! Fault injection system for testing failure modes and recovery procedures

use std::collections::HashMap;
// Removed unused Arc and Mutex imports
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

use super::FaultType;
use crate::rt::Frame;

/// Fault injection scenario configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultInjectionScenario {
    pub name: String,
    pub fault_type: FaultType,
    pub trigger_condition: TriggerCondition,
    pub duration: Option<Duration>,
    pub recovery_condition: Option<RecoveryCondition>,
    pub enabled: bool,
}

/// Conditions that trigger fault injection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerCondition {
    /// Trigger after a specific time delay
    TimeDelay(Duration),
    /// Trigger after a certain number of ticks
    TickCount(u64),
    /// Trigger when torque exceeds threshold
    TorqueThreshold(f32),
    /// Trigger when temperature exceeds threshold
    TemperatureThreshold(f32),
    /// Trigger when plugin execution time exceeds threshold
    PluginTimeoutThreshold(Duration),
    /// Trigger manually via API call
    Manual,
    /// Trigger randomly with given probability (0.0 to 1.0)
    RandomProbability(f64),
    /// Trigger on specific USB operation count
    UsbOperationCount(u32),
}

/// Conditions for fault recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryCondition {
    /// Recover after time duration
    TimeDelay(Duration),
    /// Recover after tick count
    TickCount(u64),
    /// Recover when torque drops below threshold
    TorqueBelowThreshold(f32),
    /// Recover when temperature drops below threshold
    TemperatureBelowThreshold(f32),
    /// Recover manually via API call
    Manual,
}

/// Fault injection state for a scenario
#[derive(Debug, Clone)]
struct InjectionState {
    scenario: FaultInjectionScenario,
    triggered: bool,
    trigger_time: Option<Instant>,
    tick_count: u64,
    usb_operation_count: u32,
    recovered: bool,
    recovery_time: Option<Instant>,
}

impl InjectionState {
    fn new(scenario: FaultInjectionScenario) -> Self {
        Self {
            scenario,
            triggered: false,
            trigger_time: None,
            tick_count: 0,
            usb_operation_count: 0,
            recovered: false,
            recovery_time: None,
        }
    }

    /// Check if trigger condition is met
    fn check_trigger(&mut self, context: &InjectionContext) -> bool {
        if self.triggered || !self.scenario.enabled {
            return false;
        }

        let should_trigger = match &self.scenario.trigger_condition {
            TriggerCondition::TimeDelay(delay) => context.start_time.elapsed() >= *delay,
            TriggerCondition::TickCount(count) => self.tick_count >= *count,
            TriggerCondition::TorqueThreshold(threshold) => {
                context.current_torque.abs() > *threshold
            }
            TriggerCondition::TemperatureThreshold(threshold) => context.temperature > *threshold,
            TriggerCondition::PluginTimeoutThreshold(threshold) => {
                context.plugin_execution_time > *threshold
            }
            TriggerCondition::Manual => false, // Only triggered via API
            TriggerCondition::RandomProbability(prob) => rand::random::<f64>() < *prob,
            TriggerCondition::UsbOperationCount(count) => self.usb_operation_count >= *count,
        };

        if should_trigger {
            self.triggered = true;
            self.trigger_time = Some(Instant::now());
        }

        should_trigger
    }

    /// Check if recovery condition is met
    fn check_recovery(&mut self, context: &InjectionContext) -> bool {
        if !self.triggered || self.recovered {
            return false;
        }

        let should_recover = if let Some(recovery_condition) = &self.scenario.recovery_condition {
            match recovery_condition {
                RecoveryCondition::TimeDelay(delay) => {
                    self.trigger_time.map_or(false, |t| t.elapsed() >= *delay)
                }
                RecoveryCondition::TickCount(count) => self.tick_count >= *count,
                RecoveryCondition::TorqueBelowThreshold(threshold) => {
                    context.current_torque.abs() < *threshold
                }
                RecoveryCondition::TemperatureBelowThreshold(threshold) => {
                    context.temperature < *threshold
                }
                RecoveryCondition::Manual => false, // Only recovered via API
            }
        } else {
            // No recovery condition - check duration
            if let Some(duration) = self.scenario.duration {
                self.trigger_time.map_or(false, |t| t.elapsed() >= duration)
            } else {
                false // Permanent fault
            }
        };

        if should_recover {
            self.recovered = true;
            self.recovery_time = Some(Instant::now());
        }

        should_recover
    }

    /// Update tick count
    fn update_tick(&mut self) {
        self.tick_count += 1;
    }

    /// Update USB operation count
    fn update_usb_operation(&mut self) {
        self.usb_operation_count += 1;
    }
}

/// Context information for fault injection decisions
#[derive(Debug, Clone)]
pub struct InjectionContext {
    pub start_time: Instant,
    pub current_torque: f32,
    pub temperature: f32,
    pub plugin_execution_time: Duration,
    pub frame: Option<Frame>,
}

impl Default for InjectionContext {
    fn default() -> Self {
        Self {
            start_time: Instant::now(),
            current_torque: 0.0,
            temperature: 25.0,
            plugin_execution_time: Duration::ZERO,
            frame: None,
        }
    }
}

/// Fault injection system for testing failure modes
pub struct FaultInjectionSystem {
    scenarios: HashMap<String, InjectionState>,
    active_faults: HashMap<FaultType, String>, // fault_type -> scenario_name
    enabled: bool,
    start_time: Instant,
    fault_callbacks: Vec<Box<dyn Fn(FaultType, &str) + Send + Sync>>,
    recovery_callbacks: Vec<Box<dyn Fn(FaultType, &str) + Send + Sync>>,
}

impl FaultInjectionSystem {
    /// Create new fault injection system
    pub fn new() -> Self {
        Self {
            scenarios: HashMap::new(),
            active_faults: HashMap::new(),
            enabled: false, // Disabled by default for safety
            start_time: Instant::now(),
            fault_callbacks: Vec::new(),
            recovery_callbacks: Vec::new(),
        }
    }

    /// Enable or disable fault injection
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            // Clear all active faults when disabled
            self.active_faults.clear();
            for state in self.scenarios.values_mut() {
                state.triggered = false;
                state.recovered = false;
                state.trigger_time = None;
                state.recovery_time = None;
            }
        }
    }

    /// Check if fault injection is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Add fault injection scenario
    pub fn add_scenario(&mut self, scenario: FaultInjectionScenario) -> Result<(), String> {
        if self.scenarios.contains_key(&scenario.name) {
            return Err(format!("Scenario '{}' already exists", scenario.name));
        }

        let state = InjectionState::new(scenario);
        self.scenarios.insert(state.scenario.name.clone(), state);
        Ok(())
    }

    /// Remove fault injection scenario
    pub fn remove_scenario(&mut self, name: &str) -> Result<(), String> {
        if let Some(state) = self.scenarios.remove(name) {
            // Remove from active faults if it was active
            if state.triggered && !state.recovered {
                self.active_faults.remove(&state.scenario.fault_type);
            }
            Ok(())
        } else {
            Err(format!("Scenario '{}' not found", name))
        }
    }

    /// Get scenario by name
    pub fn get_scenario(&self, name: &str) -> Option<&FaultInjectionScenario> {
        self.scenarios.get(name).map(|state| &state.scenario)
    }

    /// Get all scenarios
    pub fn get_all_scenarios(&self) -> Vec<&FaultInjectionScenario> {
        self.scenarios
            .values()
            .map(|state| &state.scenario)
            .collect()
    }

    /// Manually trigger a scenario
    pub fn trigger_scenario(&mut self, name: &str) -> Result<(), String> {
        if !self.enabled {
            return Err("Fault injection is disabled".to_string());
        }

        let state = self
            .scenarios
            .get_mut(name)
            .ok_or_else(|| format!("Scenario '{}' not found", name))?;

        if state.triggered {
            return Err(format!("Scenario '{}' is already triggered", name));
        }

        // Only allow manual trigger for Manual trigger condition
        if !matches!(state.scenario.trigger_condition, TriggerCondition::Manual) {
            return Err(format!("Scenario '{}' is not manually triggerable", name));
        }

        state.triggered = true;
        state.trigger_time = Some(Instant::now());
        self.active_faults
            .insert(state.scenario.fault_type, name.to_string());

        // Notify callbacks
        for callback in &self.fault_callbacks {
            callback(state.scenario.fault_type, name);
        }

        Ok(())
    }

    /// Manually recover a scenario
    pub fn recover_scenario(&mut self, name: &str) -> Result<(), String> {
        let state = self
            .scenarios
            .get_mut(name)
            .ok_or_else(|| format!("Scenario '{}' not found", name))?;

        if !state.triggered {
            return Err(format!("Scenario '{}' is not triggered", name));
        }

        if state.recovered {
            return Err(format!("Scenario '{}' is already recovered", name));
        }

        // Check if manual recovery is allowed
        if let Some(recovery_condition) = &state.scenario.recovery_condition {
            if !matches!(recovery_condition, RecoveryCondition::Manual) {
                return Err(format!(
                    "Scenario '{}' does not support manual recovery",
                    name
                ));
            }
        }

        state.recovered = true;
        state.recovery_time = Some(Instant::now());
        self.active_faults.remove(&state.scenario.fault_type);

        // Notify callbacks
        for callback in &self.recovery_callbacks {
            callback(state.scenario.fault_type, name);
        }

        Ok(())
    }

    /// Update fault injection system with current context
    pub fn update(&mut self, context: &InjectionContext) -> Vec<FaultType> {
        if !self.enabled {
            return Vec::new();
        }

        let mut new_faults = Vec::new();
        let mut recovered_faults = Vec::new();

        // Check all scenarios
        for (name, state) in self.scenarios.iter_mut() {
            // Update tick count
            state.update_tick();

            // Check for new triggers
            if state.check_trigger(context) {
                self.active_faults
                    .insert(state.scenario.fault_type, name.clone());
                new_faults.push(state.scenario.fault_type);

                // Notify callbacks
                for callback in &self.fault_callbacks {
                    callback(state.scenario.fault_type, name);
                }
            }

            // Check for recoveries
            if state.check_recovery(context) {
                self.active_faults.remove(&state.scenario.fault_type);
                recovered_faults.push(state.scenario.fault_type);

                // Notify callbacks
                for callback in &self.recovery_callbacks {
                    callback(state.scenario.fault_type, name);
                }
            }
        }

        new_faults
    }

    /// Update USB operation count for all scenarios
    pub fn update_usb_operation(&mut self) {
        for state in self.scenarios.values_mut() {
            state.update_usb_operation();
        }
    }

    /// Check if a fault type is currently active
    pub fn is_fault_active(&self, fault_type: FaultType) -> bool {
        self.active_faults.contains_key(&fault_type)
    }

    /// Get active faults
    pub fn get_active_faults(&self) -> Vec<(FaultType, String)> {
        self.active_faults
            .iter()
            .map(|(fault_type, scenario_name)| (*fault_type, scenario_name.clone()))
            .collect()
    }

    /// Get scenario statistics
    pub fn get_scenario_stats(&self, name: &str) -> Option<ScenarioStats> {
        self.scenarios.get(name).map(|state| ScenarioStats {
            name: name.to_string(),
            triggered: state.triggered,
            trigger_time: state.trigger_time,
            recovered: state.recovered,
            recovery_time: state.recovery_time,
            tick_count: state.tick_count,
            usb_operation_count: state.usb_operation_count,
        })
    }

    /// Get all scenario statistics
    pub fn get_all_scenario_stats(&self) -> Vec<ScenarioStats> {
        self.scenarios
            .iter()
            .map(|(name, state)| ScenarioStats {
                name: name.clone(),
                triggered: state.triggered,
                trigger_time: state.trigger_time,
                recovered: state.recovered,
                recovery_time: state.recovery_time,
                tick_count: state.tick_count,
                usb_operation_count: state.usb_operation_count,
            })
            .collect()
    }

    /// Add fault callback
    pub fn add_fault_callback<F>(&mut self, callback: F)
    where
        F: Fn(FaultType, &str) + Send + Sync + 'static,
    {
        self.fault_callbacks.push(Box::new(callback));
    }

    /// Add recovery callback
    pub fn add_recovery_callback<F>(&mut self, callback: F)
    where
        F: Fn(FaultType, &str) + Send + Sync + 'static,
    {
        self.recovery_callbacks.push(Box::new(callback));
    }

    /// Reset all scenarios
    pub fn reset_all_scenarios(&mut self) {
        self.active_faults.clear();
        for state in self.scenarios.values_mut() {
            state.triggered = false;
            state.recovered = false;
            state.trigger_time = None;
            state.recovery_time = None;
            state.tick_count = 0;
            state.usb_operation_count = 0;
        }
        self.start_time = Instant::now();
    }

    /// Reset specific scenario
    pub fn reset_scenario(&mut self, name: &str) -> Result<(), String> {
        let state = self
            .scenarios
            .get_mut(name)
            .ok_or_else(|| format!("Scenario '{}' not found", name))?;

        if state.triggered && !state.recovered {
            self.active_faults.remove(&state.scenario.fault_type);
        }

        state.triggered = false;
        state.recovered = false;
        state.trigger_time = None;
        state.recovery_time = None;
        state.tick_count = 0;
        state.usb_operation_count = 0;

        Ok(())
    }

    /// Create predefined test scenarios
    pub fn create_test_scenarios(&mut self) -> Result<(), String> {
        // USB stall scenario
        self.add_scenario(FaultInjectionScenario {
            name: "usb_stall_after_5s".to_string(),
            fault_type: FaultType::UsbStall,
            trigger_condition: TriggerCondition::TimeDelay(Duration::from_secs(5)),
            duration: Some(Duration::from_secs(2)),
            recovery_condition: None,
            enabled: false,
        })?;

        // Encoder NaN scenario
        self.add_scenario(FaultInjectionScenario {
            name: "encoder_nan_random".to_string(),
            fault_type: FaultType::EncoderNaN,
            trigger_condition: TriggerCondition::RandomProbability(0.001), // 0.1% chance per tick
            duration: Some(Duration::from_millis(100)),
            recovery_condition: None,
            enabled: false,
        })?;

        // Thermal limit scenario
        self.add_scenario(FaultInjectionScenario {
            name: "thermal_limit_high_torque".to_string(),
            fault_type: FaultType::ThermalLimit,
            trigger_condition: TriggerCondition::TorqueThreshold(20.0),
            duration: None,
            recovery_condition: Some(RecoveryCondition::TorqueBelowThreshold(10.0)),
            enabled: false,
        })?;

        // Plugin overrun scenario
        self.add_scenario(FaultInjectionScenario {
            name: "plugin_overrun_after_1000_ticks".to_string(),
            fault_type: FaultType::PluginOverrun,
            trigger_condition: TriggerCondition::TickCount(1000),
            duration: Some(Duration::from_secs(1)),
            recovery_condition: None,
            enabled: false,
        })?;

        // Manual fault scenario for testing
        self.add_scenario(FaultInjectionScenario {
            name: "manual_test_fault".to_string(),
            fault_type: FaultType::SafetyInterlockViolation,
            trigger_condition: TriggerCondition::Manual,
            duration: None,
            recovery_condition: Some(RecoveryCondition::Manual),
            enabled: false,
        })?;

        Ok(())
    }
}

/// Statistics for a fault injection scenario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioStats {
    pub name: String,
    pub triggered: bool,
    #[serde(with = "option_instant_serde")]
    pub trigger_time: Option<Instant>,
    pub recovered: bool,
    #[serde(with = "option_instant_serde")]
    pub recovery_time: Option<Instant>,
    pub tick_count: u64,
    pub usb_operation_count: u32,
}

// Serde modules for Instant serialization
mod option_instant_serde {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(opt_instant: &Option<Instant>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match opt_instant {
            Some(_instant) => {
                let duration_since_epoch = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                Some(duration_since_epoch.as_secs()).serialize(serializer)
            }
            None => None::<u64>.serialize(serializer),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Instant>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt_secs = Option::<u64>::deserialize(deserializer)?;
        Ok(opt_secs.map(|_secs| Instant::now())) // Approximate for deserialization
    }
}

impl ScenarioStats {
    /// Get fault duration if both triggered and recovered
    pub fn fault_duration(&self) -> Option<Duration> {
        match (self.trigger_time, self.recovery_time) {
            (Some(trigger), Some(recovery)) => Some(recovery.duration_since(trigger)),
            _ => None,
        }
    }

    /// Check if fault is currently active
    pub fn is_active(&self) -> bool {
        self.triggered && !self.recovered
    }
}

impl Default for FaultInjectionSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_fault_injection_system_creation() {
        let system = FaultInjectionSystem::new();
        assert!(!system.is_enabled());
        assert!(system.get_all_scenarios().is_empty());
        assert!(system.get_active_faults().is_empty());
    }

    #[test]
    fn test_add_remove_scenario() {
        let mut system = FaultInjectionSystem::new();

        let scenario = FaultInjectionScenario {
            name: "test_scenario".to_string(),
            fault_type: FaultType::UsbStall,
            trigger_condition: TriggerCondition::TimeDelay(Duration::from_secs(1)),
            duration: Some(Duration::from_secs(2)),
            recovery_condition: None,
            enabled: true,
        };

        // Add scenario
        system.add_scenario(scenario.clone()).unwrap();
        assert_eq!(system.get_all_scenarios().len(), 1);
        assert!(system.get_scenario("test_scenario").is_some());

        // Try to add duplicate
        let result = system.add_scenario(scenario);
        assert!(result.is_err());

        // Remove scenario
        system.remove_scenario("test_scenario").unwrap();
        assert!(system.get_all_scenarios().is_empty());

        // Try to remove non-existent
        let result = system.remove_scenario("test_scenario");
        assert!(result.is_err());
    }

    #[test]
    fn test_manual_trigger_recovery() {
        let mut system = FaultInjectionSystem::new();
        system.set_enabled(true);

        let scenario = FaultInjectionScenario {
            name: "manual_test".to_string(),
            fault_type: FaultType::SafetyInterlockViolation,
            trigger_condition: TriggerCondition::Manual,
            duration: None,
            recovery_condition: Some(RecoveryCondition::Manual),
            enabled: true,
        };

        system.add_scenario(scenario).unwrap();

        // Trigger manually
        system.trigger_scenario("manual_test").unwrap();
        assert!(system.is_fault_active(FaultType::SafetyInterlockViolation));

        let active_faults = system.get_active_faults();
        assert_eq!(active_faults.len(), 1);
        assert_eq!(active_faults[0].0, FaultType::SafetyInterlockViolation);

        // Recover manually
        system.recover_scenario("manual_test").unwrap();
        assert!(!system.is_fault_active(FaultType::SafetyInterlockViolation));
        assert!(system.get_active_faults().is_empty());
    }

    #[test]
    fn test_time_delay_trigger() {
        let mut system = FaultInjectionSystem::new();
        system.set_enabled(true);

        let scenario = FaultInjectionScenario {
            name: "time_delay_test".to_string(),
            fault_type: FaultType::UsbStall,
            trigger_condition: TriggerCondition::TimeDelay(Duration::from_millis(100)),
            duration: Some(Duration::from_millis(50)),
            recovery_condition: None,
            enabled: true,
        };

        system.add_scenario(scenario).unwrap();

        let context = InjectionContext::default();

        // Should not trigger immediately
        let faults = system.update(&context);
        assert!(faults.is_empty());
        assert!(!system.is_fault_active(FaultType::UsbStall));

        // Wait for trigger
        std::thread::sleep(Duration::from_millis(110));
        let faults = system.update(&context);
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0], FaultType::UsbStall);
        assert!(system.is_fault_active(FaultType::UsbStall));

        // Wait for recovery
        std::thread::sleep(Duration::from_millis(60));
        let faults = system.update(&context);
        assert!(faults.is_empty());
        assert!(!system.is_fault_active(FaultType::UsbStall));
    }

    #[test]
    fn test_torque_threshold_trigger() {
        let mut system = FaultInjectionSystem::new();
        system.set_enabled(true);

        let scenario = FaultInjectionScenario {
            name: "torque_test".to_string(),
            fault_type: FaultType::ThermalLimit,
            trigger_condition: TriggerCondition::TorqueThreshold(10.0),
            duration: None,
            recovery_condition: Some(RecoveryCondition::TorqueBelowThreshold(5.0)),
            enabled: true,
        };

        system.add_scenario(scenario).unwrap();

        // Low torque - should not trigger
        let mut context = InjectionContext::default();
        context.current_torque = 5.0;
        let faults = system.update(&context);
        assert!(faults.is_empty());

        // High torque - should trigger
        context.current_torque = 15.0;
        let faults = system.update(&context);
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0], FaultType::ThermalLimit);

        // Reduce torque - should recover
        context.current_torque = 3.0;
        let faults = system.update(&context);
        assert!(faults.is_empty());
        assert!(!system.is_fault_active(FaultType::ThermalLimit));
    }

    #[test]
    fn test_tick_count_trigger() {
        let mut system = FaultInjectionSystem::new();
        system.set_enabled(true);

        let scenario = FaultInjectionScenario {
            name: "tick_test".to_string(),
            fault_type: FaultType::PluginOverrun,
            trigger_condition: TriggerCondition::TickCount(5),
            duration: Some(Duration::from_millis(100)),
            recovery_condition: None,
            enabled: true,
        };

        system.add_scenario(scenario).unwrap();

        let context = InjectionContext::default();

        // Update multiple times
        for i in 0..10 {
            let faults = system.update(&context);
            if i == 4 {
                // 5th tick (0-indexed)
                assert_eq!(faults.len(), 1);
                assert_eq!(faults[0], FaultType::PluginOverrun);
            } else if i < 4 {
                assert!(faults.is_empty());
            }
        }
    }

    #[test]
    fn test_random_probability_trigger() {
        let mut system = FaultInjectionSystem::new();
        system.set_enabled(true);

        let scenario = FaultInjectionScenario {
            name: "random_test".to_string(),
            fault_type: FaultType::EncoderNaN,
            trigger_condition: TriggerCondition::RandomProbability(1.0), // 100% chance
            duration: Some(Duration::from_millis(10)),
            recovery_condition: None,
            enabled: true,
        };

        system.add_scenario(scenario).unwrap();

        let context = InjectionContext::default();

        // Should trigger on first update with 100% probability
        let faults = system.update(&context);
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0], FaultType::EncoderNaN);
    }

    #[test]
    fn test_usb_operation_count_trigger() {
        let mut system = FaultInjectionSystem::new();
        system.set_enabled(true);

        let scenario = FaultInjectionScenario {
            name: "usb_count_test".to_string(),
            fault_type: FaultType::UsbStall,
            trigger_condition: TriggerCondition::UsbOperationCount(3),
            duration: Some(Duration::from_millis(50)),
            recovery_condition: None,
            enabled: true,
        };

        system.add_scenario(scenario).unwrap();

        let context = InjectionContext::default();

        // Update USB operations
        for i in 0..5 {
            system.update_usb_operation();
            let faults = system.update(&context);

            if i == 2 {
                // 3rd operation (0-indexed)
                assert_eq!(faults.len(), 1);
                assert_eq!(faults[0], FaultType::UsbStall);
            } else if i < 2 {
                assert!(faults.is_empty());
            }
        }
    }

    #[test]
    fn test_scenario_statistics() {
        let mut system = FaultInjectionSystem::new();
        system.set_enabled(true);

        let scenario = FaultInjectionScenario {
            name: "stats_test".to_string(),
            fault_type: FaultType::UsbStall,
            trigger_condition: TriggerCondition::Manual,
            duration: None,
            recovery_condition: Some(RecoveryCondition::Manual),
            enabled: true,
        };

        system.add_scenario(scenario).unwrap();

        // Initial stats
        let stats = system.get_scenario_stats("stats_test").unwrap();
        assert!(!stats.triggered);
        assert!(!stats.recovered);
        assert!(stats.trigger_time.is_none());
        assert!(stats.recovery_time.is_none());

        // Trigger and check stats
        system.trigger_scenario("stats_test").unwrap();
        let stats = system.get_scenario_stats("stats_test").unwrap();
        assert!(stats.triggered);
        assert!(!stats.recovered);
        assert!(stats.trigger_time.is_some());
        assert!(stats.is_active());

        // Recover and check stats
        system.recover_scenario("stats_test").unwrap();
        let stats = system.get_scenario_stats("stats_test").unwrap();
        assert!(stats.triggered);
        assert!(stats.recovered);
        assert!(stats.recovery_time.is_some());
        assert!(!stats.is_active());
        assert!(stats.fault_duration().is_some());
    }

    #[test]
    fn test_enable_disable() {
        let mut system = FaultInjectionSystem::new();

        let scenario = FaultInjectionScenario {
            name: "enable_test".to_string(),
            fault_type: FaultType::UsbStall,
            trigger_condition: TriggerCondition::Manual,
            duration: None,
            recovery_condition: Some(RecoveryCondition::Manual),
            enabled: true,
        };

        system.add_scenario(scenario).unwrap();

        // Should not trigger when disabled
        let result = system.trigger_scenario("enable_test");
        assert!(result.is_err());

        // Enable and trigger
        system.set_enabled(true);
        system.trigger_scenario("enable_test").unwrap();
        assert!(system.is_fault_active(FaultType::UsbStall));

        // Disable should clear active faults
        system.set_enabled(false);
        assert!(!system.is_fault_active(FaultType::UsbStall));
    }

    #[test]
    fn test_reset_scenarios() {
        let mut system = FaultInjectionSystem::new();
        system.set_enabled(true);

        let scenario = FaultInjectionScenario {
            name: "reset_test".to_string(),
            fault_type: FaultType::UsbStall,
            trigger_condition: TriggerCondition::Manual,
            duration: None,
            recovery_condition: Some(RecoveryCondition::Manual),
            enabled: true,
        };

        system.add_scenario(scenario).unwrap();

        // Trigger scenario
        system.trigger_scenario("reset_test").unwrap();
        assert!(system.is_fault_active(FaultType::UsbStall));

        // Reset specific scenario
        system.reset_scenario("reset_test").unwrap();
        assert!(!system.is_fault_active(FaultType::UsbStall));

        let stats = system.get_scenario_stats("reset_test").unwrap();
        assert!(!stats.triggered);
        assert!(!stats.recovered);
    }

    #[test]
    fn test_create_test_scenarios() {
        let mut system = FaultInjectionSystem::new();
        system.create_test_scenarios().unwrap();

        let scenarios = system.get_all_scenarios();
        assert!(scenarios.len() >= 5);

        // Check that specific scenarios exist
        assert!(system.get_scenario("usb_stall_after_5s").is_some());
        assert!(system.get_scenario("encoder_nan_random").is_some());
        assert!(system.get_scenario("thermal_limit_high_torque").is_some());
        assert!(
            system
                .get_scenario("plugin_overrun_after_1000_ticks")
                .is_some()
        );
        assert!(system.get_scenario("manual_test_fault").is_some());
    }

    #[test]
    fn test_callbacks() {
        let mut system = FaultInjectionSystem::new();
        system.set_enabled(true);

        let fault_triggered = Arc::new(Mutex::new(false));
        let recovery_triggered = Arc::new(Mutex::new(false));

        let fault_flag = fault_triggered.clone();
        let recovery_flag = recovery_triggered.clone();

        system.add_fault_callback(move |_fault_type, _scenario| {
            *fault_flag.lock().unwrap() = true;
        });

        system.add_recovery_callback(move |_fault_type, _scenario| {
            *recovery_flag.lock().unwrap() = true;
        });

        let scenario = FaultInjectionScenario {
            name: "callback_test".to_string(),
            fault_type: FaultType::UsbStall,
            trigger_condition: TriggerCondition::Manual,
            duration: None,
            recovery_condition: Some(RecoveryCondition::Manual),
            enabled: true,
        };

        system.add_scenario(scenario).unwrap();

        // Trigger should call fault callback
        system.trigger_scenario("callback_test").unwrap();
        assert!(*fault_triggered.lock().unwrap());

        // Recovery should call recovery callback
        system.recover_scenario("callback_test").unwrap();
        assert!(*recovery_triggered.lock().unwrap());
    }
}
