//! Property-based tests for ABI types.
//!
//! These tests verify serialization roundtrips and invariants using
//! property-based testing with proptest.

use openracing_plugin_abi::*;
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_plugin_header_roundtrip(
        magic: u32,
        version: u32,
        caps: u32,
        reserved: u32
    ) {
        let header = PluginHeader {
            magic,
            abi_version: version,
            capabilities: caps,
            reserved,
        };

        let bytes = header.to_bytes();
        let restored = PluginHeader::from_bytes(&bytes);

        prop_assert_eq!(header, restored);
    }

    #[test]
    fn test_telemetry_frame_roundtrip(
        timestamp: u64,
        angle: f32,
        speed: f32,
        temp: f32,
        faults: u32,
        pad: u32
    ) {
        let frame = TelemetryFrame {
            timestamp_us: timestamp,
            wheel_angle_deg: angle,
            wheel_speed_rad_s: speed,
            temperature_c: temp,
            fault_flags: faults,
            _pad: pad,
        };

        let bytes = frame.to_bytes();
        let restored = TelemetryFrame::from_bytes(&bytes);

        prop_assert_eq!(frame.timestamp_us, restored.timestamp_us);
        prop_assert_eq!(frame.wheel_angle_deg.to_bits(), restored.wheel_angle_deg.to_bits());
        prop_assert_eq!(frame.wheel_speed_rad_s.to_bits(), restored.wheel_speed_rad_s.to_bits());
        prop_assert_eq!(frame.temperature_c.to_bits(), restored.temperature_c.to_bits());
        prop_assert_eq!(frame.fault_flags, restored.fault_flags);
    }

    #[test]
    fn test_capability_bits_valid(caps_bits in 0u32..=0xFF) {
        let caps = PluginCapabilities::from_bits_truncate(caps_bits);

        prop_assert!(caps.bits() <= PluginCapabilities::all().bits());
    }

    #[test]
    fn test_capability_union(a in 0u32..7, b in 0u32..7) {
        let caps_a = PluginCapabilities::from_bits_truncate(a);
        let caps_b = PluginCapabilities::from_bits_truncate(b);
        let union = caps_a | caps_b;

        prop_assert!(union.contains(caps_a));
        prop_assert!(union.contains(caps_b));
    }

    #[test]
    fn test_capability_intersection(a in 0u32..7, b in 0u32..7) {
        let caps_a = PluginCapabilities::from_bits_truncate(a);
        let caps_b = PluginCapabilities::from_bits_truncate(b);
        let intersection = caps_a & caps_b;

        prop_assert!(caps_a.contains(intersection));
        prop_assert!(caps_b.contains(intersection));
    }

    #[test]
    fn test_header_capability_preserved(caps_bits in 0u32..7) {
        let caps = PluginCapabilities::from_bits_truncate(caps_bits);
        let header = PluginHeader::new(caps);

        prop_assert_eq!(header.get_capabilities(), caps);
    }

    #[test]
    fn test_telemetry_byte_array_size(_timestamp: u64) {
        let frame = TelemetryFrame::default();
        let bytes = frame.to_bytes();

        prop_assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn test_header_byte_array_size(_magic: u32) {
        let header = PluginHeader::default();
        let bytes = header.to_bytes();

        prop_assert_eq!(bytes.len(), 16);
    }
}

mod telemetry_property_tests {
    use super::*;

    proptest! {
        #[test]
        fn test_temperature_range(temp in -100.0f32..200.0f32) {
            let frame = TelemetryFrame {
                temperature_c: temp,
                ..Default::default()
            };

            let is_normal = frame.is_temperature_normal();
            prop_assert_eq!(is_normal, (20.0..=80.0).contains(&temp));
        }

        #[test]
        fn test_angle_range(angle in -2000.0f32..2000.0f32) {
            let frame = TelemetryFrame {
                wheel_angle_deg: angle,
                ..Default::default()
            };

            let is_valid = frame.is_angle_valid();
            prop_assert_eq!(is_valid, (-1800.0..=1800.0).contains(&angle));
        }

        #[test]
        fn test_fault_flags(flags: u32) {
            let frame = TelemetryFrame {
                fault_flags: flags,
                ..Default::default()
            };

            prop_assert_eq!(frame.has_faults(), flags != 0);
        }
    }
}

mod header_property_tests {
    use super::*;

    proptest! {
        #[test]
        fn test_header_validation(magic: u32, version: u32) {
            let header = PluginHeader {
                magic,
                abi_version: version,
                ..Default::default()
            };

            let is_valid = header.is_valid();

            if magic == PLUG_ABI_MAGIC && version == PLUG_ABI_VERSION {
                prop_assert!(is_valid);
            } else {
                prop_assert!(!is_valid);
            }
        }

        #[test]
        fn test_capability_has_capability_consistent(caps_bits in 0u32..7) {
            let caps = PluginCapabilities::from_bits_truncate(caps_bits);
            let header = PluginHeader::new(caps);

            for flag in [PluginCapabilities::TELEMETRY, PluginCapabilities::LEDS, PluginCapabilities::HAPTICS] {
                prop_assert_eq!(header.has_capability(flag), caps.contains(flag));
            }
        }
    }
}

#[cfg(feature = "serde")]
mod serde_property_tests {
    use super::*;

    proptest! {
        #[test]
        fn test_telemetry_frame_serde_roundtrip(
            timestamp: u64,
            angle: f32,
            speed: f32,
            temp: f32,
            faults: u32
        ) {
            let frame = TelemetryFrame {
                timestamp_us: timestamp,
                wheel_angle_deg: angle,
                wheel_speed_rad_s: speed,
                temperature_c: temp,
                fault_flags: faults,
                _pad: 0,
            };

            let json = serde_json::to_string(&frame).unwrap();
            let restored: TelemetryFrame = serde_json::from_str(&json).unwrap();

            prop_assert_eq!(frame.timestamp_us, restored.timestamp_us);
            prop_assert_eq!(frame.wheel_angle_deg.to_bits(), restored.wheel_angle_deg.to_bits());
            prop_assert_eq!(frame.wheel_speed_rad_s.to_bits(), restored.wheel_speed_rad_s.to_bits());
            prop_assert_eq!(frame.temperature_c.to_bits(), restored.temperature_c.to_bits());
            prop_assert_eq!(frame.fault_flags, restored.fault_flags);
        }

        #[test]
        fn test_plugin_init_status_serde_roundtrip(status_code in 0u8..5) {
            let status = match status_code {
                0 => PluginInitStatus::Uninitialized,
                1 => PluginInitStatus::Initializing,
                2 => PluginInitStatus::Initialized,
                3 => PluginInitStatus::Failed,
                _ => PluginInitStatus::ShutDown,
            };

            let json = serde_json::to_string(&status).unwrap();
            let restored: PluginInitStatus = serde_json::from_str(&json).unwrap();

            prop_assert_eq!(status, restored);
        }
    }
}
