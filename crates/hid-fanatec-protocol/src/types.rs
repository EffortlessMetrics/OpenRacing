//! Fanatec device model classification.

#![deny(static_mut_refs)]

use crate::ids::product_ids;

/// Fanatec wheelbase model classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanatecModel {
    /// DD1 / Podium DD1 — 20 Nm direct drive.
    Dd1,
    /// DD2 / Podium DD2 — 25 Nm direct drive.
    Dd2,
    /// CSL Elite (belt-driven, 6 Nm). Covers both standard and PS4 variants.
    CslElite,
    /// CSL DD — direct drive, 5/8 Nm (boost kit dependent).
    CslDd,
    /// Gran Turismo DD Pro — direct drive, 5/8 Nm.
    GtDdPro,
    /// ClubSport V2 (legacy USB stack, 8 Nm).
    ClubSportV2,
    /// ClubSport V2.5 (belt-driven, 8 Nm).
    ClubSportV25,
    /// Unknown or future Fanatec wheelbase.
    Unknown,
}

impl FanatecModel {
    /// Classify a device by its product ID.
    pub fn from_product_id(product_id: u16) -> Self {
        match product_id {
            product_ids::DD1 => Self::Dd1,
            product_ids::DD2 | product_ids::DD2_VARIANT => Self::Dd2,
            product_ids::CSL_ELITE | product_ids::CSL_ELITE_PS4 => Self::CslElite,
            product_ids::CSL_DD | product_ids::CSL_DD_LEGACY => Self::CslDd,
            product_ids::GT_DD_PRO => Self::GtDdPro,
            product_ids::CLUBSPORT_V2_LEGACY => Self::ClubSportV2,
            product_ids::CLUBSPORT_V2_5 => Self::ClubSportV25,
            _ => Self::Unknown,
        }
    }

    /// Maximum continuous torque in Newton-meters for this model.
    pub fn max_torque_nm(self) -> f32 {
        match self {
            Self::Dd1 => 20.0,
            Self::Dd2 => 25.0,
            Self::CslElite => 6.0,
            Self::CslDd | Self::GtDdPro => 8.0,
            Self::ClubSportV2 | Self::ClubSportV25 => 8.0,
            Self::Unknown => 5.0,
        }
    }

    /// Encoder counts per revolution.
    pub fn encoder_cpr(self) -> u32 {
        match self {
            Self::Dd1 | Self::Dd2 => 16_384,
            Self::CslDd | Self::GtDdPro => 16_384,
            _ => 4_096,
        }
    }
}

/// Return `true` if the product ID corresponds to a wheelbase (not pedals or rim accessories).
pub fn is_wheelbase_product(product_id: u16) -> bool {
    matches!(
        product_id,
        product_ids::DD1
            | product_ids::DD2
            | product_ids::DD2_VARIANT
            | product_ids::CSL_ELITE_PS4
            | product_ids::CLUBSPORT_V2_5
            | product_ids::CSL_DD_LEGACY
            | product_ids::CSL_DD
            | product_ids::GT_DD_PRO
            | product_ids::CSL_ELITE
            | product_ids::CLUBSPORT_V2_LEGACY
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_wheelbase_product_known() -> Result<(), Box<dyn std::error::Error>> {
        assert!(is_wheelbase_product(product_ids::DD1));
        assert!(is_wheelbase_product(product_ids::DD2));
        assert!(is_wheelbase_product(product_ids::DD2_VARIANT));
        assert!(is_wheelbase_product(product_ids::CSL_DD));
        assert!(is_wheelbase_product(product_ids::GT_DD_PRO));
        assert!(is_wheelbase_product(product_ids::CSL_ELITE));
        assert!(is_wheelbase_product(product_ids::CLUBSPORT_V2_LEGACY));
        Ok(())
    }

    #[test]
    fn test_is_wheelbase_product_unknown() -> Result<(), Box<dyn std::error::Error>> {
        assert!(!is_wheelbase_product(0xFFFF));
        assert!(!is_wheelbase_product(0x0000));
        Ok(())
    }

    #[test]
    fn test_model_dd1() -> Result<(), Box<dyn std::error::Error>> {
        let model = FanatecModel::from_product_id(product_ids::DD1);
        assert_eq!(model, FanatecModel::Dd1);
        assert!((model.max_torque_nm() - 20.0).abs() < 0.1);
        assert_eq!(model.encoder_cpr(), 16_384);
        Ok(())
    }

    #[test]
    fn test_model_csl_dd() -> Result<(), Box<dyn std::error::Error>> {
        let model = FanatecModel::from_product_id(product_ids::CSL_DD);
        assert_eq!(model, FanatecModel::CslDd);
        assert!((model.max_torque_nm() - 8.0).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn test_model_unknown() -> Result<(), Box<dyn std::error::Error>> {
        let model = FanatecModel::from_product_id(0xDEAD);
        assert_eq!(model, FanatecModel::Unknown);
        assert!((model.max_torque_nm() - 5.0).abs() < 0.1);
        Ok(())
    }
}
