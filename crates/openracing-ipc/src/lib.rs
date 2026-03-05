//! IPC server and transport for OpenRacing
//!
//! This crate provides the IPC (Inter-Process Communication) infrastructure for
//! OpenRacing, supporting multiple transport types and wire protocol stability.
//!
//! # Architecture
//!
//! The crate is organized into several modules:
//!
//! - [`server`]: IPC server with platform-specific transports
//! - [`transport`]: Transport abstraction (TCP, Unix socket, Named Pipe)
//! - [`codec`]: Message encoding and decoding
//! - [`handlers`]: gRPC service handler traits
//! - [`error`]: IPC-specific error types
//!
//! # IPC Config Creation
//!
//! ```
//! use openracing_ipc::prelude::*;
//!
//! // Default configuration uses platform-native transport
//! let config = IpcConfig::default();
//! assert_eq!(config.server_name, "openracing-ipc");
//!
//! // Customize with builder methods
//! let config = IpcConfig::with_transport(TransportType::tcp())
//!     .max_connections(50)
//!     .health_buffer_size(500);
//!
//! assert_eq!(config.transport.max_connections, 50);
//! assert_eq!(config.health_buffer_size, 500);
//! ```
//!
//! # Message Construction
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use openracing_ipc::prelude::*;
//!
//! // Create a wire message header
//! let header = MessageHeader::new(message_types::DEVICE, 256, 1);
//! let bytes = header.encode();
//! assert_eq!(bytes.len(), MessageHeader::SIZE);
//!
//! // Round-trip encode/decode
//! let decoded = MessageHeader::decode(&bytes)?;
//! assert_eq!(decoded.message_type, message_types::DEVICE);
//! assert_eq!(decoded.payload_len, 256);
//! assert_eq!(decoded.sequence, 1);
//! # Ok(())
//! # }
//! ```
//!
//! # Transport Setup
//!
//! ```
//! use openracing_ipc::prelude::*;
//! use std::time::Duration;
//!
//! // Build a transport configuration
//! let config = TransportBuilder::new()
//!     .transport(TransportType::tcp())
//!     .max_connections(50)
//!     .connection_timeout(Duration::from_secs(10))
//!     .enable_acl(true)
//!     .build();
//!
//! assert_eq!(config.max_connections, 50);
//! assert!(config.enable_acl);
//! ```
//!
//! # Wire Protocol Stability
//!
//! The wire protocol is designed for backward compatibility:
//! - Uses Protocol Buffers (prost) for serialization
//! - Supports feature negotiation between client and server
//! - Version compatibility checking at connection time
//!
//! # Platform Support
//!
//! - **Windows**: Named Pipes (primary), TCP (fallback)
//! - **Linux/macOS**: Unix Domain Sockets (primary), TCP (fallback)
//!
//! # Example
//!
//! ```no_run
//! use openracing_ipc::prelude::*;
//! use std::sync::Arc;
//!
//! async fn run_server() -> Result<(), IpcError> {
//!     let config = IpcConfig::default();
//!     let server = IpcServer::new(config);
//!     server.start().await
//! }
//! ```

#![deny(static_mut_refs)]
#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![warn(missing_docs, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod codec;
pub mod error;
pub mod handlers;
pub mod prelude;
pub mod server;
pub mod transport;
pub mod version;

pub use codec::{MessageCodec, MessageDecoder, MessageEncoder};
pub use error::{IpcError, IpcResult};
pub use server::{IpcConfig, IpcServer};
pub use transport::{Transport, TransportType};
pub use version::{
    FeatureFlags, NegotiationResult, ProtocolVersion, VersionInfo, VersionNegotiator,
};

/// Current wire protocol version
pub const PROTOCOL_VERSION: &str = "1.0.0";

/// Minimum supported client version
pub const MIN_CLIENT_VERSION: &str = "1.0.0";

/// Default IPC port for TCP transport
pub const DEFAULT_TCP_PORT: u16 = 50051;
