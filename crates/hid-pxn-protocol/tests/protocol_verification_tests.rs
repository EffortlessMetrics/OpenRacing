//! Protocol verification tests for PXN / Lite Star HID devices.
//!
//! Cross-verifies VID/PID constants and known-good identification data
//! against external kernel and community sources.
//!
//! ## Source citations
//!
//! | Source | URL / location | Data used |
//! |--------|---------------|-----------|
//! | Linux kernel `hid-ids.h` | `torvalds/linux` (mainline ≥6.15) | `USB_VENDOR_ID_LITE_STAR = 0x11ff`, all PXN PIDs |
//! | Linux kernel `hid-universal-pidff.c` | `torvalds/linux` (mainline ≥6.15) | Device table with `HID_PIDFF_QUIRK_PERIODIC_SINE_ONLY` |
//! | JacKeTUs/linux-steering-wheels | Compatibility table (GitHub) | PXN V10/V12/V12 Lite — Gold rating, `11ff:XXXX` |
//!
//! ## Protocol notes
//!
//! PXN devices use standard USB HID PID (Physical Interface Device) for
//! force feedback. The Linux kernel applies `HID_PIDFF_QUIRK_PERIODIC_SINE_ONLY`,
//! which limits periodic effects to sine waveform only.

use racing_wheel_hid_pxn_protocol::{
    PRODUCT_GT987, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE, PRODUCT_V12_LITE_2, VENDOR_ID,
    is_pxn, product_name,
};

// ─── VID cross-verification against Linux kernel ────────────────────────────

/// VID 0x11FF — `USB_VENDOR_ID_LITE_STAR` in Linux kernel `hid-ids.h`.
///
/// Source: `torvalds/linux` mainline, `drivers/hid/hid-ids.h`:
///   `#define USB_VENDOR_ID_LITE_STAR 0x11ff`
///
/// Cross-checked: JacKeTUs/linux-steering-wheels compatibility table
/// lists all PXN devices under VID `11ff`.
///
/// Note: VID 0x11FF is NOT in public USB-ID databases (the-sz.com,
/// devicehunt.com, usb-ids.gowdy.us) — the kernel is the authoritative
/// source.
#[test]
fn vid_matches_kernel_usb_vendor_id_lite_star() {
    assert_eq!(
        VENDOR_ID, 0x11FF,
        "VID must match Linux kernel USB_VENDOR_ID_LITE_STAR = 0x11ff"
    );
}

// ─── PID cross-verification against Linux kernel hid-ids.h ──────────────────

/// PXN V10 PID 0x3245 — `USB_DEVICE_ID_PXN_V10` in kernel `hid-ids.h`.
///
/// Source: `#define USB_DEVICE_ID_PXN_V10 0x3245`
/// Cross-checked: linux-steering-wheels (Gold, `11ff:3245`),
///                `hid-universal-pidff.c` device table.
#[test]
fn v10_pid_matches_kernel_hid_ids() {
    assert_eq!(PRODUCT_V10, 0x3245, "Must match USB_DEVICE_ID_PXN_V10");
}

/// PXN V12 PID 0x1212 — `USB_DEVICE_ID_PXN_V12` in kernel `hid-ids.h`.
///
/// Source: `#define USB_DEVICE_ID_PXN_V12 0x1212`
/// Cross-checked: linux-steering-wheels (Gold, `11ff:1212`),
///                `hid-universal-pidff.c` device table.
#[test]
fn v12_pid_matches_kernel_hid_ids() {
    assert_eq!(PRODUCT_V12, 0x1212, "Must match USB_DEVICE_ID_PXN_V12");
}

/// PXN V12 Lite PID 0x1112 — `USB_DEVICE_ID_PXN_V12_LITE` in kernel.
///
/// Source: `#define USB_DEVICE_ID_PXN_V12_LITE 0x1112`
/// Cross-checked: linux-steering-wheels (Gold, `11ff:1112`),
///                `hid-universal-pidff.c` device table.
#[test]
fn v12_lite_pid_matches_kernel_hid_ids() {
    assert_eq!(
        PRODUCT_V12_LITE, 0x1112,
        "Must match USB_DEVICE_ID_PXN_V12_LITE"
    );
}

/// PXN V12 Lite variant 2 PID 0x1211 — `USB_DEVICE_ID_PXN_V12_LITE_2`.
///
/// Source: `#define USB_DEVICE_ID_PXN_V12_LITE_2 0x1211`
/// Cross-checked: linux-steering-wheels (Gold, `11ff:1211`),
///                `hid-universal-pidff.c` device table.
#[test]
fn v12_lite_2_pid_matches_kernel_hid_ids() {
    assert_eq!(
        PRODUCT_V12_LITE_2, 0x1211,
        "Must match USB_DEVICE_ID_PXN_V12_LITE_2"
    );
}

/// Lite Star GT987 PID 0x2141 — `USB_DEVICE_ID_LITE_STAR_GT987`.
///
/// Source: `#define USB_DEVICE_ID_LITE_STAR_GT987 0x2141`
/// Cross-checked: linux-steering-wheels (Gold, `11ff:2141`),
///                `hid-universal-pidff.c` device table.
#[test]
fn gt987_pid_matches_kernel_hid_ids() {
    assert_eq!(
        PRODUCT_GT987, 0x2141,
        "Must match USB_DEVICE_ID_LITE_STAR_GT987"
    );
}

// ─── Kernel hid-ids.h exact constant table ──────────────────────────────────

/// Table-driven test: every PID must exactly match the corresponding
/// `#define` from Linux kernel `hid-ids.h` (mainline ≥6.15).
///
/// If any of these fail, the kernel definition has changed and ids.rs
/// must be updated.
#[test]
fn table_driven_kernel_hid_ids_cross_check() -> Result<(), String> {
    // (constant, expected value, kernel #define name)
    let table: &[(u16, u16, &str)] = &[
        (VENDOR_ID, 0x11FF, "USB_VENDOR_ID_LITE_STAR"),
        (PRODUCT_V10, 0x3245, "USB_DEVICE_ID_PXN_V10"),
        (PRODUCT_V12, 0x1212, "USB_DEVICE_ID_PXN_V12"),
        (PRODUCT_V12_LITE, 0x1112, "USB_DEVICE_ID_PXN_V12_LITE"),
        (PRODUCT_V12_LITE_2, 0x1211, "USB_DEVICE_ID_PXN_V12_LITE_2"),
        (PRODUCT_GT987, 0x2141, "USB_DEVICE_ID_LITE_STAR_GT987"),
    ];

    for &(actual, expected, kernel_name) in table {
        if actual != expected {
            return Err(format!(
                "{kernel_name}: expected {expected:#06x}, got {actual:#06x} — \
                 update ids.rs to match kernel hid-ids.h"
            ));
        }
    }
    Ok(())
}

// ─── linux-steering-wheels compatibility cross-check ────────────────────────

/// All PXN devices have Gold rating in JacKeTUs/linux-steering-wheels.
/// Verify that `is_pxn` recognises every VID:PID pair listed there.
///
/// Source: linux-steering-wheels compatibility table
///   11ff:3245 (V10, Gold), 11ff:1212 (V12, Gold),
///   11ff:1112 (V12 Lite, Gold), 11ff:1211 (V12 Lite SE, Gold),
///   11ff:2141 (GT987, Gold)
#[test]
fn linux_steering_wheels_gold_devices() -> Result<(), String> {
    let gold_devices: &[(u16, &str)] = &[
        (0x3245, "PXN V10"),
        (0x1212, "PXN V12"),
        (0x1112, "PXN V12 Lite"),
        (0x1211, "PXN V12 Lite (SE)"),
        (0x2141, "Lite Star GT987 FF"),
    ];

    for &(pid, name) in gold_devices {
        if !is_pxn(0x11FF, pid) {
            return Err(format!(
                "{name} (11ff:{pid:04x}): not recognised by is_pxn — \
                 linux-steering-wheels lists this as Gold"
            ));
        }
    }
    Ok(())
}

// ─── Product name verification ──────────────────────────────────────────────

/// Every known PID must have a non-empty product name.
#[test]
fn all_known_pids_have_product_names() -> Result<(), String> {
    let known: &[(u16, &str)] = &[
        (PRODUCT_V10, "V10"),
        (PRODUCT_V12, "V12"),
        (PRODUCT_V12_LITE, "V12 Lite"),
        (PRODUCT_V12_LITE_2, "V12 Lite (SE)"),
        (PRODUCT_GT987, "GT987"),
    ];

    for &(pid, label) in known {
        match product_name(pid) {
            Some(name) if !name.is_empty() => {}
            Some(_) => {
                return Err(format!("PID {pid:#06x} ({label}): name is empty"));
            }
            None => {
                return Err(format!("PID {pid:#06x} ({label}): no name returned"));
            }
        }
    }
    Ok(())
}

// ─── Known-good VID:PID byte pairs ─────────────────────────────────────────

/// Verify VID:PID byte representation matches what would appear in a USB
/// device descriptor (little-endian).
///
/// In a USB device descriptor, idVendor and idProduct are 16-bit LE fields.
#[test]
fn vid_pid_le_byte_representation() {
    // VID 0x11FF in LE = [0xFF, 0x11]
    let vid_bytes = VENDOR_ID.to_le_bytes();
    assert_eq!(vid_bytes, [0xFF, 0x11]);

    // PID V10 0x3245 in LE = [0x45, 0x32]
    let v10_bytes = PRODUCT_V10.to_le_bytes();
    assert_eq!(v10_bytes, [0x45, 0x32]);

    // PID V12 0x1212 in LE = [0x12, 0x12]
    let v12_bytes = PRODUCT_V12.to_le_bytes();
    assert_eq!(v12_bytes, [0x12, 0x12]);

    // PID V12 Lite 0x1112 in LE = [0x12, 0x11]
    let v12_lite_bytes = PRODUCT_V12_LITE.to_le_bytes();
    assert_eq!(v12_lite_bytes, [0x12, 0x11]);

    // PID V12 Lite 2 0x1211 in LE = [0x11, 0x12]
    let v12_lite_2_bytes = PRODUCT_V12_LITE_2.to_le_bytes();
    assert_eq!(v12_lite_2_bytes, [0x11, 0x12]);

    // PID GT987 0x2141 in LE = [0x41, 0x21]
    let gt987_bytes = PRODUCT_GT987.to_le_bytes();
    assert_eq!(gt987_bytes, [0x41, 0x21]);
}

// ─── PID uniqueness ─────────────────────────────────────────────────────────

/// All product IDs must be distinct to avoid mis-identification.
#[test]
fn all_pids_unique() -> Result<(), String> {
    let pids: &[(u16, &str)] = &[
        (PRODUCT_V10, "V10"),
        (PRODUCT_V12, "V12"),
        (PRODUCT_V12_LITE, "V12 Lite"),
        (PRODUCT_V12_LITE_2, "V12 Lite 2"),
        (PRODUCT_GT987, "GT987"),
    ];
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            if pids[i].0 == pids[j].0 {
                return Err(format!(
                    "PID collision: {} and {} both = {:#06x}",
                    pids[i].1, pids[j].1, pids[i].0
                ));
            }
        }
    }
    Ok(())
}

// ─── Protocol quirk documentation ───────────────────────────────────────────

/// PXN devices use HID PID (Physical Interface Device) force feedback
/// with `HID_PIDFF_QUIRK_PERIODIC_SINE_ONLY` applied in the kernel.
///
/// This test documents that the protocol is standard HID PID, not a
/// vendor-specific protocol. The kernel quirk restricts periodic effects
/// to sine-only waveform.
///
/// Source: `hid-universal-pidff.c` device table (mainline ≥6.15).
#[test]
fn protocol_is_standard_hid_pid_with_sine_only_quirk() {
    // This is a documentation test — PXN uses standard HID PID FFB.
    // The sine-only quirk is applied in kernel space, not in our constants.
    // Verify all known PXN PIDs are tracked (completeness check).
    let all_known = [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ];
    for pid in all_known {
        assert!(
            is_pxn(VENDOR_ID, pid),
            "PID {pid:#06x} must be recognised — listed in hid-universal-pidff.c"
        );
    }
}

// ─── Cross-vendor collision guard ───────────────────────────────────────────

/// PXN VID 0x11FF must not collide with other sim racing vendors tracked
/// in this workspace.
#[test]
fn vid_does_not_collide_with_other_vendors() {
    let other_vids: &[(u16, &str)] = &[
        (0x0483, "STMicroelectronics (VRS/Simagic/Cube Controls)"),
        (0x30B7, "Heusinkveld"),
        (0x04D8, "Microchip (Heusinkveld legacy)"),
        (0x16D0, "Simucube / MCS"),
        (0x0EB7, "Simucube 2"),
        (0x3416, "Cammus"),
        (0x0346, "Moza"),
        (0x2433, "Asetek"),
        (0x1DD2, "Leo Bodnar"),
        (0x044F, "Thrustmaster"),
        (0x046D, "Logitech"),
    ];
    for &(vid, name) in other_vids {
        assert_ne!(
            VENDOR_ID, vid,
            "PXN VID must not collide with {name} VID {vid:#06x}"
        );
    }
}
