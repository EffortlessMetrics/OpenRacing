//! Plugin SDK for developing racing wheel plugins

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin SDK version
pub const SDK_VERSION: &str = "1.0.0";

/// Telemetry data structure for plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkTelemetry {
    /// Force feedback scalar (-1.0 to 1.0)
    pub ffb_scalar: f32,
    /// Engine RPM
    pub rpm: f32,
    /// Vehicle speed (m/s)
    pub speed_ms: f32,
    /// Slip ratio (0.0 to 1.0)
    pub slip_ratio: f32,
    /// Current gear (-1 = reverse, 0 = neutral, 1+ = forward gears)
    pub gear: i8,
    /// Race flags
    pub flags: TelemetryFlags,
    /// Car identifier
    pub car_id: Option<String>,
    /// Track identifier
    pub track_id: Option<String>,
    /// Custom data from other plugins
    pub custom_data: HashMap<String, serde_json::Value>,
}

/// Race flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFlags {
    pub green_flag: bool,
    pub yellow_flag: bool,
    pub red_flag: bool,
    pub checkered_flag: bool,
    pub blue_flag: bool,
    pub white_flag: bool,
    pub pit_limiter: bool,
    pub drs_enabled: bool,
    pub ers_available: bool,
}

/// LED mapping input for plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkLedInput {
    /// Current telemetry
    pub telemetry: SdkTelemetry,
    /// Number of available LEDs
    pub led_count: u32,
    /// Current LED state
    pub current_leds: Vec<SdkLedColor>,
}

/// LED color representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkLedColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// DSP filter input for plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkDspInput {
    /// Input force feedback signal (-1.0 to 1.0)
    pub ffb_input: f32,
    /// Wheel angular velocity (rad/s)
    pub wheel_speed: f32,
    /// Wheel angle (radians)
    pub wheel_angle: f32,
    /// Sample rate (Hz)
    pub sample_rate: f32,
    /// Time delta since last sample (seconds)
    pub dt: f32,
}

/// Plugin execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkContext {
    /// Plugin execution budget in microseconds
    pub budget_us: u32,
    /// Update rate in Hz
    pub update_rate_hz: u32,
    /// Frame number
    pub frame_number: u64,
}

/// Plugin output types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SdkOutput {
    /// Telemetry processing output
    Telemetry {
        telemetry: SdkTelemetry,
        custom_data: serde_json::Value,
    },
    /// LED mapping output
    Led {
        led_pattern: Vec<SdkLedColor>,
        brightness: f32,
        duration_ms: u32,
    },
    /// DSP filter output
    Dsp {
        ffb_output: f32,
        filter_state: serde_json::Value,
    },
}

/// SDK error types
#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    #[error("Capability required: {0}")]
    CapabilityRequired(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Processing error: {0}")]
    ProcessingError(String),
}

/// SDK result type
pub type SdkResult<T> = Result<T, SdkError>;

/// WASM plugin trait
pub trait WasmPlugin {
    /// Initialize the plugin with configuration
    fn initialize(&mut self, config: serde_json::Value) -> SdkResult<()>;

    /// Process telemetry data
    fn process_telemetry(
        &mut self,
        input: SdkTelemetry,
        context: SdkContext,
    ) -> SdkResult<SdkOutput>;

    /// Process LED mapping
    fn process_led_mapping(
        &mut self,
        input: SdkLedInput,
        context: SdkContext,
    ) -> SdkResult<SdkOutput>;

    /// Shutdown the plugin
    fn shutdown(&mut self) -> SdkResult<()>;
}

/// Macro to export a WASM plugin (placeholder implementation)
#[macro_export]
macro_rules! export_wasm_plugin {
    ($plugin_type:ty) => {
        // This would contain the actual WASM export logic
        // For now, it's just a placeholder to make the sample compile
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sdk_version_is_semver() -> Result<(), semver::Error> {
        let _version = semver::Version::parse(SDK_VERSION)?;
        Ok(())
    }

    #[test]
    fn test_sdk_telemetry_serialization_roundtrip() -> Result<(), serde_json::Error> {
        let telemetry = SdkTelemetry {
            ffb_scalar: 0.75,
            rpm: 7500.0,
            speed_ms: 55.0,
            slip_ratio: 0.15,
            gear: 4,
            flags: TelemetryFlags {
                green_flag: true,
                yellow_flag: false,
                red_flag: false,
                checkered_flag: false,
                blue_flag: false,
                white_flag: false,
                pit_limiter: false,
                drs_enabled: true,
                ers_available: true,
            },
            car_id: Some("car_001".to_string()),
            track_id: Some("spa".to_string()),
            custom_data: HashMap::new(),
        };

        let json = serde_json::to_string(&telemetry)?;
        let deserialized: SdkTelemetry = serde_json::from_str(&json)?;

        assert_eq!(deserialized.ffb_scalar, telemetry.ffb_scalar);
        assert_eq!(deserialized.rpm, telemetry.rpm);
        assert_eq!(deserialized.gear, telemetry.gear);
        assert_eq!(deserialized.car_id, telemetry.car_id);
        assert_eq!(deserialized.track_id, telemetry.track_id);
        assert!(deserialized.flags.green_flag);
        assert!(deserialized.flags.drs_enabled);
        assert!(!deserialized.flags.yellow_flag);
        Ok(())
    }

    #[test]
    fn test_sdk_telemetry_with_custom_data() -> Result<(), serde_json::Error> {
        let mut custom = HashMap::new();
        custom.insert(
            "brake_temp".to_string(),
            serde_json::json!({"fl": 450.0, "fr": 460.0}),
        );

        let telemetry = SdkTelemetry {
            ffb_scalar: 0.0,
            rpm: 0.0,
            speed_ms: 0.0,
            slip_ratio: 0.0,
            gear: 0,
            flags: TelemetryFlags {
                green_flag: false,
                yellow_flag: false,
                red_flag: false,
                checkered_flag: false,
                blue_flag: false,
                white_flag: false,
                pit_limiter: false,
                drs_enabled: false,
                ers_available: false,
            },
            car_id: None,
            track_id: None,
            custom_data: custom,
        };

        let json = serde_json::to_string(&telemetry)?;
        let deserialized: SdkTelemetry = serde_json::from_str(&json)?;
        assert!(deserialized.custom_data.contains_key("brake_temp"));
        Ok(())
    }

    #[test]
    fn test_sdk_led_color_serialization() -> Result<(), serde_json::Error> {
        let color = SdkLedColor {
            r: 255,
            g: 128,
            b: 0,
        };
        let json = serde_json::to_string(&color)?;
        let deserialized: SdkLedColor = serde_json::from_str(&json)?;
        assert_eq!(deserialized.r, 255);
        assert_eq!(deserialized.g, 128);
        assert_eq!(deserialized.b, 0);
        Ok(())
    }

    #[test]
    fn test_sdk_dsp_input_serialization() -> Result<(), serde_json::Error> {
        let input = SdkDspInput {
            ffb_input: -0.5,
            wheel_speed: 3.0,
            wheel_angle: 1.57,
            sample_rate: 1000.0,
            dt: 0.001,
        };
        let json = serde_json::to_string(&input)?;
        let deserialized: SdkDspInput = serde_json::from_str(&json)?;
        assert_eq!(deserialized.ffb_input, -0.5);
        assert_eq!(deserialized.sample_rate, 1000.0);
        assert_eq!(deserialized.dt, 0.001);
        Ok(())
    }

    #[test]
    fn test_sdk_context_serialization() -> Result<(), serde_json::Error> {
        let context = SdkContext {
            budget_us: 200,
            update_rate_hz: 1000,
            frame_number: 42,
        };
        let json = serde_json::to_string(&context)?;
        let deserialized: SdkContext = serde_json::from_str(&json)?;
        assert_eq!(deserialized.budget_us, 200);
        assert_eq!(deserialized.update_rate_hz, 1000);
        assert_eq!(deserialized.frame_number, 42);
        Ok(())
    }

    #[test]
    fn test_sdk_output_telemetry_variant() -> Result<(), serde_json::Error> {
        let output = SdkOutput::Telemetry {
            telemetry: SdkTelemetry {
                ffb_scalar: 1.0,
                rpm: 8000.0,
                speed_ms: 60.0,
                slip_ratio: 0.0,
                gear: 5,
                flags: TelemetryFlags {
                    green_flag: true,
                    yellow_flag: false,
                    red_flag: false,
                    checkered_flag: false,
                    blue_flag: false,
                    white_flag: false,
                    pit_limiter: false,
                    drs_enabled: false,
                    ers_available: false,
                },
                car_id: None,
                track_id: None,
                custom_data: HashMap::new(),
            },
            custom_data: serde_json::json!({}),
        };
        let json = serde_json::to_string(&output)?;
        let deserialized: SdkOutput = serde_json::from_str(&json)?;
        match deserialized {
            SdkOutput::Telemetry { telemetry, .. } => {
                assert_eq!(telemetry.gear, 5);
            }
            _ => panic!("Expected Telemetry variant"),
        }
        Ok(())
    }

    #[test]
    fn test_sdk_output_led_variant() -> Result<(), serde_json::Error> {
        let output = SdkOutput::Led {
            led_pattern: vec![
                SdkLedColor { r: 255, g: 0, b: 0 },
                SdkLedColor { r: 0, g: 255, b: 0 },
            ],
            brightness: 0.8,
            duration_ms: 100,
        };
        let json = serde_json::to_string(&output)?;
        let deserialized: SdkOutput = serde_json::from_str(&json)?;
        match deserialized {
            SdkOutput::Led {
                led_pattern,
                brightness,
                ..
            } => {
                assert_eq!(led_pattern.len(), 2);
                assert_eq!(brightness, 0.8);
            }
            _ => panic!("Expected Led variant"),
        }
        Ok(())
    }

    #[test]
    fn test_sdk_output_dsp_variant() -> Result<(), serde_json::Error> {
        let output = SdkOutput::Dsp {
            ffb_output: -0.3,
            filter_state: serde_json::json!({"lowpass_z": 0.5}),
        };
        let json = serde_json::to_string(&output)?;
        let deserialized: SdkOutput = serde_json::from_str(&json)?;
        match deserialized {
            SdkOutput::Dsp {
                ffb_output,
                filter_state,
            } => {
                assert_eq!(ffb_output, -0.3);
                assert!(filter_state.get("lowpass_z").is_some());
            }
            _ => panic!("Expected Dsp variant"),
        }
        Ok(())
    }

    #[test]
    fn test_sdk_error_display() {
        let err = SdkError::CapabilityRequired("ProcessDsp".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("ProcessDsp"));

        let err = SdkError::InvalidInput("negative speed".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("negative speed"));

        let err = SdkError::ProcessingError("buffer overflow".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("buffer overflow"));
    }

    #[test]
    fn test_telemetry_gear_reverse_neutral_forward() -> Result<(), serde_json::Error> {
        for gear in [-1_i8, 0, 1, 6] {
            let telemetry = SdkTelemetry {
                ffb_scalar: 0.0,
                rpm: 0.0,
                speed_ms: 0.0,
                slip_ratio: 0.0,
                gear,
                flags: TelemetryFlags {
                    green_flag: false,
                    yellow_flag: false,
                    red_flag: false,
                    checkered_flag: false,
                    blue_flag: false,
                    white_flag: false,
                    pit_limiter: false,
                    drs_enabled: false,
                    ers_available: false,
                },
                car_id: None,
                track_id: None,
                custom_data: HashMap::new(),
            };
            let json = serde_json::to_string(&telemetry)?;
            let deser: SdkTelemetry = serde_json::from_str(&json)?;
            assert_eq!(deser.gear, gear);
        }
        Ok(())
    }
}
