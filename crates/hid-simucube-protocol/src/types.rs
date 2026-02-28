//! Type definitions for Simucube protocol

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum WheelModel {
    Simucube2Sport,
    Simucube2Pro,
    Simucube2Ultimate,
    SimucubeActivePedal,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WheelCapabilities {
    pub max_torque_nm: f32,
    pub encoder_resolution_bits: u32,
    pub supports_wireless: bool,
    pub supports_active_pedal: bool,
    pub max_speed_rpm: u16,
}

impl Default for WheelCapabilities {
    fn default() -> Self {
        Self {
            max_torque_nm: 25.0,
            encoder_resolution_bits: 22,
            supports_wireless: true,
            supports_active_pedal: true,
            max_speed_rpm: 3000,
        }
    }
}

impl WheelCapabilities {
    pub fn for_model(model: WheelModel) -> Self {
        match model {
            WheelModel::Simucube2Sport => Self {
                max_torque_nm: 17.0,
                ..Default::default()
            },
            WheelModel::Simucube2Pro => Self {
                max_torque_nm: 25.0,
                ..Default::default()
            },
            WheelModel::Simucube2Ultimate => Self {
                max_torque_nm: 32.0,
                ..Default::default()
            },
            WheelModel::SimucubeActivePedal => Self {
                max_torque_nm: 0.0,
                encoder_resolution_bits: 16,
                supports_wireless: false,
                supports_active_pedal: true,
                max_speed_rpm: 0,
            },
            WheelModel::Unknown => Self::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DeviceStatus {
    #[default]
    Disconnected,
    Ready,
    Enabled,
    Error,
    Calibrating,
}

impl DeviceStatus {
    pub fn from_flags(flags: u8) -> Self {
        if flags & 0x01 == 0 {
            return Self::Disconnected;
        }
        if flags & 0x04 != 0 {
            return Self::Calibrating;
        }
        if flags & 0x02 == 0 {
            return Self::Ready;
        }
        if flags & 0x08 != 0 {
            return Self::Error;
        }
        Self::Enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wheel_capabilities_sport() {
        let caps = WheelCapabilities::for_model(WheelModel::Simucube2Sport);
        assert_eq!(caps.max_torque_nm, 17.0);
        assert!(caps.supports_wireless);
    }

    #[test]
    fn test_wheel_capabilities_pro() {
        let caps = WheelCapabilities::for_model(WheelModel::Simucube2Pro);
        assert_eq!(caps.max_torque_nm, 25.0);
    }

    #[test]
    fn test_wheel_capabilities_ultimate() {
        let caps = WheelCapabilities::for_model(WheelModel::Simucube2Ultimate);
        assert_eq!(caps.max_torque_nm, 32.0);
    }

    #[test]
    fn test_device_status_from_flags() {
        assert_eq!(DeviceStatus::from_flags(0x00), DeviceStatus::Disconnected);
        assert_eq!(DeviceStatus::from_flags(0x01), DeviceStatus::Ready);
        assert_eq!(DeviceStatus::from_flags(0x03), DeviceStatus::Enabled);
        assert_eq!(DeviceStatus::from_flags(0x05), DeviceStatus::Calibrating);
        assert_eq!(DeviceStatus::from_flags(0x09), DeviceStatus::Ready);
    }
}
