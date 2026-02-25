//! Unit tests for ABI types.
//!
//! These tests verify the layout, size, and alignment of ABI types
//! to ensure ABI stability across platforms.

use openracing_plugin_abi::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

mod size_and_alignment {
    use super::*;

    #[test]
    fn test_plugin_header_size() {
        assert_eq!(std::mem::size_of::<PluginHeader>(), 16);
    }

    #[test]
    fn test_plugin_header_alignment() {
        assert_eq!(std::mem::align_of::<PluginHeader>(), 4);
    }

    #[test]
    fn test_telemetry_frame_size() {
        assert_eq!(std::mem::size_of::<TelemetryFrame>(), 32);
    }

    #[test]
    fn test_telemetry_frame_alignment() {
        assert_eq!(std::mem::align_of::<TelemetryFrame>(), 8);
    }

    #[test]
    fn test_plugin_capabilities_size() {
        assert_eq!(std::mem::size_of::<PluginCapabilities>(), 4);
    }

    #[test]
    fn test_plugin_header_field_offsets() {
        let header = PluginHeader::default();
        let base = &header as *const _ as usize;

        let magic_offset = &header.magic as *const _ as usize - base;
        let version_offset = &header.abi_version as *const _ as usize - base;
        let caps_offset = &header.capabilities as *const _ as usize - base;
        let reserved_offset = &header.reserved as *const _ as usize - base;

        assert_eq!(magic_offset, 0);
        assert_eq!(version_offset, 4);
        assert_eq!(caps_offset, 8);
        assert_eq!(reserved_offset, 12);
    }

    #[test]
    fn test_telemetry_frame_field_offsets() {
        let frame = TelemetryFrame::default();
        let base = &frame as *const _ as usize;

        let ts_offset = &frame.timestamp_us as *const _ as usize - base;
        let angle_offset = &frame.wheel_angle_deg as *const _ as usize - base;
        let speed_offset = &frame.wheel_speed_rad_s as *const _ as usize - base;
        let temp_offset = &frame.temperature_c as *const _ as usize - base;
        let faults_offset = &frame.fault_flags as *const _ as usize - base;
        let pad_offset = &frame._pad as *const _ as usize - base;

        assert_eq!(ts_offset, 0);
        assert_eq!(angle_offset, 8);
        assert_eq!(speed_offset, 12);
        assert_eq!(temp_offset, 16);
        assert_eq!(faults_offset, 20);
        assert_eq!(pad_offset, 24);
    }
}

mod plugin_header {
    use super::*;

    #[test]
    fn test_default_header() {
        let header = PluginHeader::default();

        assert_eq!(header.magic, PLUG_ABI_MAGIC);
        assert_eq!(header.abi_version, PLUG_ABI_VERSION);
        assert_eq!(header.capabilities, 0);
        assert_eq!(header.reserved, 0);
    }

    #[test]
    fn test_new_header_with_capabilities() {
        let caps =
            PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
        let header = PluginHeader::new(caps);

        assert!(header.is_valid());
        assert_eq!(header.get_capabilities(), caps);
    }

    #[test]
    fn test_invalid_magic() {
        let header = PluginHeader {
            magic: 0xDEADBEEF,
            ..Default::default()
        };

        assert!(!header.is_valid());
    }

    #[test]
    fn test_invalid_version() {
        let header = PluginHeader {
            abi_version: 0x0002_0000,
            ..Default::default()
        };

        assert!(!header.is_valid());
    }

    #[test]
    fn test_byte_roundtrip() -> TestResult {
        let original =
            PluginHeader::new(PluginCapabilities::TELEMETRY | PluginCapabilities::HAPTICS);

        let bytes = original.to_bytes();
        let restored = PluginHeader::from_bytes(&bytes);

        assert_eq!(original, restored);
        Ok(())
    }

    #[test]
    fn test_capability_query() {
        let header = PluginHeader::new(PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS);

        assert!(header.has_capability(PluginCapabilities::TELEMETRY));
        assert!(header.has_capability(PluginCapabilities::LEDS));
        assert!(!header.has_capability(PluginCapabilities::HAPTICS));
    }
}

mod telemetry_frame {
    use super::*;

    #[test]
    fn test_default_frame() {
        let frame = TelemetryFrame::default();

        assert_eq!(frame.timestamp_us, 0);
        assert_eq!(frame.wheel_angle_deg, 0.0);
        assert_eq!(frame.wheel_speed_rad_s, 0.0);
        assert_eq!(frame.temperature_c, 20.0);
        assert_eq!(frame.fault_flags, 0);
        assert_eq!(frame._pad, 0);
    }

    #[test]
    fn test_new_frame_with_timestamp() {
        let frame = TelemetryFrame::new(12345678);

        assert_eq!(frame.timestamp_us, 12345678);
        assert_eq!(frame.temperature_c, 20.0);
    }

    #[test]
    fn test_frame_with_all_values() {
        let frame = TelemetryFrame::with_values(1000000, 90.0, 1.57, 45.5, 0x0F);

        assert_eq!(frame.timestamp_us, 1000000);
        assert_eq!(frame.wheel_angle_deg, 90.0);
        assert_eq!(frame.wheel_speed_rad_s, 1.57);
        assert_eq!(frame.temperature_c, 45.5);
        assert_eq!(frame.fault_flags, 0x0F);
    }

    #[test]
    fn test_byte_roundtrip() {
        let original = TelemetryFrame::with_values(
            0x123456789ABCDEF0,
            -180.0,
            std::f32::consts::PI,
            75.5,
            0xFF00FF00,
        );

        let bytes = original.to_bytes();
        let restored = TelemetryFrame::from_bytes(&bytes);

        assert_eq!(original.timestamp_us, restored.timestamp_us);
        assert_eq!(original.wheel_angle_deg, restored.wheel_angle_deg);
        assert_eq!(original.wheel_speed_rad_s, restored.wheel_speed_rad_s);
        assert_eq!(original.temperature_c, restored.temperature_c);
        assert_eq!(original.fault_flags, restored.fault_flags);
    }

    #[test]
    fn test_temperature_validation() {
        let normal = TelemetryFrame::with_values(0, 0.0, 0.0, 45.0, 0);
        assert!(normal.is_temperature_normal());

        let cold = TelemetryFrame::with_values(0, 0.0, 0.0, 15.0, 0);
        assert!(!cold.is_temperature_normal());

        let hot = TelemetryFrame::with_values(0, 0.0, 0.0, 85.0, 0);
        assert!(!hot.is_temperature_normal());
    }

    #[test]
    fn test_angle_validation() {
        let normal = TelemetryFrame::with_values(0, 450.0, 0.0, 20.0, 0);
        assert!(normal.is_angle_valid());

        let extreme = TelemetryFrame::with_values(0, 2000.0, 0.0, 20.0, 0);
        assert!(!extreme.is_angle_valid());
    }

    #[test]
    fn test_fault_detection() {
        let no_faults = TelemetryFrame::with_values(0, 0.0, 0.0, 20.0, 0);
        assert!(!no_faults.has_faults());

        let with_faults = TelemetryFrame::with_values(0, 0.0, 0.0, 20.0, 1);
        assert!(with_faults.has_faults());
    }
}

mod capabilities {
    use super::*;

    #[test]
    fn test_single_capabilities() {
        assert_eq!(PluginCapabilities::TELEMETRY.bits(), 0b0001);
        assert_eq!(PluginCapabilities::LEDS.bits(), 0b0010);
        assert_eq!(PluginCapabilities::HAPTICS.bits(), 0b0100);
    }

    #[test]
    fn test_combined_capabilities() {
        let combined = PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS;
        assert_eq!(combined.bits(), 0b0011);

        let all_valid = combined | PluginCapabilities::HAPTICS;
        assert_eq!(all_valid.bits(), 0b0111);
    }

    #[test]
    fn test_reserved_bits() {
        let valid = PluginCapabilities::all() & !PluginCapabilities::RESERVED;
        assert_eq!(valid.bits(), 0b0111);
    }

    #[test]
    fn test_from_bits_truncate_preserves_valid() {
        let truncated = PluginCapabilities::from_bits_truncate(0b0111);
        assert_eq!(
            truncated,
            PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS
        );
    }
}

mod wasm_exports {
    use super::*;

    #[test]
    fn test_export_validation_all_present() {
        let validation = WasmExportValidation {
            has_process: true,
            has_memory: true,
            has_init: true,
            has_shutdown: true,
            has_get_info: true,
        };

        assert!(validation.is_valid());
        assert!(validation.missing_required().is_empty());
    }

    #[test]
    fn test_export_validation_minimal() {
        let validation = WasmExportValidation {
            has_process: true,
            has_memory: true,
            ..Default::default()
        };

        assert!(validation.is_valid());
    }

    #[test]
    fn test_export_validation_missing_process() {
        let validation = WasmExportValidation {
            has_process: false,
            has_memory: true,
            ..Default::default()
        };

        assert!(!validation.is_valid());
        assert_eq!(validation.missing_required(), vec!["process"]);
    }

    #[test]
    fn test_export_validation_missing_memory() {
        let validation = WasmExportValidation {
            has_process: true,
            has_memory: false,
            ..Default::default()
        };

        assert!(!validation.is_valid());
        assert_eq!(validation.missing_required(), vec!["memory"]);
    }

    #[test]
    fn test_export_validation_missing_all() {
        let validation = WasmExportValidation::default();

        assert!(!validation.is_valid());
        let missing = validation.missing_required();
        assert_eq!(missing.len(), 2);
        assert!(missing.contains(&"process"));
        assert!(missing.contains(&"memory"));
    }
}

mod init_status {
    use super::*;

    #[test]
    fn test_default_status() {
        let status = PluginInitStatus::default();
        assert_eq!(status, PluginInitStatus::Uninitialized);
    }

    #[test]
    fn test_status_transitions() {
        let status = PluginInitStatus::Uninitialized;
        assert_eq!(status, PluginInitStatus::Uninitialized);

        let status = PluginInitStatus::Initializing;
        assert_eq!(status, PluginInitStatus::Initializing);

        let status = PluginInitStatus::Initialized;
        assert_eq!(status, PluginInitStatus::Initialized);

        let status = PluginInitStatus::ShutDown;
        assert_eq!(status, PluginInitStatus::ShutDown);
    }
}

mod plugin_info {
    use super::*;

    #[test]
    fn test_default_info() {
        let info = WasmPluginInfo::default();

        assert!(info.name.is_empty());
        assert!(info.version.is_empty());
        assert!(info.author.is_empty());
        assert!(info.description.is_empty());
        assert_eq!(info.abi_version, WASM_ABI_VERSION);
    }

    #[test]
    fn test_info_with_values() {
        let info = WasmPluginInfo {
            name: "test_plugin".to_string(),
            version: "1.0.0".to_string(),
            author: "Test Author".to_string(),
            description: "A test plugin".to_string(),
            abi_version: WASM_ABI_VERSION,
        };

        assert_eq!(info.name, "test_plugin");
        assert_eq!(info.version, "1.0.0");
    }
}

mod constants {
    use super::*;

    #[test]
    fn test_abi_version_format() {
        let major = (PLUG_ABI_VERSION >> 16) & 0xFFFF;
        let minor = PLUG_ABI_VERSION & 0xFFFF;

        assert_eq!(major, 1);
        assert_eq!(minor, 0);
    }

    #[test]
    fn test_magic_value() {
        let magic_bytes = PLUG_ABI_MAGIC.to_le_bytes();
        assert_eq!(&magic_bytes, b"1LWW");
    }

    #[test]
    fn test_log_level_values() {
        assert!(log_level::ERROR < log_level::WARN);
        assert!(log_level::WARN < log_level::INFO);
        assert!(log_level::INFO < log_level::DEBUG);
        assert!(log_level::DEBUG < log_level::TRACE);
    }

    #[test]
    fn test_return_code_values() {
        assert_eq!(return_code::SUCCESS, 0);
        assert!(return_code::ERROR < 0);
        assert!(return_code::INVALID_ARG < 0);
        assert!(return_code::PERMISSION_DENIED < 0);
        assert!(return_code::BUFFER_TOO_SMALL < 0);
        assert!(return_code::NOT_INITIALIZED < 0);
    }
}
