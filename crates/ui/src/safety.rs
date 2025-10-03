//! Safety UI components for high torque consent flow

use racing_wheel_engine::safety::{ConsentRequirements, InterlockChallenge, ButtonCombo};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

/// UI state for the safety consent flow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentFlowState {
    pub step: ConsentStep,
    pub requirements: ConsentRequirements,
    pub challenge: Option<InterlockChallenge>,
    pub time_remaining: Option<Duration>,
    pub error_message: Option<String>,
}

/// Steps in the consent flow
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConsentStep {
    /// Initial step - show warnings and disclaimers
    ShowWarnings,
    /// User must explicitly consent to high torque
    RequireConsent,
    /// Waiting for physical button combo
    AwaitingPhysicalAck,
    /// High torque activated successfully
    Activated,
    /// Flow cancelled or failed
    Failed { reason: String },
}

/// UI consent dialog component
#[derive(Debug, Clone)]
pub struct ConsentDialog {
    state: ConsentFlowState,
    consent_given: bool,
    warnings_acknowledged: bool,
    disclaimers_acknowledged: bool,
}

impl ConsentDialog {
    /// Create new consent dialog
    pub fn new(requirements: ConsentRequirements) -> Self {
        Self {
            state: ConsentFlowState {
                step: ConsentStep::ShowWarnings,
                requirements,
                challenge: None,
                time_remaining: None,
                error_message: None,
            },
            consent_given: false,
            warnings_acknowledged: false,
            disclaimers_acknowledged: false,
        }
    }

    /// Get current state
    pub fn state(&self) -> &ConsentFlowState {
        &self.state
    }

    /// Acknowledge warnings
    pub fn acknowledge_warnings(&mut self) -> Result<(), String> {
        if self.state.step != ConsentStep::ShowWarnings {
            return Err("Not in warnings step".to_string());
        }

        self.warnings_acknowledged = true;
        
        if self.warnings_acknowledged && self.disclaimers_acknowledged {
            self.state.step = ConsentStep::RequireConsent;
        }

        Ok(())
    }

    /// Acknowledge disclaimers
    pub fn acknowledge_disclaimers(&mut self) -> Result<(), String> {
        if self.state.step != ConsentStep::ShowWarnings {
            return Err("Not in warnings step".to_string());
        }

        self.disclaimers_acknowledged = true;
        
        if self.warnings_acknowledged && self.disclaimers_acknowledged {
            self.state.step = ConsentStep::RequireConsent;
        }

        Ok(())
    }

    /// Provide explicit consent
    pub fn provide_consent(&mut self) -> Result<(), String> {
        if self.state.step != ConsentStep::RequireConsent {
            return Err("Not in consent step".to_string());
        }

        if !self.warnings_acknowledged || !self.disclaimers_acknowledged {
            return Err("Must acknowledge warnings and disclaimers first".to_string());
        }

        self.consent_given = true;
        Ok(())
    }

    /// Start physical acknowledgment phase
    pub fn start_physical_ack(&mut self, challenge: InterlockChallenge) -> Result<(), String> {
        if !self.consent_given {
            return Err("UI consent not provided".to_string());
        }

        let expires = challenge.expires;
        self.state.challenge = Some(challenge);
        self.state.step = ConsentStep::AwaitingPhysicalAck;
        self.state.time_remaining = Some(
            expires.duration_since(Instant::now())
        );

        Ok(())
    }

    /// Update time remaining for challenge
    pub fn update_time_remaining(&mut self, remaining: Duration) {
        self.state.time_remaining = Some(remaining);
        
        if remaining == Duration::ZERO {
            self.state.step = ConsentStep::Failed {
                reason: "Challenge expired".to_string(),
            };
            self.state.error_message = Some("Challenge expired. Please try again.".to_string());
        }
    }

    /// Mark as activated
    pub fn mark_activated(&mut self) {
        self.state.step = ConsentStep::Activated;
        self.state.challenge = None;
        self.state.time_remaining = None;
        self.state.error_message = None;
    }

    /// Mark as failed
    pub fn mark_failed(&mut self, reason: String) {
        self.state.step = ConsentStep::Failed { reason: reason.clone() };
        self.state.error_message = Some(reason);
        self.state.challenge = None;
        self.state.time_remaining = None;
    }

    /// Cancel the flow
    pub fn cancel(&mut self) {
        self.state.step = ConsentStep::Failed {
            reason: "Cancelled by user".to_string(),
        };
        self.state.error_message = Some("High torque activation cancelled".to_string());
        self.state.challenge = None;
        self.state.time_remaining = None;
    }

    /// Check if flow is complete (success or failure)
    pub fn is_complete(&self) -> bool {
        matches!(self.state.step, ConsentStep::Activated | ConsentStep::Failed { .. })
    }

    /// Check if consent has been fully provided
    pub fn is_consent_complete(&self) -> bool {
        self.consent_given && self.warnings_acknowledged && self.disclaimers_acknowledged
    }
}

/// Physical button combo instruction component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComboInstructions {
    pub combo: ButtonCombo,
    pub instructions: Vec<String>,
    pub hold_duration_secs: f32,
    pub visual_aid: Option<String>,
}

impl ComboInstructions {
    /// Get instructions for a button combo
    pub fn for_combo(combo: ButtonCombo, hold_duration: Duration) -> Self {
        let (instructions, visual_aid) = match combo {
            ButtonCombo::BothClutchPaddles => (
                vec![
                    "Hold BOTH clutch paddles simultaneously".to_string(),
                    format!("Keep holding for {:.1} seconds", hold_duration.as_secs_f32()),
                    "Release when prompted by the system".to_string(),
                ],
                Some("clutch_paddles_diagram.svg".to_string()),
            ),
            ButtonCombo::CustomSequence(_) => (
                vec![
                    "Follow the custom button sequence".to_string(),
                    "Refer to your device manual for details".to_string(),
                ],
                None,
            ),
        };

        Self {
            combo,
            instructions,
            hold_duration_secs: hold_duration.as_secs_f32(),
            visual_aid,
        }
    }
}

/// Safety banner component for active high torque mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyBanner {
    pub active: bool,
    pub current_torque_nm: f32,
    pub max_torque_nm: f32,
    pub hands_on_status: Option<bool>,
    pub time_active: Duration,
    pub emergency_stop_combo: ButtonCombo,
}

impl SafetyBanner {
    /// Create new safety banner
    pub fn new(max_torque_nm: f32, emergency_stop_combo: ButtonCombo) -> Self {
        Self {
            active: false,
            current_torque_nm: 0.0,
            max_torque_nm,
            hands_on_status: None,
            time_active: Duration::ZERO,
            emergency_stop_combo,
        }
    }

    /// Activate the banner
    pub fn activate(&mut self) {
        self.active = true;
        self.time_active = Duration::ZERO;
    }

    /// Deactivate the banner
    pub fn deactivate(&mut self) {
        self.active = false;
        self.current_torque_nm = 0.0;
        self.time_active = Duration::ZERO;
    }

    /// Update torque reading
    pub fn update_torque(&mut self, torque_nm: f32) {
        self.current_torque_nm = torque_nm;
    }

    /// Update hands-on status
    pub fn update_hands_on(&mut self, hands_on: Option<bool>) {
        self.hands_on_status = hands_on;
    }

    /// Update active time
    pub fn update_time_active(&mut self, time_active: Duration) {
        self.time_active = time_active;
    }

    /// Get warning level based on current torque
    pub fn get_warning_level(&self) -> WarningLevel {
        let torque_ratio = self.current_torque_nm / self.max_torque_nm;
        
        if torque_ratio > 0.9 {
            WarningLevel::Critical
        } else if torque_ratio > 0.7 {
            WarningLevel::High
        } else if torque_ratio > 0.5 {
            WarningLevel::Medium
        } else {
            WarningLevel::Low
        }
    }

    /// Check if hands-off warning should be shown
    pub fn should_show_hands_off_warning(&self) -> bool {
        matches!(self.hands_on_status, Some(false))
    }
}

/// Warning levels for safety banner
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WarningLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl WarningLevel {
    /// Get color for warning level
    pub fn color(&self) -> &'static str {
        match self {
            WarningLevel::Low => "#4CAF50",      // Green
            WarningLevel::Medium => "#FF9800",   // Orange
            WarningLevel::High => "#FF5722",     // Red-Orange
            WarningLevel::Critical => "#F44336", // Red
        }
    }

    /// Get text for warning level
    pub fn text(&self) -> &'static str {
        match self {
            WarningLevel::Low => "Normal",
            WarningLevel::Medium => "Moderate",
            WarningLevel::High => "High",
            WarningLevel::Critical => "CRITICAL",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_requirements() -> ConsentRequirements {
        ConsentRequirements {
            max_torque_nm: 25.0,
            warnings: vec![
                "High torque warning".to_string(),
                "Safety warning".to_string(),
            ],
            disclaimers: vec![
                "Liability disclaimer".to_string(),
            ],
            requires_explicit_consent: true,
        }
    }

    #[test]
    fn test_consent_dialog_flow() {
        let requirements = create_test_requirements();
        let mut dialog = ConsentDialog::new(requirements);

        // Initial state
        assert_eq!(dialog.state().step, ConsentStep::ShowWarnings);
        assert!(!dialog.is_consent_complete());

        // Acknowledge warnings
        dialog.acknowledge_warnings().unwrap();
        assert!(dialog.warnings_acknowledged);

        // Still in warnings step until disclaimers acknowledged
        assert_eq!(dialog.state().step, ConsentStep::ShowWarnings);

        // Acknowledge disclaimers
        dialog.acknowledge_disclaimers().unwrap();
        assert!(dialog.disclaimers_acknowledged);

        // Should move to consent step
        assert_eq!(dialog.state().step, ConsentStep::RequireConsent);

        // Provide consent
        dialog.provide_consent().unwrap();
        assert!(dialog.is_consent_complete());
    }

    #[test]
    fn test_consent_dialog_cancel() {
        let requirements = create_test_requirements();
        let mut dialog = ConsentDialog::new(requirements);

        dialog.cancel();
        assert!(dialog.is_complete());
        assert!(matches!(dialog.state().step, ConsentStep::Failed { .. }));
    }

    #[test]
    fn test_combo_instructions() {
        let combo = ButtonCombo::BothClutchPaddles;
        let duration = Duration::from_secs(2);
        let instructions = ComboInstructions::for_combo(combo, duration);

        assert_eq!(instructions.combo, combo);
        assert_eq!(instructions.hold_duration_secs, 2.0);
        assert!(!instructions.instructions.is_empty());
    }

    #[test]
    fn test_safety_banner() {
        let mut banner = SafetyBanner::new(25.0, ButtonCombo::BothClutchPaddles);

        assert!(!banner.active);

        banner.activate();
        assert!(banner.active);

        // Test warning levels
        banner.update_torque(5.0);
        assert_eq!(banner.get_warning_level(), WarningLevel::Low);

        banner.update_torque(15.0);
        assert_eq!(banner.get_warning_level(), WarningLevel::Medium);

        banner.update_torque(20.0);
        assert_eq!(banner.get_warning_level(), WarningLevel::High);

        banner.update_torque(23.0);
        assert_eq!(banner.get_warning_level(), WarningLevel::Critical);

        // Test hands-off warning
        banner.update_hands_on(Some(false));
        assert!(banner.should_show_hands_off_warning());

        banner.update_hands_on(Some(true));
        assert!(!banner.should_show_hands_off_warning());
    }
}