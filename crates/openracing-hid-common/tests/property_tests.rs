//! Property-based tests for the openracing-hid-common crate.
//!
//! Uses proptest to verify invariants on ReportParser, ReportBuilder,
//! HidDeviceInfo, and error types across randomized inputs.

use openracing_hid_common::{HidCommonError, HidDeviceInfo, ReportBuilder, ReportParser};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// ReportParser – round-trip property tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Writing then reading a u8 must produce the original value.
    #[test]
    fn prop_u8_roundtrip(val in any::<u8>()) {
        let mut builder = ReportBuilder::with_capacity(1);
        builder.write_u8(val);
        let mut parser = ReportParser::new(builder.into_inner());
        let parsed = parser.read_u8().map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(parsed, val);
    }

    /// Writing then reading an i8 must produce the original value.
    #[test]
    fn prop_i8_roundtrip(val in any::<i8>()) {
        let mut builder = ReportBuilder::with_capacity(1);
        builder.write_i8(val);
        let mut parser = ReportParser::new(builder.into_inner());
        let parsed = parser.read_i8().map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(parsed, val);
    }

    /// Writing then reading a u16 LE must produce the original value.
    #[test]
    fn prop_u16_le_roundtrip(val in any::<u16>()) {
        let mut builder = ReportBuilder::with_capacity(2);
        builder.write_u16_le(val);
        let mut parser = ReportParser::new(builder.into_inner());
        let parsed = parser.read_u16_le().map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(parsed, val);
    }

    /// Writing then reading an i16 LE must produce the original value.
    #[test]
    fn prop_i16_le_roundtrip(val in any::<i16>()) {
        let mut builder = ReportBuilder::with_capacity(2);
        builder.write_i16_le(val);
        let mut parser = ReportParser::new(builder.into_inner());
        let parsed = parser.read_i16_le().map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(parsed, val);
    }

    /// Writing then reading a u32 LE must produce the original value.
    #[test]
    fn prop_u32_le_roundtrip(val in any::<u32>()) {
        let mut builder = ReportBuilder::with_capacity(4);
        builder.write_u32_le(val);
        let mut parser = ReportParser::new(builder.into_inner());
        let parsed = parser.read_u32_le().map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(parsed, val);
    }

    /// Writing then reading an f32 LE must produce the original value (bit-exact for non-NaN).
    #[test]
    fn prop_f32_le_roundtrip(val in any::<f32>().prop_filter("skip NaN", |v| !v.is_nan())) {
        let mut builder = ReportBuilder::with_capacity(4);
        builder.write_f32_le(val);
        let mut parser = ReportParser::new(builder.into_inner());
        let parsed = parser.read_f32_le().map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(parsed.to_bits(), val.to_bits());
    }

    /// Writing then reading a byte slice must produce the original bytes.
    #[test]
    fn prop_bytes_roundtrip(data in proptest::collection::vec(any::<u8>(), 0..128)) {
        let mut builder = ReportBuilder::with_capacity(data.len());
        builder.write_bytes(&data);
        let mut parser = ReportParser::new(builder.into_inner());
        let parsed = parser.read_bytes(data.len()).map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(parsed, data);
    }

    /// ReportParser::remaining must always equal buffer length minus consumed bytes.
    #[test]
    fn prop_remaining_decreases_correctly(data in proptest::collection::vec(any::<u8>(), 1..64)) {
        let len = data.len();
        let mut parser = ReportParser::new(data);
        prop_assert_eq!(parser.remaining(), len);
        if len >= 1 {
            let _ = parser.read_u8();
            prop_assert_eq!(parser.remaining(), len - 1);
        }
    }

    /// ReportParser::peek_u8 must not change remaining().
    #[test]
    fn prop_peek_does_not_consume(data in proptest::collection::vec(any::<u8>(), 1..64)) {
        let mut parser = ReportParser::new(data);
        let before = parser.remaining();
        let _ = parser.peek_u8();
        prop_assert_eq!(parser.remaining(), before);
    }

    /// ReportParser::skip(n) then remaining() must equal max(0, len - n).
    #[test]
    fn prop_skip_clamps_correctly(
        data in proptest::collection::vec(any::<u8>(), 0..64),
        skip in 0usize..128,
    ) {
        let len = data.len();
        let mut parser = ReportParser::new(data);
        parser.skip(skip);
        let expected = len.saturating_sub(skip);
        prop_assert_eq!(parser.remaining(), expected);
    }

    /// ReportParser::reset must restore remaining to full buffer length.
    #[test]
    fn prop_reset_restores_position(
        data in proptest::collection::vec(any::<u8>(), 1..64),
        consume in 0usize..64,
    ) {
        let len = data.len();
        let mut parser = ReportParser::new(data);
        parser.skip(consume);
        parser.reset();
        prop_assert_eq!(parser.remaining(), len);
    }

    /// ReportBuilder::len must equal the number of bytes written.
    #[test]
    fn prop_builder_len_tracks_writes(count in 0usize..64) {
        let mut builder = ReportBuilder::with_capacity(count);
        for _ in 0..count {
            builder.write_u8(0xAA);
        }
        prop_assert_eq!(builder.len(), count);
    }

    /// ReportBuilder::is_empty must be true only when len == 0.
    #[test]
    fn prop_builder_is_empty_consistent(count in 0usize..64) {
        let mut builder = ReportBuilder::with_capacity(count);
        prop_assert!(builder.is_empty());
        for _ in 0..count {
            builder.write_u8(0x00);
        }
        prop_assert_eq!(builder.is_empty(), count == 0);
    }

    /// A multi-field write/read round-trip must preserve all values.
    #[test]
    fn prop_multi_field_roundtrip(
        a in any::<u8>(),
        b in any::<u16>(),
        c in any::<i16>(),
        d in any::<u32>(),
    ) {
        let mut builder = ReportBuilder::with_capacity(9);
        builder.write_u8(a).write_u16_le(b).write_i16_le(c).write_u32_le(d);
        let mut parser = ReportParser::new(builder.into_inner());
        let pa = parser.read_u8().map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        let pb = parser.read_u16_le().map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        let pc = parser.read_i16_le().map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        let pd = parser.read_u32_le().map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(pa, a);
        prop_assert_eq!(pb, b);
        prop_assert_eq!(pc, c);
        prop_assert_eq!(pd, d);
    }

    /// HidDeviceInfo::matches must return true iff both VID and PID match.
    #[test]
    fn prop_device_info_matches(vid in any::<u16>(), pid in any::<u16>(), other_vid in any::<u16>(), other_pid in any::<u16>()) {
        let info = HidDeviceInfo::new(vid, pid, "test".to_string());
        prop_assert_eq!(info.matches(vid, pid), true);
        if other_vid != vid || other_pid != pid {
            prop_assert_eq!(info.matches(other_vid, other_pid), false);
        }
    }

    /// HidDeviceInfo::display_name must be non-empty for any VID/PID.
    #[test]
    fn prop_device_info_display_name_non_empty(vid in any::<u16>(), pid in any::<u16>()) {
        let info = HidDeviceInfo::new(vid, pid, "test".to_string());
        prop_assert!(!info.display_name().is_empty());
    }
}

// ---------------------------------------------------------------------------
// Error type invariants
// ---------------------------------------------------------------------------

#[test]
fn error_io_from_conversion() {
    let io_err = std::io::Error::other("pipe broken");
    let hid_err: HidCommonError = io_err.into();
    let msg = hid_err.to_string();
    assert!(msg.contains("pipe broken"), "message: {msg}");
}

#[test]
fn error_all_variants_display_non_empty() {
    let variants: Vec<HidCommonError> = vec![
        HidCommonError::DeviceNotFound("dev".to_string()),
        HidCommonError::OpenError("open".to_string()),
        HidCommonError::ReadError("read".to_string()),
        HidCommonError::WriteError("write".to_string()),
        HidCommonError::InvalidReport("inv".to_string()),
        HidCommonError::Disconnected,
        HidCommonError::IoError(std::io::Error::other("io")),
    ];
    for err in &variants {
        assert!(
            !err.to_string().is_empty(),
            "Display must be non-empty for {err:?}"
        );
    }
}

#[test]
fn error_debug_format_non_empty() {
    let err = HidCommonError::DeviceNotFound("x".to_string());
    assert!(!format!("{err:?}").is_empty());
}

// ---------------------------------------------------------------------------
// ReportBuilder edge cases
// ---------------------------------------------------------------------------

#[test]
fn builder_default_capacity_is_64() {
    let builder = ReportBuilder::default();
    // Default pre-allocates 64 zero bytes
    assert_eq!(builder.len(), 64);
}

#[test]
fn builder_with_capacity_starts_empty() {
    let builder = ReportBuilder::with_capacity(64);
    assert!(builder.is_empty());
    assert_eq!(builder.len(), 0);
}

#[test]
fn builder_as_slice_matches_into_inner() {
    let mut builder = ReportBuilder::with_capacity(4);
    builder.write_u8(0x01).write_u16_le(0x0203);
    let slice = builder.as_slice().to_vec();
    let inner = builder.into_inner();
    assert_eq!(slice, inner);
}

// ---------------------------------------------------------------------------
// HidDeviceInfo builder chain
// ---------------------------------------------------------------------------

#[test]
fn device_info_builder_chain() {
    let info = HidDeviceInfo::new(0x1234, 0x5678, "/dev/hid0".to_string())
        .with_serial("SN-001")
        .with_manufacturer("Acme")
        .with_product_name("Widget");
    assert_eq!(info.serial_number.as_deref(), Some("SN-001"));
    assert_eq!(info.manufacturer.as_deref(), Some("Acme"));
    assert_eq!(info.product_name.as_deref(), Some("Widget"));
    assert_eq!(info.display_name(), "Widget");
}

#[test]
fn device_info_display_name_priority() {
    // product_name takes priority over manufacturer
    let info = HidDeviceInfo::new(0, 0, String::new())
        .with_manufacturer("Mfr")
        .with_product_name("Product");
    assert_eq!(info.display_name(), "Product");

    // manufacturer is fallback when no product_name
    let info2 = HidDeviceInfo::new(0, 0, String::new()).with_manufacturer("Mfr");
    assert_eq!(info2.display_name(), "Mfr");

    // hex VID:PID when neither is set
    let info3 = HidDeviceInfo::new(0x00AB, 0x00CD, String::new());
    assert_eq!(info3.display_name(), "00ab:00cd");
}

#[test]
fn device_info_serde_roundtrip() -> Result<(), serde_json::Error> {
    let info = HidDeviceInfo::new(0x1234, 0x5678, "/dev/hid".to_string())
        .with_serial("SN")
        .with_manufacturer("Mfr")
        .with_product_name("Prod");
    let json = serde_json::to_string(&info)?;
    let parsed: HidDeviceInfo = serde_json::from_str(&json)?;
    assert_eq!(parsed.vendor_id, info.vendor_id);
    assert_eq!(parsed.product_id, info.product_id);
    assert_eq!(parsed.serial_number, info.serial_number);
    assert_eq!(parsed.manufacturer, info.manufacturer);
    assert_eq!(parsed.product_name, info.product_name);
    assert_eq!(parsed.path, info.path);
    Ok(())
}
