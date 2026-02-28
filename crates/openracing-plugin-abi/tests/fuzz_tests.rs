//! Fuzzing tests for ABI type deserialization.
//!
//! These tests verify that ABI types handle edge cases correctly,
//! including NaN, Infinity, and invalid byte patterns.

use openracing_plugin_abi::*;

type TestResult = Result<(), String>;

mod telemetry_frame_fuzz {
    use super::*;

    #[test]
    fn fuzz_nan_values() -> TestResult {
        let frame = TelemetryFrame::with_values(0, f32::NAN, f32::NAN, f32::NAN, 0);

        let bytes = frame.to_bytes();
        let restored = TelemetryFrame::from_bytes(&bytes);

        assert!(restored.wheel_angle_deg.is_nan());
        assert!(restored.wheel_speed_rad_s.is_nan());
        assert!(restored.temperature_c.is_nan());
        Ok(())
    }

    #[test]
    fn fuzz_infinity_values() -> TestResult {
        let frame =
            TelemetryFrame::with_values(0, f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, 0);

        let bytes = frame.to_bytes();
        let restored = TelemetryFrame::from_bytes(&bytes);

        assert!(restored.wheel_angle_deg.is_infinite());
        assert!(restored.wheel_speed_rad_s.is_infinite());
        assert!(restored.temperature_c.is_infinite());
        Ok(())
    }

    #[test]
    fn fuzz_subnormal_values() -> TestResult {
        let subnormal = f32::from_bits(0x00000001);
        let frame = TelemetryFrame::with_values(0, subnormal, -subnormal, subnormal, 0);

        let bytes = frame.to_bytes();
        let restored = TelemetryFrame::from_bytes(&bytes);

        assert_eq!(
            frame.wheel_angle_deg.to_bits(),
            restored.wheel_angle_deg.to_bits()
        );
        assert_eq!(
            frame.wheel_speed_rad_s.to_bits(),
            restored.wheel_speed_rad_s.to_bits()
        );
        assert_eq!(
            frame.temperature_c.to_bits(),
            restored.temperature_c.to_bits()
        );
        Ok(())
    }

    #[test]
    fn fuzz_max_values() -> TestResult {
        let frame = TelemetryFrame::with_values(u64::MAX, f32::MAX, f32::MIN, f32::MAX, u32::MAX);

        let bytes = frame.to_bytes();
        let restored = TelemetryFrame::from_bytes(&bytes);

        assert_eq!(frame.timestamp_us, restored.timestamp_us);
        assert_eq!(frame.fault_flags, restored.fault_flags);
        Ok(())
    }

    #[test]
    fn fuzz_zero_values() -> TestResult {
        let frame = TelemetryFrame::with_values(0, 0.0, 0.0, 0.0, 0);

        let bytes = frame.to_bytes();
        let restored = TelemetryFrame::from_bytes(&bytes);

        assert_eq!(restored.timestamp_us, 0);
        assert_eq!(restored.wheel_angle_deg, 0.0);
        assert_eq!(restored.wheel_speed_rad_s, 0.0);
        assert_eq!(restored.temperature_c, 0.0);
        assert_eq!(restored.fault_flags, 0);
        Ok(())
    }

    #[test]
    fn fuzz_negative_zero() -> TestResult {
        let neg_zero = f32::from_bits(0x80000000);
        let frame = TelemetryFrame::with_values(0, neg_zero, neg_zero, neg_zero, 0);

        let bytes = frame.to_bytes();
        let restored = TelemetryFrame::from_bytes(&bytes);

        assert_eq!(
            frame.wheel_angle_deg.to_bits(),
            restored.wheel_angle_deg.to_bits()
        );
        assert_eq!(
            frame.wheel_speed_rad_s.to_bits(),
            restored.wheel_speed_rad_s.to_bits()
        );
        assert_eq!(
            frame.temperature_c.to_bits(),
            restored.temperature_c.to_bits()
        );
        Ok(())
    }

    #[test]
    fn fuzz_all_byte_patterns() {
        for byte_val in 0u8..=255u8 {
            let bytes = [byte_val; 32];
            let frame = TelemetryFrame::from_bytes(&bytes);

            let restored_bytes = frame.to_bytes();
            let restored_frame = TelemetryFrame::from_bytes(&restored_bytes);

            assert_eq!(frame.timestamp_us, restored_frame.timestamp_us);
            assert_eq!(frame.fault_flags, restored_frame.fault_flags);
        }
    }
}

mod plugin_header_fuzz {
    use super::*;

    #[test]
    fn fuzz_all_zeros() -> TestResult {
        let bytes = [0u8; 16];
        let header = PluginHeader::from_bytes(&bytes);

        assert_eq!(header.magic, 0);
        assert_eq!(header.abi_version, 0);
        assert!(!header.is_valid());
        Ok(())
    }

    #[test]
    fn fuzz_all_ones() -> TestResult {
        let bytes = [0xFFu8; 16];
        let header = PluginHeader::from_bytes(&bytes);

        let restored = PluginHeader::from_bytes(&header.to_bytes());
        assert_eq!(header, restored);
        Ok(())
    }

    #[test]
    fn fuzz_random_bytes() {
        for i in 0u8..=255u8 {
            let bytes = [i; 16];
            let header = PluginHeader::from_bytes(&bytes);

            let restored_bytes = header.to_bytes();
            let restored = PluginHeader::from_bytes(&restored_bytes);

            assert_eq!(header, restored);
        }
    }

    #[test]
    fn fuzz_boundary_capability_bits() -> TestResult {
        let boundary_values = [0u32, 1, 2, 3, 4, 5, 6, 7, u32::MAX];

        for caps in boundary_values {
            let header = PluginHeader {
                magic: PLUG_ABI_MAGIC,
                abi_version: PLUG_ABI_VERSION,
                capabilities: caps,
                reserved: 0,
            };

            let bytes = header.to_bytes();
            let restored = PluginHeader::from_bytes(&bytes);

            assert_eq!(header, restored);
        }
        Ok(())
    }

    #[test]
    fn fuzz_partial_magic() -> TestResult {
        let mut header = PluginHeader::default();

        for i in 0u8..=255u8 {
            header.magic = i as u32;
            let bytes = header.to_bytes();
            let restored = PluginHeader::from_bytes(&bytes);
            assert_eq!(header.magic, restored.magic);
        }
        Ok(())
    }
}

mod capability_fuzz {
    use super::*;

    #[test]
    fn fuzz_all_bits() {
        for bits in 0u32..=0xFF {
            let caps = PluginCapabilities::from_bits_truncate(bits);

            let combined = caps.bits();
            let restored = PluginCapabilities::from_bits_truncate(combined);

            assert_eq!(caps, restored);
        }
    }

    #[test]
    fn fuzz_reserved_bits_stripped() {
        for bits in 0u32..=0xFF {
            let caps = PluginCapabilities::from_bits_truncate(bits);
            let valid_bits = PluginCapabilities::all().bits();
            let valid_mask = PluginCapabilities::TELEMETRY
                | PluginCapabilities::LEDS
                | PluginCapabilities::HAPTICS;

            if bits <= valid_mask.bits() {
                assert_eq!(caps.bits(), bits);
            }
            assert!(caps.bits() <= valid_bits);
        }
    }
}

mod byte_pattern_fuzz {
    use super::*;

    #[test]
    fn fuzz_telemetry_alternating_bytes() {
        for pattern in 0u8..=255u8 {
            let mut bytes = [0u8; 32];
            for (i, b) in bytes.iter_mut().enumerate() {
                *b = if i % 2 == 0 { pattern } else { !pattern };
            }

            let frame = TelemetryFrame::from_bytes(&bytes);
            let restored = TelemetryFrame::from_bytes(&frame.to_bytes());

            assert_eq!(frame.timestamp_us, restored.timestamp_us);
            assert_eq!(frame.fault_flags, restored.fault_flags);
        }
    }

    #[test]
    fn fuzz_header_alternating_bytes() {
        for pattern in 0u8..=255u8 {
            let mut bytes = [0u8; 16];
            for (i, b) in bytes.iter_mut().enumerate() {
                *b = if i % 2 == 0 { pattern } else { !pattern };
            }

            let header = PluginHeader::from_bytes(&bytes);
            let restored = PluginHeader::from_bytes(&header.to_bytes());

            assert_eq!(header, restored);
        }
    }

    #[test]
    fn fuzz_telemetry_counting_bytes() {
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = i as u8;
        }

        let frame = TelemetryFrame::from_bytes(&bytes);
        let restored = TelemetryFrame::from_bytes(&frame.to_bytes());

        assert_eq!(frame.timestamp_us, restored.timestamp_us);
        assert_eq!(frame.fault_flags, restored.fault_flags);
    }

    #[test]
    fn fuzz_header_counting_bytes() {
        let mut bytes = [0u8; 16];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = i as u8;
        }

        let header = PluginHeader::from_bytes(&bytes);
        let restored = PluginHeader::from_bytes(&header.to_bytes());

        assert_eq!(header, restored);
    }
}

#[cfg(feature = "serde")]
mod serde_fuzz {
    use super::*;

    #[test]
    fn fuzz_telemetry_serde_edge_cases() {
        let edge_cases = [
            TelemetryFrame::with_values(0, 0.0, 0.0, 0.0, 0),
            TelemetryFrame::with_values(u64::MAX, f32::MAX, f32::MIN, f32::MAX, u32::MAX),
            TelemetryFrame::with_values(12345, 90.0, 1.57, 45.5, 0xFF),
            TelemetryFrame::with_values(0, -1800.0, 0.0, 20.0, 0),
        ];

        for frame in edge_cases {
            let json = serde_json::to_string(&frame).unwrap();
            let restored: TelemetryFrame = serde_json::from_str(&json).unwrap();

            assert_eq!(frame.timestamp_us, restored.timestamp_us);
            assert_eq!(frame.fault_flags, restored.fault_flags);
        }
    }

    #[test]
    fn fuzz_telemetry_serde_roundtrip_finite() {
        for temp in [20.0, 45.0, 80.0, f32::MAX, f32::MIN] {
            for angle in [-1800.0, -90.0, 0.0, 90.0, 1800.0] {
                let frame = TelemetryFrame::with_values(12345, angle, 1.57, temp, 0xFF);
                let json = serde_json::to_string(&frame).unwrap();
                let restored: TelemetryFrame = serde_json::from_str(&json).unwrap();

                assert_eq!(frame.timestamp_us, restored.timestamp_us);
            }
        }
    }
}
