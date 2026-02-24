//! Fanatec device model classification.

#![deny(static_mut_refs)]

use crate::ids::{product_ids, rim_ids};

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
            product_ids::CLUBSPORT_V2 | product_ids::CLUBSPORT_V2_LEGACY => Self::ClubSportV2,
            product_ids::CSL_ELITE_BASE | product_ids::CSL_ELITE => Self::CslElite,
            product_ids::CLUBSPORT_V2_5 => Self::ClubSportV25,
            product_ids::DD1 => Self::Dd1,
            product_ids::DD2 => Self::Dd2,
            product_ids::CSL_DD | product_ids::CSL_DD_LEGACY => Self::CslDd,
            product_ids::GT_DD_PRO => Self::GtDdPro,
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
        product_ids::CLUBSPORT_V2
            | product_ids::CSL_ELITE_BASE
            | product_ids::CLUBSPORT_V2_5
            | product_ids::DD1
            | product_ids::DD2
            | product_ids::CSL_DD_LEGACY
            | product_ids::CSL_DD
            | product_ids::GT_DD_PRO
            | product_ids::CSL_ELITE
            | product_ids::CLUBSPORT_V2_LEGACY
    )
}

/// Return `true` if the product ID corresponds to a standalone pedal device.
pub fn is_pedal_product(product_id: u16) -> bool {
    matches!(
        product_id,
        product_ids::CLUBSPORT_PEDALS_V1_V2
            | product_ids::CLUBSPORT_PEDALS_V3
            | product_ids::CSL_PEDALS_LC
            | product_ids::CSL_PEDALS_V2
    )
}

/// Fanatec standalone pedal device model classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanatecPedalModel {
    /// ClubSport Pedals V1 or V2 (2-pedal or 3-pedal set).
    ClubSportV1V2,
    /// ClubSport Pedals V3 with load cell brake.
    ClubSportV3,
    /// CSL Pedals with Load Cell Kit.
    CslPedalsLc,
    /// CSL Pedals V2.
    CslPedalsV2,
    /// Unknown or future pedal device.
    Unknown,
}

impl FanatecPedalModel {
    /// Classify a pedal device from its product ID.
    pub fn from_product_id(product_id: u16) -> Self {
        match product_id {
            product_ids::CLUBSPORT_PEDALS_V1_V2 => Self::ClubSportV1V2,
            product_ids::CLUBSPORT_PEDALS_V3 => Self::ClubSportV3,
            product_ids::CSL_PEDALS_LC => Self::CslPedalsLc,
            product_ids::CSL_PEDALS_V2 => Self::CslPedalsV2,
            _ => Self::Unknown,
        }
    }

    /// Number of analog axes for this pedal model (throttle + brake [+ clutch]).
    pub fn axis_count(self) -> u8 {
        match self {
            Self::ClubSportV3 | Self::CslPedalsLc | Self::CslPedalsV2 => 3,
            Self::ClubSportV1V2 => 2,
            Self::Unknown => 2,
        }
    }
}

/// Steering wheel rim IDs as reported in feature report 0x02, byte 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanatecRimId {
    BmwGt2,
    FormulaV2,
    FormulaV25,
    /// McLaren GT3 V2 — funky switch + rotary encoders + dual clutch paddles.
    McLarenGt3V2,
    Porsche918Rsr,
    ClubSportRs,
    Wrc,
    CslEliteP1,
    PodiumHub,
    /// Rim is not attached or ID is unrecognised.
    Unknown,
}

impl FanatecRimId {
    /// Decode a rim ID byte from the device-info feature report.
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            rim_ids::BMW_GT2 => Self::BmwGt2,
            rim_ids::FORMULA_V2 => Self::FormulaV2,
            rim_ids::FORMULA_V2_5 => Self::FormulaV25,
            rim_ids::MCLAREN_GT3_V2 => Self::McLarenGt3V2,
            rim_ids::PORSCHE_918_RSR => Self::Porsche918Rsr,
            rim_ids::CLUBSPORT_RS => Self::ClubSportRs,
            rim_ids::WRC => Self::Wrc,
            rim_ids::CSL_ELITE_P1 => Self::CslEliteP1,
            rim_ids::PODIUM_HUB => Self::PodiumHub,
            _ => Self::Unknown,
        }
    }

    /// Return `true` if this rim has a funky switch (multidirectional D-pad with rotation).
    pub fn has_funky_switch(self) -> bool {
        matches!(self, Self::McLarenGt3V2)
    }

    /// Return `true` if this rim has dual-clutch paddles.
    pub fn has_dual_clutch(self) -> bool {
        matches!(self, Self::FormulaV2 | Self::FormulaV25 | Self::McLarenGt3V2)
    }

    /// Return `true` if this rim has rotary encoders (beyond the standard hat switch).
    pub fn has_rotary_encoders(self) -> bool {
        matches!(self, Self::McLarenGt3V2 | Self::FormulaV25)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_wheelbase_product_known() -> Result<(), Box<dyn std::error::Error>> {
        assert!(is_wheelbase_product(product_ids::CLUBSPORT_V2));
        assert!(is_wheelbase_product(product_ids::CSL_ELITE_BASE));
        assert!(is_wheelbase_product(product_ids::CLUBSPORT_V2_5));
        assert!(is_wheelbase_product(product_ids::DD1));
        assert!(is_wheelbase_product(product_ids::DD2));
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

    #[test]
    fn test_is_pedal_product() -> Result<(), Box<dyn std::error::Error>> {
        assert!(is_pedal_product(product_ids::CLUBSPORT_PEDALS_V3));
        assert!(is_pedal_product(product_ids::CLUBSPORT_PEDALS_V1_V2));
        assert!(is_pedal_product(product_ids::CSL_PEDALS_LC));
        assert!(is_pedal_product(product_ids::CSL_PEDALS_V2));
        assert!(!is_pedal_product(product_ids::CSL_DD));
        assert!(!is_pedal_product(product_ids::DD1));
        assert!(!is_pedal_product(0xFFFF));
        Ok(())
    }

    #[test]
    fn test_pedal_model_from_product_id() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            FanatecPedalModel::from_product_id(product_ids::CLUBSPORT_PEDALS_V3),
            FanatecPedalModel::ClubSportV3
        );
        assert_eq!(
            FanatecPedalModel::from_product_id(product_ids::CLUBSPORT_PEDALS_V1_V2),
            FanatecPedalModel::ClubSportV1V2
        );
        assert_eq!(
            FanatecPedalModel::from_product_id(product_ids::CSL_PEDALS_LC),
            FanatecPedalModel::CslPedalsLc
        );
        assert_eq!(
            FanatecPedalModel::from_product_id(0xDEAD),
            FanatecPedalModel::Unknown
        );
        Ok(())
    }

    #[test]
    fn test_pedal_model_axis_count() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(FanatecPedalModel::ClubSportV3.axis_count(), 3);
        assert_eq!(FanatecPedalModel::ClubSportV1V2.axis_count(), 2);
        assert_eq!(FanatecPedalModel::CslPedalsLc.axis_count(), 3);
        Ok(())
    }

    #[test]
    fn test_rim_id_mclaren_from_byte() -> Result<(), Box<dyn std::error::Error>> {
        let rim = FanatecRimId::from_byte(rim_ids::MCLAREN_GT3_V2);
        assert_eq!(rim, FanatecRimId::McLarenGt3V2);
        assert!(rim.has_funky_switch());
        assert!(rim.has_dual_clutch());
        assert!(rim.has_rotary_encoders());
        Ok(())
    }

    #[test]
    fn test_rim_id_unknown_byte() -> Result<(), Box<dyn std::error::Error>> {
        let rim = FanatecRimId::from_byte(0xFF);
        assert_eq!(rim, FanatecRimId::Unknown);
        assert!(!rim.has_funky_switch());
        assert!(!rim.has_dual_clutch());
        Ok(())
    }

    #[test]
    fn test_rim_id_formula_v2_has_dual_clutch() -> Result<(), Box<dyn std::error::Error>> {
        let rim = FanatecRimId::from_byte(rim_ids::FORMULA_V2);
        assert!(rim.has_dual_clutch());
        assert!(!rim.has_funky_switch());
        Ok(())
    }
}
