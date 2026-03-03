//! Snapshot tests for CurveError — ensure error messages are stable.

use openracing_curves::CurveError;

#[test]
fn snapshot_curve_error_control_point_out_of_range() {
    let err = CurveError::ControlPointOutOfRange {
        point_index: 2,
        coordinate: "x",
        value: 1.5,
    };
    insta::assert_snapshot!("curve_error_control_point_oor", format!("{}", err));
}

#[test]
fn snapshot_curve_error_control_point_y() {
    let err = CurveError::ControlPointOutOfRange {
        point_index: 0,
        coordinate: "y",
        value: -0.1,
    };
    insta::assert_snapshot!("curve_error_control_point_y_oor", format!("{}", err));
}

#[test]
fn snapshot_curve_error_invalid_configuration() {
    let err = CurveError::InvalidConfiguration("exponent must be positive".to_string());
    insta::assert_snapshot!("curve_error_invalid_config", format!("{}", err));
}

#[test]
fn snapshot_curve_error_debug() {
    insta::assert_debug_snapshot!(
        "curve_error_control_point_debug",
        CurveError::ControlPointOutOfRange {
            point_index: 3,
            coordinate: "x",
            value: 2.0,
        }
    );
    insta::assert_debug_snapshot!(
        "curve_error_invalid_config_debug",
        CurveError::InvalidConfiguration("logarithmic base must be > 1".to_string())
    );
}
