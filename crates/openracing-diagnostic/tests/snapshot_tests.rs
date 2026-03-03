//! Snapshot tests for diagnostic types and error formatting.
//!
//! These tests verify that diagnostic data structures, error messages,
//! and format constants are formatted consistently and remain stable.

use openracing_diagnostic::{
    DiagnosticError, FrameData, HealthEventData, IndexEntry, SafetyStateSimple, StreamType,
    TelemetryData, WbbFooter, WbbHeader,
};

mod error_message_snapshots {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn recording_error() {
        assert_snapshot!(
            DiagnosticError::Recording("buffer overflow in stream A".into()).to_string()
        );
    }

    #[test]
    fn replay_error() {
        assert_snapshot!(DiagnosticError::Replay("frame index out of bounds".into()).to_string());
    }

    #[test]
    fn format_error() {
        assert_snapshot!(DiagnosticError::Format("unexpected stream tag 0xFF".into()).to_string());
    }

    #[test]
    fn io_error() {
        assert_snapshot!(
            DiagnosticError::Io("permission denied: /var/recordings".into()).to_string()
        );
    }

    #[test]
    fn serialization_error() {
        assert_snapshot!(
            DiagnosticError::Serialization("failed to encode FrameData".into()).to_string()
        );
    }

    #[test]
    fn deserialization_error() {
        assert_snapshot!(
            DiagnosticError::Deserialization("invalid StreamARecord length".into()).to_string()
        );
    }

    #[test]
    fn compression_error() {
        assert_snapshot!(
            DiagnosticError::Compression("gzip decompression failed".into()).to_string()
        );
    }

    #[test]
    fn size_limit_error() {
        assert_snapshot!(
            DiagnosticError::SizeLimit("bundle exceeds 25 MB limit".into()).to_string()
        );
    }

    #[test]
    fn configuration_error() {
        assert_snapshot!(
            DiagnosticError::Configuration("compression level 15 is invalid".into()).to_string()
        );
    }

    #[test]
    fn validation_error() {
        assert_snapshot!(
            DiagnosticError::Validation("frame count mismatch in footer".into()).to_string()
        );
    }

    #[test]
    fn crc_mismatch_error() {
        assert_snapshot!(
            DiagnosticError::CrcMismatch {
                expected: 0xDEADBEEF,
                actual: 0xCAFEBABE,
            }
            .to_string()
        );
    }

    #[test]
    fn invalid_magic_error() {
        assert_snapshot!(
            DiagnosticError::InvalidMagic {
                expected: *b"WBB1",
                actual: *b"XXXX",
            }
            .to_string()
        );
    }

    #[test]
    fn unsupported_version_error() {
        assert_snapshot!(DiagnosticError::UnsupportedVersion(99).to_string());
    }
}

mod frame_data_snapshots {
    use super::*;
    use insta::assert_debug_snapshot;

    #[test]
    fn default_frame() {
        assert_debug_snapshot!(FrameData::default());
    }

    #[test]
    fn typical_frame() {
        let frame = FrameData {
            ffb_in: 0.75,
            torque_out: -0.42,
            wheel_speed: 12.5,
            hands_off: false,
            ts_mono_ns: 1_000_000_000,
            seq: 42,
        };
        assert_debug_snapshot!(frame);
    }

    #[test]
    fn hands_off_frame() {
        let frame = FrameData {
            ffb_in: 0.0,
            torque_out: 0.0,
            wheel_speed: 0.0,
            hands_off: true,
            ts_mono_ns: 5_000_000_000,
            seq: 100,
        };
        assert_debug_snapshot!(frame);
    }
}

mod safety_state_snapshots {
    use super::*;
    use insta::assert_debug_snapshot;

    #[test]
    fn safe_torque() {
        assert_debug_snapshot!(SafetyStateSimple::SafeTorque);
    }

    #[test]
    fn high_torque_challenge() {
        assert_debug_snapshot!(SafetyStateSimple::HighTorqueChallenge);
    }

    #[test]
    fn awaiting_physical_ack() {
        assert_debug_snapshot!(SafetyStateSimple::AwaitingPhysicalAck);
    }

    #[test]
    fn high_torque_active() {
        assert_debug_snapshot!(SafetyStateSimple::HighTorqueActive);
    }

    #[test]
    fn faulted() {
        assert_debug_snapshot!(SafetyStateSimple::Faulted {
            fault_type: "watchdog_timeout".into(),
        });
    }
}

mod telemetry_data_snapshots {
    use super::*;
    use insta::assert_debug_snapshot;

    #[test]
    fn default_telemetry() {
        assert_debug_snapshot!(TelemetryData::default());
    }

    #[test]
    fn race_telemetry() {
        let telemetry = TelemetryData {
            ffb_scalar: 0.85,
            rpm: 7200.0,
            speed_ms: 55.0,
            slip_ratio: 0.05,
            gear: 4,
            car_id: Some("porsche_911_gt3".into()),
            track_id: Some("nurburgring_gp".into()),
        };
        assert_debug_snapshot!(telemetry);
    }

    #[test]
    fn idle_telemetry() {
        let telemetry = TelemetryData {
            ffb_scalar: 1.0,
            rpm: 800.0,
            speed_ms: 0.0,
            slip_ratio: 0.0,
            gear: 0,
            car_id: None,
            track_id: None,
        };
        assert_debug_snapshot!(telemetry);
    }
}

mod health_event_snapshots {
    use super::*;
    use insta::assert_debug_snapshot;

    #[test]
    fn device_connected_event() {
        let event = HealthEventData {
            timestamp_ns: 1_700_000_000_000_000_000,
            device_id: "fanatec-csl-dd".into(),
            event_type: "DeviceConnected".into(),
            context: serde_json::json!({
                "vid": "0EB7",
                "pid": "0001",
                "firmware": "1.34"
            }),
        };
        assert_debug_snapshot!(event);
    }

    #[test]
    fn fault_event() {
        let event = HealthEventData {
            timestamp_ns: 1_700_000_001_000_000_000,
            device_id: "moza-r9".into(),
            event_type: "FaultDetected".into(),
            context: serde_json::json!({
                "fault": "watchdog_timeout",
                "duration_ms": 15
            }),
        };
        assert_debug_snapshot!(event);
    }
}

mod format_snapshots {
    use super::*;
    use insta::{assert_debug_snapshot, assert_snapshot};

    #[test]
    fn wbb_header_debug() {
        let header = WbbHeader::new("test-device-001", 1, 0x07, 6);
        // Redact time-dependent fields for deterministic snapshots
        let output = format!(
            "WbbHeader {{ magic: {:?}, version: {}, device_id: {:?}, \
             ffb_mode: {}, stream_flags: 0x{:02X}, compression_level: {}, \
             has_stream_a: {}, has_stream_b: {}, has_stream_c: {} }}",
            header.magic,
            header.version,
            header.device_id,
            header.ffb_mode,
            header.stream_flags,
            header.compression_level,
            header.has_stream_a(),
            header.has_stream_b(),
            header.has_stream_c(),
        );
        assert_snapshot!(output);
    }

    #[test]
    fn wbb_header_validation_bad_magic() {
        let mut header = WbbHeader::new("test", 1, 0x07, 6);
        header.magic = *b"XXXX";
        let err = header.validate().unwrap_err();
        assert_snapshot!(err.to_string());
    }

    #[test]
    fn wbb_header_validation_bad_version() {
        let mut header = WbbHeader::new("test", 1, 0x07, 6);
        header.version = 99;
        let err = header.validate().unwrap_err();
        assert_snapshot!(err.to_string());
    }

    #[test]
    fn wbb_header_validation_bad_compression() {
        let header = WbbHeader::new("test", 1, 0x07, 10);
        let err = header.validate().unwrap_err();
        assert_snapshot!(err.to_string());
    }

    #[test]
    fn wbb_footer_debug() {
        let footer = WbbFooter::new(5000, 5000);
        assert_debug_snapshot!(footer);
    }

    #[test]
    fn wbb_footer_validation_bad_magic() {
        let mut footer = WbbFooter::new(1000, 1000);
        footer.footer_magic = *b"ZZZZ";
        let err = footer.validate().unwrap_err();
        assert_snapshot!(err.to_string());
    }

    #[test]
    fn index_entry_debug() {
        let entry = IndexEntry::new(500, 100);
        assert_debug_snapshot!(entry);
    }

    #[test]
    fn stream_type_flags() {
        let output = format!(
            "A=0x{:02X} B=0x{:02X} C=0x{:02X}",
            StreamType::A.flag(),
            StreamType::B.flag(),
            StreamType::C.flag(),
        );
        assert_snapshot!(output);
    }
}

mod config_snapshots {
    use insta::assert_snapshot;
    use openracing_diagnostic::BlackboxConfig;

    #[test]
    fn default_config() {
        let config = BlackboxConfig::new("my-device", "/tmp/recordings");
        let output = format!(
            "BlackboxConfig {{ device_id: {:?}, max_duration_s: {}, \
             max_file_size_bytes: {}, compression_level: {}, \
             stream_a: {}, stream_b: {}, stream_c: {}, \
             stream_flags: 0x{:02X} }}",
            config.device_id,
            config.max_duration_s,
            config.max_file_size_bytes,
            config.compression_level,
            config.enable_stream_a,
            config.enable_stream_b,
            config.enable_stream_c,
            config.stream_flags(),
        );
        assert_snapshot!(output);
    }

    #[test]
    fn stream_flags_partial() {
        let mut config = BlackboxConfig::new("test", "/tmp");
        config.enable_stream_b = false;
        let output = format!(
            "stream_a={} stream_b={} stream_c={} flags=0x{:02X}",
            config.enable_stream_a,
            config.enable_stream_b,
            config.enable_stream_c,
            config.stream_flags(),
        );
        assert_snapshot!(output);
    }
}

mod replay_config_snapshots {
    use insta::assert_debug_snapshot;
    use openracing_diagnostic::ReplayConfig;

    #[test]
    fn default_replay_config() {
        assert_debug_snapshot!(ReplayConfig::default());
    }

    #[test]
    fn custom_replay_config() {
        let config = ReplayConfig {
            deterministic_seed: 0xDEAD_BEEF,
            fp_tolerance: 1e-3,
            strict_timing: true,
            max_duration_s: 120,
            validate_outputs: false,
        };
        assert_debug_snapshot!(config);
    }
}
