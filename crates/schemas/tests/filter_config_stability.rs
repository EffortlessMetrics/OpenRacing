//! Tests for FilterConfig stability at 1kHz update rates

use racing_wheel_schemas::prelude::FilterConfig;

#[test]
fn defaults_are_stable_at_1khz() {
    let config = FilterConfig::default();
    
    // Verify stable defaults that won't cause oscillation at 1kHz
    assert_eq!(config.reconstruction, 0, "Reconstruction should be 0 for stability");
    assert_eq!(config.friction.value(), 0.0, "Friction should be 0.0 for stability");
    assert_eq!(config.damper.value(), 0.0, "Damper should be 0.0 for stability");
    assert_eq!(config.inertia.value(), 0.0, "Inertia should be 0.0 for stability");
    assert_eq!(config.slew_rate.value(), 1.0, "Slew rate should be 1.0 (no limiting) for stability");
    
    // Verify no notch filters by default
    assert!(config.notch_filters.is_empty(), "No notch filters by default for stability");
    
    // Verify linear curve (no modification)
    assert_eq!(config.curve_points.len(), 2, "Should have linear curve by default");
    assert_eq!(config.curve_points[0].input, 0.0);
    assert_eq!(config.curve_points[0].output, 0.0);
    assert_eq!(config.curve_points[1].input, 1.0);
    assert_eq!(config.curve_points[1].output, 1.0);
    
    // Verify no torque cap by default
    assert_eq!(config.torque_cap.value(), 1.0, "No torque cap by default");
    
    // Verify the configuration is linear (no curve modification)
    assert!(config.is_linear(), "Default configuration should be linear");
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