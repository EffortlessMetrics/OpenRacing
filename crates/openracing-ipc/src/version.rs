//! Protocol version negotiation and feature flags for IPC
//!
//! This module provides structured version types, feature flag bitmaps,
//! and a negotiation protocol for ensuring client-server compatibility.
//!
//! # Version Compatibility Rules
//!
//! - Major version must match exactly (breaking changes)
//! - Minor version: newer client works with older server within same major
//! - Patch version: always compatible within same major.minor
//!
//! # Examples
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use openracing_ipc::version::{ProtocolVersion, FeatureFlags, VersionNegotiator};
//!
//! let client = ProtocolVersion::new(1, 2, 0);
//! let server = ProtocolVersion::new(1, 1, 0);
//! let min = ProtocolVersion::new(1, 0, 0);
//!
//! let negotiator = VersionNegotiator::new(server, min);
//! let result = negotiator.negotiate(&client, FeatureFlags::DEVICE_MANAGEMENT)?;
//! assert!(result.compatible);
//! # Ok(())
//! # }
//! ```

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{IpcError, IpcResult};

/// Protocol version with semantic versioning (major.minor.patch).
///
/// # Wire Format
///
/// Encodes to 6 bytes: 2 bytes each for major, minor, patch (little-endian u16).
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use openracing_ipc::version::ProtocolVersion;
///
/// let v = ProtocolVersion::new(1, 2, 3);
/// assert_eq!(v.major(), 1);
/// assert_eq!(v.minor(), 2);
/// assert_eq!(v.patch(), 3);
///
/// let bytes = v.to_bytes();
/// let decoded = ProtocolVersion::from_bytes(&bytes)?;
/// assert_eq!(v, decoded);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProtocolVersion {
    major: u16,
    minor: u16,
    patch: u16,
}

impl ProtocolVersion {
    /// Wire size in bytes (3 × u16 = 6 bytes).
    pub const SIZE: usize = 6;

    /// Create a new protocol version.
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parse a version string like "1.2.3".
    pub fn parse(s: &str) -> IpcResult<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(IpcError::InvalidConfig(format!(
                "Invalid version format '{}': expected major.minor.patch",
                s
            )));
        }

        let major = parts[0]
            .parse::<u16>()
            .map_err(|_| IpcError::InvalidConfig(format!("Invalid major version in '{}'", s)))?;
        let minor = parts[1]
            .parse::<u16>()
            .map_err(|_| IpcError::InvalidConfig(format!("Invalid minor version in '{}'", s)))?;
        let patch = parts[2]
            .parse::<u16>()
            .map_err(|_| IpcError::InvalidConfig(format!("Invalid patch version in '{}'", s)))?;

        Ok(Self {
            major,
            minor,
            patch,
        })
    }

    /// Major version number.
    pub const fn major(&self) -> u16 {
        self.major
    }

    /// Minor version number.
    pub const fn minor(&self) -> u16 {
        self.minor
    }

    /// Patch version number.
    pub const fn patch(&self) -> u16 {
        self.patch
    }

    /// Encode to 6 bytes (little-endian).
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..2].copy_from_slice(&self.major.to_le_bytes());
        buf[2..4].copy_from_slice(&self.minor.to_le_bytes());
        buf[4..6].copy_from_slice(&self.patch.to_le_bytes());
        buf
    }

    /// Decode from bytes (little-endian).
    pub fn from_bytes(bytes: &[u8]) -> IpcResult<Self> {
        if bytes.len() < Self::SIZE {
            return Err(IpcError::DecodingFailed(format!(
                "Insufficient bytes for ProtocolVersion: need {}, got {}",
                Self::SIZE,
                bytes.len()
            )));
        }
        let major = u16::from_le_bytes([bytes[0], bytes[1]]);
        let minor = u16::from_le_bytes([bytes[2], bytes[3]]);
        let patch = u16::from_le_bytes([bytes[4], bytes[5]]);
        Ok(Self {
            major,
            minor,
            patch,
        })
    }

    /// Check if this version is compatible with a minimum required version.
    ///
    /// Compatibility rules:
    /// - Major versions must match
    /// - This version's minor must be >= minimum's minor
    /// - If minor versions match, this patch must be >= minimum's patch
    pub fn is_compatible_with(&self, min: &ProtocolVersion) -> bool {
        if self.major != min.major {
            return false;
        }
        if self.minor < min.minor {
            return false;
        }
        if self.minor == min.minor && self.patch < min.patch {
            return false;
        }
        true
    }

    /// Check backward compatibility: can a message from `older` be read by `self`?
    ///
    /// Within the same major version, a newer reader can always read older messages.
    pub fn can_read_from(&self, older: &ProtocolVersion) -> bool {
        self.major == older.major
            && (self.minor > older.minor
                || (self.minor == older.minor && self.patch >= older.patch))
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialOrd for ProtocolVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ProtocolVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
    }
}

/// Feature flags bitmap for capability negotiation.
///
/// Each bit represents a feature the client or server supports.
/// During negotiation, the intersection (AND) of client and server
/// flags determines the enabled feature set.
///
/// # Examples
///
/// ```
/// use openracing_ipc::version::FeatureFlags;
///
/// let client = FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::PROFILE_MANAGEMENT;
/// let server = FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::SAFETY_CONTROL;
///
/// let negotiated = client & server;
/// assert!(negotiated.contains(FeatureFlags::DEVICE_MANAGEMENT));
/// assert!(!negotiated.contains(FeatureFlags::PROFILE_MANAGEMENT));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FeatureFlags(u64);

impl FeatureFlags {
    /// No features.
    pub const NONE: Self = Self(0);
    /// Device management (list, status, subscribe).
    pub const DEVICE_MANAGEMENT: Self = Self(1 << 0);
    /// Profile management (list, apply, switch).
    pub const PROFILE_MANAGEMENT: Self = Self(1 << 1);
    /// Safety control (high torque, e-stop).
    pub const SAFETY_CONTROL: Self = Self(1 << 2);
    /// Health monitoring (subscribe, diagnostics).
    pub const HEALTH_MONITORING: Self = Self(1 << 3);
    /// Game integration.
    pub const GAME_INTEGRATION: Self = Self(1 << 4);
    /// Streaming health events.
    pub const STREAMING_HEALTH: Self = Self(1 << 5);
    /// Streaming device events.
    pub const STREAMING_DEVICES: Self = Self(1 << 6);
    /// Telemetry data access.
    pub const TELEMETRY: Self = Self(1 << 7);

    /// All v1.0 features.
    pub const ALL_V1: Self = Self(0xFF);

    /// Wire size in bytes.
    pub const SIZE: usize = 8;

    /// Create from raw bits.
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// Get raw bits.
    pub const fn bits(&self) -> u64 {
        self.0
    }

    /// Check if specific flags are all set.
    pub const fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Check if any of the specified flags are set.
    pub const fn intersects(&self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    /// Check if no flags are set.
    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Count the number of set flags.
    pub const fn count(&self) -> u32 {
        self.0.count_ones()
    }

    /// Encode to 8 bytes (little-endian).
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        self.0.to_le_bytes()
    }

    /// Decode from bytes (little-endian).
    pub fn from_bytes(bytes: &[u8]) -> IpcResult<Self> {
        if bytes.len() < Self::SIZE {
            return Err(IpcError::DecodingFailed(format!(
                "Insufficient bytes for FeatureFlags: need {}, got {}",
                Self::SIZE,
                bytes.len()
            )));
        }
        let bits = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        Ok(Self(bits))
    }

    /// Get human-readable names for set flags.
    pub fn names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.contains(Self::DEVICE_MANAGEMENT) {
            names.push("device_management");
        }
        if self.contains(Self::PROFILE_MANAGEMENT) {
            names.push("profile_management");
        }
        if self.contains(Self::SAFETY_CONTROL) {
            names.push("safety_control");
        }
        if self.contains(Self::HEALTH_MONITORING) {
            names.push("health_monitoring");
        }
        if self.contains(Self::GAME_INTEGRATION) {
            names.push("game_integration");
        }
        if self.contains(Self::STREAMING_HEALTH) {
            names.push("streaming_health");
        }
        if self.contains(Self::STREAMING_DEVICES) {
            names.push("streaming_devices");
        }
        if self.contains(Self::TELEMETRY) {
            names.push("telemetry");
        }
        names
    }
}

impl std::ops::BitOr for FeatureFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitAnd for FeatureFlags {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl std::ops::BitOrAssign for FeatureFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl fmt::Display for FeatureFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = self.names();
        if names.is_empty() {
            write!(f, "(none)")
        } else {
            write!(f, "{}", names.join(", "))
        }
    }
}

/// Version information exchanged during handshake.
///
/// # Wire Format
///
/// ```text
/// [ProtocolVersion: 6 bytes][FeatureFlags: 8 bytes][MinVersion: 6 bytes] = 20 bytes
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Current protocol version.
    pub version: ProtocolVersion,
    /// Supported feature flags.
    pub features: FeatureFlags,
    /// Minimum protocol version this endpoint supports.
    pub min_version: ProtocolVersion,
}

impl VersionInfo {
    /// Wire size in bytes.
    pub const SIZE: usize = ProtocolVersion::SIZE + FeatureFlags::SIZE + ProtocolVersion::SIZE;

    /// Create a new version info.
    pub const fn new(
        version: ProtocolVersion,
        features: FeatureFlags,
        min_version: ProtocolVersion,
    ) -> Self {
        Self {
            version,
            features,
            min_version,
        }
    }

    /// Encode to bytes.
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        let ver = self.version.to_bytes();
        let feat = self.features.to_bytes();
        let min = self.min_version.to_bytes();
        buf[0..6].copy_from_slice(&ver);
        buf[6..14].copy_from_slice(&feat);
        buf[14..20].copy_from_slice(&min);
        buf
    }

    /// Decode from bytes.
    pub fn from_bytes(bytes: &[u8]) -> IpcResult<Self> {
        if bytes.len() < Self::SIZE {
            return Err(IpcError::DecodingFailed(format!(
                "Insufficient bytes for VersionInfo: need {}, got {}",
                Self::SIZE,
                bytes.len()
            )));
        }
        let version = ProtocolVersion::from_bytes(&bytes[0..6])?;
        let features = FeatureFlags::from_bytes(&bytes[6..14])?;
        let min_version = ProtocolVersion::from_bytes(&bytes[14..20])?;
        Ok(Self {
            version,
            features,
            min_version,
        })
    }
}

/// Result of version negotiation between client and server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NegotiationResult {
    /// Whether the versions are compatible.
    pub compatible: bool,
    /// The effective protocol version (minimum of both sides).
    pub effective_version: ProtocolVersion,
    /// Negotiated feature flags (intersection of both sides).
    pub negotiated_features: FeatureFlags,
    /// Server's version info.
    pub server_version: ProtocolVersion,
    /// Client's version info.
    pub client_version: ProtocolVersion,
    /// Human-readable reason if incompatible.
    pub rejection_reason: Option<String>,
}

/// Version negotiator that performs the handshake protocol.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use openracing_ipc::version::{ProtocolVersion, FeatureFlags, VersionNegotiator};
///
/// let server_version = ProtocolVersion::new(1, 2, 0);
/// let min_version = ProtocolVersion::new(1, 0, 0);
///
/// let negotiator = VersionNegotiator::new(server_version, min_version);
///
/// // Compatible client
/// let client_version = ProtocolVersion::new(1, 1, 0);
/// let client_features = FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::TELEMETRY;
/// let result = negotiator.negotiate(&client_version, client_features)?;
/// assert!(result.compatible);
///
/// // Incompatible client (different major)
/// let old_client = ProtocolVersion::new(2, 0, 0);
/// let result = negotiator.negotiate(&old_client, FeatureFlags::NONE)?;
/// assert!(!result.compatible);
/// assert!(result.rejection_reason.is_some());
/// # Ok(())
/// # }
/// ```
pub struct VersionNegotiator {
    server_version: ProtocolVersion,
    server_features: FeatureFlags,
    min_supported: ProtocolVersion,
}

impl VersionNegotiator {
    /// Create a new negotiator with server version and minimum supported version.
    ///
    /// The server features default to `FeatureFlags::ALL_V1`.
    pub fn new(server_version: ProtocolVersion, min_supported: ProtocolVersion) -> Self {
        Self {
            server_version,
            server_features: FeatureFlags::ALL_V1,
            min_supported,
        }
    }

    /// Create a negotiator with explicit feature flags.
    pub fn with_features(
        server_version: ProtocolVersion,
        min_supported: ProtocolVersion,
        server_features: FeatureFlags,
    ) -> Self {
        Self {
            server_version,
            server_features,
            min_supported,
        }
    }

    /// Server version.
    pub fn server_version(&self) -> ProtocolVersion {
        self.server_version
    }

    /// Minimum supported version.
    pub fn min_supported(&self) -> ProtocolVersion {
        self.min_supported
    }

    /// Server feature flags.
    pub fn server_features(&self) -> FeatureFlags {
        self.server_features
    }

    /// Perform version negotiation with a client.
    ///
    /// Returns a `NegotiationResult` indicating compatibility and negotiated features.
    /// This always succeeds (returns Ok) — incompatibility is indicated in the result,
    /// not as an error, so callers can inspect the reason.
    pub fn negotiate(
        &self,
        client_version: &ProtocolVersion,
        client_features: FeatureFlags,
    ) -> IpcResult<NegotiationResult> {
        // Check major version match
        if client_version.major() != self.server_version.major() {
            return Ok(NegotiationResult {
                compatible: false,
                effective_version: self.server_version,
                negotiated_features: FeatureFlags::NONE,
                server_version: self.server_version,
                client_version: *client_version,
                rejection_reason: Some(format!(
                    "Major version mismatch: client v{} is not compatible with server v{} \
                     (major versions must match)",
                    client_version, self.server_version
                )),
            });
        }

        // Check minimum version requirement
        if !client_version.is_compatible_with(&self.min_supported) {
            return Ok(NegotiationResult {
                compatible: false,
                effective_version: self.server_version,
                negotiated_features: FeatureFlags::NONE,
                server_version: self.server_version,
                client_version: *client_version,
                rejection_reason: Some(format!(
                    "Client version v{} is below minimum supported v{}: please upgrade",
                    client_version, self.min_supported
                )),
            });
        }

        // Negotiate features (intersection)
        let negotiated = client_features & self.server_features;

        // Effective version is the lower of the two
        let effective = if *client_version < self.server_version {
            *client_version
        } else {
            self.server_version
        };

        Ok(NegotiationResult {
            compatible: true,
            effective_version: effective,
            negotiated_features: negotiated,
            server_version: self.server_version,
            client_version: *client_version,
            rejection_reason: None,
        })
    }

    /// Negotiate using a full VersionInfo struct from the client.
    pub fn negotiate_info(&self, client_info: &VersionInfo) -> IpcResult<NegotiationResult> {
        // First, check if the server meets the client's minimum requirement
        if !self
            .server_version
            .is_compatible_with(&client_info.min_version)
        {
            return Ok(NegotiationResult {
                compatible: false,
                effective_version: self.server_version,
                negotiated_features: FeatureFlags::NONE,
                server_version: self.server_version,
                client_version: client_info.version,
                rejection_reason: Some(format!(
                    "Server version v{} is below client's minimum required v{}: \
                     client requires a newer server",
                    self.server_version, client_info.min_version
                )),
            });
        }

        self.negotiate(&client_info.version, client_info.features)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ProtocolVersion tests ---

    #[test]
    fn test_version_new_and_accessors() {
        let v = ProtocolVersion::new(1, 2, 3);
        assert_eq!(v.major(), 1);
        assert_eq!(v.minor(), 2);
        assert_eq!(v.patch(), 3);
    }

    #[test]
    fn test_version_display() {
        let v = ProtocolVersion::new(1, 2, 3);
        assert_eq!(format!("{v}"), "1.2.3");
    }

    #[test]
    fn test_version_parse() -> IpcResult<()> {
        let v = ProtocolVersion::parse("1.2.3")?;
        assert_eq!(v, ProtocolVersion::new(1, 2, 3));
        Ok(())
    }

    #[test]
    fn test_version_parse_invalid() {
        assert!(ProtocolVersion::parse("1.2").is_err());
        assert!(ProtocolVersion::parse("abc").is_err());
        assert!(ProtocolVersion::parse("1.2.x").is_err());
        assert!(ProtocolVersion::parse("").is_err());
    }

    #[test]
    fn test_version_bytes_roundtrip() -> IpcResult<()> {
        let v = ProtocolVersion::new(1, 2, 3);
        let bytes = v.to_bytes();
        assert_eq!(bytes.len(), ProtocolVersion::SIZE);
        let decoded = ProtocolVersion::from_bytes(&bytes)?;
        assert_eq!(v, decoded);
        Ok(())
    }

    #[test]
    fn test_version_from_bytes_too_short() {
        assert!(ProtocolVersion::from_bytes(&[0; 5]).is_err());
    }

    #[test]
    fn test_version_ordering() {
        let v1_0_0 = ProtocolVersion::new(1, 0, 0);
        let v1_1_0 = ProtocolVersion::new(1, 1, 0);
        let v1_1_1 = ProtocolVersion::new(1, 1, 1);
        let v2_0_0 = ProtocolVersion::new(2, 0, 0);

        assert!(v1_0_0 < v1_1_0);
        assert!(v1_1_0 < v1_1_1);
        assert!(v1_1_1 < v2_0_0);
    }

    #[test]
    fn test_version_compatibility() {
        let min = ProtocolVersion::new(1, 0, 0);
        assert!(ProtocolVersion::new(1, 0, 0).is_compatible_with(&min));
        assert!(ProtocolVersion::new(1, 1, 0).is_compatible_with(&min));
        assert!(ProtocolVersion::new(1, 0, 1).is_compatible_with(&min));
        assert!(!ProtocolVersion::new(0, 9, 0).is_compatible_with(&min));
        assert!(!ProtocolVersion::new(2, 0, 0).is_compatible_with(&min));
    }

    #[test]
    fn test_version_can_read_from() {
        let v1_1 = ProtocolVersion::new(1, 1, 0);
        let v1_0 = ProtocolVersion::new(1, 0, 0);
        let v2_0 = ProtocolVersion::new(2, 0, 0);

        assert!(v1_1.can_read_from(&v1_0));
        assert!(v1_1.can_read_from(&v1_1));
        assert!(!v1_0.can_read_from(&v1_1));
        assert!(!v2_0.can_read_from(&v1_0));
    }

    // --- FeatureFlags tests ---

    #[test]
    fn test_feature_flags_basic_ops() {
        let a = FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::SAFETY_CONTROL;
        assert!(a.contains(FeatureFlags::DEVICE_MANAGEMENT));
        assert!(a.contains(FeatureFlags::SAFETY_CONTROL));
        assert!(!a.contains(FeatureFlags::TELEMETRY));
        assert!(!a.is_empty());
        assert_eq!(a.count(), 2);
    }

    #[test]
    fn test_feature_flags_intersection() {
        let client = FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::PROFILE_MANAGEMENT;
        let server = FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::SAFETY_CONTROL;
        let negotiated = client & server;
        assert!(negotiated.contains(FeatureFlags::DEVICE_MANAGEMENT));
        assert!(!negotiated.contains(FeatureFlags::PROFILE_MANAGEMENT));
        assert!(!negotiated.contains(FeatureFlags::SAFETY_CONTROL));
    }

    #[test]
    fn test_feature_flags_none() {
        assert!(FeatureFlags::NONE.is_empty());
        assert_eq!(FeatureFlags::NONE.count(), 0);
    }

    #[test]
    fn test_feature_flags_bytes_roundtrip() -> IpcResult<()> {
        let flags = FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::TELEMETRY;
        let bytes = flags.to_bytes();
        let decoded = FeatureFlags::from_bytes(&bytes)?;
        assert_eq!(flags, decoded);
        Ok(())
    }

    #[test]
    fn test_feature_flags_names() {
        let flags = FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::SAFETY_CONTROL;
        let names = flags.names();
        assert!(names.contains(&"device_management"));
        assert!(names.contains(&"safety_control"));
        assert!(!names.contains(&"telemetry"));
    }

    #[test]
    fn test_feature_flags_display() {
        let flags = FeatureFlags::DEVICE_MANAGEMENT;
        assert_eq!(format!("{flags}"), "device_management");
        assert_eq!(format!("{}", FeatureFlags::NONE), "(none)");
    }

    #[test]
    fn test_feature_flags_from_bytes_too_short() {
        assert!(FeatureFlags::from_bytes(&[0; 7]).is_err());
    }

    // --- VersionInfo tests ---

    #[test]
    fn test_version_info_bytes_roundtrip() -> IpcResult<()> {
        let info = VersionInfo::new(
            ProtocolVersion::new(1, 2, 3),
            FeatureFlags::ALL_V1,
            ProtocolVersion::new(1, 0, 0),
        );
        let bytes = info.to_bytes();
        assert_eq!(bytes.len(), VersionInfo::SIZE);
        let decoded = VersionInfo::from_bytes(&bytes)?;
        assert_eq!(info, decoded);
        Ok(())
    }

    #[test]
    fn test_version_info_from_bytes_too_short() {
        assert!(VersionInfo::from_bytes(&[0; 19]).is_err());
    }

    // --- VersionNegotiator tests ---

    #[test]
    fn test_negotiation_compatible() -> IpcResult<()> {
        let negotiator =
            VersionNegotiator::new(ProtocolVersion::new(1, 2, 0), ProtocolVersion::new(1, 0, 0));
        let result = negotiator.negotiate(
            &ProtocolVersion::new(1, 1, 0),
            FeatureFlags::DEVICE_MANAGEMENT,
        )?;
        assert!(result.compatible);
        assert!(result.rejection_reason.is_none());
        assert!(
            result
                .negotiated_features
                .contains(FeatureFlags::DEVICE_MANAGEMENT)
        );
        // Effective version is the lower (client's 1.1.0)
        assert_eq!(result.effective_version, ProtocolVersion::new(1, 1, 0));
        Ok(())
    }

    #[test]
    fn test_negotiation_major_mismatch() -> IpcResult<()> {
        let negotiator =
            VersionNegotiator::new(ProtocolVersion::new(1, 0, 0), ProtocolVersion::new(1, 0, 0));
        let result = negotiator.negotiate(
            &ProtocolVersion::new(2, 0, 0),
            FeatureFlags::DEVICE_MANAGEMENT,
        )?;
        assert!(!result.compatible);
        let reason = result.rejection_reason.as_deref();
        assert!(reason.is_some());
        assert!(
            reason
                .unwrap_or_default()
                .contains("Major version mismatch")
        );
        Ok(())
    }

    #[test]
    fn test_negotiation_below_minimum() -> IpcResult<()> {
        let negotiator =
            VersionNegotiator::new(ProtocolVersion::new(1, 2, 0), ProtocolVersion::new(1, 1, 0));
        let result = negotiator.negotiate(
            &ProtocolVersion::new(1, 0, 0),
            FeatureFlags::DEVICE_MANAGEMENT,
        )?;
        assert!(!result.compatible);
        let reason = result.rejection_reason.as_deref();
        assert!(reason.unwrap_or_default().contains("below minimum"));
        Ok(())
    }

    #[test]
    fn test_negotiation_feature_intersection() -> IpcResult<()> {
        let negotiator = VersionNegotiator::with_features(
            ProtocolVersion::new(1, 0, 0),
            ProtocolVersion::new(1, 0, 0),
            FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::SAFETY_CONTROL,
        );
        let client_features = FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::PROFILE_MANAGEMENT;
        let result = negotiator.negotiate(&ProtocolVersion::new(1, 0, 0), client_features)?;
        assert!(result.compatible);
        assert!(
            result
                .negotiated_features
                .contains(FeatureFlags::DEVICE_MANAGEMENT)
        );
        assert!(
            !result
                .negotiated_features
                .contains(FeatureFlags::PROFILE_MANAGEMENT)
        );
        assert!(
            !result
                .negotiated_features
                .contains(FeatureFlags::SAFETY_CONTROL)
        );
        Ok(())
    }

    #[test]
    fn test_negotiate_info_server_below_client_min() -> IpcResult<()> {
        let negotiator =
            VersionNegotiator::new(ProtocolVersion::new(1, 0, 0), ProtocolVersion::new(1, 0, 0));
        let client_info = VersionInfo::new(
            ProtocolVersion::new(1, 2, 0),
            FeatureFlags::DEVICE_MANAGEMENT,
            ProtocolVersion::new(1, 1, 0), // client requires server >= 1.1.0
        );
        let result = negotiator.negotiate_info(&client_info)?;
        assert!(!result.compatible);
        let reason = result.rejection_reason.as_deref();
        assert!(
            reason
                .unwrap_or_default()
                .contains("client requires a newer server")
        );
        Ok(())
    }

    #[test]
    fn test_negotiation_newer_client_with_older_server() -> IpcResult<()> {
        let negotiator =
            VersionNegotiator::new(ProtocolVersion::new(1, 1, 0), ProtocolVersion::new(1, 0, 0));
        let result = negotiator.negotiate(&ProtocolVersion::new(1, 3, 0), FeatureFlags::ALL_V1)?;
        assert!(result.compatible);
        // Effective version should be server's version (lower)
        assert_eq!(result.effective_version, ProtocolVersion::new(1, 1, 0));
        Ok(())
    }

    #[test]
    fn test_negotiation_same_version() -> IpcResult<()> {
        let negotiator =
            VersionNegotiator::new(ProtocolVersion::new(1, 0, 0), ProtocolVersion::new(1, 0, 0));
        let result = negotiator.negotiate(&ProtocolVersion::new(1, 0, 0), FeatureFlags::ALL_V1)?;
        assert!(result.compatible);
        assert_eq!(result.effective_version, ProtocolVersion::new(1, 0, 0));
        assert_eq!(result.negotiated_features, FeatureFlags::ALL_V1);
        Ok(())
    }

    #[test]
    fn test_negotiation_no_features() -> IpcResult<()> {
        let negotiator =
            VersionNegotiator::new(ProtocolVersion::new(1, 0, 0), ProtocolVersion::new(1, 0, 0));
        let result = negotiator.negotiate(&ProtocolVersion::new(1, 0, 0), FeatureFlags::NONE)?;
        assert!(result.compatible);
        assert!(result.negotiated_features.is_empty());
        Ok(())
    }
}
