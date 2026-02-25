//! File format definitions for .wbb format
//!
//! Defines the binary file format structure and constants for blackbox recordings.
//!
//! # File Format Stability
//!
//! The .wbb v1 format is designed for long-term stability:
//! - Magic number: "WBB1" (4 bytes)
//! - Version field allows future format extensions
//! - Header size field enables backward-compatible extensions
//! - Footer contains index for random access
//!
//! ## Versioning
//!
//! - Version 1: Initial format with Stream A/B/C support
//! - Future versions will maintain backward compatibility with v1 readers

use serde::{Deserialize, Serialize};

/// .wbb v1 file format magic number
pub const WBB_MAGIC: &[u8; 4] = b"WBB1";

/// .wbb v1 footer magic number (WBB1 reversed)
pub const WBB_FOOTER_MAGIC: &[u8; 4] = b"1BBW";

/// Current file format version
pub const WBB_VERSION: u32 = 1;

/// Default timebase: 1ms per tick for 1kHz recording
pub const DEFAULT_TIMEBASE_NS: u64 = 1_000_000;

/// Index interval: create index entry every 100ms
pub const INDEX_INTERVAL_MS: u32 = 100;

/// Maximum supported file format version
pub const MAX_SUPPORTED_VERSION: u32 = 1;

/// Stream A identifier (1kHz frames)
pub const STREAM_A_ID: u8 = 0x01;

/// Stream B identifier (60Hz telemetry)
pub const STREAM_B_ID: u8 = 0x02;

/// Stream C identifier (health events)
pub const STREAM_C_ID: u8 = 0x04;

/// .wbb v1 file header
///
/// This header appears at the start of every .wbb file and provides
/// metadata about the recording.
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbbHeader {
    /// Magic number "WBB1"
    pub magic: [u8; 4],
    /// File format version (1)
    pub version: u32,
    /// Device UUID
    pub device_id: String,
    /// Engine version string
    pub engine_version: String,
    /// Recording start time (Unix timestamp)
    pub start_time_unix: u64,
    /// Timebase for timestamps (nanoseconds per tick)
    pub timebase_ns: u64,
    /// FFB mode during recording
    pub ffb_mode: u8,
    /// Stream flags (bit 0: A, bit 1: B, bit 2: C)
    pub stream_flags: u8,
    /// Compression level used (0-9)
    pub compression_level: u8,
    /// Reserved for future use
    pub reserved: [u8; 15],
    /// Header size (for extensibility)
    pub header_size: u32,
}

impl WbbHeader {
    /// Create a new header with the specified parameters
    pub fn new(
        device_id: impl Into<String>,
        ffb_mode: u8,
        stream_flags: u8,
        compression_level: u8,
    ) -> Self {
        Self {
            magic: *WBB_MAGIC,
            version: WBB_VERSION,
            device_id: device_id.into(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            start_time_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            timebase_ns: DEFAULT_TIMEBASE_NS,
            ffb_mode,
            stream_flags,
            compression_level,
            reserved: [0; 15],
            header_size: 0,
        }
    }

    /// Check if Stream A is enabled
    pub fn has_stream_a(&self) -> bool {
        (self.stream_flags & STREAM_A_ID) != 0
    }

    /// Check if Stream B is enabled
    pub fn has_stream_b(&self) -> bool {
        (self.stream_flags & STREAM_B_ID) != 0
    }

    /// Check if Stream C is enabled
    pub fn has_stream_c(&self) -> bool {
        (self.stream_flags & STREAM_C_ID) != 0
    }

    /// Validate header fields
    pub fn validate(&self) -> Result<(), crate::error::DiagnosticError> {
        use crate::error::DiagnosticError;

        if &self.magic != WBB_MAGIC {
            return Err(DiagnosticError::InvalidMagic {
                expected: *WBB_MAGIC,
                actual: self.magic,
            });
        }

        if self.version > MAX_SUPPORTED_VERSION {
            return Err(DiagnosticError::UnsupportedVersion(self.version));
        }

        if self.compression_level > 9 {
            return Err(DiagnosticError::Configuration(format!(
                "Invalid compression level: {}",
                self.compression_level
            )));
        }

        Ok(())
    }
}

/// Index entry for random access (every 100ms)
///
/// Index entries enable fast seeking to specific timestamps
/// during replay operations.
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Timestamp offset from start (ms)
    pub timestamp_ms: u32,
    /// File offset for Stream A
    pub stream_a_offset: u64,
    /// File offset for Stream B
    pub stream_b_offset: u64,
    /// File offset for Stream C
    pub stream_c_offset: u64,
    /// Number of frames in this 100ms window
    pub frame_count: u32,
}

impl IndexEntry {
    /// Create a new index entry
    pub fn new(timestamp_ms: u32, frame_count: u32) -> Self {
        Self {
            timestamp_ms,
            stream_a_offset: 0,
            stream_b_offset: 0,
            stream_c_offset: 0,
            frame_count,
        }
    }
}

/// .wbb v1 file footer
///
/// The footer appears at the end of every .wbb file and contains
/// summary information and the index location for random access.
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbbFooter {
    /// Total recording duration (ms)
    pub duration_ms: u32,
    /// Total frames recorded
    pub total_frames: u64,
    /// Index offset in file
    pub index_offset: u64,
    /// Number of index entries
    pub index_count: u32,
    /// CRC32C of entire file (excluding this field)
    pub file_crc32c: u32,
    /// Footer magic "1BBW" (WBB1 reversed)
    pub footer_magic: [u8; 4],
}

impl WbbFooter {
    /// Create a new footer
    pub fn new(duration_ms: u32, total_frames: u64) -> Self {
        Self {
            duration_ms,
            total_frames,
            index_offset: 0,
            index_count: 0,
            file_crc32c: 0,
            footer_magic: *WBB_FOOTER_MAGIC,
        }
    }

    /// Validate footer fields
    pub fn validate(&self) -> Result<(), crate::error::DiagnosticError> {
        use crate::error::DiagnosticError;

        if &self.footer_magic != WBB_FOOTER_MAGIC {
            return Err(DiagnosticError::InvalidMagic {
                expected: *WBB_FOOTER_MAGIC,
                actual: self.footer_magic,
            });
        }

        Ok(())
    }
}

/// Stream type identifier for record routing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StreamType {
    /// 1kHz frames
    A = 0,
    /// 60Hz telemetry
    B = 1,
    /// Health events
    C = 2,
}

impl StreamType {
    /// Get the stream flag bit for this stream type
    pub fn flag(&self) -> u8 {
        match self {
            StreamType::A => STREAM_A_ID,
            StreamType::B => STREAM_B_ID,
            StreamType::C => STREAM_C_ID,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_wbb_header_creation() {
        let header = WbbHeader::new("test-device", 1, 0x07, 6);

        assert_eq!(header.magic, *WBB_MAGIC);
        assert_eq!(header.version, WBB_VERSION);
        assert_eq!(header.device_id, "test-device");
        assert_eq!(header.ffb_mode, 1);
        assert_eq!(header.stream_flags, 0x07);
        assert_eq!(header.compression_level, 6);
    }

    #[test]
    fn test_stream_flags() {
        let header = WbbHeader::new("test", 0, STREAM_A_ID | STREAM_C_ID, 0);

        assert!(header.has_stream_a());
        assert!(!header.has_stream_b());
        assert!(header.has_stream_c());
    }

    #[test]
    fn test_header_validation() {
        let mut header = WbbHeader::new("test", 1, 0x07, 6);
        assert!(header.validate().is_ok());

        header.magic = *b"XXXX";
        assert!(header.validate().is_err());

        header.magic = *WBB_MAGIC;
        header.version = 99;
        assert!(header.validate().is_err());
    }

    #[test]
    fn test_footer_validation() {
        let footer = WbbFooter::new(1000, 100);
        assert!(footer.validate().is_ok());
    }

    #[test]
    fn test_index_entry() {
        let entry = IndexEntry::new(100, 100);
        assert_eq!(entry.timestamp_ms, 100);
        assert_eq!(entry.frame_count, 100);
    }

    #[test]
    fn test_stream_type_flag() {
        assert_eq!(StreamType::A.flag(), STREAM_A_ID);
        assert_eq!(StreamType::B.flag(), STREAM_B_ID);
        assert_eq!(StreamType::C.flag(), STREAM_C_ID);
    }
}
