//! Snapshot tests for ABI version compatibility.
//!
//! These tests verify that ABI types serialize consistently and that
//! the snapshot outputs remain stable across versions.

use insta::assert_snapshot;
use openracing_plugin_abi::*;

mod plugin_header_snapshots {
    use super::*;

    #[test]
    fn test_default_header() {
        let header = PluginHeader::default();
        assert_snapshot!(format!("{:#?}", header));
    }

    #[test]
    fn test_header_with_telemetry() {
        let header = PluginHeader::new(PluginCapabilities::TELEMETRY);
        assert_snapshot!(format!("{:#?}", header));
    }

    #[test]
    fn test_header_with_all_capabilities() {
        let header = PluginHeader::new(
            PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS,
        );
        assert_snapshot!(format!("{:#?}", header));
    }

    #[test]
    fn test_header_bytes() {
        let header = PluginHeader::new(PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS);
        let bytes = header.to_bytes();
        assert_snapshot!(format!("{:02X?}", bytes));
    }
}

mod telemetry_frame_snapshots {
    use super::*;

    #[test]
    fn test_default_frame() {
        let frame = TelemetryFrame::default();
        assert_snapshot!(format!("{:#?}", frame));
    }

    #[test]
    fn test_frame_with_values() {
        let frame = TelemetryFrame::with_values(1234567890, 45.5, 1.57, 55.0, 0x0F);
        assert_snapshot!(format!("{:#?}", frame));
    }

    #[test]
    fn test_frame_bytes() {
        let frame = TelemetryFrame::with_values(0x1122334455667788, 90.0, 0.0, 25.0, 0xAABBCCDD);
        let bytes = frame.to_bytes();
        assert_snapshot!(format!("{:02X?}", bytes));
    }
}

mod capabilities_snapshots {
    use super::*;

    #[test]
    fn test_telemetry_capability() {
        let caps = PluginCapabilities::TELEMETRY;
        assert_snapshot!(format!("{:?}", caps));
    }

    #[test]
    fn test_leds_capability() {
        let caps = PluginCapabilities::LEDS;
        assert_snapshot!(format!("{:?}", caps));
    }

    #[test]
    fn test_haptics_capability() {
        let caps = PluginCapabilities::HAPTICS;
        assert_snapshot!(format!("{:?}", caps));
    }

    #[test]
    fn test_combined_capabilities() {
        let caps = PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS;
        assert_snapshot!(format!("{:?}", caps));
    }

    #[test]
    fn test_all_valid_capabilities() {
        let caps =
            PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
        assert_snapshot!(format!("{:?}", caps));
    }
}

mod wasm_validation_snapshots {
    use super::*;

    #[test]
    fn test_valid_minimal_exports() {
        let validation = WasmExportValidation {
            has_process: true,
            has_memory: true,
            ..Default::default()
        };
        assert_snapshot!(format!("{:#?}", validation));
    }

    #[test]
    fn test_valid_all_exports() {
        let validation = WasmExportValidation {
            has_process: true,
            has_memory: true,
            has_init: true,
            has_shutdown: true,
            has_get_info: true,
        };
        assert_snapshot!(format!("{:#?}", validation));
    }

    #[test]
    fn test_missing_required_exports() {
        let validation = WasmExportValidation::default();
        let missing = validation.missing_required();
        assert_snapshot!(format!("{:?}", missing));
    }
}

mod init_status_snapshots {
    use super::*;

    #[test]
    fn test_all_statuses() {
        let statuses = format!(
            "{:?} {:?} {:?} {:?} {:?}",
            PluginInitStatus::Uninitialized,
            PluginInitStatus::Initializing,
            PluginInitStatus::Initialized,
            PluginInitStatus::Failed,
            PluginInitStatus::ShutDown
        );
        assert_snapshot!(statuses);
    }
}

mod plugin_info_snapshots {
    use super::*;

    #[test]
    fn test_default_info() {
        let info = WasmPluginInfo::default();
        assert_snapshot!(format!("{:#?}", info));
    }

    #[test]
    fn test_populated_info() {
        let info = WasmPluginInfo {
            name: "example-plugin".to_string(),
            version: "1.2.3".to_string(),
            author: "OpenRacing Team".to_string(),
            description: "An example plugin for testing".to_string(),
            abi_version: WASM_ABI_VERSION,
        };
        assert_snapshot!(format!("{:#?}", info));
    }
}

mod constants_snapshots {
    use super::*;

    #[test]
    fn test_all_constants() {
        let constants = format!(
            "PLUG_ABI_VERSION={:#010X}\nPLUG_ABI_MAGIC={:#010X}\nWASM_ABI_VERSION={}\nHOST_MODULE={}",
            PLUG_ABI_VERSION, PLUG_ABI_MAGIC, WASM_ABI_VERSION, HOST_MODULE
        );
        assert_snapshot!(constants);
    }

    #[test]
    fn test_log_levels() {
        let levels = format!(
            "ERROR={} WARN={} INFO={} DEBUG={} TRACE={}",
            log_level::ERROR,
            log_level::WARN,
            log_level::INFO,
            log_level::DEBUG,
            log_level::TRACE
        );
        assert_snapshot!(levels);
    }

    #[test]
    fn test_return_codes() {
        let codes = format!(
            "SUCCESS={} ERROR={} INVALID_ARG={} PERMISSION_DENIED={} BUFFER_TOO_SMALL={} NOT_INITIALIZED={}",
            return_code::SUCCESS,
            return_code::ERROR,
            return_code::INVALID_ARG,
            return_code::PERMISSION_DENIED,
            return_code::BUFFER_TOO_SMALL,
            return_code::NOT_INITIALIZED
        );
        assert_snapshot!(codes);
    }

    #[test]
    fn test_capability_strings() {
        let strings = format!(
            "READ_TELEMETRY={}\nMODIFY_TELEMETRY={}\nCONTROL_LEDS={}\nPROCESS_DSP={}",
            capability_str::READ_TELEMETRY,
            capability_str::MODIFY_TELEMETRY,
            capability_str::CONTROL_LEDS,
            capability_str::PROCESS_DSP
        );
        assert_snapshot!(strings);
    }
}

#[cfg(feature = "serde")]
mod serde_snapshots {
    use super::*;

    #[test]
    fn test_telemetry_frame_json() {
        let frame = TelemetryFrame::with_values(12345, 90.0, 1.57, 45.5, 0xFF);
        let json = serde_json::to_string_pretty(&frame).unwrap();
        assert_snapshot!(json);
    }

    #[test]
    fn test_init_status_json() {
        let status = PluginInitStatus::Initialized;
        let json = serde_json::to_string_pretty(&status).unwrap();
        assert_snapshot!(json);
    }

    #[test]
    fn test_plugin_info_json() {
        let info = WasmPluginInfo {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            description: "test".to_string(),
            abi_version: 1,
        };
        let json = serde_json::to_string_pretty(&info).unwrap();
        assert_snapshot!(json);
    }
}
