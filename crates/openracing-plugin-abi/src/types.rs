//! ABI type definitions for plugins.
//!
//! This module contains all type definitions used in the plugin ABI,
//! including capability flags, plugin headers, and state structures.

use crate::constants::{PLUG_ABI_MAGIC, PLUG_ABI_VERSION, WASM_ABI_VERSION};
use bitflags::bitflags;

bitflags! {
    /// Plugin capability flags.
    ///
    /// These flags indicate what operations a plugin can perform.
    /// All unused bits are reserved for future capabilities.
    ///
    /// # ABI Stability
    ///
    /// The bit layout of these flags is guaranteed to be stable.
    /// New capabilities may be added using currently reserved bits.
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

/// Plugin header for ABI handshake and capability declaration.
///
/// All integers are stored in little-endian format.
/// This structure is used for initial handshake between host and plugin.
///
/// # ABI Stability
///
/// This structure has a fixed size of 16 bytes and 4-byte alignment.
/// The layout is guaranteed to be stable across versions.
///
/// # Memory Layout
///
/// | Offset | Size | Field        |
/// |--------|------|--------------|
/// | 0      | 4    | magic        |
/// | 4      | 4    | abi_version  |
/// | 8      | 4    | capabilities |
/// | 12     | 4    | reserved     |
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

impl PluginHeader {
    /// Create a new plugin header with specified capabilities.
    #[must_use]
    pub fn new(capabilities: PluginCapabilities) -> Self {
        Self {
            magic: PLUG_ABI_MAGIC,
            abi_version: PLUG_ABI_VERSION,
            capabilities: capabilities.bits(),
            reserved: 0,
        }
    }

    /// Validate the header magic and version.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.magic == PLUG_ABI_MAGIC && self.abi_version == PLUG_ABI_VERSION
    }

    /// Get the capabilities as a bitflags struct.
    #[must_use]
    pub fn get_capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::from_bits_truncate(self.capabilities)
    }

    /// Convert header to byte array (little-endian).
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&self.magic.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.abi_version.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.capabilities.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.reserved.to_le_bytes());
        bytes
    }

    /// Create header from byte array (little-endian).
    #[must_use]
    pub fn from_bytes(bytes: &[u8; 16]) -> Self {
        Self {
            magic: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            abi_version: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            capabilities: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            reserved: u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        }
    }

    /// Check if a specific capability is set.
    #[must_use]
    pub fn has_capability(&self, cap: PluginCapabilities) -> bool {
        self.get_capabilities().contains(cap)
    }
}

static_assertions::const_assert_eq!(std::mem::size_of::<PluginHeader>(), 16);
static_assertions::const_assert_eq!(std::mem::align_of::<PluginHeader>(), 4);
static_assertions::const_assert_eq!(std::mem::size_of::<PluginCapabilities>(), 4);

/// Plugin initialization status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PluginInitStatus {
    /// Plugin has not been initialized yet
    #[default]
    Uninitialized,
    /// Plugin is currently initializing
    Initializing,
    /// Plugin initialized successfully
    Initialized,
    /// Plugin initialization failed
    Failed,
    /// Plugin has been shut down
    ShutDown,
}

/// WASM plugin export validation result.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WasmExportValidation {
    /// Whether the required 'process' function is exported
    pub has_process: bool,
    /// Whether the required 'memory' export is present
    pub has_memory: bool,
    /// Whether the optional 'init' function is exported
    pub has_init: bool,
    /// Whether the optional 'shutdown' function is exported
    pub has_shutdown: bool,
    /// Whether the optional 'get_info' function is exported
    pub has_get_info: bool,
}

impl WasmExportValidation {
    /// Check if all required exports are present.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.has_process && self.has_memory
    }

    /// Get a list of missing required exports.
    #[must_use]
    pub fn missing_required(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.has_process {
            missing.push(crate::constants::wasm_export::PROCESS);
        }
        if !self.has_memory {
            missing.push(crate::constants::wasm_export::MEMORY);
        }
        missing
    }
}

/// Plugin info structure returned by get_info().
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WasmPluginInfo {
    /// Plugin name
    pub name: String,
    /// Plugin version string
    pub version: String,
    /// Plugin author
    pub author: String,
    /// Plugin description
    pub description: String,
    /// ABI version the plugin was built for
    pub abi_version: u32,
}

impl Default for WasmPluginInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: String::new(),
            author: String::new(),
            description: String::new(),
            abi_version: WASM_ABI_VERSION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_header_size_and_alignment() {
        assert_eq!(std::mem::size_of::<PluginHeader>(), 16);
        assert_eq!(std::mem::align_of::<PluginHeader>(), 4);
    }

    #[test]
    fn test_plugin_capabilities_bitflags() {
        let caps = PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS;
        assert_eq!(caps.bits(), 0b0000_0011);

        let caps_with_haptics = caps | PluginCapabilities::HAPTICS;
        assert_eq!(caps_with_haptics.bits(), 0b0000_0111);

        let valid_caps =
            PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
        assert_eq!(valid_caps.bits() & PluginCapabilities::RESERVED.bits(), 0);
    }

    #[test]
    fn test_plugin_header_byte_serialization() {
        let header = PluginHeader::new(PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS);
        let bytes = header.to_bytes();

        let expected_bytes = [
            0x31, 0x4C, 0x57, 0x57, 0x00, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ];

        assert_eq!(bytes, expected_bytes);

        let restored = PluginHeader::from_bytes(&bytes);
        assert_eq!(header, restored);
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
            abi_version: 0x00020000,
            ..Default::default()
        };
        assert!(!invalid_version.is_valid());
    }

    #[test]
    fn test_plugin_header_capability_check() {
        let header = PluginHeader::new(PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS);
        assert!(header.has_capability(PluginCapabilities::TELEMETRY));
        assert!(header.has_capability(PluginCapabilities::LEDS));
        assert!(!header.has_capability(PluginCapabilities::HAPTICS));
    }

    #[test]
    fn test_capability_flags_reserved_bits() {
        let reserved_mask = PluginCapabilities::RESERVED.bits();
        let valid_mask = (PluginCapabilities::TELEMETRY
            | PluginCapabilities::LEDS
            | PluginCapabilities::HAPTICS)
            .bits();

        assert_eq!(reserved_mask & valid_mask, 0);
        assert_eq!(reserved_mask | valid_mask, 0xFFFF_FFFF);
    }

    #[test]
    fn test_plugin_init_status_default() {
        let status = PluginInitStatus::default();
        assert_eq!(status, PluginInitStatus::Uninitialized);
    }

    #[test]
    fn test_wasm_export_validation_valid() {
        let validation = WasmExportValidation {
            has_process: true,
            has_memory: true,
            has_init: false,
            has_shutdown: false,
            has_get_info: false,
        };

        assert!(validation.is_valid());
        assert!(validation.missing_required().is_empty());
    }

    #[test]
    fn test_wasm_export_validation_missing_process() {
        let validation = WasmExportValidation {
            has_process: false,
            has_memory: true,
            ..Default::default()
        };

        assert!(!validation.is_valid());
        let missing = validation.missing_required();
        assert_eq!(missing.len(), 1);
        assert!(missing.contains(&"process"));
    }

    #[test]
    fn test_wasm_export_validation_missing_both() {
        let validation = WasmExportValidation::default();

        assert!(!validation.is_valid());
        let missing = validation.missing_required();
        assert_eq!(missing.len(), 2);
    }

    #[test]
    fn test_wasm_plugin_info_default() {
        let info = WasmPluginInfo::default();

        assert!(info.name.is_empty());
        assert!(info.version.is_empty());
        assert_eq!(info.abi_version, WASM_ABI_VERSION);
    }
}
