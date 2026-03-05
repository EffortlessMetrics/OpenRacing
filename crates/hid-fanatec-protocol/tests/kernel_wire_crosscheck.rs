//! Cross-check tests for Fanatec slot-based FFB protocol constants against
//! the community Linux kernel driver `gotzl/hid-fanatecff` (`hid-ftecff.c`).
//!
//! These tests pin the slot IDs, effect command bytes, and encoder wire format
//! to the values defined in the kernel driver's `ftecff_update_slot()` and
//! `ftecff_timer()` functions. If any assertion fails, the `slots.rs` constants
//! have drifted from the authoritative kernel source.

use racing_wheel_hid_fanatec_protocol::{effect_cmd, slot};

// ── Slot IDs (kernel: slot_id in ftecff_effects[]) ─────────────────────────

/// Constant force = slot 0.
#[test]
fn slot_constant_is_0() {
    assert_eq!(slot::CONSTANT, 0);
}

/// Spring = slot 1.
#[test]
fn slot_spring_is_1() {
    assert_eq!(slot::SPRING, 1);
}

/// Damper = slot 2.
#[test]
fn slot_damper_is_2() {
    assert_eq!(slot::DAMPER, 2);
}

/// Inertia = slot 3.
#[test]
fn slot_inertia_is_3() {
    assert_eq!(slot::INERTIA, 3);
}

/// Friction = slot 4.
#[test]
fn slot_friction_is_4() {
    assert_eq!(slot::FRICTION, 4);
}

// ── Effect command bytes (byte 1 of 7-byte payload) ────────────────────────

/// Constant force effect cmd = 0x08 (kernel: `ftec_slot_cmd` for FF_CONSTANT).
#[test]
fn effect_cmd_constant_is_0x08() {
    assert_eq!(effect_cmd::CONSTANT, 0x08);
}

/// Spring effect cmd = 0x0B (kernel: `ftec_slot_cmd` for FF_SPRING).
#[test]
fn effect_cmd_spring_is_0x0b() {
    assert_eq!(effect_cmd::SPRING, 0x0B);
}

/// Resistance (damper/inertia/friction) effect cmd = 0x0C
/// (kernel: `ftec_slot_cmd` for FF_DAMPER, FF_INERTIA, FF_FRICTION).
#[test]
fn effect_cmd_resistance_is_0x0c() {
    assert_eq!(effect_cmd::RESISTANCE, 0x0C);
}

// ── Slot command size ──────────────────────────────────────────────────────

/// All slot commands are 7 bytes.
#[test]
fn slot_cmd_size_is_7() {
    use racing_wheel_hid_fanatec_protocol::SLOT_CMD_SIZE;
    assert_eq!(SLOT_CMD_SIZE, 7);
}

// ── Wire-format structure checks ───────────────────────────────────────────

/// Constant low-res: slot ID in upper nibble, effect cmd in byte 1.
/// Kernel: `ftecff_update_slot()` for FF_CONSTANT without FTEC_HIGHRES.
#[test]
fn constant_lowres_wire_structure() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::encode_constant_lowres;

    let cmd = encode_constant_lowres(0x4000);
    assert_eq!(cmd[0] >> 4, slot::CONSTANT, "upper nibble = slot 0");
    assert_eq!(cmd[0] & 0x01, 0x01, "active flag set for non-zero");
    assert_eq!(cmd[1], 0x08, "constant effect cmd");
    // Byte 2 = TRANSLATE_FORCE(0x4000, 8) = (0x4000 + 0x8000) >> 8 = 0xC0
    assert_eq!(cmd[2], 0xC0, "8-bit force for +0x4000");
    // Bytes 3-6 are padding (low-res has no high byte or marker)
    assert_eq!(cmd[3], 0x00);
    assert_eq!(cmd[6], 0x00, "no highres marker");
    Ok(())
}

/// Constant high-res: byte 6 = 0x01 as highres marker.
/// Kernel: `ftecff_update_slot()` for FF_CONSTANT with FTEC_HIGHRES.
#[test]
fn constant_highres_wire_structure() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::encode_constant_highres;

    let cmd = encode_constant_highres(0);
    assert_eq!(cmd[0] >> 4, slot::CONSTANT, "upper nibble = slot 0");
    assert_eq!(cmd[1], 0x08, "constant effect cmd");
    // Zero force in 16-bit = TRANSLATE_FORCE(0, 16) = 0x8000
    let force = u16::from_le_bytes([cmd[2], cmd[3]]);
    assert_eq!(force, 0x8000, "16-bit center");
    assert_eq!(cmd[6], 0x01, "highres marker byte");
    Ok(())
}

/// Spring: slot 1, effect cmd 0x0B.
/// Kernel: `ftecff_update_slot()` for FF_SPRING.
#[test]
fn spring_wire_structure() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::encode_spring;

    let cmd = encode_spring(0, 0, 0x4000, 0x4000, 0xFFFF);
    assert_eq!(cmd[0] >> 4, slot::SPRING, "upper nibble = slot 1");
    assert_eq!(cmd[1], 0x0B, "spring effect cmd");
    assert_eq!(cmd[6], 0xFF, "full saturation clip");
    Ok(())
}

/// Damper: slot 2, effect cmd 0x0C.
/// Kernel: `ftecff_update_slot()` for FF_DAMPER.
#[test]
fn damper_wire_structure() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::encode_damper;

    let cmd = encode_damper(0x2000, -0x2000, 0x8000);
    assert_eq!(cmd[0] >> 4, slot::DAMPER, "upper nibble = slot 2");
    assert_eq!(cmd[1], 0x0C, "resistance effect cmd");
    Ok(())
}

/// Inertia: slot 3, effect cmd 0x0C (same cmd as damper/friction).
#[test]
fn inertia_wire_structure() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::encode_inertia;

    let cmd = encode_inertia(0x2000, -0x2000, 0x8000);
    assert_eq!(cmd[0] >> 4, slot::INERTIA, "upper nibble = slot 3");
    assert_eq!(cmd[1], 0x0C, "resistance effect cmd");
    Ok(())
}

/// Friction: slot 4, effect cmd 0x0C.
#[test]
fn friction_wire_structure() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::encode_friction;

    let cmd = encode_friction(0x2000, -0x2000, 0x8000);
    assert_eq!(cmd[0] >> 4, slot::FRICTION, "upper nibble = slot 4");
    assert_eq!(cmd[1], 0x0C, "resistance effect cmd");
    Ok(())
}

/// Stop all effects = [0xF3, 0, 0, 0, 0, 0, 0].
/// Kernel: `ftecff_stop_effects()` sends this exact sequence.
#[test]
fn stop_all_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::encode_stop_all;

    assert_eq!(
        encode_stop_all(),
        [0xF3, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        "must match ftecff_stop_effects() output"
    );
    Ok(())
}

// ── Tuning protocol cross-checks ──────────────────────────────────────────

/// Tuning report size = 64 bytes (FTEC_TUNING_REPORT_SIZE in hid-ftec.h).
#[test]
fn tuning_report_size_is_64() {
    use racing_wheel_hid_fanatec_protocol::tuning;
    assert_eq!(tuning::TUNING_REPORT_SIZE, 64, "FTEC_TUNING_REPORT_SIZE");
}

/// Tuning report prefix: [0xFF, 0x03, ...] (from hid-ftec.c tuning handler).
#[test]
fn tuning_report_prefix() {
    use racing_wheel_hid_fanatec_protocol::tuning;
    assert_eq!(tuning::TUNING_HEADER_0, 0xFF, "tuning header byte 0");
    assert_eq!(tuning::TUNING_HEADER_1, 0x03, "tuning header byte 1");
}
