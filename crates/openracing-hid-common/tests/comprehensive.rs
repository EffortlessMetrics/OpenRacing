//! Comprehensive tests for the openracing-hid-common crate.
//!
//! Covers report parsing, report building, round-trip encoding/decoding,
//! device info, mock HID devices, and error handling.

use openracing_hid_common::{
    HidCommonError, HidDevice, HidDeviceInfo, HidPort, ReportBuilder, ReportParser,
    hid_traits::mock::{MockHidDevice, MockHidPort},
};

// ---------------------------------------------------------------------------
// ReportParser — construction and basic reads
// ---------------------------------------------------------------------------

#[test]
fn parser_empty_buffer_read_u8_fails() {
    let mut parser = ReportParser::new(vec![]);
    assert!(parser.read_u8().is_err());
}

#[test]
fn parser_empty_buffer_remaining_is_zero() {
    let parser = ReportParser::new(vec![]);
    assert_eq!(parser.remaining(), 0);
}

#[test]
fn parser_from_slice_reads_correctly() -> Result<(), HidCommonError> {
    let data: &[u8] = &[0xDE, 0xAD];
    let mut parser = ReportParser::from_slice(data);
    assert_eq!(parser.read_u8()?, 0xDE);
    assert_eq!(parser.read_u8()?, 0xAD);
    Ok(())
}

#[test]
fn parser_read_i8_positive_and_negative() -> Result<(), HidCommonError> {
    // 0x7F = 127, 0x80 = -128, 0xFF = -1
    let mut parser = ReportParser::new(vec![0x7F, 0x80, 0xFF]);
    assert_eq!(parser.read_i8()?, 127);
    assert_eq!(parser.read_i8()?, -128);
    assert_eq!(parser.read_i8()?, -1);
    Ok(())
}

#[test]
fn parser_read_i16_le_negative() -> Result<(), HidCommonError> {
    // -1 in little-endian i16 = [0xFF, 0xFF]
    let mut parser = ReportParser::new(vec![0xFF, 0xFF]);
    assert_eq!(parser.read_i16_le()?, -1);
    Ok(())
}

#[test]
fn parser_read_i16_le_min_max() -> Result<(), HidCommonError> {
    // i16::MIN = -32768 = 0x8000 LE = [0x00, 0x80]
    // i16::MAX = 32767 = 0x7FFF LE = [0xFF, 0x7F]
    let mut parser = ReportParser::new(vec![0x00, 0x80, 0xFF, 0x7F]);
    assert_eq!(parser.read_i16_le()?, i16::MIN);
    assert_eq!(parser.read_i16_le()?, i16::MAX);
    Ok(())
}

#[test]
fn parser_read_u16_be() -> Result<(), HidCommonError> {
    // 0x1234 big-endian = [0x12, 0x34]
    let mut parser = ReportParser::new(vec![0x12, 0x34]);
    assert_eq!(parser.read_u16_be()?, 0x1234);
    Ok(())
}

#[test]
fn parser_read_u16_be_boundary() -> Result<(), HidCommonError> {
    let mut parser = ReportParser::new(vec![0xFF, 0xFF, 0x00, 0x00]);
    assert_eq!(parser.read_u16_be()?, 0xFFFF);
    assert_eq!(parser.read_u16_be()?, 0x0000);
    Ok(())
}

#[test]
fn parser_read_i32_le_negative() -> Result<(), HidCommonError> {
    // -1 in LE i32 = [0xFF, 0xFF, 0xFF, 0xFF]
    let mut parser = ReportParser::new(vec![0xFF, 0xFF, 0xFF, 0xFF]);
    assert_eq!(parser.read_i32_le()?, -1);
    Ok(())
}

#[test]
fn parser_read_i32_le_min_max() -> Result<(), HidCommonError> {
    // i32::MIN LE = [0x00, 0x00, 0x00, 0x80]
    // i32::MAX LE = [0xFF, 0xFF, 0xFF, 0x7F]
    let mut parser = ReportParser::new(vec![0x00, 0x00, 0x00, 0x80, 0xFF, 0xFF, 0xFF, 0x7F]);
    assert_eq!(parser.read_i32_le()?, i32::MIN);
    assert_eq!(parser.read_i32_le()?, i32::MAX);
    Ok(())
}

#[test]
fn parser_read_f32_le_special_values() -> Result<(), HidCommonError> {
    // Test 0.0
    let mut parser = ReportParser::new(0.0_f32.to_le_bytes().to_vec());
    let val = parser.read_f32_le()?;
    assert!((val - 0.0).abs() < f32::EPSILON);

    // Test -1.0
    let mut parser = ReportParser::new((-1.0_f32).to_le_bytes().to_vec());
    let val = parser.read_f32_le()?;
    assert!((val - (-1.0)).abs() < f32::EPSILON);

    // Test infinity
    let mut parser = ReportParser::new(f32::INFINITY.to_le_bytes().to_vec());
    let val = parser.read_f32_le()?;
    assert!(val.is_infinite() && val.is_sign_positive());

    // Test NaN
    let mut parser = ReportParser::new(f32::NAN.to_le_bytes().to_vec());
    let val = parser.read_f32_le()?;
    assert!(val.is_nan());

    Ok(())
}

// ---------------------------------------------------------------------------
// ReportParser — navigation (remaining, peek, skip, reset)
// ---------------------------------------------------------------------------

#[test]
fn parser_remaining_tracks_reads() -> Result<(), HidCommonError> {
    let mut parser = ReportParser::new(vec![0x01, 0x02, 0x03, 0x04, 0x05]);
    assert_eq!(parser.remaining(), 5);
    parser.read_u8()?;
    assert_eq!(parser.remaining(), 4);
    parser.read_u16_le()?;
    assert_eq!(parser.remaining(), 2);
    parser.read_bytes(2)?;
    assert_eq!(parser.remaining(), 0);
    Ok(())
}

#[test]
fn parser_peek_does_not_advance() -> Result<(), HidCommonError> {
    let mut parser = ReportParser::new(vec![0xAB, 0xCD]);
    assert_eq!(parser.peek_u8()?, 0xAB);
    assert_eq!(parser.peek_u8()?, 0xAB);
    assert_eq!(parser.remaining(), 2);
    // After read, peek should show next byte
    parser.read_u8()?;
    assert_eq!(parser.peek_u8()?, 0xCD);
    Ok(())
}

#[test]
fn parser_peek_on_empty_fails() {
    let mut parser = ReportParser::new(vec![]);
    assert!(parser.peek_u8().is_err());
}

#[test]
fn parser_skip_advances_position() -> Result<(), HidCommonError> {
    let mut parser = ReportParser::new(vec![0x01, 0x02, 0x03, 0x04]);
    parser.skip(2);
    assert_eq!(parser.remaining(), 2);
    assert_eq!(parser.read_u8()?, 0x03);
    Ok(())
}

#[test]
fn parser_skip_past_end_clamps() {
    let mut parser = ReportParser::new(vec![0x01, 0x02]);
    parser.skip(100);
    assert_eq!(parser.remaining(), 0);
    assert!(parser.read_u8().is_err());
}

#[test]
fn parser_reset_returns_to_start() -> Result<(), HidCommonError> {
    let mut parser = ReportParser::new(vec![0xAA, 0xBB]);
    parser.read_u8()?;
    assert_eq!(parser.remaining(), 1);
    parser.reset();
    assert_eq!(parser.remaining(), 2);
    assert_eq!(parser.read_u8()?, 0xAA);
    Ok(())
}

#[test]
fn parser_into_inner_returns_full_buffer() {
    let data = vec![0x01, 0x02, 0x03];
    let mut parser = ReportParser::new(data.clone());
    let _ = parser.read_u8();
    // into_inner returns full buffer regardless of position
    assert_eq!(parser.into_inner(), data);
}

#[test]
fn parser_slice_returns_full_buffer() -> Result<(), HidCommonError> {
    let data = vec![0x10, 0x20, 0x30];
    let mut parser = ReportParser::new(data.clone());
    parser.read_u8()?;
    assert_eq!(parser.slice(), &data[..]);
    Ok(())
}

// ---------------------------------------------------------------------------
// ReportParser — error handling for malformed / truncated reports
// ---------------------------------------------------------------------------

#[test]
fn parser_truncated_u16_single_byte() {
    let mut parser = ReportParser::new(vec![0x01]);
    let result = parser.read_u16_le();
    assert!(result.is_err());
    match result {
        Err(HidCommonError::InvalidReport(msg)) => {
            assert!(msg.contains("end of data"), "unexpected message: {msg}");
        }
        other => panic!("expected InvalidReport, got: {other:?}"),
    }
}

#[test]
fn parser_truncated_u16_be_single_byte() {
    let mut parser = ReportParser::new(vec![0x01]);
    assert!(parser.read_u16_be().is_err());
}

#[test]
fn parser_truncated_u32_two_bytes() {
    let mut parser = ReportParser::new(vec![0x01, 0x02]);
    assert!(parser.read_u32_le().is_err());
}

#[test]
fn parser_truncated_u32_three_bytes() {
    let mut parser = ReportParser::new(vec![0x01, 0x02, 0x03]);
    assert!(parser.read_u32_le().is_err());
}

#[test]
fn parser_truncated_f32() {
    let mut parser = ReportParser::new(vec![0x01, 0x02]);
    assert!(parser.read_f32_le().is_err());
}

#[test]
fn parser_read_bytes_past_end() {
    let mut parser = ReportParser::new(vec![0x01, 0x02]);
    let result = parser.read_bytes(5);
    assert!(result.is_err());
}

#[test]
fn parser_read_bytes_exact_boundary() -> Result<(), HidCommonError> {
    let mut parser = ReportParser::new(vec![0x01, 0x02, 0x03]);
    let bytes = parser.read_bytes(3)?;
    assert_eq!(bytes, vec![0x01, 0x02, 0x03]);
    assert_eq!(parser.remaining(), 0);
    // Next read should fail
    assert!(parser.read_bytes(1).is_err());
    Ok(())
}

#[test]
fn parser_sequential_reads_exhaust_buffer() {
    let mut parser = ReportParser::new(vec![0x01, 0x02, 0x03]);
    assert!(parser.read_u8().is_ok());
    assert!(parser.read_u8().is_ok());
    assert!(parser.read_u8().is_ok());
    assert!(parser.read_u8().is_err());
    assert!(parser.read_i8().is_err());
    assert!(parser.read_u16_le().is_err());
    assert!(parser.read_i16_le().is_err());
    assert!(parser.read_u16_be().is_err());
    assert!(parser.read_u32_le().is_err());
    assert!(parser.read_i32_le().is_err());
    assert!(parser.read_f32_le().is_err());
}

// ---------------------------------------------------------------------------
// ReportParser — various report sizes
// ---------------------------------------------------------------------------

#[test]
fn parser_single_byte_report() -> Result<(), HidCommonError> {
    let mut parser = ReportParser::new(vec![0x42]);
    assert_eq!(parser.remaining(), 1);
    assert_eq!(parser.read_u8()?, 0x42);
    assert_eq!(parser.remaining(), 0);
    Ok(())
}

#[test]
fn parser_large_report_256_bytes() -> Result<(), HidCommonError> {
    let data: Vec<u8> = (0..=255).collect();
    let mut parser = ReportParser::new(data);
    assert_eq!(parser.remaining(), 256);
    for i in 0..=255u8 {
        assert_eq!(parser.read_u8()?, i);
    }
    assert_eq!(parser.remaining(), 0);
    Ok(())
}

#[test]
fn parser_large_report_mixed_reads() -> Result<(), HidCommonError> {
    // Simulate a realistic 64-byte HID report
    let mut builder = ReportBuilder::with_capacity(64);
    builder
        .write_u8(0x01) // report ID
        .write_u16_le(0x1234) // steering angle
        .write_u16_le(0x5678) // throttle
        .write_u16_le(0x9ABC) // brake
        .write_u8(0xFF) // buttons byte 1
        .write_u8(0x00) // buttons byte 2
        .write_u32_le(0xDEADBEEF); // timestamp

    let report = builder.into_inner();
    let mut parser = ReportParser::new(report);

    assert_eq!(parser.read_u8()?, 0x01); // report ID
    assert_eq!(parser.read_u16_le()?, 0x1234); // steering
    assert_eq!(parser.read_u16_le()?, 0x5678); // throttle
    assert_eq!(parser.read_u16_le()?, 0x9ABC); // brake
    assert_eq!(parser.read_u8()?, 0xFF); // buttons 1
    assert_eq!(parser.read_u8()?, 0x00); // buttons 2
    assert_eq!(parser.read_u32_le()?, 0xDEADBEEF); // timestamp
    Ok(())
}

// ---------------------------------------------------------------------------
// ReportBuilder — construction and writes
// ---------------------------------------------------------------------------

#[test]
fn builder_with_capacity_starts_empty() {
    let builder = ReportBuilder::with_capacity(64);
    assert!(builder.is_empty());
    assert_eq!(builder.len(), 0);
}

#[test]
fn builder_default_has_64_zero_bytes() {
    let builder = ReportBuilder::default();
    assert_eq!(builder.len(), 64);
    assert!(!builder.is_empty());
    assert!(builder.as_slice().iter().all(|&b| b == 0));
}

#[test]
fn builder_new_zero_is_empty() {
    let builder = ReportBuilder::new(0);
    assert!(builder.is_empty());
    assert_eq!(builder.len(), 0);
}

#[test]
fn builder_write_i8() {
    let mut builder = ReportBuilder::with_capacity(4);
    builder.write_i8(-1).write_i8(127).write_i8(-128);
    let data = builder.into_inner();
    assert_eq!(data, vec![0xFF, 0x7F, 0x80]);
}

#[test]
fn builder_write_i16_le() {
    let mut builder = ReportBuilder::with_capacity(4);
    builder.write_i16_le(-1).write_i16_le(i16::MIN);
    let data = builder.into_inner();
    assert_eq!(data, vec![0xFF, 0xFF, 0x00, 0x80]);
}

#[test]
fn builder_write_f32_le() -> Result<(), HidCommonError> {
    let mut builder = ReportBuilder::with_capacity(4);
    builder.write_f32_le(std::f32::consts::PI);
    let data = builder.into_inner();

    let mut parser = ReportParser::new(data);
    let val = parser.read_f32_le()?;
    assert!((val - std::f32::consts::PI).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_as_slice_matches_into_inner() {
    let mut builder = ReportBuilder::with_capacity(8);
    builder.write_u8(0xAA).write_u16_le(0xBBCC);
    let slice_copy = builder.as_slice().to_vec();
    let inner = builder.into_inner();
    assert_eq!(slice_copy, inner);
}

#[test]
fn builder_len_tracks_writes() {
    let mut builder = ReportBuilder::with_capacity(16);
    assert_eq!(builder.len(), 0);
    builder.write_u8(0x01);
    assert_eq!(builder.len(), 1);
    builder.write_u16_le(0x1234);
    assert_eq!(builder.len(), 3);
    builder.write_u32_le(0x12345678);
    assert_eq!(builder.len(), 7);
    builder.write_bytes(&[0x01, 0x02, 0x03]);
    assert_eq!(builder.len(), 10);
}

// ---------------------------------------------------------------------------
// Round-trip: build → parse
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_u8() -> Result<(), HidCommonError> {
    let values: Vec<u8> = vec![0x00, 0x01, 0x7F, 0x80, 0xFE, 0xFF];
    let mut builder = ReportBuilder::with_capacity(values.len());
    for &v in &values {
        builder.write_u8(v);
    }
    let mut parser = ReportParser::new(builder.into_inner());
    for &expected in &values {
        assert_eq!(parser.read_u8()?, expected);
    }
    Ok(())
}

#[test]
fn roundtrip_i8() -> Result<(), HidCommonError> {
    let values: Vec<i8> = vec![-128, -1, 0, 1, 127];
    let mut builder = ReportBuilder::with_capacity(values.len());
    for &v in &values {
        builder.write_i8(v);
    }
    let mut parser = ReportParser::new(builder.into_inner());
    for &expected in &values {
        assert_eq!(parser.read_i8()?, expected);
    }
    Ok(())
}

#[test]
fn roundtrip_u16_le() -> Result<(), HidCommonError> {
    let values: Vec<u16> = vec![0x0000, 0x0001, 0x00FF, 0x0100, 0x1234, 0x7FFF, 0xFFFF];
    let mut builder = ReportBuilder::with_capacity(values.len() * 2);
    for &v in &values {
        builder.write_u16_le(v);
    }
    let mut parser = ReportParser::new(builder.into_inner());
    for &expected in &values {
        assert_eq!(parser.read_u16_le()?, expected);
    }
    Ok(())
}

#[test]
fn roundtrip_i16_le() -> Result<(), HidCommonError> {
    let values: Vec<i16> = vec![i16::MIN, -1, 0, 1, i16::MAX];
    let mut builder = ReportBuilder::with_capacity(values.len() * 2);
    for &v in &values {
        builder.write_i16_le(v);
    }
    let mut parser = ReportParser::new(builder.into_inner());
    for &expected in &values {
        assert_eq!(parser.read_i16_le()?, expected);
    }
    Ok(())
}

#[test]
fn roundtrip_u32_le() -> Result<(), HidCommonError> {
    let values: Vec<u32> = vec![0, 1, 0xFF, 0xFFFF, 0x12345678, u32::MAX];
    let mut builder = ReportBuilder::with_capacity(values.len() * 4);
    for &v in &values {
        builder.write_u32_le(v);
    }
    let mut parser = ReportParser::new(builder.into_inner());
    for &expected in &values {
        assert_eq!(parser.read_u32_le()?, expected);
    }
    Ok(())
}

#[test]
fn roundtrip_f32_le() -> Result<(), HidCommonError> {
    let values: Vec<f32> = vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        std::f32::consts::PI,
        f32::MAX,
        f32::MIN,
    ];
    let mut builder = ReportBuilder::with_capacity(values.len() * 4);
    for &v in &values {
        builder.write_f32_le(v);
    }
    let mut parser = ReportParser::new(builder.into_inner());
    for &expected in &values {
        let got = parser.read_f32_le()?;
        assert!(
            (got - expected).abs() < f32::EPSILON || (got == 0.0 && expected == 0.0),
            "expected {expected}, got {got}"
        );
    }
    Ok(())
}

#[test]
fn roundtrip_mixed_types() -> Result<(), HidCommonError> {
    let mut builder = ReportBuilder::with_capacity(32);
    builder
        .write_u8(0x01)
        .write_i8(-42)
        .write_u16_le(0xCAFE)
        .write_i16_le(-1000)
        .write_u32_le(0xDEADBEEF)
        .write_f32_le(2.5)
        .write_bytes(&[0x10, 0x20, 0x30]);

    let mut parser = ReportParser::new(builder.into_inner());
    assert_eq!(parser.read_u8()?, 0x01);
    assert_eq!(parser.read_i8()?, -42);
    assert_eq!(parser.read_u16_le()?, 0xCAFE);
    assert_eq!(parser.read_i16_le()?, -1000);
    assert_eq!(parser.read_u32_le()?, 0xDEADBEEF);
    let f = parser.read_f32_le()?;
    assert!((f - 2.5).abs() < f32::EPSILON);
    assert_eq!(parser.read_bytes(3)?, vec![0x10, 0x20, 0x30]);
    assert_eq!(parser.remaining(), 0);
    Ok(())
}

// ---------------------------------------------------------------------------
// ReportParser — reset + re-read (simulates re-parsing same report)
// ---------------------------------------------------------------------------

#[test]
fn parser_reset_and_reparse() -> Result<(), HidCommonError> {
    let mut parser = ReportParser::new(vec![0xAA, 0xBB, 0xCC]);
    assert_eq!(parser.read_u8()?, 0xAA);
    assert_eq!(parser.read_u8()?, 0xBB);

    parser.reset();
    assert_eq!(parser.read_u8()?, 0xAA);
    assert_eq!(parser.read_u8()?, 0xBB);
    assert_eq!(parser.read_u8()?, 0xCC);
    assert!(parser.read_u8().is_err());
    Ok(())
}

// ---------------------------------------------------------------------------
// HidDeviceInfo
// ---------------------------------------------------------------------------

#[test]
fn device_info_with_serial() {
    let info = HidDeviceInfo::new(0x0483, 0x5740, "/dev/hidraw0".into()).with_serial("SN123456");
    assert_eq!(info.serial_number.as_deref(), Some("SN123456"));
}

#[test]
fn device_info_builder_chain() {
    let info = HidDeviceInfo::new(0x0483, 0x5740, "/dev/hidraw0".into())
        .with_serial("SN001")
        .with_manufacturer("ACME Racing")
        .with_product_name("Pro Wheel v2");
    assert_eq!(info.serial_number.as_deref(), Some("SN001"));
    assert_eq!(info.manufacturer.as_deref(), Some("ACME Racing"));
    assert_eq!(info.product_name.as_deref(), Some("Pro Wheel v2"));
    assert_eq!(info.display_name(), "Pro Wheel v2");
}

#[test]
fn device_info_display_name_precedence() {
    // product_name has highest priority
    let info = HidDeviceInfo::new(0x1234, 0x5678, "p".into())
        .with_manufacturer("Mfr")
        .with_product_name("Product");
    assert_eq!(info.display_name(), "Product");

    // Falls back to manufacturer
    let info = HidDeviceInfo::new(0x1234, 0x5678, "p".into()).with_manufacturer("Mfr");
    assert_eq!(info.display_name(), "Mfr");

    // Falls back to VID:PID
    let info = HidDeviceInfo::new(0x1234, 0x5678, "p".into());
    assert_eq!(info.display_name(), "1234:5678");
}

#[test]
fn device_info_default_fields() {
    let info = HidDeviceInfo::default();
    assert_eq!(info.vendor_id, 0);
    assert_eq!(info.product_id, 0);
    assert!(info.serial_number.is_none());
    assert!(info.manufacturer.is_none());
    assert!(info.product_name.is_none());
    assert!(info.path.is_empty());
}

#[test]
fn device_info_matches_multiple_ids() {
    let info = HidDeviceInfo::new(0x046D, 0xC266, "path".into());
    assert!(info.matches(0x046D, 0xC266));
    assert!(!info.matches(0x046D, 0xC267));
    assert!(!info.matches(0x046E, 0xC266));
    assert!(!info.matches(0x0000, 0x0000));
}

#[test]
fn device_info_serde_roundtrip() -> Result<(), serde_json::Error> {
    let info = HidDeviceInfo::new(0x046D, 0xC266, "/dev/hidraw1".into())
        .with_serial("ABC123")
        .with_manufacturer("Logitech")
        .with_product_name("G Pro Wheel");

    let json = serde_json::to_string(&info)?;
    let deserialized: HidDeviceInfo = serde_json::from_str(&json)?;

    assert_eq!(deserialized.vendor_id, info.vendor_id);
    assert_eq!(deserialized.product_id, info.product_id);
    assert_eq!(deserialized.serial_number, info.serial_number);
    assert_eq!(deserialized.manufacturer, info.manufacturer);
    assert_eq!(deserialized.product_name, info.product_name);
    assert_eq!(deserialized.path, info.path);
    Ok(())
}

// ---------------------------------------------------------------------------
// MockHidDevice — extended coverage
// ---------------------------------------------------------------------------

#[test]
fn mock_device_reconnect_after_disconnect() {
    let mut device = MockHidDevice::new(0x1234, 0x5678, "/dev/test");

    device.disconnect();
    assert!(!device.is_connected());
    assert!(device.write_report(&[0x01]).is_err());

    device.reconnect();
    assert!(device.is_connected());
    assert!(device.write_report(&[0x02]).is_ok());
}

#[test]
fn mock_device_read_empty_queue_returns_error() {
    let mut device = MockHidDevice::new(0x1234, 0x5678, "/dev/test");
    let result = device.read_report(100);
    assert!(result.is_err());
    match result {
        Err(HidCommonError::ReadError(msg)) => {
            assert!(msg.contains("No data"), "unexpected message: {msg}");
        }
        other => panic!("expected ReadError, got: {other:?}"),
    }
}

#[test]
fn mock_device_multiple_writes_tracked() {
    let mut device = MockHidDevice::new(0x1234, 0x5678, "/dev/test");

    assert!(device.write_report(&[0x01]).is_ok());
    assert!(device.write_report(&[0x02, 0x03]).is_ok());
    assert!(device.write_report(&[0x04, 0x05, 0x06]).is_ok());

    let history = device.get_write_history();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0], vec![0x01]);
    assert_eq!(history[1], vec![0x02, 0x03]);
    assert_eq!(history[2], vec![0x04, 0x05, 0x06]);
}

#[test]
fn mock_device_multiple_reads_fifo() -> Result<(), HidCommonError> {
    let mut device = MockHidDevice::new(0x1234, 0x5678, "/dev/test");

    device.queue_read(vec![0xAA]);
    device.queue_read(vec![0xBB, 0xCC]);
    device.queue_read(vec![0xDD]);

    assert_eq!(device.read_report(100)?, vec![0xAA]);
    assert_eq!(device.read_report(100)?, vec![0xBB, 0xCC]);
    assert_eq!(device.read_report(100)?, vec![0xDD]);
    assert!(device.read_report(100).is_err());
    Ok(())
}

#[test]
fn mock_device_close() -> Result<(), HidCommonError> {
    let mut device = MockHidDevice::new(0x1234, 0x5678, "/dev/test");
    assert!(device.is_connected());
    device.close()?;
    assert!(!device.is_connected());
    assert!(device.write_report(&[0x01]).is_err());
    Ok(())
}

#[test]
fn mock_device_write_returns_length() -> Result<(), HidCommonError> {
    let mut device = MockHidDevice::new(0x1234, 0x5678, "/dev/test");
    let written = device.write_report(&[0x01, 0x02, 0x03, 0x04, 0x05])?;
    assert_eq!(written, 5);

    let written = device.write_report(&[])?;
    assert_eq!(written, 0);
    Ok(())
}

#[test]
fn mock_device_disconnect_read_fails() {
    let mut device = MockHidDevice::new(0x1234, 0x5678, "/dev/test");
    device.queue_read(vec![0x01]);
    device.disconnect();
    assert!(matches!(
        device.read_report(100),
        Err(HidCommonError::Disconnected)
    ));
}

// ---------------------------------------------------------------------------
// MockHidPort — extended coverage
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mock_port_list_devices() -> Result<(), HidCommonError> {
    let mut port = MockHidPort::new();
    port.add_device(MockHidDevice::new(0x1234, 0x5678, "/dev/hidraw0"));
    port.add_device(MockHidDevice::new(0xAAAA, 0xBBBB, "/dev/hidraw1"));

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].vendor_id, 0x1234);
    assert_eq!(devices[1].vendor_id, 0xAAAA);
    Ok(())
}

#[tokio::test]
async fn mock_port_open_device_found() -> Result<(), HidCommonError> {
    let mut port = MockHidPort::new();
    port.add_device(MockHidDevice::new(0x1234, 0x5678, "/dev/hidraw0"));

    let dev = port.open_device("/dev/hidraw0").await?;
    assert!(dev.is_connected());
    assert_eq!(dev.get_device_info().vendor_id, 0x1234);
    Ok(())
}

#[tokio::test]
async fn mock_port_open_device_not_found() {
    let port = MockHidPort::new();
    let result = port.open_device("/dev/nonexistent").await;
    assert!(matches!(result, Err(HidCommonError::DeviceNotFound(_))));
}

#[tokio::test]
async fn mock_port_refresh_succeeds() -> Result<(), HidCommonError> {
    let port = MockHidPort::new();
    port.refresh().await?;
    Ok(())
}

#[tokio::test]
async fn mock_port_default() -> Result<(), HidCommonError> {
    let port = MockHidPort::default();
    assert_eq!(port.device_count(), 0);
    let devices = port.list_devices().await?;
    assert!(devices.is_empty());
    Ok(())
}

// ---------------------------------------------------------------------------
// HidCommonError — all variants and Display
// ---------------------------------------------------------------------------

#[test]
fn error_device_not_found_display() {
    let err = HidCommonError::DeviceNotFound("vid:pid".into());
    assert_eq!(format!("{err}"), "Device not found: vid:pid");
}

#[test]
fn error_open_error_display() {
    let err = HidCommonError::OpenError("permission denied".into());
    assert_eq!(format!("{err}"), "Failed to open device: permission denied");
}

#[test]
fn error_read_error_display() {
    let err = HidCommonError::ReadError("timeout".into());
    assert_eq!(format!("{err}"), "Failed to read from device: timeout");
}

#[test]
fn error_write_error_display() {
    let err = HidCommonError::WriteError("broken pipe".into());
    assert_eq!(format!("{err}"), "Failed to write to device: broken pipe");
}

#[test]
fn error_invalid_report_display() {
    let err = HidCommonError::InvalidReport("bad CRC".into());
    assert_eq!(format!("{err}"), "Invalid report format: bad CRC");
}

#[test]
fn error_disconnected_display() {
    let err = HidCommonError::Disconnected;
    assert_eq!(format!("{err}"), "Device disconnected");
}

#[test]
fn error_io_error_conversion() {
    let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broke");
    let hid_err: HidCommonError = io_err.into();
    let display = format!("{hid_err}");
    assert!(display.contains("pipe broke"), "unexpected: {display}");
}

#[test]
fn error_debug_formatting() {
    let err = HidCommonError::Disconnected;
    let debug = format!("{err:?}");
    assert!(debug.contains("Disconnected"));
}

// ---------------------------------------------------------------------------
// ReportParser + ReportBuilder — simulated PIDFF-style effect encoding
// ---------------------------------------------------------------------------

/// PIDFF Set Effect report (simplified): constant force effect
#[test]
fn simulated_pidff_constant_force_roundtrip() -> Result<(), HidCommonError> {
    const REPORT_ID_SET_EFFECT: u8 = 0x01;
    const EFFECT_TYPE_CONSTANT: u8 = 0x01;

    let effect_index: u8 = 3;
    let duration_ms: u16 = 5000;
    let direction: u16 = 180; // degrees
    let magnitude: i16 = -16384; // 50% reverse force

    let mut builder = ReportBuilder::with_capacity(16);
    builder
        .write_u8(REPORT_ID_SET_EFFECT)
        .write_u8(EFFECT_TYPE_CONSTANT)
        .write_u8(effect_index)
        .write_u16_le(duration_ms)
        .write_u16_le(direction)
        .write_i16_le(magnitude);

    let report = builder.into_inner();
    assert_eq!(report.len(), 9);

    let mut parser = ReportParser::new(report);
    assert_eq!(parser.read_u8()?, REPORT_ID_SET_EFFECT);
    assert_eq!(parser.read_u8()?, EFFECT_TYPE_CONSTANT);
    assert_eq!(parser.read_u8()?, effect_index);
    assert_eq!(parser.read_u16_le()?, duration_ms);
    assert_eq!(parser.read_u16_le()?, direction);
    assert_eq!(parser.read_i16_le()?, magnitude);
    assert_eq!(parser.remaining(), 0);
    Ok(())
}

/// PIDFF Set Effect report (simplified): periodic (sine) effect
#[test]
fn simulated_pidff_periodic_sine_roundtrip() -> Result<(), HidCommonError> {
    const REPORT_ID_SET_EFFECT: u8 = 0x01;
    const EFFECT_TYPE_PERIODIC: u8 = 0x02;
    const WAVEFORM_SINE: u8 = 0x01;

    let effect_index: u8 = 1;
    let duration_ms: u16 = 2000;
    let period_ms: u16 = 100;
    let magnitude: i16 = 10000;
    let offset: i16 = 0;
    let phase: u16 = 90; // degrees

    let mut builder = ReportBuilder::with_capacity(16);
    builder
        .write_u8(REPORT_ID_SET_EFFECT)
        .write_u8(EFFECT_TYPE_PERIODIC)
        .write_u8(WAVEFORM_SINE)
        .write_u8(effect_index)
        .write_u16_le(duration_ms)
        .write_u16_le(period_ms)
        .write_i16_le(magnitude)
        .write_i16_le(offset)
        .write_u16_le(phase);

    let report = builder.into_inner();
    let mut parser = ReportParser::new(report);

    assert_eq!(parser.read_u8()?, REPORT_ID_SET_EFFECT);
    assert_eq!(parser.read_u8()?, EFFECT_TYPE_PERIODIC);
    assert_eq!(parser.read_u8()?, WAVEFORM_SINE);
    assert_eq!(parser.read_u8()?, effect_index);
    assert_eq!(parser.read_u16_le()?, duration_ms);
    assert_eq!(parser.read_u16_le()?, period_ms);
    assert_eq!(parser.read_i16_le()?, magnitude);
    assert_eq!(parser.read_i16_le()?, offset);
    assert_eq!(parser.read_u16_le()?, phase);
    assert_eq!(parser.remaining(), 0);
    Ok(())
}

/// PIDFF-style spring/damper condition effect
#[test]
fn simulated_pidff_spring_condition_roundtrip() -> Result<(), HidCommonError> {
    const REPORT_ID_SET_CONDITION: u8 = 0x03;

    let effect_index: u8 = 5;
    let center_point: i16 = 0;
    let dead_band: u16 = 100;
    let positive_coefficient: i16 = 8000;
    let negative_coefficient: i16 = -8000;
    let positive_saturation: u16 = 10000;
    let negative_saturation: u16 = 10000;

    let mut builder = ReportBuilder::with_capacity(16);
    builder
        .write_u8(REPORT_ID_SET_CONDITION)
        .write_u8(effect_index)
        .write_i16_le(center_point)
        .write_u16_le(dead_band)
        .write_i16_le(positive_coefficient)
        .write_i16_le(negative_coefficient)
        .write_u16_le(positive_saturation)
        .write_u16_le(negative_saturation);

    let report = builder.into_inner();
    let mut parser = ReportParser::new(report);

    assert_eq!(parser.read_u8()?, REPORT_ID_SET_CONDITION);
    assert_eq!(parser.read_u8()?, effect_index);
    assert_eq!(parser.read_i16_le()?, center_point);
    assert_eq!(parser.read_u16_le()?, dead_band);
    assert_eq!(parser.read_i16_le()?, positive_coefficient);
    assert_eq!(parser.read_i16_le()?, negative_coefficient);
    assert_eq!(parser.read_u16_le()?, positive_saturation);
    assert_eq!(parser.read_u16_le()?, negative_saturation);
    assert_eq!(parser.remaining(), 0);
    Ok(())
}

/// PIDFF-style damper condition effect with asymmetric coefficients
#[test]
fn simulated_pidff_damper_condition_roundtrip() -> Result<(), HidCommonError> {
    const REPORT_ID_SET_CONDITION: u8 = 0x03;

    let effect_index: u8 = 7;
    let center_point: i16 = 500;
    let dead_band: u16 = 50;
    let positive_coefficient: i16 = 12000;
    let negative_coefficient: i16 = -6000;
    let positive_saturation: u16 = 16383;
    let negative_saturation: u16 = 16383;

    let mut builder = ReportBuilder::with_capacity(16);
    builder
        .write_u8(REPORT_ID_SET_CONDITION)
        .write_u8(effect_index)
        .write_i16_le(center_point)
        .write_u16_le(dead_band)
        .write_i16_le(positive_coefficient)
        .write_i16_le(negative_coefficient)
        .write_u16_le(positive_saturation)
        .write_u16_le(negative_saturation);

    let report = builder.into_inner();
    let mut parser = ReportParser::new(report);

    assert_eq!(parser.read_u8()?, REPORT_ID_SET_CONDITION);
    assert_eq!(parser.read_u8()?, effect_index);
    assert_eq!(parser.read_i16_le()?, center_point);
    assert_eq!(parser.read_u16_le()?, dead_band);
    assert_eq!(parser.read_i16_le()?, positive_coefficient);
    assert_eq!(parser.read_i16_le()?, negative_coefficient);
    assert_eq!(parser.read_u16_le()?, positive_saturation);
    assert_eq!(parser.read_u16_le()?, negative_saturation);
    Ok(())
}

/// Simulated capability report: device reports which effect types it supports
#[test]
fn simulated_capability_report_parsing() -> Result<(), HidCommonError> {
    const CAP_CONSTANT: u8 = 0x01;
    const CAP_PERIODIC: u8 = 0x02;
    const CAP_SPRING: u8 = 0x04;
    const CAP_DAMPER: u8 = 0x08;
    const CAP_FRICTION: u8 = 0x10;
    const CAP_INERTIA: u8 = 0x20;

    // Device supports constant, periodic, spring, and damper
    let capabilities = CAP_CONSTANT | CAP_PERIODIC | CAP_SPRING | CAP_DAMPER;
    let max_effects: u8 = 16;
    let max_simultaneous: u8 = 4;

    let mut builder = ReportBuilder::with_capacity(4);
    builder
        .write_u8(0x05) // report ID for capabilities
        .write_u8(capabilities)
        .write_u8(max_effects)
        .write_u8(max_simultaneous);

    let report = builder.into_inner();
    let mut parser = ReportParser::new(report);

    let _report_id = parser.read_u8()?;
    let caps = parser.read_u8()?;
    let max_eff = parser.read_u8()?;
    let max_sim = parser.read_u8()?;

    assert!(caps & CAP_CONSTANT != 0, "should support constant");
    assert!(caps & CAP_PERIODIC != 0, "should support periodic");
    assert!(caps & CAP_SPRING != 0, "should support spring");
    assert!(caps & CAP_DAMPER != 0, "should support damper");
    assert!(caps & CAP_FRICTION == 0, "should not support friction");
    assert!(caps & CAP_INERTIA == 0, "should not support inertia");
    assert_eq!(max_eff, 16);
    assert_eq!(max_sim, 4);
    Ok(())
}

/// Simulated malformed PIDFF report — too short for expected fields
#[test]
fn simulated_pidff_malformed_truncated() {
    // Only 3 bytes but we expect at least 9 for a full effect report
    let truncated = vec![0x01, 0x01, 0x03];
    let mut parser = ReportParser::new(truncated);

    // Can read report ID, type, and index
    assert!(parser.read_u8().is_ok());
    assert!(parser.read_u8().is_ok());
    assert!(parser.read_u8().is_ok());
    // But duration (u16) fails
    assert!(parser.read_u16_le().is_err());
}

/// Simulated malformed capability report — zero-length
#[test]
fn simulated_capability_report_empty() {
    let mut parser = ReportParser::new(vec![]);
    assert!(parser.read_u8().is_err());
}

// ---------------------------------------------------------------------------
// Edge cases — all-zeros and all-ones reports
// ---------------------------------------------------------------------------

#[test]
fn parser_all_zeros_report() -> Result<(), HidCommonError> {
    let data = vec![0x00; 16];
    let mut parser = ReportParser::new(data);
    assert_eq!(parser.read_u8()?, 0);
    assert_eq!(parser.read_u16_le()?, 0);
    assert_eq!(parser.read_u32_le()?, 0);
    assert_eq!(parser.read_i8()?, 0);
    assert_eq!(parser.read_i16_le()?, 0);
    assert_eq!(parser.read_i32_le()?, 0);
    Ok(())
}

#[test]
fn parser_all_ones_report() -> Result<(), HidCommonError> {
    let data = vec![0xFF; 16];
    let mut parser = ReportParser::new(data);
    assert_eq!(parser.read_u8()?, 0xFF);
    assert_eq!(parser.read_u16_le()?, 0xFFFF);
    assert_eq!(parser.read_u32_le()?, 0xFFFFFFFF);
    assert_eq!(parser.read_i8()?, -1);
    assert_eq!(parser.read_i16_le()?, -1);
    assert_eq!(parser.read_i32_le()?, -1);
    Ok(())
}

// ---------------------------------------------------------------------------
// ReportBuilder — building from ReportBuilder::new (pre-filled with zeros)
// ---------------------------------------------------------------------------

#[test]
fn builder_new_prefills_with_zeros() {
    let builder = ReportBuilder::new(8);
    let data = builder.into_inner();
    assert_eq!(data.len(), 8);
    assert!(data.iter().all(|&b| b == 0));
}

#[test]
fn builder_new_appends_after_zeros() {
    let mut builder = ReportBuilder::new(2);
    builder.write_u8(0xAA);
    let data = builder.into_inner();
    // 2 zeros + 1 appended byte
    assert_eq!(data, vec![0x00, 0x00, 0xAA]);
}
