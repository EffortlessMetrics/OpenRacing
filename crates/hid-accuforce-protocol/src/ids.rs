//! AccuForce USB vendor and product ID constants.
//!
//! VID `0x1FC9` is the NXP Semiconductors USB Vendor ID. SimXperience uses
//! NXP USB chips internally for the AccuForce Pro wheelbase.
//!
//! Sources (verified 2025-06):
//! - RetroBat Wheels.cs (commit 0a54752): `VID_1FC9&PID_804C`
//!   <https://github.com/RetroBat-Official/emulatorlauncher/blob/master/emulatorLauncher/Common/Wheels.cs>
//! - JacKeTUs/linux-steering-wheels compatibility table: VID `1fc9`,
//!   PID `804c`, rated **Platinum** with `hid-pidff` driver
//!   <https://github.com/JacKeTUs/linux-steering-wheels>
//! - Apkallu-Industries/Pitwall `SimXAccuforce.xml`:
//!   `productId="804C" vendorId="1FC9"`, category "wheel"
//!   <https://github.com/Apkallu-Industries/Pitwall>
//!
//! The AccuForce Pro V1 and V2 share the same VID/PID — no V2-specific PID
//! has been observed in any public source.

/// NXP Semiconductors USB Vendor ID (used internally by SimXperience AccuForce).
pub const VENDOR_ID: u16 = 0x1FC9;

// ── Confirmed product IDs ────────────────────────────────────────────────────

/// SimXperience AccuForce Pro product ID (covers both V1 and V2 hardware).
///
/// Confirmed via three independent sources:
/// - RetroBat Wheels.cs (commit 0a54752: `VID_1FC9&PID_804C`)
/// - JacKeTUs/linux-steering-wheels (Platinum, hid-pidff)
/// - Apkallu-Industries/Pitwall SimXAccuforce.xml (`productId="804C"`)
pub const PID_ACCUFORCE_PRO: u16 = 0x804C;

/// Returns `true` if the VID/PID pair identifies an AccuForce device.
pub fn is_accuforce(vid: u16, pid: u16) -> bool {
    vid == VENDOR_ID && is_accuforce_pid(pid)
}

/// Returns `true` if `pid` is a known AccuForce product ID (VID not checked).
pub fn is_accuforce_pid(pid: u16) -> bool {
    matches!(pid, PID_ACCUFORCE_PRO)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pro_pid_recognised() {
        assert!(is_accuforce(VENDOR_ID, PID_ACCUFORCE_PRO));
        assert!(is_accuforce_pid(PID_ACCUFORCE_PRO));
    }

    #[test]
    fn wrong_vid_not_recognised() {
        assert!(!is_accuforce(0x0000, PID_ACCUFORCE_PRO));
        assert!(!is_accuforce(0x16D0, PID_ACCUFORCE_PRO)); // Simucube VID
        assert!(!is_accuforce(0x1DD2, PID_ACCUFORCE_PRO)); // Leo Bodnar VID
    }

    #[test]
    fn unknown_pid_not_recognised() {
        assert!(!is_accuforce(VENDOR_ID, 0xFFFF));
        assert!(!is_accuforce_pid(0xFFFF));
    }
}
