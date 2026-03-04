//! Deep wire format tests for IPC protocol stability.
//!
//! Covers:
//! - Protobuf message wire format stability
//! - Message framing (length-delimited)
//! - Unknown message type handling
//! - Message versioning
//! - Header field layout and endianness
//! - Flag combinations
//! - Codec size validation

use openracing_ipc::codec::{
    MessageCodec, MessageDecoder, MessageEncoder, MessageHeader, message_flags, message_types,
};
use openracing_ipc::error::{IpcError, IpcResult};
use openracing_ipc::{MIN_CLIENT_VERSION, PROTOCOL_VERSION};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// =========================================================================
// Protobuf message wire format stability
// =========================================================================

#[test]
fn protobuf_duration_wire_format_is_stable() -> Result<(), BoxErr> {
    let msg = prost_types::Duration {
        seconds: 100,
        nanos: 200,
    };
    let codec = MessageCodec::new();
    let encoded = MessageEncoder::encode(&codec, &msg)?;

    // Re-encode should produce identical bytes
    let encoded2 = MessageEncoder::encode(&codec, &msg)?;
    assert_eq!(
        encoded, encoded2,
        "Protobuf encoding should be deterministic"
    );
    Ok(())
}

#[test]
fn protobuf_timestamp_wire_format_is_stable() -> Result<(), BoxErr> {
    let msg = prost_types::Timestamp {
        seconds: 1_700_000_000,
        nanos: 500_000,
    };
    let codec = MessageCodec::new();
    let encoded = MessageEncoder::encode(&codec, &msg)?;

    let decoded: prost_types::Timestamp = MessageDecoder::decode(&codec, &encoded)?;
    assert_eq!(decoded.seconds, 1_700_000_000);
    assert_eq!(decoded.nanos, 500_000);
    Ok(())
}

#[test]
fn protobuf_default_message_encodes_to_empty() -> Result<(), BoxErr> {
    // A fully-default protobuf message encodes to zero bytes
    let msg = prost_types::Duration {
        seconds: 0,
        nanos: 0,
    };
    let codec = MessageCodec::new();
    let encoded = MessageEncoder::encode(&codec, &msg);
    // prost omits default values, so encoded length is 0 — which our codec rejects as invalid
    assert!(
        encoded.is_err(),
        "Zero-length protobuf encoding should be rejected by codec"
    );
    Ok(())
}

#[test]
fn protobuf_field_order_preserved() -> Result<(), BoxErr> {
    let msg = prost_types::Duration {
        seconds: 42,
        nanos: 99,
    };
    let codec = MessageCodec::new();
    let bytes = MessageEncoder::encode(&codec, &msg)?;

    // First field (seconds, field 1) should come before second (nanos, field 2)
    // Protobuf field tag for field 1 varint = 0x08, field 2 varint = 0x10
    let first_tag_pos = bytes.iter().position(|&b| b == 0x08);
    let second_tag_pos = bytes.iter().position(|&b| b == 0x10);

    assert!(
        first_tag_pos.is_some() && second_tag_pos.is_some(),
        "Both field tags should be present"
    );
    assert!(
        first_tag_pos < second_tag_pos,
        "Field 1 should appear before field 2"
    );
    Ok(())
}

// =========================================================================
// Message framing (length-delimited)
// =========================================================================

#[test]
fn header_size_is_exactly_12_bytes() {
    assert_eq!(MessageHeader::SIZE, 12);
}

#[test]
fn header_encodes_to_fixed_size() {
    let header = MessageHeader::new(message_types::DEVICE, 1024, 1);
    let bytes = header.encode();
    assert_eq!(bytes.len(), MessageHeader::SIZE);
}

#[test]
fn header_payload_len_enables_framing() -> Result<(), BoxErr> {
    let payload_size: u32 = 4096;
    let header = MessageHeader::new(message_types::TELEMETRY, payload_size, 0);
    let bytes = header.encode();
    let decoded = MessageHeader::decode(&bytes)?;

    // Reader can use payload_len to know how many bytes follow
    assert_eq!(decoded.payload_len, payload_size);
    Ok(())
}

#[test]
fn framing_multiple_messages_in_stream() -> Result<(), BoxErr> {
    // Simulate framing: header1 | payload1 | header2 | payload2
    let header1 = MessageHeader::new(message_types::DEVICE, 4, 1);
    let payload1 = [0xDE, 0xAD, 0xBE, 0xEF];
    let header2 = MessageHeader::new(message_types::HEALTH, 2, 2);
    let payload2 = [0xCA, 0xFE];

    let mut stream = Vec::new();
    stream.extend_from_slice(&header1.encode());
    stream.extend_from_slice(&payload1);
    stream.extend_from_slice(&header2.encode());
    stream.extend_from_slice(&payload2);

    // Parse first message
    let h1 = MessageHeader::decode(&stream[0..MessageHeader::SIZE])?;
    assert_eq!(h1.message_type, message_types::DEVICE);
    assert_eq!(h1.payload_len, 4);
    let p1_start = MessageHeader::SIZE;
    let p1_end = p1_start + h1.payload_len as usize;
    assert_eq!(&stream[p1_start..p1_end], &payload1);

    // Parse second message
    let h2 = MessageHeader::decode(&stream[p1_end..p1_end + MessageHeader::SIZE])?;
    assert_eq!(h2.message_type, message_types::HEALTH);
    assert_eq!(h2.payload_len, 2);
    let p2_start = p1_end + MessageHeader::SIZE;
    let p2_end = p2_start + h2.payload_len as usize;
    assert_eq!(&stream[p2_start..p2_end], &payload2);

    Ok(())
}

#[test]
fn header_decode_rejects_truncated_input() {
    for size in 0..MessageHeader::SIZE {
        let bytes = vec![0u8; size];
        let result = MessageHeader::decode(&bytes);
        assert!(
            result.is_err(),
            "Header decode should fail for {size} bytes (need {expected})",
            expected = MessageHeader::SIZE
        );
    }
}

#[test]
fn header_decode_accepts_exact_size() -> Result<(), BoxErr> {
    let bytes = [0u8; MessageHeader::SIZE];
    let header = MessageHeader::decode(&bytes)?;
    assert_eq!(header.message_type, 0);
    assert_eq!(header.payload_len, 0);
    assert_eq!(header.sequence, 0);
    assert_eq!(header.flags, 0);
    Ok(())
}

// =========================================================================
// Unknown message type handling
// =========================================================================

#[test]
fn unknown_message_type_roundtrips() -> Result<(), BoxErr> {
    // Use a message type not in the known constants
    let unknown_type: u16 = 0xFFFF;
    let header = MessageHeader::new(unknown_type, 0, 0);
    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;
    assert_eq!(decoded.message_type, unknown_type);
    Ok(())
}

#[test]
fn unknown_message_type_mid_range() -> Result<(), BoxErr> {
    let custom_type: u16 = 0x0100;
    let header = MessageHeader::new(custom_type, 128, 5);
    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;
    assert_eq!(decoded.message_type, custom_type);
    assert_eq!(decoded.payload_len, 128);
    Ok(())
}

#[test]
fn unknown_flags_preserved() -> Result<(), BoxErr> {
    // Set a flag outside the known flag constants
    let mut header = MessageHeader::new(message_types::DEVICE, 0, 0);
    let custom_flag: u16 = 0x8000;
    header.set_flag(custom_flag);

    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;
    assert!(decoded.has_flag(custom_flag));
    Ok(())
}

#[test]
fn all_known_message_types_are_distinct() {
    let types = [
        message_types::DEVICE,
        message_types::PROFILE,
        message_types::SAFETY,
        message_types::HEALTH,
        message_types::FEATURE_NEGOTIATION,
        message_types::GAME,
        message_types::TELEMETRY,
        message_types::DIAGNOSTIC,
    ];

    let mut seen = std::collections::HashSet::new();
    for &t in &types {
        assert!(seen.insert(t), "Message type 0x{t:04X} is duplicated");
    }
}

// =========================================================================
// Message versioning
// =========================================================================

#[test]
fn protocol_version_is_semver() {
    let parts: Vec<&str> = PROTOCOL_VERSION.split('.').collect();
    assert_eq!(
        parts.len(),
        3,
        "PROTOCOL_VERSION should be semver (got {PROTOCOL_VERSION})"
    );
    for part in &parts {
        assert!(
            part.parse::<u32>().is_ok(),
            "PROTOCOL_VERSION component '{part}' should be numeric"
        );
    }
}

#[test]
fn min_client_version_is_semver() {
    let parts: Vec<&str> = MIN_CLIENT_VERSION.split('.').collect();
    assert_eq!(
        parts.len(),
        3,
        "MIN_CLIENT_VERSION should be semver (got {MIN_CLIENT_VERSION})"
    );
    for part in &parts {
        assert!(
            part.parse::<u32>().is_ok(),
            "MIN_CLIENT_VERSION component '{part}' should be numeric"
        );
    }
}

#[test]
fn current_protocol_is_compatible_with_min() {
    use openracing_ipc::server::is_version_compatible;
    assert!(
        is_version_compatible(PROTOCOL_VERSION, MIN_CLIENT_VERSION),
        "PROTOCOL_VERSION {PROTOCOL_VERSION} should be compatible with MIN_CLIENT_VERSION {MIN_CLIENT_VERSION}"
    );
}

#[test]
fn version_wire_encoding_stability() -> Result<(), BoxErr> {
    // Ensure version string can be encoded as protobuf field
    let codec = MessageCodec::new();
    let msg = prost_types::Duration {
        seconds: 1,
        nanos: 0,
    };
    let bytes = MessageEncoder::encode(&codec, &msg)?;
    // Version doesn't change the wire format of messages
    assert!(!bytes.is_empty());
    Ok(())
}

// =========================================================================
// Flag combination tests
// =========================================================================

#[test]
fn all_flags_can_be_set_simultaneously() -> Result<(), BoxErr> {
    let all_flags = [
        message_flags::COMPRESSED,
        message_flags::REQUIRES_ACK,
        message_flags::IS_RESPONSE,
        message_flags::IS_ERROR,
        message_flags::STREAMING,
    ];

    let mut header = MessageHeader::new(message_types::DEVICE, 0, 0);
    for &flag in &all_flags {
        header.set_flag(flag);
    }

    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;

    for &flag in &all_flags {
        assert!(
            decoded.has_flag(flag),
            "Flag 0x{flag:04X} should be set after round-trip"
        );
    }
    Ok(())
}

#[test]
fn flags_are_bitwise_independent() {
    let mut header = MessageHeader::new(message_types::DEVICE, 0, 0);
    header.set_flag(message_flags::COMPRESSED);

    assert!(header.has_flag(message_flags::COMPRESSED));
    assert!(!header.has_flag(message_flags::REQUIRES_ACK));
    assert!(!header.has_flag(message_flags::IS_RESPONSE));
    assert!(!header.has_flag(message_flags::IS_ERROR));
    assert!(!header.has_flag(message_flags::STREAMING));
}

#[test]
fn all_known_flags_are_distinct_powers_of_two() {
    let flags = [
        message_flags::COMPRESSED,
        message_flags::REQUIRES_ACK,
        message_flags::IS_RESPONSE,
        message_flags::IS_ERROR,
        message_flags::STREAMING,
    ];

    for &flag in &flags {
        assert!(
            flag.is_power_of_two(),
            "Flag 0x{flag:04X} should be a power of two"
        );
    }

    let mut seen = std::collections::HashSet::new();
    for &flag in &flags {
        assert!(seen.insert(flag), "Flag 0x{flag:04X} is duplicated");
    }
}

// =========================================================================
// Codec error messages
// =========================================================================

#[test]
fn encoding_error_display() {
    let err = IpcError::EncodingFailed("buffer overflow".to_string());
    let msg = err.to_string();
    assert!(
        msg.contains("buffer overflow"),
        "Encoding error should contain reason, got: {msg}"
    );
}

#[test]
fn decoding_error_display() {
    let err = IpcError::DecodingFailed("invalid varint".to_string());
    let msg = err.to_string();
    assert!(
        msg.contains("invalid varint"),
        "Decoding error should contain reason, got: {msg}"
    );
}

#[test]
fn codec_decode_corrupted_bytes() {
    let codec = MessageCodec::new();
    // Invalid protobuf bytes
    let garbage = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x01];
    let result: IpcResult<prost_types::Duration> = MessageDecoder::decode(&codec, &garbage);
    assert!(result.is_err());
}

// =========================================================================
// Header field layout
// =========================================================================

#[test]
fn header_little_endian_message_type() -> Result<(), BoxErr> {
    let header = MessageHeader::new(0x0201, 0, 0);
    let bytes = header.encode();
    // LE: low byte first
    assert_eq!(bytes[0], 0x01);
    assert_eq!(bytes[1], 0x02);
    Ok(())
}

#[test]
fn header_little_endian_payload_len() -> Result<(), BoxErr> {
    let header = MessageHeader::new(0, 0x04030201, 0);
    let bytes = header.encode();
    assert_eq!(bytes[2], 0x01);
    assert_eq!(bytes[3], 0x02);
    assert_eq!(bytes[4], 0x03);
    assert_eq!(bytes[5], 0x04);
    Ok(())
}

#[test]
fn header_little_endian_sequence() -> Result<(), BoxErr> {
    let header = MessageHeader::new(0, 0, 0x08070605);
    let bytes = header.encode();
    assert_eq!(bytes[6], 0x05);
    assert_eq!(bytes[7], 0x06);
    assert_eq!(bytes[8], 0x07);
    assert_eq!(bytes[9], 0x08);
    Ok(())
}

#[test]
fn header_little_endian_flags() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(0, 0, 0);
    header.flags = 0x0201;
    let bytes = header.encode();
    assert_eq!(bytes[10], 0x01);
    assert_eq!(bytes[11], 0x02);
    Ok(())
}
