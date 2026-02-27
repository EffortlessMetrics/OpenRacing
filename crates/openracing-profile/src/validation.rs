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
    if settings.ffb.overall_gain < 0.0 || settings.ffb.overall_gain > 1.0 {
        return Err(crate::ProfileError::ValidationError(
            "FFB overall gain must be between 0.0 and 1.0".to_string(),
        ));
    }

    if settings.ffb.torque_limit < 0.0 || settings.ffb.torque_limit > 100.0 {
        return Err(crate::ProfileError::ValidationError(
            "Torque limit must be between 0.0 and 100.0".to_string(),
        ));
    }

    // Input validation
    if settings.input.steering_range < 90 || settings.input.steering_range > 3600 {
        return Err(crate::ProfileError::ValidationError(
            "Steering range must be between 90 and 3600 degrees".to_string(),
        ));
    }

    // Advanced validation
    if settings.advanced.filter_strength < 0.0 || settings.advanced.filter_strength > 1.0 {
        return Err(crate::ProfileError::ValidationError(
            "Filter strength must be between 0.0 and 1.0".to_string(),
        ));
    }

    Ok(())
}

pub fn merge_profiles(base: &WheelProfile, overlay: &WheelProfile) -> WheelProfile {
    let mut result = base.clone();

    // Merge FFB settings
    if overlay.settings.ffb.overall_gain != base.settings.ffb.overall_gain {
        result.settings.ffb.overall_gain = overlay.settings.ffb.overall_gain;
    }

    if overlay.settings.ffb.torque_limit != base.settings.ffb.torque_limit {
        result.settings.ffb.torque_limit = overlay.settings.ffb.torque_limit;
    }

    // Merge input settings
    if overlay.settings.input.steering_range != base.settings.input.steering_range {
        result.settings.input.steering_range = overlay.settings.input.steering_range;
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
    fn test_merge_profiles() {
        let base = WheelProfile::new("Base", "device");
        let mut overlay = WheelProfile::new("Overlay", "device");
        overlay.settings.ffb.overall_gain = 0.5;

        let merged = merge_profiles(&base, &overlay);

        assert_eq!(merged.settings.ffb.overall_gain, 0.5);
    }
}
