//! Extended snapshot tests for PXN wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering hex-formatted
//! constant values, boundary device identification, and exhaustive
//! product name lookups.

use insta::assert_snapshot;
use racing_wheel_hid_pxn_protocol::{
    PRODUCT_GT987, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE, PRODUCT_V12_LITE_2, VENDOR_ID,
    is_pxn, product_name,
};

// ── Hex-formatted ID constants ───────────────────────────────────────────────

#[test]
fn test_snapshot_vendor_id_hex() {
    assert_snapshot!(format!("0x{VENDOR_ID:04X}"));
}

#[test]
fn test_snapshot_all_pids_hex() {
    let pids = [
        ("V10", PRODUCT_V10),
        ("V12", PRODUCT_V12),
        ("V12 Lite", PRODUCT_V12_LITE),
        ("V12 Lite 2", PRODUCT_V12_LITE_2),
        ("GT987", PRODUCT_GT987),
    ];
    let formatted: Vec<String> = pids
        .iter()
        .map(|(name, pid)| format!("{name}: 0x{pid:04X}"))
        .collect();
    assert_snapshot!(formatted.join(", "));
}

// ── Device identification boundary values ────────────────────────────────────

#[test]
fn test_snapshot_is_pxn_all_known_products() {
    let products: Vec<String> = [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ]
    .iter()
    .map(|&pid| format!("0x{pid:04X}={}", is_pxn(VENDOR_ID, pid)))
    .collect();
    assert_snapshot!(products.join(", "));
}

#[test]
fn test_snapshot_is_pxn_wrong_vid() {
    let results: Vec<String> = [0x0000u16, 0x16D0, 0x1FC9, 0xFFFF]
        .iter()
        .map(|&vid| format!("VID=0x{vid:04X}={}", is_pxn(vid, PRODUCT_V10)))
        .collect();
    assert_snapshot!(results.join(", "));
}

#[test]
fn test_snapshot_is_pxn_unknown_pid() {
    let results: Vec<String> = [0x0000u16, 0x0001, 0x1111, 0xFFFF]
        .iter()
        .map(|&pid| format!("0x{pid:04X}={}", is_pxn(VENDOR_ID, pid)))
        .collect();
    assert_snapshot!(results.join(", "));
}

// ── Product name exhaustive lookup ───────────────────────────────────────────

#[test]
fn test_snapshot_product_name_all_known() {
    let names: Vec<String> = [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ]
    .iter()
    .map(|&pid| format!("0x{pid:04X} -> {:?}", product_name(pid)))
    .collect();
    assert_snapshot!(names.join("\n"));
}

#[test]
fn test_snapshot_product_name_unknown() {
    let unknowns: Vec<String> = [0x0000u16, 0x0001, 0xFFFF]
        .iter()
        .map(|&pid| format!("0x{pid:04X} -> {:?}", product_name(pid)))
        .collect();
    assert_snapshot!(unknowns.join("\n"));
}

// ── VID/PID pair summary ─────────────────────────────────────────────────────

#[test]
fn test_snapshot_full_device_table() {
    let all_products = [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ];
    let table: Vec<String> = all_products
        .iter()
        .map(|&pid| {
            format!(
                "VID=0x{:04X} PID=0x{pid:04X} name={:?} match={}",
                VENDOR_ID,
                product_name(pid),
                is_pxn(VENDOR_ID, pid)
            )
        })
        .collect();
    assert_snapshot!(table.join("\n"));
}
