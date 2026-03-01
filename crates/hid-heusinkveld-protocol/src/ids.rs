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
//! ## Verification status (web-verified 2025-07)
//!
//! | Field | Confidence | Sources |
//! |-------|------------|---------|
//! | VID 0x04D8 | ✅ Confirmed | the-sz.com, devicehunt.com (Microchip Technology, Inc.) |
//! | Sprint PID 0xF6D0 | 🔶 Community | OpenFlight `compat/devices/heusinkveld/sprint-pedals.yaml` |
//! | Ultimate PID 0xF6D2 | 🔶 Community | OpenFlight `compat/devices/heusinkveld/ultimate-pedals-0241.yaml` |
//! | Pro PID 0xF6D3 | ⚠ Estimated | Sequential after 0xF6D2; Pro is discontinued; **zero external evidence** |
//! | Sprint load 55 kg | ⚠ Plausible | heusinkveld.com (no exact kg figure published for Sprint) |
//! | Ultimate+ load 140 kg | ✅ Confirmed | heusinkveld.com ("up to 140kg of force") |
//! | Pro load 200 kg | ⚠ Plausible | Discontinued; no current product page |
//!
//! ## Source audit (2025-07, web-verified)
//!
//! The following external databases were searched and returned **no** Heusinkveld entries:
//! - the-sz.com (VID 0x04D8 listed as Microchip; ~45 PIDs indexed, none in 0xF6Dx range)
//! - devicehunt.com (VID 0x04D8 = Microchip Technology, Inc.; no PIDs in 0xF6Dx range)
//! - Linux kernel `hid-ids.h` (no Heusinkveld defines; searched `torvalds/linux` HEAD)
//! - Linux kernel `hid-universal-pidff.c` (no Heusinkveld device table entries)
//! - JacKeTUs/linux-steering-wheels (covers FFB wheelbases only; Heusinkveld pedals absent)
//! - SDL `usb_ids.h` (no Heusinkveld or Microchip sim-peripheral entries)
//! - moonrail/asetek_wheelbase_cli (Asetek-only; 0 Heusinkveld references)
//!
//! **Conclusion:** Heusinkveld pedals have zero presence in any public USB ID
//! database, Linux kernel driver, or open-source sim-racing project beyond the
//! OpenFlight sister project. All PIDs should be treated as community-sourced
//! and require a USB descriptor dump from real hardware to fully confirm.
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
//! "One" steering wheel, and the new **RaceCenter** pedal line (2- and 3-pedal sets).
//! These are separate USB devices whose PIDs are unknown.
//!
//! **Note:** These are **pedal** devices — no torque/Nm values apply.

/// Heusinkveld USB Vendor ID (Microchip Technology licensed VID).
///
/// VID `0x04D8` is shared by many Microchip PIC-based devices.
/// Dispatch by PID is required at runtime.
///
/// ✅ Confirmed by: the-sz.com, devicehunt.com (Microchip Technology, Inc.).
pub const HEUSINKVELD_VENDOR_ID: u16 = 0x04D8;

/// Heusinkveld Sprint pedals.
///
/// 🔶 Community-sourced: OpenFlight `sprint-pedals.yaml` — VID 0x04D8, PID 0xF6D0.
/// Not present in any public USB ID database or Linux kernel driver.
pub const HEUSINKVELD_SPRINT_PID: u16 = 0xF6D0;
/// Heusinkveld Ultimate+ pedals.
///
/// 🔶 Community-sourced: OpenFlight `ultimate-pedals-0241.yaml` — VID 0x04D8, PID 0xF6D2.
/// Not present in any public USB ID database or Linux kernel driver.
pub const HEUSINKVELD_ULTIMATE_PID: u16 = 0xF6D2;
/// Heusinkveld Sim Pedals Pro (discontinued).
///
/// ⚠ Estimated (sequential after 0xF6D2). **Zero external evidence.**
/// Pro is discontinued; no current product page. Needs hardware confirmation.
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
