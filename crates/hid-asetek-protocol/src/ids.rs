//! Device IDs for Asetek SimSports products
//!
//! VID `0x2433` is the official USB vendor ID registered to Asetek A/S.
//!
//! ## Sources (all cross-referenced, verified 2025)
//!
//! - **Linux kernel upstream** (`torvalds/linux`, `drivers/hid/hid-ids.h`):
//!   `USB_VENDOR_ID_ASETEK 0x2433`, `USB_DEVICE_ID_ASETEK_{INVICTA,FORTE,LA_PRIMA,TONY_KANAAN}`.
//! - **Linux kernel upstream** (`torvalds/linux`, `drivers/hid/hid-universal-pidff.c`):
//!   driver table lists all four PIDs; no device-specific quirk flags applied.
//! - **JacKeTUs/linux-steering-wheels** compatibility table:
//!   Invicta `f300`, Forte `f301`, La Prima `f303`, Tony Kanaan `f306` — all Gold support.
//! - **USB VID registries** (the-sz.com, devicehunt.com):
//!   VID `0x2433` → "Asetek A/S" / "ASETEK". Only PID `0xB200` (NZXT Kraken X60)
//!   is registered in public databases; SimSports PIDs `0xF3xx` are absent from
//!   the-sz.com and devicehunt.com but are authoritatively confirmed by the
//!   Linux kernel HID driver (merged upstream).
//! - **moonrail/asetek_wheelbase_cli** (community Linux CLI tool):
//!   udev rules cite VID `0x2433`, PID `0xF303` (La Prima wheelbase) and
//!   PID `0xF203` (La Prima steering wheel — separate USB device, not tracked here).
//!
//! ## Verification status (web-verified 2025-07)
//!
//! | Field | Confidence | Sources |
//! |-------|------------|---------|
//! | VID 0x2433 | ✅ Confirmed | the-sz.com, devicehunt.com, Linux `hid-ids.h` |
//! | Invicta PID 0xF300 | ✅ Confirmed | Linux `hid-ids.h`, `hid-universal-pidff.c`, JacKeTUs |
//! | Forte PID 0xF301 | ✅ Confirmed | Linux `hid-ids.h`, `hid-universal-pidff.c`, JacKeTUs |
//! | La Prima PID 0xF303 | ✅ Confirmed | Linux `hid-ids.h`, `hid-universal-pidff.c`, JacKeTUs, asetek_wheelbase_cli |
//! | Tony Kanaan PID 0xF306 | ✅ Confirmed | Linux `hid-ids.h`, `hid-universal-pidff.c`, JacKeTUs |
//!
//! All four PIDs have **zero external-evidence flags** — every PID is confirmed
//! by at least three independent sources including the Linux kernel.
//!
//! ## Protocol notes
//!
//! All four wheelbases present a standard **USB HID PID** (Physical Interface
//! Device) force-feedback descriptor, allowing the Linux `hid-pidff` /
//! `hid-universal-pidff` driver to handle FFB effects without vendor-specific
//! protocol logic. Linux kernel support landed in 6.15 (backported to 6.12.24+).
//!
//! ## Known Asetek USB products not tracked here
//!
//! The La Prima steering wheel rim enumerates as a separate USB device with
//! PID `0xF203` (per moonrail/asetek_wheelbase_cli udev rules). Forte and
//! Invicta wheel rims likely have their own PIDs as well.

/// Asetek A/S USB Vendor ID.
///
/// ✅ Confirmed by: the-sz.com, devicehunt.com, Linux `hid-ids.h`.
pub const ASETEK_VENDOR_ID: u16 = 0x2433;

/// Asetek Invicta (27 Nm premium direct drive).
///
/// ✅ Confirmed by: Linux `hid-ids.h`, `hid-universal-pidff.c`, JacKeTUs.
pub const ASETEK_INVICTA_PID: u16 = 0xF300;
/// Asetek Forte (18 Nm mid-range direct drive).
///
/// ✅ Confirmed by: Linux `hid-ids.h`, `hid-universal-pidff.c`, JacKeTUs.
pub const ASETEK_FORTE_PID: u16 = 0xF301;
/// Asetek La Prima (12 Nm entry direct drive).
///
/// ✅ Confirmed by: Linux `hid-ids.h`, `hid-universal-pidff.c`, JacKeTUs,
/// moonrail/asetek_wheelbase_cli.
pub const ASETEK_LAPRIMA_PID: u16 = 0xF303;
/// Asetek Tony Kanaan Edition (27 Nm, Invicta-based special edition).
///
/// ✅ Confirmed by: Linux `hid-ids.h`, `hid-universal-pidff.c`, JacKeTUs.
pub const ASETEK_TONY_KANAAN_PID: u16 = 0xF306;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsetekModel {
    Forte,
    Invicta,
    LaPrima,
    TonyKanaan,
    Unknown,
}

impl AsetekModel {
    pub fn from_product_id(product_id: u16) -> Self {
        match product_id {
            ASETEK_FORTE_PID => Self::Forte,
            ASETEK_INVICTA_PID => Self::Invicta,
            ASETEK_LAPRIMA_PID => Self::LaPrima,
            ASETEK_TONY_KANAAN_PID => Self::TonyKanaan,
            _ => Self::Unknown,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Forte => "Asetek Forte",
            Self::Invicta => "Asetek Invicta",
            Self::LaPrima => "Asetek La Prima",
            Self::TonyKanaan => "Asetek Tony Kanaan Edition",
            Self::Unknown => "Unknown Asetek Device",
        }
    }

    pub fn max_torque_nm(&self) -> f32 {
        match self {
            Self::Forte => 18.0,
            Self::Invicta => 27.0,
            Self::LaPrima => 12.0,
            Self::TonyKanaan => 27.0,
            Self::Unknown => 18.0,
        }
    }
}

pub fn asetek_model_from_info(vendor_id: u16, product_id: u16) -> AsetekModel {
    if vendor_id != ASETEK_VENDOR_ID {
        return AsetekModel::Unknown;
    }
    AsetekModel::from_product_id(product_id)
}

pub fn is_asetek_device(vendor_id: u16) -> bool {
    vendor_id == ASETEK_VENDOR_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_from_pid() {
        assert_eq!(
            AsetekModel::from_product_id(ASETEK_FORTE_PID),
            AsetekModel::Forte
        );
        assert_eq!(
            AsetekModel::from_product_id(ASETEK_INVICTA_PID),
            AsetekModel::Invicta
        );
        assert_eq!(AsetekModel::from_product_id(0xFFFF), AsetekModel::Unknown);
    }

    #[test]
    fn test_max_torque() {
        assert_eq!(AsetekModel::Forte.max_torque_nm(), 18.0);
        assert_eq!(AsetekModel::Invicta.max_torque_nm(), 27.0);
        assert_eq!(AsetekModel::LaPrima.max_torque_nm(), 12.0);
    }

    #[test]
    fn test_display_name() {
        assert_eq!(AsetekModel::Forte.display_name(), "Asetek Forte");
    }
}
