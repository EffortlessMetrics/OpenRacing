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
    }

    #[test]
    fn test_button_box_capabilities_extended() {
        let caps = ButtonBoxCapabilities::extended();
        assert_eq!(caps.button_count, 32);
        assert_eq!(caps.rotary_encoder_count, 8);
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
}
