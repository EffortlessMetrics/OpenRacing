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

#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![warn(missing_docs, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod codec;
pub mod error;
pub mod handlers;
pub mod prelude;
pub mod server;
pub mod transport;

pub use codec::{MessageCodec, MessageDecoder, MessageEncoder};
pub use error::{IpcError, IpcResult};
pub use server::{IpcConfig, IpcServer};
pub use transport::{Transport, TransportType};

/// Current wire protocol version
pub const PROTOCOL_VERSION: &str = "1.0.0";

/// Minimum supported client version
pub const MIN_CLIENT_VERSION: &str = "1.0.0";

/// Default IPC port for TCP transport
pub const DEFAULT_TCP_PORT: u16 = 50051;
