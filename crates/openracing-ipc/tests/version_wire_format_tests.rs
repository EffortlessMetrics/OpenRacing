//! Wire format stability tests for IPC version negotiation.
//!
//! These tests ensure the binary wire format of version negotiation types
//! is stable across builds and versions. Any format change must be intentional.

use openracing_ipc::version::{FeatureFlags, ProtocolVersion, VersionInfo};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// =========================================================================
// Snapshot tests: known-good byte encodings
// =========================================================================

#[test]
fn snapshot_protocol_version_v1_0_0_bytes() -> Result<(), BoxErr> {
    let v = ProtocolVersion::new(1, 0, 0);
    let bytes = v.to_bytes();
    // v1.0.0 = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00] little-endian
    let expected: [u8; 6] = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(bytes, expected, "Wire format for v1.0.0 must not change");
    Ok(())
}

#[test]
fn snapshot_protocol_version_v1_2_3_bytes() -> Result<(), BoxErr> {
    let v = ProtocolVersion::new(1, 2, 3);
    let bytes = v.to_bytes();
    let expected: [u8; 6] = [0x01, 0x00, 0x02, 0x00, 0x03, 0x00];
    assert_eq!(bytes, expected, "Wire format for v1.2.3 must not change");
    Ok(())
}

#[test]
fn snapshot_feature_flags_device_management_bytes() -> Result<(), BoxErr> {
    let flags = FeatureFlags::DEVICE_MANAGEMENT;
    let bytes = flags.to_bytes();
    let expected: [u8; 8] = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(
        bytes, expected,
        "Wire format for DEVICE_MANAGEMENT flag must not change"
    );
    Ok(())
}

#[test]
fn snapshot_feature_flags_all_v1_bytes() -> Result<(), BoxErr> {
    let flags = FeatureFlags::ALL_V1;
    let bytes = flags.to_bytes();
    let expected: [u8; 8] = [0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(
        bytes, expected,
        "Wire format for ALL_V1 flags must not change"
    );
    Ok(())
}

#[test]
fn snapshot_feature_flags_none_bytes() -> Result<(), BoxErr> {
    let flags = FeatureFlags::NONE;
    let bytes = flags.to_bytes();
    let expected: [u8; 8] = [0x00; 8];
    assert_eq!(
        bytes, expected,
        "Wire format for NONE flags must not change"
    );
    Ok(())
}

#[test]
fn snapshot_version_info_bytes() -> Result<(), BoxErr> {
    let info = VersionInfo::new(
        ProtocolVersion::new(1, 0, 0),
        FeatureFlags::ALL_V1,
        ProtocolVersion::new(1, 0, 0),
    );
    let bytes = info.to_bytes();
    assert_eq!(bytes.len(), VersionInfo::SIZE);
    #[rustfmt::skip]
    let expected: [u8; 20] = [
        // version: 1.0.0
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
        // features: ALL_V1 (0xFF)
        0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        // min_version: 1.0.0
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    assert_eq!(
        bytes, expected,
        "Wire format for VersionInfo must not change"
    );
    Ok(())
}

// =========================================================================
// Roundtrip tests: encode → decode → compare
// =========================================================================

#[test]
fn roundtrip_protocol_version() -> Result<(), BoxErr> {
    let versions = [
        ProtocolVersion::new(0, 0, 0),
        ProtocolVersion::new(1, 0, 0),
        ProtocolVersion::new(1, 2, 3),
        ProtocolVersion::new(255, 255, 255),
        ProtocolVersion::new(u16::MAX, u16::MAX, u16::MAX),
    ];
    for v in &versions {
        let bytes = v.to_bytes();
        let decoded = ProtocolVersion::from_bytes(&bytes)?;
        assert_eq!(*v, decoded, "Roundtrip failed for {v}");
    }
    Ok(())
}

#[test]
fn roundtrip_feature_flags() -> Result<(), BoxErr> {
    let flag_sets = [
        FeatureFlags::NONE,
        FeatureFlags::DEVICE_MANAGEMENT,
        FeatureFlags::ALL_V1,
        FeatureFlags::from_bits(u64::MAX),
        FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::TELEMETRY,
    ];
    for flags in &flag_sets {
        let bytes = flags.to_bytes();
        let decoded = FeatureFlags::from_bytes(&bytes)?;
        assert_eq!(
            *flags,
            decoded,
            "Roundtrip failed for flags 0x{:x}",
            flags.bits()
        );
    }
    Ok(())
}

#[test]
fn roundtrip_version_info() -> Result<(), BoxErr> {
    let infos = [
        VersionInfo::new(
            ProtocolVersion::new(1, 0, 0),
            FeatureFlags::NONE,
            ProtocolVersion::new(1, 0, 0),
        ),
        VersionInfo::new(
            ProtocolVersion::new(1, 5, 2),
            FeatureFlags::ALL_V1,
            ProtocolVersion::new(1, 0, 0),
        ),
        VersionInfo::new(
            ProtocolVersion::new(u16::MAX, u16::MAX, u16::MAX),
            FeatureFlags::from_bits(u64::MAX),
            ProtocolVersion::new(0, 0, 0),
        ),
    ];
    for info in &infos {
        let bytes = info.to_bytes();
        let decoded = VersionInfo::from_bytes(&bytes)?;
        assert_eq!(*info, decoded, "Roundtrip failed for {:?}", info);
    }
    Ok(())
}

// =========================================================================
// Version bump detection: sizes must remain stable
// =========================================================================

#[test]
fn version_wire_sizes_are_stable() -> Result<(), BoxErr> {
    assert_eq!(
        ProtocolVersion::SIZE,
        6,
        "ProtocolVersion wire size changed"
    );
    assert_eq!(FeatureFlags::SIZE, 8, "FeatureFlags wire size changed");
    assert_eq!(VersionInfo::SIZE, 20, "VersionInfo wire size changed");
    Ok(())
}

#[test]
fn feature_flag_bit_positions_are_stable() -> Result<(), BoxErr> {
    assert_eq!(FeatureFlags::DEVICE_MANAGEMENT.bits(), 1 << 0);
    assert_eq!(FeatureFlags::PROFILE_MANAGEMENT.bits(), 1 << 1);
    assert_eq!(FeatureFlags::SAFETY_CONTROL.bits(), 1 << 2);
    assert_eq!(FeatureFlags::HEALTH_MONITORING.bits(), 1 << 3);
    assert_eq!(FeatureFlags::GAME_INTEGRATION.bits(), 1 << 4);
    assert_eq!(FeatureFlags::STREAMING_HEALTH.bits(), 1 << 5);
    assert_eq!(FeatureFlags::STREAMING_DEVICES.bits(), 1 << 6);
    assert_eq!(FeatureFlags::TELEMETRY.bits(), 1 << 7);
    Ok(())
}

// =========================================================================
// Cross-version compatibility: v1.0 messages readable by v1.1 parser
// =========================================================================

#[test]
fn cross_version_v1_0_message_readable_by_v1_1() -> Result<(), BoxErr> {
    // Simulate a v1.0 client sending a VersionInfo
    let v1_0_info = VersionInfo::new(
        ProtocolVersion::new(1, 0, 0),
        FeatureFlags::DEVICE_MANAGEMENT,
        ProtocolVersion::new(1, 0, 0),
    );
    let wire_bytes = v1_0_info.to_bytes();

    // A v1.1 parser reads the same bytes
    let parsed = VersionInfo::from_bytes(&wire_bytes)?;
    assert_eq!(parsed.version, ProtocolVersion::new(1, 0, 0));
    assert!(parsed.features.contains(FeatureFlags::DEVICE_MANAGEMENT));
    Ok(())
}

#[test]
fn cross_version_v1_1_message_readable_by_v1_0() -> Result<(), BoxErr> {
    // v1.1 client with new features
    let v1_1_info = VersionInfo::new(
        ProtocolVersion::new(1, 1, 0),
        FeatureFlags::ALL_V1,
        ProtocolVersion::new(1, 0, 0),
    );
    let wire_bytes = v1_1_info.to_bytes();

    // v1.0 parser can still decode the bytes
    let parsed = VersionInfo::from_bytes(&wire_bytes)?;
    assert_eq!(parsed.version, ProtocolVersion::new(1, 1, 0));
    // v1.0 parser sees all bits, even ones it doesn't recognize yet
    assert_eq!(parsed.features.bits(), FeatureFlags::ALL_V1.bits());
    Ok(())
}

#[test]
fn cross_version_unknown_feature_flags_preserved() -> Result<(), BoxErr> {
    // Future version uses bits beyond the v1 range
    let future_flags = FeatureFlags::from_bits(0xFF | (1 << 32));
    let info = VersionInfo::new(
        ProtocolVersion::new(1, 5, 0),
        future_flags,
        ProtocolVersion::new(1, 0, 0),
    );
    let wire_bytes = info.to_bytes();
    let parsed = VersionInfo::from_bytes(&wire_bytes)?;

    // Known v1 flags are still accessible
    assert!(parsed.features.contains(FeatureFlags::ALL_V1));
    // Unknown bits are preserved in the raw value
    assert_eq!(parsed.features.bits(), future_flags.bits());
    Ok(())
}

#[test]
fn extra_trailing_bytes_do_not_break_decode() -> Result<(), BoxErr> {
    let info = VersionInfo::new(
        ProtocolVersion::new(1, 0, 0),
        FeatureFlags::DEVICE_MANAGEMENT,
        ProtocolVersion::new(1, 0, 0),
    );
    let mut wire_bytes = info.to_bytes().to_vec();
    // Append extra bytes (as a future extension might)
    wire_bytes.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);

    let parsed = VersionInfo::from_bytes(&wire_bytes)?;
    assert_eq!(parsed, info);
    Ok(())
}

// =========================================================================
// Insta snapshot tests for JSON serialization
// =========================================================================

#[test]
fn snapshot_version_info_json() -> Result<(), BoxErr> {
    let info = VersionInfo::new(
        ProtocolVersion::new(1, 0, 0),
        FeatureFlags::ALL_V1,
        ProtocolVersion::new(1, 0, 0),
    );
    let json = serde_json::to_string_pretty(&info)?;
    insta::assert_snapshot!("version_info_v1_json", json);
    Ok(())
}

#[test]
fn snapshot_protocol_version_json() -> Result<(), BoxErr> {
    let v = ProtocolVersion::new(1, 2, 3);
    let json = serde_json::to_string_pretty(&v)?;
    insta::assert_snapshot!("protocol_version_1_2_3_json", json);
    Ok(())
}

#[test]
fn snapshot_feature_flags_json() -> Result<(), BoxErr> {
    let flags = FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::SAFETY_CONTROL;
    let json = serde_json::to_string_pretty(&flags)?;
    insta::assert_snapshot!("feature_flags_device_safety_json", json);
    Ok(())
}
