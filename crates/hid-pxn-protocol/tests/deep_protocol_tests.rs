//! Deep protocol tests for PXN HID protocol.
//!
//! Tests cover device identification (V10/V12/V12 Lite/GT987),
//! product naming, and VID/PID validation.

use racing_wheel_hid_pxn_protocol::{
    is_pxn, product_name, PRODUCT_GT987, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE,
    PRODUCT_V12_LITE_2, VENDOR_ID,
};

// ─── Device identification ───────────────────────────────────────────────────

#[test]
fn vendor_id_is_lite_star() {
    assert_eq!(VENDOR_ID, 0x11FF);
}

#[test]
fn v10_product_id() {
    assert_eq!(PRODUCT_V10, 0x3245);
}

#[test]
fn v12_product_id() {
    assert_eq!(PRODUCT_V12, 0x1212);
}

#[test]
fn v12_lite_product_id() {
    assert_eq!(PRODUCT_V12_LITE, 0x1112);
}

#[test]
fn v12_lite_2_product_id() {
    assert_eq!(PRODUCT_V12_LITE_2, 0x1211);
}

#[test]
fn gt987_product_id() {
    assert_eq!(PRODUCT_GT987, 0x2141);
}

// ─── is_pxn identification ──────────────────────────────────────────────────

#[test]
fn is_pxn_recognises_all_known_products() {
    assert!(is_pxn(VENDOR_ID, PRODUCT_V10));
    assert!(is_pxn(VENDOR_ID, PRODUCT_V12));
    assert!(is_pxn(VENDOR_ID, PRODUCT_V12_LITE));
    assert!(is_pxn(VENDOR_ID, PRODUCT_V12_LITE_2));
    assert!(is_pxn(VENDOR_ID, PRODUCT_GT987));
}

#[test]
fn is_pxn_rejects_wrong_vendor() {
    assert!(!is_pxn(0x0000, PRODUCT_V10));
    assert!(!is_pxn(0x1234, PRODUCT_V12));
    assert!(!is_pxn(0xFFFF, PRODUCT_GT987));
}

#[test]
fn is_pxn_rejects_unknown_pid_with_correct_vid() {
    assert!(!is_pxn(VENDOR_ID, 0x0000));
    assert!(!is_pxn(VENDOR_ID, 0xFFFF));
    assert!(!is_pxn(VENDOR_ID, 0x1234));
}

#[test]
fn is_pxn_rejects_both_wrong() {
    assert!(!is_pxn(0x0000, 0x0000));
}

// ─── Product names ──────────────────────────────────────────────────────────

#[test]
fn product_name_v10() {
    assert_eq!(product_name(PRODUCT_V10), Some("PXN V10"));
}

#[test]
fn product_name_v12() {
    assert_eq!(product_name(PRODUCT_V12), Some("PXN V12"));
}

#[test]
fn product_name_v12_lite() {
    assert_eq!(product_name(PRODUCT_V12_LITE), Some("PXN V12 Lite"));
}

#[test]
fn product_name_v12_lite_se() {
    assert_eq!(product_name(PRODUCT_V12_LITE_2), Some("PXN V12 Lite (SE)"));
}

#[test]
fn product_name_gt987() {
    assert_eq!(product_name(PRODUCT_GT987), Some("Lite Star GT987 FF"));
}

#[test]
fn product_name_unknown_returns_none() {
    assert_eq!(product_name(0x0000), None);
    assert_eq!(product_name(0xFFFF), None);
    assert_eq!(product_name(0x1234), None);
}

// ─── PID uniqueness ─────────────────────────────────────────────────────────

#[test]
fn all_product_ids_unique() {
    let pids = [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ];
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(pids[i], pids[j], "PIDs at index {i} and {j} must differ");
        }
    }
}

#[test]
fn all_known_pids_have_names() {
    let pids = [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ];
    for pid in &pids {
        assert!(
            product_name(*pid).is_some(),
            "PID 0x{pid:04X} should have a name"
        );
    }
}

#[test]
fn product_names_non_empty() {
    let pids = [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ];
    for pid in &pids {
        if let Some(name) = product_name(*pid) {
            assert!(!name.is_empty());
        }
    }
}
