//! PXN device model classification and capabilities.

/// PXN wheel model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PxnModel {
    /// PXN V10 – 10 Nm direct drive.
    V10,
    /// PXN V12 – 12 Nm direct drive.
    V12,
    /// PXN V12 Lite – 12 Nm compact direct drive.
    V12Lite,
    /// PXN V12 Lite SE – 12 Nm compact SE variant.
    V12LiteSe,
    /// GT987 FF – Lite Star OEM variant.
    Gt987Ff,
}

impl PxnModel {
    /// Construct a model from a USB product ID, returning `None` for unknown PIDs.
    pub fn from_pid(pid: u16) -> Option<Self> {
        match pid {
            crate::ids::PRODUCT_V10 => Some(PxnModel::V10),
            crate::ids::PRODUCT_V12 => Some(PxnModel::V12),
            crate::ids::PRODUCT_V12_LITE => Some(PxnModel::V12Lite),
            crate::ids::PRODUCT_V12_LITE_SE => Some(PxnModel::V12LiteSe),
            crate::ids::PRODUCT_GT987_FF => Some(PxnModel::Gt987Ff),
            _ => None,
        }
    }

    /// Maximum continuous torque in Newton-metres.
    pub fn max_torque_nm(self) -> f32 {
        match self {
            PxnModel::V10 => 10.0,
            PxnModel::V12 => 12.0,
            PxnModel::V12Lite => 12.0,
            PxnModel::V12LiteSe => 12.0,
            // GT987 FF torque spec not officially published; 5 Nm is a conservative estimate.
            PxnModel::Gt987Ff => 5.0,
        }
    }

    /// Human-readable product name.
    pub fn name(self) -> &'static str {
        match self {
            PxnModel::V10 => "PXN V10",
            PxnModel::V12 => "PXN V12",
            PxnModel::V12Lite => "PXN V12 Lite",
            PxnModel::V12LiteSe => "PXN V12 Lite SE",
            PxnModel::Gt987Ff => "GT987 FF (Lite Star OEM)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{
        PRODUCT_GT987_FF, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE, PRODUCT_V12_LITE_SE,
    };

    #[test]
    fn from_pid_known() {
        assert_eq!(PxnModel::from_pid(PRODUCT_V10), Some(PxnModel::V10));
        assert_eq!(PxnModel::from_pid(PRODUCT_V12), Some(PxnModel::V12));
        assert_eq!(
            PxnModel::from_pid(PRODUCT_V12_LITE),
            Some(PxnModel::V12Lite)
        );
        assert_eq!(
            PxnModel::from_pid(PRODUCT_V12_LITE_SE),
            Some(PxnModel::V12LiteSe)
        );
        assert_eq!(
            PxnModel::from_pid(PRODUCT_GT987_FF),
            Some(PxnModel::Gt987Ff)
        );
    }

    #[test]
    fn from_pid_unknown() {
        assert_eq!(PxnModel::from_pid(0xFFFF), None);
    }

    #[test]
    fn torque_values() {
        assert!((PxnModel::V10.max_torque_nm() - 10.0).abs() < 0.001);
        assert!((PxnModel::V12.max_torque_nm() - 12.0).abs() < 0.001);
        assert!((PxnModel::V12Lite.max_torque_nm() - 12.0).abs() < 0.001);
        assert!((PxnModel::V12LiteSe.max_torque_nm() - 12.0).abs() < 0.001);
        assert!(PxnModel::Gt987Ff.max_torque_nm() > 0.0);
    }

    #[test]
    fn names() {
        assert_eq!(PxnModel::V10.name(), "PXN V10");
        assert_eq!(PxnModel::V12.name(), "PXN V12");
        assert_eq!(PxnModel::V12Lite.name(), "PXN V12 Lite");
        assert_eq!(PxnModel::V12LiteSe.name(), "PXN V12 Lite SE");
        assert_eq!(PxnModel::Gt987Ff.name(), "GT987 FF (Lite Star OEM)");
    }
}
