//! A/B partition management for firmware updates
//!
//! Provides partition types and health status tracking for atomic firmware updates.

use serde::{Deserialize, Serialize};

/// Firmware partition identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Partition {
    /// Partition A
    A,
    /// Partition B
    B,
}

impl Partition {
    /// Get the other partition
    pub fn other(self) -> Self {
        match self {
            Partition::A => Partition::B,
            Partition::B => Partition::A,
        }
    }
}

impl std::fmt::Display for Partition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Partition::A => write!(f, "A"),
            Partition::B => write!(f, "B"),
        }
    }
}

/// Firmware partition status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    /// Partition identifier
    pub partition: Partition,

    /// Whether this partition is currently active (booted)
    pub active: bool,

    /// Whether this partition is bootable
    pub bootable: bool,

    /// Firmware version in this partition
    pub version: Option<semver::Version>,

    /// Size of firmware in bytes
    pub size_bytes: u64,

    /// SHA256 hash of firmware
    pub hash: Option<String>,

    /// Last update timestamp
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,

    /// Health status of this partition
    pub health: PartitionHealth,
}

impl PartitionInfo {
    /// Create a new partition info for an empty partition
    pub fn empty(partition: Partition) -> Self {
        Self {
            partition,
            active: false,
            bootable: false,
            version: None,
            size_bytes: 0,
            hash: None,
            updated_at: None,
            health: PartitionHealth::Unknown,
        }
    }

    /// Check if this partition can be used for an update
    pub fn can_update(&self) -> bool {
        !self.active
            && self.health
                != PartitionHealth::Corrupted {
                    reason: String::new(),
                }
    }
}

/// Health status of a firmware partition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum PartitionHealth {
    /// Partition is healthy and functional
    Healthy,

    /// Partition has minor issues but is functional
    Degraded {
        /// Reason for degraded status
        reason: String,
    },

    /// Partition is corrupted or non-functional
    Corrupted {
        /// Reason for corruption
        reason: String,
    },

    /// Partition status is unknown
    #[default]
    Unknown,
}

impl PartitionHealth {
    /// Check if the partition is usable
    pub fn is_usable(&self) -> bool {
        matches!(
            self,
            PartitionHealth::Healthy | PartitionHealth::Degraded { .. }
        )
    }

    /// Check if the partition needs repair
    pub fn needs_repair(&self) -> bool {
        matches!(
            self,
            PartitionHealth::Corrupted { .. } | PartitionHealth::Unknown
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partition_other() {
        assert_eq!(Partition::A.other(), Partition::B);
        assert_eq!(Partition::B.other(), Partition::A);
    }

    #[test]
    fn test_partition_display() {
        assert_eq!(format!("{}", Partition::A), "A");
        assert_eq!(format!("{}", Partition::B), "B");
    }

    #[test]
    fn test_partition_info_empty() {
        let info = PartitionInfo::empty(Partition::A);
        assert_eq!(info.partition, Partition::A);
        assert!(!info.active);
        assert!(!info.bootable);
        assert!(info.version.is_none());
        assert_eq!(info.size_bytes, 0);
    }

    #[test]
    fn test_partition_info_can_update() {
        let inactive = PartitionInfo {
            partition: Partition::B,
            active: false,
            bootable: false,
            version: None,
            size_bytes: 0,
            hash: None,
            updated_at: None,
            health: PartitionHealth::Unknown,
        };
        assert!(inactive.can_update());

        let active = PartitionInfo {
            partition: Partition::A,
            active: true,
            bootable: true,
            version: Some(semver::Version::new(1, 0, 0)),
            size_bytes: 1024,
            hash: Some("hash".to_string()),
            updated_at: None,
            health: PartitionHealth::Healthy,
        };
        assert!(!active.can_update());
    }

    #[test]
    fn test_partition_health_is_usable() {
        assert!(PartitionHealth::Healthy.is_usable());
        assert!(
            PartitionHealth::Degraded {
                reason: "test".to_string()
            }
            .is_usable()
        );
        assert!(
            !PartitionHealth::Corrupted {
                reason: "test".to_string()
            }
            .is_usable()
        );
        assert!(!PartitionHealth::Unknown.is_usable());
    }

    #[test]
    fn test_partition_health_needs_repair() {
        assert!(!PartitionHealth::Healthy.needs_repair());
        assert!(
            !PartitionHealth::Degraded {
                reason: "test".to_string()
            }
            .needs_repair()
        );
        assert!(
            PartitionHealth::Corrupted {
                reason: "test".to_string()
            }
            .needs_repair()
        );
        assert!(PartitionHealth::Unknown.needs_repair());
    }
}
