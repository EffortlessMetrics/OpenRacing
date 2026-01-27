//! Plugin ABI definitions with versioning and endianness
//!
//! This module defines the stable ABI for native plugins, including:
//! - Version handshake protocol
//! - Capability bitflags
//! - C-compatible data structures
//! - Endianness documentation (little-endian for all integers)

use bitflags::bitflags;

/// Plugin ABI version constant for handshake
/// Format: major version (16 bits) << 16 | minor version (16 bits)
/// Version 1.0 = 0x0001_0000
pub const PLUG_ABI_VERSION: u32 = 0x0001_0000;

/// Plugin ABI magic number for handshake validation
/// 'WWL1' in little-endian: 0x57574C31
pub const PLUG_ABI_MAGIC: u32 = 0x57574C31;

bitflags! {
    /// Plugin capability flags
    ///
    /// These flags indicate what operations a plugin can perform.
    /// All unused bits are reserved for future capabilities.
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct PluginCapabilities: u32 {
        /// Plugin can read telemetry data
        const TELEMETRY    = 0b0000_0001;

        /// Plugin can control LED patterns
        const LEDS         = 0b0000_0010;

        /// Plugin can process haptic feedback
        const HAPTICS      = 0b0000_0100;

        /// Reserved bits for future capabilities
        /// Plugins should not set these bits
        const RESERVED     = 0xFFFF_FFF8;
    }
}

/// Plugin header for ABI handshake and capability declaration
///
/// All integers are stored in little-endian format.
/// This structure is used for initial handshake between host and plugin.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginHeader {
    /// Magic number for validation (little-endian)
    /// Must be PLUG_ABI_MAGIC (0x57574C31)
    pub magic: u32,

    /// ABI version (little-endian)
    /// Must match PLUG_ABI_VERSION for compatibility
    pub abi_version: u32,

    /// Plugin capabilities bitfield (little-endian)
    /// See PluginCapabilities for valid flags
    pub capabilities: u32,

    /// Reserved field for future use
    /// Must be set to 0
    pub reserved: u32,
}

/// Telemetry frame for real-time plugin communication
///
/// All integers are stored in little-endian format.
/// Field names updated to match new schema conventions.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TelemetryFrame {
    /// Timestamp in microseconds (little-endian)
    pub timestamp_us: u64,

    /// Wheel angle in degrees (not millidegrees)
    /// Range: -1800.0 to +1800.0 degrees for 5-turn wheels
    pub wheel_angle_deg: f32,

    /// Wheel speed in radians per second (not mrad/s)
    /// Positive values indicate clockwise rotation
    pub wheel_speed_rad_s: f32,

    /// Temperature in degrees Celsius (not temp_c)
    /// Typical range: 20-80°C for normal operation
    pub temperature_c: f32,

    /// Fault flags bitfield (not faults)
    /// Each bit represents a specific fault condition
    pub fault_flags: u32,

    /// Padding to ensure 8-byte alignment
    pub _pad: u32,
}

impl Default for PluginHeader {
    fn default() -> Self {
        Self {
            magic: PLUG_ABI_MAGIC,
            abi_version: PLUG_ABI_VERSION,
            capabilities: 0,
            reserved: 0,
        }
    }
}

impl Default for TelemetryFrame {
    fn default() -> Self {
        Self {
            timestamp_us: 0,
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 20.0, // Room temperature default
            fault_flags: 0,
            _pad: 0,
        }
    }
}

impl PluginHeader {
    /// Create a new plugin header with specified capabilities
    pub fn new(capabilities: PluginCapabilities) -> Self {
        Self {
            magic: PLUG_ABI_MAGIC,
            abi_version: PLUG_ABI_VERSION,
            capabilities: capabilities.bits(),
            reserved: 0,
        }
    }

    /// Validate the header magic and version
    pub fn is_valid(&self) -> bool {
        self.magic == PLUG_ABI_MAGIC && self.abi_version == PLUG_ABI_VERSION
    }

    /// Get the capabilities as a bitflags struct
    pub fn get_capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::from_bits_truncate(self.capabilities)
    }

    /// Convert header to byte array (little-endian)
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&self.magic.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.abi_version.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.capabilities.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.reserved.to_le_bytes());
        bytes
    }

    /// Create header from byte array (little-endian)
    pub fn from_bytes(bytes: &[u8; 16]) -> Self {
        Self {
            magic: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            abi_version: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            capabilities: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            reserved: u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        }
    }
}

impl TelemetryFrame {
    /// Create a new telemetry frame with timestamp
    pub fn new(timestamp_us: u64) -> Self {
        Self {
            timestamp_us,
            ..Default::default()
        }
    }

    /// Convert frame to byte array for IPC
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[0..8].copy_from_slice(&self.timestamp_us.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.wheel_angle_deg.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.wheel_speed_rad_s.to_le_bytes());
        bytes[16..20].copy_from_slice(&self.temperature_c.to_le_bytes());
        bytes[20..24].copy_from_slice(&self.fault_flags.to_le_bytes());
        bytes[24..28].copy_from_slice(&self._pad.to_le_bytes());
        // bytes[28..32] remain zero (additional padding)
        bytes
    }

    /// Create frame from byte array
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            timestamp_us: u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            wheel_angle_deg: f32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            wheel_speed_rad_s: f32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
            temperature_c: f32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
            fault_flags: u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
            _pad: u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
        }
    }
}

// Compile-time size and alignment assertions
// These ensure ABI stability across different platforms and compilers
static_assertions::const_assert_eq!(std::mem::size_of::<PluginHeader>(), 16);
static_assertions::const_assert_eq!(std::mem::align_of::<PluginHeader>(), 4);
static_assertions::const_assert_eq!(std::mem::size_of::<TelemetryFrame>(), 32);
static_assertions::const_assert_eq!(std::mem::align_of::<TelemetryFrame>(), 8);

// Ensure bitflags has correct size
static_assertions::const_assert_eq!(std::mem::size_of::<PluginCapabilities>(), 4);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_header_size_and_alignment() {
        // Verify size and alignment requirements
        assert_eq!(std::mem::size_of::<PluginHeader>(), 16);
        assert_eq!(std::mem::align_of::<PluginHeader>(), 4);
    }

    #[test]
    fn test_telemetry_frame_size_and_alignment() {
        // Verify size and alignment requirements
        assert_eq!(std::mem::size_of::<TelemetryFrame>(), 32);
        assert_eq!(std::mem::align_of::<TelemetryFrame>(), 8);
    }

    #[test]
    fn test_plugin_capabilities_bitflags() {
        let caps = PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS;
        assert_eq!(caps.bits(), 0b0000_0011);

        let caps_with_haptics = caps | PluginCapabilities::HAPTICS;
        assert_eq!(caps_with_haptics.bits(), 0b0000_0111);

        // Test reserved bits are not set in valid capabilities
        let valid_caps =
            PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
        assert_eq!(valid_caps.bits() & PluginCapabilities::RESERVED.bits(), 0);
    }

    #[test]
    fn test_plugin_header_byte_exact_serialization() {
        // Create header with known values
        let header = PluginHeader {
            magic: PLUG_ABI_MAGIC,
            abi_version: PLUG_ABI_VERSION,
            capabilities: (PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS).bits(),
            reserved: 0,
        };

        // Convert to bytes
        let bytes = header.to_bytes();

        // Verify exact byte representation (little-endian)
        let expected_bytes = [
            // magic: 0x57574C31 in LE
            0x31, 0x4C, 0x57, 0x57, // abi_version: 0x00010000 in LE
            0x00, 0x00, 0x01, 0x00, // capabilities: 0x00000003 in LE
            0x03, 0x00, 0x00, 0x00, // reserved: 0x00000000 in LE
            0x00, 0x00, 0x00, 0x00,
        ];

        assert_eq!(bytes, expected_bytes);

        // Test round-trip conversion
        let restored_header = PluginHeader::from_bytes(&bytes);
        assert_eq!(header, restored_header);
    }

    #[test]
    fn test_plugin_header_validation() {
        let valid_header = PluginHeader::default();
        assert!(valid_header.is_valid());

        let invalid_magic = PluginHeader {
            magic: 0xDEADBEEF,
            ..Default::default()
        };
        assert!(!invalid_magic.is_valid());

        let invalid_version = PluginHeader {
            abi_version: 0x00020000, // Version 2.0
            ..Default::default()
        };
        assert!(!invalid_version.is_valid());
    }

    #[test]
    fn test_telemetry_frame_byte_serialization() {
        let frame = TelemetryFrame {
            timestamp_us: 1234567890,
            wheel_angle_deg: 45.5,
            wheel_speed_rad_s: std::f32::consts::PI,
            temperature_c: 65.0,
            fault_flags: 0x12345678,
            _pad: 0,
        };

        // Test round-trip conversion
        let bytes = frame.to_bytes();
        let restored_frame = TelemetryFrame::from_bytes(&bytes);

        assert_eq!(frame.timestamp_us, restored_frame.timestamp_us);
        assert_eq!(frame.wheel_angle_deg, restored_frame.wheel_angle_deg);
        assert_eq!(frame.wheel_speed_rad_s, restored_frame.wheel_speed_rad_s);
        assert_eq!(frame.temperature_c, restored_frame.temperature_c);
        assert_eq!(frame.fault_flags, restored_frame.fault_flags);
        assert_eq!(frame._pad, restored_frame._pad);
    }

    #[test]
    fn test_endianness_documentation() {
        // This test documents the little-endian requirement
        let test_value: u32 = 0x12345678;
        let le_bytes = test_value.to_le_bytes();

        // On little-endian systems, this should be [0x78, 0x56, 0x34, 0x12]
        // On big-endian systems, to_le_bytes() will swap to little-endian
        let expected_le = [0x78, 0x56, 0x34, 0x12];
        assert_eq!(le_bytes, expected_le);

        // Verify round-trip
        let restored = u32::from_le_bytes(le_bytes);
        assert_eq!(restored, test_value);
    }

    #[test]
    fn test_abi_constants() {
        // Verify ABI constants have expected values
        assert_eq!(PLUG_ABI_VERSION, 0x0001_0000); // Version 1.0
        assert_eq!(PLUG_ABI_MAGIC, 0x57574C31); // 'WWL1' in LE
    }

    #[test]
    fn test_capability_flags_reserved_bits() {
        // Ensure reserved bits are properly defined
        let reserved_mask = PluginCapabilities::RESERVED.bits();
        let valid_mask = (PluginCapabilities::TELEMETRY
            | PluginCapabilities::LEDS
            | PluginCapabilities::HAPTICS)
            .bits();

        // Reserved and valid bits should not overlap
        assert_eq!(reserved_mask & valid_mask, 0);

        // All bits should be accounted for
        assert_eq!(reserved_mask | valid_mask, 0xFFFF_FFFF);
    }
}
