//! Tests for config::FilterConfig stability at 1kHz update rates

use racing_wheel_schemas::config::FilterConfig;

#[test]
fn defaults_are_stable_at_1khz() {
    let config = FilterConfig::default();

    // Verify stable defaults that won't cause oscillation at 1kHz
    assert_eq!(
        config.reconstruction, 0,
        "Reconstruction should be 0 for stability"
    );
    assert_eq!(config.friction, 0.0, "Friction should be 0.0 for stability");
    assert_eq!(config.damper, 0.0, "Damper should be 0.0 for stability");
    assert_eq!(config.inertia, 0.0, "Inertia should be 0.0 for stability");
    assert_eq!(
        config.slew_rate, 1.0,
        "Slew rate should be 1.0 (no limiting) for stability"
    );

    // Verify explicit torque cap for test predictability
    assert_eq!(
        config.torque_cap,
        Some(10.0),
        "Torque cap should be Some(10.0) for test predictability"
    );

    // Verify no notch filters by default
    assert!(
        config.notch_filters.is_empty(),
        "No notch filters by default for stability"
    );

    // Verify linear curve (no modification)
    assert_eq!(
        config.curve_points.len(),
        2,
        "Should have linear curve by default"
    );
    assert_eq!(config.curve_points[0].input, 0.0);
    assert_eq!(config.curve_points[0].output, 0.0);
    assert_eq!(config.curve_points[1].input, 1.0);
    assert_eq!(config.curve_points[1].output, 1.0);

    // Verify bumpstop and hands_off use their defaults
    assert!(
        config.bumpstop.enabled,
        "Bumpstop should be enabled by default"
    );
    assert_eq!(
        config.bumpstop.strength, 0.5,
        "Bumpstop strength should use default"
    );

    assert!(
        config.hands_off.enabled,
        "Hands-off detection should be enabled by default"
    );
    assert_eq!(
        config.hands_off.sensitivity, 0.3,
        "Hands-off sensitivity should use default"
    );
}

#[test]
fn filter_config_compiles_with_defaults() {
    // This test ensures FilterConfig::default() compiles successfully
    let _config = FilterConfig::default();

    // Test that we can create a FilterConfig and it has expected properties
    let config = FilterConfig::default();

    // Verify all required fields are present and accessible
    let _ = config.reconstruction;
    let _ = config.friction;
    let _ = config.damper;
    let _ = config.inertia;
    let _ = config.notch_filters;
    let _ = config.slew_rate;
    let _ = config.curve_points;
    let _ = config.torque_cap;
    let _ = config.bumpstop;
    let _ = config.hands_off;
}

#[test]
fn filter_config_stability_properties() {
    let config = FilterConfig::default();

    // Test that the configuration represents a stable, pass-through filter
    // that won't introduce oscillations at 1kHz

    // No filtering or modification should be applied
    assert_eq!(config.reconstruction, 0, "No reconstruction filtering");
    assert_eq!(config.friction, 0.0, "No friction applied");
    assert_eq!(config.damper, 0.0, "No damping applied");
    assert_eq!(config.inertia, 0.0, "No inertia simulation");

    // No rate limiting
    assert_eq!(config.slew_rate, 1.0, "No slew rate limiting");

    // Linear curve (1:1 mapping)
    let curve = &config.curve_points;
    assert_eq!(curve.len(), 2, "Linear curve has exactly 2 points");

    // Verify it's a perfect linear mapping
    let input_range = curve[1].input - curve[0].input;
    let output_range = curve[1].output - curve[0].output;
    assert!(
        (input_range - 1.0).abs() < f32::EPSILON,
        "Input range should be 1.0"
    );
    assert!(
        (output_range - 1.0).abs() < f32::EPSILON,
        "Output range should be 1.0"
    );
    assert!(
        (curve[0].input - 0.0).abs() < f32::EPSILON,
        "Curve starts at 0.0"
    );
    assert!(
        (curve[0].output - 0.0).abs() < f32::EPSILON,
        "Curve output starts at 0.0"
    );

    // No frequency-based filtering
    assert!(
        config.notch_filters.is_empty(),
        "No notch filters that could cause resonance"
    );
}
