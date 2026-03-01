//! Device IDs for Heusinkveld products.
//!
//! ## VID: 0x04D8 (Microchip Technology)
//!
//! Heusinkveld does **not** own a USB-IF Vendor ID. Their pedals use VID
//! `0x04D8`, which belongs to Microchip Technology Inc. and is shared by
//! many products built on Microchip PIC microcontrollers. Heusinkveld
//! firmware runs on a Microchip USB HID controller, hence this VID.
//!
//! Because VID `0x04D8` is extremely common (programming tools, serial
//! bridges, custom HID devices), runtime disambiguation **must** be done
//! by product ID. See `crates/engine/src/hid/vendor/mod.rs` for dispatch
//! logic and `docs/FRICTION_LOG.md` (F-034) for details.
//!
//! ## Verification status
//!
//! | Field | Status | Source |
//! |-------|--------|--------|
//! | VID 0x04D8 | ✅ Confirmed | usb-ids.gowdy.us (Microchip Technology, Inc.); OpenFlight device YAML manifests |
//! | Sprint PID 0xF6D0 | 🔶 Community | OpenFlight `compat/devices/heusinkveld/sprint-pedals.yaml` (community estimate) |
//! | Ultimate PID 0xF6D2 | 🔶 Community | OpenFlight `compat/devices/heusinkveld/ultimate-pedals-0241.yaml` (cross-ref) |
//! | Pro PID 0xF6D3 | ⚠ Estimated | Sequential after 0xF6D2; Pro is discontinued; not independently confirmed |
//! | Sprint load 55 kg | ⚠ Plausible | heusinkveld.com (no kg listed for Sprint) |
//! | Ultimate+ load 140 kg | ✅ Confirmed | heusinkveld.com ("up to 140kg of force") |
//! | Pro load 200 kg | ⚠ Plausible | Discontinued; no current product page |
//!
//! ## Source audit (2026-07)
//!
//! The following external databases were searched and returned **no** Heusinkveld entries:
//! - USB-IF / usb-ids.gowdy.us (VID 0x04D8 listed as Microchip; PIDs 0xF6D0–0xF6D3 absent)
//! - devicehunt.com (VID 0x04D8 listed; no Heusinkveld PIDs)
//! - linux-hardware.org (search "heusinkveld" → 0 results)
//! - Linux kernel `hid-ids.h` (no Heusinkveld defines)
//! - systemd hwdb `70-joystick.hwdb` (no Heusinkveld entries)
//! - SDL `usb_ids.h` / `controller_list.h` (no Heusinkveld entries)
//! - JacKeTUs/linux-steering-wheels (covers FFB wheels only; 0 Heusinkveld entries)
//!
//! The PIDs originate from the OpenFlight sister project
//! (`EffortlessMetrics/OpenFlight`, `compat/devices/heusinkveld/` YAML manifests)
//! which cites community USB descriptor captures. PIDs follow a sequential
//! 0xF6Dx pattern: Sprint=0xF6D0, Sprint+=0xF6D1, Ultimate=0xF6D2,
//! Endurance=0xF6D4. A USB descriptor dump from real hardware is still
//! needed to fully confirm. See `docs/protocols/SOURCES.md` for details.
//!
//! ### Prior VID note
//!
//! An earlier version of this file used VID `0x16D0` (MCS Electronics /
//! OpenMoko) with PIDs `0x1156`–`0x1158`. That set was never confirmed by
//! any external database. The OpenFlight community data (VID `0x04D8`,
//! PIDs `0xF6Dx`) is more consistent with Heusinkveld's Microchip-based
//! firmware and provides broader device coverage.
//!
//! ## Known Heusinkveld USB products not yet covered
//!
//! Heusinkveld also sells a Handbrake, MagShift sequential shifter, MagShift Mini,
//! and "One" steering wheel. These are separate USB devices whose PIDs are unknown.
//! Some may enumerate under VID `0x0EB7` (Fanatec) per OpenFlight alt-manifests.
//!
//! **Note:** These are **pedal** devices — no torque/Nm values apply.

/// Heusinkveld USB Vendor ID (Microchip Technology licensed VID).
///
/// VID `0x04D8` is shared by many Microchip PIC-based devices.
/// Dispatch by PID is required at runtime.
///
/// Source: OpenFlight `compat/devices/heusinkveld/*.yaml` (community).
pub const HEUSINKVELD_VENDOR_ID: u16 = 0x04D8;

/// Heusinkveld Sprint pedals (PID community-sourced from OpenFlight).
///
/// Source: `EffortlessMetrics/OpenFlight` `sprint-pedals.yaml` — VID 0x04D8, PID 0xF6D0.
pub const HEUSINKVELD_SPRINT_PID: u16 = 0xF6D0;
/// Heusinkveld Ultimate+ pedals (PID community-sourced from OpenFlight).
///
/// Source: `EffortlessMetrics/OpenFlight` `ultimate-pedals-0241.yaml` cross-ref — VID 0x04D8, PID 0xF6D2.
pub const HEUSINKVELD_ULTIMATE_PID: u16 = 0xF6D2;
/// Heusinkveld Sim Pedals Pro (discontinued). ⚠ PID estimated (sequential after 0xF6D2).
///
/// Not independently confirmed. Pro is discontinued; no current product page.
pub const HEUSINKVELD_PRO_PID: u16 = 0xF6D3;

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
