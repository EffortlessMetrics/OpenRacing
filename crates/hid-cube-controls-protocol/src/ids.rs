//! Cube Controls USB vendor and product ID constants.
//!
//! # Status: PROVISIONAL — UNVERIFIED
//!
//! **Important:** Cube Controls S.r.l. (Italy) manufactures premium sim-racing
//! **steering wheels** (button boxes / rims), **not** wheelbases. Their products
//! (GT Pro V2, Formula CSX-3, GT-X2, F-CORE, etc.) are USB/Bluetooth input
//! devices with buttons, rotary encoders, and paddles. They do **not** produce
//! force feedback. Force feedback comes from the wheelbase (a separate device
//! by another vendor).
//!
//! VID `0x0483` is the STMicroelectronics shared VID used by thousands of
//! STM32-based USB devices. Community reports suggest Cube Controls steering
//! wheels enumerate on this VID, but no official USB descriptor capture or SDK
//! confirms it. The PIDs below are internal estimates — they are **not** from a
//! USB device tree capture and may be incorrect.
//!
//! **Research pass (2025-06 / 2025-07):** The following sources were checked:
//! - JacKeTUs/linux-steering-wheels: **no** Cube Controls entries
//!   <https://github.com/JacKeTUs/linux-steering-wheels>
//! - JacKeTUs/simracing-hwdb: **no** Cube Controls hwdb file
//!   <https://github.com/JacKeTUs/simracing-hwdb>
//! - RetroBat Wheels.cs: **no** Cube Controls entries
//!   <https://github.com/RetroBat-Official/emulatorlauncher/blob/master/emulatorLauncher/Common/Wheels.cs>
//! - devicehunt.com (VID 0x0483): PIDs 0x0C73–0x0C75 **not registered**
//! - cubecontrols.com: no USB VID/PID information published; product pages
//!   provide specs but omit USB identifiers (checked GT Pro V2, Formula CSX-3,
//!   GT-X2)
//! - Linux kernel hid-ids.h: no Cube Controls entries
//! - SDL GameControllerDB: no Cube Controls entries
//! - GitHub code search: no independent USB captures found
//! - Reddit/RaceDepartment forums: no USB descriptor reports found
//! - EffortlessMetrics/OpenFlight: uses **different** VID/PID estimates
//!   (VID 0x0EB7 / PID 0x0E03) — also unconfirmed, likely incorrect
//!
//! **Known products not yet represented:**
//! - GT-X2 (round wheel with 5" touchscreen, USB Q-CONN)
//! - F-CORE (flagship formula-style wheel)
//! - SP-01 (pedal set, input-only)
//!
//! These will be added when real USB captures provide VID/PID data.
//!
//! TODO(web-verify): Obtain a USB descriptor capture (`lsusb -v` or
//! USBTreeView) from real Cube Controls hardware to confirm or replace
//! the provisional VID/PIDs below.
//!
//! See `docs/protocols/SOURCES.md` for policy.
//!
//! ACTION REQUIRED: Once confirmed from real hardware (e.g. `lsusb -v` or
//! USBTreeView capture), update the constants below and remove the PROVISIONAL
//! annotations.

/// Cube Controls USB Vendor ID.
///
/// **PROVISIONAL** — VID 0x0483 is the STMicroelectronics shared VID used by
/// many STM32-based devices (including Simagic 0x0522 and VRS 0xa355 in
/// sim-racing). Cube Controls steering wheels use STM32 MCUs, so this VID is
/// plausible but has not been confirmed from hardware captures.
pub const CUBE_CONTROLS_VENDOR_ID: u16 = 0x0483;

/// Cube Controls GT Pro product ID (provisional — not confirmed from hardware).
pub const CUBE_CONTROLS_GT_PRO_PID: u16 = 0x0C73;

/// Cube Controls Formula Pro product ID (provisional — not confirmed from hardware).
pub const CUBE_CONTROLS_FORMULA_PRO_PID: u16 = 0x0C74;

/// Cube Controls CSX3 product ID (provisional — not confirmed from hardware).
pub const CUBE_CONTROLS_CSX3_PID: u16 = 0x0C75;

/// Cube Controls model classification.
///
/// Note: Cube Controls products are steering wheel button boxes (input devices),
/// not wheelbases. They do not produce force feedback. The `max_torque_nm()`
/// method returns a placeholder value for API compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CubeControlsModel {
    /// GT Pro — F1-style wireless steering wheel / button box (provisional PID)
    GtPro,
    /// Formula Pro — Formula racing steering wheel / button box (provisional PID)
    FormulaPro,
    /// CSX3 — High-end steering wheel with 4" touchscreen (provisional PID)
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

    /// Placeholder torque value in Nm (for API compatibility).
    ///
    /// Cube Controls products are steering wheels (input devices), not
    /// wheelbases. They do not produce force feedback torque. This value
    /// exists only to satisfy the `VendorProtocol` interface and should
    /// not be used for FFB force scaling. Returns 0.0 for safety.
    pub fn max_torque_nm(self) -> f32 {
        // These are input-only devices — torque is not applicable.
        // Return 0.0 to prevent accidentally scaling FFB output.
        0.0
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
        assert_eq!(
            CubeControlsModel::GtPro.display_name(),
            "Cube Controls GT Pro"
        );
        assert_eq!(
            CubeControlsModel::FormulaPro.display_name(),
            "Cube Controls Formula Pro"
        );
        assert_eq!(CubeControlsModel::Csx3.display_name(), "Cube Controls CSX3");
        assert!(!CubeControlsModel::Unknown.display_name().is_empty());
    }

    #[test]
    fn torque_is_zero_for_input_devices() {
        // Cube Controls products are steering wheels (input-only), not force
        // feedback devices — torque should be 0.0.
        assert!((CubeControlsModel::GtPro.max_torque_nm() - 0.0).abs() < f32::EPSILON);
        assert!((CubeControlsModel::FormulaPro.max_torque_nm() - 0.0).abs() < f32::EPSILON);
        assert!((CubeControlsModel::Csx3.max_torque_nm() - 0.0).abs() < f32::EPSILON);
        assert!((CubeControlsModel::Unknown.max_torque_nm() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn all_models_provisional() {
        assert!(CubeControlsModel::GtPro.is_provisional());
        assert!(CubeControlsModel::FormulaPro.is_provisional());
        assert!(CubeControlsModel::Csx3.is_provisional());
        assert!(CubeControlsModel::Unknown.is_provisional());
    }
}
