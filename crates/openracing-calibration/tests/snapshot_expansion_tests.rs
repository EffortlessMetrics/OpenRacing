//! Snapshot tests for CalibrationError — ensure error messages are stable.

use openracing_calibration::CalibrationError;

#[test]
fn snapshot_calibration_error_invalid_data() {
    let err = CalibrationError::InvalidData;
    insta::assert_snapshot!("calibration_error_invalid_data", format!("{}", err));
}

#[test]
fn snapshot_calibration_error_not_complete() {
    let err = CalibrationError::NotComplete;
    insta::assert_snapshot!("calibration_error_not_complete", format!("{}", err));
}

#[test]
fn snapshot_calibration_error_device_error() {
    let err = CalibrationError::DeviceError("USB timeout after 5000ms".to_string());
    insta::assert_snapshot!("calibration_error_device_error", format!("{}", err));
}

#[test]
fn snapshot_calibration_error_debug() {
    insta::assert_debug_snapshot!(
        "calibration_error_invalid_data_debug",
        CalibrationError::InvalidData
    );
    insta::assert_debug_snapshot!(
        "calibration_error_not_complete_debug",
        CalibrationError::NotComplete
    );
    insta::assert_debug_snapshot!(
        "calibration_error_device_error_debug",
        CalibrationError::DeviceError("sensor failure".to_string())
    );
}
