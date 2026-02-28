//! Cube Controls USB vendor and product ID constants.
//!
//! # Status: PROVISIONAL
//!
//! VID `0x0483` is the STMicroelectronics shared VID used by a large number of
//! STM32-based USB HID devices. Multiple community sources report Cube Controls
//! wheels on this VID, but no official USB descriptor capture or SDK confirms
//! it. The PIDs below are internal estimates — they are **not** from a USB
//! device tree capture and may be incorrect.
//!
//! The JacKeTUs/linux-steering-wheels compatibility table (the primary community
//! reference used by this project) contains no Cube Controls entries as of the
//! last research pass (2025-01). See `docs/protocols/SOURCES.md` for policy.
//!
//! ACTION REQUIRED: Once confirmed from real hardware (e.g. `lsusb -v` or
//! USBTreeView capture), update the constants below and remove the PROVISIONAL
//! annotations.

/// Cube Controls USB Vendor ID.
///
/// **PROVISIONAL** — VID 0x0483 is the STMicroelectronics shared VID used by
/// many STM32-based devices. Community reports suggest Cube Controls devices
/// use this VID, but it has not been confirmed from hardware captures.
pub const CUBE_CONTROLS_VENDOR_ID: u16 = 0x0483;

/// Cube Controls GT Pro product ID (provisional — not confirmed from hardware).
pub const CUBE_CONTROLS_GT_PRO_PID: u16 = 0x0C73;

/// Cube Controls Formula Pro product ID (provisional — not confirmed from hardware).
pub const CUBE_CONTROLS_FORMULA_PRO_PID: u16 = 0x0C74;

/// Cube Controls CSX3 product ID (provisional — not confirmed from hardware).
pub const CUBE_CONTROLS_CSX3_PID: u16 = 0x0C75;

/// Cube Controls model classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CubeControlsModel {
    /// GT Pro — F1-style wheel, up to ~20 Nm (provisional PID)
    GtPro,
    /// Formula Pro — Formula racing wheel, up to ~20 Nm (provisional PID)
    FormulaPro,
    /// CSX3 — High-end customizable wheel, up to ~20 Nm (provisional PID)
    Csx3,
    /// Future or unrecognised Cube Controls product
    Unknown,
}

impl CubeControlsModel {
    /// Resolve model from a USB product ID.
    pub fn from_product_id(pid: u16) -> Self {
        match pid {
            CUBE_CONTROLS_GT_PRO_PID => Self::GtPro,
            CUBE_CONTROLS_FORMULA_PRO_PID => Self::FormulaPro,
            CUBE_CONTROLS_CSX3_PID => Self::Csx3,
            _ => Self::Unknown,
        }
    }

    /// Human-readable product name.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::GtPro => "Cube Controls GT Pro",
            Self::FormulaPro => "Cube Controls Formula Pro",
            Self::Csx3 => "Cube Controls CSX3",
            Self::Unknown => "Cube Controls (unknown model)",
        }
    }

    /// Rated peak torque in Nm.
    ///
    /// All current Cube Controls models are rated at approximately 20 Nm.
    /// The exact value per model is not publicly documented.
    pub fn max_torque_nm(self) -> f32 {
        match self {
            Self::GtPro | Self::FormulaPro | Self::Csx3 => 20.0,
            Self::Unknown => 20.0, // conservative default
        }
    }

    /// Returns `true` for all models while VID/PIDs remain unconfirmed.
    pub fn is_provisional(self) -> bool {
        true
    }
}

/// Returns `true` when `product_id` is a provisionally known Cube Controls PID.
pub fn is_cube_controls_product(product_id: u16) -> bool {
    matches!(
        product_id,
        CUBE_CONTROLS_GT_PRO_PID | CUBE_CONTROLS_FORMULA_PRO_PID | CUBE_CONTROLS_CSX3_PID
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_pids_recognised() {
        assert!(is_cube_controls_product(CUBE_CONTROLS_GT_PRO_PID));
        assert!(is_cube_controls_product(CUBE_CONTROLS_FORMULA_PRO_PID));
        assert!(is_cube_controls_product(CUBE_CONTROLS_CSX3_PID));
    }

    #[test]
    fn unknown_pid_not_recognised() {
        assert!(!is_cube_controls_product(0x0001));
        assert!(!is_cube_controls_product(0x0522)); // Simagic legacy
        assert!(!is_cube_controls_product(0xFFFF));
    }

    #[test]
    fn model_from_pid_known() {
        assert_eq!(
            CubeControlsModel::from_product_id(CUBE_CONTROLS_GT_PRO_PID),
            CubeControlsModel::GtPro
        );
        assert_eq!(
            CubeControlsModel::from_product_id(CUBE_CONTROLS_FORMULA_PRO_PID),
            CubeControlsModel::FormulaPro
        );
        assert_eq!(
            CubeControlsModel::from_product_id(CUBE_CONTROLS_CSX3_PID),
            CubeControlsModel::Csx3
        );
    }

    #[test]
    fn model_from_pid_unknown() {
        assert_eq!(
            CubeControlsModel::from_product_id(0xFFFF),
            CubeControlsModel::Unknown
        );
    }

    #[test]
    fn display_names_non_empty() {
        assert_eq!(CubeControlsModel::GtPro.display_name(), "Cube Controls GT Pro");
        assert_eq!(
            CubeControlsModel::FormulaPro.display_name(),
            "Cube Controls Formula Pro"
        );
        assert_eq!(CubeControlsModel::Csx3.display_name(), "Cube Controls CSX3");
        assert!(!CubeControlsModel::Unknown.display_name().is_empty());
    }

    #[test]
    fn torque_always_positive() {
        assert!(CubeControlsModel::GtPro.max_torque_nm() > 0.0);
        assert!(CubeControlsModel::FormulaPro.max_torque_nm() > 0.0);
        assert!(CubeControlsModel::Csx3.max_torque_nm() > 0.0);
        assert!(CubeControlsModel::Unknown.max_torque_nm() > 0.0);
    }

    #[test]
    fn all_models_provisional() {
        assert!(CubeControlsModel::GtPro.is_provisional());
        assert!(CubeControlsModel::FormulaPro.is_provisional());
        assert!(CubeControlsModel::Csx3.is_provisional());
        assert!(CubeControlsModel::Unknown.is_provisional());
    }
}
