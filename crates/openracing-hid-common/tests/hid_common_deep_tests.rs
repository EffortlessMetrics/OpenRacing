//! Deep tests for openracing-hid-common: HID common types, device info,
//! report parsing, builder round-trips, mock device behavior, and error paths.

use openracing_hid_common::{
    HidCommonError, HidDevice, HidDeviceInfo, HidPort, ReportBuilder, ReportParser,
    hid_traits::mock::{MockHidDevice, MockHidPort},
};

// ===========================================================================
// HidDeviceInfo — construction, builder pattern, matching, display
// ===========================================================================

#[test]
fn device_info_new_sets_required_fields() {
    let info = HidDeviceInfo::new(0x1234, 0x5678, "/dev/hid0".into());
    assert_eq!(info.vendor_id, 0x1234);
    assert_eq!(info.product_id, 0x5678);
    assert_eq!(info.path, "/dev/hid0");
    assert!(info.serial_number.is_none());
    assert!(info.manufacturer.is_none());
    assert!(info.product_name.is_none());
}

#[test]
fn device_info_default_is_zeroed() {
    let info = HidDeviceInfo::default();
    assert_eq!(info.vendor_id, 0);
    assert_eq!(info.product_id, 0);
    assert!(info.path.is_empty());
}

#[test]
fn device_info_builder_chain() {
    let info = HidDeviceInfo::new(0x0001, 0x0002, "p".into())
        .with_serial("SN-42")
        .with_manufacturer("Acme")
        .with_product_name("Widget");

    assert_eq!(info.serial_number.as_deref(), Some("SN-42"));
    assert_eq!(info.manufacturer.as_deref(), Some("Acme"));
    assert_eq!(info.product_name.as_deref(), Some("Widget"));
}

#[test]
fn device_info_matches_exact_ids() {
    let info = HidDeviceInfo::new(0xBEEF, 0xCAFE, "x".into());
    assert!(info.matches(0xBEEF, 0xCAFE));
    assert!(!info.matches(0xBEEF, 0x0000));
    assert!(!info.matches(0x0000, 0xCAFE));
    assert!(!info.matches(0, 0));
}

#[test]
fn device_info_display_name_with_product() {
    let info = HidDeviceInfo::new(1, 2, "p".into()).with_product_name("Wheel");
    let name = info.display_name();
    assert!(name.contains("Wheel"), "display_name should contain product: {name}");
}

#[test]
fn device_info_display_name_without_product() {
    let info = HidDeviceInfo::new(0x1234, 0x5678, "p".into());
    let name = info.display_name();
    // Should still produce a reasonable string with vendor/product ids
    assert!(!name.is_empty());
}

#[test]
fn device_info_clone_is_independent() {
    let info = HidDeviceInfo::new(1, 2, "a".into()).with_serial("s1");
    let cloned = info.clone();
    assert_eq!(info.serial_number, cloned.serial_number);
    assert_eq!(info.vendor_id, cloned.vendor_id);
}

#[test]
fn device_info_serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let info = HidDeviceInfo::new(0xAA, 0xBB, "/dev/hid1".into())
        .with_serial("S-001")
        .with_manufacturer("Mfg")
        .with_product_name("Prod");

    let json = serde_json::to_string(&info)?;
    let back: HidDeviceInfo = serde_json::from_str(&json)?;

    assert_eq!(back.vendor_id, info.vendor_id);
    assert_eq!(back.product_id, info.product_id);
    assert_eq!(back.path, info.path);
    assert_eq!(back.serial_number, info.serial_number);
    assert_eq!(back.manufacturer, info.manufacturer);
    assert_eq!(back.product_name, info.product_name);
    Ok(())
}

// ===========================================================================
// ReportParser — edge cases, all read types, error conditions
// ===========================================================================

#[test]
fn parser_read_u8_sequential() -> Result<(), HidCommonError> {
    let mut p = ReportParser::new(vec![0x00, 0x7F, 0xFF]);
    assert_eq!(p.read_u8()?, 0x00);
    assert_eq!(p.read_u8()?, 0x7F);
    assert_eq!(p.read_u8()?, 0xFF);
    assert_eq!(p.remaining(), 0);
    Ok(())
}

#[test]
fn parser_read_u8_past_end_is_error() {
    let mut p = ReportParser::new(vec![0x42]);
    let _ = p.read_u8(); // consume the byte
    let err = p.read_u8();
    assert!(err.is_err());
}

#[test]
fn parser_read_i8_boundary_values() -> Result<(), HidCommonError> {
    let mut p = ReportParser::new(vec![0x00, 0x01, 0x7F, 0x80, 0xFE, 0xFF]);
    assert_eq!(p.read_i8()?, 0);
    assert_eq!(p.read_i8()?, 1);
    assert_eq!(p.read_i8()?, 127);
    assert_eq!(p.read_i8()?, -128);
    assert_eq!(p.read_i8()?, -2);
    assert_eq!(p.read_i8()?, -1);
    Ok(())
}

#[test]
fn parser_read_u16_le_byte_order() -> Result<(), HidCommonError> {
    // 0x0201 in LE = [0x01, 0x02]
    let mut p = ReportParser::new(vec![0x01, 0x02]);
    assert_eq!(p.read_u16_le()?, 0x0201);
    Ok(())
}

#[test]
fn parser_read_u16_le_boundary() -> Result<(), HidCommonError> {
    // u16::MAX = 0xFFFF LE = [0xFF, 0xFF]
    let mut p = ReportParser::new(vec![0xFF, 0xFF, 0x00, 0x00]);
    assert_eq!(p.read_u16_le()?, u16::MAX);
    assert_eq!(p.read_u16_le()?, 0);
    Ok(())
}

#[test]
fn parser_read_u16_le_insufficient_bytes() {
    let mut p = ReportParser::new(vec![0x01]);
    assert!(p.read_u16_le().is_err());
}

#[test]
fn parser_read_u16_be_byte_order() -> Result<(), HidCommonError> {
    // 0x0102 in BE = [0x01, 0x02]
    let mut p = ReportParser::new(vec![0x01, 0x02]);
    assert_eq!(p.read_u16_be()?, 0x0102);
    Ok(())
}

#[test]
fn parser_read_i16_le_boundary_values() -> Result<(), HidCommonError> {
    // i16::MIN = -32768 LE = [0x00, 0x80]
    // i16::MAX = 32767 LE = [0xFF, 0x7F]
    let mut p = ReportParser::new(vec![0x00, 0x80, 0xFF, 0x7F]);
    assert_eq!(p.read_i16_le()?, i16::MIN);
    assert_eq!(p.read_i16_le()?, i16::MAX);
    Ok(())
}

#[test]
fn parser_read_u32_le_byte_order() -> Result<(), HidCommonError> {
    // 0x04030201 LE = [0x01, 0x02, 0x03, 0x04]
    let mut p = ReportParser::new(vec![0x01, 0x02, 0x03, 0x04]);
    assert_eq!(p.read_u32_le()?, 0x04030201);
    Ok(())
}

#[test]
fn parser_read_u32_le_max() -> Result<(), HidCommonError> {
    let mut p = ReportParser::new(vec![0xFF, 0xFF, 0xFF, 0xFF]);
    assert_eq!(p.read_u32_le()?, u32::MAX);
    Ok(())
}

#[test]
fn parser_read_u32_le_insufficient_bytes() {
    let mut p = ReportParser::new(vec![0x01, 0x02, 0x03]);
    assert!(p.read_u32_le().is_err());
}

#[test]
fn parser_read_i32_le_negative() -> Result<(), HidCommonError> {
    // -1 in LE i32 = [0xFF, 0xFF, 0xFF, 0xFF]
    let mut p = ReportParser::new(vec![0xFF, 0xFF, 0xFF, 0xFF]);
    assert_eq!(p.read_i32_le()?, -1);
    Ok(())
}

#[test]
fn parser_read_i32_le_min_max() -> Result<(), HidCommonError> {
    let min_bytes = i32::MIN.to_le_bytes();
    let max_bytes = i32::MAX.to_le_bytes();
    let mut data = Vec::new();
    data.extend_from_slice(&min_bytes);
    data.extend_from_slice(&max_bytes);

    let mut p = ReportParser::new(data);
    assert_eq!(p.read_i32_le()?, i32::MIN);
    assert_eq!(p.read_i32_le()?, i32::MAX);
    Ok(())
}

#[test]
fn parser_read_f32_le_known_value() -> Result<(), HidCommonError> {
    let val: f32 = 3.125;
    let bytes = val.to_le_bytes();
    let mut p = ReportParser::new(bytes.to_vec());
    let read = p.read_f32_le()?;
    assert!((read - val).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn parser_read_f32_le_zero_and_negative() -> Result<(), HidCommonError> {
    let mut data = Vec::new();
    data.extend_from_slice(&0.0_f32.to_le_bytes());
    data.extend_from_slice(&(-1.0_f32).to_le_bytes());
    let mut p = ReportParser::new(data);
    assert!((p.read_f32_le()? - 0.0).abs() < f32::EPSILON);
    assert!((p.read_f32_le()? - (-1.0)).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn parser_read_bytes_exact() -> Result<(), HidCommonError> {
    let mut p = ReportParser::new(vec![0xAA, 0xBB, 0xCC, 0xDD]);
    let bytes = p.read_bytes(2)?;
    assert_eq!(bytes, vec![0xAA, 0xBB]);
    assert_eq!(p.remaining(), 2);
    Ok(())
}

#[test]
fn parser_read_bytes_zero_count() -> Result<(), HidCommonError> {
    let mut p = ReportParser::new(vec![0x01]);
    let bytes = p.read_bytes(0)?;
    assert!(bytes.is_empty());
    assert_eq!(p.remaining(), 1);
    Ok(())
}

#[test]
fn parser_read_bytes_exceeds_remaining() {
    let mut p = ReportParser::new(vec![0x01]);
    assert!(p.read_bytes(2).is_err());
}

#[test]
fn parser_peek_u8_does_not_advance() -> Result<(), HidCommonError> {
    let mut p = ReportParser::new(vec![0x42, 0x43]);
    let peeked = p.peek_u8()?;
    assert_eq!(peeked, 0x42);
    assert_eq!(p.remaining(), 2);
    // read still gives same value
    assert_eq!(p.read_u8()?, 0x42);
    assert_eq!(p.remaining(), 1);
    Ok(())
}

#[test]
fn parser_peek_u8_empty_is_error() {
    let mut p = ReportParser::new(vec![]);
    assert!(p.peek_u8().is_err());
}

#[test]
fn parser_skip_advances_position() -> Result<(), HidCommonError> {
    let mut p = ReportParser::new(vec![0x01, 0x02, 0x03, 0x04]);
    p.skip(2);
    assert_eq!(p.remaining(), 2);
    assert_eq!(p.read_u8()?, 0x03);
    Ok(())
}

#[test]
fn parser_skip_past_end_saturates() {
    let mut p = ReportParser::new(vec![0x01]);
    p.skip(100);
    assert_eq!(p.remaining(), 0);
}

#[test]
fn parser_reset_returns_to_start() -> Result<(), HidCommonError> {
    let mut p = ReportParser::new(vec![0xAA, 0xBB]);
    let _ = p.read_u8()?;
    p.reset();
    assert_eq!(p.remaining(), 2);
    assert_eq!(p.read_u8()?, 0xAA);
    Ok(())
}

#[test]
fn parser_into_inner_returns_full_buffer() {
    let data = vec![0x01, 0x02, 0x03];
    let mut p = ReportParser::new(data.clone());
    let _ = p.read_u8();
    let inner = p.into_inner();
    assert_eq!(inner, data);
}

#[test]
fn parser_slice_returns_full_buffer() {
    let data = vec![0x01, 0x02, 0x03];
    let p = ReportParser::new(data.clone());
    assert_eq!(p.slice(), &data[..]);
}

#[test]
fn parser_from_slice_equivalent_to_new() -> Result<(), HidCommonError> {
    let data = [0x10, 0x20, 0x30];
    let mut p1 = ReportParser::new(data.to_vec());
    let mut p2 = ReportParser::from_slice(&data);
    assert_eq!(p1.read_u8()?, p2.read_u8()?);
    assert_eq!(p1.read_u16_le()?, p2.read_u16_le()?);
    Ok(())
}

// ===========================================================================
// ReportBuilder — construction, writes, length, empty
// ===========================================================================

#[test]
fn builder_default_has_preallocated_buffer() {
    let b = ReportBuilder::default();
    // Default pre-fills a 64-byte zero buffer via new(64)
    assert_eq!(b.len(), 64);
    assert!(!b.is_empty());
}

#[test]
fn builder_write_u8_increases_len() {
    let mut b = ReportBuilder::with_capacity(4);
    b.write_u8(0xAA);
    assert_eq!(b.len(), 1);
    assert!(!b.is_empty());
}

#[test]
fn builder_write_all_types() {
    let mut b = ReportBuilder::with_capacity(64);
    b.write_u8(0x01)
        .write_i8(-1)
        .write_u16_le(0x0201)
        .write_i16_le(-1)
        .write_u32_le(0x04030201)
        .write_f32_le(1.0)
        .write_bytes(&[0xAA, 0xBB]);

    // 1 + 1 + 2 + 2 + 4 + 4 + 2 = 16
    assert_eq!(b.len(), 16);
}

#[test]
fn builder_as_slice_matches_into_inner() {
    let mut b = ReportBuilder::with_capacity(4);
    b.write_u8(0x42).write_u8(0x43);
    let slice = b.as_slice().to_vec();
    let inner = b.into_inner();
    assert_eq!(slice, inner);
}

#[test]
fn builder_with_capacity_creates_empty() {
    let b = ReportBuilder::with_capacity(128);
    assert!(b.is_empty());
    assert_eq!(b.len(), 0);
}

// ===========================================================================
// Round-trip: Builder → Parser for all types
// ===========================================================================

#[test]
fn round_trip_u8() -> Result<(), HidCommonError> {
    let values: &[u8] = &[0, 1, 127, 128, 255];
    for &v in values {
        let mut b = ReportBuilder::with_capacity(1);
        b.write_u8(v);
        let mut p = ReportParser::new(b.into_inner());
        assert_eq!(p.read_u8()?, v);
    }
    Ok(())
}

#[test]
fn round_trip_i8() -> Result<(), HidCommonError> {
    let values: &[i8] = &[-128, -1, 0, 1, 127];
    for &v in values {
        let mut b = ReportBuilder::with_capacity(1);
        b.write_i8(v);
        let mut p = ReportParser::new(b.into_inner());
        assert_eq!(p.read_i8()?, v);
    }
    Ok(())
}

#[test]
fn round_trip_u16_le() -> Result<(), HidCommonError> {
    let values: &[u16] = &[0, 1, 0x00FF, 0xFF00, u16::MAX];
    for &v in values {
        let mut b = ReportBuilder::with_capacity(2);
        b.write_u16_le(v);
        let mut p = ReportParser::new(b.into_inner());
        assert_eq!(p.read_u16_le()?, v);
    }
    Ok(())
}

#[test]
fn round_trip_i16_le() -> Result<(), HidCommonError> {
    let values: &[i16] = &[i16::MIN, -1, 0, 1, i16::MAX];
    for &v in values {
        let mut b = ReportBuilder::with_capacity(2);
        b.write_i16_le(v);
        let mut p = ReportParser::new(b.into_inner());
        assert_eq!(p.read_i16_le()?, v);
    }
    Ok(())
}

#[test]
fn round_trip_u32_le() -> Result<(), HidCommonError> {
    let values: &[u32] = &[0, 1, 0xDEAD_BEEF, u32::MAX];
    for &v in values {
        let mut b = ReportBuilder::with_capacity(4);
        b.write_u32_le(v);
        let mut p = ReportParser::new(b.into_inner());
        assert_eq!(p.read_u32_le()?, v);
    }
    Ok(())
}

#[test]
fn round_trip_f32_le() -> Result<(), HidCommonError> {
    let values: &[f32] = &[0.0, -0.0, 1.0, -1.0, f32::MIN, f32::MAX, f32::EPSILON];
    for &v in values {
        let mut b = ReportBuilder::with_capacity(4);
        b.write_f32_le(v);
        let mut p = ReportParser::new(b.into_inner());
        let read = p.read_f32_le()?;
        assert!(
            (read - v).abs() < f32::EPSILON || (read == 0.0 && v == 0.0),
            "round-trip failed for {v}: got {read}"
        );
    }
    Ok(())
}

#[test]
fn round_trip_bytes() -> Result<(), HidCommonError> {
    let data = vec![0x01, 0x02, 0x03, 0x04, 0x05];
    let mut b = ReportBuilder::with_capacity(data.len());
    b.write_bytes(&data);
    let mut p = ReportParser::new(b.into_inner());
    assert_eq!(p.read_bytes(data.len())?, data);
    Ok(())
}

#[test]
fn round_trip_mixed_report() -> Result<(), HidCommonError> {
    let mut b = ReportBuilder::with_capacity(32);
    b.write_u8(0x01)          // report id
        .write_u16_le(1000)   // axis value
        .write_i16_le(-500)   // signed axis
        .write_u32_le(12345)  // timestamp
        .write_f32_le(0.75);  // normalized value

    let mut p = ReportParser::new(b.into_inner());
    assert_eq!(p.read_u8()?, 0x01);
    assert_eq!(p.read_u16_le()?, 1000);
    assert_eq!(p.read_i16_le()?, -500);
    assert_eq!(p.read_u32_le()?, 12345);
    assert!((p.read_f32_le()? - 0.75).abs() < f32::EPSILON);
    assert_eq!(p.remaining(), 0);
    Ok(())
}

// ===========================================================================
// HidCommonError — display, variants, from conversions
// ===========================================================================

#[test]
fn error_device_not_found_contains_name() {
    let e = HidCommonError::DeviceNotFound("wheel-123".into());
    let msg = format!("{e}");
    assert!(msg.contains("wheel-123"));
}

#[test]
fn error_open_error_message() {
    let e = HidCommonError::OpenError("access denied".into());
    let msg = format!("{e}");
    assert!(msg.contains("access denied"));
}

#[test]
fn error_invalid_report_message() {
    let e = HidCommonError::InvalidReport("bad header".into());
    let msg = format!("{e}");
    assert!(msg.contains("bad header"));
}

#[test]
fn error_disconnected_display() {
    let e = HidCommonError::Disconnected;
    let msg = format!("{e}");
    assert!(!msg.is_empty());
}

#[test]
fn error_io_error_from_conversion() {
    let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broken");
    let hid_err: HidCommonError = io_err.into();
    let msg = format!("{hid_err}");
    assert!(msg.contains("pipe broken") || msg.contains("IO"));
}

#[test]
fn error_is_std_error() {
    let e = HidCommonError::ReadError("timeout".into());
    let _: &dyn std::error::Error = &e;
}

#[test]
fn error_write_error_display() {
    let e = HidCommonError::WriteError("buffer full".into());
    let msg = format!("{e}");
    assert!(msg.contains("buffer full"));
}

// ===========================================================================
// MockHidDevice — queue reads, write history, connect/disconnect
// ===========================================================================

#[test]
fn mock_device_initial_state() {
    let dev = MockHidDevice::new(0x1234, 0x5678, "/dev/hid0");
    assert!(dev.is_connected());
    let info = dev.get_device_info();
    assert_eq!(info.vendor_id, 0x1234);
    assert_eq!(info.product_id, 0x5678);
}

#[test]
fn mock_device_write_and_history() -> Result<(), HidCommonError> {
    let mut dev = MockHidDevice::new(1, 2, "p");
    let written = dev.write_report(&[0x01, 0x02, 0x03])?;
    assert!(written > 0);
    let history = dev.get_write_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0], vec![0x01, 0x02, 0x03]);
    Ok(())
}

#[test]
fn mock_device_multiple_writes() -> Result<(), HidCommonError> {
    let mut dev = MockHidDevice::new(1, 2, "p");
    dev.write_report(&[0x01])?;
    dev.write_report(&[0x02])?;
    dev.write_report(&[0x03])?;
    let history = dev.get_write_history();
    assert_eq!(history.len(), 3);
    Ok(())
}

#[test]
fn mock_device_queue_and_read() -> Result<(), HidCommonError> {
    let mut dev = MockHidDevice::new(1, 2, "p");
    dev.queue_read(vec![0xAA, 0xBB]);
    dev.queue_read(vec![0xCC]);
    let r1 = dev.read_report(100)?;
    assert_eq!(r1, vec![0xAA, 0xBB]);
    let r2 = dev.read_report(100)?;
    assert_eq!(r2, vec![0xCC]);
    Ok(())
}

#[test]
fn mock_device_disconnect_prevents_write() {
    let mut dev = MockHidDevice::new(1, 2, "p");
    dev.disconnect();
    assert!(!dev.is_connected());
    let result = dev.write_report(&[0x01]);
    assert!(result.is_err());
}

#[test]
fn mock_device_disconnect_prevents_read() {
    let mut dev = MockHidDevice::new(1, 2, "p");
    dev.queue_read(vec![0x01]);
    dev.disconnect();
    let result = dev.read_report(100);
    assert!(result.is_err());
}

#[test]
fn mock_device_reconnect_restores_connection() -> Result<(), HidCommonError> {
    let mut dev = MockHidDevice::new(1, 2, "p");
    dev.disconnect();
    assert!(!dev.is_connected());
    dev.reconnect();
    assert!(dev.is_connected());
    // Should be able to write again
    let written = dev.write_report(&[0x01])?;
    assert!(written > 0);
    Ok(())
}

#[test]
fn mock_device_close() -> Result<(), HidCommonError> {
    let mut dev = MockHidDevice::new(1, 2, "p");
    dev.close()?;
    Ok(())
}

// ===========================================================================
// MockHidPort — device listing, open
// ===========================================================================

#[test]
fn mock_port_default_empty() {
    let port = MockHidPort::default();
    assert_eq!(port.device_count(), 0);
}

#[test]
fn mock_port_add_devices() {
    let mut port = MockHidPort::new();
    port.add_device(MockHidDevice::new(1, 1, "a"));
    port.add_device(MockHidDevice::new(2, 2, "b"));
    assert_eq!(port.device_count(), 2);
}

#[tokio::test]
async fn mock_port_list_devices() -> Result<(), HidCommonError> {
    let mut port = MockHidPort::new();
    port.add_device(MockHidDevice::new(0xAA, 0xBB, "/dev/a"));
    port.add_device(MockHidDevice::new(0xCC, 0xDD, "/dev/b"));
    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 2);
    Ok(())
}

#[tokio::test]
async fn mock_port_open_device() -> Result<(), HidCommonError> {
    let mut port = MockHidPort::new();
    port.add_device(MockHidDevice::new(0xAA, 0xBB, "test_path"));
    let dev = port.open_device("test_path").await?;
    assert!(dev.is_connected());
    Ok(())
}

#[tokio::test]
async fn mock_port_refresh() -> Result<(), HidCommonError> {
    let port = MockHidPort::new();
    port.refresh().await?;
    Ok(())
}

// ===========================================================================
// Parser — multi-read interleaved with skip/reset/peek
// ===========================================================================

#[test]
fn parser_interleaved_operations() -> Result<(), HidCommonError> {
    let mut b = ReportBuilder::with_capacity(16);
    b.write_u8(0x01)
        .write_u16_le(0x1234)
        .write_u8(0xFF)
        .write_u32_le(0xDEADBEEF);

    let mut p = ReportParser::new(b.into_inner());

    // Read report id
    assert_eq!(p.read_u8()?, 0x01);
    // Peek at next byte (low byte of u16)
    assert_eq!(p.peek_u8()?, 0x34);
    // Read the u16
    assert_eq!(p.read_u16_le()?, 0x1234);
    // Skip the 0xFF byte
    p.skip(1);
    // Read the u32
    assert_eq!(p.read_u32_le()?, 0xDEADBEEF);
    assert_eq!(p.remaining(), 0);
    Ok(())
}

#[test]
fn parser_reset_after_partial_read() -> Result<(), HidCommonError> {
    let mut p = ReportParser::new(vec![0x01, 0x02, 0x03]);
    let _ = p.read_u8()?;
    let _ = p.read_u8()?;
    assert_eq!(p.remaining(), 1);
    p.reset();
    assert_eq!(p.remaining(), 3);
    assert_eq!(p.read_u8()?, 0x01);
    Ok(())
}

// ===========================================================================
// Builder edge cases
// ===========================================================================

#[test]
fn builder_write_empty_bytes() {
    let mut b = ReportBuilder::new(0);
    b.write_bytes(&[]);
    assert!(b.is_empty());
}

#[test]
fn builder_large_report() {
    let mut b = ReportBuilder::with_capacity(256);
    for i in 0..=255u8 {
        b.write_u8(i);
    }
    assert_eq!(b.len(), 256);
}

#[test]
fn builder_chaining_returns_same_builder() {
    let mut b = ReportBuilder::with_capacity(8);
    let ptr_before = std::ptr::from_ref(&b);
    let returned = b.write_u8(1);
    let ptr_after = std::ptr::from_ref(returned).cast::<ReportBuilder>();
    // The builder method should return &mut Self for chaining
    assert_eq!(ptr_before, ptr_after);
}
