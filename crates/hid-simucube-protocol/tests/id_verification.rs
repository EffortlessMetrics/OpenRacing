//! Cross-reference tests for Simucube VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use hid_simucube_protocol::{
    SIMUCUBE_1_BOOTLOADER_PID, SIMUCUBE_1_PID, SIMUCUBE_2_BOOTLOADER_PID, SIMUCUBE_2_PRO_PID,
    SIMUCUBE_2_SPORT_PID, SIMUCUBE_2_ULTIMATE_PID, SIMUCUBE_ACTIVE_PEDAL_PID, SIMUCUBE_VENDOR_ID,
    SIMUCUBE_WIRELESS_WHEEL_PID,
};

/// Simucube VID must be 0x16D0 (MCS Electronics / OpenMoko).
///
/// Source: USB VID registry; JacKeTUs/linux-steering-wheels.
#[test]
fn vendor_id_is_16d0() {
    assert_eq!(
        SIMUCUBE_VENDOR_ID, 0x16D0,
        "Simucube VID changed — update ids.rs and SOURCES.md"
    );
}

/// Simucube 2 Ultimate (32 Nm) PID must be 0x0D5F.
///
/// Source: Official Simucube developer docs (Simucube/simucube-docs.github.io).
#[test]
fn sc2_ultimate_pid_is_0d5f() {
    assert_eq!(SIMUCUBE_2_ULTIMATE_PID, 0x0D5F);
}

/// Simucube 2 Pro (25 Nm) PID must be 0x0D60.
///
/// Source: Official Simucube developer docs (Simucube/simucube-docs.github.io).
#[test]
fn sc2_pro_pid_is_0d60() {
    assert_eq!(SIMUCUBE_2_PRO_PID, 0x0D60);
}

/// Simucube 2 Sport (17 Nm) PID must be 0x0D61.
///
/// Source: Official Simucube developer docs (Simucube/simucube-docs.github.io).
#[test]
fn sc2_sport_pid_is_0d61() {
    assert_eq!(SIMUCUBE_2_SPORT_PID, 0x0D61);
}

/// Simucube 1 PID must be 0x0D5A.
///
/// Source: Official Simucube developer docs (Simucube/simucube-docs.github.io);
/// confirmed by gro-ove/actools SimuCube.cs and RiddleTime/Race-Element.
#[test]
fn sc1_pid_is_0d5a() {
    assert_eq!(SIMUCUBE_1_PID, 0x0D5A);
}

/// Simucube SC-Link Hub (ActivePedal) PID must be 0x0D66.
///
/// Source: Official Simucube developer docs (Simucube/simucube-docs.github.io).
#[test]
fn sc_link_hub_pid_is_0d66() {
    assert_eq!(SIMUCUBE_ACTIVE_PEDAL_PID, 0x0D66);
}

/// Simucube 2 bootloader/firmware-upgrade PID must be 0x0D5E.
///
/// Source: Granite Devices wiki udev rules (Using_Simucube_wheel_base_in_Linux).
#[test]
fn sc2_bootloader_pid_is_0d5e() {
    assert_eq!(SIMUCUBE_2_BOOTLOADER_PID, 0x0D5E);
}

/// Simucube 1 bootloader/firmware-upgrade PID must be 0x0D5B.
///
/// Source: Granite Devices wiki udev rules (Using_Simucube_wheel_base_in_Linux).
#[test]
fn sc1_bootloader_pid_is_0d5b() {
    assert_eq!(SIMUCUBE_1_BOOTLOADER_PID, 0x0D5B);
}

/// SimuCube Wireless Wheel PID must be 0x0D63.
///
/// Note: This PID is estimated — it is **not** present in the official Simucube
/// developer PID table (accessed 2025-07). Do not rely on this value without
/// independent confirmation.
#[test]
fn wireless_wheel_pid_is_0d63() {
    assert_eq!(SIMUCUBE_WIRELESS_WHEEL_PID, 0x0D63);
}

/// All normal-operation PIDs must be distinct from each other.
#[test]
fn all_pids_are_unique() {
    let pids = [
        SIMUCUBE_1_PID,
        SIMUCUBE_2_SPORT_PID,
        SIMUCUBE_2_PRO_PID,
        SIMUCUBE_2_ULTIMATE_PID,
        SIMUCUBE_ACTIVE_PEDAL_PID,
        SIMUCUBE_WIRELESS_WHEEL_PID,
    ];
    for (i, a) in pids.iter().enumerate() {
        for (j, b) in pids.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "PID at index {i} ({a:#06X}) collides with index {j}");
            }
        }
    }
}

/// Bootloader PIDs must be distinct from all normal-operation PIDs.
#[test]
fn bootloader_pids_distinct_from_normal() {
    let normal = [
        SIMUCUBE_1_PID,
        SIMUCUBE_2_SPORT_PID,
        SIMUCUBE_2_PRO_PID,
        SIMUCUBE_2_ULTIMATE_PID,
        SIMUCUBE_ACTIVE_PEDAL_PID,
        SIMUCUBE_WIRELESS_WHEEL_PID,
    ];
    let boot = [SIMUCUBE_1_BOOTLOADER_PID, SIMUCUBE_2_BOOTLOADER_PID];
    for b in &boot {
        assert!(
            !normal.contains(b),
            "bootloader PID {b:#06X} must not overlap with normal PIDs"
        );
    }
}
