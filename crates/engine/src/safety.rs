//! Safety systems and fault handling

use racing_wheel_schemas::prelude::TorqueNm;
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Safety state machine for torque management
#[derive(Debug, Clone, PartialEq)]
pub enum SafetyState {
    /// Safe torque mode (limited torque)
    SafeTorque,
    /// High torque challenge in progress - waiting for physical button combo
    HighTorqueChallenge {
        challenge_token: u32,
        expires: Instant,
        ui_consent_given: bool,
    },
    /// Waiting for physical button combo acknowledgment from device
    AwaitingPhysicalAck {
        challenge_token: u32,
        expires: Instant,
        combo_start: Option<Instant>,
    },
    /// High torque active
    HighTorqueActive {
        since: Instant,
        device_token: u32,
        last_hands_on: Instant,
    },
    /// Faulted state (torque disabled)
    Faulted { fault: FaultType, since: Instant },
}

/// Types of faults that can occur
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FaultType {
    UsbStall,
    EncoderNaN,
    ThermalLimit,
    Overcurrent,
    PluginOverrun,
    TimingViolation,
    SafetyInterlockViolation,
    HandsOffTimeout,
    PipelineFault,
}

/// Physical button combination for safety interlock
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ButtonCombo {
    /// Both clutch paddles held for 2 seconds
    BothClutchPaddles,
    /// Specific button sequence (implementation dependent)
    CustomSequence(u32),
}

/// Safety interlock challenge state
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InterlockChallenge {
    pub challenge_token: u32,
    pub combo_required: ButtonCombo,
    #[serde(with = "instant_serde")]
    pub expires: Instant,
    pub ui_consent_given: bool,
    #[serde(with = "option_instant_serde")]
    pub combo_start: Option<Instant>,
}

/// Safety interlock acknowledgment from device
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InterlockAck {
    pub challenge_token: u32,
    pub device_token: u32,
    pub combo_completed: ButtonCombo,
    pub timestamp: Instant,
}

/// UI consent requirements for high torque mode
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConsentRequirements {
    pub max_torque_nm: f32,
    pub warnings: Vec<String>,
    pub disclaimers: Vec<String>,
    pub requires_explicit_consent: bool,
}

impl std::fmt::Display for FaultType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

/// Safety service for managing torque limits and fault handling
pub struct SafetyService {
    pub(crate) state: SafetyState,
    max_safe_torque_nm: f32,
    max_high_torque_nm: f32,
    fault_count: HashMap<FaultType, u32>,
    pub(crate) active_challenge: Option<InterlockChallenge>,
    device_tokens: HashMap<String, u32>, // device_id -> token
    hands_off_timeout: Duration,
    combo_hold_duration: Duration,
}

impl SafetyService {
    /// Create new safety service
    pub fn new(max_safe_torque_nm: f32, max_high_torque_nm: f32) -> Self {
        Self {
            state: SafetyState::SafeTorque,
            max_safe_torque_nm,
            max_high_torque_nm,
            fault_count: HashMap::new(),
            active_challenge: None,
            device_tokens: HashMap::new(),
            hands_off_timeout: Duration::from_secs(5),
            combo_hold_duration: Duration::from_secs(2),
        }
    }

    /// Create new safety service with custom timeouts
    pub fn with_timeouts(
        max_safe_torque_nm: f32,
        max_high_torque_nm: f32,
        hands_off_timeout: Duration,
        combo_hold_duration: Duration,
    ) -> Self {
        Self {
            state: SafetyState::SafeTorque,
            max_safe_torque_nm,
            max_high_torque_nm,
            fault_count: HashMap::new(),
            active_challenge: None,
            device_tokens: HashMap::new(),
            hands_off_timeout,
            combo_hold_duration,
        }
    }

    /// Get current safety state
    pub fn state(&self) -> &SafetyState {
        &self.state
    }

    /// Get maximum allowed torque for current state
    pub fn max_torque_nm(&self) -> f32 {
        match &self.state {
            SafetyState::SafeTorque => self.max_safe_torque_nm,
            SafetyState::HighTorqueChallenge { .. } => self.max_safe_torque_nm,
            SafetyState::AwaitingPhysicalAck { .. } => self.max_safe_torque_nm,
            SafetyState::HighTorqueActive { .. } => self.max_high_torque_nm,
            SafetyState::Faulted { .. } => 0.0,
        }
    }

    /// Get the current maximum allowed torque as TorqueNm
    pub fn get_max_torque(&self, is_high_torque_enabled: bool) -> TorqueNm {
        let torque_nm = match &self.state {
            SafetyState::Faulted { .. } => 0.0,
            SafetyState::HighTorqueActive { .. } if is_high_torque_enabled => {
                self.max_high_torque_nm
            }
            _ => self.max_safe_torque_nm,
        };
        TorqueNm::new(torque_nm).expect("torque_nm should be valid")
    }

    /// Get consent requirements for high torque mode
    pub fn get_consent_requirements(&self) -> ConsentRequirements {
        ConsentRequirements {
            max_torque_nm: self.max_high_torque_nm,
            warnings: vec![
                "High torque mode enables forces up to {:.1} Nm".to_string(),
                "Ensure wheel is properly mounted and secure".to_string(),
                "Keep hands on wheel at all times during operation".to_string(),
                "Emergency stop available via physical button combo".to_string(),
            ],
            disclaimers: vec![
                "High torque forces can cause injury if misused".to_string(),
                "User assumes all risk for high torque operation".to_string(),
                "Disable high torque when not actively racing".to_string(),
            ],
            requires_explicit_consent: true,
        }
    }

    /// Check if device has valid high torque token
    pub fn has_valid_token(&self, device_id: &str) -> bool {
        self.device_tokens.contains_key(device_id)
    }

    /// Get active challenge information
    pub fn get_active_challenge(&self) -> Option<&InterlockChallenge> {
        self.active_challenge.as_ref()
    }

    /// Request high torque mode - starts the challenge process
    pub fn request_high_torque(&mut self, _device_id: &str) -> Result<InterlockChallenge, String> {
        // Check preconditions
        self.check_high_torque_preconditions()?;

        match &self.state {
            SafetyState::SafeTorque => {
                let challenge_token = rand::random::<u32>();
                let challenge = InterlockChallenge {
                    challenge_token,
                    combo_required: ButtonCombo::BothClutchPaddles,
                    expires: Instant::now() + Duration::from_secs(30),
                    ui_consent_given: false,
                    combo_start: None,
                };

                self.state = SafetyState::HighTorqueChallenge {
                    challenge_token,
                    expires: challenge.expires,
                    ui_consent_given: false,
                };

                self.active_challenge = Some(challenge.clone());
                Ok(challenge)
            }
            SafetyState::HighTorqueActive { .. } => Err("High torque already active".to_string()),
            SafetyState::Faulted { fault, .. } => Err(format!(
                "Cannot enable high torque while faulted: {}",
                fault
            )),
            SafetyState::HighTorqueChallenge { .. } | SafetyState::AwaitingPhysicalAck { .. } => {
                Err("High torque challenge already in progress".to_string())
            }
        }
    }

    /// Provide UI consent for high torque mode
    pub fn provide_ui_consent(&mut self, challenge_token: u32) -> Result<(), String> {
        match &mut self.state {
            SafetyState::HighTorqueChallenge {
                challenge_token: token,
                expires,
                ui_consent_given,
            } => {
                if *token != challenge_token {
                    return Err("Invalid challenge token".to_string());
                }

                if Instant::now() > *expires {
                    self.state = SafetyState::SafeTorque;
                    self.active_challenge = None;
                    return Err("Challenge expired".to_string());
                }

                *ui_consent_given = true;

                // Update active challenge
                if let Some(ref mut challenge) = self.active_challenge {
                    challenge.ui_consent_given = true;
                }

                // Transition to awaiting physical acknowledgment
                self.state = SafetyState::AwaitingPhysicalAck {
                    challenge_token: *token,
                    expires: *expires,
                    combo_start: None,
                };

                Ok(())
            }
            _ => Err("No active challenge requiring UI consent".to_string()),
        }
    }

    /// Report button combo start from device
    pub fn report_combo_start(&mut self, challenge_token: u32) -> Result<(), String> {
        match &mut self.state {
            SafetyState::AwaitingPhysicalAck {
                challenge_token: token,
                expires,
                combo_start,
            } => {
                if *token != challenge_token {
                    return Err("Invalid challenge token".to_string());
                }

                if Instant::now() > *expires {
                    self.state = SafetyState::SafeTorque;
                    self.active_challenge = None;
                    return Err("Challenge expired".to_string());
                }

                *combo_start = Some(Instant::now());

                // Update active challenge
                if let Some(ref mut challenge) = self.active_challenge {
                    challenge.combo_start = Some(Instant::now());
                }

                Ok(())
            }
            _ => Err("Not awaiting physical acknowledgment".to_string()),
        }
    }

    /// Check high torque preconditions
    fn check_high_torque_preconditions(&self) -> Result<(), String> {
        // Check for active faults
        if !self.fault_count.is_empty() {
            return Err("Cannot enable high torque with active faults".to_string());
        }

        // Additional precondition checks can be added here
        // - Temperature limits
        // - Recent hands-on detection
        // - Device health status

        Ok(())
    }

    /// Confirm high torque challenge with device acknowledgment
    pub fn confirm_high_torque(
        &mut self,
        device_id: &str,
        ack: InterlockAck,
    ) -> Result<(), String> {
        match &self.state {
            SafetyState::AwaitingPhysicalAck {
                challenge_token,
                expires,
                combo_start,
            } => {
                if ack.challenge_token != *challenge_token {
                    return Err("Invalid challenge token in acknowledgment".to_string());
                }

                if Instant::now() > *expires {
                    self.state = SafetyState::SafeTorque;
                    self.active_challenge = None;
                    return Err("Challenge expired".to_string());
                }

                // Verify combo was held for required duration
                if let Some(start_time) = combo_start {
                    let hold_duration = ack.timestamp.duration_since(*start_time);
                    if hold_duration < self.combo_hold_duration {
                        return Err(format!(
                            "Button combo held for only {:.1}s, required {:.1}s",
                            hold_duration.as_secs_f32(),
                            self.combo_hold_duration.as_secs_f32()
                        ));
                    }
                } else {
                    return Err("Button combo start not detected".to_string());
                }

                // Store device token (persists until power cycle)
                self.device_tokens
                    .insert(device_id.to_string(), ack.device_token);

                // Activate high torque mode
                self.state = SafetyState::HighTorqueActive {
                    since: Instant::now(),
                    device_token: ack.device_token,
                    last_hands_on: Instant::now(),
                };

                self.active_challenge = None;
                Ok(())
            }
            _ => Err("No active challenge awaiting physical acknowledgment".to_string()),
        }
    }

    /// Report a fault
    pub fn report_fault(&mut self, fault: FaultType) {
        *self.fault_count.entry(fault).or_insert(0) += 1;

        self.state = SafetyState::Faulted {
            fault,
            since: Instant::now(),
        };
    }

    /// Clear fault if conditions are met
    pub fn clear_fault(&mut self) -> Result<(), String> {
        match &self.state {
            SafetyState::Faulted { fault: _, since } => {
                // Require minimum fault duration before clearing
                if since.elapsed() < std::time::Duration::from_millis(100) {
                    return Err("Fault duration too short".to_string());
                }

                self.state = SafetyState::SafeTorque;
                Ok(())
            }
            _ => Err("No active fault to clear".to_string()),
        }
    }

    /// Update hands-on status and check for timeout
    pub fn update_hands_on_status(&mut self, hands_on: bool) -> Result<(), String> {
        match &mut self.state {
            SafetyState::HighTorqueActive { last_hands_on, .. } => {
                if hands_on {
                    *last_hands_on = Instant::now();
                } else {
                    let hands_off_duration = last_hands_on.elapsed();
                    if hands_off_duration > self.hands_off_timeout {
                        self.report_fault(FaultType::HandsOffTimeout);
                        return Err(format!(
                            "Hands-off timeout exceeded: {:.1}s > {:.1}s",
                            hands_off_duration.as_secs_f32(),
                            self.hands_off_timeout.as_secs_f32()
                        ));
                    }
                }
                Ok(())
            }
            _ => Ok(()), // Hands-on detection only matters in high torque mode
        }
    }

    /// Check if hands-off timeout should trigger (legacy method)
    pub fn check_hands_off_timeout(&mut self, hands_off_duration: Duration) {
        if hands_off_duration > self.hands_off_timeout
            && let SafetyState::HighTorqueActive { .. } = &self.state
        {
            self.report_fault(FaultType::HandsOffTimeout);
        }
    }

    /// Cancel active challenge
    pub fn cancel_challenge(&mut self) -> Result<(), String> {
        match &self.state {
            SafetyState::HighTorqueChallenge { .. } | SafetyState::AwaitingPhysicalAck { .. } => {
                self.state = SafetyState::SafeTorque;
                self.active_challenge = None;
                Ok(())
            }
            _ => Err("No active challenge to cancel".to_string()),
        }
    }

    /// Force disable high torque mode
    pub fn disable_high_torque(&mut self, device_id: &str) -> Result<(), String> {
        match &self.state {
            SafetyState::HighTorqueActive { .. } => {
                self.state = SafetyState::SafeTorque;
                self.device_tokens.remove(device_id);
                Ok(())
            }
            _ => Err("High torque mode not active".to_string()),
        }
    }

    /// Check if challenge has expired and clean up if needed
    pub fn check_challenge_expiry(&mut self) -> bool {
        let now = Instant::now();
        let expired = match &self.state {
            SafetyState::HighTorqueChallenge { expires, .. }
            | SafetyState::AwaitingPhysicalAck { expires, .. } => now > *expires,
            _ => false,
        };

        if expired {
            self.state = SafetyState::SafeTorque;
            self.active_challenge = None;
        }

        expired
    }

    /// Get time remaining for active challenge
    pub fn get_challenge_time_remaining(&self) -> Option<Duration> {
        match &self.state {
            SafetyState::HighTorqueChallenge { expires, .. }
            | SafetyState::AwaitingPhysicalAck { expires, .. } => {
                let now = Instant::now();
                if now < *expires {
                    Some(*expires - now)
                } else {
                    Some(Duration::ZERO)
                }
            }
            _ => None,
        }
    }
}

impl Default for SafetyService {
    fn default() -> Self {
        Self::new(5.0, 25.0) // 5Nm safe, 25Nm high torque
    }
}

// Serde modules for Instant serialization
mod instant_serde {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(_instant: &Instant, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Convert to duration since a reference point (not perfect but works for UI)
        let duration_since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        duration_since_epoch.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Instant, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        // This is approximate - for UI purposes only
        Ok(Instant::now() + Duration::from_secs(secs))
    }
}

mod option_instant_serde {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
        Ok(opt_secs.map(|secs| Instant::now() + Duration::from_secs(secs)))
    }
}

pub mod fault_injection;
pub mod fmea;
pub mod hardware_watchdog;
pub mod integration;
pub mod watchdog;

#[cfg(test)]
pub mod comprehensive_tests;
#[cfg(test)]
mod tests;

pub use fault_injection::{FaultInjectionScenario, FaultInjectionSystem, TriggerCondition};
pub use fmea::{AudioAlert, FaultThresholds, FmeaSystem, SoftStopController};
pub use hardware_watchdog::{
    FaultLogEntry, HardwareWatchdog, SafetyInterlockState, SafetyInterlockSystem, SafetyTickResult,
    SafetyTrigger, SharedWatchdog, SoftwareWatchdog, TimeoutResponse, TorqueLimit, WatchdogError,
    WatchdogTimeoutHandler,
};
pub use integration::{FaultManagerContext, FaultManagerResult, IntegratedFaultManager};
pub use watchdog::{HealthStatus, SystemComponent, WatchdogConfig, WatchdogSystem};
