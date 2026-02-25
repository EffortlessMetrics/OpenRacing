//! Message encoding and decoding for IPC

use prost::Message;

use crate::error::{IpcError, IpcResult};

/// Message codec for encoding and decoding IPC messages
#[derive(Debug, Clone, Copy)]
pub struct MessageCodec {
    /// Maximum message size in bytes
    max_message_size: usize,
}

impl MessageCodec {
    /// Create a new codec with default settings
    pub fn new() -> Self {
        Self {
            max_message_size: 16 * 1024 * 1024, // 16 MB
        }
    }

    /// Create a codec with custom max message size
    pub fn with_max_size(max_message_size: usize) -> Self {
        Self { max_message_size }
    }

    /// Get the maximum message size
    pub fn max_message_size(&self) -> usize {
        self.max_message_size
    }

    /// Check if a message size is valid
    pub fn is_valid_size(&self, size: usize) -> bool {
        size > 0 && size <= self.max_message_size
    }
}

impl Default for MessageCodec {
    fn default() -> Self {
        Self::new()
    }
}

/// Message encoder trait
pub trait MessageEncoder {
    /// Encode a message to bytes
    fn encode<M: Message>(&self, message: &M) -> IpcResult<Vec<u8>>;

    /// Encode a message to a pre-allocated buffer
    fn encode_to_buffer<M: Message>(&self, message: &M, buffer: &mut Vec<u8>) -> IpcResult<()>;
}

/// Message decoder trait
pub trait MessageDecoder {
    /// Decode bytes to a message
    fn decode<M: Message + Default>(&self, bytes: &[u8]) -> IpcResult<M>;

    /// Get the encoded length of a message
    fn encoded_len<M: Message>(&self, message: &M) -> usize;
}

impl MessageEncoder for MessageCodec {
    fn encode<M: Message>(&self, message: &M) -> IpcResult<Vec<u8>> {
        let encoded_len = message.encoded_len();
        if !self.is_valid_size(encoded_len) {
            return Err(IpcError::EncodingFailed(format!(
                "Message size {} exceeds maximum {}",
                encoded_len, self.max_message_size
            )));
        }

        let mut buffer = Vec::with_capacity(encoded_len);
        message
            .encode(&mut buffer)
            .map_err(|e| IpcError::EncodingFailed(e.to_string()))?;

        Ok(buffer)
    }

    fn encode_to_buffer<M: Message>(&self, message: &M, buffer: &mut Vec<u8>) -> IpcResult<()> {
        let encoded_len = message.encoded_len();
        if !self.is_valid_size(encoded_len) {
            return Err(IpcError::EncodingFailed(format!(
                "Message size {} exceeds maximum {}",
                encoded_len, self.max_message_size
            )));
        }

        buffer.clear();
        buffer.reserve(encoded_len);
        message
            .encode(buffer)
            .map_err(|e| IpcError::EncodingFailed(e.to_string()))?;

        Ok(())
    }
}

impl MessageDecoder for MessageCodec {
    fn decode<M: Message + Default>(&self, bytes: &[u8]) -> IpcResult<M> {
        if !self.is_valid_size(bytes.len()) {
            return Err(IpcError::DecodingFailed(format!(
                "Message size {} exceeds maximum {}",
                bytes.len(),
                self.max_message_size
            )));
        }

        M::decode(bytes).map_err(|e| IpcError::DecodingFailed(e.to_string()))
    }

    fn encoded_len<M: Message>(&self, message: &M) -> usize {
        message.encoded_len()
    }
}

/// Wire message header
#[derive(Debug, Clone, Copy)]
pub struct MessageHeader {
    /// Message type identifier
    pub message_type: u16,
    /// Payload length
    pub payload_len: u32,
    /// Sequence number
    pub sequence: u32,
    /// Flags
    pub flags: u16,
}

impl MessageHeader {
    /// Header size in bytes
    pub const SIZE: usize = 12;

    /// Create a new message header
    pub fn new(message_type: u16, payload_len: u32, sequence: u32) -> Self {
        Self {
            message_type,
            payload_len,
            sequence,
            flags: 0,
        }
    }

    /// Encode the header to bytes
    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut buffer = [0u8; Self::SIZE];
        buffer[0..2].copy_from_slice(&self.message_type.to_le_bytes());
        buffer[2..6].copy_from_slice(&self.payload_len.to_le_bytes());
        buffer[6..10].copy_from_slice(&self.sequence.to_le_bytes());
        buffer[10..12].copy_from_slice(&self.flags.to_le_bytes());
        buffer
    }

    /// Decode a header from bytes
    pub fn decode(bytes: &[u8]) -> IpcResult<Self> {
        if bytes.len() < Self::SIZE {
            return Err(IpcError::DecodingFailed(
                "Insufficient bytes for message header".to_string(),
            ));
        }

        let message_type = u16::from_le_bytes([bytes[0], bytes[1]]);
        let payload_len = u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        let sequence = u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]);
        let flags = u16::from_le_bytes([bytes[10], bytes[11]]);

        Ok(Self {
            message_type,
            payload_len,
            sequence,
            flags,
        })
    }

    /// Set a flag
    pub fn set_flag(&mut self, flag: u16) {
        self.flags |= flag;
    }

    /// Check if a flag is set
    pub fn has_flag(&self, flag: u16) -> bool {
        (self.flags & flag) != 0
    }
}

/// Message type identifiers
pub mod message_types {
    /// Device management message
    pub const DEVICE: u16 = 0x0001;
    /// Profile management message
    pub const PROFILE: u16 = 0x0002;
    /// Safety control message
    pub const SAFETY: u16 = 0x0003;
    /// Health event message
    pub const HEALTH: u16 = 0x0004;
    /// Feature negotiation message
    pub const FEATURE_NEGOTIATION: u16 = 0x0005;
    /// Game integration message
    pub const GAME: u16 = 0x0006;
    /// Telemetry message
    pub const TELEMETRY: u16 = 0x0007;
    /// Diagnostic message
    pub const DIAGNOSTIC: u16 = 0x0008;
}

/// Message flags
pub mod message_flags {
    /// Compressed message
    pub const COMPRESSED: u16 = 0x0001;
    /// Requires acknowledgment
    pub const REQUIRES_ACK: u16 = 0x0002;
    /// Is a response
    pub const IS_RESPONSE: u16 = 0x0004;
    /// Error response
    pub const IS_ERROR: u16 = 0x0008;
    /// Streaming message
    pub const STREAMING: u16 = 0x0010;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_codec_default() {
        let codec = MessageCodec::default();
        assert!(codec.is_valid_size(1024));
        assert!(!codec.is_valid_size(0));
        assert!(!codec.is_valid_size(100 * 1024 * 1024));
    }

    #[test]
    fn test_message_header_encode_decode() -> IpcResult<()> {
        let header = MessageHeader::new(message_types::DEVICE, 1024, 42);
        let encoded = header.encode();
        let decoded = MessageHeader::decode(&encoded)?;

        assert_eq!(decoded.message_type, message_types::DEVICE);
        assert_eq!(decoded.payload_len, 1024);
        assert_eq!(decoded.sequence, 42);

        Ok(())
    }

    #[test]
    fn test_message_header_flags() {
        let mut header = MessageHeader::new(message_types::DEVICE, 100, 1);
        assert!(!header.has_flag(message_flags::COMPRESSED));

        header.set_flag(message_flags::COMPRESSED);
        assert!(header.has_flag(message_flags::COMPRESSED));

        header.set_flag(message_flags::REQUIRES_ACK);
        assert!(header.has_flag(message_flags::COMPRESSED));
        assert!(header.has_flag(message_flags::REQUIRES_ACK));
    }

    #[test]
    fn test_message_header_decode_insufficient_bytes() {
        let bytes = [0u8; 8];
        let result = MessageHeader::decode(&bytes);
        assert!(result.is_err());
    }
}
