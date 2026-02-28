//! Snapshot tests for curve LUTs using insta.

use openracing_curves::{BezierCurve, CurveLut, CurveType};

fn normalize_lut_for_snapshot(lut: &CurveLut) -> Vec<f32> {
    lut.table()
        .iter()
        .map(|&v| (v * 1000.0).round() / 1000.0)
        .collect()
}

#[test]
fn snapshot_linear_lut() {
    let lut = CurveLut::linear();
    let snapshot = normalize_lut_for_snapshot(&lut);
    insta::assert_json_snapshot!("linear_lut", snapshot);
}

#[test]
fn snapshot_exponential_2_lut() -> Result<(), Box<dyn std::error::Error>> {
    let curve = CurveType::exponential(2.0)?;
    let lut = curve.to_lut();
    let snapshot = normalize_lut_for_snapshot(&lut);
    insta::assert_json_snapshot!("exponential_2_lut", snapshot);
    Ok(())
}

#[test]
fn snapshot_exponential_05_lut() -> Result<(), Box<dyn std::error::Error>> {
    let curve = CurveType::exponential(0.5)?;
    let lut = curve.to_lut();
    let snapshot = normalize_lut_for_snapshot(&lut);
    insta::assert_json_snapshot!("exponential_05_lut", snapshot);
    Ok(())
}

#[test]
fn snapshot_logarithmic_10_lut() -> Result<(), Box<dyn std::error::Error>> {
    let curve = CurveType::logarithmic(10.0)?;
    let lut = curve.to_lut();
    let snapshot = normalize_lut_for_snapshot(&lut);
    insta::assert_json_snapshot!("logarithmic_10_lut", snapshot);
    Ok(())
}

#[test]
fn snapshot_logarithmic_e_lut() -> Result<(), Box<dyn std::error::Error>> {
    let curve = CurveType::logarithmic(std::f32::consts::E)?;
    let lut = curve.to_lut();
    let snapshot = normalize_lut_for_snapshot(&lut);
    insta::assert_json_snapshot!("logarithmic_e_lut", snapshot);
    Ok(())
}

#[test]
fn snapshot_bezier_linear_lut() {
    let curve = BezierCurve::linear();
    let lut = curve.to_lut();
    let snapshot = normalize_lut_for_snapshot(&lut);
    insta::assert_json_snapshot!("bezier_linear_lut", snapshot);
}

#[test]
fn snapshot_bezier_ease_in_lut() {
    let curve = BezierCurve::ease_in();
    let lut = curve.to_lut();
    let snapshot = normalize_lut_for_snapshot(&lut);
    insta::assert_json_snapshot!("bezier_ease_in_lut", snapshot);
}

#[test]
fn snapshot_bezier_ease_out_lut() {
    let curve = BezierCurve::ease_out();
    let lut = curve.to_lut();
    let snapshot = normalize_lut_for_snapshot(&lut);
    insta::assert_json_snapshot!("bezier_ease_out_lut", snapshot);
}

#[test]
fn snapshot_bezier_ease_in_out_lut() {
    let curve = BezierCurve::ease_in_out();
    let lut = curve.to_lut();
    let snapshot = normalize_lut_for_snapshot(&lut);
    insta::assert_json_snapshot!("bezier_ease_in_out_lut", snapshot);
}

#[test]
fn snapshot_bezier_s_curve_lut() -> Result<(), Box<dyn std::error::Error>> {
    let curve = BezierCurve::new([(0.0, 0.0), (0.25, 0.75), (0.75, 0.25), (1.0, 1.0)])?;
    let lut = curve.to_lut();
    let snapshot = normalize_lut_for_snapshot(&lut);
    insta::assert_json_snapshot!("bezier_s_curve_lut", snapshot);
    Ok(())
}

#[test]
fn snapshot_bezier_aggressive_lut() -> Result<(), Box<dyn std::error::Error>> {
    let curve = BezierCurve::new([(0.0, 0.0), (0.1, 0.9), (0.9, 0.1), (1.0, 1.0)])?;
    let lut = curve.to_lut();
    let snapshot = normalize_lut_for_snapshot(&lut);
    insta::assert_json_snapshot!("bezier_aggressive_lut", snapshot);
    Ok(())
}
