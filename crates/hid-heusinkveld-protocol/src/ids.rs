//! Device IDs for Heusinkveld products.
//!
//! ## VID history
//!
//! Heusinkveld products use **multiple USB Vendor IDs** depending on the
//! hardware platform revision:
//!
//! | VID    | Chip vendor      | Products using this VID |
//! |--------|------------------|------------------------|
//! | 0x30B7 | Unknown (current)| Sprint, Ultimate, Handbrake V2 |
//! | 0x04D8 | Microchip        | Legacy/older firmware revisions |
//! | 0x10C4 | Silicon Labs     | Original Handbrake (V1) |
//! | 0xA020 | Unknown          | Sequential Shifter |
//!
//! ## Verification status (2025-07, updated 2026-03)
//!
//! | Field | Confidence | Sources |
//! |-------|------------|---------|
//! | VID 0x30B7 | 🔶 Community | JacKeTUs/simracing-hwdb `90-heusinkveld.hwdb` |
//! | VID 0x04D8 | ✅ Confirmed VID (Microchip) | the-sz.com, devicehunt.com |
//! | Sprint PID 0x1001 (VID 0x30B7) | 🔶 Community | JacKeTUs/simracing-hwdb (1 source) |
//! | Ultimate PID 0x1003 (VID 0x30B7) | 🔶 Community | JacKeTUs/simracing-hwdb (1 source) |
//! | Handbrake V2 PID 0x1002 (VID 0x30B7) | 🔶 Community | JacKeTUs/simracing-hwdb (1 source) |
//! | Handbrake PID 0x8B82 (VID 0x10C4) | 🔶 Community | JacKeTUs/simracing-hwdb (1 source) |
//! | Sequential Shifter PID 0x3142 (VID 0xA020) | 🔶 Community | JacKeTUs/simracing-hwdb (1 source) |
//! | Legacy Sprint PID 0xF6D0 (VID 0x04D8) | 🔶 Community | OpenFlight YAML (1 source) |
//! | Legacy Ultimate PID 0xF6D2 (VID 0x04D8) | 🔶 Community | OpenFlight YAML (1 source) |
//! | Pro PID 0xF6D3 (VID 0x04D8) | ⚠ Estimated | Sequential guess; **zero external evidence** |
//!
//! ## Source priority
//!
//! The JacKeTUs/simracing-hwdb data (VID 0x30B7) is preferred over the
//! OpenFlight YAML data (VID 0x04D8) because:
//! - simracing-hwdb is a widely-used, community-maintained Linux hwdb
//! - Multiple VIDs suggest a hardware revision; 0x30B7 may be current firmware
//! - The old VID 0x04D8 (Microchip) is shared by thousands of devices
//!
//! Both VID sets are supported for maximum plug-and-play compatibility.
//!
//! **Note:** These are **pedal** devices — no torque/Nm values apply.

// ── Current VID (from simracing-hwdb) ────────────────────────────────────────

/// Primary Heusinkveld USB Vendor ID (current hardware).
///
/// 🔶 Community-sourced: JacKeTUs/simracing-hwdb `90-heusinkveld.hwdb`.
/// Used by Sprint pedals, Ultimate pedals, and Handbrake V2.
pub const HEUSINKVELD_VENDOR_ID: u16 = 0x30B7;

/// Legacy Heusinkveld USB Vendor ID (Microchip Technology licensed VID).
///
/// VID `0x04D8` is shared by many Microchip PIC-based devices.
/// Used by older firmware revisions of Heusinkveld pedals.
/// ✅ VID confirmed by: the-sz.com, devicehunt.com (Microchip Technology, Inc.).
pub const HEUSINKVELD_LEGACY_VENDOR_ID: u16 = 0x04D8;

/// Heusinkveld Handbrake V1 VID (Silicon Labs).
///
/// 🔶 Community-sourced: JacKeTUs/simracing-hwdb.
pub const HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID: u16 = 0x10C4;

/// Heusinkveld Sequential Shifter VID.
///
/// 🔶 Community-sourced: JacKeTUs/simracing-hwdb.
pub const HEUSINKVELD_SHIFTER_VENDOR_ID: u16 = 0xA020;

// ── Current PIDs (VID 0x30B7, from simracing-hwdb) ──────────────────────────

/// Heusinkveld Sprint pedals (current firmware).
///
/// 🔶 Community-sourced: JacKeTUs/simracing-hwdb — VID 0x30B7, PID 0x1001.
pub const HEUSINKVELD_SPRINT_PID: u16 = 0x1001;

/// Heusinkveld Handbrake V2.
///
/// 🔶 Community-sourced: JacKeTUs/simracing-hwdb — VID 0x30B7, PID 0x1002.
pub const HEUSINKVELD_HANDBRAKE_V2_PID: u16 = 0x1002;

/// Heusinkveld Ultimate pedals (current firmware).
///
/// 🔶 Community-sourced: JacKeTUs/simracing-hwdb — VID 0x30B7, PID 0x1003.
pub const HEUSINKVELD_ULTIMATE_PID: u16 = 0x1003;

// ── Peripheral PIDs (different VIDs) ────────────────────────────────────────

/// Heusinkveld Handbrake V1 (VID 0x10C4 Silicon Labs).
///
/// 🔶 Community-sourced: JacKeTUs/simracing-hwdb — VID 0x10C4, PID 0x8B82.
pub const HEUSINKVELD_HANDBRAKE_V1_PID: u16 = 0x8B82;

/// Heusinkveld Sequential Shifter (VID 0xA020).
///
/// 🔶 Community-sourced: JacKeTUs/simracing-hwdb — VID 0xA020, PID 0x3142.
pub const HEUSINKVELD_SHIFTER_PID: u16 = 0x3142;

// ── Legacy PIDs (VID 0x04D8, from OpenFlight) ───────────────────────────────

/// Heusinkveld Sprint pedals (legacy/old firmware, VID 0x04D8).
///
/// 🔶 Community-sourced: OpenFlight `sprint-pedals.yaml`.
pub const HEUSINKVELD_LEGACY_SPRINT_PID: u16 = 0xF6D0;

/// Heusinkveld Ultimate+ pedals (legacy/old firmware, VID 0x04D8).
///
/// 🔶 Community-sourced: OpenFlight `ultimate-pedals-0241.yaml`.
pub const HEUSINKVELD_LEGACY_ULTIMATE_PID: u16 = 0xF6D2;

/// Heusinkveld Sim Pedals Pro (discontinued, VID 0x04D8).
///
/// ⚠ Estimated (sequential after 0xF6D2). **Zero external evidence.**
pub const HEUSINKVELD_PRO_PID: u16 = 0xF6D3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeusinkveldModel {
    Sprint,
    Ultimate,
    Pro,
    HandbrakeV1,
    HandbrakeV2,
    SequentialShifter,
    Unknown,
}

impl HeusinkveldModel {
    pub fn from_vid_pid(vendor_id: u16, product_id: u16) -> Self {
        match (vendor_id, product_id) {
            // Current hardware (VID 0x30B7)
            (HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID) => Self::Sprint,
            (HEUSINKVELD_VENDOR_ID, HEUSINKVELD_ULTIMATE_PID) => Self::Ultimate,
            (HEUSINKVELD_VENDOR_ID, HEUSINKVELD_HANDBRAKE_V2_PID) => Self::HandbrakeV2,
            // Legacy hardware (VID 0x04D8)
            (HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_LEGACY_SPRINT_PID) => Self::Sprint,
            (HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_LEGACY_ULTIMATE_PID) => Self::Ultimate,
            (HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_PRO_PID) => Self::Pro,
            // Peripherals (other VIDs)
            (HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID, HEUSINKVELD_HANDBRAKE_V1_PID) => {
                Self::HandbrakeV1
            }
            (HEUSINKVELD_SHIFTER_VENDOR_ID, HEUSINKVELD_SHIFTER_PID) => Self::SequentialShifter,
            _ => Self::Unknown,
        }
    }

    /// Backwards-compatible: match by PID only (assumes current VID 0x30B7).
    pub fn from_product_id(product_id: u16) -> Self {
        match product_id {
            HEUSINKVELD_SPRINT_PID | HEUSINKVELD_LEGACY_SPRINT_PID => Self::Sprint,
            HEUSINKVELD_ULTIMATE_PID | HEUSINKVELD_LEGACY_ULTIMATE_PID => Self::Ultimate,
            HEUSINKVELD_PRO_PID => Self::Pro,
            HEUSINKVELD_HANDBRAKE_V1_PID => Self::HandbrakeV1,
            HEUSINKVELD_HANDBRAKE_V2_PID => Self::HandbrakeV2,
            HEUSINKVELD_SHIFTER_PID => Self::SequentialShifter,
            _ => Self::Unknown,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Sprint => "Heusinkveld Sprint",
            Self::Ultimate => "Heusinkveld Ultimate+",
            Self::Pro => "Heusinkveld Pro",
            Self::HandbrakeV1 => "Heusinkveld Handbrake",
            Self::HandbrakeV2 => "Heusinkveld Handbrake V2",
            Self::SequentialShifter => "Heusinkveld Sequential Shifter",
            Self::Unknown => "Unknown Heusinkveld Device",
        }
    }

    /// Maximum brake load in kilograms.
    ///
    /// * Sprint: 55 kg (plausible; heusinkveld.com does not publish an exact kg figure).
    /// * Ultimate+: 140 kg (confirmed: heusinkveld.com "up to 140kg of force").
    /// * Pro (discontinued): 200 kg (plausible; no current product page to confirm).
    pub fn max_load_kg(&self) -> f32 {
        match self {
            Self::Sprint => 55.0,
            Self::Ultimate => 140.0,
            Self::Pro => 200.0,
            Self::Unknown => 140.0,
            // Not pedal devices
            Self::HandbrakeV1 | Self::HandbrakeV2 | Self::SequentialShifter => 0.0,
        }
    }

    /// Pedal count (0 for non-pedal devices).
    pub fn pedal_count(&self) -> usize {
        match self {
            Self::Sprint => 2,
            Self::Ultimate => 3,
            Self::Pro => 3,
            Self::Unknown => 3,
            Self::HandbrakeV1 | Self::HandbrakeV2 | Self::SequentialShifter => 0,
        }
    }
}

/// Returns true if the VID/PID pair identifies any Heusinkveld device.
pub fn heusinkveld_model_from_info(vendor_id: u16, product_id: u16) -> HeusinkveldModel {
    HeusinkveldModel::from_vid_pid(vendor_id, product_id)
}

/// Returns true if the VID could be a Heusinkveld device.
///
/// Note: VID 0x04D8 (Microchip) and 0x10C4 (Silicon Labs) are shared by many
/// devices, so this check alone is not sufficient. Always verify PID too.
pub fn is_heusinkveld_device(vendor_id: u16) -> bool {
    matches!(
        vendor_id,
        HEUSINKVELD_VENDOR_ID
            | HEUSINKVELD_LEGACY_VENDOR_ID
            | HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID
            | HEUSINKVELD_SHIFTER_VENDOR_ID
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_from_vid_pid_current() {
        assert_eq!(
            HeusinkveldModel::from_vid_pid(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID),
            HeusinkveldModel::Sprint
        );
        assert_eq!(
            HeusinkveldModel::from_vid_pid(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_ULTIMATE_PID),
            HeusinkveldModel::Ultimate
        );
        assert_eq!(
            HeusinkveldModel::from_vid_pid(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_HANDBRAKE_V2_PID),
            HeusinkveldModel::HandbrakeV2
        );
    }

    #[test]
    fn test_model_from_vid_pid_legacy() {
        assert_eq!(
            HeusinkveldModel::from_vid_pid(
                HEUSINKVELD_LEGACY_VENDOR_ID,
                HEUSINKVELD_LEGACY_SPRINT_PID
            ),
            HeusinkveldModel::Sprint
        );
        assert_eq!(
            HeusinkveldModel::from_vid_pid(
                HEUSINKVELD_LEGACY_VENDOR_ID,
                HEUSINKVELD_LEGACY_ULTIMATE_PID
            ),
            HeusinkveldModel::Ultimate
        );
        assert_eq!(
            HeusinkveldModel::from_vid_pid(HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_PRO_PID),
            HeusinkveldModel::Pro
        );
    }

    #[test]
    fn test_model_from_vid_pid_peripherals() {
        assert_eq!(
            HeusinkveldModel::from_vid_pid(
                HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
                HEUSINKVELD_HANDBRAKE_V1_PID
            ),
            HeusinkveldModel::HandbrakeV1
        );
        assert_eq!(
            HeusinkveldModel::from_vid_pid(
                HEUSINKVELD_SHIFTER_VENDOR_ID,
                HEUSINKVELD_SHIFTER_PID
            ),
            HeusinkveldModel::SequentialShifter
        );
    }

    #[test]
    fn test_unknown_vid_pid() {
        assert_eq!(
            HeusinkveldModel::from_vid_pid(0x0000, HEUSINKVELD_SPRINT_PID),
            HeusinkveldModel::Unknown
        );
        assert_eq!(
            HeusinkveldModel::from_vid_pid(HEUSINKVELD_VENDOR_ID, 0xFFFF),
            HeusinkveldModel::Unknown
        );
    }

    #[test]
    fn test_model_from_pid_backwards_compat() {
        // Current PIDs
        assert_eq!(
            HeusinkveldModel::from_product_id(HEUSINKVELD_SPRINT_PID),
            HeusinkveldModel::Sprint
        );
        assert_eq!(
            HeusinkveldModel::from_product_id(HEUSINKVELD_ULTIMATE_PID),
            HeusinkveldModel::Ultimate
        );
        // Legacy PIDs also resolve
        assert_eq!(
            HeusinkveldModel::from_product_id(HEUSINKVELD_LEGACY_SPRINT_PID),
            HeusinkveldModel::Sprint
        );
        assert_eq!(
            HeusinkveldModel::from_product_id(HEUSINKVELD_LEGACY_ULTIMATE_PID),
            HeusinkveldModel::Ultimate
        );
        assert_eq!(
            HeusinkveldModel::from_product_id(0xFFFF),
            HeusinkveldModel::Unknown
        );
    }

    #[test]
    fn test_max_load() {
        assert_eq!(HeusinkveldModel::Sprint.max_load_kg(), 55.0);
        assert_eq!(HeusinkveldModel::Ultimate.max_load_kg(), 140.0);
        assert_eq!(HeusinkveldModel::Pro.max_load_kg(), 200.0);
        assert_eq!(HeusinkveldModel::HandbrakeV1.max_load_kg(), 0.0);
        assert_eq!(HeusinkveldModel::HandbrakeV2.max_load_kg(), 0.0);
        assert_eq!(HeusinkveldModel::SequentialShifter.max_load_kg(), 0.0);
    }

    #[test]
    fn test_pedal_count() {
        assert_eq!(HeusinkveldModel::Sprint.pedal_count(), 2);
        assert_eq!(HeusinkveldModel::Ultimate.pedal_count(), 3);
        assert_eq!(HeusinkveldModel::Pro.pedal_count(), 3);
        assert_eq!(HeusinkveldModel::HandbrakeV1.pedal_count(), 0);
        assert_eq!(HeusinkveldModel::HandbrakeV2.pedal_count(), 0);
        assert_eq!(HeusinkveldModel::SequentialShifter.pedal_count(), 0);
    }

    #[test]
    fn test_display_name() {
        assert_eq!(
            HeusinkveldModel::Sprint.display_name(),
            "Heusinkveld Sprint"
        );
        assert_eq!(
            HeusinkveldModel::Ultimate.display_name(),
            "Heusinkveld Ultimate+"
        );
        assert_eq!(
            HeusinkveldModel::HandbrakeV1.display_name(),
            "Heusinkveld Handbrake"
        );
        assert_eq!(
            HeusinkveldModel::SequentialShifter.display_name(),
            "Heusinkveld Sequential Shifter"
        );
    }

    #[test]
    fn test_is_heusinkveld_device() {
        assert!(is_heusinkveld_device(HEUSINKVELD_VENDOR_ID));
        assert!(is_heusinkveld_device(HEUSINKVELD_LEGACY_VENDOR_ID));
        assert!(is_heusinkveld_device(HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID));
        assert!(is_heusinkveld_device(HEUSINKVELD_SHIFTER_VENDOR_ID));
        assert!(!is_heusinkveld_device(0x0000));
        assert!(!is_heusinkveld_device(0x16D0));
    }

    #[test]
    fn test_heusinkveld_model_from_info() {
        assert_eq!(
            heusinkveld_model_from_info(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID),
            HeusinkveldModel::Sprint
        );
        assert_eq!(
            heusinkveld_model_from_info(
                HEUSINKVELD_LEGACY_VENDOR_ID,
                HEUSINKVELD_LEGACY_SPRINT_PID
            ),
            HeusinkveldModel::Sprint
        );
        assert_eq!(
            heusinkveld_model_from_info(0x0000, HEUSINKVELD_SPRINT_PID),
            HeusinkveldModel::Unknown
        );
    }
}
