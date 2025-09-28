//! Safety systems and fault handling

use std::time::Instant;

/// Safety state machine for torque management
#[derive(Debug, Clone, PartialEq)]
pub enum SafetyState {
    /// Safe torque mode (limited torque)
    SafeTorque,
    /// High torque challenge in progress
    HighTorqueChallenge {
        challenge_token: u32,
        expires: Instant,
    },
    /// High torque active
    HighTorqueActive {
        since: Instant,
        device_token: u32,
    },
    /// Faulted state (torque disabled)
    Faulted {
        fault: FaultType,
        since: Instant,
    },
}

/// Types of faults that can occur
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultType {
    UsbStall,
    EncoderNaN,
    ThermalLimit,
    Overcurrent,
    PluginOverrun,
    TimingViolation,
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
        }
    }
}

/// Safety service for managing torque limits and fault handling
pub struct SafetyService {
    state: SafetyState,
    max_safe_torque_nm: f32,
    max_high_torque_nm: f32,
    fault_count: std::collections::HashMap<FaultType, u32>,
}

impl SafetyService {
    /// Create new safety service
    pub fn new(max_safe_torque_nm: f32, max_high_torque_nm: f32) -> Self {
        Self {
            state: SafetyState::SafeTorque,
            max_safe_torque_nm,
            max_high_torque_nm,
            fault_count: std::collections::HashMap::new(),
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
            SafetyState::HighTorqueActive { .. } => self.max_high_torque_nm,
            SafetyState::Faulted { .. } => 0.0,
        }
    }

    /// Request high torque mode
    pub fn request_high_torque(&mut self) -> Result<u32, String> {
        match &self.state {
            SafetyState::SafeTorque => {
                let challenge_token = rand::random::<u32>();
                self.state = SafetyState::HighTorqueChallenge {
                    challenge_token,
                    expires: Instant::now() + std::time::Duration::from_secs(30),
                };
                Ok(challenge_token)
            }
            SafetyState::HighTorqueActive { .. } => {
                Err("High torque already active".to_string())
            }
            SafetyState::Faulted { fault, .. } => {
                Err(format!("Cannot enable high torque while faulted: {}", fault))
            }
            SafetyState::HighTorqueChallenge { .. } => {
                Err("High torque challenge already in progress".to_string())
            }
        }
    }

    /// Confirm high torque challenge
    pub fn confirm_high_torque(&mut self, device_token: u32) -> Result<(), String> {
        match &self.state {
            SafetyState::HighTorqueChallenge { expires, .. } => {
                if Instant::now() > *expires {
                    self.state = SafetyState::SafeTorque;
                    return Err("Challenge expired".to_string());
                }

                self.state = SafetyState::HighTorqueActive {
                    since: Instant::now(),
                    device_token,
                };
                Ok(())
            }
            _ => Err("No active challenge".to_string()),
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
            SafetyState::Faulted { fault, since } => {
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

    /// Check if hands-off timeout should trigger
    pub fn check_hands_off_timeout(&mut self, hands_off_duration: std::time::Duration) {
        if hands_off_duration > std::time::Duration::from_secs(5) {
            match &self.state {
                SafetyState::HighTorqueActive { .. } => {
                    self.state = SafetyState::SafeTorque;
                }
                _ => {}
            }
        }
    }
}

impl Default for SafetyService {
    fn default() -> Self {
        Self::new(5.0, 25.0) // 5Nm safe, 25Nm high torque
    }
}