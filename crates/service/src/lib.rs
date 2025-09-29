//! Racing Wheel Service - Main service daemon

pub mod service;
pub mod device;
pub mod ipc_simple;
// pub mod ipc;
// pub mod ipc_client;
pub mod profile_service;
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
#[cfg(test)]
pub mod game_integration_tests;
pub mod game_integration;
pub mod game_integration_service;
#[cfg(test)]
pub mod game_integration_e2e_tests;
// #[cfg(test)]
// pub mod ipc_tests;

pub use service::*;
pub use device::*;
pub use ipc_simple::{IpcServer, IpcConfig, TransportType, HealthEventInternal, IpcClient, IpcClientConfig};
// pub use ipc::{IpcServer, IpcConfig, TransportType, HealthEventInternal};
// pub use ipc_client::{IpcClient, IpcClientConfig};
pub use profile_service::*;
pub use device_service::*;
pub use safety_service::*;
pub use game_service::GameService;
pub use telemetry::*;
pub use process_detection::*;
pub use auto_profile_switching::*;
pub use config_validation::*;
pub use game_integration_service::*;