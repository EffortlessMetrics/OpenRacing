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

// ── Complete model variant matrix ────────────────────────────────────────────

/// Snapshot all 14 LogitechModel variants with their full properties.
#[test]
fn test_snapshot_all_device_variants() {
    let all_models = [
        ("WingManFormulaForce", lg::LogitechModel::WingManFormulaForce),
        ("MOMO", lg::LogitechModel::MOMO),
        ("DrivingForceEX", lg::LogitechModel::DrivingForceEX),
        ("DrivingForcePro", lg::LogitechModel::DrivingForcePro),
        ("DrivingForceGT", lg::LogitechModel::DrivingForceGT),
        ("SpeedForceWireless", lg::LogitechModel::SpeedForceWireless),
        ("VibrationWheel", lg::LogitechModel::VibrationWheel),
        ("G25", lg::LogitechModel::G25),
        ("G27", lg::LogitechModel::G27),
        ("G29", lg::LogitechModel::G29),
        ("G920", lg::LogitechModel::G920),
        ("G923", lg::LogitechModel::G923),
        ("GPro", lg::LogitechModel::GPro),
        ("Unknown", lg::LogitechModel::Unknown),
    ];
    let results: Vec<String> = all_models
        .iter()
        .map(|(name, model)| {
            format!(
                "{name}: torque={:.1}Nm, rotation={}°, range={}, friction={}, trueforce={}",
                model.max_torque_nm(),
                model.max_rotation_deg(),
                model.supports_range_command(),
                model.supports_hardware_friction(),
                model.supports_trueforce(),
            )
        })
        .collect();
    assert_snapshot!(results.join("\n"));
}

// ── Protocol constants snapshot ──────────────────────────────────────────────

/// Snapshot all protocol constants to detect accidental changes.
#[test]
fn test_snapshot_protocol_constants() {
    use racing_wheel_hid_logitech_protocol::ids::{commands, report_ids};
    let snapshot = format!(
        "LOGITECH_VENDOR_ID: 0x{:04X}\n\
         report_ids:\n\
         \x20 STANDARD_INPUT: 0x{:02X}\n\
         \x20 VENDOR: 0x{:02X}\n\
         \x20 CONSTANT_FORCE: 0x{:02X}\n\
         \x20 DEVICE_GAIN: 0x{:02X}\n\
         commands:\n\
         \x20 NATIVE_MODE: 0x{:02X}\n\
         \x20 SET_RANGE: 0x{:02X}\n\
         \x20 SET_AUTOCENTER: 0x{:02X}\n\
         \x20 SET_LEDS: 0x{:02X}\n\
         \x20 MODE_SWITCH: 0x{:02X}\n\
         sizes:\n\
         \x20 CONSTANT_FORCE_REPORT_LEN: {}\n\
         \x20 VENDOR_REPORT_LEN: {}",
        lg::LOGITECH_VENDOR_ID,
        report_ids::STANDARD_INPUT,
        report_ids::VENDOR,
        report_ids::CONSTANT_FORCE,
        report_ids::DEVICE_GAIN,
        commands::NATIVE_MODE,
        commands::SET_RANGE,
        commands::SET_AUTOCENTER,
        commands::SET_LEDS,
        commands::MODE_SWITCH,
        lg::CONSTANT_FORCE_REPORT_LEN,
        lg::VENDOR_REPORT_LEN,
    );
    assert_snapshot!(snapshot);
}

// ── PID-to-model mapping snapshot ────────────────────────────────────────────

/// Snapshot all known PIDs mapped to their model classification.
#[test]
fn test_snapshot_all_pid_to_model() {
    let pids = [
        ("MOMO", lg::product_ids::MOMO),
        ("MOMO_2", lg::product_ids::MOMO_2),
        ("WINGMAN_FFG", lg::product_ids::WINGMAN_FORMULA_FORCE_GP),
        ("WINGMAN_FF", lg::product_ids::WINGMAN_FORMULA_FORCE),
        ("VIBRATION_WHEEL", lg::product_ids::VIBRATION_WHEEL),
        ("DRIVING_FORCE_EX", lg::product_ids::DRIVING_FORCE_EX),
        ("DRIVING_FORCE_PRO", lg::product_ids::DRIVING_FORCE_PRO),
        ("DRIVING_FORCE_GT", lg::product_ids::DRIVING_FORCE_GT),
        ("SPEED_FORCE_WIRELESS", lg::product_ids::SPEED_FORCE_WIRELESS),
        ("G25", lg::product_ids::G25),
        ("G27", lg::product_ids::G27),
        ("G29_PS", lg::product_ids::G29_PS),
        ("G920", lg::product_ids::G920),
        ("G923", lg::product_ids::G923),
        ("G923_PS", lg::product_ids::G923_PS),
        ("G923_XBOX", lg::product_ids::G923_XBOX),
        ("G923_XBOX_ALT", lg::product_ids::G923_XBOX_ALT),
        ("G_PRO", lg::product_ids::G_PRO),
        ("G_PRO_XBOX", lg::product_ids::G_PRO_XBOX),
    ];
    let results: Vec<String> = pids
        .iter()
        .map(|(name, pid)| {
            let model = lg::LogitechModel::from_product_id(*pid);
            format!("{name} (0x{pid:04X}) -> {model:?}")
        })
        .collect();
    assert_snapshot!(results.join("\n"));
}
