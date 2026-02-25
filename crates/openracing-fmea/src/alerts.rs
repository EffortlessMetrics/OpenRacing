//! Audio alerts for fault conditions.
//!
//! Provides audio feedback for fault detection and system status.
//! Alert types indicate severity and recommended user action.

use crate::FaultType;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Audio alert types for fault conditions.
///
/// Each alert type indicates a different severity level and
/// recommended user action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum AudioAlert {
    /// Single beep for minor faults or informational alerts.
    SingleBeep,
    /// Double beep for moderate faults requiring attention.
    DoubleBeep,
    /// Triple beep for significant faults.
    TripleBeep,
    /// Continuous beep for critical faults requiring immediate action.
    ContinuousBeep,
    /// Custom pattern with specified number of beeps.
    CustomPattern(u8),
    /// Urgent alert - rapid beeping.
    Urgent,
    /// Warning alert - ascending tones.
    Warning,
    /// Success alert - descending tones.
    Success,
    /// System startup sound.
    Startup,
    /// System shutdown sound.
    Shutdown,
}

impl AudioAlert {
    /// Get the severity level of this alert (1-5, where 5 is most severe).
    pub fn severity(&self) -> u8 {
        match self {
            AudioAlert::SingleBeep => 1,
            AudioAlert::DoubleBeep => 2,
            AudioAlert::TripleBeep => 3,
            AudioAlert::CustomPattern(count) => (*count).clamp(1, 5),
            AudioAlert::ContinuousBeep => 5,
            AudioAlert::Urgent => 5,
            AudioAlert::Warning => 3,
            AudioAlert::Success => 1,
            AudioAlert::Startup => 1,
            AudioAlert::Shutdown => 2,
        }
    }

    /// Get the number of beeps for this alert.
    pub fn beep_count(&self) -> u8 {
        match self {
            AudioAlert::SingleBeep => 1,
            AudioAlert::DoubleBeep => 2,
            AudioAlert::TripleBeep => 3,
            AudioAlert::CustomPattern(count) => *count,
            AudioAlert::ContinuousBeep => 0, // Continuous has no count
            AudioAlert::Urgent => 5,
            AudioAlert::Warning => 3,
            AudioAlert::Success => 2,
            AudioAlert::Startup => 2,
            AudioAlert::Shutdown => 3,
        }
    }

    /// Check if this alert is continuous (not a fixed pattern).
    pub fn is_continuous(&self) -> bool {
        matches!(self, AudioAlert::ContinuousBeep | AudioAlert::Urgent)
    }

    /// Get the recommended alert for a fault type.
    pub fn for_fault_type(fault_type: FaultType) -> Self {
        match fault_type {
            FaultType::Overcurrent => AudioAlert::Urgent,
            FaultType::ThermalLimit => AudioAlert::ContinuousBeep,
            FaultType::UsbStall => AudioAlert::DoubleBeep,
            FaultType::EncoderNaN => AudioAlert::DoubleBeep,
            FaultType::SafetyInterlockViolation => AudioAlert::ContinuousBeep,
            FaultType::HandsOffTimeout => AudioAlert::TripleBeep,
            FaultType::PluginOverrun => AudioAlert::SingleBeep,
            FaultType::TimingViolation => AudioAlert::SingleBeep,
            FaultType::PipelineFault => AudioAlert::DoubleBeep,
        }
    }
}

/// Audio alert system for fault notifications.
///
/// Manages alert generation, throttling, and prioritization.
#[derive(Debug, Clone)]
pub struct AudioAlertSystem {
    /// Whether audio alerts are enabled.
    enabled: bool,
    /// Minimum time between alerts in milliseconds.
    min_interval_ms: u64,
    /// Time since last alert.
    last_alert_time: Option<u64>,
    /// Current active alert (if any).
    active_alert: Option<AudioAlert>,
    /// Alert queue for pending alerts.
    pending_alerts: heapless::Vec<AudioAlert, 8>,
}

impl Default for AudioAlertSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioAlertSystem {
    /// Create a new audio alert system.
    pub fn new() -> Self {
        Self {
            enabled: true,
            min_interval_ms: 500,
            last_alert_time: None,
            active_alert: None,
            pending_alerts: heapless::Vec::new(),
        }
    }

    /// Create a new audio alert system with custom settings.
    pub fn with_settings(enabled: bool, min_interval_ms: u64) -> Self {
        Self {
            enabled,
            min_interval_ms,
            last_alert_time: None,
            active_alert: None,
            pending_alerts: heapless::Vec::new(),
        }
    }

    /// Enable or disable audio alerts.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if audio alerts are enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set the minimum interval between alerts.
    pub fn set_min_interval(&mut self, interval_ms: u64) {
        self.min_interval_ms = interval_ms;
    }

    /// Trigger an audio alert.
    ///
    /// Returns `true` if the alert was triggered immediately,
    /// or `false` if it was queued or suppressed.
    pub fn trigger(&mut self, alert: AudioAlert, current_time_ms: u64) -> bool {
        if !self.enabled {
            return false;
        }

        // Check if we can play immediately
        let can_play = match self.last_alert_time {
            None => true,
            Some(last) => current_time_ms.saturating_sub(last) >= self.min_interval_ms,
        };

        if can_play {
            self.active_alert = Some(alert);
            self.last_alert_time = Some(current_time_ms);
            return true;
        }

        // Queue the alert if it's higher priority than existing pending alerts
        self.queue_alert(alert)
    }

    /// Queue an alert if it's higher priority than existing pending alerts.
    fn queue_alert(&mut self, alert: AudioAlert) -> bool {
        // Don't queue if we already have a higher or equal priority alert
        let alert_severity = alert.severity();

        for pending in &self.pending_alerts {
            if pending.severity() >= alert_severity {
                // Already have equal or higher priority alert, suppress this one
                return false;
            }
        }

        // Remove lower priority alerts
        self.pending_alerts
            .retain(|a| a.severity() > alert_severity);

        // Add the new alert - return false to indicate "queued, not played"
        let _ = self.pending_alerts.push(alert);
        false
    }

    /// Update the alert system with current time.
    ///
    /// Returns the current active alert (if any) after updating state.
    pub fn update(&mut self, current_time_ms: u64) -> Option<AudioAlert> {
        // Check if current alert is complete
        if let Some(alert) = self.active_alert {
            if alert.is_continuous() {
                // Continuous alerts stay active until explicitly stopped
                return Some(alert);
            }

            // Non-continuous alerts complete after min_interval
            if let Some(last) = self.last_alert_time
                && current_time_ms.saturating_sub(last) >= self.min_interval_ms
            {
                self.active_alert = None;
            }
        }

        // If no active alert and we have pending alerts, start the highest priority
        if self.active_alert.is_none() && !self.pending_alerts.is_empty() {
            // Find highest priority alert
            let mut highest_idx = 0;
            let mut highest_severity = 0;

            for (idx, alert) in self.pending_alerts.iter().enumerate() {
                if alert.severity() > highest_severity {
                    highest_severity = alert.severity();
                    highest_idx = idx;
                }
            }

            if self.pending_alerts.len() > highest_idx {
                let alert = self.pending_alerts.swap_remove(highest_idx);
                self.active_alert = Some(alert);
                self.last_alert_time = Some(current_time_ms);
            }
        }

        self.active_alert
    }

    /// Stop the current alert.
    pub fn stop(&mut self) {
        self.active_alert = None;
    }

    /// Stop the current alert and clear pending alerts.
    pub fn stop_all(&mut self) {
        self.active_alert = None;
        self.pending_alerts.clear();
    }

    /// Check if an alert is currently active.
    pub fn is_alert_active(&self) -> bool {
        self.active_alert.is_some()
    }

    /// Get the current active alert.
    pub fn current_alert(&self) -> Option<AudioAlert> {
        self.active_alert
    }

    /// Get the number of pending alerts.
    pub fn pending_count(&self) -> usize {
        self.pending_alerts.len()
    }

    /// Clear all pending alerts.
    pub fn clear_pending(&mut self) {
        self.pending_alerts.clear();
    }

    /// Trigger an alert for a specific fault type.
    pub fn trigger_for_fault(&mut self, fault_type: FaultType, current_time_ms: u64) -> bool {
        let alert = AudioAlert::for_fault_type(fault_type);
        self.trigger(alert, current_time_ms)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn test_audio_alert_severity() {
        assert_eq!(AudioAlert::SingleBeep.severity(), 1);
        assert_eq!(AudioAlert::DoubleBeep.severity(), 2);
        assert_eq!(AudioAlert::TripleBeep.severity(), 3);
        assert_eq!(AudioAlert::ContinuousBeep.severity(), 5);
        assert_eq!(AudioAlert::Urgent.severity(), 5);
    }

    #[test]
    fn test_audio_alert_beep_count() {
        assert_eq!(AudioAlert::SingleBeep.beep_count(), 1);
        assert_eq!(AudioAlert::DoubleBeep.beep_count(), 2);
        assert_eq!(AudioAlert::TripleBeep.beep_count(), 3);
        assert_eq!(AudioAlert::CustomPattern(4).beep_count(), 4);
    }

    #[test]
    fn test_audio_alert_continuous() {
        assert!(AudioAlert::ContinuousBeep.is_continuous());
        assert!(AudioAlert::Urgent.is_continuous());
        assert!(!AudioAlert::SingleBeep.is_continuous());
        assert!(!AudioAlert::DoubleBeep.is_continuous());
    }

    #[test]
    fn test_audio_alert_for_fault_type() {
        assert_eq!(
            AudioAlert::for_fault_type(FaultType::Overcurrent),
            AudioAlert::Urgent
        );
        assert_eq!(
            AudioAlert::for_fault_type(FaultType::ThermalLimit),
            AudioAlert::ContinuousBeep
        );
        assert_eq!(
            AudioAlert::for_fault_type(FaultType::PluginOverrun),
            AudioAlert::SingleBeep
        );
    }

    #[test]
    fn test_audio_alert_system_creation() {
        let system = AudioAlertSystem::new();
        assert!(system.is_enabled());
        assert!(!system.is_alert_active());
        assert_eq!(system.pending_count(), 0);
    }

    #[test]
    fn test_audio_alert_system_trigger() {
        let mut system = AudioAlertSystem::new();

        let triggered = system.trigger(AudioAlert::SingleBeep, 0);
        assert!(triggered);
        assert!(system.is_alert_active());
        assert_eq!(system.current_alert(), Some(AudioAlert::SingleBeep));
    }

    #[test]
    fn test_audio_alert_system_disabled() {
        let mut system = AudioAlertSystem::new();
        system.set_enabled(false);

        let triggered = system.trigger(AudioAlert::SingleBeep, 0);
        assert!(!triggered);
        assert!(!system.is_alert_active());
    }

    #[test]
    fn test_audio_alert_system_throttling() {
        let mut system = AudioAlertSystem::with_settings(true, 100);

        // First alert triggers
        let triggered = system.trigger(AudioAlert::SingleBeep, 0);
        assert!(triggered);

        // Second alert too soon, should queue
        let triggered = system.trigger(AudioAlert::DoubleBeep, 50);
        assert!(!triggered);
        assert_eq!(system.pending_count(), 1);

        // Update to process queue after interval
        system.stop();
        let alert = system.update(150);
        assert!(alert.is_some());
    }

    #[test]
    fn test_audio_alert_system_stop() {
        let mut system = AudioAlertSystem::new();
        system.trigger(AudioAlert::ContinuousBeep, 0);

        assert!(system.is_alert_active());
        system.stop();
        assert!(!system.is_alert_active());
    }

    #[test]
    fn test_audio_alert_system_stop_all() {
        let mut system = AudioAlertSystem::new();
        system.trigger(AudioAlert::SingleBeep, 0);
        system.trigger(AudioAlert::DoubleBeep, 50); // Queued

        system.stop_all();
        assert!(!system.is_alert_active());
        assert_eq!(system.pending_count(), 0);
    }

    #[test]
    fn test_audio_alert_system_fault_trigger() {
        let mut system = AudioAlertSystem::new();

        let triggered = system.trigger_for_fault(FaultType::Overcurrent, 0);
        assert!(triggered);
        assert_eq!(system.current_alert(), Some(AudioAlert::Urgent));
    }
}
