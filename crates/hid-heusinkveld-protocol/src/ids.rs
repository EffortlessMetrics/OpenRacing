//! Device IDs for Heusinkveld products.
//!
//! ## VID collision: 0x16D0 (MCS Electronics / OpenMoko)
//!
//! Heusinkveld does **not** own a USB-IF Vendor ID. Their pedals use VID
//! `0x16D0`, which belongs to MCS Electronics and is shared by many
//! unrelated products (it is available for sub-licensing). In the sim
//! racing world, at least two other vendors also ship on this VID:
//!
//! * **Granite Devices (Simucube 2)** — PIDs `0x0D5A`–`0x0D66`
//! * **Simagic (legacy)** — PID `0x0D5A` (M10, collides with Simucube 1)
//!
//! Runtime disambiguation **must** be done by product ID, not by vendor ID
//! alone. See `crates/engine/src/hid/vendor/mod.rs` for the dispatch logic
//! and `docs/FRICTION_LOG.md` (F-034) for details.
//!
//! ## Verification status
//!
//! | Field | Status | Source |
//! |-------|--------|--------|
//! | VID 0x16D0 | ✅ Confirmed | linux-hardware.org, codebase-wide consistency |
//! | Sprint PID 0x1156 | ⚠ Unverified externally | Not in USB-IF DB or linux-hardware.org |
//! | Ultimate PID 0x1157 | ⚠ Unverified externally | Not in USB-IF DB or linux-hardware.org |
//! | Pro PID 0x1158 | ⚠ Unverified externally | Sim Pedals Pro (discontinued) |
//! | Sprint load 55 kg | ⚠ Plausible | heusinkveld.com (no kg listed for Sprint) |
//! | Ultimate+ load 140 kg | ✅ Confirmed | heusinkveld.com ("up to 140kg of force") |
//! | Pro load 200 kg | ⚠ Plausible | Discontinued; no current product page |
//!
//! **Note:** These are **pedal** devices — no torque/Nm values apply.

/// Heusinkveld USB Vendor ID (MCS Electronics sub-licensed VID).
///
/// **Shared VID** — also used by Simucube 2 and legacy Simagic.
/// Dispatch by PID is required at runtime.
pub const HEUSINKVELD_VENDOR_ID: u16 = 0x16D0;

/// Heusinkveld Sprint pedals. ⚠ PID unverified in external USB databases.
pub const HEUSINKVELD_SPRINT_PID: u16 = 0x1156;
/// Heusinkveld Ultimate+ pedals. ⚠ PID unverified in external USB databases.
pub const HEUSINKVELD_ULTIMATE_PID: u16 = 0x1157;
/// Heusinkveld Sim Pedals Pro (discontinued). ⚠ PID unverified in external USB databases.
pub const HEUSINKVELD_PRO_PID: u16 = 0x1158;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeusinkveldModel {
    Sprint,
    Ultimate,
    Pro,
    Unknown,
}

impl HeusinkveldModel {
    pub fn from_product_id(product_id: u16) -> Self {
        match product_id {
            HEUSINKVELD_SPRINT_PID => Self::Sprint,
            HEUSINKVELD_ULTIMATE_PID => Self::Ultimate,
            HEUSINKVELD_PRO_PID => Self::Pro,
            _ => Self::Unknown,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Sprint => "Heusinkveld Sprint",
            Self::Ultimate => "Heusinkveld Ultimate+",
            Self::Pro => "Heusinkveld Pro",
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
        }
    }

    /// Sprint pedal count (2-pedal base set; optional clutch sold separately).
    pub fn pedal_count(&self) -> usize {
        match self {
            Self::Sprint => 2,
            Self::Ultimate => 3,
            Self::Pro => 3,
            // Default assumes 3 (safest; under-reporting risks ignoring an axis).
            Self::Unknown => 3,
        }
    }
}

pub fn heusinkveld_model_from_info(vendor_id: u16, product_id: u16) -> HeusinkveldModel {
    if vendor_id != HEUSINKVELD_VENDOR_ID {
        return HeusinkveldModel::Unknown;
    }
    HeusinkveldModel::from_product_id(product_id)
}

pub fn is_heusinkveld_device(vendor_id: u16) -> bool {
    vendor_id == HEUSINKVELD_VENDOR_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_from_pid() {
        assert_eq!(
            HeusinkveldModel::from_product_id(HEUSINKVELD_SPRINT_PID),
            HeusinkveldModel::Sprint
        );
        assert_eq!(
            HeusinkveldModel::from_product_id(HEUSINKVELD_ULTIMATE_PID),
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
    }

    #[test]
    fn test_pedal_count() {
        assert_eq!(HeusinkveldModel::Sprint.pedal_count(), 2);
        assert_eq!(HeusinkveldModel::Ultimate.pedal_count(), 3);
        assert_eq!(HeusinkveldModel::Pro.pedal_count(), 3);
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
    }
}
