//! Extended snapshot tests for Logitech wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering LED commands,
//! device identification, encoder boundary values, and model capability
//! queries that would detect wire-format regressions.

use insta::assert_snapshot;
use racing_wheel_hid_logitech_protocol as lg;

// ── Encoder boundary values ──────────────────────────────────────────────────

#[test]
fn test_snapshot_constant_force_negative_quarter() {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    enc.encode(-0.55, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_constant_force_clamp_above_max() {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    enc.encode(10.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_constant_force_clamp_below_neg_max() {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    enc.encode(-10.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

// ── LED report ───────────────────────────────────────────────────────────────

#[test]
fn test_snapshot_set_leds_all_on() {
    let r = lg::build_set_leds_report(0x1F);
    assert_snapshot!(format!("{r:02X?}"));
}

#[test]
fn test_snapshot_set_leds_all_off() {
    let r = lg::build_set_leds_report(0x00);
    assert_snapshot!(format!("{r:02X?}"));
}

// ── Range report boundary values ─────────────────────────────────────────────

#[test]
fn test_snapshot_set_range_270() {
    let r = lg::build_set_range_report(270);
    assert_snapshot!(format!("{r:02X?}"));
}

#[test]
fn test_snapshot_set_range_1080() {
    let r = lg::build_set_range_report(1080);
    assert_snapshot!(format!("{r:02X?}"));
}

// ── Gain report boundary ─────────────────────────────────────────────────────

#[test]
fn test_snapshot_gain_zero() {
    let r = lg::build_gain_report(0x00);
    assert_snapshot!(format!("{r:02X?}"));
}

#[test]
fn test_snapshot_gain_half() {
    let r = lg::build_gain_report(0x80);
    assert_snapshot!(format!("{r:02X?}"));
}

// ── is_wheel_product lookup ──────────────────────────────────────────────────

#[test]
fn test_snapshot_is_wheel_product_known_pids() {
    let results: Vec<String> = [
        lg::product_ids::G25,
        lg::product_ids::G27,
        lg::product_ids::G29_PS,
        lg::product_ids::G920,
        lg::product_ids::G923,
        lg::product_ids::G_PRO,
        lg::product_ids::DRIVING_FORCE_GT,
    ]
    .iter()
    .map(|&pid| format!("0x{pid:04X}={}", lg::is_wheel_product(pid)))
    .collect();
    assert_snapshot!(results.join(", "));
}

#[test]
fn test_snapshot_is_wheel_product_unknown_pid() {
    assert_snapshot!(format!("unknown={}", lg::is_wheel_product(0xFFFF)));
}

// ── Model capability matrix ──────────────────────────────────────────────────

#[test]
fn test_snapshot_model_capabilities_all() {
    let models = [
        ("G25", lg::LogitechModel::G25),
        ("G27", lg::LogitechModel::G27),
        ("G29", lg::LogitechModel::G29),
        ("G920", lg::LogitechModel::G920),
        ("G923", lg::LogitechModel::G923),
        ("GPro", lg::LogitechModel::GPro),
        ("DrivingForcePro", lg::LogitechModel::DrivingForcePro),
        ("DrivingForceEX", lg::LogitechModel::DrivingForceEX),
        ("DrivingForceGT", lg::LogitechModel::DrivingForceGT),
    ];
    let results: Vec<String> = models
        .iter()
        .map(|(name, model)| {
            format!(
                "{name}: range={}, friction={}, trueforce={}",
                model.supports_range_command(),
                model.supports_hardware_friction(),
                model.supports_trueforce(),
            )
        })
        .collect();
    assert_snapshot!(results.join("\n"));
}

// ── Autocenter boundary ──────────────────────────────────────────────────────

#[test]
fn test_snapshot_autocenter_mid_strength() {
    let r = lg::build_set_autocenter_report(0x40, 0x80);
    assert_snapshot!(format!("{r:02X?}"));
}
