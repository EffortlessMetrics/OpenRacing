//! Additional insta snapshot tests for the PXN HID protocol.
//!
//! Complements `snapshot_tests.rs` with boundary-value device detection,
//! vendor ID formatting, and sorted PID listing.

use insta::assert_debug_snapshot;
use racing_wheel_hid_pxn_protocol as pxn;

#[test]
fn snapshot_vendor_id_formats() {
    assert_debug_snapshot!(format!(
        "decimal={}, hex={:#06X}, binary={:#018b}",
        pxn::VENDOR_ID,
        pxn::VENDOR_ID,
        pxn::VENDOR_ID
    ));
}

#[test]
fn snapshot_all_products_classification() {
    let pids: &[u16] = &[
        pxn::PRODUCT_V10,
        pxn::PRODUCT_V12,
        pxn::PRODUCT_V12_LITE,
        pxn::PRODUCT_V12_LITE_2,
        pxn::PRODUCT_GT987,
        0x0000,
        0xFFFF,
    ];
    let summary: Vec<String> = pids
        .iter()
        .map(|pid| {
            let name = pxn::product_name(*pid).unwrap_or("(none)");
            let matched = pxn::is_pxn(pxn::VENDOR_ID, *pid);
            format!("PID={pid:#06X}: name={name}, is_pxn={matched}")
        })
        .collect();
    assert_debug_snapshot!(summary);
}

#[test]
fn snapshot_is_pxn_boundary_vids() {
    let results = [
        ("vid_minus_one", pxn::is_pxn(pxn::VENDOR_ID - 1, pxn::PRODUCT_V10)),
        ("vid_exact", pxn::is_pxn(pxn::VENDOR_ID, pxn::PRODUCT_V10)),
        ("vid_plus_one", pxn::is_pxn(pxn::VENDOR_ID + 1, pxn::PRODUCT_V10)),
        ("vid_zero", pxn::is_pxn(0x0000, pxn::PRODUCT_V12)),
        ("vid_max", pxn::is_pxn(0xFFFF, pxn::PRODUCT_GT987)),
    ];
    assert_debug_snapshot!(format!("{results:?}"));
}

#[test]
fn snapshot_pids_sorted_by_value() {
    let mut pids = [
        ("V10", pxn::PRODUCT_V10),
        ("V12", pxn::PRODUCT_V12),
        ("V12_LITE", pxn::PRODUCT_V12_LITE),
        ("V12_LITE_2", pxn::PRODUCT_V12_LITE_2),
        ("GT987", pxn::PRODUCT_GT987),
    ];
    pids.sort_by_key(|(_name, pid)| *pid);
    let summary: Vec<String> = pids
        .iter()
        .map(|(name, pid)| format!("{name}={pid:#06X}"))
        .collect();
    assert_debug_snapshot!(summary);
}
