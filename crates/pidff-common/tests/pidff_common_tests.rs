//! Deep integration tests for the openracing-pidff-common crate.
//!
//! Covers: PID FFB descriptor parsing, effect upload/download encoding,
//! condition report encoding, envelope/timing, common PIDFF protocol
//! shared by VRS/OpenFFBoard/Simucube/Cammus, and proptest fuzzing.

use openracing_pidff_common::*;

// ── PID FFB descriptor parsing: report IDs ───────────────────────────────────

#[test]
fn report_ids_match_usb_hid_pid_spec() -> Result<(), String> {
    let expected = [
        (report_ids::SET_EFFECT, 0x01, "SET_EFFECT"),
        (report_ids::SET_ENVELOPE, 0x02, "SET_ENVELOPE"),
        (report_ids::SET_CONDITION, 0x03, "SET_CONDITION"),
        (report_ids::SET_PERIODIC, 0x04, "SET_PERIODIC"),
        (report_ids::SET_CONSTANT_FORCE, 0x05, "SET_CONSTANT_FORCE"),
        (report_ids::SET_RAMP_FORCE, 0x06, "SET_RAMP_FORCE"),
        (report_ids::EFFECT_OPERATION, 0x0A, "EFFECT_OPERATION"),
        (report_ids::BLOCK_FREE, 0x0B, "BLOCK_FREE"),
        (report_ids::DEVICE_CONTROL, 0x0C, "DEVICE_CONTROL"),
        (report_ids::DEVICE_GAIN, 0x0D, "DEVICE_GAIN"),
        (report_ids::CREATE_NEW_EFFECT, 0x11, "CREATE_NEW_EFFECT"),
        (report_ids::BLOCK_LOAD, 0x12, "BLOCK_LOAD"),
        (report_ids::PID_POOL, 0x13, "PID_POOL"),
    ];
    for (actual, spec_val, name) in expected {
        if actual != spec_val {
            return Err(format!("{name}: expected {spec_val:#x}, got {actual:#x}"));
        }
    }
    Ok(())
}

#[test]
fn report_ids_are_all_unique() -> Result<(), String> {
    let ids = [
        report_ids::SET_EFFECT,
        report_ids::SET_ENVELOPE,
        report_ids::SET_CONDITION,
        report_ids::SET_PERIODIC,
        report_ids::SET_CONSTANT_FORCE,
        report_ids::SET_RAMP_FORCE,
        report_ids::EFFECT_OPERATION,
        report_ids::BLOCK_FREE,
        report_ids::DEVICE_CONTROL,
        report_ids::DEVICE_GAIN,
        report_ids::CREATE_NEW_EFFECT,
        report_ids::BLOCK_LOAD,
        report_ids::PID_POOL,
    ];
    for (i, &a) in ids.iter().enumerate() {
        for (j, &b) in ids.iter().enumerate() {
            if i != j && a == b {
                return Err(format!("duplicate report ID {a:#x} at indices {i} and {j}"));
            }
        }
    }
    Ok(())
}

// ── PID FFB descriptor parsing: effect types ─────────────────────────────────

#[test]
fn effect_type_values_match_spec() -> Result<(), String> {
    let expected = [
        (EffectType::Constant, 1),
        (EffectType::Ramp, 2),
        (EffectType::Square, 3),
        (EffectType::Sine, 4),
        (EffectType::Triangle, 5),
        (EffectType::SawtoothUp, 6),
        (EffectType::SawtoothDown, 7),
        (EffectType::Spring, 8),
        (EffectType::Damper, 9),
        (EffectType::Inertia, 10),
        (EffectType::Friction, 11),
    ];
    for (etype, val) in expected {
        if etype as u8 != val {
            return Err(format!("{:?}: expected {val}, got {}", etype, etype as u8));
        }
    }
    Ok(())
}

#[test]
fn effect_op_values_match_spec() -> Result<(), String> {
    if EffectOp::Start as u8 != 1 {
        return Err("Start".into());
    }
    if EffectOp::StartSolo as u8 != 2 {
        return Err("StartSolo".into());
    }
    if EffectOp::Stop as u8 != 3 {
        return Err("Stop".into());
    }
    Ok(())
}

#[test]
fn block_load_status_values() -> Result<(), String> {
    if BlockLoadStatus::Success as u8 != 1 {
        return Err("Success".into());
    }
    if BlockLoadStatus::Full as u8 != 2 {
        return Err("Full".into());
    }
    if BlockLoadStatus::Error as u8 != 3 {
        return Err("Error".into());
    }
    Ok(())
}

#[test]
fn duration_infinite_is_max_u16() -> Result<(), String> {
    if DURATION_INFINITE != 0xFFFF {
        return Err(format!("expected 0xFFFF, got {DURATION_INFINITE:#x}"));
    }
    Ok(())
}

// ── Device control flags ─────────────────────────────────────────────────────

#[test]
fn device_control_flags_match_spec() -> Result<(), String> {
    if device_control::ENABLE_ACTUATORS != 0x01 {
        return Err("ENABLE_ACTUATORS".into());
    }
    if device_control::DISABLE_ACTUATORS != 0x02 {
        return Err("DISABLE_ACTUATORS".into());
    }
    if device_control::STOP_ALL_EFFECTS != 0x04 {
        return Err("STOP_ALL_EFFECTS".into());
    }
    if device_control::DEVICE_RESET != 0x08 {
        return Err("DEVICE_RESET".into());
    }
    if device_control::DEVICE_PAUSE != 0x10 {
        return Err("DEVICE_PAUSE".into());
    }
    if device_control::DEVICE_CONTINUE != 0x20 {
        return Err("DEVICE_CONTINUE".into());
    }
    Ok(())
}

#[test]
fn device_control_flags_are_single_bit() -> Result<(), String> {
    let flags = [
        device_control::ENABLE_ACTUATORS,
        device_control::DISABLE_ACTUATORS,
        device_control::STOP_ALL_EFFECTS,
        device_control::DEVICE_RESET,
        device_control::DEVICE_PAUSE,
        device_control::DEVICE_CONTINUE,
    ];
    for &flag in &flags {
        if flag.count_ones() != 1 {
            return Err(format!("flag {flag:#x} is not a single-bit value"));
        }
    }
    Ok(())
}

#[test]
fn device_control_flags_combinable() -> Result<(), String> {
    let combined = device_control::ENABLE_ACTUATORS | device_control::STOP_ALL_EFFECTS;
    if combined != 0x05 {
        return Err(format!("expected 0x05, got {combined:#x}"));
    }
    Ok(())
}

// ── Report sizes ─────────────────────────────────────────────────────────────

#[test]
fn report_sizes_match_pid_spec() -> Result<(), String> {
    if SET_EFFECT_LEN != 14 {
        return Err("SET_EFFECT_LEN".into());
    }
    if SET_ENVELOPE_LEN != 10 {
        return Err("SET_ENVELOPE_LEN".into());
    }
    if SET_CONDITION_LEN != 14 {
        return Err("SET_CONDITION_LEN".into());
    }
    if SET_PERIODIC_LEN != 10 {
        return Err("SET_PERIODIC_LEN".into());
    }
    if SET_CONSTANT_FORCE_LEN != 4 {
        return Err("SET_CONSTANT_FORCE_LEN".into());
    }
    if SET_RAMP_FORCE_LEN != 6 {
        return Err("SET_RAMP_FORCE_LEN".into());
    }
    if EFFECT_OPERATION_LEN != 4 {
        return Err("EFFECT_OPERATION_LEN".into());
    }
    if DEVICE_CONTROL_LEN != 2 {
        return Err("DEVICE_CONTROL_LEN".into());
    }
    if DEVICE_GAIN_LEN != 4 {
        return Err("DEVICE_GAIN_LEN".into());
    }
    if BLOCK_FREE_LEN != 2 {
        return Err("BLOCK_FREE_LEN".into());
    }
    if CREATE_NEW_EFFECT_LEN != 2 {
        return Err("CREATE_NEW_EFFECT_LEN".into());
    }
    Ok(())
}

// ── Effect upload encoding: Set Effect ───────────────────────────────────────

#[test]
fn set_effect_constant_infinite_layout() -> Result<(), String> {
    let buf = encode_set_effect(1, EffectType::Constant, DURATION_INFINITE, 255, 0);
    if buf.len() != SET_EFFECT_LEN {
        return Err(format!("wrong length: {}", buf.len()));
    }
    if buf[0] != report_ids::SET_EFFECT {
        return Err("wrong report ID".into());
    }
    if buf[1] != 1 {
        return Err("wrong block_index".into());
    }
    if buf[2] != EffectType::Constant as u8 {
        return Err("wrong effect_type".into());
    }
    let dur = u16::from_le_bytes([buf[3], buf[4]]);
    if dur != DURATION_INFINITE {
        return Err(format!("wrong duration: {dur}"));
    }
    if buf[9] != 255 {
        return Err("wrong gain".into());
    }
    if buf[10] != 0xFF {
        return Err("trigger button should be 0xFF (none)".into());
    }
    let dir = u16::from_le_bytes([buf[11], buf[12]]);
    if dir != 0 {
        return Err(format!("wrong direction: {dir}"));
    }
    Ok(())
}

#[test]
fn set_effect_all_types_report_id() -> Result<(), String> {
    let types = [
        EffectType::Constant,
        EffectType::Ramp,
        EffectType::Square,
        EffectType::Sine,
        EffectType::Triangle,
        EffectType::SawtoothUp,
        EffectType::SawtoothDown,
        EffectType::Spring,
        EffectType::Damper,
        EffectType::Inertia,
        EffectType::Friction,
    ];
    for etype in types {
        let buf = encode_set_effect(1, etype, 1000, 128, 0);
        if buf[0] != report_ids::SET_EFFECT {
            return Err(format!("{:?}: wrong report ID", etype));
        }
        if buf[2] != etype as u8 {
            return Err(format!("{:?}: wrong type byte", etype));
        }
    }
    Ok(())
}

#[test]
fn set_effect_direction_full_range() -> Result<(), String> {
    // Direction is in hundredths of degrees: 0-35999
    for &dir_val in &[0u16, 9000, 18000, 27000, 35999] {
        let buf = encode_set_effect(1, EffectType::Sine, 1000, 128, dir_val);
        let encoded = u16::from_le_bytes([buf[11], buf[12]]);
        if encoded != dir_val {
            return Err(format!("direction {dir_val}: got {encoded}"));
        }
    }
    Ok(())
}

#[test]
fn set_effect_duration_boundary_values() -> Result<(), String> {
    for &dur_val in &[0u16, 1, 1000, 30000, 0xFFFE, DURATION_INFINITE] {
        let buf = encode_set_effect(1, EffectType::Constant, dur_val, 128, 0);
        let encoded = u16::from_le_bytes([buf[3], buf[4]]);
        if encoded != dur_val {
            return Err(format!("duration {dur_val}: got {encoded}"));
        }
    }
    Ok(())
}

// ── Effect upload encoding: Constant Force ───────────────────────────────────

#[test]
fn constant_force_magnitude_roundtrip() -> Result<(), String> {
    let test_values: &[i16] = &[0, 1, -1, 100, -100, 10000, -10000, i16::MAX, i16::MIN];
    for &mag in test_values {
        let buf = encode_set_constant_force(1, mag);
        if buf[0] != report_ids::SET_CONSTANT_FORCE {
            return Err(format!("mag {mag}: wrong report ID"));
        }
        if buf[1] != 1 {
            return Err(format!("mag {mag}: wrong block_index"));
        }
        let decoded = i16::from_le_bytes([buf[2], buf[3]]);
        if decoded != mag {
            return Err(format!("mag {mag}: decoded as {decoded}"));
        }
    }
    Ok(())
}

#[test]
fn constant_force_block_index_preserved() -> Result<(), String> {
    for idx in [0u8, 1, 10, 127, 255] {
        let buf = encode_set_constant_force(idx, 5000);
        if buf[1] != idx {
            return Err(format!("block_index {idx}: got {}", buf[1]));
        }
    }
    Ok(())
}

// ── Effect upload encoding: Ramp Force ───────────────────────────────────────

#[test]
fn ramp_force_start_end_preserved() -> Result<(), String> {
    let cases: &[(i16, i16)] = &[
        (0, 0),
        (-5000, 5000),
        (5000, -5000),
        (i16::MIN, i16::MAX),
        (i16::MAX, i16::MIN),
    ];
    for &(start, end) in cases {
        let buf = encode_set_ramp_force(1, start, end);
        if buf[0] != report_ids::SET_RAMP_FORCE {
            return Err("wrong report ID".into());
        }
        let s = i16::from_le_bytes([buf[2], buf[3]]);
        let e = i16::from_le_bytes([buf[4], buf[5]]);
        if s != start {
            return Err(format!("start {start}: got {s}"));
        }
        if e != end {
            return Err(format!("end {end}: got {e}"));
        }
    }
    Ok(())
}

#[test]
fn ramp_force_report_length() -> Result<(), String> {
    let buf = encode_set_ramp_force(1, 0, 0);
    if buf.len() != SET_RAMP_FORCE_LEN {
        return Err(format!("expected {SET_RAMP_FORCE_LEN}, got {}", buf.len()));
    }
    Ok(())
}

// ── Effect upload encoding: Periodic ─────────────────────────────────────────

#[test]
fn periodic_all_params_preserved() -> Result<(), String> {
    let buf = encode_set_periodic(2, 7500, -2000, 9000, 250);
    if buf[0] != report_ids::SET_PERIODIC {
        return Err("wrong report ID".into());
    }
    if buf[1] != 2 {
        return Err("wrong block_index".into());
    }
    let mag = u16::from_le_bytes([buf[2], buf[3]]);
    let offset = i16::from_le_bytes([buf[4], buf[5]]);
    let phase = u16::from_le_bytes([buf[6], buf[7]]);
    let period = u16::from_le_bytes([buf[8], buf[9]]);
    if mag != 7500 {
        return Err(format!("magnitude: {mag}"));
    }
    if offset != -2000 {
        return Err(format!("offset: {offset}"));
    }
    if phase != 9000 {
        return Err(format!("phase: {phase}"));
    }
    if period != 250 {
        return Err(format!("period: {period}"));
    }
    Ok(())
}

#[test]
fn periodic_extreme_values() -> Result<(), String> {
    let buf = encode_set_periodic(1, u16::MAX, i16::MIN, u16::MAX, u16::MAX);
    let mag = u16::from_le_bytes([buf[2], buf[3]]);
    let offset = i16::from_le_bytes([buf[4], buf[5]]);
    let phase = u16::from_le_bytes([buf[6], buf[7]]);
    let period = u16::from_le_bytes([buf[8], buf[9]]);
    if mag != u16::MAX {
        return Err("magnitude".into());
    }
    if offset != i16::MIN {
        return Err("offset".into());
    }
    if phase != u16::MAX {
        return Err("phase".into());
    }
    if period != u16::MAX {
        return Err("period".into());
    }
    Ok(())
}

// ── Condition report encoding ────────────────────────────────────────────────

#[test]
fn condition_all_params_preserved() -> Result<(), String> {
    let buf = encode_set_condition(1, 0, -500, 3000, -2000, 10000, 8000, 50);
    if buf.len() != SET_CONDITION_LEN {
        return Err(format!("wrong len: {}", buf.len()));
    }
    if buf[0] != report_ids::SET_CONDITION {
        return Err("wrong report ID".into());
    }
    if buf[1] != 1 {
        return Err("wrong block_index".into());
    }
    if buf[2] != 0 {
        return Err("wrong axis".into());
    }
    let center = i16::from_le_bytes([buf[3], buf[4]]);
    let pos_c = i16::from_le_bytes([buf[5], buf[6]]);
    let neg_c = i16::from_le_bytes([buf[7], buf[8]]);
    let pos_s = u16::from_le_bytes([buf[9], buf[10]]);
    let neg_s = u16::from_le_bytes([buf[11], buf[12]]);
    if center != -500 {
        return Err(format!("center: {center}"));
    }
    if pos_c != 3000 {
        return Err(format!("pos_coeff: {pos_c}"));
    }
    if neg_c != -2000 {
        return Err(format!("neg_coeff: {neg_c}"));
    }
    if pos_s != 10000 {
        return Err(format!("pos_sat: {pos_s}"));
    }
    if neg_s != 8000 {
        return Err(format!("neg_sat: {neg_s}"));
    }
    if buf[13] != 50 {
        return Err(format!("dead_band: {}", buf[13]));
    }
    Ok(())
}

#[test]
fn condition_y_axis() -> Result<(), String> {
    let buf = encode_set_condition(1, 1, 0, 0, 0, 0, 0, 0);
    if buf[2] != 1 {
        return Err(format!("expected axis 1, got {}", buf[2]));
    }
    Ok(())
}

#[test]
fn condition_symmetric_coefficients() -> Result<(), String> {
    let buf = encode_set_condition(1, 0, 0, 5000, -5000, 10000, 10000, 0);
    let pos = i16::from_le_bytes([buf[5], buf[6]]);
    let neg = i16::from_le_bytes([buf[7], buf[8]]);
    if pos != -neg {
        return Err(format!("not symmetric: pos={pos}, neg={neg}"));
    }
    Ok(())
}

#[test]
fn condition_extreme_values() -> Result<(), String> {
    let buf = encode_set_condition(
        255,
        0xFF,
        i16::MIN,
        i16::MAX,
        i16::MIN,
        u16::MAX,
        u16::MAX,
        255,
    );
    if buf[1] != 255 {
        return Err("block_index".into());
    }
    if buf[2] != 0xFF {
        return Err("axis".into());
    }
    let center = i16::from_le_bytes([buf[3], buf[4]]);
    if center != i16::MIN {
        return Err("center".into());
    }
    let pos_c = i16::from_le_bytes([buf[5], buf[6]]);
    if pos_c != i16::MAX {
        return Err("pos_coeff".into());
    }
    if buf[13] != 255 {
        return Err("dead_band".into());
    }
    Ok(())
}

#[test]
fn condition_zero_deadband() -> Result<(), String> {
    let buf = encode_set_condition(1, 0, 0, 1000, -1000, 5000, 5000, 0);
    if buf[13] != 0 {
        return Err(format!("expected deadband 0, got {}", buf[13]));
    }
    Ok(())
}

// ── Envelope / timing ────────────────────────────────────────────────────────

#[test]
fn envelope_all_params_preserved() -> Result<(), String> {
    let buf = encode_set_envelope(1, 5000, 8000, 100, 200);
    if buf.len() != SET_ENVELOPE_LEN {
        return Err(format!("wrong len: {}", buf.len()));
    }
    if buf[0] != report_ids::SET_ENVELOPE {
        return Err("wrong report ID".into());
    }
    if buf[1] != 1 {
        return Err("wrong block_index".into());
    }
    let attack = u16::from_le_bytes([buf[2], buf[3]]);
    let fade = u16::from_le_bytes([buf[4], buf[5]]);
    let at_ms = u16::from_le_bytes([buf[6], buf[7]]);
    let ft_ms = u16::from_le_bytes([buf[8], buf[9]]);
    if attack != 5000 {
        return Err(format!("attack: {attack}"));
    }
    if fade != 8000 {
        return Err(format!("fade: {fade}"));
    }
    if at_ms != 100 {
        return Err(format!("attack_time: {at_ms}"));
    }
    if ft_ms != 200 {
        return Err(format!("fade_time: {ft_ms}"));
    }
    Ok(())
}

#[test]
fn envelope_zero_timing_is_instant() -> Result<(), String> {
    let buf = encode_set_envelope(1, 10000, 0, 0, 0);
    let at_ms = u16::from_le_bytes([buf[6], buf[7]]);
    let ft_ms = u16::from_le_bytes([buf[8], buf[9]]);
    if at_ms != 0 {
        return Err("attack_time should be 0".into());
    }
    if ft_ms != 0 {
        return Err("fade_time should be 0".into());
    }
    Ok(())
}

#[test]
fn envelope_extreme_timing() -> Result<(), String> {
    let buf = encode_set_envelope(1, u16::MAX, u16::MAX, u16::MAX, u16::MAX);
    let attack = u16::from_le_bytes([buf[2], buf[3]]);
    let fade = u16::from_le_bytes([buf[4], buf[5]]);
    let at_ms = u16::from_le_bytes([buf[6], buf[7]]);
    let ft_ms = u16::from_le_bytes([buf[8], buf[9]]);
    if attack != u16::MAX {
        return Err("attack".into());
    }
    if fade != u16::MAX {
        return Err("fade".into());
    }
    if at_ms != u16::MAX {
        return Err("attack_time".into());
    }
    if ft_ms != u16::MAX {
        return Err("fade_time".into());
    }
    Ok(())
}

#[test]
fn envelope_no_fade() -> Result<(), String> {
    let buf = encode_set_envelope(1, 10000, 10000, 500, 0);
    let fade_time = u16::from_le_bytes([buf[8], buf[9]]);
    if fade_time != 0 {
        return Err(format!("expected fade_time 0, got {fade_time}"));
    }
    Ok(())
}

// ── Common PIDFF protocol: effect operations ─────────────────────────────────

#[test]
fn effect_operation_all_ops() -> Result<(), String> {
    let ops = [
        (EffectOp::Start, 1u8),
        (EffectOp::StartSolo, 2),
        (EffectOp::Stop, 3),
    ];
    for (op, expected) in ops {
        let buf = encode_effect_operation(1, op, 0);
        if buf[0] != report_ids::EFFECT_OPERATION {
            return Err(format!("{:?}: wrong report ID", op));
        }
        if buf[2] != expected {
            return Err(format!("{:?}: expected {expected}, got {}", op, buf[2]));
        }
    }
    Ok(())
}

#[test]
fn effect_operation_loop_count() -> Result<(), String> {
    for count in [0u8, 1, 5, 127, 255] {
        let buf = encode_effect_operation(1, EffectOp::Start, count);
        if buf[3] != count {
            return Err(format!("loop {count}: got {}", buf[3]));
        }
    }
    Ok(())
}

#[test]
fn effect_operation_length() -> Result<(), String> {
    let buf = encode_effect_operation(1, EffectOp::Start, 0);
    if buf.len() != EFFECT_OPERATION_LEN {
        return Err(format!(
            "expected {EFFECT_OPERATION_LEN}, got {}",
            buf.len()
        ));
    }
    Ok(())
}

// ── Common PIDFF protocol: block free ────────────────────────────────────────

#[test]
fn block_free_encoding() -> Result<(), String> {
    for idx in [0u8, 1, 10, 127, 255] {
        let buf = encode_block_free(idx);
        if buf.len() != BLOCK_FREE_LEN {
            return Err(format!("wrong len for idx {idx}"));
        }
        if buf[0] != report_ids::BLOCK_FREE {
            return Err(format!("wrong report ID for idx {idx}"));
        }
        if buf[1] != idx {
            return Err(format!("idx {idx}: got {}", buf[1]));
        }
    }
    Ok(())
}

// ── Common PIDFF protocol: device control ────────────────────────────────────

#[test]
fn device_control_all_commands() -> Result<(), String> {
    let cmds = [
        device_control::ENABLE_ACTUATORS,
        device_control::DISABLE_ACTUATORS,
        device_control::STOP_ALL_EFFECTS,
        device_control::DEVICE_RESET,
        device_control::DEVICE_PAUSE,
        device_control::DEVICE_CONTINUE,
    ];
    for &cmd in &cmds {
        let buf = encode_device_control(cmd);
        if buf.len() != DEVICE_CONTROL_LEN {
            return Err(format!("wrong len for cmd {cmd:#x}"));
        }
        if buf[0] != report_ids::DEVICE_CONTROL {
            return Err(format!("wrong report ID for cmd {cmd:#x}"));
        }
        if buf[1] != cmd {
            return Err(format!("cmd {cmd:#x}: got {:#x}", buf[1]));
        }
    }
    Ok(())
}

// ── Common PIDFF protocol: device gain ───────────────────────────────────────

#[test]
fn device_gain_within_range() -> Result<(), String> {
    for &gain in &[0u16, 1, 5000, 9999, 10000] {
        let buf = encode_device_gain(gain);
        let encoded = u16::from_le_bytes([buf[2], buf[3]]);
        if encoded != gain {
            return Err(format!("gain {gain}: got {encoded}"));
        }
    }
    Ok(())
}

#[test]
fn device_gain_clamps_above_10000() -> Result<(), String> {
    for &gain in &[10001u16, 20000, u16::MAX] {
        let buf = encode_device_gain(gain);
        let encoded = u16::from_le_bytes([buf[2], buf[3]]);
        if encoded != 10000 {
            return Err(format!("gain {gain}: expected 10000, got {encoded}"));
        }
    }
    Ok(())
}

#[test]
fn device_gain_report_layout() -> Result<(), String> {
    let buf = encode_device_gain(5000);
    if buf.len() != DEVICE_GAIN_LEN {
        return Err(format!("wrong len: {}", buf.len()));
    }
    if buf[0] != report_ids::DEVICE_GAIN {
        return Err("wrong report ID".into());
    }
    Ok(())
}

// ── Create new effect ────────────────────────────────────────────────────────

#[test]
fn create_new_effect_all_types() -> Result<(), String> {
    let types = [
        EffectType::Constant,
        EffectType::Ramp,
        EffectType::Square,
        EffectType::Sine,
        EffectType::Triangle,
        EffectType::SawtoothUp,
        EffectType::SawtoothDown,
        EffectType::Spring,
        EffectType::Damper,
        EffectType::Inertia,
        EffectType::Friction,
    ];
    for etype in types {
        let buf = encode_create_new_effect(etype);
        if buf.len() != CREATE_NEW_EFFECT_LEN {
            return Err(format!("{:?}: wrong len", etype));
        }
        if buf[0] != report_ids::CREATE_NEW_EFFECT {
            return Err(format!("{:?}: wrong report ID", etype));
        }
        if buf[1] != etype as u8 {
            return Err(format!("{:?}: wrong type byte", etype));
        }
    }
    Ok(())
}

// ── Block load parsing ───────────────────────────────────────────────────────

#[test]
fn block_load_success() -> Result<(), String> {
    let buf = [0x12, 3, 1, 0x00, 0x10];
    let r = parse_block_load(&buf).ok_or("parse failed")?;
    if r.block_index != 3 {
        return Err("wrong block_index".into());
    }
    if r.status != BlockLoadStatus::Success {
        return Err("wrong status".into());
    }
    if r.ram_pool_available != 0x1000 {
        return Err(format!("wrong ram: {}", r.ram_pool_available));
    }
    Ok(())
}

#[test]
fn block_load_full_status() -> Result<(), String> {
    let buf = [0x12, 0, 2, 0x00, 0x00];
    let r = parse_block_load(&buf).ok_or("parse failed")?;
    if r.status != BlockLoadStatus::Full {
        return Err("expected Full status".into());
    }
    Ok(())
}

#[test]
fn block_load_error_status() -> Result<(), String> {
    let buf = [0x12, 0, 3, 0x00, 0x00];
    let r = parse_block_load(&buf).ok_or("parse failed")?;
    if r.status != BlockLoadStatus::Error {
        return Err("expected Error status".into());
    }
    Ok(())
}

#[test]
fn block_load_rejects_too_short() -> Result<(), String> {
    if parse_block_load(&[0x12, 0, 1]).is_some() {
        return Err("should reject 3-byte buffer".into());
    }
    if parse_block_load(&[]).is_some() {
        return Err("should reject empty buffer".into());
    }
    Ok(())
}

#[test]
fn block_load_rejects_wrong_report_id() -> Result<(), String> {
    let buf = [0x13, 0, 1, 0, 0];
    if parse_block_load(&buf).is_some() {
        return Err("should reject wrong report ID".into());
    }
    Ok(())
}

#[test]
fn block_load_rejects_invalid_status() -> Result<(), String> {
    for status in [0u8, 4, 255] {
        let buf = [0x12, 0, status, 0, 0];
        if parse_block_load(&buf).is_some() {
            return Err(format!("should reject status {status}"));
        }
    }
    Ok(())
}

#[test]
fn block_load_ram_pool_le16() -> Result<(), String> {
    let buf = [0x12, 1, 1, 0xCD, 0xAB]; // ram = 0xABCD
    let r = parse_block_load(&buf).ok_or("parse failed")?;
    if r.ram_pool_available != 0xABCD {
        return Err(format!("expected 0xABCD, got {:#x}", r.ram_pool_available));
    }
    Ok(())
}

// ── Encoder allocation-free: all encoders return fixed arrays ────────────────

#[test]
fn all_encoders_return_fixed_size_arrays() -> Result<(), String> {
    // This test verifies the allocation-free property by checking that all
    // encoder return types are fixed-size arrays (compile-time guarantee).
    let a = encode_set_effect(1, EffectType::Constant, 0, 0, 0);
    let b = encode_set_envelope(1, 0, 0, 0, 0);
    let c = encode_set_condition(1, 0, 0, 0, 0, 0, 0, 0);
    let d = encode_set_periodic(1, 0, 0, 0, 0);
    let e = encode_set_constant_force(1, 0);
    let f = encode_set_ramp_force(1, 0, 0);
    let g = encode_effect_operation(1, EffectOp::Start, 0);
    let h = encode_block_free(1);
    let i = encode_device_control(0);
    let j = encode_device_gain(0);
    let k = encode_create_new_effect(EffectType::Constant);

    if a.len() != SET_EFFECT_LEN {
        return Err("set_effect".into());
    }
    if b.len() != SET_ENVELOPE_LEN {
        return Err("set_envelope".into());
    }
    if c.len() != SET_CONDITION_LEN {
        return Err("set_condition".into());
    }
    if d.len() != SET_PERIODIC_LEN {
        return Err("set_periodic".into());
    }
    if e.len() != SET_CONSTANT_FORCE_LEN {
        return Err("set_constant_force".into());
    }
    if f.len() != SET_RAMP_FORCE_LEN {
        return Err("set_ramp_force".into());
    }
    if g.len() != EFFECT_OPERATION_LEN {
        return Err("effect_operation".into());
    }
    if h.len() != BLOCK_FREE_LEN {
        return Err("block_free".into());
    }
    if i.len() != DEVICE_CONTROL_LEN {
        return Err("device_control".into());
    }
    if j.len() != DEVICE_GAIN_LEN {
        return Err("device_gain".into());
    }
    if k.len() != CREATE_NEW_EFFECT_LEN {
        return Err("create_new_effect".into());
    }
    Ok(())
}

// ── PIDFF protocol: first byte is always the correct report ID ───────────────

#[test]
fn all_encoders_first_byte_is_report_id() -> Result<(), String> {
    let checks: &[(u8, &str)] = &[
        (
            encode_set_effect(1, EffectType::Constant, 0, 0, 0)[0],
            "set_effect",
        ),
        (encode_set_envelope(1, 0, 0, 0, 0)[0], "set_envelope"),
        (
            encode_set_condition(1, 0, 0, 0, 0, 0, 0, 0)[0],
            "set_condition",
        ),
        (encode_set_periodic(1, 0, 0, 0, 0)[0], "set_periodic"),
        (encode_set_constant_force(1, 0)[0], "set_constant_force"),
        (encode_set_ramp_force(1, 0, 0)[0], "set_ramp_force"),
        (
            encode_effect_operation(1, EffectOp::Start, 0)[0],
            "effect_operation",
        ),
        (encode_block_free(1)[0], "block_free"),
        (encode_device_control(0)[0], "device_control"),
        (encode_device_gain(0)[0], "device_gain"),
        (
            encode_create_new_effect(EffectType::Constant)[0],
            "create_new_effect",
        ),
    ];
    let expected_ids = [
        report_ids::SET_EFFECT,
        report_ids::SET_ENVELOPE,
        report_ids::SET_CONDITION,
        report_ids::SET_PERIODIC,
        report_ids::SET_CONSTANT_FORCE,
        report_ids::SET_RAMP_FORCE,
        report_ids::EFFECT_OPERATION,
        report_ids::BLOCK_FREE,
        report_ids::DEVICE_CONTROL,
        report_ids::DEVICE_GAIN,
        report_ids::CREATE_NEW_EFFECT,
    ];
    for (i, &(actual, name)) in checks.iter().enumerate() {
        if actual != expected_ids[i] {
            return Err(format!(
                "{name}: expected {:#x}, got {actual:#x}",
                expected_ids[i]
            ));
        }
    }
    Ok(())
}

// ── Proptest fuzzing ─────────────────────────────────────────────────────────

mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_constant_force_magnitude_roundtrip(mag in i16::MIN..=i16::MAX) {
            let buf = encode_set_constant_force(1, mag);
            prop_assert_eq!(buf[0], report_ids::SET_CONSTANT_FORCE);
            prop_assert_eq!(buf[1], 1);
            prop_assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), mag);
        }

        #[test]
        fn prop_ramp_force_roundtrip(start in i16::MIN..=i16::MAX, end in i16::MIN..=i16::MAX) {
            let buf = encode_set_ramp_force(1, start, end);
            prop_assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), start);
            prop_assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), end);
        }

        #[test]
        fn prop_periodic_magnitude_roundtrip(mag in 0u16..=u16::MAX) {
            let buf = encode_set_periodic(1, mag, 0, 0, 100);
            prop_assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), mag);
        }

        #[test]
        fn prop_periodic_offset_roundtrip(offset in i16::MIN..=i16::MAX) {
            let buf = encode_set_periodic(1, 0, offset, 0, 100);
            prop_assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), offset);
        }

        #[test]
        fn prop_periodic_phase_roundtrip(phase in 0u16..=u16::MAX) {
            let buf = encode_set_periodic(1, 0, 0, phase, 100);
            prop_assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), phase);
        }

        #[test]
        fn prop_device_gain_bounded(gain in 0u16..=u16::MAX) {
            let buf = encode_device_gain(gain);
            let encoded = u16::from_le_bytes([buf[2], buf[3]]);
            prop_assert!(encoded <= 10000);
            if gain <= 10000 {
                prop_assert_eq!(encoded, gain);
            } else {
                prop_assert_eq!(encoded, 10000);
            }
        }

        #[test]
        fn prop_condition_center_roundtrip(center in i16::MIN..=i16::MAX) {
            let buf = encode_set_condition(1, 0, center, 0, 0, 0, 0, 0);
            prop_assert_eq!(i16::from_le_bytes([buf[3], buf[4]]), center);
        }

        #[test]
        fn prop_condition_coefficients_roundtrip(
            pos in i16::MIN..=i16::MAX,
            neg in i16::MIN..=i16::MAX,
        ) {
            let buf = encode_set_condition(1, 0, 0, pos, neg, 0, 0, 0);
            prop_assert_eq!(i16::from_le_bytes([buf[5], buf[6]]), pos);
            prop_assert_eq!(i16::from_le_bytes([buf[7], buf[8]]), neg);
        }

        #[test]
        fn prop_condition_saturation_roundtrip(
            pos_sat in 0u16..=u16::MAX,
            neg_sat in 0u16..=u16::MAX,
        ) {
            let buf = encode_set_condition(1, 0, 0, 0, 0, pos_sat, neg_sat, 0);
            prop_assert_eq!(u16::from_le_bytes([buf[9], buf[10]]), pos_sat);
            prop_assert_eq!(u16::from_le_bytes([buf[11], buf[12]]), neg_sat);
        }

        #[test]
        fn prop_envelope_all_params(
            attack in 0u16..=u16::MAX,
            fade in 0u16..=u16::MAX,
            at_ms in 0u16..=u16::MAX,
            ft_ms in 0u16..=u16::MAX,
        ) {
            let buf = encode_set_envelope(1, attack, fade, at_ms, ft_ms);
            prop_assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), attack);
            prop_assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), fade);
            prop_assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), at_ms);
            prop_assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), ft_ms);
        }

        #[test]
        fn prop_effect_operation_block_index(block in 0u8..=255u8) {
            let buf = encode_effect_operation(block, EffectOp::Start, 0);
            prop_assert_eq!(buf[0], report_ids::EFFECT_OPERATION);
            prop_assert_eq!(buf[1], block);
        }

        #[test]
        fn prop_block_free_index(block in 0u8..=255u8) {
            let buf = encode_block_free(block);
            prop_assert_eq!(buf[0], report_ids::BLOCK_FREE);
            prop_assert_eq!(buf[1], block);
        }

        #[test]
        fn prop_set_effect_gain_preserved(gain in 0u8..=255u8) {
            let buf = encode_set_effect(1, EffectType::Constant, 1000, gain, 0);
            prop_assert_eq!(buf[9], gain);
        }

        #[test]
        fn prop_set_effect_duration_preserved(dur in 0u16..=u16::MAX) {
            let buf = encode_set_effect(1, EffectType::Constant, dur, 128, 0);
            prop_assert_eq!(u16::from_le_bytes([buf[3], buf[4]]), dur);
        }

        #[test]
        fn prop_set_effect_direction_preserved(dir in 0u16..=u16::MAX) {
            let buf = encode_set_effect(1, EffectType::Sine, 1000, 128, dir);
            prop_assert_eq!(u16::from_le_bytes([buf[11], buf[12]]), dir);
        }

        #[test]
        fn prop_block_load_valid_status_roundtrip(
            block_idx in 0u8..=255u8,
            status_byte in 1u8..=3u8,
            ram_lo in 0u8..=255u8,
            ram_hi in 0u8..=255u8,
        ) {
            let buf = [report_ids::BLOCK_LOAD, block_idx, status_byte, ram_lo, ram_hi];
            let result = parse_block_load(&buf);
            prop_assert!(result.is_some());
            if let Some(r) = result {
                prop_assert_eq!(r.block_index, block_idx);
                prop_assert_eq!(r.ram_pool_available, u16::from_le_bytes([ram_lo, ram_hi]));
            }
        }

        #[test]
        fn prop_block_load_invalid_status_rejected(status in 4u8..=255u8) {
            let buf = [report_ids::BLOCK_LOAD, 0, status, 0, 0];
            prop_assert!(parse_block_load(&buf).is_none());
        }
    }
}
