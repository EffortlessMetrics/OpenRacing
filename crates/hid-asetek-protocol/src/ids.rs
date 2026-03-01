//! Device IDs for Asetek SimSports products
//!
//! VID `0x2433` is the official USB vendor ID registered to Asetek A/S.
//!
//! ## Sources (all cross-referenced, verified 2025)
//!
//! - **Linux kernel upstream** (`torvalds/linux`, `drivers/hid/hid-ids.h:201-205`):
//!   `USB_VENDOR_ID_ASETEK 0x2433`, `USB_DEVICE_ID_ASETEK_{INVICTA,FORTE,LA_PRIMA,TONY_KANAAN}`.
//! - **Linux kernel upstream** (`torvalds/linux`, `drivers/hid/hid-universal-pidff.c`):
//!   driver table lists all four PIDs; no device-specific quirk flags applied.
//! - **JacKeTUs/linux-steering-wheels** compatibility table:
//!   Invicta `f300`, Forte `f301`, La Prima `f303`, Tony Kanaan `f306` — all Gold support.
//! - **USB VID registries** (the-sz.com, usb-ids.gowdy.us, devicehunt.com):
//!   VID `0x2433` → Asetek A/S.
//!
//! ## Protocol notes
//!
//! All four wheelbases present a standard **USB HID PID** (Physical Interface
//! Device) force-feedback descriptor, allowing the Linux `hid-pidff` /
//! `hid-universal-pidff` driver to handle FFB effects without vendor-specific
//! protocol logic.

/// Asetek A/S USB Vendor ID.
/// Source: USB-IF VID registry; Linux `drivers/hid/hid-ids.h`.
pub const ASETEK_VENDOR_ID: u16 = 0x2433;

/// Asetek Invicta (27 Nm premium direct drive).
/// Source: Linux `USB_DEVICE_ID_ASETEK_INVICTA` (`hid-ids.h`).
pub const ASETEK_INVICTA_PID: u16 = 0xF300;
/// Asetek Forte (18 Nm mid-range direct drive).
/// Source: Linux `USB_DEVICE_ID_ASETEK_FORTE` (`hid-ids.h`).
pub const ASETEK_FORTE_PID: u16 = 0xF301;
/// Asetek La Prima (12 Nm entry direct drive).
/// Source: Linux `USB_DEVICE_ID_ASETEK_LA_PRIMA` (`hid-ids.h`).
pub const ASETEK_LAPRIMA_PID: u16 = 0xF303;
/// Asetek Tony Kanaan Edition (27 Nm, Invicta-based special edition).
/// Source: Linux `USB_DEVICE_ID_ASETEK_TONY_KANAAN` (`hid-ids.h`).
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
