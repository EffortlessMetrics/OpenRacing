//! Snapshot tests for the PXN HID protocol.
//!
//! These tests lock in device properties and protocol constants to catch
//! accidental regressions in device classification and identification.

use insta::assert_debug_snapshot;
use racing_wheel_hid_pxn_protocol as pxn;

// ── Product name snapshots for every known PID ───────────────────────────────

#[test]
fn test_snapshot_product_name_v10() {
    assert_debug_snapshot!(pxn::product_name(pxn::PRODUCT_V10));
}

#[test]
fn test_snapshot_product_name_v12() {
    assert_debug_snapshot!(pxn::product_name(pxn::PRODUCT_V12));
}

#[test]
fn test_snapshot_product_name_v12_lite() {
    assert_debug_snapshot!(pxn::product_name(pxn::PRODUCT_V12_LITE));
}

#[test]
fn test_snapshot_product_name_v12_lite_2() {
    assert_debug_snapshot!(pxn::product_name(pxn::PRODUCT_V12_LITE_2));
}

#[test]
fn test_snapshot_product_name_gt987() {
    assert_debug_snapshot!(pxn::product_name(pxn::PRODUCT_GT987));
}

#[test]
fn test_snapshot_product_name_unknown() {
    assert_debug_snapshot!(pxn::product_name(0xFFFF));
}

// ── Protocol constants snapshot ──────────────────────────────────────────────

#[test]
fn test_snapshot_protocol_constants() {
    assert_debug_snapshot!(format!(
        "VENDOR_ID={:#06X}, V10={:#06X}, V12={:#06X}, V12_LITE={:#06X}, \
         V12_LITE_2={:#06X}, GT987={:#06X}",
        pxn::VENDOR_ID,
        pxn::PRODUCT_V10,
        pxn::PRODUCT_V12,
        pxn::PRODUCT_V12_LITE,
        pxn::PRODUCT_V12_LITE_2,
        pxn::PRODUCT_GT987,
    ));
}

// ── is_pxn results snapshot ──────────────────────────────────────────────────

#[test]
fn test_snapshot_is_pxn() {
    let results = [
        ("v10_correct", pxn::is_pxn(pxn::VENDOR_ID, pxn::PRODUCT_V10)),
        ("v12_correct", pxn::is_pxn(pxn::VENDOR_ID, pxn::PRODUCT_V12)),
        (
            "v12_lite_correct",
            pxn::is_pxn(pxn::VENDOR_ID, pxn::PRODUCT_V12_LITE),
        ),
        (
            "v12_lite_2_correct",
            pxn::is_pxn(pxn::VENDOR_ID, pxn::PRODUCT_V12_LITE_2),
        ),
        (
            "gt987_correct",
            pxn::is_pxn(pxn::VENDOR_ID, pxn::PRODUCT_GT987),
        ),
        ("wrong_vid", pxn::is_pxn(0x0000, pxn::PRODUCT_V10)),
        ("wrong_pid", pxn::is_pxn(pxn::VENDOR_ID, 0xFFFF)),
    ];
    assert_debug_snapshot!(format!("{results:?}"));
}

// ── All known PIDs list snapshot ─────────────────────────────────────────────

#[test]
fn test_snapshot_all_known_pids() {
    let pids = [
        ("V10", pxn::PRODUCT_V10),
        ("V12", pxn::PRODUCT_V12),
        ("V12_LITE", pxn::PRODUCT_V12_LITE),
        ("V12_LITE_2", pxn::PRODUCT_V12_LITE_2),
        ("GT987", pxn::PRODUCT_GT987),
    ];
    let summary: Vec<String> = pids
        .iter()
        .map(|(name, pid)| format!("{name}={pid:#06x}"))
        .collect();
    assert_debug_snapshot!(summary.join(", "));
}

// ── Product names summary snapshot ───────────────────────────────────────────

#[test]
fn test_snapshot_product_names() {
    let results = [
        ("V10", pxn::product_name(pxn::PRODUCT_V10)),
        ("V12", pxn::product_name(pxn::PRODUCT_V12)),
        ("V12_LITE", pxn::product_name(pxn::PRODUCT_V12_LITE)),
        ("V12_LITE_2", pxn::product_name(pxn::PRODUCT_V12_LITE_2)),
        ("GT987", pxn::product_name(pxn::PRODUCT_GT987)),
        ("unknown", pxn::product_name(0xFFFF)),
    ];
    assert_debug_snapshot!(format!("{results:?}"));
}
