#![allow(clippy::redundant_closure)]
//! Comprehensive tests for plugin ABI definitions.
//!
//! Tests cover:
//! - ABI struct layouts and sizes
//! - Version negotiation
//! - Error code exhaustiveness
//! - Byte serialization roundtrips
//! - Capability bitflag invariants
//! - Host function parameter validation

use openracing_plugin_abi::*;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// ABI struct layout and size tests
// ---------------------------------------------------------------------------

mod layout_tests {
    use super::*;

    #[test]
    fn test_plugin_header_size_is_16() {
        assert_eq!(std::mem::size_of::<PluginHeader>(), 16);
    }

    #[test]
    fn test_plugin_header_alignment_is_4() {
        assert_eq!(std::mem::align_of::<PluginHeader>(), 4);
    }

    #[test]
    fn test_telemetry_frame_size_is_32() {
        assert_eq!(std::mem::size_of::<TelemetryFrame>(), 32);
    }

    #[test]
    fn test_telemetry_frame_alignment_is_8() {
        assert_eq!(std::mem::align_of::<TelemetryFrame>(), 8);
    }

    #[test]
    fn test_plugin_capabilities_size_is_4() {
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

// ---------------------------------------------------------------------------
// Version negotiation tests
// ---------------------------------------------------------------------------

mod version_tests {
    use super::*;

    #[test]
    fn test_plug_abi_version_is_1_0() {
        assert_eq!(PLUG_ABI_VERSION, 0x0001_0000);
    }

    #[test]
    fn test_plug_abi_magic_is_wwl1() {
        assert_eq!(PLUG_ABI_MAGIC, 0x57574C31);
        // 'W' = 0x57, 'W' = 0x57, 'L' = 0x4C, '1' = 0x31
    }

    #[test]
    fn test_wasm_abi_version_is_1() {
        assert_eq!(WASM_ABI_VERSION, 1);
    }

    #[test]
    fn test_default_header_is_valid() {
        let header = PluginHeader::default();
        assert!(header.is_valid());
        assert_eq!(header.magic, PLUG_ABI_MAGIC);
        assert_eq!(header.abi_version, PLUG_ABI_VERSION);
        assert_eq!(header.capabilities, 0);
        assert_eq!(header.reserved, 0);
    }

    #[test]
    fn test_invalid_magic_makes_header_invalid() {
        let header = PluginHeader {
            magic: 0xDEADBEEF,
            ..Default::default()
        };
        assert!(!header.is_valid());
    }

    #[test]
    fn test_invalid_version_makes_header_invalid() {
        let header = PluginHeader {
            abi_version: 0x0002_0000,
            ..Default::default()
        };
        assert!(!header.is_valid());
    }

    #[test]
    fn test_header_with_capabilities_is_valid() {
        let caps = PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS;
        let header = PluginHeader::new(caps);
        assert!(header.is_valid());
        assert_eq!(header.get_capabilities(), caps);
    }

    #[test]
    fn test_telemetry_frame_abi_version() {
        assert_eq!(TelemetryFrame::abi_version(), PLUG_ABI_VERSION);
    }
}

// ---------------------------------------------------------------------------
// Error code exhaustiveness and consistency
// ---------------------------------------------------------------------------

mod error_code_tests {
    use super::*;

    #[test]
    fn test_success_is_zero() {
        assert_eq!(return_code::SUCCESS, 0);
    }

    #[test]
    fn test_all_error_codes_are_negative() {
        const {
            assert!(return_code::ERROR < 0);
            assert!(return_code::INVALID_ARG < 0);
            assert!(return_code::PERMISSION_DENIED < 0);
            assert!(return_code::BUFFER_TOO_SMALL < 0);
            assert!(return_code::NOT_INITIALIZED < 0);
        }
    }

    #[test]
    fn test_error_codes_are_unique() {
        let codes = [
            return_code::SUCCESS,
            return_code::ERROR,
            return_code::INVALID_ARG,
            return_code::PERMISSION_DENIED,
            return_code::BUFFER_TOO_SMALL,
            return_code::NOT_INITIALIZED,
        ];

        for (i, &a) in codes.iter().enumerate() {
            for (j, &b) in codes.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "Error codes at index {i} and {j} must be unique");
                }
            }
        }
    }

    #[test]
    fn test_log_levels_are_ordered() {
        const {
            assert!(log_level::ERROR < log_level::WARN);
            assert!(log_level::WARN < log_level::INFO);
            assert!(log_level::INFO < log_level::DEBUG);
            assert!(log_level::DEBUG < log_level::TRACE);
        }
    }

    #[test]
    fn test_log_levels_are_unique() {
        let levels = [
            log_level::ERROR,
            log_level::WARN,
            log_level::INFO,
            log_level::DEBUG,
            log_level::TRACE,
        ];
        for (i, &a) in levels.iter().enumerate() {
            for (j, &b) in levels.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "Log levels at index {i} and {j} must be unique");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Capability bitflag tests
// ---------------------------------------------------------------------------

mod capability_tests {
    use super::*;

    #[test]
    fn test_known_capability_bits() {
        assert_eq!(PluginCapabilities::TELEMETRY.bits(), 0b0000_0001);
        assert_eq!(PluginCapabilities::LEDS.bits(), 0b0000_0010);
        assert_eq!(PluginCapabilities::HAPTICS.bits(), 0b0000_0100);
    }

    #[test]
    fn test_reserved_bits_do_not_overlap_known() {
        let known =
            PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
        let reserved = PluginCapabilities::RESERVED;
        assert_eq!(known.bits() & reserved.bits(), 0);
    }

    #[test]
    fn test_known_plus_reserved_covers_all_bits() {
        let known =
            PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
        let reserved = PluginCapabilities::RESERVED;
        assert_eq!(known.bits() | reserved.bits(), 0xFFFF_FFFF);
    }

    #[test]
    fn test_empty_capabilities() {
        let caps = PluginCapabilities::empty();
        assert_eq!(caps.bits(), 0);
        assert!(!caps.contains(PluginCapabilities::TELEMETRY));
    }

    #[test]
    fn test_all_capabilities() {
        let all = PluginCapabilities::all();
        assert!(all.contains(PluginCapabilities::TELEMETRY));
        assert!(all.contains(PluginCapabilities::LEDS));
        assert!(all.contains(PluginCapabilities::HAPTICS));
        assert!(all.contains(PluginCapabilities::RESERVED));
    }

    #[test]
    fn test_capability_strings_match_expected() {
        assert_eq!(capability_str::READ_TELEMETRY, "read_telemetry");
        assert_eq!(capability_str::MODIFY_TELEMETRY, "modify_telemetry");
        assert_eq!(capability_str::CONTROL_LEDS, "control_leds");
        assert_eq!(capability_str::PROCESS_DSP, "process_dsp");
    }
}

// ---------------------------------------------------------------------------
// Byte serialization proptests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_header_byte_roundtrip(
        magic: u32,
        version: u32,
        caps: u32,
        reserved: u32,
    ) {
        let header = PluginHeader { magic, abi_version: version, capabilities: caps, reserved };
        let bytes = header.to_bytes();
        let restored = PluginHeader::from_bytes(&bytes);
        prop_assert_eq!(header, restored);
    }

    #[test]
    fn prop_telemetry_byte_roundtrip(
        ts: u64,
        angle: f32,
        speed: f32,
        temp: f32,
        faults: u32,
        pad: u32,
    ) {
        let frame = TelemetryFrame {
            timestamp_us: ts,
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
        prop_assert_eq!(frame._pad, restored._pad);
    }

    #[test]
    fn prop_header_bytes_are_16(magic: u32) {
        let header = PluginHeader { magic, ..Default::default() };
        prop_assert_eq!(header.to_bytes().len(), 16);
    }

    #[test]
    fn prop_telemetry_bytes_are_32(ts: u64) {
        let frame = TelemetryFrame::new(ts);
        prop_assert_eq!(frame.to_bytes().len(), 32);
    }

    #[test]
    fn prop_header_little_endian_magic(magic: u32) {
        let header = PluginHeader { magic, ..Default::default() };
        let bytes = header.to_bytes();
        let restored_magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        prop_assert_eq!(restored_magic, magic);
    }

    #[test]
    fn prop_capability_roundtrip_through_header(caps_bits in 0u32..7u32) {
        let caps = PluginCapabilities::from_bits_truncate(caps_bits);
        let header = PluginHeader::new(caps);
        prop_assert_eq!(header.get_capabilities(), caps);
    }

    #[test]
    fn prop_header_has_capability_consistent(caps_bits in 0u32..7u32) {
        let caps = PluginCapabilities::from_bits_truncate(caps_bits);
        let header = PluginHeader::new(caps);

        for flag in [PluginCapabilities::TELEMETRY, PluginCapabilities::LEDS, PluginCapabilities::HAPTICS] {
            prop_assert_eq!(header.has_capability(flag), caps.contains(flag));
        }
    }
}

// ---------------------------------------------------------------------------
// WasmExportValidation tests
// ---------------------------------------------------------------------------

mod export_validation_tests {
    use super::*;

    #[test]
    fn test_valid_exports() {
        let v = WasmExportValidation {
            has_process: true,
            has_memory: true,
            has_init: false,
            has_shutdown: false,
            has_get_info: false,
        };
        assert!(v.is_valid());
        assert!(v.missing_required().is_empty());
    }

    #[test]
    fn test_missing_process() {
        let v = WasmExportValidation {
            has_process: false,
            has_memory: true,
            ..Default::default()
        };
        assert!(!v.is_valid());
        let missing = v.missing_required();
        assert_eq!(missing.len(), 1);
        assert!(missing.contains(&"process"));
    }

    #[test]
    fn test_missing_memory() {
        let v = WasmExportValidation {
            has_process: true,
            has_memory: false,
            ..Default::default()
        };
        assert!(!v.is_valid());
        let missing = v.missing_required();
        assert_eq!(missing.len(), 1);
        assert!(missing.contains(&"memory"));
    }

    #[test]
    fn test_missing_both_required() {
        let v = WasmExportValidation::default();
        assert!(!v.is_valid());
        assert_eq!(v.missing_required().len(), 2);
    }

    #[test]
    fn test_optional_exports_dont_affect_validity() {
        let v = WasmExportValidation {
            has_process: true,
            has_memory: true,
            has_init: true,
            has_shutdown: true,
            has_get_info: true,
        };
        assert!(v.is_valid());
    }
}

// ---------------------------------------------------------------------------
// PluginInitStatus tests
// ---------------------------------------------------------------------------

mod init_status_tests {
    use super::*;

    #[test]
    fn test_default_is_uninitialized() {
        let status = PluginInitStatus::default();
        assert_eq!(status, PluginInitStatus::Uninitialized);
    }

    #[test]
    fn test_all_variants_are_distinct() {
        let variants = [
            PluginInitStatus::Uninitialized,
            PluginInitStatus::Initializing,
            PluginInitStatus::Initialized,
            PluginInitStatus::Failed,
            PluginInitStatus::ShutDown,
        ];

        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "Variants at index {i} and {j} must be distinct");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// WasmPluginInfo tests
// ---------------------------------------------------------------------------

mod plugin_info_tests {
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
}

// ---------------------------------------------------------------------------
// Host function names and module tests
// ---------------------------------------------------------------------------

mod host_function_tests {
    use super::*;

    #[test]
    fn test_host_module_name() {
        assert_eq!(HOST_MODULE, "env");
    }

    #[test]
    fn test_host_function_names_exist() {
        let names = [
            host_function::LOG_DEBUG,
            host_function::LOG_INFO,
            host_function::LOG_WARN,
            host_function::LOG_ERROR,
            host_function::PLUGIN_LOG,
            host_function::CHECK_CAPABILITY,
            host_function::GET_TELEMETRY,
            host_function::GET_TIMESTAMP_US,
        ];

        for name in &names {
            assert!(!name.is_empty(), "Host function name must not be empty");
        }

        // All names must be unique
        for (i, &a) in names.iter().enumerate() {
            for (j, &b) in names.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "Host function names at index {i} and {j} must be unique");
                }
            }
        }
    }

    #[test]
    fn test_wasm_export_names() {
        assert_eq!(wasm_export::PROCESS, "process");
        assert_eq!(wasm_export::MEMORY, "memory");
    }

    #[test]
    fn test_wasm_optional_export_names() {
        assert_eq!(wasm_optional_export::INIT, "init");
        assert_eq!(wasm_optional_export::SHUTDOWN, "shutdown");
        assert_eq!(wasm_optional_export::GET_INFO, "get_info");
    }

    #[test]
    fn test_host_function_names_module_matches_constants() {
        assert_eq!(host_function_names::LOG_DEBUG, host_function::LOG_DEBUG);
        assert_eq!(host_function_names::LOG_INFO, host_function::LOG_INFO);
        assert_eq!(host_function_names::LOG_WARN, host_function::LOG_WARN);
        assert_eq!(host_function_names::LOG_ERROR, host_function::LOG_ERROR);
        assert_eq!(host_function_names::PLUGIN_LOG, host_function::PLUGIN_LOG);
        assert_eq!(host_function_names::CHECK_CAPABILITY, host_function::CHECK_CAPABILITY);
        assert_eq!(host_function_names::GET_TELEMETRY, host_function::GET_TELEMETRY);
        assert_eq!(host_function_names::GET_TIMESTAMP_US, host_function::GET_TIMESTAMP_US);
    }
}

// ---------------------------------------------------------------------------
// Host function validation tests
// ---------------------------------------------------------------------------

mod validation_tests {
    use openracing_plugin_abi::host_functions::validation;

    #[test]
    fn test_validate_string_params_valid() {
        let result = validation::validate_string_params(0, 10, 1024);
        assert_eq!(result, openracing_plugin_abi::return_code::SUCCESS);
    }

    #[test]
    fn test_validate_string_params_negative_ptr() {
        let result = validation::validate_string_params(-1, 10, 1024);
        assert_eq!(result, openracing_plugin_abi::return_code::INVALID_ARG);
    }

    #[test]
    fn test_validate_string_params_negative_len() {
        let result = validation::validate_string_params(0, -1, 1024);
        assert_eq!(result, openracing_plugin_abi::return_code::INVALID_ARG);
    }

    #[test]
    fn test_validate_string_params_too_long() {
        let result = validation::validate_string_params(0, 2048, 1024);
        assert_eq!(result, openracing_plugin_abi::return_code::BUFFER_TOO_SMALL);
    }

    #[test]
    fn test_validate_output_buffer_valid() {
        let result = validation::validate_output_buffer(0, 64, 32);
        assert_eq!(result, openracing_plugin_abi::return_code::SUCCESS);
    }

    #[test]
    fn test_validate_output_buffer_negative_ptr() {
        let result = validation::validate_output_buffer(-1, 64, 32);
        assert_eq!(result, openracing_plugin_abi::return_code::INVALID_ARG);
    }

    #[test]
    fn test_validate_output_buffer_too_small() {
        let result = validation::validate_output_buffer(0, 16, 32);
        assert_eq!(result, openracing_plugin_abi::return_code::BUFFER_TOO_SMALL);
    }
}

// ---------------------------------------------------------------------------
// Telemetry frame validation
// ---------------------------------------------------------------------------

mod telemetry_tests {
    use super::*;

    #[test]
    fn test_default_temperature_is_normal() {
        let frame = TelemetryFrame::default();
        assert!(frame.is_temperature_normal());
    }

    #[test]
    fn test_temperature_boundaries() {
        let low = TelemetryFrame { temperature_c: 19.9, ..Default::default() };
        assert!(!low.is_temperature_normal());

        let at_20 = TelemetryFrame { temperature_c: 20.0, ..Default::default() };
        assert!(at_20.is_temperature_normal());

        let at_80 = TelemetryFrame { temperature_c: 80.0, ..Default::default() };
        assert!(at_80.is_temperature_normal());

        let over_80 = TelemetryFrame { temperature_c: 80.1, ..Default::default() };
        assert!(!over_80.is_temperature_normal());
    }

    #[test]
    fn test_angle_boundaries() {
        let valid = TelemetryFrame { wheel_angle_deg: 1800.0, ..Default::default() };
        assert!(valid.is_angle_valid());

        let neg_valid = TelemetryFrame { wheel_angle_deg: -1800.0, ..Default::default() };
        assert!(neg_valid.is_angle_valid());

        let invalid = TelemetryFrame { wheel_angle_deg: 1800.1, ..Default::default() };
        assert!(!invalid.is_angle_valid());
    }

    #[test]
    fn test_no_faults_by_default() {
        let frame = TelemetryFrame::default();
        assert!(!frame.has_faults());
    }

    #[test]
    fn test_faults_detected() {
        let frame = TelemetryFrame { fault_flags: 0x01, ..Default::default() };
        assert!(frame.has_faults());
    }

    #[test]
    fn test_telemetry_with_values() {
        let frame = TelemetryFrame::with_values(999, 45.0, 2.75, 55.0, 0xAB);
        assert_eq!(frame.timestamp_us, 999);
        assert!((frame.wheel_angle_deg - 45.0).abs() < f32::EPSILON);
        assert!((frame.wheel_speed_rad_s - 2.75).abs() < f32::EPSILON);
        assert!((frame.temperature_c - 55.0).abs() < f32::EPSILON);
        assert_eq!(frame.fault_flags, 0xAB);
        assert_eq!(frame._pad, 0);
    }
}
