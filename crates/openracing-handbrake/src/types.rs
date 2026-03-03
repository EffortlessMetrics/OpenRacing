//! Handbrake type definitions

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HandbrakeType {
    #[default]
    Analog,
    Digital,
    LoadCell,
    HallEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HandbrakeCapabilities {
    pub handbrake_type: HandbrakeType,
    pub max_load_kg: Option<f32>,
    pub has_hall_effect_sensor: bool,
    pub supports_calibration: bool,
}

impl Default for HandbrakeCapabilities {
    fn default() -> Self {
        Self {
            handbrake_type: HandbrakeType::Analog,
            max_load_kg: None,
            has_hall_effect_sensor: false,
            supports_calibration: true,
        }
    }
}

impl HandbrakeCapabilities {
    pub fn analog() -> Self {
        Self {
            handbrake_type: HandbrakeType::Analog,
            max_load_kg: None,
            has_hall_effect_sensor: false,
            supports_calibration: true,
        }
    }

    pub fn load_cell(max_load_kg: f32) -> Self {
        Self {
            handbrake_type: HandbrakeType::LoadCell,
            max_load_kg: Some(max_load_kg),
            has_hall_effect_sensor: false,
            supports_calibration: true,
        }
    }

    pub fn hall_effect() -> Self {
        Self {
            handbrake_type: HandbrakeType::HallEffect,
            max_load_kg: None,
            has_hall_effect_sensor: true,
            supports_calibration: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handbrake_capabilities_analog() {
        let caps = HandbrakeCapabilities::analog();
        assert_eq!(caps.handbrake_type, HandbrakeType::Analog);
        assert!(!caps.has_hall_effect_sensor);
        assert!(caps.supports_calibration);
        assert_eq!(caps.max_load_kg, None);
    }

    #[test]
    fn test_handbrake_capabilities_load_cell() {
        let caps = HandbrakeCapabilities::load_cell(100.0);
        assert_eq!(caps.handbrake_type, HandbrakeType::LoadCell);
        assert_eq!(caps.max_load_kg, Some(100.0));
    }

    #[test]
    fn test_handbrake_capabilities_hall_effect() {
        let caps = HandbrakeCapabilities::hall_effect();
        assert_eq!(caps.handbrake_type, HandbrakeType::HallEffect);
        assert!(caps.has_hall_effect_sensor);
    }

    #[test]
    fn test_handbrake_capabilities_default() {
        let caps = HandbrakeCapabilities::default();
        assert_eq!(caps.handbrake_type, HandbrakeType::Analog);
        assert_eq!(caps.max_load_kg, None);
        assert!(!caps.has_hall_effect_sensor);
        assert!(caps.supports_calibration);
    }

    #[test]
    fn test_handbrake_type_default() {
        let ht = HandbrakeType::default();
        assert_eq!(ht, HandbrakeType::Analog);
    }

    #[test]
    fn test_handbrake_type_variants_distinct() {
        assert_ne!(HandbrakeType::Analog, HandbrakeType::Digital);
        assert_ne!(HandbrakeType::Analog, HandbrakeType::LoadCell);
        assert_ne!(HandbrakeType::Analog, HandbrakeType::HallEffect);
        assert_ne!(HandbrakeType::Digital, HandbrakeType::LoadCell);
        assert_ne!(HandbrakeType::Digital, HandbrakeType::HallEffect);
        assert_ne!(HandbrakeType::LoadCell, HandbrakeType::HallEffect);
    }

    #[test]
    fn test_load_cell_capabilities_has_load() {
        let caps = HandbrakeCapabilities::load_cell(50.0);
        assert_eq!(caps.max_load_kg, Some(50.0));
        assert!(!caps.has_hall_effect_sensor);
    }

    #[test]
    fn test_handbrake_type_clone_copy() {
        let a = HandbrakeType::HallEffect;
        let b = a;
        #[allow(clippy::clone_on_copy)]
        let c = a.clone();
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn test_handbrake_capabilities_clone() {
        let caps = HandbrakeCapabilities::load_cell(75.0);
        #[allow(clippy::clone_on_copy)]
        let cloned = caps.clone();
        assert_eq!(caps, cloned);
        assert_eq!(cloned.max_load_kg, Some(75.0));
    }

    #[test]
    fn test_handbrake_capabilities_load_cell_zero_load() {
        let caps = HandbrakeCapabilities::load_cell(0.0);
        assert_eq!(caps.max_load_kg, Some(0.0));
        assert_eq!(caps.handbrake_type, HandbrakeType::LoadCell);
    }

    #[test]
    fn test_handbrake_capabilities_analog_no_load() {
        let caps = HandbrakeCapabilities::analog();
        assert_eq!(caps.max_load_kg, None);
        assert!(!caps.has_hall_effect_sensor);
        assert!(caps.supports_calibration);
    }

    #[test]
    fn test_handbrake_capabilities_eq() {
        let a = HandbrakeCapabilities::analog();
        let b = HandbrakeCapabilities::analog();
        assert_eq!(a, b);
        assert_ne!(a, HandbrakeCapabilities::hall_effect());
    }
}
