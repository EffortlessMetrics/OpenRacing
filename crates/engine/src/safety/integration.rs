//! Integration module for FMEA system components

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::{
    FaultType, SafetyService, SafetyState,
    fault_injection::{FaultInjectionSystem, InjectionContext},
    fmea::{FaultMarker, FmeaSystem},
    watchdog::{HealthStatus, SystemComponent, WatchdogConfig, WatchdogSystem},
};

/// Integrated fault management system combining FMEA, watchdog, and fault injection
pub struct IntegratedFaultManager {
    fmea_system: FmeaSystem,
    watchdog_system: WatchdogSystem,
    fault_injection: FaultInjectionSystem,
    safety_service: SafetyService,
    blackbox_markers: Vec<FaultMarker>,
    recovery_procedures: HashMap<FaultType, RecoveryProcedure>,
    fault_history: Vec<FaultEvent>,
    system_start_time: Instant,
}

/// Recovery procedure for a specific fault type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryProcedure {
    pub fault_type: FaultType,
    pub steps: Vec<RecoveryStep>,
    pub max_attempts: u32,
    pub backoff_strategy: BackoffStrategy,
    pub success_criteria: Vec<String>,
}

/// Individual recovery step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStep {
    pub description: String,
    pub action: RecoveryAction,
    pub timeout: Duration,
    pub required: bool,
}

/// Recovery action types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryAction {
    /// Wait for a specified duration
    Wait(Duration),
    /// Reset a system component
    ResetComponent(SystemComponent),
    /// Restart communication with device
    RestartCommunication,
    /// Recalibrate device
    RecalibrateDevice,
    /// Reduce torque to safe level
    ReduceTorque(f32),
    /// Clear fault condition
    ClearFault,
    /// Quarantine plugin
    QuarantinePlugin(String),
    /// Restart plugin
    RestartPlugin(String),
    /// Custom action with parameters
    Custom(String, HashMap<String, String>),
}

/// Backoff strategy for recovery attempts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackoffStrategy {
    /// Fixed delay between attempts
    Fixed(Duration),
    /// Exponential backoff with base delay
    Exponential { base: Duration, max: Duration },
    /// Linear increase in delay
    Linear { increment: Duration, max: Duration },
}

/// Fault event for history tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultEvent {
    pub fault_type: FaultType,
    #[serde(with = "instant_serde")]
    pub timestamp: Instant,
    pub context: String,
    pub recovery_attempted: bool,
    pub recovery_successful: bool,
    #[serde(with = "option_duration_serde")]
    pub recovery_duration: Option<Duration>,
    pub blackbox_marker_id: Option<u64>,
}

/// System health summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthSummary {
    pub overall_status: HealthStatus,
    pub component_health: HashMap<SystemComponent, HealthStatus>,
    pub active_faults: Vec<FaultType>,
    pub quarantined_plugins: Vec<String>,
    pub fault_count_24h: u32,
    pub uptime: Duration,
    pub last_fault: Option<FaultEvent>,
}

impl IntegratedFaultManager {
    /// Create new integrated fault manager
    pub fn new(
        max_safe_torque_nm: f32,
        max_high_torque_nm: f32,
        watchdog_config: WatchdogConfig,
    ) -> Self {
        let mut manager = Self {
            fmea_system: FmeaSystem::new(),
            watchdog_system: WatchdogSystem::new(watchdog_config),
            fault_injection: FaultInjectionSystem::new(),
            safety_service: SafetyService::new(max_safe_torque_nm, max_high_torque_nm),
            blackbox_markers: Vec::new(),
            recovery_procedures: HashMap::new(),
            fault_history: Vec::new(),
            system_start_time: Instant::now(),
        };

        manager.initialize_recovery_procedures();
        manager.setup_callbacks();
        manager
    }

    /// Initialize default recovery procedures
    fn initialize_recovery_procedures(&mut self) {
        // USB Stall recovery
        self.recovery_procedures.insert(
            FaultType::UsbStall,
            RecoveryProcedure {
                fault_type: FaultType::UsbStall,
                steps: vec![
                    RecoveryStep {
                        description: "Wait for USB timeout".to_string(),
                        action: RecoveryAction::Wait(Duration::from_millis(100)),
                        timeout: Duration::from_millis(200),
                        required: true,
                    },
                    RecoveryStep {
                        description: "Restart USB communication".to_string(),
                        action: RecoveryAction::RestartCommunication,
                        timeout: Duration::from_secs(2),
                        required: true,
                    },
                    RecoveryStep {
                        description: "Clear fault condition".to_string(),
                        action: RecoveryAction::ClearFault,
                        timeout: Duration::from_millis(100),
                        required: true,
                    },
                ],
                max_attempts: 3,
                backoff_strategy: BackoffStrategy::Exponential {
                    base: Duration::from_millis(500),
                    max: Duration::from_secs(5),
                },
                success_criteria: vec!["USB communication restored".to_string()],
            },
        );

        // Encoder NaN recovery
        self.recovery_procedures.insert(
            FaultType::EncoderNaN,
            RecoveryProcedure {
                fault_type: FaultType::EncoderNaN,
                steps: vec![
                    RecoveryStep {
                        description: "Recalibrate encoder".to_string(),
                        action: RecoveryAction::RecalibrateDevice,
                        timeout: Duration::from_secs(5),
                        required: true,
                    },
                    RecoveryStep {
                        description: "Clear fault condition".to_string(),
                        action: RecoveryAction::ClearFault,
                        timeout: Duration::from_millis(100),
                        required: true,
                    },
                ],
                max_attempts: 2,
                backoff_strategy: BackoffStrategy::Fixed(Duration::from_secs(1)),
                success_criteria: vec!["Encoder values within normal range".to_string()],
            },
        );

        // Thermal limit recovery
        self.recovery_procedures.insert(
            FaultType::ThermalLimit,
            RecoveryProcedure {
                fault_type: FaultType::ThermalLimit,
                steps: vec![
                    RecoveryStep {
                        description: "Reduce torque to safe level".to_string(),
                        action: RecoveryAction::ReduceTorque(5.0),
                        timeout: Duration::from_millis(50),
                        required: true,
                    },
                    RecoveryStep {
                        description: "Wait for cooldown".to_string(),
                        action: RecoveryAction::Wait(Duration::from_secs(30)),
                        timeout: Duration::from_secs(35),
                        required: true,
                    },
                    RecoveryStep {
                        description: "Clear fault condition".to_string(),
                        action: RecoveryAction::ClearFault,
                        timeout: Duration::from_millis(100),
                        required: true,
                    },
                ],
                max_attempts: 1,
                backoff_strategy: BackoffStrategy::Fixed(Duration::from_secs(60)),
                success_criteria: vec!["Temperature below threshold".to_string()],
            },
        );

        // Plugin overrun recovery
        self.recovery_procedures.insert(
            FaultType::PluginOverrun,
            RecoveryProcedure {
                fault_type: FaultType::PluginOverrun,
                steps: vec![RecoveryStep {
                    description: "Quarantine plugin".to_string(),
                    action: RecoveryAction::QuarantinePlugin("".to_string()), // Plugin ID filled at runtime
                    timeout: Duration::from_millis(10),
                    required: true,
                }],
                max_attempts: 1,
                backoff_strategy: BackoffStrategy::Fixed(Duration::from_secs(300)),
                success_criteria: vec!["Plugin quarantined successfully".to_string()],
            },
        );
    }

    /// Setup callbacks between systems
    fn setup_callbacks(&mut self) {
        // Setup watchdog fault callbacks
        // Note: In a real implementation, these would use proper callback mechanisms
        // For now, we'll handle this in the update loop
    }

    /// Update all fault management systems
    pub fn update(&mut self, context: &FaultManagerContext) -> FaultManagerResult {
        let mut result = FaultManagerResult::default();

        // Update watchdog system
        let watchdog_faults = self.watchdog_system.perform_health_checks();
        for fault in watchdog_faults {
            result.new_faults.push(fault);
        }

        // Update plugin execution tracking
        if let Some(plugin_execution) = &context.plugin_execution
            && let Some(fault) = self.watchdog_system.record_plugin_execution(
                &plugin_execution.plugin_id,
                plugin_execution.execution_time_us,
            )
        {
            result.new_faults.push(fault);
        }

        // Update component heartbeats
        for (component, heartbeat) in &context.component_heartbeats {
            if *heartbeat {
                self.watchdog_system.heartbeat(*component);
            }
        }

        // Detect faults using FMEA system
        if let Some(usb_info) = &context.usb_info
            && let Some(fault) = self
                .fmea_system
                .detect_usb_fault(usb_info.consecutive_failures, usb_info.last_success)
        {
            result.new_faults.push(fault);
        }

        if let Some(encoder_value) = context.encoder_value
            && let Some(fault) = self.fmea_system.detect_encoder_fault(encoder_value)
        {
            result.new_faults.push(fault);
        }

        if let Some(temperature) = context.temperature {
            let current_thermal_fault = matches!(
                self.safety_service.state(),
                SafetyState::Faulted {
                    fault: FaultType::ThermalLimit,
                    ..
                }
            );
            if let Some(fault) = self
                .fmea_system
                .detect_thermal_fault(temperature, current_thermal_fault)
            {
                result.new_faults.push(fault);
            }
        }

        if let Some(jitter_us) = context.timing_jitter_us
            && let Some(fault) = self.fmea_system.detect_timing_violation(jitter_us)
        {
            result.new_faults.push(fault);
        }

        // Update fault injection system
        let injection_context = InjectionContext {
            start_time: self.system_start_time,
            current_torque: context.current_torque,
            temperature: context.temperature.unwrap_or(25.0),
            plugin_execution_time: context
                .plugin_execution
                .as_ref()
                .map(|pe| Duration::from_micros(pe.execution_time_us))
                .unwrap_or_default(),
            frame: context.frame,
        };

        let injected_faults = self.fault_injection.update(&injection_context);
        result.new_faults.extend(injected_faults);

        // Handle all new faults
        for fault_type in &result.new_faults {
            self.handle_fault(*fault_type, context.current_torque);
        }

        // Update soft-stop controller
        result.current_torque_multiplier = self.fmea_system.update_soft_stop();
        result.soft_stop_active = self.fmea_system.is_soft_stop_active();

        // Check for recovery opportunities
        self.check_recovery_opportunities(&mut result);

        // Update fault history
        for fault_type in &result.new_faults {
            self.fault_history.push(FaultEvent {
                fault_type: *fault_type,
                timestamp: Instant::now(),
                context: format!(
                    "Torque: {:.1}Nm, Temp: {:.1}°C",
                    context.current_torque,
                    context.temperature.unwrap_or(0.0)
                ),
                recovery_attempted: false,
                recovery_successful: false,
                recovery_duration: None,
                blackbox_marker_id: None,
            });
        }

        result
    }

    /// Handle a detected fault
    fn handle_fault(&mut self, fault_type: FaultType, current_torque: f32) {
        // Handle fault through FMEA system
        if let Err(e) =
            self.fmea_system
                .handle_fault(fault_type, current_torque, &mut self.safety_service)
        {
            eprintln!("Error handling fault {:?}: {}", fault_type, e);
        }

        // Create blackbox marker
        let marker = FaultMarker {
            fault_type,
            timestamp: Instant::now(),
            pre_fault_data_offset: 0, // Would be calculated by blackbox system
            post_fault_data_length: 0, // Would be calculated by blackbox system
            device_state: HashMap::new(), // Would be populated with actual device state
            telemetry_snapshot: None, // Would be populated with telemetry data
            plugin_states: HashMap::new(), // Would be populated with plugin states
            recovery_actions: self
                .recovery_procedures
                .get(&fault_type)
                .map(|proc| {
                    proc.steps
                        .iter()
                        .map(|step| step.description.clone())
                        .collect()
                })
                .unwrap_or_default(),
        };

        self.blackbox_markers.push(marker);
    }

    /// Check for recovery opportunities
    fn check_recovery_opportunities(&mut self, result: &mut FaultManagerResult) {
        // Check if safety service can clear faults
        if matches!(self.safety_service.state(), SafetyState::Faulted { .. }) {
            // Attempt to clear fault if conditions are met
            if let Ok(()) = self.safety_service.clear_fault() {
                result
                    .recovery_actions
                    .push("Safety fault cleared".to_string());
            }
        }

        // Check for plugin quarantine releases
        let quarantined_plugins = self.watchdog_system.get_quarantined_plugins();
        for (plugin_id, remaining_time) in quarantined_plugins {
            if remaining_time.is_zero()
                && let Ok(()) = self.watchdog_system.release_plugin_quarantine(&plugin_id)
            {
                result
                    .recovery_actions
                    .push(format!("Released plugin quarantine: {}", plugin_id));
            }
        }
    }

    /// Execute recovery procedure for a fault type
    pub fn execute_recovery_procedure(&mut self, fault_type: FaultType) -> Result<(), String> {
        let procedure = self
            .recovery_procedures
            .get(&fault_type)
            .ok_or_else(|| format!("No recovery procedure for fault type: {:?}", fault_type))?
            .clone();

        let start_time = Instant::now();
        let mut recovery_successful = false;

        for attempt in 0..procedure.max_attempts {
            let mut step_results = Vec::new();
            let mut all_steps_successful = true;

            for step in &procedure.steps {
                let step_start = Instant::now();
                let step_result = self.execute_recovery_step(step, fault_type);
                let step_duration = step_start.elapsed();

                step_results.push((step.description.clone(), step_result.is_ok(), step_duration));

                if step_result.is_err() && step.required {
                    all_steps_successful = false;
                    if step.required {
                        break;
                    }
                }
            }

            if all_steps_successful {
                recovery_successful = true;
                break;
            }

            // Apply backoff strategy if not the last attempt
            if attempt < procedure.max_attempts - 1 {
                let backoff_duration =
                    self.calculate_backoff_duration(&procedure.backoff_strategy, attempt);
                std::thread::sleep(backoff_duration);
            }
        }

        // Update fault history
        if let Some(last_fault) = self
            .fault_history
            .iter_mut()
            .rev()
            .find(|f| f.fault_type == fault_type)
        {
            last_fault.recovery_attempted = true;
            last_fault.recovery_successful = recovery_successful;
            last_fault.recovery_duration = Some(start_time.elapsed());
        }

        if recovery_successful {
            Ok(())
        } else {
            Err(format!(
                "Recovery procedure failed for fault type: {:?}",
                fault_type
            ))
        }
    }

    /// Execute a single recovery step
    fn execute_recovery_step(
        &mut self,
        step: &RecoveryStep,
        _fault_type: FaultType,
    ) -> Result<(), String> {
        match &step.action {
            RecoveryAction::Wait(duration) => {
                std::thread::sleep(*duration);
                Ok(())
            }
            RecoveryAction::ResetComponent(component) => {
                // Reset component health status
                self.watchdog_system
                    .report_component_failure(*component, Some("Manual reset".to_string()));
                self.watchdog_system.heartbeat(*component);
                Ok(())
            }
            RecoveryAction::RestartCommunication => {
                // In a real implementation, this would restart USB/HID communication
                Ok(())
            }
            RecoveryAction::RecalibrateDevice => {
                // In a real implementation, this would trigger device recalibration
                Ok(())
            }
            RecoveryAction::ReduceTorque(_target_torque) => {
                // Force torque reduction through soft-stop
                self.fmea_system.force_stop_soft_stop();
                Ok(())
            }
            RecoveryAction::ClearFault => self.safety_service.clear_fault(),
            RecoveryAction::QuarantinePlugin(_plugin_id) => {
                // Plugin quarantine is handled by watchdog system automatically
                Ok(())
            }
            RecoveryAction::RestartPlugin(plugin_id) => {
                // Release from quarantine to allow restart
                self.watchdog_system.release_plugin_quarantine(plugin_id)
            }
            RecoveryAction::Custom(action_name, _parameters) => {
                // Custom actions would be implemented based on specific needs
                Err(format!("Custom action not implemented: {}", action_name))
            }
        }
    }

    /// Calculate backoff duration based on strategy
    fn calculate_backoff_duration(&self, strategy: &BackoffStrategy, attempt: u32) -> Duration {
        match strategy {
            BackoffStrategy::Fixed(duration) => *duration,
            BackoffStrategy::Exponential { base, max } => {
                let exponential_duration = *base * 2_u32.pow(attempt);
                std::cmp::min(
                    Duration::from_millis(exponential_duration.as_millis() as u64),
                    *max,
                )
            }
            BackoffStrategy::Linear { increment, max } => {
                let linear_duration = *increment * (attempt + 1);
                std::cmp::min(linear_duration, *max)
            }
        }
    }

    /// Get system health summary
    pub fn get_health_summary(&self) -> SystemHealthSummary {
        let component_health = self.watchdog_system.get_health_summary();
        let overall_status = if component_health
            .values()
            .any(|&status| status == HealthStatus::Faulted)
        {
            HealthStatus::Faulted
        } else if component_health
            .values()
            .any(|&status| status == HealthStatus::Degraded)
        {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        let active_faults = match self.safety_service.state() {
            SafetyState::Faulted { fault, .. } => vec![*fault],
            _ => Vec::new(),
        };

        let quarantined_plugins = self
            .watchdog_system
            .get_quarantined_plugins()
            .into_iter()
            .map(|(plugin_id, _)| plugin_id)
            .collect();

        let fault_count_24h = self
            .fault_history
            .iter()
            .filter(|event| event.timestamp.elapsed() < Duration::from_secs(24 * 60 * 60))
            .count() as u32;

        SystemHealthSummary {
            overall_status,
            component_health,
            active_faults,
            quarantined_plugins,
            fault_count_24h,
            uptime: self.system_start_time.elapsed(),
            last_fault: self.fault_history.last().cloned(),
        }
    }

    /// Get fault statistics
    pub fn get_fault_statistics(&self) -> HashMap<FaultType, FaultStatistics> {
        let mut stats = HashMap::new();

        for event in &self.fault_history {
            let stat = stats
                .entry(event.fault_type)
                .or_insert_with(|| FaultStatistics {
                    fault_type: event.fault_type,
                    total_count: 0,
                    recovery_success_rate: 0.0,
                    average_recovery_time: Duration::ZERO,
                    last_occurrence: None,
                });

            stat.total_count += 1;
            stat.last_occurrence = Some(event.timestamp);
        }

        // Calculate recovery statistics
        for (fault_type, stat) in stats.iter_mut() {
            let fault_events: Vec<_> = self
                .fault_history
                .iter()
                .filter(|e| e.fault_type == *fault_type)
                .collect();

            let recovery_attempts = fault_events.iter().filter(|e| e.recovery_attempted).count();
            let recovery_successes = fault_events
                .iter()
                .filter(|e| e.recovery_successful)
                .count();

            if recovery_attempts > 0 {
                stat.recovery_success_rate = recovery_successes as f64 / recovery_attempts as f64;
            }

            let total_recovery_time: Duration = fault_events
                .iter()
                .filter_map(|e| e.recovery_duration)
                .sum();

            if recovery_successes > 0 {
                stat.average_recovery_time = total_recovery_time / recovery_successes as u32;
            }
        }

        stats
    }

    /// Enable fault injection for testing
    pub fn enable_fault_injection(&mut self, enabled: bool) {
        self.fault_injection.set_enabled(enabled);
    }

    /// Get fault injection system for testing
    pub fn fault_injection_mut(&mut self) -> &mut FaultInjectionSystem {
        &mut self.fault_injection
    }

    /// Get safety service
    pub fn safety_service(&self) -> &SafetyService {
        &self.safety_service
    }

    /// Get mutable safety service
    pub fn safety_service_mut(&mut self) -> &mut SafetyService {
        &mut self.safety_service
    }

    /// Get watchdog system
    pub fn watchdog_system(&self) -> &WatchdogSystem {
        &self.watchdog_system
    }

    /// Get FMEA system
    pub fn fmea_system(&self) -> &FmeaSystem {
        &self.fmea_system
    }

    /// Get blackbox markers
    pub fn get_blackbox_markers(&self) -> &[FaultMarker] {
        &self.blackbox_markers
    }

    /// Clear old blackbox markers
    pub fn clear_old_blackbox_markers(&mut self, older_than: Duration) {
        let cutoff = Instant::now() - older_than;
        self.blackbox_markers
            .retain(|marker| marker.timestamp > cutoff);
    }
}

/// Context information for fault manager updates
#[derive(Debug, Clone, Default)]
pub struct FaultManagerContext {
    pub current_torque: f32,
    pub temperature: Option<f32>,
    pub encoder_value: Option<f32>,
    pub timing_jitter_us: Option<u64>,
    pub usb_info: Option<UsbInfo>,
    pub plugin_execution: Option<PluginExecution>,
    pub component_heartbeats: HashMap<SystemComponent, bool>,
    pub frame: Option<crate::rt::Frame>,
}

/// USB communication information
#[derive(Debug, Clone)]
pub struct UsbInfo {
    pub consecutive_failures: u32,
    pub last_success: Option<Instant>,
}

/// Plugin execution information
#[derive(Debug, Clone)]
pub struct PluginExecution {
    pub plugin_id: String,
    pub execution_time_us: u64,
}

/// Result of fault manager update
#[derive(Debug, Clone, Default)]
pub struct FaultManagerResult {
    pub new_faults: Vec<FaultType>,
    pub recovery_actions: Vec<String>,
    pub current_torque_multiplier: f32,
    pub soft_stop_active: bool,
}

/// Fault statistics for a specific fault type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultStatistics {
    pub fault_type: FaultType,
    pub total_count: u32,
    pub recovery_success_rate: f64,
    #[serde(with = "duration_serde")]
    pub average_recovery_time: Duration,
    #[serde(with = "option_instant_serde")]
    pub last_occurrence: Option<Instant>,
}

// Serde modules for time types
mod instant_serde {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(_instant: &Instant, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration_since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        duration_since_epoch.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Instant, D::Error>
    where
        D: Deserializer<'de>,
    {
        let _secs = u64::deserialize(deserializer)?;
        Ok(Instant::now()) // Approximate for deserialization
    }
}

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
        Ok(opt_secs.map(|_secs| Instant::now()))
    }
}

mod duration_serde {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_millis().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u128::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis as u64))
    }
}

mod option_duration_serde {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(opt_duration: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match opt_duration {
            Some(duration) => Some(duration.as_millis()).serialize(serializer),
            None => None::<u128>.serialize(serializer),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt_millis = Option::<u128>::deserialize(deserializer)?;
        Ok(opt_millis.map(|millis| Duration::from_millis(millis as u64)))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_integrated_fault_manager_creation() {
        let manager = IntegratedFaultManager::new(5.0, 25.0, WatchdogConfig::default());

        let health = manager.get_health_summary();
        assert_eq!(health.overall_status, HealthStatus::Healthy); // Initial state
        assert!(health.active_faults.is_empty());
        assert!(health.quarantined_plugins.is_empty());
    }

    #[test]
    fn test_fault_detection_and_handling() {
        let mut manager = IntegratedFaultManager::new(5.0, 25.0, WatchdogConfig::default());

        let context = FaultManagerContext {
            current_torque: 10.0,
            encoder_value: Some(f32::NAN), // Trigger encoder fault
            ..Default::default()
        };

        let mut result = FaultManagerResult::default();
        for _ in 0..5 {
            result = manager.update(&context);
            if result.new_faults.contains(&FaultType::EncoderNaN) {
                break;
            }
        }

        // Should detect encoder fault after threshold
        assert!(result.new_faults.contains(&FaultType::EncoderNaN));

        // Should have created blackbox marker
        assert!(!manager.get_blackbox_markers().is_empty());

        // Should have fault in history
        let stats = manager.get_fault_statistics();
        assert!(stats.contains_key(&FaultType::EncoderNaN));
    }

    #[test]
    fn test_recovery_procedure_execution() {
        let mut manager = IntegratedFaultManager::new(5.0, 25.0, WatchdogConfig::default());

        // Trigger a fault first
        manager.safety_service.report_fault(FaultType::UsbStall);

        // Execute recovery procedure
        let result = manager.execute_recovery_procedure(FaultType::UsbStall);
        assert!(result.is_ok());

        // Check that recovery was recorded in history
        let stats = manager.get_fault_statistics();
        if let Some(usb_stats) = stats.get(&FaultType::UsbStall) {
            assert!(usb_stats.recovery_success_rate > 0.0);
        }
    }

    #[test]
    fn test_plugin_quarantine_integration() {
        let mut manager = IntegratedFaultManager::new(5.0, 25.0, WatchdogConfig::default());

        let context = FaultManagerContext {
            plugin_execution: Some(PluginExecution {
                plugin_id: "test_plugin".to_string(),
                execution_time_us: 200, // Over default 100us threshold
            }),
            ..Default::default()
        };

        // Trigger multiple plugin overruns
        for _ in 0..10 {
            let result = manager.update(&context);
            if result.new_faults.contains(&FaultType::PluginOverrun) {
                break;
            }
        }

        let health = manager.get_health_summary();
        assert!(
            health
                .quarantined_plugins
                .contains(&"test_plugin".to_string())
        );
    }

    #[test]
    fn test_soft_stop_integration() {
        let mut manager = IntegratedFaultManager::new(5.0, 25.0, WatchdogConfig::default());

        // Trigger thermal fault
        let context = FaultManagerContext {
            current_torque: 15.0,
            temperature: Some(85.0), // Over thermal threshold
            ..Default::default()
        };

        let result = manager.update(&context);

        // Should trigger soft stop
        assert!(result.soft_stop_active);
        std::thread::sleep(Duration::from_millis(5));
        let result = manager.update(&context);
        assert!(result.current_torque_multiplier < 1.0);
    }

    #[test]
    fn test_health_summary() {
        let mut manager = IntegratedFaultManager::new(5.0, 25.0, WatchdogConfig::default());

        // Send some heartbeats
        let mut context = FaultManagerContext::default();
        context
            .component_heartbeats
            .insert(SystemComponent::RtThread, true);
        context
            .component_heartbeats
            .insert(SystemComponent::HidCommunication, true);

        manager.update(&context);

        let health = manager.get_health_summary();
        assert_eq!(
            health.component_health[&SystemComponent::RtThread],
            HealthStatus::Healthy
        );
        assert_eq!(
            health.component_health[&SystemComponent::HidCommunication],
            HealthStatus::Healthy
        );
    }

    #[test]
    fn test_fault_injection_integration() {
        let mut manager = IntegratedFaultManager::new(5.0, 25.0, WatchdogConfig::default());

        // Enable fault injection
        manager.enable_fault_injection(true);

        // Add a manual test scenario
        let scenario = crate::safety::fault_injection::FaultInjectionScenario {
            name: "test_scenario".to_string(),
            fault_type: FaultType::UsbStall,
            trigger_condition: crate::safety::fault_injection::TriggerCondition::Manual,
            duration: Some(Duration::from_millis(100)),
            recovery_condition: None,
            enabled: true,
        };

        manager
            .fault_injection_mut()
            .add_scenario(scenario)
            .unwrap();
        manager
            .fault_injection_mut()
            .trigger_scenario("test_scenario")
            .unwrap();

        let context = FaultManagerContext::default();
        let result = manager.update(&context);

        // Should detect injected fault
        assert!(result.new_faults.contains(&FaultType::UsbStall));
    }

    #[test]
    fn test_blackbox_marker_cleanup() {
        let mut manager = IntegratedFaultManager::new(5.0, 25.0, WatchdogConfig::default());

        // Create some fault markers
        manager.handle_fault(FaultType::UsbStall, 10.0);
        manager.handle_fault(FaultType::ThermalLimit, 5.0);

        assert_eq!(manager.get_blackbox_markers().len(), 2);

        // Clear old markers (none should be cleared since they're recent)
        manager.clear_old_blackbox_markers(Duration::from_secs(1));
        assert_eq!(manager.get_blackbox_markers().len(), 2);

        // Clear all markers
        std::thread::sleep(Duration::from_millis(2));
        manager.clear_old_blackbox_markers(Duration::from_millis(1));
        assert_eq!(manager.get_blackbox_markers().len(), 0);
    }
}
