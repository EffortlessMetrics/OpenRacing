//! Profile validation

use crate::{ProfileResult, WheelProfile, WheelSettings};

pub fn validate_profile(profile: &WheelProfile) -> ProfileResult<()> {
    if profile.name.is_empty() {
        return Err(crate::ProfileError::ValidationError(
            "Profile name cannot be empty".to_string(),
        ));
    }

    if profile.device_id.is_empty() {
        return Err(crate::ProfileError::ValidationError(
            "Device ID cannot be empty".to_string(),
        ));
    }

    validate_settings(&profile.settings)?;

    Ok(())
}

pub fn validate_settings(settings: &WheelSettings) -> ProfileResult<()> {
    // FFB validation
    if !settings.ffb.overall_gain.is_finite()
        || settings.ffb.overall_gain < 0.0
        || settings.ffb.overall_gain > 1.0
    {
        return Err(crate::ProfileError::ValidationError(
            "FFB overall gain must be a finite value between 0.0 and 1.0".to_string(),
        ));
    }

    if !settings.ffb.torque_limit.is_finite()
        || settings.ffb.torque_limit < 0.0
        || settings.ffb.torque_limit > 100.0
    {
        return Err(crate::ProfileError::ValidationError(
            "Torque limit must be a finite value between 0.0 and 100.0".to_string(),
        ));
    }

    // Input validation
    if settings.input.steering_range < 90 || settings.input.steering_range > 3600 {
        return Err(crate::ProfileError::ValidationError(
            "Steering range must be between 90 and 3600 degrees".to_string(),
        ));
    }

    validate_custom_curve(
        settings.input.throttle_curve,
        settings.input.custom_throttle_curve.as_ref(),
        "throttle",
    )?;
    validate_custom_curve(
        settings.input.brake_curve,
        settings.input.custom_brake_curve.as_ref(),
        "brake",
    )?;
    validate_custom_curve(
        settings.input.clutch_curve,
        settings.input.custom_clutch_curve.as_ref(),
        "clutch",
    )?;

    // Advanced validation
    if !settings.advanced.filter_strength.is_finite()
        || settings.advanced.filter_strength < 0.0
        || settings.advanced.filter_strength > 1.0
    {
        return Err(crate::ProfileError::ValidationError(
            "Filter strength must be a finite value between 0.0 and 1.0".to_string(),
        ));
    }

    Ok(())
}

fn validate_custom_curve(
    curve_type: crate::CurveType,
    custom_curve: Option<&crate::tuning::CustomCurve>,
    name: &str,
) -> ProfileResult<()> {
    if curve_type != crate::CurveType::Custom {
        return Ok(());
    }

    let curve = custom_curve.ok_or_else(|| {
        crate::ProfileError::ValidationError(format!(
            "{} curve type is Custom but no custom curve data provided",
            name
        ))
    })?;

    if curve.points.len() < 2 {
        return Err(crate::ProfileError::ValidationError(format!(
            "{} custom curve must have at least 2 points",
            name
        )));
    }

    if curve.points.first().map(|p| p.x).unwrap_or(f32::NAN) > f32::EPSILON {
        return Err(crate::ProfileError::ValidationError(format!(
            "{} custom curve must start at x=0.0",
            name
        )));
    }

    if (curve.points.last().map(|p| p.x).unwrap_or(f32::NAN) - 1.0).abs() > f32::EPSILON {
        return Err(crate::ProfileError::ValidationError(format!(
            "{} custom curve must end at x=1.0",
            name
        )));
    }

    let mut last_x = -1.0;
    for point in &curve.points {
        if point.x <= last_x {
            return Err(crate::ProfileError::ValidationError(format!(
                "{} custom curve points must be strictly monotonically increasing on X",
                name
            )));
        }
        if !point.x.is_finite()
            || !point.y.is_finite()
            || point.x < 0.0
            || point.x > 1.0
            || point.y < 0.0
            || point.y > 1.0
        {
            return Err(crate::ProfileError::ValidationError(format!(
                "{} custom curve points must be finite and normalized between 0.0 and 1.0",
                name
            )));
        }
        last_x = point.x;
    }

    Ok(())
}

pub fn merge_profiles(base: &WheelProfile, overlay: &WheelProfile) -> WheelProfile {
    let mut result = base.clone();

    // Merge FFB settings
    if (overlay.settings.ffb.overall_gain - base.settings.ffb.overall_gain).abs() > f32::EPSILON {
        result.settings.ffb.overall_gain = overlay.settings.ffb.overall_gain;
    }
    if (overlay.settings.ffb.torque_limit - base.settings.ffb.torque_limit).abs() > f32::EPSILON {
        result.settings.ffb.torque_limit = overlay.settings.ffb.torque_limit;
    }
    if (overlay.settings.ffb.spring_strength - base.settings.ffb.spring_strength).abs()
        > f32::EPSILON
    {
        result.settings.ffb.spring_strength = overlay.settings.ffb.spring_strength;
    }
    if (overlay.settings.ffb.damper_strength - base.settings.ffb.damper_strength).abs()
        > f32::EPSILON
    {
        result.settings.ffb.damper_strength = overlay.settings.ffb.damper_strength;
    }
    if (overlay.settings.ffb.friction_strength - base.settings.ffb.friction_strength).abs()
        > f32::EPSILON
    {
        result.settings.ffb.friction_strength = overlay.settings.ffb.friction_strength;
    }
    if overlay.settings.ffb.effects_enabled != base.settings.ffb.effects_enabled {
        result.settings.ffb.effects_enabled = overlay.settings.ffb.effects_enabled;
    }

    // Merge input settings
    if overlay.settings.input.steering_range != base.settings.input.steering_range {
        result.settings.input.steering_range = overlay.settings.input.steering_range;
    }
    if overlay.settings.input.steering_deadzone != base.settings.input.steering_deadzone {
        result.settings.input.steering_deadzone = overlay.settings.input.steering_deadzone;
    }
    if overlay.settings.input.throttle_curve != base.settings.input.throttle_curve {
        result.settings.input.throttle_curve = overlay.settings.input.throttle_curve;
    }
    if overlay.settings.input.brake_curve != base.settings.input.brake_curve {
        result.settings.input.brake_curve = overlay.settings.input.brake_curve;
    }
    if overlay.settings.input.clutch_curve != base.settings.input.clutch_curve {
        result.settings.input.clutch_curve = overlay.settings.input.clutch_curve;
    }
    if overlay.settings.input.custom_throttle_curve != base.settings.input.custom_throttle_curve {
        result.settings.input.custom_throttle_curve =
            overlay.settings.input.custom_throttle_curve.clone();
    }
    if overlay.settings.input.custom_brake_curve != base.settings.input.custom_brake_curve {
        result.settings.input.custom_brake_curve =
            overlay.settings.input.custom_brake_curve.clone();
    }
    if overlay.settings.input.custom_clutch_curve != base.settings.input.custom_clutch_curve {
        result.settings.input.custom_clutch_curve =
            overlay.settings.input.custom_clutch_curve.clone();
    }

    // Merge limits
    if overlay.settings.limits.max_speed != base.settings.limits.max_speed {
        result.settings.limits.max_speed = overlay.settings.limits.max_speed;
    }
    if overlay.settings.limits.max_temp != base.settings.limits.max_temp {
        result.settings.limits.max_temp = overlay.settings.limits.max_temp;
    }
    if overlay.settings.limits.emergency_stop != base.settings.limits.emergency_stop {
        result.settings.limits.emergency_stop = overlay.settings.limits.emergency_stop;
    }

    // Merge advanced settings
    if overlay.settings.advanced.filter_enabled != base.settings.advanced.filter_enabled {
        result.settings.advanced.filter_enabled = overlay.settings.advanced.filter_enabled;
    }
    if (overlay.settings.advanced.filter_strength - base.settings.advanced.filter_strength).abs()
        > f32::EPSILON
    {
        result.settings.advanced.filter_strength = overlay.settings.advanced.filter_strength;
    }
    if overlay.settings.advanced.led_mode != base.settings.advanced.led_mode {
        result.settings.advanced.led_mode = overlay.settings.advanced.led_mode;
    }
    if overlay.settings.advanced.telemetry_enabled != base.settings.advanced.telemetry_enabled {
        result.settings.advanced.telemetry_enabled = overlay.settings.advanced.telemetry_enabled;
    }

    result.modified_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_profile_empty_name() {
        let mut profile = WheelProfile::new("Test", "device");
        profile.name = String::new();

        let result = validate_profile(&profile);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_profile_empty_device_id() {
        let mut profile = WheelProfile::new("Test", "device");
        profile.device_id = String::new();

        let result = validate_profile(&profile);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::ProfileError::ValidationError(_))
        ));
    }

    #[test]
    fn test_validate_profile_valid() {
        let profile = WheelProfile::new("Test", "device");

        let result = validate_profile(&profile);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_settings_invalid_gain() {
        let mut settings = WheelSettings::default();
        settings.ffb.overall_gain = 1.5;

        let result = validate_settings(&settings);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_settings_invalid_range() {
        let mut settings = WheelSettings::default();
        settings.input.steering_range = 5000;

        let result = validate_settings(&settings);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_settings_torque_at_boundaries() -> Result<(), Box<dyn std::error::Error>> {
        let mut settings = WheelSettings::default();

        settings.ffb.torque_limit = 0.0;
        validate_settings(&settings)?;

        settings.ffb.torque_limit = 100.0;
        validate_settings(&settings)?;

        Ok(())
    }

    #[test]
    fn test_validate_settings_torque_out_of_range() {
        let mut settings = WheelSettings::default();

        settings.ffb.torque_limit = -0.01;
        assert!(validate_settings(&settings).is_err());

        settings.ffb.torque_limit = 100.01;
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn test_validate_settings_filter_strength_at_boundaries()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut settings = WheelSettings::default();

        settings.advanced.filter_strength = 0.0;
        validate_settings(&settings)?;

        settings.advanced.filter_strength = 1.0;
        validate_settings(&settings)?;

        Ok(())
    }

    #[test]
    fn test_validate_settings_filter_strength_out_of_range() {
        let mut settings = WheelSettings::default();

        settings.advanced.filter_strength = -0.01;
        assert!(validate_settings(&settings).is_err());

        settings.advanced.filter_strength = 1.01;
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn test_validate_settings_first_error_wins() {
        // Both gain and steering range are invalid
        let mut settings = WheelSettings::default();
        settings.ffb.overall_gain = 5.0;
        settings.input.steering_range = 50;

        let result = validate_settings(&settings);
        assert!(result.is_err());
        // Should report gain error first (checked before steering)
        let err_msg = match result {
            Err(e) => format!("{}", e),
            Ok(()) => String::new(),
        };
        assert!(
            err_msg.contains("gain"),
            "first error should be about gain, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_merge_profiles() {
        let base = WheelProfile::new("Base", "device");
        let mut overlay = WheelProfile::new("Overlay", "device");
        overlay.settings.ffb.overall_gain = 0.5;

        let merged = merge_profiles(&base, &overlay);

        assert_eq!(merged.settings.ffb.overall_gain, 0.5);
    }

    #[test]
    fn test_merge_identical_profiles_preserves_values() {
        let base = WheelProfile::new("Same", "device");
        let overlay = base.clone();
        let merged = merge_profiles(&base, &overlay);

        // When overlay == base, merged keeps base values
        assert!(
            (merged.settings.ffb.overall_gain - base.settings.ffb.overall_gain).abs()
                < f32::EPSILON
        );
        assert_eq!(
            merged.settings.input.steering_range,
            base.settings.input.steering_range
        );
        assert_eq!(
            merged.settings.ffb.torque_limit,
            base.settings.ffb.torque_limit
        );
    }

    #[test]
    fn test_merge_updates_modified_at() {
        let base = WheelProfile::new("Base", "device");
        let overlay = WheelProfile::new("Overlay", "device");
        let merged = merge_profiles(&base, &overlay);

        assert!(
            merged.modified_at >= base.modified_at,
            "merged modified_at should be >= base"
        );
    }

    #[test]
    fn test_validate_profile_with_invalid_settings_fails() {
        let mut profile = WheelProfile::new("Test", "device");
        profile.settings.ffb.overall_gain = -1.0;

        let result = validate_profile(&profile);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Custom curve validation unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_custom_curve_type_without_data_rejected() {
        let mut settings = WheelSettings::default();
        settings.input.throttle_curve = crate::CurveType::Custom;
        settings.input.custom_throttle_curve = None;
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn test_custom_curve_too_few_points_rejected() {
        let mut settings = WheelSettings::default();
        settings.input.brake_curve = crate::CurveType::Custom;
        settings.input.custom_brake_curve = Some(crate::tuning::CustomCurve::new(vec![
            crate::tuning::CurvePoint::new(0.0, 0.0),
        ]));
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn test_custom_curve_non_monotonic_rejected() {
        let mut settings = WheelSettings::default();
        settings.input.clutch_curve = crate::CurveType::Custom;
        settings.input.custom_clutch_curve = Some(crate::tuning::CustomCurve::new(vec![
            crate::tuning::CurvePoint::new(0.0, 0.0),
            crate::tuning::CurvePoint::new(0.8, 0.5),
            crate::tuning::CurvePoint::new(0.5, 0.9), // non-monotonic
            crate::tuning::CurvePoint::new(1.0, 1.0),
        ]));
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn test_custom_curve_out_of_range_rejected() {
        let mut settings = WheelSettings::default();
        settings.input.throttle_curve = crate::CurveType::Custom;
        settings.input.custom_throttle_curve = Some(crate::tuning::CustomCurve::new(vec![
            crate::tuning::CurvePoint::new(0.0, 0.0),
            crate::tuning::CurvePoint::new(0.5, 1.5), // y > 1.0
            crate::tuning::CurvePoint::new(1.0, 1.0),
        ]));
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn test_custom_curve_wrong_start_rejected() {
        let mut settings = WheelSettings::default();
        settings.input.throttle_curve = crate::CurveType::Custom;
        settings.input.custom_throttle_curve = Some(crate::tuning::CustomCurve::new(vec![
            crate::tuning::CurvePoint::new(0.1, 0.0), // doesn't start at 0.0
            crate::tuning::CurvePoint::new(1.0, 1.0),
        ]));
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn test_custom_curve_wrong_end_rejected() {
        let mut settings = WheelSettings::default();
        settings.input.throttle_curve = crate::CurveType::Custom;
        settings.input.custom_throttle_curve = Some(crate::tuning::CustomCurve::new(vec![
            crate::tuning::CurvePoint::new(0.0, 0.0),
            crate::tuning::CurvePoint::new(0.9, 1.0), // doesn't end at x=1.0
        ]));
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn test_valid_custom_curve_accepted() -> Result<(), Box<dyn std::error::Error>> {
        let mut settings = WheelSettings::default();
        settings.input.throttle_curve = crate::CurveType::Custom;
        settings.input.custom_throttle_curve = Some(crate::tuning::CustomCurve::new(vec![
            crate::tuning::CurvePoint::new(0.0, 0.0),
            crate::tuning::CurvePoint::new(0.25, 0.1),
            crate::tuning::CurvePoint::new(0.5, 0.4),
            crate::tuning::CurvePoint::new(0.75, 0.8),
            crate::tuning::CurvePoint::new(1.0, 1.0),
        ]));
        validate_settings(&settings)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // NaN / Infinity rejection for all float fields
    // -----------------------------------------------------------------------

    #[test]
    fn test_gain_nan_rejected() {
        let mut s = WheelSettings::default();
        s.ffb.overall_gain = f32::NAN;
        assert!(validate_settings(&s).is_err(), "NaN gain must be rejected");
    }

    #[test]
    fn test_torque_limit_nan_rejected() {
        let mut s = WheelSettings::default();
        s.ffb.torque_limit = f32::NAN;
        assert!(
            validate_settings(&s).is_err(),
            "NaN torque limit must be rejected"
        );
    }

    #[test]
    fn test_torque_limit_infinity_rejected() {
        let mut s = WheelSettings::default();
        s.ffb.torque_limit = f32::INFINITY;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn test_filter_strength_infinity_rejected() {
        let mut s = WheelSettings::default();
        s.advanced.filter_strength = f32::NEG_INFINITY;
        assert!(validate_settings(&s).is_err());
    }

    #[test]
    fn test_custom_curve_nan_point_rejected() {
        let mut settings = WheelSettings::default();
        settings.input.throttle_curve = crate::CurveType::Custom;
        settings.input.custom_throttle_curve = Some(crate::tuning::CustomCurve::new(vec![
            crate::tuning::CurvePoint::new(0.0, 0.0),
            crate::tuning::CurvePoint::new(f32::NAN, 0.5),
            crate::tuning::CurvePoint::new(1.0, 1.0),
        ]));
        assert!(
            validate_settings(&settings).is_err(),
            "NaN in curve point X must be rejected"
        );
    }

    #[test]
    fn test_custom_curve_infinity_point_rejected() {
        let mut settings = WheelSettings::default();
        settings.input.brake_curve = crate::CurveType::Custom;
        settings.input.custom_brake_curve = Some(crate::tuning::CustomCurve::new(vec![
            crate::tuning::CurvePoint::new(0.0, 0.0),
            crate::tuning::CurvePoint::new(0.5, f32::INFINITY),
            crate::tuning::CurvePoint::new(1.0, 1.0),
        ]));
        assert!(
            validate_settings(&settings).is_err(),
            "Infinity in curve point Y must be rejected"
        );
    }

    #[test]
    fn test_merge_profiles_exhaustive() {
        let base = WheelProfile::new("Base", "device");
        let mut overlay = WheelProfile::new("Overlay", "device");

        overlay.settings.ffb.spring_strength = 0.7;
        overlay.settings.input.throttle_curve = crate::CurveType::Custom;
        overlay.settings.input.custom_throttle_curve = Some(crate::tuning::CustomCurve::default());
        overlay.settings.advanced.led_mode = crate::LedMode::Rpm;

        let merged = merge_profiles(&base, &overlay);

        assert!((merged.settings.ffb.spring_strength - 0.7).abs() < f32::EPSILON);
        assert_eq!(
            merged.settings.input.throttle_curve,
            crate::CurveType::Custom
        );
        assert!(merged.settings.input.custom_throttle_curve.is_some());
        assert_eq!(merged.settings.advanced.led_mode, crate::LedMode::Rpm);
    }
}
