//! Racing Wheel Service - Complete system integration with graceful degradation

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]

pub mod service;
pub mod device;
pub mod daemon;
mod daemon_platform;
pub mod ipc_simple;
pub mod ipc_service;
pub mod profile_service;
pub mod profile_repository;
pub mod device_service;
pub mod safety_service;
pub mod service_tests;
pub mod game_service;
pub mod game_support_matrix;
pub mod config_writers;
pub mod telemetry;
pub mod process_detection;
pub mod auto_profile_switching;
pub mod config_validation;
pub mod observability;
pub mod system_config;
pub mod anticheat;
pub mod diagnostic_service;
#[cfg(test)]
pub mod integration_tests;
#[cfg(test)]
pub mod game_integration_tests;
pub mod game_integration;
pub mod game_integration_service;
#[cfg(test)]
pub mod game_integration_e2e_tests;
#[cfg(test)]
pub mod profile_repository_test;
#[cfg(test)]
pub mod daemon_tests;

pub use service::*;
pub use device::*;
pub use daemon::{ServiceDaemon, ServiceConfig};
pub use ipc_simple::{IpcServer, IpcConfig, TransportType, HealthEventInternal, IpcClient, IpcClientConfig};
pub use ipc_service::{WheelServiceImpl};
pub use profile_service::*;
pub use device_service::*;
pub use safety_service::*;
pub use system_config::{SystemConfig, FeatureFlags};
pub use anticheat::AntiCheatReport;
pub use diagnostic_service::{DiagnosticService, DiagnosticResult, DiagnosticStatus};

// Type aliases for application services to match expected naming
pub type ApplicationProfileService = profile_service::ProfileService;
pub type ApplicationDeviceService = device_service::ApplicationDeviceService;
pub type ApplicationSafetyService = safety_service::ApplicationSafetyService;
pub type ApplicationGameService = game_service::GameService;
pub use game_service::GameService;
pub use telemetry::*;
pub use process_detection::*;
pub use auto_profile_switching::*;
pub use config_validation::*;
pub use game_integration_service::*;
pub use observability::*;