//! Deep tests for the OpenRacing plugin ABI crate.
//!
//! Covers ABI version constants, FFI type layouts, function pointer tables,
//! capability flags, error codes, plugin descriptor construction, and
//! ABI compatibility checks.

use openracing_plugin_abi::constants::{
    HOST_MODULE, PLUG_ABI_MAGIC, PLUG_ABI_VERSION, WASM_ABI_VERSION, capability_str,
    host_function, log_level, return_code, wasm_export, wasm_optional_export,
};
use openracing_plugin_abi::host_functions::{names, signatures, validation};
use openracing_plugin_abi::telemetry_frame::TelemetryFrame;
use openracing_plugin_abi::types::{
    PluginCapabilities, PluginHeader, PluginInitStatus, WasmExportValidation, WasmPluginInfo,
};

// ---------------------------------------------------------------------------
// ABI version constants
// ---------------------------------------------------------------------------

#[test]
fn abi_version_format_major_minor() {
    // Version format: major (upper 16 bits) << 16 | minor (lower 16 bits)
    let major = PLUG_ABI_VERSION >> 16;
    let minor = PLUG_ABI_VERSION & 0xFFFF;
    assert_eq!(major, 1, "Expected major version 1");
    assert_eq!(minor, 0, "Expected minor version 0");
}

#[test]
fn abi_magic_matches_expected_ascii() {
    // 'WWL1' in little-endian: 0x57574C31
    let bytes = PLUG_ABI_MAGIC.to_le_bytes();
    assert_eq!(bytes[0], b'1');
    assert_eq!(bytes[1], b'L');
    assert_eq!(bytes[2], b'W');
    assert_eq!(bytes[3], b'W');
}

#[test]
fn wasm_abi_version_is_positive_nonzero() {
    const _: () = assert!(WASM_ABI_VERSION > 0);
}

#[test]
fn host_module_is_env() {
    assert_eq!(HOST_MODULE, "env");
}

#[test]
fn plug_abi_version_and_magic_are_distinct() {
    assert_ne!(PLUG_ABI_VERSION, PLUG_ABI_MAGIC);
}

// ---------------------------------------------------------------------------
// FFI type layouts (size and alignment)
// ---------------------------------------------------------------------------

#[test]
fn plugin_header_size_is_16() {
    assert_eq!(std::mem::size_of::<PluginHeader>(), 16);
}

#[test]
fn plugin_header_alignment_is_4() {
    assert_eq!(std::mem::align_of::<PluginHeader>(), 4);
}

#[test]
fn plugin_capabilities_size_is_4() {
    assert_eq!(std::mem::size_of::<PluginCapabilities>(), 4);
}

#[test]
fn plugin_capabilities_alignment_is_4() {
    assert_eq!(std::mem::align_of::<PluginCapabilities>(), 4);
}

#[test]
fn telemetry_frame_size_is_32() {
    assert_eq!(std::mem::size_of::<TelemetryFrame>(), 32);
}

#[test]
fn telemetry_frame_alignment_is_8() {
    assert_eq!(std::mem::align_of::<TelemetryFrame>(), 8);
}

#[test]
fn plugin_header_field_offsets_match_documented_layout() {
    // PluginHeader memory layout:
    // offset 0: magic (4), offset 4: abi_version (4),
    // offset 8: capabilities (4), offset 12: reserved (4)
    let header = PluginHeader::default();
    let base = &header as *const PluginHeader as usize;
    let magic_offset = &header.magic as *const u32 as usize - base;
    let abi_offset = &header.abi_version as *const u32 as usize - base;
    let caps_offset = &header.capabilities as *const u32 as usize - base;
    let reserved_offset = &header.reserved as *const u32 as usize - base;

    assert_eq!(magic_offset, 0);
    assert_eq!(abi_offset, 4);
    assert_eq!(caps_offset, 8);
    assert_eq!(reserved_offset, 12);
}

#[test]
fn telemetry_frame_field_offsets_match_documented_layout() {
    let frame = TelemetryFrame::default();
    let base = &frame as *const TelemetryFrame as usize;
    let ts_offset = &frame.timestamp_us as *const u64 as usize - base;
    let angle_offset = &frame.wheel_angle_deg as *const f32 as usize - base;
    let speed_offset = &frame.wheel_speed_rad_s as *const f32 as usize - base;
    let temp_offset = &frame.temperature_c as *const f32 as usize - base;
    let fault_offset = &frame.fault_flags as *const u32 as usize - base;
    let pad_offset = &frame._pad as *const u32 as usize - base;

    assert_eq!(ts_offset, 0);
    assert_eq!(angle_offset, 8);
    assert_eq!(speed_offset, 12);
    assert_eq!(temp_offset, 16);
    assert_eq!(fault_offset, 20);
    assert_eq!(pad_offset, 24);
}

// ---------------------------------------------------------------------------
// Function pointer table signatures (type assertions via size checks)
// ---------------------------------------------------------------------------

#[test]
fn log_fn_signature_size() {
    assert_eq!(
        std::mem::size_of::<signatures::LogFn>(),
        std::mem::size_of::<usize>(),
        "LogFn should be pointer-sized"
    );
}

#[test]
fn plugin_log_fn_signature_size() {
    assert_eq!(
        std::mem::size_of::<signatures::PluginLogFn>(),
        std::mem::size_of::<usize>(),
    );
}

#[test]
fn check_capability_fn_signature_size() {
    assert_eq!(
        std::mem::size_of::<signatures::CheckCapabilityFn>(),
        std::mem::size_of::<usize>(),
    );
}

#[test]
fn get_telemetry_fn_signature_size() {
    assert_eq!(
        std::mem::size_of::<signatures::GetTelemetryFn>(),
        std::mem::size_of::<usize>(),
    );
}

#[test]
fn get_timestamp_fn_signature_size() {
    assert_eq!(
        std::mem::size_of::<signatures::GetTimestampFn>(),
        std::mem::size_of::<usize>(),
    );
}

// ---------------------------------------------------------------------------
// Capability flags
// ---------------------------------------------------------------------------

#[test]
fn capability_individual_bit_positions() {
    assert_eq!(PluginCapabilities::TELEMETRY.bits(), 0b0000_0001);
    assert_eq!(PluginCapabilities::LEDS.bits(), 0b0000_0010);
    assert_eq!(PluginCapabilities::HAPTICS.bits(), 0b0000_0100);
}

#[test]
fn capability_flags_no_overlap_with_reserved() {
    let defined = PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
    assert_eq!(defined.bits() & PluginCapabilities::RESERVED.bits(), 0);
}

#[test]
fn capability_reserved_covers_remaining_bits() {
    let defined = PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
    assert_eq!(
        defined.bits() | PluginCapabilities::RESERVED.bits(),
        0xFFFF_FFFF
    );
}

#[test]
fn capability_empty_has_no_bits_set() {
    let empty = PluginCapabilities::empty();
    assert_eq!(empty.bits(), 0);
}

#[test]
fn capability_all_defined_flags_combine_correctly() {
    let all = PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
    assert_eq!(all.bits(), 0b0000_0111);
    assert!(all.contains(PluginCapabilities::TELEMETRY));
    assert!(all.contains(PluginCapabilities::LEDS));
    assert!(all.contains(PluginCapabilities::HAPTICS));
}

#[test]
fn capability_from_bits_truncate_drops_reserved() {
    let caps = PluginCapabilities::from_bits_truncate(0xFFFF_FFFF);
    // Should contain all defined bits
    assert!(caps.contains(PluginCapabilities::TELEMETRY));
    assert!(caps.contains(PluginCapabilities::LEDS));
    assert!(caps.contains(PluginCapabilities::HAPTICS));
    // AND the reserved bits too, since RESERVED is defined
    assert!(caps.contains(PluginCapabilities::RESERVED));
}

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

#[test]
fn return_code_success_is_zero() {
    assert_eq!(return_code::SUCCESS, 0);
}

#[test]
fn all_error_return_codes_are_negative() {
    let codes = [
        return_code::ERROR,
        return_code::INVALID_ARG,
        return_code::PERMISSION_DENIED,
        return_code::BUFFER_TOO_SMALL,
        return_code::NOT_INITIALIZED,
    ];
    for code in codes {
        assert!(code < 0, "Error code {code} should be negative");
    }
}

#[test]
fn error_return_codes_are_unique() {
    let codes = [
        return_code::ERROR,
        return_code::INVALID_ARG,
        return_code::PERMISSION_DENIED,
        return_code::BUFFER_TOO_SMALL,
        return_code::NOT_INITIALIZED,
    ];
    for i in 0..codes.len() {
        for j in (i + 1)..codes.len() {
            assert_ne!(codes[i], codes[j], "Error codes at {i} and {j} collide");
        }
    }
}

#[test]
fn log_levels_are_ordered() {
    const _: () = assert!(log_level::ERROR < log_level::WARN);
    const _: () = assert!(log_level::WARN < log_level::INFO);
    const _: () = assert!(log_level::INFO < log_level::DEBUG);
    const _: () = assert!(log_level::DEBUG < log_level::TRACE);
}

#[test]
fn log_levels_are_contiguous() {
    const _: () = assert!(log_level::WARN - log_level::ERROR == 1);
    const _: () = assert!(log_level::INFO - log_level::WARN == 1);
    const _: () = assert!(log_level::DEBUG - log_level::INFO == 1);
    const _: () = assert!(log_level::TRACE - log_level::DEBUG == 1);
}

// ---------------------------------------------------------------------------
// Plugin descriptor construction
// ---------------------------------------------------------------------------

#[test]
fn plugin_header_default_is_valid() {
    let header = PluginHeader::default();
    assert!(header.is_valid());
    assert_eq!(header.magic, PLUG_ABI_MAGIC);
    assert_eq!(header.abi_version, PLUG_ABI_VERSION);
    assert_eq!(header.capabilities, 0);
    assert_eq!(header.reserved, 0);
}

#[test]
fn plugin_header_new_with_capabilities() {
    let caps = PluginCapabilities::TELEMETRY | PluginCapabilities::HAPTICS;
    let header = PluginHeader::new(caps);
    assert!(header.is_valid());
    assert!(header.has_capability(PluginCapabilities::TELEMETRY));
    assert!(header.has_capability(PluginCapabilities::HAPTICS));
    assert!(!header.has_capability(PluginCapabilities::LEDS));
}

#[test]
fn plugin_header_invalid_magic_rejected() {
    let header = PluginHeader {
        magic: 0xDEADBEEF,
        ..Default::default()
    };
    assert!(!header.is_valid());
}

#[test]
fn plugin_header_invalid_version_rejected() {
    let header = PluginHeader {
        abi_version: 0x0002_0000,
        ..Default::default()
    };
    assert!(!header.is_valid());
}

#[test]
fn plugin_header_roundtrip_bytes() {
    let header = PluginHeader::new(PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS);
    let bytes = header.to_bytes();
    let restored = PluginHeader::from_bytes(&bytes);
    assert_eq!(header, restored);
}

#[test]
fn plugin_header_bytes_are_little_endian() {
    let header = PluginHeader::default();
    let bytes = header.to_bytes();
    let magic_from_bytes = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    assert_eq!(magic_from_bytes, PLUG_ABI_MAGIC);
}

#[test]
fn plugin_header_zero_reserved_field() {
    let header =
        PluginHeader::new(PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS);
    assert_eq!(header.reserved, 0);
}

#[test]
fn plugin_header_get_capabilities_returns_correct_flags() {
    let header = PluginHeader::new(PluginCapabilities::LEDS);
    let caps = header.get_capabilities();
    assert!(caps.contains(PluginCapabilities::LEDS));
    assert!(!caps.contains(PluginCapabilities::TELEMETRY));
}

// ---------------------------------------------------------------------------
// Plugin init status
// ---------------------------------------------------------------------------

#[test]
fn plugin_init_status_default_is_uninitialized() {
    assert_eq!(PluginInitStatus::default(), PluginInitStatus::Uninitialized);
}

#[test]
fn plugin_init_status_all_variants_are_distinct() {
    let variants = [
        PluginInitStatus::Uninitialized,
        PluginInitStatus::Initializing,
        PluginInitStatus::Initialized,
        PluginInitStatus::Failed,
        PluginInitStatus::ShutDown,
    ];
    for i in 0..variants.len() {
        for j in (i + 1)..variants.len() {
            assert_ne!(variants[i], variants[j]);
        }
    }
}

#[test]
fn plugin_init_status_clone_eq() {
    let status = PluginInitStatus::Initialized;
    let cloned = status;
    assert_eq!(status, cloned);
}

// ---------------------------------------------------------------------------
// WASM export validation
// ---------------------------------------------------------------------------

#[test]
fn wasm_export_validation_default_is_invalid() {
    let v = WasmExportValidation::default();
    assert!(!v.is_valid());
    assert!(!v.has_process);
    assert!(!v.has_memory);
}

#[test]
fn wasm_export_validation_all_required_makes_valid() {
    let v = WasmExportValidation {
        has_process: true,
        has_memory: true,
        ..Default::default()
    };
    assert!(v.is_valid());
    assert!(v.missing_required().is_empty());
}

#[test]
fn wasm_export_validation_missing_process_only() {
    let v = WasmExportValidation {
        has_process: false,
        has_memory: true,
        ..Default::default()
    };
    assert!(!v.is_valid());
    let missing = v.missing_required();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0], wasm_export::PROCESS);
}

#[test]
fn wasm_export_validation_missing_memory_only() {
    let v = WasmExportValidation {
        has_process: true,
        has_memory: false,
        ..Default::default()
    };
    assert!(!v.is_valid());
    let missing = v.missing_required();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0], wasm_export::MEMORY);
}

#[test]
fn wasm_export_validation_missing_both_required() {
    let v = WasmExportValidation::default();
    let missing = v.missing_required();
    assert_eq!(missing.len(), 2);
    assert!(missing.contains(&wasm_export::PROCESS));
    assert!(missing.contains(&wasm_export::MEMORY));
}

#[test]
fn wasm_export_validation_optional_exports_dont_affect_validity() {
    let v = WasmExportValidation {
        has_process: true,
        has_memory: true,
        has_init: true,
        has_shutdown: true,
        has_get_info: true,
    };
    assert!(v.is_valid());
    assert!(v.missing_required().is_empty());
}

// ---------------------------------------------------------------------------
// WASM plugin info
// ---------------------------------------------------------------------------

#[test]
fn wasm_plugin_info_default_fields() {
    let info = WasmPluginInfo::default();
    assert!(info.name.is_empty());
    assert!(info.version.is_empty());
    assert!(info.author.is_empty());
    assert!(info.description.is_empty());
    assert_eq!(info.abi_version, WASM_ABI_VERSION);
}

#[test]
fn wasm_plugin_info_custom_construction() {
    let info = WasmPluginInfo {
        name: "test-plugin".to_string(),
        version: "1.0.0".to_string(),
        author: "Test Author".to_string(),
        description: "A test plugin".to_string(),
        abi_version: WASM_ABI_VERSION,
    };
    assert_eq!(info.name, "test-plugin");
    assert_eq!(info.version, "1.0.0");
    assert_eq!(info.abi_version, WASM_ABI_VERSION);
}

// ---------------------------------------------------------------------------
// ABI compatibility checks
// ---------------------------------------------------------------------------

#[test]
fn header_with_all_zero_capabilities_is_valid() {
    let header = PluginHeader {
        magic: PLUG_ABI_MAGIC,
        abi_version: PLUG_ABI_VERSION,
        capabilities: 0,
        reserved: 0,
    };
    assert!(header.is_valid());
}

#[test]
fn header_with_max_u32_magic_is_invalid() {
    let header = PluginHeader {
        magic: u32::MAX,
        abi_version: PLUG_ABI_VERSION,
        capabilities: 0,
        reserved: 0,
    };
    assert!(!header.is_valid());
}

#[test]
fn header_with_zero_magic_is_invalid() {
    let header = PluginHeader {
        magic: 0,
        abi_version: PLUG_ABI_VERSION,
        capabilities: 0,
        reserved: 0,
    };
    assert!(!header.is_valid());
}

#[test]
fn header_with_zero_version_is_invalid() {
    let header = PluginHeader {
        magic: PLUG_ABI_MAGIC,
        abi_version: 0,
        capabilities: 0,
        reserved: 0,
    };
    assert!(!header.is_valid());
}

#[test]
fn header_roundtrip_with_all_capabilities() {
    let all_caps =
        PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
    let header = PluginHeader::new(all_caps);
    let bytes = header.to_bytes();
    let restored = PluginHeader::from_bytes(&bytes);

    assert_eq!(restored.magic, PLUG_ABI_MAGIC);
    assert_eq!(restored.abi_version, PLUG_ABI_VERSION);
    assert_eq!(restored.capabilities, all_caps.bits());
    assert_eq!(restored.reserved, 0);
    assert!(restored.is_valid());
}

// ---------------------------------------------------------------------------
// Telemetry frame ABI
// ---------------------------------------------------------------------------

#[test]
fn telemetry_frame_default_temperature_is_20() {
    let frame = TelemetryFrame::default();
    assert!((frame.temperature_c - 20.0).abs() < f32::EPSILON);
}

#[test]
fn telemetry_frame_new_sets_timestamp() {
    let frame = TelemetryFrame::new(999_999);
    assert_eq!(frame.timestamp_us, 999_999);
    assert!((frame.wheel_angle_deg - 0.0).abs() < f32::EPSILON);
}

#[test]
fn telemetry_frame_with_values_roundtrip() {
    let frame =
        TelemetryFrame::with_values(1_000_000, 90.0, std::f32::consts::PI, 45.0, 0x0001);
    let bytes = frame.to_bytes();
    let restored = TelemetryFrame::from_bytes(&bytes);

    assert_eq!(frame.timestamp_us, restored.timestamp_us);
    assert_eq!(frame.wheel_angle_deg, restored.wheel_angle_deg);
    assert_eq!(frame.wheel_speed_rad_s, restored.wheel_speed_rad_s);
    assert_eq!(frame.temperature_c, restored.temperature_c);
    assert_eq!(frame.fault_flags, restored.fault_flags);
}

#[test]
fn telemetry_frame_boundary_temperatures() {
    let cold = TelemetryFrame::with_values(0, 0.0, 0.0, 19.9, 0);
    assert!(!cold.is_temperature_normal());

    let low_normal = TelemetryFrame::with_values(0, 0.0, 0.0, 20.0, 0);
    assert!(low_normal.is_temperature_normal());

    let high_normal = TelemetryFrame::with_values(0, 0.0, 0.0, 80.0, 0);
    assert!(high_normal.is_temperature_normal());

    let hot = TelemetryFrame::with_values(0, 0.0, 0.0, 80.1, 0);
    assert!(!hot.is_temperature_normal());
}

#[test]
fn telemetry_frame_boundary_angles() {
    let valid_neg = TelemetryFrame::with_values(0, -1800.0, 0.0, 20.0, 0);
    assert!(valid_neg.is_angle_valid());

    let valid_pos = TelemetryFrame::with_values(0, 1800.0, 0.0, 20.0, 0);
    assert!(valid_pos.is_angle_valid());

    let too_neg = TelemetryFrame::with_values(0, -1800.1, 0.0, 20.0, 0);
    assert!(!too_neg.is_angle_valid());

    let too_pos = TelemetryFrame::with_values(0, 1800.1, 0.0, 20.0, 0);
    assert!(!too_pos.is_angle_valid());
}

#[test]
fn telemetry_frame_fault_flags() {
    let no_fault = TelemetryFrame::with_values(0, 0.0, 0.0, 20.0, 0);
    assert!(!no_fault.has_faults());

    let with_fault = TelemetryFrame::with_values(0, 0.0, 0.0, 20.0, 1);
    assert!(with_fault.has_faults());

    let many_faults = TelemetryFrame::with_values(0, 0.0, 0.0, 20.0, 0xFFFF_FFFF);
    assert!(many_faults.has_faults());
}

#[test]
fn telemetry_frame_abi_version_matches() {
    assert_eq!(TelemetryFrame::abi_version(), PLUG_ABI_VERSION);
}

#[test]
fn telemetry_frame_bytes_zero_pad() {
    let frame = TelemetryFrame::default();
    let bytes = frame.to_bytes();
    // _pad field is at offset 24..28 and should be all zeros
    assert_eq!(bytes[24], 0);
    assert_eq!(bytes[25], 0);
    assert_eq!(bytes[26], 0);
    assert_eq!(bytes[27], 0);
}

// ---------------------------------------------------------------------------
// Host function and export name constants
// ---------------------------------------------------------------------------

#[test]
fn host_function_names_are_nonempty_ascii() {
    let names_list = [
        host_function::LOG_DEBUG,
        host_function::LOG_INFO,
        host_function::LOG_WARN,
        host_function::LOG_ERROR,
        host_function::PLUGIN_LOG,
        host_function::CHECK_CAPABILITY,
        host_function::GET_TELEMETRY,
        host_function::GET_TIMESTAMP_US,
    ];
    for name in names_list {
        assert!(!name.is_empty());
        assert!(name.is_ascii());
    }
}

#[test]
fn host_function_names_module_re_exports_match() {
    assert_eq!(names::LOG_DEBUG, host_function::LOG_DEBUG);
    assert_eq!(names::LOG_INFO, host_function::LOG_INFO);
    assert_eq!(names::LOG_WARN, host_function::LOG_WARN);
    assert_eq!(names::LOG_ERROR, host_function::LOG_ERROR);
    assert_eq!(names::PLUGIN_LOG, host_function::PLUGIN_LOG);
    assert_eq!(names::CHECK_CAPABILITY, host_function::CHECK_CAPABILITY);
    assert_eq!(names::GET_TELEMETRY, host_function::GET_TELEMETRY);
    assert_eq!(names::GET_TIMESTAMP_US, host_function::GET_TIMESTAMP_US);
}

#[test]
fn wasm_export_names_are_valid_identifiers() {
    let exports = [wasm_export::PROCESS, wasm_export::MEMORY];
    for name in exports {
        assert!(!name.is_empty());
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    }
}

#[test]
fn wasm_optional_export_names_are_valid_identifiers() {
    let exports = [
        wasm_optional_export::INIT,
        wasm_optional_export::SHUTDOWN,
        wasm_optional_export::GET_INFO,
    ];
    for name in exports {
        assert!(!name.is_empty());
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    }
}

#[test]
fn capability_strings_are_snake_case() {
    let caps = [
        capability_str::READ_TELEMETRY,
        capability_str::MODIFY_TELEMETRY,
        capability_str::CONTROL_LEDS,
        capability_str::PROCESS_DSP,
    ];
    for cap in caps {
        assert!(!cap.is_empty());
        assert!(cap.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
    }
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

#[test]
fn validate_string_params_valid() {
    let result = validation::validate_string_params(0, 10, 1024);
    assert_eq!(result, return_code::SUCCESS);
}

#[test]
fn validate_string_params_negative_ptr() {
    let result = validation::validate_string_params(-1, 10, 1024);
    assert_eq!(result, return_code::INVALID_ARG);
}

#[test]
fn validate_string_params_negative_len() {
    let result = validation::validate_string_params(0, -1, 1024);
    assert_eq!(result, return_code::INVALID_ARG);
}

#[test]
fn validate_string_params_too_long() {
    let result = validation::validate_string_params(0, 2048, 1024);
    assert_eq!(result, return_code::BUFFER_TOO_SMALL);
}

#[test]
fn validate_output_buffer_valid() {
    let result = validation::validate_output_buffer(0, 64, 32);
    assert_eq!(result, return_code::SUCCESS);
}

#[test]
fn validate_output_buffer_negative_ptr() {
    let result = validation::validate_output_buffer(-1, 64, 32);
    assert_eq!(result, return_code::INVALID_ARG);
}

#[test]
fn validate_output_buffer_too_small() {
    let result = validation::validate_output_buffer(0, 16, 32);
    assert_eq!(result, return_code::BUFFER_TOO_SMALL);
}

#[test]
fn validate_output_buffer_exact_size() {
    let result = validation::validate_output_buffer(0, 32, 32);
    assert_eq!(result, return_code::SUCCESS);
}

// ---------------------------------------------------------------------------
// Serde (when feature is enabled)
// ---------------------------------------------------------------------------

#[test]
fn telemetry_frame_serde_roundtrip() {
    let frame = TelemetryFrame::with_values(123_456, 45.0, 1.5, 55.0, 0x42);
    let json = serde_json::to_string(&frame);
    assert!(json.is_ok());
    let json_str = json.as_deref().unwrap_or("");
    let restored: Result<TelemetryFrame, _> = serde_json::from_str(json_str);
    assert!(restored.is_ok());
    if let Ok(r) = restored {
        assert_eq!(r.timestamp_us, 123_456);
        assert_eq!(r.fault_flags, 0x42);
    }
}

#[test]
fn plugin_init_status_serde_roundtrip() {
    let status = PluginInitStatus::Initialized;
    let json = serde_json::to_string(&status);
    assert!(json.is_ok());
    let json_str = json.as_deref().unwrap_or("");
    let restored: Result<PluginInitStatus, _> = serde_json::from_str(json_str);
    assert!(restored.is_ok());
    if let Ok(r) = restored {
        assert_eq!(r, PluginInitStatus::Initialized);
    }
}

// ---------------------------------------------------------------------------
// Edge cases and fuzz-adjacent tests
// ---------------------------------------------------------------------------

#[test]
fn plugin_header_from_bytes_all_zeros() {
    let bytes = [0u8; 16];
    let header = PluginHeader::from_bytes(&bytes);
    assert!(!header.is_valid());
    assert_eq!(header.magic, 0);
    assert_eq!(header.abi_version, 0);
}

#[test]
fn plugin_header_from_bytes_all_ones() {
    let bytes = [0xFF; 16];
    let header = PluginHeader::from_bytes(&bytes);
    assert!(!header.is_valid());
    assert_eq!(header.magic, u32::MAX);
}

#[test]
fn telemetry_frame_from_bytes_all_zeros() {
    let bytes = [0u8; 32];
    let frame = TelemetryFrame::from_bytes(&bytes);
    assert_eq!(frame.timestamp_us, 0);
    assert_eq!(frame.wheel_angle_deg, 0.0);
    assert_eq!(frame.fault_flags, 0);
}

#[test]
fn telemetry_frame_max_timestamp() {
    let frame = TelemetryFrame::new(u64::MAX);
    assert_eq!(frame.timestamp_us, u64::MAX);
    let bytes = frame.to_bytes();
    let restored = TelemetryFrame::from_bytes(&bytes);
    assert_eq!(restored.timestamp_us, u64::MAX);
}

#[test]
fn plugin_header_has_capability_with_empty_header() {
    let header = PluginHeader::default();
    assert!(!header.has_capability(PluginCapabilities::TELEMETRY));
    assert!(!header.has_capability(PluginCapabilities::LEDS));
    assert!(!header.has_capability(PluginCapabilities::HAPTICS));
}
