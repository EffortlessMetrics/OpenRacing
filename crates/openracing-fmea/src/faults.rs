//! Fault types, thresholds, and detection mechanisms.

use core::fmt;
use core::time::Duration;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Types of faults that can occur in the FFB system.
///
/// Each fault type has specific detection criteria and recovery procedures
/// defined in the FMEA matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum FaultType {
    /// USB communication has stalled (timeout or consecutive failures)
    UsbStall,
    /// Encoder returned NaN or infinite value
    EncoderNaN,
    /// Temperature exceeded safe operating limit
    ThermalLimit,
    /// Current exceeded safe threshold
    Overcurrent,
    /// Plugin execution exceeded timing budget
    PluginOverrun,
    /// Real-time timing constraint violated (jitter)
    TimingViolation,
    /// Safety interlock protocol violated
    SafetyInterlockViolation,
    /// Hands-off timeout exceeded during high-torque operation
    HandsOffTimeout,
    /// Filter pipeline processing error
    PipelineFault,
}

impl fmt::Display for FaultType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FaultType::UsbStall => write!(f, "USB communication stall"),
            FaultType::EncoderNaN => write!(f, "Encoder returned invalid data"),
            FaultType::ThermalLimit => write!(f, "Thermal protection triggered"),
            FaultType::Overcurrent => write!(f, "Overcurrent protection triggered"),
            FaultType::PluginOverrun => write!(f, "Plugin exceeded timing budget"),
            FaultType::TimingViolation => write!(f, "Real-time timing violation"),
            FaultType::SafetyInterlockViolation => write!(f, "Safety interlock violation"),
            FaultType::HandsOffTimeout => write!(f, "Hands-off timeout exceeded"),
            FaultType::PipelineFault => write!(f, "Filter pipeline processing fault"),
        }
    }
}

impl FaultType {
    /// Returns the severity level of this fault type.
    ///
    /// Severity levels:
    /// - 1: Critical - requires immediate action
    /// - 2: High - requires prompt action
    /// - 3: Medium - can be logged and monitored
    /// - 4: Low - informational only
    pub fn severity(&self) -> u8 {
        match self {
            FaultType::Overcurrent => 1,
            FaultType::ThermalLimit => 1,
            FaultType::UsbStall => 2,
            FaultType::EncoderNaN => 2,
            FaultType::SafetyInterlockViolation => 2,
            FaultType::HandsOffTimeout => 2,
            FaultType::PluginOverrun => 3,
            FaultType::TimingViolation => 3,
            FaultType::PipelineFault => 3,
        }
    }

    /// Returns true if this fault type requires immediate torque reduction.
    pub fn requires_immediate_response(&self) -> bool {
        matches!(
            self,
            FaultType::Overcurrent
                | FaultType::ThermalLimit
                | FaultType::UsbStall
                | FaultType::EncoderNaN
                | FaultType::SafetyInterlockViolation
                | FaultType::HandsOffTimeout
        )
    }

    /// Returns true if this fault can be automatically recovered.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            FaultType::UsbStall
                | FaultType::ThermalLimit
                | FaultType::PluginOverrun
                | FaultType::TimingViolation
                | FaultType::PipelineFault
        )
    }

    /// Returns the default maximum response time in milliseconds.
    pub fn default_max_response_time_ms(&self) -> u64 {
        match self {
            FaultType::Overcurrent => 10,
            FaultType::ThermalLimit => 50,
            FaultType::UsbStall => 50,
            FaultType::EncoderNaN => 50,
            FaultType::SafetyInterlockViolation => 10,
            FaultType::HandsOffTimeout => 50,
            FaultType::PluginOverrun => 1,
            FaultType::TimingViolation => 1,
            FaultType::PipelineFault => 10,
        }
    }
}

/// Fault detection thresholds and configuration.
///
/// These thresholds define when faults are triggered based on monitored parameters.
/// All thresholds are designed to provide early warning while avoiding false positives.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FaultThresholds {
    /// USB communication timeout threshold in milliseconds.
    pub usb_timeout_ms: u64,
    /// Maximum consecutive USB failures before fault is triggered.
    pub usb_max_consecutive_failures: u32,
    /// Encoder NaN detection window in milliseconds.
    pub encoder_nan_window_ms: u64,
    /// Maximum encoder NaN count allowed in the detection window.
    pub encoder_max_nan_count: u32,
    /// Thermal protection threshold in Celsius.
    pub thermal_limit_celsius: f32,
    /// Thermal hysteresis for recovery (temperature must drop this much below limit).
    pub thermal_hysteresis_celsius: f32,
    /// Plugin execution timeout per tick in microseconds.
    pub plugin_timeout_us: u64,
    /// Maximum plugin overruns before quarantine.
    pub plugin_max_overruns: u32,
    /// Timing violation threshold (jitter) in microseconds.
    pub timing_violation_threshold_us: u64,
    /// Maximum timing violations before fault is triggered.
    pub timing_max_violations: u32,
    /// Maximum current in Amperes before overcurrent fault.
    pub overcurrent_limit_a: f32,
    /// Hands-off timeout in seconds during high-torque operation.
    pub hands_off_timeout_secs: f32,
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
            overcurrent_limit_a: 10.0,
            hands_off_timeout_secs: 5.0,
        }
    }
}

impl FaultThresholds {
    /// Create new thresholds with conservative defaults for development.
    pub fn conservative() -> Self {
        Self {
            usb_timeout_ms: 5,
            usb_max_consecutive_failures: 2,
            encoder_nan_window_ms: 500,
            encoder_max_nan_count: 3,
            thermal_limit_celsius: 70.0,
            thermal_hysteresis_celsius: 10.0,
            plugin_timeout_us: 50,
            plugin_max_overruns: 5,
            timing_violation_threshold_us: 100,
            timing_max_violations: 50,
            overcurrent_limit_a: 8.0,
            hands_off_timeout_secs: 3.0,
        }
    }

    /// Create new thresholds with relaxed settings for testing.
    pub fn relaxed() -> Self {
        Self {
            usb_timeout_ms: 100,
            usb_max_consecutive_failures: 10,
            encoder_nan_window_ms: 5000,
            encoder_max_nan_count: 20,
            thermal_limit_celsius: 90.0,
            thermal_hysteresis_celsius: 2.0,
            plugin_timeout_us: 500,
            plugin_max_overruns: 50,
            timing_violation_threshold_us: 1000,
            timing_max_violations: 500,
            overcurrent_limit_a: 15.0,
            hands_off_timeout_secs: 10.0,
        }
    }

    /// Validate thresholds are within safe operating ranges.
    ///
    /// # Errors
    ///
    /// Returns an error if any threshold is outside safe bounds.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.usb_timeout_ms == 0 {
            return Err("USB timeout must be greater than 0");
        }
        if self.usb_max_consecutive_failures == 0 {
            return Err("USB max consecutive failures must be greater than 0");
        }
        if self.encoder_max_nan_count == 0 {
            return Err("Encoder max NaN count must be greater than 0");
        }
        if self.thermal_limit_celsius < 40.0 || self.thermal_limit_celsius > 120.0 {
            return Err("Thermal limit must be between 40°C and 120°C");
        }
        if self.thermal_hysteresis_celsius < 0.0 {
            return Err("Thermal hysteresis cannot be negative");
        }
        if self.plugin_timeout_us == 0 {
            return Err("Plugin timeout must be greater than 0");
        }
        if self.overcurrent_limit_a <= 0.0 {
            return Err("Overcurrent limit must be positive");
        }
        if self.hands_off_timeout_secs <= 0.0 {
            return Err("Hands-off timeout must be positive");
        }
        Ok(())
    }
}

/// Fault action to take when a fault is detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum FaultAction {
    /// Immediately ramp torque to zero using soft-stop.
    SoftStop,
    /// Quarantine the faulty component and continue.
    Quarantine,
    /// Log the fault and continue operation.
    LogAndContinue,
    /// Attempt to restart the faulty component.
    Restart,
    /// Enter safe mode with reduced functionality.
    SafeMode,
}

impl FaultAction {
    /// Returns true if this action requires torque modification.
    pub fn affects_torque(&self) -> bool {
        matches!(self, FaultAction::SoftStop | FaultAction::SafeMode)
    }

    /// Returns true if this action allows continued operation.
    pub fn allows_operation(&self) -> bool {
        matches!(
            self,
            FaultAction::LogAndContinue | FaultAction::Quarantine | FaultAction::Restart
        )
    }
}

/// Post-mortem data collection configuration.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PostMortemConfig {
    /// Duration to capture before fault (seconds).
    pub pre_fault_capture_duration: f32,
    /// Duration to capture after fault (seconds).
    pub post_fault_capture_duration: f32,
    /// Include telemetry data in capture.
    pub include_telemetry: bool,
    /// Include device state in capture.
    pub include_device_state: bool,
    /// Include plugin state in capture.
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

/// Fault detection state for tracking occurrences and timing.
#[derive(Debug, Clone, Default)]
pub struct FaultDetectionState {
    /// Number of consecutive fault detections.
    pub consecutive_count: u32,
    /// Time of last fault occurrence.
    pub last_occurrence: Option<Duration>,
    /// Start of the detection window.
    pub window_start: Option<Duration>,
    /// Count of faults within the current window.
    pub window_count: u32,
    /// Whether this fault is currently quarantined.
    pub quarantined: bool,
    /// When quarantine expires.
    pub quarantine_until: Option<Duration>,
}

impl FaultDetectionState {
    /// Create a new fault detection state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a fault occurrence.
    pub fn record_fault(&mut self, timestamp: Duration) {
        self.consecutive_count = self.consecutive_count.saturating_add(1);
        self.last_occurrence = Some(timestamp);
    }

    /// Clear the consecutive fault count.
    pub fn clear_consecutive(&mut self) {
        self.consecutive_count = 0;
    }

    /// Update the detection window.
    pub fn update_window(&mut self, timestamp: Duration, window_duration: Duration) -> bool {
        let should_reset = match self.window_start {
            None => true,
            Some(start) => timestamp.saturating_sub(start) > window_duration,
        };

        if should_reset {
            self.window_start = Some(timestamp);
            self.window_count = 1;
        } else {
            self.window_count = self.window_count.saturating_add(1);
        }

        should_reset
    }

    /// Check if currently quarantined.
    pub fn is_quarantined(&self, now: Duration) -> bool {
        match self.quarantine_until {
            Some(until) => now < until,
            None => false,
        }
    }

    /// Set quarantine.
    pub fn set_quarantine(&mut self, duration: Duration, now: Duration) {
        self.quarantined = true;
        self.quarantine_until = Some(now.saturating_add(duration));
    }

    /// Clear quarantine.
    pub fn clear_quarantine(&mut self) {
        self.quarantined = false;
        self.quarantine_until = None;
    }
}

/// Blackbox fault marker for post-mortem analysis.
#[derive(Debug, Clone)]
pub struct FaultMarker {
    /// Type of fault that occurred.
    pub fault_type: FaultType,
    /// Timestamp of fault detection.
    pub timestamp: Duration,
    /// Offset into blackbox data for pre-fault data.
    pub pre_fault_data_offset: u64,
    /// Length of post-fault data capture.
    pub post_fault_data_length: u64,
    /// Device state at time of fault.
    pub device_state: heapless::Vec<(heapless::String<32>, heapless::String<64>), 16>,
    /// Telemetry snapshot at time of fault.
    pub telemetry_snapshot: Option<heapless::Vec<u8, 256>>,
    /// Plugin states at time of fault.
    pub plugin_states: heapless::Vec<(heapless::String<32>, heapless::String<64>), 8>,
    /// Recovery actions taken.
    pub recovery_actions: heapless::Vec<heapless::String<64>, 8>,
}

impl FaultMarker {
    /// Create a new fault marker for the given fault type.
    pub fn new(fault_type: FaultType, timestamp: Duration) -> Self {
        Self {
            fault_type,
            timestamp,
            pre_fault_data_offset: 0,
            post_fault_data_length: 0,
            device_state: heapless::Vec::new(),
            telemetry_snapshot: None,
            plugin_states: heapless::Vec::new(),
            recovery_actions: heapless::Vec::new(),
        }
    }

    /// Add a device state entry.
    ///
    /// Returns `true` if added successfully, `false` if capacity exceeded.
    pub fn add_device_state(&mut self, key: &str, value: &str) -> bool {
        let mut k = heapless::String::new();
        if k.push_str(key).is_err() {
            return false;
        }
        let mut v = heapless::String::new();
        if v.push_str(value).is_err() {
            return false;
        }
        self.device_state.push((k, v)).is_ok()
    }

    /// Add a plugin state entry.
    ///
    /// Returns `true` if added successfully, `false` if capacity exceeded.
    pub fn add_plugin_state(&mut self, plugin_id: &str, state: &str) -> bool {
        let mut k = heapless::String::new();
        if k.push_str(plugin_id).is_err() {
            return false;
        }
        let mut v = heapless::String::new();
        if v.push_str(state).is_err() {
            return false;
        }
        self.plugin_states.push((k, v)).is_ok()
    }

    /// Add a recovery action.
    ///
    /// Returns `true` if added successfully, `false` if capacity exceeded.
    pub fn add_recovery_action(&mut self, action: &str) -> bool {
        let mut s = heapless::String::new();
        if s.push_str(action).is_err() {
            return false;
        }
        self.recovery_actions.push(s).is_ok()
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn test_fault_type_severity() {
        assert_eq!(FaultType::Overcurrent.severity(), 1);
        assert_eq!(FaultType::ThermalLimit.severity(), 1);
        assert_eq!(FaultType::UsbStall.severity(), 2);
        assert_eq!(FaultType::PluginOverrun.severity(), 3);
    }

    #[test]
    fn test_fault_type_requires_immediate_response() {
        assert!(FaultType::Overcurrent.requires_immediate_response());
        assert!(FaultType::ThermalLimit.requires_immediate_response());
        assert!(!FaultType::PluginOverrun.requires_immediate_response());
        assert!(!FaultType::TimingViolation.requires_immediate_response());
    }

    #[test]
    fn test_fault_type_is_recoverable() {
        assert!(FaultType::UsbStall.is_recoverable());
        assert!(FaultType::ThermalLimit.is_recoverable());
        assert!(!FaultType::EncoderNaN.is_recoverable());
        assert!(!FaultType::SafetyInterlockViolation.is_recoverable());
    }

    #[test]
    fn test_fault_thresholds_default() {
        let thresholds = FaultThresholds::default();
        assert!(thresholds.validate().is_ok());
        assert_eq!(thresholds.usb_timeout_ms, 10);
        assert_eq!(thresholds.thermal_limit_celsius, 80.0);
    }

    #[test]
    fn test_fault_thresholds_conservative() {
        let thresholds = FaultThresholds::conservative();
        assert!(thresholds.validate().is_ok());
        assert!(thresholds.thermal_limit_celsius < 80.0);
    }

    #[test]
    fn test_fault_thresholds_relaxed() {
        let thresholds = FaultThresholds::relaxed();
        assert!(thresholds.validate().is_ok());
        assert!(thresholds.thermal_limit_celsius > 80.0);
    }

    #[test]
    fn test_fault_thresholds_validation() {
        let mut thresholds = FaultThresholds::default();
        assert!(thresholds.validate().is_ok());

        thresholds.thermal_limit_celsius = 30.0;
        assert!(thresholds.validate().is_err());

        thresholds.thermal_limit_celsius = 130.0;
        assert!(thresholds.validate().is_err());
    }

    #[test]
    fn test_fault_action_properties() {
        assert!(FaultAction::SoftStop.affects_torque());
        assert!(FaultAction::SafeMode.affects_torque());
        assert!(!FaultAction::LogAndContinue.affects_torque());

        assert!(FaultAction::LogAndContinue.allows_operation());
        assert!(FaultAction::Quarantine.allows_operation());
        assert!(!FaultAction::SoftStop.allows_operation());
    }

    #[test]
    fn test_fault_detection_state() {
        let mut state = FaultDetectionState::new();
        assert_eq!(state.consecutive_count, 0);

        state.record_fault(Duration::from_millis(100));
        assert_eq!(state.consecutive_count, 1);
        assert_eq!(state.last_occurrence, Some(Duration::from_millis(100)));

        state.clear_consecutive();
        assert_eq!(state.consecutive_count, 0);
    }

    #[test]
    fn test_fault_detection_state_window() {
        let mut state = FaultDetectionState::new();
        let window = Duration::from_millis(1000);

        // First fault starts window
        let reset = state.update_window(Duration::from_millis(100), window);
        assert!(reset);
        assert_eq!(state.window_count, 1);

        // Second fault within window
        let reset = state.update_window(Duration::from_millis(200), window);
        assert!(!reset);
        assert_eq!(state.window_count, 2);

        // Fault after window expires
        let reset = state.update_window(Duration::from_millis(1200), window);
        assert!(reset);
        assert_eq!(state.window_count, 1);
    }

    #[test]
    fn test_fault_detection_state_quarantine() {
        let mut state = FaultDetectionState::new();
        let now = Duration::from_secs(0);

        assert!(!state.is_quarantined(now));

        state.set_quarantine(Duration::from_secs(10), now);
        assert!(state.is_quarantined(now));
        assert!(state.is_quarantined(Duration::from_secs(5)));
        assert!(!state.is_quarantined(Duration::from_secs(15)));

        state.clear_quarantine();
        assert!(!state.is_quarantined(Duration::from_secs(5)));
    }
}
