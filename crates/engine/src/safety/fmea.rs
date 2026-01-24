//! FMEA (Failure Mode & Effects Analysis) system for fault detection and handling

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::{FaultType, SafetyService};

/// Fault detection thresholds and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultThresholds {
    /// USB communication timeout threshold
    pub usb_timeout_ms: u64,
    /// Maximum consecutive USB failures before fault
    pub usb_max_consecutive_failures: u32,
    /// Encoder NaN detection window
    pub encoder_nan_window_ms: u64,
    /// Maximum encoder NaN count in window
    pub encoder_max_nan_count: u32,
    /// Thermal protection threshold in Celsius
    pub thermal_limit_celsius: f32,
    /// Thermal hysteresis for recovery
    pub thermal_hysteresis_celsius: f32,
    /// Plugin execution timeout per tick
    pub plugin_timeout_us: u64,
    /// Maximum plugin overruns before quarantine
    pub plugin_max_overruns: u32,
    /// Timing violation threshold (jitter)
    pub timing_violation_threshold_us: u64,
    /// Maximum timing violations before fault
    pub timing_max_violations: u32,
}

impl Default for FaultThresholds {
    fn default() -> Self {
        Self {
            usb_timeout_ms: 10,
            usb_max_consecutive_failures: 3,
            encoder_nan_window_ms: 1000,
            encoder_max_nan_count: 5,
            thermal_limit_celsius: 80.0,
            thermal_hysteresis_celsius: 5.0,
            plugin_timeout_us: 100,
            plugin_max_overruns: 10,
            timing_violation_threshold_us: 250,
            timing_max_violations: 100,
        }
    }
}

/// Fault action to take when fault is detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaultAction {
    /// Immediately ramp torque to zero
    SoftStop,
    /// Quarantine the faulty component
    Quarantine,
    /// Log and continue operation
    LogAndContinue,
    /// Restart the component
    Restart,
    /// Enter safe mode
    SafeMode,
}

/// Post-mortem data collection requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostMortemConfig {
    /// Capture duration before fault (seconds)
    pub pre_fault_capture_duration: f32,
    /// Capture duration after fault (seconds)
    pub post_fault_capture_duration: f32,
    /// Include telemetry data
    pub include_telemetry: bool,
    /// Include device state
    pub include_device_state: bool,
    /// Include plugin state
    pub include_plugin_state: bool,
}

impl Default for PostMortemConfig {
    fn default() -> Self {
        Self {
            pre_fault_capture_duration: 2.0,
            post_fault_capture_duration: 1.0,
            include_telemetry: true,
            include_device_state: true,
            include_plugin_state: true,
        }
    }
}

/// FMEA entry defining fault detection, action, and post-mortem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FmeaEntry {
    pub fault_type: FaultType,
    pub detection_method: String,
    pub action: FaultAction,
    pub max_response_time_ms: u64,
    pub post_mortem: PostMortemConfig,
    pub recovery_procedure: String,
    pub enabled: bool,
}

/// Fault detection state for a specific fault type
#[derive(Debug, Clone)]
struct FaultDetectionState {
    consecutive_count: u32,
    last_occurrence: Option<Instant>,
    window_start: Option<Instant>,
    window_count: u32,
    /// TODO: Used for future fault quarantine implementation
    #[allow(dead_code)]
    quarantined: bool,
    /// TODO: Used for future fault quarantine implementation
    #[allow(dead_code)]
    quarantine_until: Option<Instant>,
}

impl Default for FaultDetectionState {
    fn default() -> Self {
        Self {
            consecutive_count: 0,
            last_occurrence: None,
            window_start: None,
            window_count: 0,
            quarantined: false,
            quarantine_until: None,
        }
    }
}

/// Soft-stop mechanism for torque ramping
#[derive(Debug, Clone)]
pub struct SoftStopController {
    active: bool,
    start_time: Option<Instant>,
    start_torque: f32,
    target_torque: f32,
    ramp_duration: Duration,
    current_torque: f32,
}

impl SoftStopController {
    pub fn new() -> Self {
        Self {
            active: false,
            start_time: None,
            start_torque: 0.0,
            target_torque: 0.0,
            ramp_duration: Duration::from_millis(50), // ≤50ms requirement
            current_torque: 0.0,
        }
    }

    /// Start soft-stop from current torque to zero
    pub fn start_soft_stop(&mut self, current_torque: f32) {
        self.active = true;
        self.start_time = Some(Instant::now());
        self.start_torque = current_torque;
        self.target_torque = 0.0;
        self.current_torque = current_torque;
    }

    /// Update soft-stop and return current torque value
    pub fn update(&mut self) -> f32 {
        if !self.active {
            return self.current_torque;
        }

        let Some(start_time) = self.start_time else {
            return self.current_torque;
        };

        let elapsed = start_time.elapsed();
        if elapsed >= self.ramp_duration {
            // Ramp complete
            self.active = false;
            self.current_torque = self.target_torque;
            return self.current_torque;
        }

        // Linear ramp
        let progress = elapsed.as_secs_f32() / self.ramp_duration.as_secs_f32();
        self.current_torque =
            self.start_torque + (self.target_torque - self.start_torque) * progress;

        self.current_torque
    }

    /// Check if soft-stop is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get remaining ramp time
    pub fn remaining_time(&self) -> Option<Duration> {
        if !self.active {
            return None;
        }

        let Some(start_time) = self.start_time else {
            return None;
        };

        let elapsed = start_time.elapsed();
        if elapsed >= self.ramp_duration {
            None
        } else {
            Some(self.ramp_duration - elapsed)
        }
    }

    /// Force stop the ramp
    pub fn force_stop(&mut self) {
        self.active = false;
        self.current_torque = 0.0;
    }
}

/// Audible alert types for fault conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioAlert {
    /// Single beep for minor faults
    SingleBeep,
    /// Double beep for moderate faults
    DoubleBeep,
    /// Continuous beep for critical faults
    ContinuousBeep,
    /// Custom pattern
    CustomPattern(u32),
}

/// Blackbox fault marker for post-mortem analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultMarker {
    pub fault_type: FaultType,
    #[serde(with = "instant_serde")]
    pub timestamp: Instant,
    pub pre_fault_data_offset: u64,
    pub post_fault_data_length: u64,
    pub device_state: HashMap<String, String>,
    pub telemetry_snapshot: Option<Vec<u8>>,
    pub plugin_states: HashMap<String, String>,
    pub recovery_actions: Vec<String>,
}

// Serde module for Instant serialization
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

/// FMEA system for comprehensive fault management
pub struct FmeaSystem {
    thresholds: FaultThresholds,
    fmea_matrix: HashMap<FaultType, FmeaEntry>,
    detection_states: HashMap<FaultType, FaultDetectionState>,
    soft_stop: SoftStopController,
    fault_markers: Vec<FaultMarker>,
    quarantined_plugins: HashMap<String, Instant>,
    audio_alerts_enabled: bool,
}

impl FmeaSystem {
    /// Create new FMEA system with default configuration
    pub fn new() -> Self {
        let mut system = Self {
            thresholds: FaultThresholds::default(),
            fmea_matrix: HashMap::new(),
            detection_states: HashMap::new(),
            soft_stop: SoftStopController::new(),
            fault_markers: Vec::new(),
            quarantined_plugins: HashMap::new(),
            audio_alerts_enabled: true,
        };

        system.initialize_fmea_matrix();
        system
    }

    /// Initialize the FMEA matrix with default entries
    fn initialize_fmea_matrix(&mut self) {
        // USB Stall fault
        self.fmea_matrix.insert(
            FaultType::UsbStall,
            FmeaEntry {
                fault_type: FaultType::UsbStall,
                detection_method: "USB write timeout or consecutive failures".to_string(),
                action: FaultAction::SoftStop,
                max_response_time_ms: 50,
                post_mortem: PostMortemConfig::default(),
                recovery_procedure: "Retry with exponential backoff, reconnect if needed"
                    .to_string(),
                enabled: true,
            },
        );

        // Encoder NaN fault
        self.fmea_matrix.insert(
            FaultType::EncoderNaN,
            FmeaEntry {
                fault_type: FaultType::EncoderNaN,
                detection_method: "NaN or infinite values in encoder data".to_string(),
                action: FaultAction::SoftStop,
                max_response_time_ms: 50,
                post_mortem: PostMortemConfig::default(),
                recovery_procedure: "Recalibrate encoder, use last known good value".to_string(),
                enabled: true,
            },
        );

        // Thermal limit fault
        self.fmea_matrix.insert(
            FaultType::ThermalLimit,
            FmeaEntry {
                fault_type: FaultType::ThermalLimit,
                detection_method: "Temperature sensor reading above threshold".to_string(),
                action: FaultAction::SoftStop,
                max_response_time_ms: 50,
                post_mortem: PostMortemConfig::default(),
                recovery_procedure: "Reduce torque, wait for cooldown with hysteresis".to_string(),
                enabled: true,
            },
        );

        // Plugin overrun fault
        self.fmea_matrix.insert(
            FaultType::PluginOverrun,
            FmeaEntry {
                fault_type: FaultType::PluginOverrun,
                detection_method: "Plugin execution time exceeds budget".to_string(),
                action: FaultAction::Quarantine,
                max_response_time_ms: 1, // Immediate
                post_mortem: PostMortemConfig::default(),
                recovery_procedure: "Quarantine plugin, continue engine operation".to_string(),
                enabled: true,
            },
        );

        // Timing violation fault
        self.fmea_matrix.insert(
            FaultType::TimingViolation,
            FmeaEntry {
                fault_type: FaultType::TimingViolation,
                detection_method: "RT loop jitter exceeds threshold".to_string(),
                action: FaultAction::LogAndContinue,
                max_response_time_ms: 1,
                post_mortem: PostMortemConfig::default(),
                recovery_procedure: "Log violation, adjust RT priority if needed".to_string(),
                enabled: true,
            },
        );

        // Overcurrent fault
        self.fmea_matrix.insert(
            FaultType::Overcurrent,
            FmeaEntry {
                fault_type: FaultType::Overcurrent,
                detection_method: "Current sensor reading above safe threshold".to_string(),
                action: FaultAction::SoftStop,
                max_response_time_ms: 10, // Very fast for safety
                post_mortem: PostMortemConfig::default(),
                recovery_procedure: "Immediate torque cutoff, check for hardware issues"
                    .to_string(),
                enabled: true,
            },
        );

        // Initialize detection states
        for fault_type in self.fmea_matrix.keys() {
            self.detection_states
                .insert(*fault_type, FaultDetectionState::default());
        }
    }

    /// Detect USB communication faults
    pub fn detect_usb_fault(
        &mut self,
        consecutive_failures: u32,
        last_success: Option<Instant>,
    ) -> Option<FaultType> {
        let state = self.detection_states.get_mut(&FaultType::UsbStall)?;

        state.consecutive_count = consecutive_failures;

        // Check timeout
        if let Some(last_success_time) = last_success {
            let timeout_threshold = Duration::from_millis(self.thresholds.usb_timeout_ms);
            if last_success_time.elapsed() > timeout_threshold {
                return Some(FaultType::UsbStall);
            }
        }

        // Check consecutive failures
        if consecutive_failures >= self.thresholds.usb_max_consecutive_failures {
            return Some(FaultType::UsbStall);
        }

        None
    }

    /// Detect encoder NaN faults
    pub fn detect_encoder_fault(&mut self, encoder_value: f32) -> Option<FaultType> {
        if !encoder_value.is_finite() {
            let state = self.detection_states.get_mut(&FaultType::EncoderNaN)?;
            let now = Instant::now();

            // Initialize or reset window
            if state.window_start.is_none()
                || now.duration_since(state.window_start.unwrap())
                    > Duration::from_millis(self.thresholds.encoder_nan_window_ms)
            {
                state.window_start = Some(now);
                state.window_count = 1;
            } else {
                state.window_count += 1;
            }

            state.last_occurrence = Some(now);

            // Check if we've exceeded the threshold
            if state.window_count >= self.thresholds.encoder_max_nan_count {
                return Some(FaultType::EncoderNaN);
            }
        }

        None
    }

    /// Detect thermal faults
    pub fn detect_thermal_fault(
        &mut self,
        temperature_celsius: f32,
        current_fault_active: bool,
    ) -> Option<FaultType> {
        // Use hysteresis for thermal protection
        let threshold = if current_fault_active {
            self.thresholds.thermal_limit_celsius - self.thresholds.thermal_hysteresis_celsius
        } else {
            self.thresholds.thermal_limit_celsius
        };

        if temperature_celsius > threshold && !current_fault_active {
            Some(FaultType::ThermalLimit)
        } else if temperature_celsius <= threshold && current_fault_active {
            // Thermal fault can be cleared
            None
        } else {
            None
        }
    }

    /// Detect plugin overrun faults
    pub fn detect_plugin_overrun(
        &mut self,
        plugin_id: &str,
        execution_time_us: u64,
    ) -> Option<FaultType> {
        if execution_time_us > self.thresholds.plugin_timeout_us {
            let state = self.detection_states.get_mut(&FaultType::PluginOverrun)?;
            state.consecutive_count += 1;
            state.last_occurrence = Some(Instant::now());

            if state.consecutive_count >= self.thresholds.plugin_max_overruns {
                // Quarantine the plugin
                self.quarantined_plugins.insert(
                    plugin_id.to_string(),
                    Instant::now() + Duration::from_secs(300), // 5 minute quarantine
                );
                return Some(FaultType::PluginOverrun);
            }
        }

        None
    }

    /// Detect timing violation faults
    pub fn detect_timing_violation(&mut self, jitter_us: u64) -> Option<FaultType> {
        if jitter_us > self.thresholds.timing_violation_threshold_us {
            let state = self.detection_states.get_mut(&FaultType::TimingViolation)?;
            state.consecutive_count += 1;
            state.last_occurrence = Some(Instant::now());

            if state.consecutive_count >= self.thresholds.timing_max_violations {
                return Some(FaultType::TimingViolation);
            }
        }

        None
    }

    /// Handle detected fault according to FMEA matrix
    pub fn handle_fault(
        &mut self,
        fault_type: FaultType,
        current_torque: f32,
        safety_service: &mut SafetyService,
    ) -> Result<(), String> {
        let fmea_entry = self
            .fmea_matrix
            .get(&fault_type)
            .ok_or_else(|| format!("No FMEA entry for fault type: {:?}", fault_type))?;

        if !fmea_entry.enabled {
            return Ok(());
        }

        let start_time = Instant::now();

        // Execute fault action
        match fmea_entry.action {
            FaultAction::SoftStop => {
                self.soft_stop.start_soft_stop(current_torque);
                safety_service.report_fault(fault_type);
                self.trigger_audio_alert(AudioAlert::DoubleBeep);
            }
            FaultAction::Quarantine => {
                // Plugin quarantine is handled in detect_plugin_overrun
                self.trigger_audio_alert(AudioAlert::SingleBeep);
            }
            FaultAction::LogAndContinue => {
                // Just log the fault, don't change safety state
                self.trigger_audio_alert(AudioAlert::SingleBeep);
            }
            FaultAction::Restart => {
                // Component restart logic would go here
                self.trigger_audio_alert(AudioAlert::SingleBeep);
            }
            FaultAction::SafeMode => {
                safety_service.report_fault(fault_type);
                self.trigger_audio_alert(AudioAlert::ContinuousBeep);
            }
        }

        // Create fault marker for blackbox
        let fault_marker = FaultMarker {
            fault_type,
            timestamp: start_time,
            pre_fault_data_offset: 0, // Would be calculated by blackbox system
            post_fault_data_length: 0, // Would be calculated by blackbox system
            device_state: HashMap::new(), // Would be populated with actual device state
            telemetry_snapshot: None, // Would be populated with telemetry data
            plugin_states: HashMap::new(), // Would be populated with plugin states
            recovery_actions: vec![fmea_entry.recovery_procedure.clone()],
        };

        self.fault_markers.push(fault_marker);

        // Check response time
        let response_time = start_time.elapsed();
        if response_time.as_millis() as u64 > fmea_entry.max_response_time_ms {
            eprintln!(
                "WARNING: Fault response time exceeded: {}ms > {}ms",
                response_time.as_millis(),
                fmea_entry.max_response_time_ms
            );
        }

        Ok(())
    }

    /// Update soft-stop controller and return current torque multiplier
    pub fn update_soft_stop(&mut self) -> f32 {
        self.soft_stop.update()
    }

    /// Check if soft-stop is active
    pub fn is_soft_stop_active(&self) -> bool {
        self.soft_stop.is_active()
    }

    /// Get remaining soft-stop time
    pub fn soft_stop_remaining_time(&self) -> Option<Duration> {
        self.soft_stop.remaining_time()
    }

    /// Force stop soft-stop mechanism
    pub fn force_stop_soft_stop(&mut self) {
        self.soft_stop.force_stop();
    }

    /// Check if plugin is quarantined
    pub fn is_plugin_quarantined(&self, plugin_id: &str) -> bool {
        if let Some(quarantine_until) = self.quarantined_plugins.get(plugin_id) {
            Instant::now() < *quarantine_until
        } else {
            false
        }
    }

    /// Remove plugin from quarantine
    pub fn release_plugin_quarantine(&mut self, plugin_id: &str) {
        self.quarantined_plugins.remove(plugin_id);
    }

    /// Get quarantined plugins
    pub fn get_quarantined_plugins(&self) -> Vec<(String, Duration)> {
        let now = Instant::now();
        self.quarantined_plugins
            .iter()
            .filter_map(|(plugin_id, quarantine_until)| {
                if now < *quarantine_until {
                    Some((plugin_id.clone(), *quarantine_until - now))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Trigger audio alert
    fn trigger_audio_alert(&self, alert: AudioAlert) {
        if !self.audio_alerts_enabled {
            return;
        }

        // In a real implementation, this would interface with the audio system
        match alert {
            AudioAlert::SingleBeep => {
                eprintln!("AUDIO: Single beep");
            }
            AudioAlert::DoubleBeep => {
                eprintln!("AUDIO: Double beep");
            }
            AudioAlert::ContinuousBeep => {
                eprintln!("AUDIO: Continuous beep");
            }
            AudioAlert::CustomPattern(pattern) => {
                eprintln!("AUDIO: Custom pattern {}", pattern);
            }
        }
    }

    /// Get fault markers for blackbox analysis
    pub fn get_fault_markers(&self) -> &[FaultMarker] {
        &self.fault_markers
    }

    /// Clear old fault markers
    pub fn clear_old_fault_markers(&mut self, older_than: Duration) {
        let cutoff = Instant::now() - older_than;
        self.fault_markers
            .retain(|marker| marker.timestamp > cutoff);
    }

    /// Get FMEA configuration
    pub fn get_fmea_matrix(&self) -> &HashMap<FaultType, FmeaEntry> {
        &self.fmea_matrix
    }

    /// Update FMEA entry
    pub fn update_fmea_entry(&mut self, fault_type: FaultType, entry: FmeaEntry) {
        self.fmea_matrix.insert(fault_type, entry);
    }

    /// Get fault detection statistics
    pub fn get_fault_statistics(&self) -> HashMap<FaultType, (u32, Option<Instant>)> {
        self.detection_states
            .iter()
            .map(|(fault_type, state)| {
                (
                    *fault_type,
                    (state.consecutive_count, state.last_occurrence),
                )
            })
            .collect()
    }

    /// Reset fault detection state for a specific fault type
    pub fn reset_fault_detection(&mut self, fault_type: FaultType) {
        if let Some(state) = self.detection_states.get_mut(&fault_type) {
            *state = FaultDetectionState::default();
        }
    }

    /// Enable or disable audio alerts
    pub fn set_audio_alerts_enabled(&mut self, enabled: bool) {
        self.audio_alerts_enabled = enabled;
    }

    /// Get current thresholds
    pub fn get_thresholds(&self) -> &FaultThresholds {
        &self.thresholds
    }

    /// Update thresholds
    pub fn update_thresholds(&mut self, thresholds: FaultThresholds) {
        self.thresholds = thresholds;
    }
}

impl Default for FmeaSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmea_system_initialization() {
        let fmea = FmeaSystem::new();

        // Should have entries for all fault types
        assert!(fmea.fmea_matrix.contains_key(&FaultType::UsbStall));
        assert!(fmea.fmea_matrix.contains_key(&FaultType::EncoderNaN));
        assert!(fmea.fmea_matrix.contains_key(&FaultType::ThermalLimit));
        assert!(fmea.fmea_matrix.contains_key(&FaultType::PluginOverrun));

        // Should have detection states
        assert!(fmea.detection_states.contains_key(&FaultType::UsbStall));
        assert!(fmea.detection_states.contains_key(&FaultType::EncoderNaN));
    }

    #[test]
    fn test_soft_stop_controller() {
        let mut controller = SoftStopController::new();

        assert!(!controller.is_active());
        assert_eq!(controller.update(), 0.0);

        // Start soft stop from 10.0 Nm
        controller.start_soft_stop(10.0);
        assert!(controller.is_active());

        // Should ramp down over time
        let initial_torque = controller.update();
        assert!(initial_torque > 0.0);
        assert!(initial_torque <= 10.0);

        // Wait for ramp to complete
        std::thread::sleep(Duration::from_millis(60));
        let final_torque = controller.update();
        assert_eq!(final_torque, 0.0);
        assert!(!controller.is_active());
    }

    #[test]
    fn test_usb_fault_detection() {
        let mut fmea = FmeaSystem::new();

        // No fault initially
        assert!(fmea.detect_usb_fault(0, Some(Instant::now())).is_none());

        // Consecutive failures
        assert!(fmea.detect_usb_fault(1, Some(Instant::now())).is_none());
        assert!(fmea.detect_usb_fault(2, Some(Instant::now())).is_none());
        assert_eq!(
            fmea.detect_usb_fault(3, Some(Instant::now())),
            Some(FaultType::UsbStall)
        );

        // Timeout
        let old_time = Instant::now() - Duration::from_millis(20);
        assert_eq!(
            fmea.detect_usb_fault(0, Some(old_time)),
            Some(FaultType::UsbStall)
        );
    }

    #[test]
    fn test_encoder_fault_detection() {
        let mut fmea = FmeaSystem::new();

        // Normal values should not trigger fault
        assert!(fmea.detect_encoder_fault(1.5).is_none());
        assert!(fmea.detect_encoder_fault(-2.3).is_none());
        assert!(fmea.detect_encoder_fault(0.0).is_none());

        // Single NaN should not trigger fault
        assert!(fmea.detect_encoder_fault(f32::NAN).is_none());
        assert!(fmea.detect_encoder_fault(f32::INFINITY).is_none());

        // Multiple NaNs in window should trigger fault
        for _ in 0..5 {
            let result = fmea.detect_encoder_fault(f32::NAN);
            if result.is_some() {
                assert_eq!(result, Some(FaultType::EncoderNaN));
                break;
            }
        }
    }

    #[test]
    fn test_thermal_fault_detection() {
        let mut fmea = FmeaSystem::new();

        // Normal temperature
        assert!(fmea.detect_thermal_fault(70.0, false).is_none());

        // Over threshold
        assert_eq!(
            fmea.detect_thermal_fault(85.0, false),
            Some(FaultType::ThermalLimit)
        );

        // Hysteresis - should not clear immediately
        assert!(fmea.detect_thermal_fault(79.0, true).is_some());

        // Below hysteresis threshold - should clear
        assert!(fmea.detect_thermal_fault(74.0, true).is_none());
    }

    #[test]
    fn test_plugin_quarantine() {
        let mut fmea = FmeaSystem::new();

        assert!(!fmea.is_plugin_quarantined("test_plugin"));

        // Trigger overruns
        for i in 0..10 {
            let result = fmea.detect_plugin_overrun("test_plugin", 150); // Over 100us threshold
            if i == 9 {
                assert_eq!(result, Some(FaultType::PluginOverrun));
            }
        }

        assert!(fmea.is_plugin_quarantined("test_plugin"));

        // Release quarantine
        fmea.release_plugin_quarantine("test_plugin");
        assert!(!fmea.is_plugin_quarantined("test_plugin"));
    }

    #[test]
    fn test_fault_handling() {
        let mut fmea = FmeaSystem::new();
        let mut safety_service = SafetyService::default();

        // Handle USB stall fault
        fmea.handle_fault(FaultType::UsbStall, 10.0, &mut safety_service)
            .unwrap();

        // Should trigger soft stop
        assert!(fmea.is_soft_stop_active());

        // Should create fault marker
        assert_eq!(fmea.fault_markers.len(), 1);
        assert_eq!(fmea.fault_markers[0].fault_type, FaultType::UsbStall);

        // Safety service should be faulted
        assert!(matches!(
            safety_service.state(),
            crate::safety::SafetyState::Faulted { .. }
        ));
    }

    #[test]
    fn test_timing_violation_detection() {
        let mut fmea = FmeaSystem::new();

        // Normal jitter
        assert!(fmea.detect_timing_violation(100).is_none());

        // High jitter but not enough violations
        for _ in 0..99 {
            assert!(fmea.detect_timing_violation(300).is_none());
        }

        // 100th violation should trigger fault
        assert_eq!(
            fmea.detect_timing_violation(300),
            Some(FaultType::TimingViolation)
        );
    }

    #[test]
    fn test_fault_statistics() {
        let mut fmea = FmeaSystem::new();

        // Trigger some faults
        fmea.detect_usb_fault(2, Some(Instant::now()));
        fmea.detect_encoder_fault(f32::NAN);

        let stats = fmea.get_fault_statistics();
        assert_eq!(stats[&FaultType::UsbStall].0, 2); // 2 consecutive failures
        assert!(stats[&FaultType::EncoderNaN].1.is_some()); // Has last occurrence
    }

    #[test]
    fn test_fault_marker_cleanup() {
        let mut fmea = FmeaSystem::new();
        let mut safety_service = SafetyService::default();

        // Create some fault markers
        fmea.handle_fault(FaultType::UsbStall, 5.0, &mut safety_service)
            .unwrap();
        fmea.handle_fault(FaultType::ThermalLimit, 3.0, &mut safety_service)
            .unwrap();

        assert_eq!(fmea.fault_markers.len(), 2);

        // Clear old markers (none should be cleared since they're recent)
        fmea.clear_old_fault_markers(Duration::from_secs(1));
        assert_eq!(fmea.fault_markers.len(), 2);

        // Clear all markers
        fmea.clear_old_fault_markers(Duration::from_millis(1));
        assert_eq!(fmea.fault_markers.len(), 0);
    }
}
