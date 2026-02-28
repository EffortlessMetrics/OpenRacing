//! Prelude module for convenient imports

pub use crate::codec::{
    MessageCodec, MessageDecoder, MessageEncoder, MessageHeader, message_flags, message_types,
};
pub use crate::error::{IpcError, IpcResult};
pub use crate::handlers::{
    DeviceCapabilities, DeviceHandler, DeviceInfo, DeviceStatus, DiagnosticInfo, FaultRecord,
    FeatureNegotiationResult, FeatureNegotiator, HealthHandler, PerformanceMetrics, ProfileHandler,
    ProfileInfo, ProfileScope, SafetyHandler, TelemetryData,
};
pub use crate::server::{
    ClientInfo, HealthEvent, HealthEventType, IpcConfig, IpcServer, PeerInfo, ServerState,
    is_version_compatible,
};
pub use crate::transport::{Transport, TransportBuilder, TransportConfig, TransportType};
pub use crate::{DEFAULT_TCP_PORT, MIN_CLIENT_VERSION, PROTOCOL_VERSION};
