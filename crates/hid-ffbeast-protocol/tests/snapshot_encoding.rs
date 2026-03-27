//! Snapshot tests for FFBeast output encoding at additional data points.
//!
//! Complements snapshot_tests.rs with individual ID constant snapshots,
//! gain at boundary/mid values, and small/three-quarter torque values.

use insta::assert_snapshot;
use racing_wheel_hid_ffbeast_protocol::{
    build_set_gain, FFBeastTorqueEncoder, FFBEAST_PRODUCT_ID_JOYSTICK, FFBEAST_PRODUCT_ID_RUDDER,
    FFBEAST_PRODUCT_ID_WHEEL, FFBEAST_VENDOR_ID,
};

// -- Individual ID constants --------------------------------------------------

#[test]
fn snapshot_vendor_id() {
    assert_snapshot!(format!("{:#06X}", FFBEAST_VENDOR_ID));
}

#[test]
fn snapshot_product_id_joystick() {
    assert_snapshot!(format!("{:#06X}", FFBEAST_PRODUCT_ID_JOYSTICK));
}

#[test]
fn snapshot_product_id_rudder() {
    assert_snapshot!(format!("{:#06X}", FFBEAST_PRODUCT_ID_RUDDER));
}

#[test]
fn snapshot_product_id_wheel() {
    assert_snapshot!(format!("{:#06X}", FFBEAST_PRODUCT_ID_WHEEL));
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
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(0.01);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn snapshot_encode_tiny_negative() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(-0.01);
    assert_snapshot!(format!("{:?}", report));
}

// -- Torque at half values ----------------------------------------------------

#[test]
fn snapshot_encode_half_positive() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(0.5);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn snapshot_encode_half_negative() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(-0.5);
    assert_snapshot!(format!("{:?}", report));
}

// -- Torque at three-quarter values -------------------------------------------

#[test]
fn snapshot_encode_three_quarter_positive() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(0.75);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn snapshot_encode_three_quarter_negative() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(-0.75);
    assert_snapshot!(format!("{:?}", report));
}
