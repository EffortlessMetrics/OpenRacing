//! Snapshot tests for pipeline and filter chain configuration

use racing_wheel_schemas::entities::{BumpstopConfig, FilterConfig, HandsOffConfig};

#[test]
fn snapshot_filter_config_default() {
    insta::assert_json_snapshot!("filter_config_default", FilterConfig::default());
}

#[test]
fn snapshot_bumpstop_config_default() {
    insta::assert_json_snapshot!("bumpstop_config_default", BumpstopConfig::default());
}

#[test]
fn snapshot_hands_off_config_default() {
    insta::assert_json_snapshot!("hands_off_config_default", HandsOffConfig::default());
}

#[test]
fn snapshot_filter_config_with_custom_bumpstop() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_schemas::domain::{CurvePoint, Gain};

    let config = FilterConfig::new_complete(
        4,
        Gain::new(0.3)?,
        Gain::new(0.5)?,
        Gain::new(0.2)?,
        vec![],
        Gain::new(0.8)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(0.9)?,
        BumpstopConfig {
            enabled: true,
            start_angle: 400.0,
            max_angle: 500.0,
            stiffness: 0.9,
            damping: 0.4,
        },
        HandsOffConfig {
            enabled: true,
            threshold: 0.08,
            timeout_seconds: 3.0,
        },
    )?;
    insta::assert_json_snapshot!("filter_config_custom", config);
    Ok(())
}
