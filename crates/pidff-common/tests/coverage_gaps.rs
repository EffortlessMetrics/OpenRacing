//! Additional coverage tests for `openracing-pidff-common`.
//!
//! These tests exercise paths that the original suite leaves uncovered:
//! padding-byte invariants on the Set Effect report, derived trait coverage
//! for the public `BlockLoadReport` / `EffectType` / `EffectOp` /
//! `BlockLoadStatus` types, block-load parser boundary cases, and a few
//! property-style invariants for padding zeros and unused report IDs.

use openracing_pidff_common::*;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Set Effect padding and direction byte layout
// ---------------------------------------------------------------------------

#[test]
fn set_effect_zeroes_trigger_repeat_and_sample_period_bytes() {
    let buf = encode_set_effect(1, EffectType::Constant, 100, 200, 0);
    // Bytes 5-8 are "trigger repeat interval / sample period" and must be 0.
    assert_eq!(buf[5], 0);
    assert_eq!(buf[6], 0);
    assert_eq!(buf[7], 0);
    assert_eq!(buf[8], 0);
}

#[test]
fn set_effect_accepts_block_index_extremes() {
    for &block in &[0u8, 1, 127, 200, 255] {
        let buf = encode_set_effect(block, EffectType::Sine, 1, 1, 1);
        assert_eq!(buf[1], block, "block index at byte 1");
        assert_eq!(buf[2], EffectType::Sine as u8);
    }
}

#[test]
fn set_effect_infinite_duration_emits_two_ff_bytes() {
    let buf = encode_set_effect(1, EffectType::Constant, DURATION_INFINITE, 0, 0);
    // The literal bytes at positions 3 and 4 must both be 0xFF — not just the
    // u16 view — so a future change to the encoder cannot silently shift them.
    assert_eq!(buf[3], 0xFF);
    assert_eq!(buf[4], 0xFF);
}

#[test]
fn set_effect_no_trigger_button_marker_is_byte_10() {
    let buf = encode_set_effect(1, EffectType::Sine, 1, 1, 0);
    assert_eq!(buf[10], 0xFF, "byte 10 carries the 'no trigger button' sentinel");
    assert_eq!(buf[13], 0, "trailing direction-high byte stays zero");
}

// ---------------------------------------------------------------------------
// Device Gain block_index byte must remain reserved (zero)
// ---------------------------------------------------------------------------

#[test]
fn device_gain_byte_one_is_reserved_zero() {
    // The legacy PIDFF Device Gain layout reserves buf[1] for the device-gain
    // block index (always 0 in practice); confirm we never accidentally clobber
    // it.
    let buf = encode_device_gain(5000);
    assert_eq!(buf[1], 0, "byte 1 must be a reserved zero");
    assert_eq!(buf[0], report_ids::DEVICE_GAIN);
}

#[test]
fn device_gain_clamps_at_exact_max_boundary() {
    let buf_at = encode_device_gain(10000);
    assert_eq!(u16::from_le_bytes([buf_at[2], buf_at[3]]), 10000);
    let buf_over = encode_device_gain(10001);
    assert_eq!(u16::from_le_bytes([buf_over[2], buf_over[3]]), 10000);
}

// ---------------------------------------------------------------------------
// Device Control report-id byte explicit assertion
// ---------------------------------------------------------------------------

#[test]
fn device_control_first_byte_always_carries_report_id() {
    let flags = [
        device_control::ENABLE_ACTUATORS,
        device_control::DISABLE_ACTUATORS,
        device_control::STOP_ALL_EFFECTS,
        device_control::DEVICE_RESET,
        device_control::DEVICE_PAUSE,
        device_control::DEVICE_CONTINUE,
        device_control::ENABLE_ACTUATORS
            | device_control::STOP_ALL_EFFECTS
            | device_control::DEVICE_CONTINUE,
    ];
    for &flag in &flags {
        let buf = encode_device_control(flag);
        assert_eq!(buf[0], report_ids::DEVICE_CONTROL);
        assert_eq!(buf[1], flag);
    }
}

// ---------------------------------------------------------------------------
// Derived-trait coverage for public types
// ---------------------------------------------------------------------------

#[test]
fn block_load_report_clone_and_equality() {
    let a = BlockLoadReport {
        block_index: 7,
        status: BlockLoadStatus::Success,
        ram_pool_available: 4096,
    };
    let b = a;
    assert_eq!(a, b);
    let c = BlockLoadReport {
        block_index: 8,
        ..a
    };
    assert_ne!(a, c);
}

#[test]
fn block_load_report_debug_format_is_descriptive() {
    let r = BlockLoadReport {
        block_index: 1,
        status: BlockLoadStatus::Full,
        ram_pool_available: 0,
    };
    let s = format!("{:?}", r);
    assert!(s.contains("BlockLoadReport"));
    assert!(s.contains("Full"));
}

#[test]
fn effect_type_debug_clone_and_eq() {
    let v = EffectType::Sine;
    let cloned = v;
    assert_eq!(v, cloned);
    assert_ne!(v, EffectType::Square);
    assert!(format!("{:?}", v).contains("Sine"));
}

#[test]
fn effect_op_debug_clone_and_eq() {
    let v = EffectOp::StartSolo;
    let cloned = v;
    assert_eq!(v, cloned);
    assert_ne!(v, EffectOp::Stop);
    assert!(format!("{:?}", v).contains("StartSolo"));
}

#[test]
fn block_load_status_debug_clone_and_eq() {
    let v = BlockLoadStatus::Error;
    let cloned = v;
    assert_eq!(v, cloned);
    assert_ne!(v, BlockLoadStatus::Success);
    assert!(format!("{:?}", v).contains("Error"));
}

// ---------------------------------------------------------------------------
// parse_block_load boundary conditions
// ---------------------------------------------------------------------------

#[test]
fn parse_block_load_rejects_four_byte_buffer() {
    // Below the 5-byte minimum even with a valid prefix.
    let buf = [0x12u8, 0, 1, 0];
    assert!(parse_block_load(&buf).is_none());
}

#[test]
fn parse_block_load_accepts_oversized_buffer() {
    // Extra trailing bytes are tolerated; only the first 5 matter.
    let buf = [0x12u8, 9, 3, 0xCD, 0xAB, 0xFF, 0xFF, 0xFF];
    let r = parse_block_load(&buf).expect("oversized buffer parses");
    assert_eq!(r.block_index, 9);
    assert_eq!(r.status, BlockLoadStatus::Error);
    assert_eq!(r.ram_pool_available, 0xABCD);
}

#[test]
fn parse_block_load_rejects_status_above_three() {
    for bad in 4u8..=255u8 {
        let buf = [0x12, 0, bad, 0, 0];
        assert!(
            parse_block_load(&buf).is_none(),
            "status byte {bad} must be rejected"
        );
    }
}

#[test]
fn parse_block_load_empty_buffer_returns_none() {
    assert!(parse_block_load(&[]).is_none());
}

// ---------------------------------------------------------------------------
// Report-size constants must match layout helpers
// ---------------------------------------------------------------------------

#[test]
fn report_size_constants_match_encoder_output() {
    assert_eq!(encode_set_effect(0, EffectType::Constant, 0, 0, 0).len(), SET_EFFECT_LEN);
    assert_eq!(encode_set_envelope(0, 0, 0, 0, 0).len(), SET_ENVELOPE_LEN);
    assert_eq!(encode_set_condition(0, 0, 0, 0, 0, 0, 0, 0).len(), SET_CONDITION_LEN);
    assert_eq!(encode_set_periodic(0, 0, 0, 0, 0).len(), SET_PERIODIC_LEN);
    assert_eq!(encode_set_constant_force(0, 0).len(), SET_CONSTANT_FORCE_LEN);
    assert_eq!(encode_set_ramp_force(0, 0, 0).len(), SET_RAMP_FORCE_LEN);
    assert_eq!(encode_effect_operation(0, EffectOp::Start, 0).len(), EFFECT_OPERATION_LEN);
    assert_eq!(encode_device_control(0).len(), DEVICE_CONTROL_LEN);
    assert_eq!(encode_device_gain(0).len(), DEVICE_GAIN_LEN);
    assert_eq!(encode_block_free(0).len(), BLOCK_FREE_LEN);
    assert_eq!(encode_create_new_effect(EffectType::Sine).len(), CREATE_NEW_EFFECT_LEN);
}

#[test]
fn pid_pool_report_id_matches_spec() {
    // Only constant in `report_ids` without an encoder; pin its spec value.
    assert_eq!(report_ids::PID_POOL, 0x13);
}

// ---------------------------------------------------------------------------
// Property tests for padding invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    #[test]
    fn prop_set_effect_padding_bytes_always_zero(
        block in 0u8..=255u8,
        dur in 0u16..=u16::MAX,
        gain in 0u8..=255u8,
        dir in 0u16..=u16::MAX,
    ) {
        let buf = encode_set_effect(block, EffectType::Constant, dur, gain, dir);
        prop_assert_eq!(buf[5], 0);
        prop_assert_eq!(buf[6], 0);
        prop_assert_eq!(buf[7], 0);
        prop_assert_eq!(buf[8], 0);
        prop_assert_eq!(buf[10], 0xFF);
        prop_assert_eq!(buf[13], 0);
    }

    #[test]
    fn prop_parse_block_load_rejects_wrong_first_byte(
        id in 0u8..=255u8,
        block in 0u8..=255u8,
        status in 1u8..=3u8,
        lo in 0u8..=255u8,
        hi in 0u8..=255u8,
    ) {
        let buf = [id, block, status, lo, hi];
        let parsed = parse_block_load(&buf);
        if id == report_ids::BLOCK_LOAD {
            prop_assert!(parsed.is_some());
        } else {
            prop_assert!(parsed.is_none());
        }
    }

    #[test]
    fn prop_device_gain_byte_one_always_zero(gain in 0u16..=u16::MAX) {
        let buf = encode_device_gain(gain);
        prop_assert_eq!(buf[1], 0);
    }
}
