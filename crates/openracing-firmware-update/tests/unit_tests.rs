//! Unit tests for firmware update crate

mod partition_tests {
    use openracing_firmware_update::prelude::*;

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

mod hardware_version_tests {
    use openracing_firmware_update::hardware_version::{HardwareVersion, HardwareVersionError};
    use std::cmp::Ordering;

    #[test]
    fn test_parse_simple_version() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("1")?;
        assert_eq!(v.components(), &[1]);
        Ok(())
    }

    #[test]
    fn test_parse_two_component_version() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("1.2")?;
        assert_eq!(v.components(), &[1, 2]);
        Ok(())
    }

    #[test]
    fn test_parse_three_component_version() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("1.2.3")?;
        assert_eq!(v.components(), &[1, 2, 3]);
        Ok(())
    }

    #[test]
    fn test_parse_empty_fails() {
        let result = HardwareVersion::parse("");
        assert!(matches!(result, Err(HardwareVersionError::Empty)));
    }

    #[test]
    fn test_numeric_comparison_10_vs_2() -> Result<(), HardwareVersionError> {
        let v2 = HardwareVersion::parse("2.0")?;
        let v10 = HardwareVersion::parse("10.0")?;

        assert!(v2 < v10, "2.0 should be less than 10.0");
        assert!(v10 > v2, "10.0 should be greater than 2.0");
        Ok(())
    }

    #[test]
    fn test_try_compare_valid() {
        let result = HardwareVersion::try_compare("2.0", "10.0");
        assert_eq!(result, Some(Ordering::Less));
    }

    #[test]
    fn test_try_compare_invalid_returns_none() {
        let result = HardwareVersion::try_compare("invalid", "1.0");
        assert_eq!(result, None);
    }
}

mod update_state_tests {
    use openracing_firmware_update::prelude::*;

    #[test]
    fn test_update_state_is_in_progress() {
        assert!(!UpdateState::Idle.is_in_progress());
        assert!(!UpdateState::Complete.is_in_progress());
        assert!(UpdateState::Verifying.is_in_progress());
        assert!(UpdateState::Flashing { progress: 50 }.is_in_progress());
        assert!(UpdateState::Downloading { progress: 25 }.is_in_progress());
        assert!(UpdateState::Rebooting.is_in_progress());
    }

    #[test]
    fn test_update_state_should_block_ffb() {
        assert!(!UpdateState::Idle.should_block_ffb());
        assert!(!UpdateState::Complete.should_block_ffb());
        assert!(UpdateState::Verifying.should_block_ffb());
        assert!(UpdateState::Flashing { progress: 50 }.should_block_ffb());
    }

    #[test]
    fn test_update_state_default() {
        let state = UpdateState::default();
        assert_eq!(state, UpdateState::Idle);
    }
}

mod staged_rollout_config_tests {
    use openracing_firmware_update::prelude::*;

    #[test]
    fn test_default_config() {
        let config = StagedRolloutConfig::default();
        assert!(config.enabled);
        assert_eq!(config.stage1_max_devices, 10);
        assert!((config.min_success_rate - 0.95).abs() < 0.001);
        assert_eq!(config.stage_delay_minutes, 60);
        assert!((config.max_error_rate - 0.05).abs() < 0.001);
        assert_eq!(config.monitoring_window_minutes, 120);
    }
}

mod delta_tests {
    use openracing_firmware_update::delta::{
        apply_simple_patch, compress_data, compute_data_hash, create_simple_patch, decompress_data,
    };

    #[test]
    fn test_compression_roundtrip() -> anyhow::Result<()> {
        let original_data = b"Hello, world! This is test data for compression.";

        let compressed = compress_data(original_data)?;
        let decompressed = decompress_data(&compressed)?;

        assert_eq!(original_data, decompressed.as_slice());
        Ok(())
    }

    #[test]
    fn test_data_hash() {
        let hash = compute_data_hash(b"test data");
        assert_eq!(hash.len(), 64);
        assert_eq!(
            hash,
            "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9"
        );
    }

    #[test]
    fn test_simple_patch() -> anyhow::Result<()> {
        let old_data = b"Hello, world!";
        let new_data = b"Hello, Rust world!";

        let patch = create_simple_patch(old_data, new_data)?;
        let result = apply_simple_patch(old_data, &patch)?;

        assert_eq!(result, new_data);
        Ok(())
    }
}
