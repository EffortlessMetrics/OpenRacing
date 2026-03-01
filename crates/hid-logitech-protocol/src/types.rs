//! Logitech device model classification.

#![deny(static_mut_refs)]

use crate::ids::product_ids;

/// Logitech wheel model classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogitechModel {
    /// MOMO Racing wheel (2.2 Nm, 900°, gear-driven).
    MOMO,
    /// Driving Force Pro (900°, belt-driven).
    DrivingForcePro,
    /// Driving Force GT (900°, belt-driven, shift LEDs).
    DrivingForceGT,
    /// Speed Force Wireless (Wii racing wheel).
    SpeedForceWireless,
    /// G25 racing wheel (2.5 Nm, 900°).
    G25,
    /// G27 racing wheel (2.5 Nm, 900°).
    G27,
    /// G29 racing wheel (2.2 Nm, 900°).
    G29,
    /// G920 racing wheel (2.2 Nm, 900°).
    G920,
    /// G923 racing wheel with TrueForce (2.2 Nm, 900°).
    G923,
    /// G PRO direct-drive racing wheel (11 Nm, 1080°; PS and Xbox/PC variants).
    GPro,
    /// Unknown or future Logitech wheel.
    Unknown,
}

impl LogitechModel {
    /// Classify a device by its product ID.
    pub fn from_product_id(product_id: u16) -> Self {
        match product_id {
            product_ids::MOMO => Self::MOMO,
            product_ids::DRIVING_FORCE_PRO => Self::DrivingForcePro,
            product_ids::DRIVING_FORCE_GT => Self::DrivingForceGT,
            product_ids::SPEED_FORCE_WIRELESS => Self::SpeedForceWireless,
            product_ids::G25 => Self::G25,
            product_ids::G27_A | product_ids::G27 => Self::G27,
            product_ids::G29_PS => Self::G29,
            product_ids::G920 => Self::G920,
            product_ids::G923 | product_ids::G923_XBOX | product_ids::G923_PS => Self::G923,
            product_ids::G_PRO | product_ids::G_PRO_XBOX => Self::GPro,
            _ => Self::Unknown,
        }
    }

    /// Maximum continuous torque in Newton-meters for this model.
    ///
    /// These are manufacturer-specified peak torque values from Logitech
    /// product data sheets. They are **not** present in any open-source
    /// driver (drivers operate in dimensionless force units). Values are
    /// used here to normalize physical torque requests to the device's
    /// ±10000 magnitude range.
    pub fn max_torque_nm(self) -> f32 {
        match self {
            Self::MOMO
            | Self::DrivingForcePro
            | Self::DrivingForceGT
            | Self::SpeedForceWireless => 2.0,
            Self::G25 | Self::G27 => 2.5,
            Self::G29 | Self::G920 | Self::G923 => 2.2,
            Self::GPro => 11.0,
            Self::Unknown => 2.0,
        }
    }

    /// Maximum wheel rotation in degrees.
    ///
    /// Source: `lg4ff_devices[]` in kernel and new-lg4ff define `max_range`
    /// as 900 for G25/G27/DFGT/G29/G923. The G PRO supports 1080° per
    /// Logitech product specifications (not yet in any open-source driver).
    pub fn max_rotation_deg(self) -> u16 {
        match self {
            Self::GPro => 1080,
            _ => 900,
        }
    }

    /// Whether this model supports TrueForce haptics.
    ///
    /// TrueForce is a proprietary Logitech haptic feedback feature
    /// exclusive to the G923. No public protocol specification exists in
    /// any open-source driver as of this writing; `berarma/new-lg4ff`
    /// supports G923 standard FFB but does not implement TrueForce.
    pub fn supports_trueforce(self) -> bool {
        matches!(self, Self::G923)
    }
}

/// Return `true` if the product ID corresponds to a known Logitech wheel.
pub fn is_wheel_product(product_id: u16) -> bool {
    matches!(
        product_id,
        product_ids::MOMO
            | product_ids::DRIVING_FORCE_PRO
            | product_ids::DRIVING_FORCE_GT
            | product_ids::SPEED_FORCE_WIRELESS
            | product_ids::G25
            | product_ids::G27_A
            | product_ids::G27
            | product_ids::G29_PS
            | product_ids::G920
            | product_ids::G923
            | product_ids::G923_XBOX
            | product_ids::G923_PS
            | product_ids::G_PRO
            | product_ids::G_PRO_XBOX
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_g920() -> Result<(), Box<dyn std::error::Error>> {
        let model = LogitechModel::from_product_id(product_ids::G920);
        assert_eq!(model, LogitechModel::G920);
        assert!((model.max_torque_nm() - 2.2).abs() < 0.05);
        assert_eq!(model.max_rotation_deg(), 900);
        assert!(!model.supports_trueforce());
        Ok(())
    }

    #[test]
    fn test_model_g923_trueforce() -> Result<(), Box<dyn std::error::Error>> {
        let model = LogitechModel::from_product_id(product_ids::G923_XBOX);
        assert_eq!(model, LogitechModel::G923);
        assert!(model.supports_trueforce());
        Ok(())
    }

    #[test]
    fn test_model_g923_ps_trueforce() -> Result<(), Box<dyn std::error::Error>> {
        let model = LogitechModel::from_product_id(product_ids::G923_PS);
        assert_eq!(model, LogitechModel::G923);
        assert!(model.supports_trueforce());
        Ok(())
    }

    #[test]
    fn test_model_g_pro() -> Result<(), Box<dyn std::error::Error>> {
        let model_ps = LogitechModel::from_product_id(product_ids::G_PRO);
        let model_xbox = LogitechModel::from_product_id(product_ids::G_PRO_XBOX);
        assert_eq!(model_ps, LogitechModel::GPro);
        assert_eq!(model_xbox, LogitechModel::GPro);
        assert!((model_ps.max_torque_nm() - 11.0).abs() < 0.05);
        assert_eq!(model_ps.max_rotation_deg(), 1080);
        Ok(())
    }

    #[test]
    fn test_model_unknown() -> Result<(), Box<dyn std::error::Error>> {
        let model = LogitechModel::from_product_id(0xDEAD);
        assert_eq!(model, LogitechModel::Unknown);
        Ok(())
    }

    #[test]
    fn test_is_wheel_product() -> Result<(), Box<dyn std::error::Error>> {
        assert!(is_wheel_product(product_ids::G920));
        assert!(is_wheel_product(product_ids::G923_XBOX));
        assert!(is_wheel_product(product_ids::G923_PS));
        assert!(is_wheel_product(product_ids::G_PRO));
        assert!(is_wheel_product(product_ids::G_PRO_XBOX));
        assert!(is_wheel_product(product_ids::G29_PS));
        assert!(!is_wheel_product(0xFFFF));
        assert!(!is_wheel_product(0x0000));
        Ok(())
    }

    #[test]
    fn test_model_g27() -> Result<(), Box<dyn std::error::Error>> {
        let model = LogitechModel::from_product_id(product_ids::G27);
        assert_eq!(model, LogitechModel::G27);
        assert!((model.max_torque_nm() - 2.5).abs() < 0.05);
        Ok(())
    }

    /// Verify that all known PIDs round-trip correctly through from_product_id → is_wheel_product.
    #[test]
    fn test_all_known_pids_are_wheels() -> Result<(), Box<dyn std::error::Error>> {
        let known_pids = [
            product_ids::MOMO,
            product_ids::DRIVING_FORCE_PRO,
            product_ids::DRIVING_FORCE_GT,
            product_ids::SPEED_FORCE_WIRELESS,
            product_ids::G25,
            product_ids::G27_A,
            product_ids::G27,
            product_ids::G29_PS,
            product_ids::G920,
            product_ids::G923,
            product_ids::G923_PS,
            product_ids::G923_XBOX,
            product_ids::G_PRO,
            product_ids::G_PRO_XBOX,
        ];
        for pid in known_pids {
            assert!(
                is_wheel_product(pid),
                "PID 0x{:04X} should be a known wheel product",
                pid
            );
            let model = LogitechModel::from_product_id(pid);
            assert_ne!(
                model,
                LogitechModel::Unknown,
                "PID 0x{:04X} should classify to a known model",
                pid
            );
        }
        Ok(())
    }

    /// Verify specific VID/PID constant values against authoritative sources
    /// (Linux kernel hid-ids.h; new-lg4ff driver; oversteer project).
    #[test]
    fn test_pid_constant_values() -> Result<(), Box<dyn std::error::Error>> {
        use crate::ids::{LOGITECH_VENDOR_ID, product_ids};
        assert_eq!(LOGITECH_VENDOR_ID, 0x046D, "Logitech VID");
        assert_eq!(
            product_ids::G25,
            0xC299,
            "G25 PID (kernel: USB_DEVICE_ID_LOGITECH_G25_WHEEL)"
        );
        assert_eq!(
            product_ids::G27,
            0xC29B,
            "G27 PID (kernel: USB_DEVICE_ID_LOGITECH_G27_WHEEL)"
        );
        assert_eq!(
            product_ids::G29_PS,
            0xC24F,
            "G29 PID (kernel: USB_DEVICE_ID_LOGITECH_G29_WHEEL)"
        );
        assert_eq!(
            product_ids::G920,
            0xC262,
            "G920 PID (kernel: USB_DEVICE_ID_LOGITECH_G920_WHEEL)"
        );
        assert_eq!(
            product_ids::G923,
            0xC266,
            "G923 native PID (new-lg4ff: USB_DEVICE_ID_LOGITECH_G923_WHEEL)"
        );
        assert_eq!(
            product_ids::G923_PS,
            0xC267,
            "G923 PS compat PID (new-lg4ff: USB_DEVICE_ID_LOGITECH_G923_PS_WHEEL)"
        );
        assert_eq!(
            product_ids::G923_XBOX,
            0xC26E,
            "G923 Xbox PID (kernel: USB_DEVICE_ID_LOGITECH_G923_XBOX_WHEEL)"
        );
        assert_eq!(
            product_ids::G_PRO,
            0xC268,
            "G PRO PS PID (oversteer: LG_GPRO_PS)"
        );
        assert_eq!(
            product_ids::G_PRO_XBOX,
            0xC272,
            "G PRO Xbox PID (oversteer: LG_GPRO_XBOX)"
        );
        Ok(())
    }
}
