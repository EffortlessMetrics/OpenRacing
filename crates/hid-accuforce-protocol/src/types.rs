//! AccuForce device classification and capabilities.

use crate::ids::PID_ACCUFORCE_PRO;

/// SimExperience AccuForce product family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccuForceModel {
    /// SimExperience AccuForce Pro (~7 Nm, 100–200 Hz USB update rate).
    Pro,
    /// Future or unrecognised AccuForce product.
    Unknown,
}

impl AccuForceModel {
    /// Resolve model from a USB product ID.
    pub fn from_product_id(pid: u16) -> Self {
        match pid {
            PID_ACCUFORCE_PRO => Self::Pro,
            _ => Self::Unknown,
        }
    }

    /// Human-readable product name.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Pro => "SimExperience AccuForce Pro",
            Self::Unknown => "SimExperience AccuForce (unknown model)",
        }
    }

    /// Rated peak torque in Newton-metres.
    ///
    /// The AccuForce Pro V1 motor is rated at ~7 Nm peak.  The V2 revision
    /// uses a larger motor commonly cited at ~13 Nm peak by community
    /// sources; however SimXperience's product page is no longer available
    /// to confirm the exact figure.  Using the conservative V1 value here
    /// until an authoritative V2 datasheet is found.
    pub fn max_torque_nm(self) -> f32 {
        match self {
            Self::Pro => 7.0,
            Self::Unknown => 7.0,
        }
    }
}

/// Resolved identity and capabilities of a connected AccuForce device.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceInfo {
    /// USB Vendor ID (expected: [`crate::ids::VENDOR_ID`]).
    pub vendor_id: u16,
    /// USB Product ID.
    pub product_id: u16,
    /// Resolved model classification.
    pub model: AccuForceModel,
}

impl DeviceInfo {
    /// Build a [`DeviceInfo`] from a VID/PID pair.
    pub fn from_vid_pid(vendor_id: u16, product_id: u16) -> Self {
        Self {
            vendor_id,
            product_id,
            model: AccuForceModel::from_product_id(product_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{PID_ACCUFORCE_PRO, VENDOR_ID};

    #[test]
    fn pro_pid_resolves_to_pro_model() {
        assert_eq!(
            AccuForceModel::from_product_id(PID_ACCUFORCE_PRO),
            AccuForceModel::Pro
        );
    }

    #[test]
    fn unknown_pid_resolves_to_unknown() {
        assert_eq!(
            AccuForceModel::from_product_id(0xFFFF),
            AccuForceModel::Unknown
        );
        assert_eq!(
            AccuForceModel::from_product_id(0x0000),
            AccuForceModel::Unknown
        );
    }

    #[test]
    fn display_names_are_non_empty() {
        assert!(!AccuForceModel::Pro.display_name().is_empty());
        assert!(!AccuForceModel::Unknown.display_name().is_empty());
    }

    #[test]
    fn max_torque_is_positive() {
        assert!(AccuForceModel::Pro.max_torque_nm() > 0.0);
        assert!(AccuForceModel::Unknown.max_torque_nm() > 0.0);
    }

    #[test]
    fn device_info_from_vid_pid() {
        let info = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
        assert_eq!(info.vendor_id, VENDOR_ID);
        assert_eq!(info.product_id, PID_ACCUFORCE_PRO);
        assert_eq!(info.model, AccuForceModel::Pro);
    }
}
