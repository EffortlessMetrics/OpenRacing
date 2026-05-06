//! Snapshot tests for profile serialization formats

use openracing_profile::{
    AdvancedSettings, CurveType, CustomCurve, FfbSettings, InputSettings, LedMode, LimitSettings,
    WheelProfile, WheelSettings,
};

// --- Default settings snapshots ---

#[test]
fn snapshot_wheel_settings_default() {
    insta::assert_json_snapshot!("wheel_settings_default", WheelSettings::default());
}

#[test]
fn snapshot_ffb_settings_default() {
    insta::assert_json_snapshot!("ffb_settings_default", FfbSettings::default());
}

#[test]
fn snapshot_input_settings_default() {
    insta::assert_json_snapshot!("input_settings_default", InputSettings::default());
}

#[test]
fn snapshot_limit_settings_default() {
    insta::assert_json_snapshot!("limit_settings_default", LimitSettings::default());
}

#[test]
fn snapshot_advanced_settings_default() {
    insta::assert_json_snapshot!("advanced_settings_default", AdvancedSettings::default());
}

// --- Profile with all fields populated ---

#[test]
fn snapshot_wheel_settings_fully_populated() {
    let settings = WheelSettings {
        ffb: FfbSettings {
            overall_gain: 0.85,
            torque_limit: 15.0,
            spring_strength: 0.4,
            damper_strength: 0.6,
            friction_strength: 0.2,
            effects_enabled: true,
        },
        input: InputSettings {
            steering_range: 1080,
            steering_deadzone: 5,
            throttle_curve: CurveType::Exponential,
            brake_curve: CurveType::Logarithmic,
            clutch_curve: CurveType::Custom,
            custom_clutch_curve: Some(CustomCurve::default()),
            ..Default::default()
        },
        limits: LimitSettings {
            max_speed: Some(120.0),
            max_temp: Some(70),
            emergency_stop: true,
        },
        advanced: AdvancedSettings {
            filter_enabled: true,
            filter_strength: 0.75,
            led_mode: LedMode::Rpm,
            telemetry_enabled: true,
        },
    };
    insta::assert_json_snapshot!("wheel_settings_fully_populated", settings);
}

#[test]
fn snapshot_default_profile_serialization() {
    let profile = WheelProfile::new("Default Profile", "sim-wheel-001");
    // Redact non-deterministic fields (id, timestamps)
    insta::assert_json_snapshot!("default_profile", profile, {
        ".id" => "[uuid]",
        ".created_at" => "[timestamp]",
        ".modified_at" => "[timestamp]",
    });
}

#[test]
fn snapshot_profile_all_fields_populated() {
    let settings = WheelSettings {
        ffb: FfbSettings {
            overall_gain: 0.9,
            torque_limit: 20.0,
            spring_strength: 0.5,
            damper_strength: 0.5,
            friction_strength: 0.3,
            effects_enabled: true,
        },
        input: InputSettings {
            steering_range: 720,
            steering_deadzone: 2,
            throttle_curve: CurveType::Exponential,
            brake_curve: CurveType::Exponential,
            clutch_curve: CurveType::Linear,
            ..Default::default()
        },
        limits: LimitSettings {
            max_speed: Some(200.0),
            max_temp: Some(65),
            emergency_stop: true,
        },
        advanced: AdvancedSettings {
            filter_enabled: true,
            filter_strength: 0.8,
            led_mode: LedMode::Speed,
            telemetry_enabled: true,
        },
    };
    let profile = WheelProfile::new("Competition Profile", "fanatec-dd1").with_settings(settings);
    insta::assert_json_snapshot!("profile_all_fields_populated", profile, {
        ".id" => "[uuid]",
        ".created_at" => "[timestamp]",
        ".modified_at" => "[timestamp]",
    });
}
