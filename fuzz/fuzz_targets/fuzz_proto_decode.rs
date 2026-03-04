//! Fuzzes protobuf message decoding for all critical IPC message types.
//!
//! Feeds arbitrary bytes into `prost::Message::decode` for every wire-facing
//! protobuf message type defined in `racing-wheel-schemas`. This catches
//! panics in the generated decode logic when fed truncated, corrupted, or
//! adversarial protobuf payloads.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_proto_decode

#![no_main]

use libfuzzer_sys::fuzz_target;
use prost::Message;
use racing_wheel_schemas::generated::wheel::v1::{
    ApplyProfileRequest, ConfigureTelemetryRequest, DeviceInfo, DiagnosticInfo,
    FeatureNegotiationRequest, FeatureNegotiationResponse, GameStatus, HealthEvent, Profile,
};

fuzz_target!(|data: &[u8]| {
    // Every decode must handle arbitrary bytes without panicking.
    let _ = DeviceInfo::decode(data);
    let _ = Profile::decode(data);
    let _ = ApplyProfileRequest::decode(data);
    let _ = HealthEvent::decode(data);
    let _ = DiagnosticInfo::decode(data);
    let _ = ConfigureTelemetryRequest::decode(data);
    let _ = GameStatus::decode(data);
    let _ = FeatureNegotiationRequest::decode(data);
    let _ = FeatureNegotiationResponse::decode(data);
});
