//! VRS DirectForce Pro device types: models, categories, pedal axes, and device identity.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]

use crate::ids::product_ids;
use crate::ids::report_ids;

/// High-level category for VRS USB products.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VrsDeviceCategory {
    Wheelbase,
    Pedals,
    Shifter,
    Handbrake,
    Unknown,
}

/// Identity metadata for a VRS product ID.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VrsDeviceIdentity {
    pub product_id: u16,
    pub name: &'static str,
    pub category: VrsDeviceCategory,
    pub supports_ffb: bool,
    pub max_torque_nm: Option<f32>,
}

/// Identify a VRS product.
pub fn identify_device(product_id: u16) -> VrsDeviceIdentity {
    match product_id {
        product_ids::DIRECTFORCE_PRO => VrsDeviceIdentity {
            product_id,
            name: "VRS DirectForce Pro",
            category: VrsDeviceCategory::Wheelbase,
            supports_ffb: true,
            max_torque_nm: Some(20.0),
        },
        product_ids::DIRECTFORCE_PRO_V2 => VrsDeviceIdentity {
            product_id,
            name: "VRS DirectForce Pro V2",
            category: VrsDeviceCategory::Wheelbase,
            supports_ffb: true,
            max_torque_nm: Some(25.0),
        },
        product_ids::PEDALS_V1 => VrsDeviceIdentity {
            product_id,
            name: "VRS Pedals V1",
            category: VrsDeviceCategory::Pedals,
            supports_ffb: false,
            max_torque_nm: None,
        },
        product_ids::PEDALS_V2 => VrsDeviceIdentity {
            product_id,
            name: "VRS Pedals V2",
            category: VrsDeviceCategory::Pedals,
            supports_ffb: false,
            max_torque_nm: None,
        },
        product_ids::HANDBRAKE => VrsDeviceIdentity {
            product_id,
            name: "VRS Handbrake",
            category: VrsDeviceCategory::Handbrake,
            supports_ffb: false,
            max_torque_nm: None,
        },
        product_ids::SHIFTER => VrsDeviceIdentity {
            product_id,
            name: "VRS Shifter",
            category: VrsDeviceCategory::Shifter,
            supports_ffb: false,
            max_torque_nm: None,
        },
        _ => VrsDeviceIdentity {
            product_id,
            name: "VRS Unknown",
            category: VrsDeviceCategory::Unknown,
            supports_ffb: false,
            max_torque_nm: None,
        },
    }
}

/// Return true when the product ID is a known VRS wheelbase.
pub fn is_wheelbase_product(product_id: u16) -> bool {
    matches!(
        identify_device(product_id).category,
        VrsDeviceCategory::Wheelbase
    )
}

/// Raw pedal axis samples parsed from an input report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VrsPedalAxesRaw {
    pub throttle: u16,
    pub brake: u16,
    pub clutch: u16,
}

/// Normalized pedal axis samples in the `[0.0, 1.0]` range.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VrsPedalAxes {
    pub throttle: f32,
    pub brake: f32,
    pub clutch: f32,
}

impl VrsPedalAxesRaw {
    /// Normalize raw pedal values to [0.0, 1.0] range.
    pub fn normalize(self) -> VrsPedalAxes {
        const MAX: f32 = u16::MAX as f32;
        VrsPedalAxes {
            throttle: self.throttle as f32 / MAX,
            brake: self.brake as f32 / MAX,
            clutch: self.clutch as f32 / MAX,
        }
    }
}

/// FFB effect types supported by VRS DirectForce Pro wheelbases (PIDFF).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VrsFfbEffectType {
    Constant,
    Ramp,
    Square,
    Sine,
    Triangle,
    SawtoothUp,
    SawtoothDown,
    Spring,
    Damper,
    Friction,
    Custom,
}

impl VrsFfbEffectType {
    /// Get the HID report ID for this effect type.
    pub fn report_id(&self) -> u8 {
        match self {
            Self::Constant => report_ids::CONSTANT_FORCE,
            Self::Ramp => report_ids::RAMP_FORCE,
            Self::Square => report_ids::SQUARE_EFFECT,
            Self::Sine => report_ids::SINE_EFFECT,
            Self::Triangle => report_ids::TRIANGLE_EFFECT,
            Self::SawtoothUp => report_ids::SAWTOOTH_UP_EFFECT,
            Self::SawtoothDown => report_ids::SAWTOOTH_DOWN_EFFECT,
            Self::Spring => report_ids::SPRING_EFFECT,
            Self::Damper => report_ids::DAMPER_EFFECT,
            Self::Friction => report_ids::FRICTION_EFFECT,
            Self::Custom => report_ids::CUSTOM_FORCE_EFFECT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identify_device_known_pids() {
        let known_pids = [0xA355u16, 0xA356, 0xA357, 0xA358, 0xA359, 0xA35A];

        for &pid in &known_pids {
            let identity = identify_device(pid);
            assert!(!identity.name.is_empty());
            match identity.category {
                VrsDeviceCategory::Wheelbase => {
                    assert!(identity.supports_ffb);
                    assert!(identity.max_torque_nm.is_some());
                }
                VrsDeviceCategory::Pedals
                | VrsDeviceCategory::Shifter
                | VrsDeviceCategory::Handbrake => {
                    assert!(!identity.supports_ffb);
                }
                VrsDeviceCategory::Unknown => {}
            }
        }
    }

    #[test]
    fn test_is_wheelbase_product_known_wheelbases() {
        let wheelbase_pids = [0xA355u16, 0xA356];

        for &pid in &wheelbase_pids {
            assert!(is_wheelbase_product(pid));
        }
    }

    #[test]
    fn test_is_wheelbase_product_non_wheelbases() {
        let non_wheelbase_pids = [0xA357u16, 0xA358, 0xA359, 0xA35A];

        for &pid in &non_wheelbase_pids {
            assert!(!is_wheelbase_product(pid));
        }
    }

    #[test]
    fn test_pedal_axes_normalize() {
        let raw = VrsPedalAxesRaw {
            throttle: 0,
            brake: 32768,
            clutch: 65535,
        };
        let normalized = raw.normalize();

        assert!((normalized.throttle - 0.0).abs() < 0.001);
        assert!((normalized.brake - 0.5).abs() < 0.001);
        assert!((normalized.clutch - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_ffb_effect_type_report_ids() {
        assert_eq!(VrsFfbEffectType::Constant.report_id(), 0x11);
        assert_eq!(VrsFfbEffectType::Spring.report_id(), 0x19);
        assert_eq!(VrsFfbEffectType::Damper.report_id(), 0x1A);
        assert_eq!(VrsFfbEffectType::Friction.report_id(), 0x1B);
    }
}
