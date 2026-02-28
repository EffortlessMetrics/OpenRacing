//! AccuForce USB vendor and product ID constants.
//!
//! VID `0x1FC9` is the NXP Semiconductors USB Vendor ID. SimExperience uses
//! NXP USB chips internally for the AccuForce Pro wheelbase.
//!
//! Sources: community USB device captures, RetroBat Wheels.cs (commit 0a54752),
//! and the existing OpenRacing engine vendor list.

/// NXP Semiconductors USB Vendor ID (used internally by SimExperience AccuForce).
pub const VENDOR_ID: u16 = 0x1FC9;

// ── Confirmed product IDs ────────────────────────────────────────────────────

/// SimExperience AccuForce Pro product ID.
///
/// Confirmed via community USB captures and the RetroBat Wheels.cs
/// compatibility table (commit 0a54752: `VID_1FC9&PID_804C`).
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
