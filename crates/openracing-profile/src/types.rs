//! Profile type definitions

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WheelProfile {
    pub id: String,
    pub name: String,
    pub device_id: String,
    /// User-facing revision counter (incremented on each save).
    pub version: u32,
    /// Schema format version for migration tracking.
    /// Old profiles without this field deserialize as 0.
    #[serde(default)]
    pub schema_version: u32,
    pub settings: WheelSettings,
    pub created_at: u64,
    pub modified_at: u64,
}

impl WheelProfile {
    pub fn new(name: impl Into<String>, device_id: impl Into<String>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            id: crate::generate_profile_id(),
            name: name.into(),
            device_id: device_id.into(),
            version: 1,
            schema_version: crate::CURRENT_SCHEMA_VERSION,
            settings: WheelSettings::default(),
            created_at: now,
            modified_at: now,
        }
    }

    pub fn with_settings(mut self, settings: WheelSettings) -> Self {
        self.settings = settings;
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WheelSettings {
    pub ffb: FfbSettings,
    pub input: InputSettings,
    pub limits: LimitSettings,
    pub advanced: AdvancedSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfbSettings {
    pub overall_gain: f32,
    pub torque_limit: f32,
    pub spring_strength: f32,
    pub damper_strength: f32,
    pub friction_strength: f32,
    pub effects_enabled: bool,
}

impl Default for FfbSettings {
    fn default() -> Self {
        Self {
            overall_gain: 1.0,
            torque_limit: 25.0,
            spring_strength: 0.0,
            damper_strength: 0.0,
            friction_strength: 0.0,
            effects_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSettings {
    pub steering_range: u16,
    pub steering_deadzone: u16,
    pub throttle_curve: CurveType,
    pub brake_curve: CurveType,
    pub clutch_curve: CurveType,
}

impl Default for InputSettings {
    fn default() -> Self {
        Self {
            steering_range: 900,
            steering_deadzone: 0,
            throttle_curve: CurveType::Linear,
            brake_curve: CurveType::Linear,
            clutch_curve: CurveType::Linear,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CurveType {
    #[default]
    Linear,
    Exponential,
    Logarithmic,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitSettings {
    pub max_speed: Option<f32>,
    pub max_temp: Option<u8>,
    pub emergency_stop: bool,
}

impl Default for LimitSettings {
    fn default() -> Self {
        Self {
            max_speed: None,
            max_temp: Some(80),
            emergency_stop: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedSettings {
    pub filter_enabled: bool,
    pub filter_strength: f32,
    pub led_mode: LedMode,
    pub telemetry_enabled: bool,
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            filter_enabled: false,
            filter_strength: 0.5,
            led_mode: LedMode::Default,
            telemetry_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LedMode {
    #[default]
    Default,
    Speed,
    Rpm,
    Custom,
    Off,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wheel_profile_creation() {
        let profile = WheelProfile::new("Test Profile", "device-1");

        assert!(!profile.id.is_empty());
        assert_eq!(profile.name, "Test Profile");
        assert_eq!(profile.device_id, "device-1");
        assert_eq!(profile.version, 1);
    }

    #[test]
    fn test_wheel_profile_with_settings() {
        let settings = WheelSettings::default();
        let profile = WheelProfile::new("Test", "device").with_settings(settings);

        assert_eq!(profile.name, "Test");
    }

    #[test]
    fn test_default_settings() {
        let settings = WheelSettings::default();

        assert_eq!(settings.ffb.overall_gain, 1.0);
        assert_eq!(settings.input.steering_range, 900);
    }
}
