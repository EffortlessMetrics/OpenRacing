//! Racing Wheel Software - Schema Definitions
//!
//! This crate contains all the schema definitions for IPC communication,
//! configuration files, and data interchange formats.

pub mod wheel {
    //! Generated protobuf types for wheel service
    tonic::include_proto!("wheel.v1");
}

pub mod config {
    //! Configuration schema types
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ProfileSchema {
        pub schema: String,
        pub scope: ProfileScope,
        pub base: BaseSettings,
        pub leds: Option<LedConfig>,
        pub haptics: Option<HapticsConfig>,
        pub signature: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ProfileScope {
        pub game: Option<String>,
        pub car: Option<String>,
        pub track: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BaseSettings {
        pub ffb_gain: f32,
        pub dor_deg: u16,
        pub torque_cap_nm: f32,
        pub filters: FilterConfig,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FilterConfig {
        pub reconstruction: u8,
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
}