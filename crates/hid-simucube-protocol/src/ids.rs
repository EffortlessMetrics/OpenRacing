//! Device IDs for Simucube products.
//!
//! Simucube 2 wheelbases (by Granite Devices) use VID `0x16D0` (MCS Electronics /
//! OpenMoko), the same VID used by the Simucube 1 IONI servo drives and
//! Heusinkveld pedals. Products are distinguished by product ID.
//!
//! ## Web-verified sources (July 2025)
//!
//! All VID/PID values below were cross-checked against four independent sources:
//!
//! - Official Simucube developer docs — `Simucube/simucube-docs.github.io`
//!   `docs/Simucube 2/Developers.md` (authoritative PID table)
//! - Granite Devices wiki USB interface documentation —
//!   <https://granitedevices.com/wiki/Simucube_product_USB_interface_documentation>
//! - Granite Devices wiki udev rules —
//!   <https://granitedevices.com/wiki/Using_Simucube_wheel_base_in_Linux>
//! - JacKeTUs/linux-steering-wheels compatibility table —
//!   <https://github.com/JacKeTUs/linux-steering-wheels>

pub const SIMUCUBE_VENDOR_ID: u16 = 0x16D0;

/// Simucube 1 (IONI-based servo drive).
/// Web-verified: official docs, wiki udev rules, linux-steering-wheels (VID 16d0, PID 0d5a).
pub const SIMUCUBE_1_PID: u16 = 0x0D5A;
/// Simucube 2 Sport (17 Nm direct drive).
/// Web-verified: official docs, wiki udev, linux-steering-wheels (VID 16d0, PID 0d61).
/// Windows guidProduct ID: `{0D6116D0-0000-0000-0000-504944564944}`.
pub const SIMUCUBE_2_SPORT_PID: u16 = 0x0D61;
/// Simucube 2 Pro (25 Nm direct drive).
/// Web-verified: official docs, wiki udev, linux-steering-wheels (VID 16d0, PID 0d60).
/// Windows guidProduct ID: `{0D6016D0-0000-0000-0000-504944564944}`.
pub const SIMUCUBE_2_PRO_PID: u16 = 0x0D60;
/// Simucube 2 Ultimate (32 Nm direct drive).
/// Web-verified: official docs, wiki udev, linux-steering-wheels (VID 16d0, PID 0d5f).
/// Windows guidProduct ID: `{0D5F16D0-0000-0000-0000-504944564944}`.
pub const SIMUCUBE_2_ULTIMATE_PID: u16 = 0x0D5F;
/// Simucube SC-Link Hub (used by ActivePedal / ActivePedal Pro).
/// Web-verified: official docs (VID 16d0, PID 0d66).
/// Windows guidProduct ID: `{0D6616D0-0000-0000-0000-504944564944}`.
pub const SIMUCUBE_ACTIVE_PEDAL_PID: u16 = 0x0D66;
/// SimuCUBE Wireless Wheel. PID estimated — **not present** in the official Simucube
/// developer PID table (accessed 2025-07). Do not rely on this value without
/// independent confirmation.
pub const SIMUCUBE_WIRELESS_WHEEL_PID: u16 = 0x0D63;

// ── Bootloader / firmware-upgrade PIDs ─────────────────────────────────────

/// Simucube 2 firmware-upgrade (bootloader) mode PID.
///
/// Verified from Granite Devices wiki udev rules for firmware flashing.
/// Source: <https://granitedevices.com/wiki/Using_Simucube_wheel_base_in_Linux>
///
/// Do **not** match on this PID for normal device enumeration.
pub const SIMUCUBE_2_BOOTLOADER_PID: u16 = 0x0D5E;

/// Simucube 1 firmware-upgrade (bootloader) mode PID.
///
/// Verified from Granite Devices wiki udev rules for firmware flashing.
/// Source: <https://granitedevices.com/wiki/Using_Simucube_wheel_base_in_Linux>
///
/// Do **not** match on this PID for normal device enumeration.
pub const SIMUCUBE_1_BOOTLOADER_PID: u16 = 0x0D5B;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimucubeModel {
    Simucube1,
    Sport,
    Pro,
    Ultimate,
    ActivePedal,
    WirelessWheel,
    Unknown,
}

impl SimucubeModel {
    pub fn from_product_id(product_id: u16) -> Self {
        match product_id {
            SIMUCUBE_1_PID => Self::Simucube1,
            SIMUCUBE_2_SPORT_PID => Self::Sport,
            SIMUCUBE_2_PRO_PID => Self::Pro,
            SIMUCUBE_2_ULTIMATE_PID => Self::Ultimate,
            SIMUCUBE_ACTIVE_PEDAL_PID => Self::ActivePedal,
            SIMUCUBE_WIRELESS_WHEEL_PID => Self::WirelessWheel,
            _ => Self::Unknown,
        }
    }

    pub fn max_torque_nm(&self) -> f32 {
        match self {
            Self::Simucube1 => 25.0,
            Self::Sport => 17.0,
            Self::Pro => 25.0,
            Self::Ultimate => 32.0,
            Self::ActivePedal => 0.0,
            Self::WirelessWheel => 0.0,
            Self::Unknown => 25.0,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Simucube1 => "Simucube 1",
            Self::Sport => "Simucube 2 Sport",
            Self::Pro => "Simucube 2 Pro",
            Self::Ultimate => "Simucube 2 Ultimate",
            Self::ActivePedal => "Simucube ActivePedal",
            Self::WirelessWheel => "SimuCube Wireless Wheel",
            Self::Unknown => "Unknown Simucube Device",
        }
    }
}

pub fn simucube_model_from_info(vendor_id: u16, product_id: u16) -> SimucubeModel {
    if vendor_id != SIMUCUBE_VENDOR_ID {
        return SimucubeModel::Unknown;
    }
    SimucubeModel::from_product_id(product_id)
}

pub fn is_simucube_device(vendor_id: u16) -> bool {
    vendor_id == SIMUCUBE_VENDOR_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_from_pid() {
        assert_eq!(
            SimucubeModel::from_product_id(SIMUCUBE_1_PID),
            SimucubeModel::Simucube1
        );
        assert_eq!(
            SimucubeModel::from_product_id(SIMUCUBE_2_SPORT_PID),
            SimucubeModel::Sport
        );
        assert_eq!(
            SimucubeModel::from_product_id(SIMUCUBE_2_PRO_PID),
            SimucubeModel::Pro
        );
        assert_eq!(
            SimucubeModel::from_product_id(SIMUCUBE_2_ULTIMATE_PID),
            SimucubeModel::Ultimate
        );
        assert_eq!(
            SimucubeModel::from_product_id(0xFFFF),
            SimucubeModel::Unknown
        );
    }

    #[test]
    fn test_max_torque() {
        assert_eq!(SimucubeModel::Simucube1.max_torque_nm(), 25.0);
        assert_eq!(SimucubeModel::Sport.max_torque_nm(), 17.0);
        assert_eq!(SimucubeModel::Pro.max_torque_nm(), 25.0);
        assert_eq!(SimucubeModel::Ultimate.max_torque_nm(), 32.0);
    }

    #[test]
    fn test_display_name() {
        assert_eq!(SimucubeModel::Pro.display_name(), "Simucube 2 Pro");
        assert_eq!(
            SimucubeModel::Unknown.display_name(),
            "Unknown Simucube Device"
        );
    }
}
