//! Protocol verification tests for the AccuForce HID protocol implementation.
//!
//! These tests cross-reference our constants against independent public sources
//! to verify that VID/PID values, report format constants, and device metadata
//! are accurate.
//!
//! ## Sources cited
//!
//! | # | Source | What it confirms |
//! |---|--------|------------------|
//! | 1 | RetroBat `Wheels.cs` (commit 0a54752) | VID `0x1FC9`, PID `0x804C` |
//! | 2 | JacKeTUs/linux-steering-wheels compat table | VID `0x1FC9`, PID `0x804C`, Platinum rating, `hid-pidff` driver |
//! | 3 | Apkallu-Industries/Pitwall `SimXAccuforce.xml` | `vendorId="1FC9"`, `productId="804C"` |
//! | 4 | USB HID PID specification (`pid1_01.pdf`) | Usage Page `0x000F` = Physical Interface Device |
//! | 5 | USB 2.0 spec §5.7.3 (Full-Speed Interrupt) | Max interrupt transfer payload = 64 bytes |

use racing_wheel_hid_accuforce_protocol::{
    AccuForceModel, DeviceInfo, HID_PID_USAGE_PAGE, MAX_REPORT_BYTES, PID_ACCUFORCE_PRO,
    RECOMMENDED_B_INTERVAL_MS, VENDOR_ID, is_accuforce, is_accuforce_pid,
};

// ════════════════════════════════════════════════════════════════════════════
// § 1. VID / PID verification against documented values
// ════════════════════════════════════════════════════════════════════════════

/// VID `0x1FC9` = NXP Semiconductors (USB chip used by SimXperience).
/// Source [1]: RetroBat Wheels.cs → `VID_1FC9&PID_804C`
/// Source [2]: JacKeTUs/linux-steering-wheels → VID `1fc9`
/// Source [3]: Apkallu-Industries/Pitwall → `vendorId="1FC9"`
#[test]
fn vid_matches_three_independent_sources() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        VENDOR_ID, 0x1FC9,
        "AccuForce VID must be 0x1FC9 (NXP Semiconductors)"
    );
    Ok(())
}

/// PID `0x804C` = SimExperience AccuForce Pro (V1 and V2 share this PID).
/// Source [1]: RetroBat Wheels.cs → `VID_1FC9&PID_804C`
/// Source [2]: JacKeTUs/linux-steering-wheels → PID `804c`, Platinum
/// Source [3]: Apkallu-Industries/Pitwall → `productId="804C"`
#[test]
fn accuforce_pro_pid_matches_three_sources() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        PID_ACCUFORCE_PRO, 0x804C,
        "AccuForce Pro PID must be 0x804C"
    );
    Ok(())
}

/// `is_accuforce()` must accept the confirmed VID/PID pair.
#[test]
fn is_accuforce_accepts_confirmed_vid_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        is_accuforce(0x1FC9, 0x804C),
        "is_accuforce must return true for confirmed VID/PID pair"
    );
    Ok(())
}

/// `is_accuforce_pid()` must accept the confirmed PID.
#[test]
fn is_accuforce_pid_accepts_confirmed_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        is_accuforce_pid(0x804C),
        "is_accuforce_pid must return true for 0x804C"
    );
    Ok(())
}

/// Wrong VID must be rejected even with the correct PID.
#[test]
fn wrong_vid_rejected() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        !is_accuforce(0x0000, PID_ACCUFORCE_PRO),
        "VID 0x0000 must be rejected"
    );
    assert!(
        !is_accuforce(0x16D0, PID_ACCUFORCE_PRO),
        "Simucube VID must be rejected"
    );
    assert!(
        !is_accuforce(0x0483, PID_ACCUFORCE_PRO),
        "STM VID must be rejected"
    );
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 2. Report format constants
// ════════════════════════════════════════════════════════════════════════════

/// AccuForce operates at full-speed USB; max report payload is 64 bytes.
/// Source [5]: USB 2.0 spec §5.7.3
#[test]
fn max_report_bytes_within_usb_full_speed() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        MAX_REPORT_BYTES, 64,
        "Full-speed USB HID max interrupt payload is 64 bytes"
    );
    Ok(())
}

/// HID PID usage page is 0x000F (Physical Interface Device).
/// Source [4]: USB HID PID specification (`pid1_01.pdf`)
#[test]
fn hid_pid_usage_page_matches_spec() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        HID_PID_USAGE_PAGE, 0x000F,
        "HID PID usage page must be 0x000F per USB spec"
    );
    Ok(())
}

/// USB update interval must be a positive, reasonable value.
#[test]
#[allow(clippy::assertions_on_constants)]
fn recommended_interval_is_reasonable() -> Result<(), Box<dyn std::error::Error>> {
    assert!(RECOMMENDED_B_INTERVAL_MS > 0, "bInterval must be positive");
    assert!(
        RECOMMENDED_B_INTERVAL_MS <= 16,
        "bInterval should not exceed 16 ms for a responsive device"
    );
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 3. Device model classification
// ════════════════════════════════════════════════════════════════════════════

/// The confirmed PID must resolve to `AccuForceModel::Pro`, not `Unknown`.
#[test]
fn confirmed_pid_resolves_to_pro_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = AccuForceModel::from_product_id(PID_ACCUFORCE_PRO);
    assert_eq!(
        model,
        AccuForceModel::Pro,
        "PID 0x804C must resolve to AccuForceModel::Pro"
    );
    Ok(())
}

/// Unknown PIDs must not resolve to a named model.
#[test]
fn unknown_pids_resolve_to_unknown() -> Result<(), Box<dyn std::error::Error>> {
    for &pid in &[0x0000u16, 0x0001, 0xFFFF] {
        let model = AccuForceModel::from_product_id(pid);
        assert_eq!(
            model,
            AccuForceModel::Unknown,
            "PID 0x{pid:04X} must resolve to Unknown"
        );
    }
    Ok(())
}

/// AccuForce Pro peak torque is rated at ~7 Nm (V1 conservative figure).
#[test]
fn pro_torque_rating() -> Result<(), Box<dyn std::error::Error>> {
    let torque = AccuForceModel::Pro.max_torque_nm();
    assert!(
        (torque - 7.0).abs() < f32::EPSILON,
        "AccuForce Pro rated torque should be 7.0 Nm, got {torque}"
    );
    Ok(())
}

/// Display name must be non-empty and mention "AccuForce".
#[test]
fn display_name_is_descriptive() -> Result<(), Box<dyn std::error::Error>> {
    let name = AccuForceModel::Pro.display_name();
    assert!(
        name.contains("AccuForce"),
        "display name must mention AccuForce, got: {name}"
    );
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 4. DeviceInfo round-trip
// ════════════════════════════════════════════════════════════════════════════

/// `DeviceInfo::from_vid_pid` must preserve the original VID/PID and resolve
/// the correct model.
#[test]
fn device_info_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let info = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
    assert_eq!(info.vendor_id, VENDOR_ID, "VID must be preserved");
    assert_eq!(info.product_id, PID_ACCUFORCE_PRO, "PID must be preserved");
    assert_eq!(info.model, AccuForceModel::Pro, "model must resolve to Pro");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 5. FFB protocol type verification
// ════════════════════════════════════════════════════════════════════════════

/// AccuForce uses standard HID PID (PIDFF) — the same usage page as other
/// PIDFF devices like Simucube, Leo Bodnar, and FFBeast. There is no
/// vendor-specific extension.
/// Source [2]: linux-steering-wheels lists `hid-pidff` as the driver.
#[test]
fn uses_standard_pidff_usage_page() -> Result<(), Box<dyn std::error::Error>> {
    // 0x000F = Physical Interface Device (PID) usage page
    assert_eq!(
        HID_PID_USAGE_PAGE, 0x000F,
        "AccuForce must use standard HID PID usage page"
    );
    Ok(())
}
