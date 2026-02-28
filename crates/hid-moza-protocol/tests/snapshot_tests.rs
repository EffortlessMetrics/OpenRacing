use insta::assert_snapshot;
use racing_wheel_hid_moza_protocol::{
    DeviceSignature, DeviceWriter, FfbMode, MOZA_VENDOR_ID, MozaDirectTorqueEncoder, MozaModel,
    MozaProtocol, REPORT_LEN, es_compatibility, identify_device, is_wheelbase_product, product_ids,
    verify_signature,
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

// ── Wheelbase model identity snapshots ───────────────────────────────────────

#[test]
fn test_snapshot_identify_r3_v1() {
    let id = identify_device(product_ids::R3_V1);
    assert_snapshot!(format!("{id:?}"));
}

#[test]
fn test_snapshot_identify_r5_v1() {
    let id = identify_device(product_ids::R5_V1);
    assert_snapshot!(format!("{id:?}"));
}

#[test]
fn test_snapshot_identify_r9_v1() {
    let id = identify_device(product_ids::R9_V1);
    assert_snapshot!(format!("{id:?}"));
}

#[test]
fn test_snapshot_identify_r12_v1() {
    let id = identify_device(product_ids::R12_V1);
    assert_snapshot!(format!("{id:?}"));
}

#[test]
fn test_snapshot_identify_r16_r21_v1() {
    let id = identify_device(product_ids::R16_R21_V1);
    assert_snapshot!(format!("{id:?}"));
}

#[test]
fn test_snapshot_identify_r16_r21_v2() {
    let id = identify_device(product_ids::R16_R21_V2);
    assert_snapshot!(format!("{id:?}"));
}

// ── Torque encoding snapshots (byte-level regression guard) ─────────────────

#[test]
fn test_snapshot_torque_r3_full_scale() {
    let enc = MozaDirectTorqueEncoder::new(MozaModel::R3.max_torque_nm());
    let mut out = [0u8; REPORT_LEN];
    enc.encode(MozaModel::R3.max_torque_nm(), 0, &mut out);
    assert_snapshot!(format!("{out:?}"));
}

#[test]
fn test_snapshot_torque_r5_half_scale() {
    let max = MozaModel::R5.max_torque_nm();
    let enc = MozaDirectTorqueEncoder::new(max);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(max * 0.5, 0, &mut out);
    assert_snapshot!(format!("{out:?}"));
}

#[test]
fn test_snapshot_torque_r9_quarter_scale() {
    let max = MozaModel::R9.max_torque_nm();
    let enc = MozaDirectTorqueEncoder::new(max);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(max * 0.25, 0, &mut out);
    assert_snapshot!(format!("{out:?}"));
}

#[test]
fn test_snapshot_torque_r12_neg_half_scale() {
    let max = MozaModel::R12.max_torque_nm();
    let enc = MozaDirectTorqueEncoder::new(max);
    let mut out = [0u8; REPORT_LEN];
    enc.encode(-max * 0.5, 0, &mut out);
    assert_snapshot!(format!("{out:?}"));
}

#[test]
fn test_snapshot_torque_r21_full_scale() {
    let enc = MozaDirectTorqueEncoder::new(MozaModel::R21.max_torque_nm());
    let mut out = [0u8; REPORT_LEN];
    enc.encode(MozaModel::R21.max_torque_nm(), 0, &mut out);
    assert_snapshot!(format!("{out:?}"));
}

#[test]
fn test_snapshot_torque_zero_disabled() {
    let enc = MozaDirectTorqueEncoder::new(MozaModel::R9.max_torque_nm());
    let mut out = [0u8; REPORT_LEN];
    enc.encode(0.0, 0, &mut out);
    assert_snapshot!(format!("{out:?}"));
}

// ── Protocol packet structure snapshots ─────────────────────────────────────

#[test]
fn test_snapshot_ffb_mode_report_direct() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_ffb_mode(product_ids::R5_V1, FfbMode::Direct);
    let mut writer = MockWriter::new();
    protocol.set_ffb_mode(&mut writer, FfbMode::Direct)?;
    assert_snapshot!(format!("{:?}", writer.last));
    Ok(())
}

#[test]
fn test_snapshot_ffb_mode_report_standard() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_ffb_mode(product_ids::R9_V2, FfbMode::Standard);
    let mut writer = MockWriter::new();
    protocol.set_ffb_mode(&mut writer, FfbMode::Standard)?;
    assert_snapshot!(format!("{:?}", writer.last));
    Ok(())
}

#[test]
fn test_snapshot_high_torque_report() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new_with_config(product_ids::R12_V1, FfbMode::Direct, true);
    let mut writer = MockWriter::new();
    protocol.enable_high_torque(&mut writer)?;
    assert_snapshot!(format!("{:?}", writer.last));
    Ok(())
}

#[test]
fn test_snapshot_start_reports_command() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R9_V1);
    let mut writer = MockWriter::new();
    protocol.start_input_reports(&mut writer)?;
    assert_snapshot!(format!("{:?}", writer.last));
    Ok(())
}

#[test]
fn test_snapshot_rotation_range_900() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(product_ids::R16_R21_V2);
    let mut writer = MockWriter::new();
    protocol.set_rotation_range(&mut writer, 900)?;
    assert_snapshot!(format!("{:?}", writer.last));
    Ok(())
}

// ── is_wheelbase_product() snapshots for known PIDs ─────────────────────────

#[test]
fn test_snapshot_is_wheelbase_known_wheelbases() {
    let wheelbases = [
        product_ids::R3_V1,
        product_ids::R3_V2,
        product_ids::R5_V1,
        product_ids::R5_V2,
        product_ids::R9_V1,
        product_ids::R9_V2,
        product_ids::R12_V1,
        product_ids::R12_V2,
        product_ids::R16_R21_V1,
        product_ids::R16_R21_V2,
    ];
    let results: Vec<(u16, bool)> = wheelbases
        .iter()
        .map(|&pid| (pid, is_wheelbase_product(pid)))
        .collect();
    assert_snapshot!(format!("{results:?}"));
}

#[test]
fn test_snapshot_is_wheelbase_peripherals() {
    let peripherals = [
        product_ids::SR_P_PEDALS,
        product_ids::HGP_SHIFTER,
        product_ids::SGP_SHIFTER,
        product_ids::HBP_HANDBRAKE,
    ];
    let results: Vec<(u16, bool)> = peripherals
        .iter()
        .map(|&pid| (pid, is_wheelbase_product(pid)))
        .collect();
    assert_snapshot!(format!("{results:?}"));
}

// ── Signature verdict snapshots ──────────────────────────────────────────────

#[test]
fn test_snapshot_signature_verdict_r5_v1() {
    let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, product_ids::R5_V1);
    let verdict = verify_signature(&sig);
    assert_snapshot!(format!("{verdict:?}"));
}

#[test]
fn test_snapshot_signature_verdict_peripheral() {
    let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, product_ids::HBP_HANDBRAKE);
    let verdict = verify_signature(&sig);
    assert_snapshot!(format!("{verdict:?}"));
}

// ── ES compatibility snapshots ───────────────────────────────────────────────

#[test]
fn test_snapshot_es_compatibility_r9_v1_unsupported() {
    let compat = es_compatibility(product_ids::R9_V1);
    assert_snapshot!(format!("{compat:?}"));
}

#[test]
fn test_snapshot_es_compatibility_r5_v2_supported() {
    let compat = es_compatibility(product_ids::R5_V2);
    assert_snapshot!(format!("{compat:?}"));
}

// ── Model max torque snapshots ───────────────────────────────────────────────

#[test]
fn test_snapshot_model_max_torque_all() {
    let models = [
        MozaModel::R3,
        MozaModel::R5,
        MozaModel::R9,
        MozaModel::R12,
        MozaModel::R16,
        MozaModel::R21,
    ];
    let torques: Vec<(&str, f32)> = models
        .iter()
        .map(|m| {
            let name = match m {
                MozaModel::R3 => "R3",
                MozaModel::R5 => "R5",
                MozaModel::R9 => "R9",
                MozaModel::R12 => "R12",
                MozaModel::R16 => "R16",
                MozaModel::R21 => "R21",
                _ => "Unknown",
            };
            (name, m.max_torque_nm())
        })
        .collect();
    assert_snapshot!(format!("{torques:?}"));
}
