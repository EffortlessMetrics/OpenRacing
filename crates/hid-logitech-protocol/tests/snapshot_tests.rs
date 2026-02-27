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
fn test_snapshot_input_report_known_sequence() {
    let data = [
        0x01u8, // report ID
        0x00, 0x80, // steering center (0x8000 LE)
        0xFF, // throttle = 255 (full)
        0x80, // brake = 128 (~50 %)
        0x00, // clutch = 0
        0b0000_0101, // buttons: bits 0 and 2 set
        0x00, // buttons high byte
        0x08, // hat switch = 0x8 (neutral)
        0x03, // both paddles
        0x00, // padding
    ];
    let state = lg::parse_input_report(&data).expect("parse should succeed");
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
