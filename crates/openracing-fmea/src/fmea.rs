//! FMEA (Failure Mode & Effects Analysis) system for comprehensive fault management.

use crate::{
    AudioAlert, AudioAlertSystem, FaultAction, FaultDetectionState, FaultThresholds, FaultType,
    FmeaError, FmeaResult, PostMortemConfig, RecoveryProcedure, SoftStopController,
};
use core::time::Duration;

/// FMEA entry defining fault detection, action, and recovery.
#[derive(Debug, Clone)]
pub struct FmeaEntry {
    /// Fault type this entry applies to.
    pub fault_type: FaultType,
    /// Detection method description.
    pub detection_method: heapless::String<128>,
    /// Action to take when fault is detected.
    pub action: FaultAction,
    /// Maximum response time in milliseconds.
    pub max_response_time_ms: u64,
    /// Post-mortem data collection configuration.
    pub post_mortem: PostMortemConfig,
    /// Recovery procedure description.
    pub recovery_procedure: heapless::String<256>,
    /// Whether this fault type is enabled for detection.
    pub enabled: bool,
}

impl FmeaEntry {
    /// Create a new FMEA entry for a fault type.
    pub fn new(fault_type: FaultType) -> Self {
        let mut detection_method = heapless::String::new();
        let mut recovery_procedure = heapless::String::new();

        match fault_type {
            FaultType::UsbStall => {
                let _ = detection_method.push_str("USB write timeout or consecutive failures");
                let _ = recovery_procedure
                    .push_str("Retry with exponential backoff, reconnect if needed");
            }
            FaultType::EncoderNaN => {
                let _ = detection_method.push_str("NaN or infinite values in encoder data");
                let _ =
                    recovery_procedure.push_str("Recalibrate encoder, use last known good value");
            }
            FaultType::ThermalLimit => {
                let _ = detection_method.push_str("Temperature sensor reading above threshold");
                let _ =
                    recovery_procedure.push_str("Reduce torque, wait for cooldown with hysteresis");
            }
            FaultType::Overcurrent => {
                let _ = detection_method.push_str("Current sensor reading above safe threshold");
                let _ = recovery_procedure
                    .push_str("Immediate torque cutoff, check for hardware issues");
            }
            FaultType::PluginOverrun => {
                let _ = detection_method.push_str("Plugin execution time exceeds budget");
                let _ = recovery_procedure.push_str("Quarantine plugin, continue engine operation");
            }
            FaultType::TimingViolation => {
                let _ = detection_method.push_str("RT loop jitter exceeds threshold");
                let _ = recovery_procedure.push_str("Log violation, adjust RT priority if needed");
            }
            FaultType::SafetyInterlockViolation => {
                let _ = detection_method.push_str("Safety interlock protocol violation");
                let _ =
                    recovery_procedure.push_str("Require new challenge, verify physical presence");
            }
            FaultType::HandsOffTimeout => {
                let _ = detection_method.push_str("Hands-off timeout exceeded during high-torque");
                let _ = recovery_procedure.push_str("Reduce to safe torque, verify hands on wheel");
            }
            FaultType::PipelineFault => {
                let _ = detection_method.push_str("Filter pipeline processing error");
                let _ = recovery_procedure.push_str("Reset pipeline, verify output validity");
            }
        }

        Self {
            fault_type,
            detection_method,
            action: Self::default_action_for(fault_type),
            max_response_time_ms: fault_type.default_max_response_time_ms(),
            post_mortem: PostMortemConfig::default(),
            recovery_procedure,
            enabled: true,
        }
    }

    /// Get the default action for a fault type.
    fn default_action_for(fault_type: FaultType) -> FaultAction {
        match fault_type {
            FaultType::UsbStall => FaultAction::SoftStop,
            FaultType::EncoderNaN => FaultAction::SoftStop,
            FaultType::ThermalLimit => FaultAction::SoftStop,
            FaultType::Overcurrent => FaultAction::SoftStop,
            FaultType::PluginOverrun => FaultAction::Quarantine,
            FaultType::TimingViolation => FaultAction::LogAndContinue,
            FaultType::SafetyInterlockViolation => FaultAction::SafeMode,
            FaultType::HandsOffTimeout => FaultAction::SoftStop,
            FaultType::PipelineFault => FaultAction::Restart,
        }
    }

    /// Set the fault action.
    pub fn with_action(mut self, action: FaultAction) -> Self {
        self.action = action;
        self
    }

    /// Set the maximum response time.
    pub fn with_response_time(mut self, ms: u64) -> Self {
        self.max_response_time_ms = ms;
        self
    }

    /// Enable or disable this entry.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// FMEA matrix containing all fault entries.
#[derive(Debug, Clone)]
pub struct FmeaMatrix {
    entries: heapless::Vec<(FaultType, FmeaEntry), 16>,
}

impl Default for FmeaMatrix {
    fn default() -> Self {
        Self::new()
    }
}

impl FmeaMatrix {
    /// Create a new empty FMEA matrix.
    pub fn new() -> Self {
        Self {
            entries: heapless::Vec::new(),
        }
    }

    /// Create a FMEA matrix with default entries for all fault types.
    pub fn with_defaults() -> Self {
        let mut matrix = Self::new();
        let _ = matrix.insert(FmeaEntry::new(FaultType::UsbStall));
        let _ = matrix.insert(FmeaEntry::new(FaultType::EncoderNaN));
        let _ = matrix.insert(FmeaEntry::new(FaultType::ThermalLimit));
        let _ = matrix.insert(FmeaEntry::new(FaultType::Overcurrent));
        let _ = matrix.insert(FmeaEntry::new(FaultType::PluginOverrun));
        let _ = matrix.insert(FmeaEntry::new(FaultType::TimingViolation));
        let _ = matrix.insert(FmeaEntry::new(FaultType::SafetyInterlockViolation));
        let _ = matrix.insert(FmeaEntry::new(FaultType::HandsOffTimeout));
        let _ = matrix.insert(FmeaEntry::new(FaultType::PipelineFault));
        matrix
    }

    /// Insert or update an entry.
    ///
    /// Returns `true` if inserted successfully, `false` if matrix is full.
    pub fn insert(&mut self, entry: FmeaEntry) -> bool {
        let fault_type = entry.fault_type;

        // Check if entry already exists
        for (ft, e) in &mut self.entries {
            if *ft == fault_type {
                *e = entry;
                return true;
            }
        }

        // Add new entry
        self.entries.push((fault_type, entry)).is_ok()
    }

    /// Get an entry by fault type.
    pub fn get(&self, fault_type: FaultType) -> Option<&FmeaEntry> {
        self.entries
            .iter()
            .find(|(ft, _)| *ft == fault_type)
            .map(|(_, entry)| entry)
    }

    /// Get a mutable entry by fault type.
    pub fn get_mut(&mut self, fault_type: FaultType) -> Option<&mut FmeaEntry> {
        self.entries
            .iter_mut()
            .find(|(ft, _)| *ft == fault_type)
            .map(|(_, entry)| entry)
    }

    /// Check if an entry exists for a fault type.
    pub fn contains(&self, fault_type: FaultType) -> bool {
        self.entries.iter().any(|(ft, _)| *ft == fault_type)
    }

    /// Get all fault types in the matrix.
    pub fn fault_types(&self) -> impl Iterator<Item = FaultType> + '_ {
        self.entries.iter().map(|(ft, _)| *ft)
    }

    /// Get the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the matrix is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove an entry.
    pub fn remove(&mut self, fault_type: FaultType) -> Option<FmeaEntry> {
        let idx = self.entries.iter().position(|(ft, _)| *ft == fault_type)?;
        Some(self.entries.swap_remove(idx).1)
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// FMEA system for comprehensive fault management.
///
/// This is the central coordinator for all fault detection, isolation,
/// and recovery operations in the FFB system.
///
/// # RT-Safety
///
/// All detection methods in this struct are RT-safe:
/// - No heap allocations in hot paths
/// - No blocking operations
/// - Bounded execution time
/// - Deterministic behavior
///
/// # State Machine
///
/// ```text
/// ┌─────────────┐     fault detected
/// │   Normal    │ ──────────────────────► ┌─────────────┐
/// └─────────────┘                         │   Faulted   │
///        ▲                                └──────┬──────┘
///        │                                       │
///        │ recovery successful                   │ soft-stop
///        │                                       ▼
///        │                               ┌─────────────┐
///        └───────────────────────────────│  Recovering │
///                                        └─────────────┘
/// ```
#[derive(Debug)]
pub struct FmeaSystem {
    /// Fault detection thresholds.
    thresholds: FaultThresholds,
    /// FMEA matrix with all fault entries.
    fmea_matrix: FmeaMatrix,
    /// Detection state for each fault type.
    detection_states: heapless::Vec<(FaultType, FaultDetectionState), 16>,
    /// Soft-stop controller.
    soft_stop: SoftStopController,
    /// Audio alert system.
    audio_alerts: AudioAlertSystem,
    /// Current time (updated each tick).
    current_time: Duration,
    /// Active fault (if any).
    active_fault: Option<FaultType>,
}

impl Default for FmeaSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FmeaSystem {
    /// Create a new FMEA system with default configuration.
    pub fn new() -> Self {
        Self::with_thresholds(FaultThresholds::default())
    }

    /// Create a new FMEA system with custom thresholds.
    pub fn with_thresholds(thresholds: FaultThresholds) -> Self {
        let fmea_matrix = FmeaMatrix::with_defaults();
        let mut detection_states = heapless::Vec::new();

        for fault_type in fmea_matrix.fault_types() {
            let _ = detection_states.push((fault_type, FaultDetectionState::new()));
        }

        Self {
            thresholds,
            fmea_matrix,
            detection_states,
            soft_stop: SoftStopController::new(),
            audio_alerts: AudioAlertSystem::new(),
            current_time: Duration::ZERO,
            active_fault: None,
        }
    }

    /// Get the current time.
    pub fn current_time(&self) -> Duration {
        self.current_time
    }

    /// Update the current time.
    pub fn update_time(&mut self, time: Duration) {
        self.current_time = time;
    }

    /// Get the current thresholds.
    pub fn thresholds(&self) -> &FaultThresholds {
        &self.thresholds
    }

    /// Update the thresholds.
    pub fn set_thresholds(&mut self, thresholds: FaultThresholds) {
        self.thresholds = thresholds;
    }

    /// Get the FMEA matrix.
    pub fn fmea_matrix(&self) -> &FmeaMatrix {
        &self.fmea_matrix
    }

    /// Get a mutable reference to the FMEA matrix.
    pub fn fmea_matrix_mut(&mut self) -> &mut FmeaMatrix {
        &mut self.fmea_matrix
    }

    /// Get detection state for a fault type.
    fn detection_state(&mut self, fault_type: FaultType) -> Option<&mut FaultDetectionState> {
        self.detection_states
            .iter_mut()
            .find(|(ft, _)| *ft == fault_type)
            .map(|(_, state)| state)
    }

    /// Get the current active fault (if any).
    pub fn active_fault(&self) -> Option<FaultType> {
        self.active_fault
    }

    /// Check if there is an active fault.
    pub fn has_active_fault(&self) -> bool {
        self.active_fault.is_some()
    }

    /// Detect USB communication faults.
    ///
    /// # RT-Safety
    ///
    /// This method is RT-safe with bounded execution time.
    ///
    /// # Arguments
    ///
    /// * `consecutive_failures` - Number of consecutive USB failures.
    /// * `last_success_time` - Time of last successful USB communication.
    ///
    /// # Returns
    ///
    /// `Some(FaultType::UsbStall)` if a fault is detected, `None` otherwise.
    pub fn detect_usb_fault(
        &mut self,
        consecutive_failures: u32,
        last_success_time: Option<Duration>,
    ) -> Option<FaultType> {
        let state = self.detection_state(FaultType::UsbStall)?;

        state.consecutive_count = consecutive_failures;

        // Check timeout
        if let Some(last_success) = last_success_time {
            let timeout = Duration::from_millis(self.thresholds.usb_timeout_ms);
            if self.current_time.saturating_sub(last_success) > timeout {
                return Some(FaultType::UsbStall);
            }
        }

        // Check consecutive failures
        if consecutive_failures >= self.thresholds.usb_max_consecutive_failures {
            return Some(FaultType::UsbStall);
        }

        None
    }

    /// Detect encoder NaN faults.
    ///
    /// # RT-Safety
    ///
    /// This method is RT-safe with bounded execution time.
    ///
    /// # Arguments
    ///
    /// * `encoder_value` - The current encoder value to check.
    ///
    /// # Returns
    ///
    /// `Some(FaultType::EncoderNaN)` if a fault is detected, `None` otherwise.
    pub fn detect_encoder_fault(&mut self, encoder_value: f32) -> Option<FaultType> {
        if encoder_value.is_finite() {
            return None;
        }

        // Cache values before borrowing
        let current_time = self.current_time;
        let window_duration = Duration::from_millis(self.thresholds.encoder_nan_window_ms);
        let max_nan_count = self.thresholds.encoder_max_nan_count;

        let state = self.detection_state(FaultType::EncoderNaN)?;

        state.window_count = state.window_count.saturating_add(1);
        state.last_occurrence = Some(current_time);

        // Check if window needs reset
        let window_start = state.window_start.unwrap_or(current_time);
        if current_time.saturating_sub(window_start) > window_duration {
            state.window_start = Some(current_time);
            state.window_count = 1;
        }

        // Check threshold
        if state.window_count >= max_nan_count {
            return Some(FaultType::EncoderNaN);
        }

        None
    }

    /// Detect thermal faults.
    ///
    /// # RT-Safety
    ///
    /// This method is RT-safe with bounded execution time.
    ///
    /// # Arguments
    ///
    /// * `temperature_celsius` - Current temperature reading.
    /// * `fault_already_active` - Whether a thermal fault is already active.
    ///
    /// # Returns
    ///
    /// `Some(FaultType::ThermalLimit)` if a fault is detected, `None` otherwise.
    pub fn detect_thermal_fault(
        &mut self,
        temperature_celsius: f32,
        fault_already_active: bool,
    ) -> Option<FaultType> {
        let threshold = if fault_already_active {
            self.thresholds.thermal_limit_celsius - self.thresholds.thermal_hysteresis_celsius
        } else {
            self.thresholds.thermal_limit_celsius
        };

        if temperature_celsius > threshold && !fault_already_active {
            Some(FaultType::ThermalLimit)
        } else {
            None
        }
    }

    /// Detect plugin overrun faults.
    ///
    /// # RT-Safety
    ///
    /// This method is RT-safe with bounded execution time.
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - Identifier of the plugin (used for quarantine).
    /// * `execution_time_us` - Plugin execution time in microseconds.
    ///
    /// # Returns
    ///
    /// `Some(FaultType::PluginOverrun)` if a fault is detected, `None` otherwise.
    pub fn detect_plugin_overrun(
        &mut self,
        _plugin_id: &str,
        execution_time_us: u64,
    ) -> Option<FaultType> {
        if execution_time_us <= self.thresholds.plugin_timeout_us {
            return None;
        }

        // Cache values before borrowing
        let current_time = self.current_time;
        let max_overruns = self.thresholds.plugin_max_overruns;

        let state = self.detection_state(FaultType::PluginOverrun)?;

        state.consecutive_count = state.consecutive_count.saturating_add(1);
        state.last_occurrence = Some(current_time);

        if state.consecutive_count >= max_overruns {
            return Some(FaultType::PluginOverrun);
        }

        None
    }

    /// Detect timing violation faults.
    ///
    /// # RT-Safety
    ///
    /// This method is RT-safe with bounded execution time.
    ///
    /// # Arguments
    ///
    /// * `jitter_us` - Jitter in microseconds.
    ///
    /// # Returns
    ///
    /// `Some(FaultType::TimingViolation)` if a fault is detected, `None` otherwise.
    pub fn detect_timing_violation(&mut self, jitter_us: u64) -> Option<FaultType> {
        if jitter_us <= self.thresholds.timing_violation_threshold_us {
            return None;
        }

        // Cache values before borrowing
        let current_time = self.current_time;
        let max_violations = self.thresholds.timing_max_violations;

        let state = self.detection_state(FaultType::TimingViolation)?;

        state.consecutive_count = state.consecutive_count.saturating_add(1);
        state.last_occurrence = Some(current_time);

        if state.consecutive_count >= max_violations {
            return Some(FaultType::TimingViolation);
        }

        None
    }

    /// Handle a detected fault.
    ///
    /// # Arguments
    ///
    /// * `fault_type` - The type of fault to handle.
    /// * `current_torque` - Current torque value (for soft-stop).
    ///
    /// # Returns
    ///
    /// Result indicating success or failure.
    #[allow(clippy::result_large_err)]
    pub fn handle_fault(&mut self, fault_type: FaultType, current_torque: f32) -> FmeaResult<()> {
        let entry = self
            .fmea_matrix
            .get(fault_type)
            .ok_or(FmeaError::UnknownFaultType(fault_type))?;

        if !entry.enabled {
            return Ok(());
        }

        // Set active fault
        self.active_fault = Some(fault_type);

        // Execute fault action
        match entry.action {
            FaultAction::SoftStop => {
                self.soft_stop.start_soft_stop(current_torque);
            }
            FaultAction::Quarantine => {
                // Plugin quarantine handled separately
            }
            FaultAction::LogAndContinue => {}
            FaultAction::Restart => {}
            FaultAction::SafeMode => {
                self.soft_stop.start_soft_stop(current_torque);
            }
        }

        // Trigger audio alert
        let alert = AudioAlert::for_fault_type(fault_type);
        self.audio_alerts
            .trigger(alert, self.current_time.as_millis() as u64);

        Ok(())
    }

    /// Clear the active fault.
    #[allow(clippy::result_large_err)]
    pub fn clear_fault(&mut self) -> FmeaResult<()> {
        if self.active_fault.is_none() {
            return Err(FmeaError::NoActiveFault);
        }

        // Reset detection state
        if let Some(fault_type) = self.active_fault
            && let Some(state) = self.detection_state(fault_type)
        {
            state.consecutive_count = 0;
        }

        self.active_fault = None;
        self.soft_stop.reset();

        Ok(())
    }

    /// Update the soft-stop controller.
    ///
    /// # Arguments
    ///
    /// * `delta` - Time elapsed since last update.
    ///
    /// # Returns
    ///
    /// Current torque after soft-stop update.
    pub fn update_soft_stop(&mut self, delta: Duration) -> f32 {
        self.soft_stop.update(delta)
    }

    /// Get the soft-stop controller.
    pub fn soft_stop(&self) -> &SoftStopController {
        &self.soft_stop
    }

    /// Get a mutable reference to the soft-stop controller.
    pub fn soft_stop_mut(&mut self) -> &mut SoftStopController {
        &mut self.soft_stop
    }

    /// Check if soft-stop is active.
    pub fn is_soft_stop_active(&self) -> bool {
        self.soft_stop.is_active()
    }

    /// Force stop the soft-stop mechanism immediately.
    pub fn force_stop_soft_stop(&mut self) {
        self.soft_stop.force_stop();
    }

    /// Get the audio alert system.
    pub fn audio_alerts(&self) -> &AudioAlertSystem {
        &self.audio_alerts
    }

    /// Get a mutable reference to the audio alert system.
    pub fn audio_alerts_mut(&mut self) -> &mut AudioAlertSystem {
        &mut self.audio_alerts
    }

    /// Update the audio alert system.
    ///
    /// # Returns
    ///
    /// Current active alert (if any).
    pub fn update_audio_alerts(&mut self) -> Option<AudioAlert> {
        self.audio_alerts
            .update(self.current_time.as_millis() as u64)
    }

    /// Get fault statistics.
    ///
    /// Returns a collection of fault types with their consecutive counts
    /// and last occurrence times.
    pub fn fault_statistics(
        &self,
    ) -> impl Iterator<Item = (FaultType, u32, Option<Duration>)> + '_ {
        self.detection_states
            .iter()
            .map(|(ft, state)| (*ft, state.consecutive_count, state.last_occurrence))
    }

    /// Reset detection state for a specific fault type.
    pub fn reset_detection_state(&mut self, fault_type: FaultType) {
        if let Some(state) = self.detection_state(fault_type) {
            state.consecutive_count = 0;
            state.window_count = 0;
            state.last_occurrence = None;
        }
    }

    /// Reset all detection states.
    pub fn reset_all_detection_states(&mut self) {
        for (_, state) in &mut self.detection_states {
            state.consecutive_count = 0;
            state.window_count = 0;
            state.last_occurrence = None;
        }
    }

    /// Check if recovery is possible for the active fault.
    pub fn can_recover(&self) -> bool {
        match self.active_fault {
            Some(ft) => ft.is_recoverable(),
            None => false,
        }
    }

    /// Get the recovery procedure for the active fault.
    pub fn recovery_procedure(&self) -> Option<RecoveryProcedure> {
        self.active_fault.map(RecoveryProcedure::default_for)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn test_fmea_entry_creation() {
        let entry = FmeaEntry::new(FaultType::UsbStall);
        assert_eq!(entry.fault_type, FaultType::UsbStall);
        assert!(entry.enabled);
        assert_eq!(entry.action, FaultAction::SoftStop);
    }

    #[test]
    fn test_fmea_entry_customization() {
        let entry = FmeaEntry::new(FaultType::TimingViolation)
            .with_action(FaultAction::LogAndContinue)
            .with_response_time(100)
            .enabled(false);

        assert_eq!(entry.action, FaultAction::LogAndContinue);
        assert_eq!(entry.max_response_time_ms, 100);
        assert!(!entry.enabled);
    }

    #[test]
    fn test_fmea_matrix() {
        let mut matrix = FmeaMatrix::new();
        assert!(matrix.is_empty());

        let entry = FmeaEntry::new(FaultType::UsbStall);
        assert!(matrix.insert(entry));
        assert_eq!(matrix.len(), 1);

        assert!(matrix.contains(FaultType::UsbStall));
        assert!(matrix.get(FaultType::UsbStall).is_some());

        let removed = matrix.remove(FaultType::UsbStall);
        assert!(removed.is_some());
        assert!(matrix.is_empty());
    }

    #[test]
    fn test_fmea_matrix_defaults() {
        let matrix = FmeaMatrix::with_defaults();
        assert!(matrix.contains(FaultType::UsbStall));
        assert!(matrix.contains(FaultType::EncoderNaN));
        assert!(matrix.contains(FaultType::ThermalLimit));
    }

    #[test]
    fn test_fmea_system_creation() {
        let fmea = FmeaSystem::new();
        assert!(!fmea.has_active_fault());
        assert!(!fmea.is_soft_stop_active());
    }

    #[test]
    fn test_fmea_system_usb_detection() {
        let mut fmea = FmeaSystem::new();

        // No fault initially
        assert!(fmea.detect_usb_fault(0, Some(Duration::ZERO)).is_none());

        // Consecutive failures
        assert!(fmea.detect_usb_fault(1, Some(Duration::ZERO)).is_none());
        assert!(fmea.detect_usb_fault(2, Some(Duration::ZERO)).is_none());
        assert_eq!(
            fmea.detect_usb_fault(3, Some(Duration::ZERO)),
            Some(FaultType::UsbStall)
        );
    }

    #[test]
    fn test_fmea_system_usb_timeout() {
        let mut fmea = FmeaSystem::new();
        fmea.update_time(Duration::from_millis(50));

        // Old last success should trigger fault
        let result = fmea.detect_usb_fault(0, Some(Duration::from_millis(20)));
        assert_eq!(result, Some(FaultType::UsbStall));
    }

    #[test]
    fn test_fmea_system_encoder_detection() {
        let mut fmea = FmeaSystem::new();

        // Normal values should not trigger
        assert!(fmea.detect_encoder_fault(1.5).is_none());
        assert!(fmea.detect_encoder_fault(0.0).is_none());

        // Single NaN should not trigger
        assert!(fmea.detect_encoder_fault(f32::NAN).is_none());

        // Multiple NaNs should trigger
        for _ in 0..5 {
            let result = fmea.detect_encoder_fault(f32::NAN);
            if result.is_some() {
                assert_eq!(result, Some(FaultType::EncoderNaN));
                return;
            }
        }
        panic!("Should have detected encoder fault after 5 NaNs");
    }

    #[test]
    fn test_fmea_system_thermal_detection() {
        let mut fmea = FmeaSystem::new();

        // Normal temperature
        assert!(fmea.detect_thermal_fault(70.0, false).is_none());

        // Over threshold
        assert_eq!(
            fmea.detect_thermal_fault(85.0, false),
            Some(FaultType::ThermalLimit)
        );

        // Hysteresis - should not clear immediately
        assert!(fmea.detect_thermal_fault(79.0, true).is_none());

        // Below hysteresis threshold
        assert!(fmea.detect_thermal_fault(74.0, true).is_none());
    }

    #[test]
    fn test_fmea_system_plugin_overrun() {
        let mut fmea = FmeaSystem::new();

        // Under threshold
        assert!(fmea.detect_plugin_overrun("test", 50).is_none());

        // Over threshold but not enough occurrences
        for i in 0..9 {
            let result = fmea.detect_plugin_overrun("test", 150);
            assert!(result.is_none(), "Should not fault on iteration {}", i);
        }

        // 10th overrun should trigger
        assert_eq!(
            fmea.detect_plugin_overrun("test", 150),
            Some(FaultType::PluginOverrun)
        );
    }

    #[test]
    fn test_fmea_system_timing_violation() {
        let mut fmea = FmeaSystem::new();

        // Normal jitter
        assert!(fmea.detect_timing_violation(100).is_none());

        // High jitter but not enough violations
        for _ in 0..99 {
            assert!(fmea.detect_timing_violation(300).is_none());
        }

        // 100th violation should trigger
        assert_eq!(
            fmea.detect_timing_violation(300),
            Some(FaultType::TimingViolation)
        );
    }

    #[test]
    fn test_fmea_system_fault_handling() {
        let mut fmea = FmeaSystem::new();

        fmea.handle_fault(FaultType::UsbStall, 10.0).unwrap();

        assert!(fmea.has_active_fault());
        assert_eq!(fmea.active_fault(), Some(FaultType::UsbStall));
        assert!(fmea.is_soft_stop_active());
    }

    #[test]
    fn test_fmea_system_clear_fault() {
        let mut fmea = FmeaSystem::new();
        fmea.handle_fault(FaultType::UsbStall, 10.0).unwrap();

        fmea.clear_fault().unwrap();
        assert!(!fmea.has_active_fault());
        assert!(!fmea.is_soft_stop_active());
    }

    #[test]
    fn test_fmea_system_clear_no_fault() {
        let mut fmea = FmeaSystem::new();
        let result = fmea.clear_fault();
        assert!(matches!(result, Err(FmeaError::NoActiveFault)));
    }

    #[test]
    fn test_fmea_system_soft_stop_update() {
        let mut fmea = FmeaSystem::new();
        fmea.handle_fault(FaultType::UsbStall, 10.0).unwrap();

        let torque = fmea.update_soft_stop(Duration::from_millis(25));
        assert!(torque > 0.0 && torque < 10.0);
    }

    #[test]
    fn test_fmea_system_statistics() {
        let mut fmea = FmeaSystem::new();
        fmea.detect_usb_fault(2, Some(Duration::ZERO));

        let stats: Vec<_> = fmea.fault_statistics().collect();
        let usb_stat = stats.iter().find(|(ft, _, _)| *ft == FaultType::UsbStall);
        assert!(usb_stat.is_some());
        assert_eq!(usb_stat.unwrap().1, 2);
    }
}
