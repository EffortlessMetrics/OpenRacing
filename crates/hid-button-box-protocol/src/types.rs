//! Type definitions for button box protocol

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonBoxCapabilities {
    pub button_count: usize,
    pub analog_axis_count: usize,
    pub has_pov_hat: bool,
    pub has_rotary_encoders: bool,
    pub rotary_encoder_count: usize,
}

impl Default for ButtonBoxCapabilities {
    fn default() -> Self {
        Self {
            button_count: 32,
            analog_axis_count: 4,
            has_pov_hat: true,
            has_rotary_encoders: true,
            rotary_encoder_count: 8,
        }
    }
}

impl ButtonBoxCapabilities {
    pub fn basic() -> Self {
        Self {
            button_count: 16,
            analog_axis_count: 0,
            has_pov_hat: true,
            has_rotary_encoders: false,
            rotary_encoder_count: 0,
        }
    }

    pub fn extended() -> Self {
        Self {
            button_count: 32,
            analog_axis_count: 4,
            has_pov_hat: true,
            has_rotary_encoders: true,
            rotary_encoder_count: 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ButtonBoxType {
    Simple,
    #[default]
    Standard,
    Extended,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RotaryEncoderState {
    pub position: i32,
    pub delta: i8,
    pub button_pressed: bool,
}

impl RotaryEncoderState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, new_position: i32) {
        let delta = new_position - self.position;
        self.delta = delta.clamp(-127, 127) as i8;
        self.position = new_position;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_button_box_capabilities_basic() {
        let caps = ButtonBoxCapabilities::basic();
        assert_eq!(caps.button_count, 16);
        assert!(!caps.has_rotary_encoders);
        assert_eq!(caps.rotary_encoder_count, 0);
        assert_eq!(caps.analog_axis_count, 0);
        assert!(caps.has_pov_hat);
    }

    #[test]
    fn test_button_box_capabilities_extended() {
        let caps = ButtonBoxCapabilities::extended();
        assert_eq!(caps.button_count, 32);
        assert_eq!(caps.rotary_encoder_count, 8);
        assert!(caps.has_rotary_encoders);
        assert_eq!(caps.analog_axis_count, 4);
    }

    #[test]
    fn test_button_box_capabilities_default() {
        let caps = ButtonBoxCapabilities::default();
        assert_eq!(caps.button_count, 32);
        assert_eq!(caps.analog_axis_count, 4);
        assert!(caps.has_pov_hat);
        assert!(caps.has_rotary_encoders);
        assert_eq!(caps.rotary_encoder_count, 8);
    }

    #[test]
    fn test_button_box_type_default() {
        let bt = ButtonBoxType::default();
        assert_eq!(bt, ButtonBoxType::Standard);
    }

    #[test]
    fn test_rotary_encoder_state() {
        let mut encoder = RotaryEncoderState::new();

        encoder.update(10);
        assert_eq!(encoder.position, 10);
        assert_eq!(encoder.delta, 10);

        encoder.update(12);
        assert_eq!(encoder.position, 12);
        assert_eq!(encoder.delta, 2);

        encoder.update(0);
        assert_eq!(encoder.delta, -12);
    }

    #[test]
    fn test_rotary_encoder_default() {
        let encoder = RotaryEncoderState::default();
        assert_eq!(encoder.position, 0);
        assert_eq!(encoder.delta, 0);
        assert!(!encoder.button_pressed);
    }

    #[test]
    fn test_rotary_encoder_delta_clamping() {
        let mut encoder = RotaryEncoderState::new();
        encoder.update(0);
        // Large positive jump
        encoder.update(500);
        assert_eq!(encoder.delta, 127);
        // Large negative jump
        encoder.update(-500);
        assert_eq!(encoder.delta, -127);
    }

    #[test]
    fn test_rotary_encoder_button_pressed() {
        let mut encoder = RotaryEncoderState::new();
        encoder.button_pressed = true;
        assert!(encoder.button_pressed);
    }

    #[test]
    fn test_button_box_type_variants_distinct() {
        assert_ne!(ButtonBoxType::Simple, ButtonBoxType::Standard);
        assert_ne!(ButtonBoxType::Simple, ButtonBoxType::Extended);
        assert_ne!(ButtonBoxType::Standard, ButtonBoxType::Extended);
    }

    #[test]
    fn test_capabilities_basic_no_analog() {
        let caps = ButtonBoxCapabilities::basic();
        assert_eq!(caps.analog_axis_count, 0);
        assert!(!caps.has_rotary_encoders);
    }

    #[test]
    fn test_capabilities_extended_matches_default() {
        let ext = ButtonBoxCapabilities::extended();
        let def = ButtonBoxCapabilities::default();
        assert_eq!(ext.button_count, def.button_count);
        assert_eq!(ext.analog_axis_count, def.analog_axis_count);
        assert_eq!(ext.has_pov_hat, def.has_pov_hat);
        assert_eq!(ext.has_rotary_encoders, def.has_rotary_encoders);
        assert_eq!(ext.rotary_encoder_count, def.rotary_encoder_count);
    }

    #[test]
    fn test_rotary_encoder_sequential_small_updates() {
        let mut encoder = RotaryEncoderState::new();
        encoder.update(1);
        assert_eq!(encoder.position, 1);
        assert_eq!(encoder.delta, 1);

        encoder.update(2);
        assert_eq!(encoder.position, 2);
        assert_eq!(encoder.delta, 1);

        encoder.update(1);
        assert_eq!(encoder.position, 1);
        assert_eq!(encoder.delta, -1);
    }

    #[test]
    fn test_rotary_encoder_update_same_position() {
        let mut encoder = RotaryEncoderState::new();
        encoder.update(5);
        assert_eq!(encoder.delta, 5);

        encoder.update(5);
        assert_eq!(encoder.position, 5);
        assert_eq!(encoder.delta, 0);
    }

    #[test]
    fn test_capabilities_clone() {
        let caps = ButtonBoxCapabilities::extended();
        let cloned = caps.clone();
        assert_eq!(cloned.button_count, caps.button_count);
        assert_eq!(cloned.rotary_encoder_count, caps.rotary_encoder_count);
    }

    #[test]
    fn test_rotary_encoder_state_clone() {
        let mut encoder = RotaryEncoderState::new();
        encoder.update(42);
        let cloned = encoder.clone();
        assert_eq!(cloned.position, 42);
    }
}
