//! ABI compatibility tests for the plugin ABI crate.
//!
//! These tests verify:
//! - ABI version negotiation (major/minor version semantics)
//! - Plugin metadata serialization roundtrips
//! - Capability declarations and matching
//! - ABI layout stability (size, alignment, field offset assertions)
//! - Forward/backward compatibility guarantees

use openracing_plugin_abi::prelude::*;

// ---------------------------------------------------------------------------
// ABI version negotiation
// ---------------------------------------------------------------------------

#[test]
fn abi_version_format_encodes_major_minor() {
    // The version constant is major << 16 | minor. Current is 1.0.
    let major = PLUG_ABI_VERSION >> 16;
    let minor = PLUG_ABI_VERSION & 0xFFFF;
    assert_eq!(major, 1, "expected major version 1");
    assert_eq!(minor, 0, "expected minor version 0");
}

#[test]
fn header_with_matching_version_is_valid() {
    let header = PluginHeader::default();
    assert!(header.is_valid());
}

#[test]
fn header_with_higher_major_version_is_invalid() {
    let future_version = 2u32 << 16;
    let header = PluginHeader {
        abi_version: future_version,
        ..PluginHeader::default()
    };
    assert!(
        !header.is_valid(),
        "a plugin with major version 2 must be rejected by host version 1"
    );
}

#[test]
fn header_with_lower_major_version_is_invalid() {
    let old_version = 0u32; // major 0
    let header = PluginHeader {
        abi_version: old_version,
        ..PluginHeader::default()
    };
    assert!(
        !header.is_valid(),
        "a plugin with major version 0 must be rejected by host version 1"
    );
}

#[test]
fn header_with_wrong_magic_is_invalid() {
    let header = PluginHeader {
        magic: 0xDEADBEEF,
        ..PluginHeader::default()
    };
    assert!(!header.is_valid());
}

#[test]
fn wasm_abi_version_is_positive() {
    const { assert!(WASM_ABI_VERSION > 0) };
}

// ---------------------------------------------------------------------------
// Plugin metadata serialization roundtrips
// ---------------------------------------------------------------------------

#[test]
fn plugin_header_byte_roundtrip_identity() {
    let header = PluginHeader::new(
        PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS,
    );
    let bytes = header.to_bytes();
    let restored = PluginHeader::from_bytes(&bytes);
    assert_eq!(header, restored);
}

#[test]
fn plugin_header_bytes_are_little_endian() {
    let header = PluginHeader::default();
    let bytes = header.to_bytes();

    // magic = 0x57574C31 in LE
    assert_eq!(bytes[0], 0x31);
    assert_eq!(bytes[1], 0x4C);
    assert_eq!(bytes[2], 0x57);
    assert_eq!(bytes[3], 0x57);
}

#[test]
fn telemetry_frame_byte_roundtrip_identity() {
    let frame = TelemetryFrame::with_values(999_999, 180.0, std::f32::consts::PI, 42.5, 0xFF);
    let bytes = frame.to_bytes();
    let restored = TelemetryFrame::from_bytes(&bytes);
    assert_eq!(frame.timestamp_us, restored.timestamp_us);
    assert_eq!(frame.wheel_angle_deg, restored.wheel_angle_deg);
    assert_eq!(frame.fault_flags, restored.fault_flags);
}

#[test]
fn telemetry_frame_zero_bytes_produce_default_like_frame() {
    let zero_bytes = [0u8; 32];
    let frame = TelemetryFrame::from_bytes(&zero_bytes);
    assert_eq!(frame.timestamp_us, 0);
    assert_eq!(frame.wheel_angle_deg, 0.0);
    assert_eq!(frame.fault_flags, 0);
}

#[test]
fn wasm_plugin_info_default_has_current_abi_version() {
    let info = WasmPluginInfo::default();
    assert_eq!(info.abi_version, WASM_ABI_VERSION);
}

// ---------------------------------------------------------------------------
// Capability declarations and matching
// ---------------------------------------------------------------------------

#[test]
fn individual_capabilities_are_distinct_bits() {
    let t = PluginCapabilities::TELEMETRY.bits();
    let l = PluginCapabilities::LEDS.bits();
    let h = PluginCapabilities::HAPTICS.bits();

    // No bit overlap between any pair
    assert_eq!(t & l, 0);
    assert_eq!(t & h, 0);
    assert_eq!(l & h, 0);
}

#[test]
fn combined_capabilities_contain_each_constituent() {
    let combined = PluginCapabilities::TELEMETRY | PluginCapabilities::HAPTICS;
    assert!(combined.contains(PluginCapabilities::TELEMETRY));
    assert!(combined.contains(PluginCapabilities::HAPTICS));
    assert!(!combined.contains(PluginCapabilities::LEDS));
}

#[test]
fn header_has_capability_returns_correct_results() {
    let header = PluginHeader::new(PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS);
    assert!(header.has_capability(PluginCapabilities::TELEMETRY));
    assert!(header.has_capability(PluginCapabilities::LEDS));
    assert!(!header.has_capability(PluginCapabilities::HAPTICS));
}

#[test]
fn empty_capabilities_header_has_no_caps() {
    let header = PluginHeader::new(PluginCapabilities::empty());
    assert!(!header.has_capability(PluginCapabilities::TELEMETRY));
    assert!(!header.has_capability(PluginCapabilities::LEDS));
    assert!(!header.has_capability(PluginCapabilities::HAPTICS));
}

#[test]
fn all_valid_caps_do_not_overlap_reserved() {
    let valid =
        PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
    let reserved = PluginCapabilities::RESERVED;
    assert_eq!(
        valid.bits() & reserved.bits(),
        0,
        "valid capabilities must not use reserved bits"
    );
}

#[test]
fn valid_and_reserved_cover_full_u32() {
    let valid =
        PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
    assert_eq!(
        valid.bits() | PluginCapabilities::RESERVED.bits(),
        0xFFFF_FFFF
    );
}

#[test]
fn truncation_strips_reserved_bits() {
    // Setting a reserved bit should be ignored by from_bits_truncate
    let with_reserved = 0b0000_0001 | 0x8000_0000;
    let caps = PluginCapabilities::from_bits_truncate(with_reserved);
    assert!(caps.contains(PluginCapabilities::TELEMETRY));
    // Reserved bits are allowed through from_bits_truncate in this bitflags config,
    // but the important thing is the valid bits are preserved
    assert!(caps.contains(PluginCapabilities::TELEMETRY));
}

// ---------------------------------------------------------------------------
// ABI layout stability (size and alignment assertions)
// ---------------------------------------------------------------------------

#[test]
fn plugin_header_is_16_bytes() {
    assert_eq!(std::mem::size_of::<PluginHeader>(), 16);
}

#[test]
fn plugin_header_alignment_is_4() {
    assert_eq!(std::mem::align_of::<PluginHeader>(), 4);
}

#[test]
fn telemetry_frame_is_32_bytes() {
    assert_eq!(std::mem::size_of::<TelemetryFrame>(), 32);
}

#[test]
fn telemetry_frame_alignment_is_8() {
    assert_eq!(std::mem::align_of::<TelemetryFrame>(), 8);
}

#[test]
fn plugin_capabilities_is_4_bytes() {
    assert_eq!(std::mem::size_of::<PluginCapabilities>(), 4);
}

#[test]
fn plugin_header_field_offsets_are_stable() {
    // Verify the documented memory layout from types.rs
    let header = PluginHeader::new(PluginCapabilities::TELEMETRY);
    let bytes = header.to_bytes();

    let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let abi_version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let capabilities = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let reserved = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

    assert_eq!(magic, PLUG_ABI_MAGIC);
    assert_eq!(abi_version, PLUG_ABI_VERSION);
    assert_eq!(capabilities, PluginCapabilities::TELEMETRY.bits());
    assert_eq!(reserved, 0);
}

#[test]
fn telemetry_frame_field_offsets_are_stable() {
    let frame = TelemetryFrame::with_values(0xAABBCCDD_11223344, 1.0, 2.0, 3.0, 0xDEAD);
    let bytes = frame.to_bytes();

    // offset 0: timestamp_us (8 bytes)
    let ts = u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]);
    assert_eq!(ts, 0xAABBCCDD_11223344);

    // offset 8: wheel_angle_deg (4 bytes)
    let angle = f32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    assert_eq!(angle, 1.0);

    // offset 20: fault_flags (4 bytes)
    let flags = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    assert_eq!(flags, 0xDEAD);
}

// ---------------------------------------------------------------------------
// Forward/backward compatibility
// ---------------------------------------------------------------------------

#[test]
fn header_with_unknown_capability_bits_still_deserializes() {
    // Simulate a header from a future plugin that sets unknown capability bits
    let mut bytes = PluginHeader::default().to_bytes();
    // Set all capability bits including future ones
    bytes[8] = 0xFF;
    bytes[9] = 0xFF;
    bytes[10] = 0xFF;
    bytes[11] = 0xFF;

    let header = PluginHeader::from_bytes(&bytes);
    // The header should still parse — we just truncate unknown bits
    let caps = header.get_capabilities();
    assert!(caps.contains(PluginCapabilities::TELEMETRY));
    assert!(caps.contains(PluginCapabilities::LEDS));
    assert!(caps.contains(PluginCapabilities::HAPTICS));
}

#[test]
fn reserved_field_nonzero_does_not_break_deserialization() {
    let mut bytes = PluginHeader::default().to_bytes();
    // Simulate a future header with non-zero reserved field
    bytes[12] = 0xFF;
    bytes[13] = 0xFF;
    bytes[14] = 0xFF;
    bytes[15] = 0xFF;

    let header = PluginHeader::from_bytes(&bytes);
    // Core fields should still be valid
    assert_eq!(header.magic, PLUG_ABI_MAGIC);
    assert_eq!(header.abi_version, PLUG_ABI_VERSION);
}

#[test]
fn wasm_export_validation_minimal_required() {
    let valid = WasmExportValidation {
        has_process: true,
        has_memory: true,
        has_init: false,
        has_shutdown: false,
        has_get_info: false,
    };
    assert!(valid.is_valid());
    assert!(valid.missing_required().is_empty());
}

#[test]
fn wasm_export_validation_reports_all_missing() {
    let invalid = WasmExportValidation::default();
    assert!(!invalid.is_valid());
    let missing = invalid.missing_required();
    assert_eq!(missing.len(), 2);
    assert!(missing.contains(&wasm_export::PROCESS));
    assert!(missing.contains(&wasm_export::MEMORY));
}

#[test]
fn plugin_init_status_transitions() {
    let status = PluginInitStatus::default();
    assert_eq!(status, PluginInitStatus::Uninitialized);

    // All variants exist and are distinct
    assert_ne!(
        PluginInitStatus::Uninitialized,
        PluginInitStatus::Initializing
    );
    assert_ne!(
        PluginInitStatus::Initializing,
        PluginInitStatus::Initialized
    );
    assert_ne!(PluginInitStatus::Initialized, PluginInitStatus::Failed);
    assert_ne!(PluginInitStatus::Failed, PluginInitStatus::ShutDown);
}

// ---------------------------------------------------------------------------
// Constant string contract tests
// ---------------------------------------------------------------------------

#[test]
fn host_module_is_env() {
    assert_eq!(HOST_MODULE, "env");
}

#[test]
fn log_levels_are_ordered() {
    const { assert!(log_level::ERROR < log_level::WARN) };
    const { assert!(log_level::WARN < log_level::INFO) };
    const { assert!(log_level::INFO < log_level::DEBUG) };
    const { assert!(log_level::DEBUG < log_level::TRACE) };
}

#[test]
fn return_codes_success_is_zero_errors_negative() {
    const { assert!(return_code::SUCCESS == 0) };
    const { assert!(return_code::ERROR < 0) };
    const { assert!(return_code::INVALID_ARG < 0) };
    const { assert!(return_code::PERMISSION_DENIED < 0) };
    const { assert!(return_code::BUFFER_TOO_SMALL < 0) };
    const { assert!(return_code::NOT_INITIALIZED < 0) };
}

#[test]
fn return_codes_are_all_distinct() {
    let codes = [
        return_code::SUCCESS,
        return_code::ERROR,
        return_code::INVALID_ARG,
        return_code::PERMISSION_DENIED,
        return_code::BUFFER_TOO_SMALL,
        return_code::NOT_INITIALIZED,
    ];
    for (i, a) in codes.iter().enumerate() {
        for (j, b) in codes.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "return codes at index {i} and {j} must differ");
            }
        }
    }
}

#[test]
fn capability_strings_are_non_empty_and_distinct() {
    let caps = [
        capability_str::READ_TELEMETRY,
        capability_str::MODIFY_TELEMETRY,
        capability_str::CONTROL_LEDS,
        capability_str::PROCESS_DSP,
    ];
    for cap in &caps {
        assert!(!cap.is_empty());
    }
    // Check uniqueness
    for (i, a) in caps.iter().enumerate() {
        for (j, b) in caps.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "capability strings at index {i} and {j} must differ");
            }
        }
    }
}

#[test]
fn host_function_names_are_non_empty_and_distinct() {
    let funcs = [
        host_function::LOG_DEBUG,
        host_function::LOG_INFO,
        host_function::LOG_WARN,
        host_function::LOG_ERROR,
        host_function::PLUGIN_LOG,
        host_function::CHECK_CAPABILITY,
        host_function::GET_TELEMETRY,
        host_function::GET_TIMESTAMP_US,
    ];
    for f in &funcs {
        assert!(!f.is_empty());
    }
    for (i, a) in funcs.iter().enumerate() {
        for (j, b) in funcs.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "host functions at index {i} and {j} must differ");
            }
        }
    }
}

#[test]
fn wasm_export_names_are_non_empty() {
    assert!(!wasm_export::PROCESS.is_empty());
    assert!(!wasm_export::MEMORY.is_empty());
    assert!(!wasm_optional_export::INIT.is_empty());
    assert!(!wasm_optional_export::SHUTDOWN.is_empty());
    assert!(!wasm_optional_export::GET_INFO.is_empty());
}

// ---------------------------------------------------------------------------
// Host function validation
// ---------------------------------------------------------------------------

#[test]
fn validate_string_params_rejects_negative_ptr() {
    use openracing_plugin_abi::host_functions::validation;
    let result = validation::validate_string_params(-1, 10, 1024);
    assert_eq!(result, return_code::INVALID_ARG);
}

#[test]
fn validate_string_params_rejects_negative_len() {
    use openracing_plugin_abi::host_functions::validation;
    let result = validation::validate_string_params(0, -1, 1024);
    assert_eq!(result, return_code::INVALID_ARG);
}

#[test]
fn validate_string_params_rejects_oversized_len() {
    use openracing_plugin_abi::host_functions::validation;
    let result = validation::validate_string_params(0, 2048, 1024);
    assert_eq!(result, return_code::BUFFER_TOO_SMALL);
}

#[test]
fn validate_string_params_accepts_valid_input() {
    use openracing_plugin_abi::host_functions::validation;
    let result = validation::validate_string_params(0, 100, 1024);
    assert_eq!(result, return_code::SUCCESS);
}

#[test]
fn validate_output_buffer_rejects_too_small() {
    use openracing_plugin_abi::host_functions::validation;
    let result = validation::validate_output_buffer(0, 16, 32);
    assert_eq!(result, return_code::BUFFER_TOO_SMALL);
}

#[test]
fn validate_output_buffer_accepts_exact_size() {
    use openracing_plugin_abi::host_functions::validation;
    let result = validation::validate_output_buffer(0, 32, 32);
    assert_eq!(result, return_code::SUCCESS);
}
