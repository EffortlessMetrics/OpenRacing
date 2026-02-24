//! Logitech device model classification.

#![deny(static_mut_refs)]

use crate::ids::product_ids;

/// Logitech wheel model classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogitechModel {
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
    /// G PRO racing wheel (2.2 Nm, 900°).
    GPro,
    /// Pro Racing Wheel (direct-drive, 11 Nm, 1080°, TrueForce).
    ProRacing,
    /// Unknown or future Logitech wheel.
    Unknown,
}

impl LogitechModel {
    /// Classify a device by its product ID.
    pub fn from_product_id(product_id: u16) -> Self {
        match product_id {
            product_ids::G25 => Self::G25,
            product_ids::G27_A | product_ids::G27 => Self::G27,
            product_ids::G29_PS | product_ids::G29_XBOX => Self::G29,
            product_ids::G920_V1 | product_ids::G920 => Self::G920,
            product_ids::G923_XBOX | product_ids::G923_PS => Self::G923,
            product_ids::G_PRO => Self::GPro,
            product_ids::PRO_RACING => Self::ProRacing,
            _ => Self::Unknown,
        }
    }

    /// Maximum continuous torque in Newton-meters for this model.
    pub fn max_torque_nm(self) -> f32 {
        match self {
            Self::G25 | Self::G27 => 2.5,
            Self::G29 | Self::G920 | Self::G923 | Self::GPro => 2.2,
            Self::ProRacing => 11.0,
            Self::Unknown => 2.0,
        }
    }

    /// Maximum wheel rotation in degrees.
    pub fn max_rotation_deg(self) -> u16 {
        match self {
            Self::ProRacing => 1080,
            _ => 900,
        }
    }

    /// Whether this model supports TrueForce haptics.
    pub fn supports_trueforce(self) -> bool {
        matches!(self, Self::G923 | Self::ProRacing)
    }
}

/// Return `true` if the product ID corresponds to a known Logitech wheel.
pub fn is_wheel_product(product_id: u16) -> bool {
    matches!(
        product_id,
        product_ids::G25
            | product_ids::G27_A
            | product_ids::G27
            | product_ids::G29_PS
            | product_ids::G29_XBOX
            | product_ids::G920_V1
            | product_ids::G920
            | product_ids::G923_XBOX
            | product_ids::G923_PS
            | product_ids::G_PRO
            | product_ids::PRO_RACING
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
    fn test_model_pro_racing() -> Result<(), Box<dyn std::error::Error>> {
        let model = LogitechModel::from_product_id(product_ids::PRO_RACING);
        assert_eq!(model, LogitechModel::ProRacing);
        assert_eq!(model.max_rotation_deg(), 1080);
        assert!((model.max_torque_nm() - 11.0).abs() < 0.1);
        assert!(model.supports_trueforce());
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
        assert!(is_wheel_product(product_ids::PRO_RACING));
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
}
