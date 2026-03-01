//! Extended snapshot tests for Moza wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering slew-rate encoding,
//! effective FFB mode resolution, torque at zero and boundary values,
//! and signature trust decisions that would detect wire-format regressions.

use insta::assert_snapshot;
use racing_wheel_hid_moza_protocol::{
    DeviceWriter, FfbMode, MozaDirectTorqueEncoder, MozaModel,
    MozaProtocol, REPORT_LEN, default_ffb_mode, default_high_torque_enabled,
    effective_ffb_mode, effective_high_torque_opt_in, identify_device, product_ids,
    signature_is_trusted,
};

// ── Mock writer for capturing feature-report bytes ───────────────────────────

struct MockWriter {
    last: Vec<u8>,
}

impl MockWriter {
    fn new() -> Self {
        Self { last: Vec::new() }
    }
}

impl DeviceWriter for MockWriter {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.last = data.to_vec();
        Ok(data.len())
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.last = data.to_vec();
        Ok(data.len())
    }
}

// ── Torque encoder boundary values ───────────────────────────────────────────

#[test]
fn test_snapshot_torque_zero_r5() {
    let enc = MozaDirectTorqueEncoder::new(MozaModel::R5.max_torque_nm());
    let mut out = [0u8; REPORT_LEN];
    enc.encode(0.0, 0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_torque_full_negative_r9() {
    let max = MozaModel::R9.max_torque_nm();
    let enc = MozaDirectTorqueEncoder::new(max);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(-max, 0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_torque_clamp_above_max_r3() {
    let max = MozaModel::R3.max_torque_nm();
    let enc = MozaDirectTorqueEncoder::new(max);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(max * 3.0, 0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

// ── Slew-rate encoding ───────────────────────────────────────────────────────

#[test]
fn test_snapshot_torque_with_slew_rate() {
    let enc = MozaDirectTorqueEncoder::new(MozaModel::R12.max_torque_nm())
        .with_slew_rate(500);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(6.0, 0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_torque_with_slew_rate_zero() {
    let enc = MozaDirectTorqueEncoder::new(MozaModel::R12.max_torque_nm())
        .with_slew_rate(0);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(0.0, 0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

// ── Effective FFB mode resolution ────────────────────────────────────────────

#[test]
fn test_snapshot_effective_ffb_mode_direct_untrusted() {
    // With no CRC32, signature is untrusted → should downgrade Direct to Standard
    let mode = effective_ffb_mode(FfbMode::Direct, None);
    assert_snapshot!(format!("{mode:?}"));
}

#[test]
fn test_snapshot_effective_ffb_mode_standard_untrusted() {
    let mode = effective_ffb_mode(FfbMode::Standard, None);
    assert_snapshot!(format!("{mode:?}"));
}

#[test]
fn test_snapshot_effective_high_torque_opt_in_untrusted() {
    let enabled = effective_high_torque_opt_in(None);
    assert_snapshot!(format!("{enabled}"));
}

// ── Default settings (no env vars set) ───────────────────────────────────────

#[test]
fn test_snapshot_default_ffb_mode() {
    let mode = default_ffb_mode();
    assert_snapshot!(format!("{mode:?}"));
}

#[test]
fn test_snapshot_default_high_torque_enabled() {
    let enabled = default_high_torque_enabled();
    assert_snapshot!(format!("{enabled}"));
}

// ── Signature trust ──────────────────────────────────────────────────────────

#[test]
fn test_snapshot_signature_is_trusted_none() {
    let trusted = signature_is_trusted(None);
    assert_snapshot!(format!("{trusted}"));
}

#[test]
fn test_snapshot_signature_is_trusted_unknown_crc() {
    let trusted = signature_is_trusted(Some(0xDEADBEEF));
    assert_snapshot!(format!("{trusted}"));
}

// ── Rotation range boundary values ───────────────────────────────────────────

#[test]
fn test_snapshot_rotation_range_270() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R5_V1);
    let mut writer = MockWriter::new();
    protocol.set_rotation_range(&mut writer, 270)?;
    assert_snapshot!(format!("{:02X?}", writer.last));
    Ok(())
}

#[test]
fn test_snapshot_rotation_range_1080() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R9_V2);
    let mut writer = MockWriter::new();
    protocol.set_rotation_range(&mut writer, 1080)?;
    assert_snapshot!(format!("{:02X?}", writer.last));
    Ok(())
}

// ── Device identity for all v2 products ──────────────────────────────────────

#[test]
fn test_snapshot_identify_r3_v2() {
    let id = identify_device(product_ids::R3_V2);
    assert_snapshot!(format!("{id:?}"));
}

#[test]
fn test_snapshot_identify_r5_v2() {
    let id = identify_device(product_ids::R5_V2);
    assert_snapshot!(format!("{id:?}"));
}

#[test]
fn test_snapshot_identify_r9_v2() {
    let id = identify_device(product_ids::R9_V2);
    assert_snapshot!(format!("{id:?}"));
}

#[test]
fn test_snapshot_identify_r12_v2() {
    let id = identify_device(product_ids::R12_V2);
    assert_snapshot!(format!("{id:?}"));
}

#[test]
fn test_snapshot_identify_unknown_pid() {
    let id = identify_device(0xFFFF);
    assert_snapshot!(format!("{id:?}"));
}

// ── Peripheral identification ────────────────────────────────────────────────

#[test]
fn test_snapshot_identify_hbp_handbrake() {
    let id = identify_device(product_ids::HBP_HANDBRAKE);
    assert_snapshot!(format!("{id:?}"));
}

#[test]
fn test_snapshot_identify_srp_pedals() {
    let id = identify_device(product_ids::SR_P_PEDALS);
    assert_snapshot!(format!("{id:?}"));
}
