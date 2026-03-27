//! Snapshot tests for OpenFFBoard device variants and protocol constants.
//!
//! Pins the exact representation of each variant (name, PID, VID) and
//! the protocol constant summary to prevent accidental regressions.

use insta::assert_snapshot;
use racing_wheel_hid_openffboard_protocol::{
    CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN, GAIN_REPORT_ID, OPENFFBOARD_PRODUCT_ID,
    OPENFFBOARD_PRODUCT_ID_ALT, OPENFFBOARD_VENDOR_ID, OpenFFBoardVariant,
};

// -- Individual variant snapshots ---------------------------------------------

#[test]
fn snapshot_variant_main() {
    let v = OpenFFBoardVariant::Main;
    assert_snapshot!(format!(
        "name={}, VID={:#06X}, PID={:#06X}",
        v.name(),
        v.vendor_id(),
        v.product_id()
    ));
}

#[test]
fn snapshot_variant_alternate() {
    let v = OpenFFBoardVariant::Alternate;
    assert_snapshot!(format!(
        "name={}, VID={:#06X}, PID={:#06X}",
        v.name(),
        v.vendor_id(),
        v.product_id()
    ));
}

// -- All variants combined ----------------------------------------------------

#[test]
fn snapshot_all_variants() {
    let lines: Vec<String> = OpenFFBoardVariant::ALL
        .iter()
        .map(|v| {
            format!(
                "{:?}: name={}, VID={:#06X}, PID={:#06X}",
                v,
                v.name(),
                v.vendor_id(),
                v.product_id()
            )
        })
        .collect();
    assert_snapshot!(lines.join("\n"));
}

// -- Variant debug representations --------------------------------------------

#[test]
fn snapshot_variant_main_debug() {
    assert_snapshot!(format!("{:?}", OpenFFBoardVariant::Main));
}

#[test]
fn snapshot_variant_alternate_debug() {
    assert_snapshot!(format!("{:?}", OpenFFBoardVariant::Alternate));
}

// -- Protocol constants summary -----------------------------------------------

#[test]
fn snapshot_full_protocol_constants() {
    assert_snapshot!(format!(
        concat!(
            "VENDOR_ID={:#06X}\n",
            "PRODUCT_ID={:#06X}\n",
            "PRODUCT_ID_ALT={:#06X}\n",
            "CONSTANT_FORCE_REPORT_ID={:#04X}\n",
            "CONSTANT_FORCE_REPORT_LEN={}\n",
            "GAIN_REPORT_ID={:#04X}\n",
            "VARIANT_COUNT={}",
        ),
        OPENFFBOARD_VENDOR_ID,
        OPENFFBOARD_PRODUCT_ID,
        OPENFFBOARD_PRODUCT_ID_ALT,
        CONSTANT_FORCE_REPORT_ID,
        CONSTANT_FORCE_REPORT_LEN,
        GAIN_REPORT_ID,
        OpenFFBoardVariant::ALL.len(),
    ));
}
