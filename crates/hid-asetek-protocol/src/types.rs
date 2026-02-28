//! Type definitions for Asetek protocol

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WheelModel {
    Forte,
    Invicta,
    LaPrima,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WheelCapabilities {
    pub max_torque_nm: f32,
    pub max_speed_rpm: u16,
    pub supports_quick_release: bool,
}

impl Default for WheelCapabilities {
    fn default() -> Self {
        Self {
            max_torque_nm: 18.0,
            max_speed_rpm: 2500,
            supports_quick_release: true,
        }
    }
}

impl WheelCapabilities {
    pub fn for_model(model: WheelModel) -> Self {
        match model {
            WheelModel::Forte => Self {
                max_torque_nm: 18.0,
                max_speed_rpm: 3000,
                supports_quick_release: true,
            },
            WheelModel::Invicta => Self {
                max_torque_nm: 27.0,
                max_speed_rpm: 2500,
                supports_quick_release: true,
            },
            WheelModel::LaPrima => Self {
                max_torque_nm: 12.0,
                max_speed_rpm: 2000,
                supports_quick_release: true,
            },
            WheelModel::Unknown => Self::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wheel_capabilities_forte() {
        let caps = WheelCapabilities::for_model(WheelModel::Forte);
        assert_eq!(caps.max_torque_nm, 18.0);
    }

    #[test]
    fn test_wheel_capabilities_invicta() {
        let caps = WheelCapabilities::for_model(WheelModel::Invicta);
        assert_eq!(caps.max_torque_nm, 27.0);
    }
}
