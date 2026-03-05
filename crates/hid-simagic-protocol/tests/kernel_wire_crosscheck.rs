//! Cross-check tests for Simagic wire-format constants against the
//! JacKeTUs/simagic-ff kernel driver (`hid-simagic.c`, commit 52e73e7).
//!
//! These tests pin the `wire` module's report type IDs, block IDs, and
//! effect operation codes to the values defined in the kernel driver.
//! If any assertion fails, the `wire.rs` constants have drifted from the
//! authoritative kernel source and must be reconciled.

use racing_wheel_hid_simagic_protocol::wire;

// ── Report type IDs (byte 0 of 64-byte output report) ──────────────────────

/// SM_SET_EFFECT_REPORT = 0x01
#[test]
fn report_type_set_effect_matches_kernel() {
    assert_eq!(wire::report_type::SET_EFFECT, 0x01, "SM_SET_EFFECT_REPORT");
}

/// SM_SET_CONDITION_REPORT = 0x03
#[test]
fn report_type_set_condition_matches_kernel() {
    assert_eq!(
        wire::report_type::SET_CONDITION,
        0x03,
        "SM_SET_CONDITION_REPORT"
    );
}

/// SM_SET_PERIODIC_REPORT = 0x04
#[test]
fn report_type_set_periodic_matches_kernel() {
    assert_eq!(
        wire::report_type::SET_PERIODIC,
        0x04,
        "SM_SET_PERIODIC_REPORT"
    );
}

/// SM_SET_CONSTANT_REPORT = 0x05
#[test]
fn report_type_set_constant_matches_kernel() {
    assert_eq!(
        wire::report_type::SET_CONSTANT,
        0x05,
        "SM_SET_CONSTANT_REPORT"
    );
}

/// SM_EFFECT_OPERATION_REPORT = 0x0A
#[test]
fn report_type_effect_operation_matches_kernel() {
    assert_eq!(
        wire::report_type::EFFECT_OPERATION,
        0x0A,
        "SM_EFFECT_OPERATION_REPORT"
    );
}

/// SM_SET_ENVELOPE_REPORT = 0x12
#[test]
fn report_type_set_envelope_matches_kernel() {
    assert_eq!(
        wire::report_type::SET_ENVELOPE,
        0x12,
        "SM_SET_ENVELOPE_REPORT"
    );
}

/// SM_SET_RAMP_FORCE_REPORT = 0x16
#[test]
fn report_type_set_ramp_force_matches_kernel() {
    assert_eq!(
        wire::report_type::SET_RAMP_FORCE,
        0x16,
        "SM_SET_RAMP_FORCE_REPORT"
    );
}

/// SM_SET_CUSTOM_FORCE_REPORT = 0x17
#[test]
fn report_type_set_custom_force_matches_kernel() {
    assert_eq!(
        wire::report_type::SET_CUSTOM_FORCE,
        0x17,
        "SM_SET_CUSTOM_FORCE_REPORT"
    );
}

/// SM_SET_GAIN = 0x40
#[test]
fn report_type_set_gain_matches_kernel() {
    assert_eq!(wire::report_type::SET_GAIN, 0x40, "SM_SET_GAIN");
}

// ── Block IDs (effect type in value[1]) ────────────────────────────────────

/// SM_CONSTANT = 0x01
#[test]
fn block_id_constant_matches_kernel() {
    assert_eq!(wire::block_id::CONSTANT, 0x01, "SM_CONSTANT");
}

/// SM_SINE = 0x02
#[test]
fn block_id_sine_matches_kernel() {
    assert_eq!(wire::block_id::SINE, 0x02, "SM_SINE");
}

/// SM_DAMPER = 0x05
#[test]
fn block_id_damper_matches_kernel() {
    assert_eq!(wire::block_id::DAMPER, 0x05, "SM_DAMPER");
}

/// SM_SPRING = 0x06
#[test]
fn block_id_spring_matches_kernel() {
    assert_eq!(wire::block_id::SPRING, 0x06, "SM_SPRING");
}

/// SM_FRICTION = 0x07
#[test]
fn block_id_friction_matches_kernel() {
    assert_eq!(wire::block_id::FRICTION, 0x07, "SM_FRICTION");
}

/// SM_INERTIA = 0x09
#[test]
fn block_id_inertia_matches_kernel() {
    assert_eq!(wire::block_id::INERTIA, 0x09, "SM_INERTIA");
}

/// SM_RAMP_FORCE = 0x0E
#[test]
fn block_id_ramp_matches_kernel() {
    assert_eq!(wire::block_id::RAMP, 0x0E, "SM_RAMP_FORCE");
}

/// SM_SQUARE = 0x0F
#[test]
fn block_id_square_matches_kernel() {
    assert_eq!(wire::block_id::SQUARE, 0x0F, "SM_SQUARE");
}

/// SM_TRIANGLE = 0x10
#[test]
fn block_id_triangle_matches_kernel() {
    assert_eq!(wire::block_id::TRIANGLE, 0x10, "SM_TRIANGLE");
}

/// SM_SAWTOOTH_UP = 0x11
#[test]
fn block_id_sawtooth_up_matches_kernel() {
    assert_eq!(wire::block_id::SAWTOOTH_UP, 0x11, "SM_SAWTOOTH_UP");
}

/// SM_SAWTOOTH_DOWN = 0x12
#[test]
fn block_id_sawtooth_down_matches_kernel() {
    assert_eq!(wire::block_id::SAWTOOTH_DOWN, 0x12, "SM_SAWTOOTH_DOWN");
}

// ── Effect operation codes ─────────────────────────────────────────────────

#[test]
fn effect_op_start_matches_kernel() {
    assert_eq!(wire::effect_op::START, 0x01);
}

#[test]
fn effect_op_stop_matches_kernel() {
    assert_eq!(wire::effect_op::STOP, 0x03);
}

// ── Report size ────────────────────────────────────────────────────────────

/// All Simagic output reports are 64 bytes.
#[test]
fn report_size_is_64() {
    assert_eq!(wire::REPORT_SIZE, 64);
}

// ── Encoder output cross-checks ────────────────────────────────────────────

/// Verify constant force encoder produces the correct report type and block ID.
#[test]
fn encode_constant_has_correct_header() -> Result<(), Box<dyn std::error::Error>> {
    let buf = wire::encode_constant(0x4000);
    assert_eq!(buf[0], 0x05, "report type must be SM_SET_CONSTANT_REPORT");
    assert_eq!(buf[1], 0x01, "block ID must be SM_CONSTANT");
    Ok(())
}

/// Verify condition encoder produces the correct report type.
#[test]
fn encode_condition_has_correct_header() -> Result<(), Box<dyn std::error::Error>> {
    let params = wire::ConditionParams {
        center: 0,
        right_coeff: 0x4000,
        left_coeff: -0x4000,
        right_saturation: 0xFFFF,
        left_saturation: 0x8000,
        deadband: 0x1000,
    };
    let buf = wire::encode_condition(wire::block_id::SPRING, &params);
    assert_eq!(buf[0], 0x03, "report type must be SM_SET_CONDITION_REPORT");
    assert_eq!(buf[1], wire::block_id::SPRING);
    assert_eq!(buf[2], 0x00, "byte 2 always 0x00 per kernel driver");
    Ok(())
}

/// Verify periodic encoder produces the correct report type.
#[test]
fn encode_periodic_has_correct_header() -> Result<(), Box<dyn std::error::Error>> {
    let buf = wire::encode_periodic(wire::block_id::SINE, 0x7FFF, 0, 0, 100);
    assert_eq!(buf[0], 0x04, "report type must be SM_SET_PERIODIC_REPORT");
    assert_eq!(buf[1], wire::block_id::SINE);
    Ok(())
}

/// Verify set-effect encoder produces the correct report type and fixed fields.
#[test]
fn encode_set_effect_has_correct_structure() -> Result<(), Box<dyn std::error::Error>> {
    let buf = wire::encode_set_effect(wire::block_id::CONSTANT, 1000);
    assert_eq!(buf[0], 0x01, "report type must be SM_SET_EFFECT_REPORT");
    assert_eq!(buf[1], wire::block_id::CONSTANT);
    assert_eq!(buf[2], 0x01, "byte 2 always 1 per kernel driver");
    assert_eq!(buf[9], 0xFF, "gain always 0xFF per kernel driver");
    assert_eq!(
        buf[10], 0xFF,
        "trigger button always 0xFF per kernel driver"
    );
    Ok(())
}

/// Verify gain encoder uses correct report type and byte layout.
#[test]
fn encode_gain_has_correct_header() -> Result<(), Box<dyn std::error::Error>> {
    let buf = wire::encode_gain(0xFFFF);
    assert_eq!(buf[0], 0x40, "report type must be SM_SET_GAIN");
    assert_eq!(buf[1], 0xFF, "gain >> 8");
    Ok(())
}

/// Verify effect operation encoder produces correct report type and opcodes.
#[test]
fn encode_effect_operation_start_stop() -> Result<(), Box<dyn std::error::Error>> {
    let start = wire::encode_effect_operation(wire::block_id::CONSTANT, true, 1);
    assert_eq!(
        start[0], 0x0A,
        "report type must be SM_EFFECT_OPERATION_REPORT"
    );
    assert_eq!(start[2], 0x01, "start op");
    assert_eq!(start[3], 1, "loop count");

    let stop = wire::encode_effect_operation(wire::block_id::DAMPER, false, 0);
    assert_eq!(
        stop[0], 0x0A,
        "report type must be SM_EFFECT_OPERATION_REPORT"
    );
    assert_eq!(stop[2], 0x03, "stop op");
    Ok(())
}

// ── Settings report IDs ────────────────────────────────────────────────────

/// Settings Feature Report IDs match the kernel driver.
#[test]
fn settings_report_ids_match_kernel() {
    use racing_wheel_hid_simagic_protocol::settings;
    assert_eq!(settings::SET_REPORT_ID, 0x80, "settings set report ID");
    assert_eq!(settings::GET_REPORT_ID, 0x81, "settings get report ID");
    assert_eq!(settings::REPORT_SIZE, 64, "settings report size");
}

// ── Rescale function cross-checks ──────────────────────────────────────────

/// Kernel's sm_rescale_signed_to_10k known values.
#[test]
fn rescale_known_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(wire::rescale_signed_to_10k(0), 0);
    assert_eq!(wire::rescale_signed_to_10k(0x7FFF), 10000);
    assert_eq!(wire::rescale_signed_to_10k(i16::MIN), -10000);
    Ok(())
}
