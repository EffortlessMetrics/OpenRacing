//! Snapshot tests for OpenFFBoard output encoding at additional data points.
//!
//! Complements snapshot_tests.rs with individual ID constant snapshots,
//! gain at boundary/mid values, and small/three-quarter torque values.

use insta::assert_snapshot;
use racing_wheel_hid_openffboard_protocol::{
    build_set_gain, OpenFFBoardTorqueEncoder, OPENFFBOARD_PRODUCT_ID,
    OPENFFBOARD_PRODUCT_ID_ALT, OPENFFBOARD_VENDOR_ID,
};

// -- Individual ID constants --------------------------------------------------

#[test]
fn snapshot_vendor_id() {
    assert_snapshot!(format!("{:#06X}", OPENFFBOARD_VENDOR_ID));
}

#[test]
fn snapshot_product_id_main() {
    assert_snapshot!(format!("{:#06X}", OPENFFBOARD_PRODUCT_ID));
}

#[test]
fn snapshot_product_id_alt() {
    assert_snapshot!(format!("{:#06X}", OPENFFBOARD_PRODUCT_ID_ALT));
}

// -- Gain at boundary and mid values ------------------------------------------

#[test]
fn snapshot_set_gain_zero() {
    let report = build_set_gain(0);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn snapshot_set_gain_mid() {
    let report = build_set_gain(128);
    assert_snapshot!(format!("{:?}", report));
}

// -- Torque at small values ---------------------------------------------------

#[test]
fn snapshot_encode_tiny_positive() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.01);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn snapshot_encode_tiny_negative() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-0.01);
    assert_snapshot!(format!("{:?}", report));
}

// -- Torque at three-quarter values -------------------------------------------

#[test]
fn snapshot_encode_three_quarter_positive() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.75);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn snapshot_encode_three_quarter_negative() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-0.75);
    assert_snapshot!(format!("{:?}", report));
}
