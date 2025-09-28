
// Protobuf stub file - generated when protoc is not available
// This file provides minimal type definitions to allow compilation

pub mod wheel {
    pub mod v1 {
        use serde::{Deserialize, Serialize};
        
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct DeviceInfo {
            pub id: String,
            pub name: String,
            pub device_type: i32,
            pub capabilities: Option<DeviceCapabilities>,
            pub state: i32,
        }
        
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct DeviceCapabilities {
            pub supports_pid: bool,
            pub supports_raw_torque_1khz: bool,
            pub supports_health_stream: bool,
            pub supports_led_bus: bool,
            pub max_torque_cnm: u32,
            pub encoder_cpr: u32,
            pub min_report_period_us: u32,
        }
        
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct Profile {
            pub schema_version: String,
            pub scope: Option<ProfileScope>,
            pub base: Option<BaseSettings>,
            pub leds: Option<LedConfig>,
            pub haptics: Option<HapticsConfig>,
            pub signature: String,
        }
        
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct ProfileScope {
            pub game: String,
            pub car: String,
            pub track: String,
        }
        
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct BaseSettings {
            pub ffb_gain: f32,
            pub dor_deg: u32,
            pub torque_cap_nm: f32,
            pub filters: Option<FilterConfig>,
        }
        
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct FilterConfig {
            pub reconstruction: u32,
            pub friction: f32,
            pub damper: f32,
            pub inertia: f32,
            pub notch_filters: Vec<NotchFilter>,
            pub slew_rate: f32,
            pub curve_points: Vec<CurvePoint>,
        }
        
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct NotchFilter {
            pub hz: f32,
            pub q: f32,
            pub gain_db: f32,
        }
        
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct CurvePoint {
            pub input: f32,
            pub output: f32,
        }
        
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct LedConfig {
            pub rpm_bands: Vec<f32>,
            pub pattern: String,
            pub brightness: f32,
        }
        
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct HapticsConfig {
            pub enabled: bool,
            pub intensity: f32,
            pub frequency_hz: f32,
        }
        
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct OpResult {
            pub success: bool,
            pub error_message: String,
        }
        
        // Stub service trait for when tonic is not available
        #[async_trait::async_trait]
        pub trait WheelService {
            // Service methods would be defined here in the real implementation
        }
    }
}
