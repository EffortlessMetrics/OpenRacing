use insta::assert_snapshot;
use racing_wheel_hid_logitech_protocol as lg;

// ── Constant-force encoder snapshots ─────────────────────────────────────────

#[test]
fn test_snapshot_constant_force_neg_one() {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    enc.encode(-2.2, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_constant_force_zero() {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    enc.encode(0.0, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_constant_force_pos_one() {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    enc.encode(2.2, &mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_constant_force_half() {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    enc.encode(1.1, &mut out); // 0.5 normalized → 5000
    assert_snapshot!(format!("{:?}", out));
}

// ── Input report parsing snapshot ────────────────────────────────────────────

/// Known raw G920/G923-style input report: steering center, full throttle,
/// brake half, no buttons, hat neutral (0x8), both paddles set.
#[test]
fn test_snapshot_input_report_known_sequence() -> Result<(), Box<dyn std::error::Error>> {
    let data = [
        0x01u8, // report ID
        0x00,
        0x80,        // steering center (0x8000 LE)
        0xFF,        // throttle = 255 (full)
        0x80,        // brake = 128 (~50 %)
        0x00,        // clutch = 0
        0b0000_0101, // buttons: bits 0 and 2 set
        0x00,        // buttons high byte
        0x08,        // hat switch = 0x8 (neutral)
        0x03,        // both paddles
        0x00,        // padding
    ];
    let state = lg::parse_input_report(&data).ok_or("parse_input_report returned None")?;
    assert_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, \
         buttons=0x{:04X}, hat=0x{:X}, paddles={}",
        state.steering,
        state.throttle,
        state.brake,
        state.clutch,
        state.buttons,
        state.hat,
        state.paddles,
    ));
    Ok(())
}

// ── Vendor command snapshots ──────────────────────────────────────────────────

#[test]
fn test_snapshot_native_mode_report() {
    let r = lg::build_native_mode_report();
    assert_snapshot!(format!("{:?}", r));
}

#[test]
fn test_snapshot_set_range_900() {
    let r = lg::build_set_range_report(900);
    assert_snapshot!(format!("{:?}", r));
}

#[test]
fn test_snapshot_set_autocenter_on() {
    let r = lg::build_set_autocenter_report(0x80, 0xFF);
    assert_snapshot!(format!("{:?}", r));
}

#[test]
fn test_snapshot_set_autocenter_off() {
    let r = lg::build_set_autocenter_report(0x00, 0x00);
    assert_snapshot!(format!("{:?}", r));
}

#[test]
fn test_snapshot_gain_full() {
    let r = lg::build_gain_report(0xFF);
    assert_snapshot!(format!("{:?}", r));
}

// ── DFP range command snapshots ──────────────────────────────────────────────

#[test]
fn test_snapshot_dfp_range_270() {
    let [coarse, fine] = lg::build_set_range_dfp_reports(270);
    assert_snapshot!(format!("coarse: {:02X?}\nfine:   {:02X?}", coarse, fine));
}

#[test]
fn test_snapshot_dfp_range_900() {
    let [coarse, fine] = lg::build_set_range_dfp_reports(900);
    assert_snapshot!(format!("coarse: {:02X?}\nfine:   {:02X?}", coarse, fine));
}

#[test]
fn test_snapshot_dfp_range_40_min_clamp() {
    let [coarse, fine] = lg::build_set_range_dfp_reports(40);
    assert_snapshot!(format!("coarse: {:02X?}\nfine:   {:02X?}", coarse, fine));
}

#[test]
fn test_snapshot_dfp_range_1080_clamps_to_900() {
    let [coarse, fine] = lg::build_set_range_dfp_reports(1080);
    assert_snapshot!(format!("coarse: {:02X?}\nfine:   {:02X?}", coarse, fine));
}

// ── Mode-switch command snapshots ────────────────────────────────────────────

#[test]
fn test_snapshot_mode_switch_dfex_no_detach() {
    let r = lg::build_mode_switch_report(0, false);
    assert_snapshot!(format!("{:02X?}", r));
}

#[test]
fn test_snapshot_mode_switch_g27_detach() {
    let r = lg::build_mode_switch_report(4, true);
    assert_snapshot!(format!("{:02X?}", r));
}

#[test]
fn test_snapshot_mode_switch_g29() {
    let r = lg::build_mode_switch_report(5, false);
    assert_snapshot!(format!("{:02X?}", r));
}

// ── Capability method snapshots ──────────────────────────────────────────────

#[test]
fn test_snapshot_g25_supports_hardware_friction() {
    assert_snapshot!(
        format!("G25.supports_hardware_friction() = {}", lg::LogitechModel::G25.supports_hardware_friction())
    );
}

#[test]
fn test_snapshot_g29_no_hardware_friction() {
    assert_snapshot!(
        format!("G29.supports_hardware_friction() = {}", lg::LogitechModel::G29.supports_hardware_friction())
    );
}

#[test]
fn test_snapshot_g920_supports_range_command() {
    assert_snapshot!(
        format!("G920.supports_range_command() = {}", lg::LogitechModel::G920.supports_range_command())
    );
}

#[test]
fn test_snapshot_dfex_no_range_command() {
    assert_snapshot!(
        format!("DrivingForceEX.supports_range_command() = {}", lg::LogitechModel::DrivingForceEX.supports_range_command())
    );
}
