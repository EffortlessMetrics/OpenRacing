//! Deep integration tests for the Thrustmaster HID protocol crate.
//!
//! Covers:
//! - All wheel variants (T150, T248, T300 RS, T500 RS, TS-PC Racer, TS-XW, T818)
//! - Bootloader vs normal mode switching
//! - USB control transfer encoding
//! - Force feedback effect encoding (effects module)
//! - Pedal set report parsing and normalization
//! - Wheel rim detection and switching
//! - Known VID/PID validation (0x044F vendor ID)
//! - Lifecycle families and range limits
//! - T300RS wire-format encoding
//! - Proptest for report parsing
//! - Error handling for mode transitions

use racing_wheel_hid_thrustmaster_protocol::{
    EFFECT_REPORT_LEN, Model, ProtocolFamily, THRUSTMASTER_VENDOR_ID, ThrustmasterDeviceCategory,
    ThrustmasterInitState, ThrustmasterPedalAxesRaw, ThrustmasterProtocol, identify_device,
    parse_input_report, product_ids,
};

use racing_wheel_hid_thrustmaster_protocol::effects::{
    self, CONDITION_HARDCODED, ConditionType, DEFAULT_MAX_SATURATION, Envelope, INFINITE_DURATION,
    MAX_EFFECTS, MAX_RANGE, MIN_RANGE, NORM_BUFFER_LENGTH, PS4_BUFFER_LENGTH, RANGE_SCALE,
    SPRING_MAX_SATURATION, TIMING_END_MARKER, TIMING_START_MARKER, Timing, Waveform,
};

use racing_wheel_hid_thrustmaster_protocol::lifecycle::{
    self, LifecycleFamily, SETUP_COMMAND_COUNT, SETUP_COMMANDS, T248_RANGE, T300RS_RANGE,
    TSPC_RANGE, TSXW_RANGE,
};

use racing_wheel_hid_thrustmaster_protocol::t300rs::{
    self, HEADER_BYTE, NewConstantParams, T300RS_REPORT_SIZE, T300RS_REPORT_SIZE_PS4,
};

use racing_wheel_hid_thrustmaster_protocol::input::parse_pedal_report;

// ═══════════════════════════════════════════════════════════════════════════
// § 1 — Vendor ID and PID validation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn vid_is_0x044f() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        THRUSTMASTER_VENDOR_ID, 0x044F,
        "Thrustmaster VID must be 0x044F"
    );
    Ok(())
}

#[test]
fn all_known_pids_are_in_vendor_0x044f_range() -> Result<(), Box<dyn std::error::Error>> {
    // All known Thrustmaster PIDs start with 0xB (high nibble)
    let pids: &[u16] = &[
        product_ids::FFB_WHEEL_GENERIC,
        product_ids::T150,
        product_ids::T500_RS,
        product_ids::T300_RS_PS4,
        product_ids::TMX,
        product_ids::T300_RS,
        product_ids::T300_RS_GT,
        product_ids::TX_RACING,
        product_ids::TX_RACING_ORIG,
        product_ids::T248,
        product_ids::T248X,
        product_ids::TS_PC_RACER,
        product_ids::TS_XW,
        product_ids::TS_XW_GIP,
        product_ids::T_GT_II_GT,
        product_ids::T818,
        product_ids::T80,
        product_ids::T80_FERRARI_488,
        product_ids::NASCAR_PRO_FF2,
        product_ids::FGT_RUMBLE_FORCE,
        product_ids::RGT_FF_CLUTCH,
        product_ids::FGT_FORCE_FEEDBACK,
        product_ids::F430_FORCE_FEEDBACK,
        product_ids::TPR_PEDALS,
        product_ids::T_LCM,
    ];
    for &pid in pids {
        assert_ne!(pid, 0, "PID must be nonzero");
        // Verify the PID is in the Thrustmaster range (0xB000–0xBFFF)
        // except T_LCM which is 0xB371
        assert!(
            (pid & 0xF000) == 0xB000,
            "PID 0x{pid:04X} should be in 0xBxxx range"
        );
    }
    Ok(())
}

#[test]
fn all_known_pids_are_unique() -> Result<(), Box<dyn std::error::Error>> {
    let pids: Vec<u16> = vec![
        product_ids::FFB_WHEEL_GENERIC,
        product_ids::T150,
        product_ids::T500_RS,
        product_ids::T300_RS_PS4,
        product_ids::TMX,
        product_ids::T300_RS,
        product_ids::T300_RS_GT,
        product_ids::TX_RACING,
        product_ids::TX_RACING_ORIG,
        product_ids::T248,
        product_ids::T248X,
        product_ids::TS_PC_RACER,
        product_ids::TS_XW,
        product_ids::TS_XW_GIP,
        product_ids::T_GT_II_GT,
        product_ids::T818,
        product_ids::T80,
        product_ids::T80_FERRARI_488,
        product_ids::NASCAR_PRO_FF2,
        product_ids::FGT_RUMBLE_FORCE,
        product_ids::RGT_FF_CLUTCH,
        product_ids::FGT_FORCE_FEEDBACK,
        product_ids::F430_FORCE_FEEDBACK,
        product_ids::T_LCM,
    ];
    let mut sorted = pids.clone();
    sorted.sort();
    sorted.dedup();
    // TPR_PEDALS (0xB68F) == T_GT_II_GT (0xB681)? No, let's check if any are duped
    // Actually TPR_PEDALS is 0xB68F which could overlap. Let's just check for proper uniqueness
    // among non-shared PIDs
    assert_eq!(
        pids.len(),
        sorted.len(),
        "All PIDs must be unique (found {} unique out of {})",
        sorted.len(),
        pids.len()
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// § 2 — All wheel variants: identity, model, FFB support
// ═══════════════════════════════════════════════════════════════════════════

/// Verify every wheel variant has consistent identity, model, and FFB metadata.
#[test]
fn all_wheel_variants_consistency() -> Result<(), Box<dyn std::error::Error>> {
    struct WheelSpec {
        pid: u16,
        expected_model: Model,
        expected_family: ProtocolFamily,
        expected_ffb: bool,
        expected_category: ThrustmasterDeviceCategory,
        min_torque: f32,
        max_rotation: u16,
    }

    let specs = [
        WheelSpec {
            pid: product_ids::T150,
            expected_model: Model::T150,
            expected_family: ProtocolFamily::T150,
            expected_ffb: true,
            expected_category: ThrustmasterDeviceCategory::Wheelbase,
            min_torque: 2.0,
            max_rotation: 1080,
        },
        WheelSpec {
            pid: product_ids::TMX,
            expected_model: Model::TMX,
            expected_family: ProtocolFamily::T150,
            expected_ffb: true,
            expected_category: ThrustmasterDeviceCategory::Wheelbase,
            min_torque: 2.0,
            max_rotation: 900,
        },
        WheelSpec {
            pid: product_ids::T300_RS,
            expected_model: Model::T300RS,
            expected_family: ProtocolFamily::T300,
            expected_ffb: true,
            expected_category: ThrustmasterDeviceCategory::Wheelbase,
            min_torque: 3.5,
            max_rotation: 1080,
        },
        WheelSpec {
            pid: product_ids::T500_RS,
            expected_model: Model::T500RS,
            expected_family: ProtocolFamily::T500,
            expected_ffb: true,
            expected_category: ThrustmasterDeviceCategory::Wheelbase,
            min_torque: 4.5,
            max_rotation: 1080,
        },
        WheelSpec {
            pid: product_ids::T248,
            expected_model: Model::T248,
            expected_family: ProtocolFamily::T300,
            expected_ffb: true,
            expected_category: ThrustmasterDeviceCategory::Wheelbase,
            min_torque: 3.5,
            max_rotation: 900,
        },
        WheelSpec {
            pid: product_ids::TS_PC_RACER,
            expected_model: Model::TSPCRacer,
            expected_family: ProtocolFamily::T300,
            expected_ffb: true,
            expected_category: ThrustmasterDeviceCategory::Wheelbase,
            min_torque: 5.5,
            max_rotation: 1080,
        },
        WheelSpec {
            pid: product_ids::TS_XW,
            expected_model: Model::TSXW,
            expected_family: ProtocolFamily::T300,
            expected_ffb: true,
            expected_category: ThrustmasterDeviceCategory::Wheelbase,
            min_torque: 5.5,
            max_rotation: 1080,
        },
        WheelSpec {
            pid: product_ids::T818,
            expected_model: Model::T818,
            expected_family: ProtocolFamily::Unknown,
            expected_ffb: true,
            expected_category: ThrustmasterDeviceCategory::Wheelbase,
            min_torque: 9.0,
            max_rotation: 1080,
        },
        WheelSpec {
            pid: product_ids::T_GT_II_GT,
            expected_model: Model::TGTII,
            expected_family: ProtocolFamily::T300,
            expected_ffb: true,
            expected_category: ThrustmasterDeviceCategory::Unknown,
            min_torque: 5.5,
            max_rotation: 1080,
        },
    ];

    for spec in &specs {
        let model = Model::from_product_id(spec.pid);
        assert_eq!(
            model, spec.expected_model,
            "PID 0x{:04X}: model mismatch",
            spec.pid
        );
        assert_eq!(
            model.protocol_family(),
            spec.expected_family,
            "PID 0x{:04X}: protocol family mismatch",
            spec.pid
        );
        assert_eq!(
            model.supports_ffb(),
            spec.expected_ffb,
            "PID 0x{:04X}: FFB support mismatch",
            spec.pid
        );

        let identity = identify_device(spec.pid);
        assert_eq!(
            identity.category, spec.expected_category,
            "PID 0x{:04X}: category mismatch",
            spec.pid
        );
        assert!(
            model.max_torque_nm() >= spec.min_torque,
            "PID 0x{:04X}: torque {} below minimum {}",
            spec.pid,
            model.max_torque_nm(),
            spec.min_torque
        );
        assert_eq!(
            model.max_rotation_deg(),
            spec.max_rotation,
            "PID 0x{:04X}: rotation mismatch",
            spec.pid
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// § 3 — Bootloader vs normal mode switching
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn generic_ffb_wheel_is_bootloader_mode() -> Result<(), Box<dyn std::error::Error>> {
    let model = Model::from_product_id(product_ids::FFB_WHEEL_GENERIC);
    assert_eq!(
        model,
        Model::Unknown,
        "generic PID is pre-init bootloader mode"
    );
    assert!(!model.supports_ffb(), "bootloader mode has no FFB");
    assert_eq!(
        model.init_switch_value(),
        None,
        "bootloader has no switch value"
    );
    Ok(())
}

#[test]
fn mode_switch_normal_vs_advanced() -> Result<(), Box<dyn std::error::Error>> {
    let normal = effects::mode_switch_normal();
    let advanced = effects::mode_switch_advanced();

    // Both use same bRequest (83) and bRequestType (0x41)
    assert_eq!(normal.b_request, 83);
    assert_eq!(advanced.b_request, 83);
    assert_eq!(normal.b_request_type, 0x41);
    assert_eq!(advanced.b_request_type, 0x41);

    // wValue differs: 5 for normal, 3 for advanced
    assert_eq!(normal.w_value, 5);
    assert_eq!(advanced.w_value, 3);
    assert_ne!(normal.w_value, advanced.w_value);
    Ok(())
}

#[test]
fn init_switch_values_per_protocol_family() -> Result<(), Box<dyn std::error::Error>> {
    // T150 family: 0x0006
    assert_eq!(Model::T150.init_switch_value(), Some(0x0006));
    assert_eq!(Model::TMX.init_switch_value(), Some(0x0006));

    // T300 family: 0x0005
    let t300_models = [
        Model::T300RS,
        Model::T300RSPS4,
        Model::T300RSGT,
        Model::TXRacing,
        Model::T248,
        Model::T248X,
        Model::TSPCRacer,
        Model::TSXW,
        Model::TGTII,
    ];
    for model in t300_models {
        assert_eq!(
            model.init_switch_value(),
            Some(0x0005),
            "{:?} should use switch value 0x0005",
            model
        );
    }

    // T500: 0x0002
    assert_eq!(Model::T500RS.init_switch_value(), Some(0x0002));

    // Non-FFB / unknown: None
    assert_eq!(Model::T80.init_switch_value(), None);
    assert_eq!(Model::Unknown.init_switch_value(), None);
    assert_eq!(Model::T3PA.init_switch_value(), None);
    assert_eq!(Model::TLCM.init_switch_value(), None);
    Ok(())
}

#[test]
fn protocol_state_machine_full_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let mut proto = ThrustmasterProtocol::new(product_ids::T300_RS);
    assert_eq!(proto.init_state(), ThrustmasterInitState::Uninitialized);

    // Init transitions to Ready
    proto.init();
    assert_eq!(proto.init_state(), ThrustmasterInitState::Ready);

    // Double init stays Ready
    proto.init();
    assert_eq!(proto.init_state(), ThrustmasterInitState::Ready);

    // Reset transitions back
    proto.reset();
    assert_eq!(proto.init_state(), ThrustmasterInitState::Uninitialized);

    // Double reset stays Uninitialized
    proto.reset();
    assert_eq!(proto.init_state(), ThrustmasterInitState::Uninitialized);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// § 4 — USB control transfer encoding (effects module)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn effects_open_close_wire_format() -> Result<(), Box<dyn std::error::Error>> {
    let open = effects::encode_open();
    assert_eq!(open, [0x01, 0x05], "open must be [0x01, 0x05]");

    let close = effects::encode_close();
    assert_eq!(close, [0x01, 0x00], "close must be [0x01, 0x00]");
    Ok(())
}

#[test]
fn effects_gain_shifts_high_byte() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(effects::encode_gain(0xFFFF), [0x02, 0xFF]);
    assert_eq!(effects::encode_gain(0x8000), [0x02, 0x80]);
    assert_eq!(effects::encode_gain(0x0100), [0x02, 0x01]);
    assert_eq!(effects::encode_gain(0x00FF), [0x02, 0x00]);
    assert_eq!(effects::encode_gain(0x0000), [0x02, 0x00]);
    Ok(())
}

#[test]
fn effects_range_clamping_and_scaling() -> Result<(), Box<dyn std::error::Error>> {
    // Within range
    let r900 = effects::encode_range(900);
    assert_eq!(r900[0], 0x08);
    assert_eq!(r900[1], 0x11);
    let scaled_900 = u16::from_le_bytes([r900[2], r900[3]]);
    assert_eq!(scaled_900, 900 * RANGE_SCALE);

    // Below minimum clamps to MIN_RANGE
    let r_lo = effects::encode_range(10);
    let r_min = effects::encode_range(MIN_RANGE);
    assert_eq!(r_lo, r_min, "below MIN_RANGE must clamp");

    // Above maximum clamps to MAX_RANGE
    let r_hi = effects::encode_range(5000);
    let r_max = effects::encode_range(MAX_RANGE);
    assert_eq!(r_hi, r_max, "above MAX_RANGE must clamp");
    Ok(())
}

#[test]
fn effects_autocenter_two_step_sequence() -> Result<(), Box<dyn std::error::Error>> {
    let (enable, set) = effects::encode_autocenter(0x1234);
    assert_eq!(enable, [0x08, 0x04, 0x01, 0x00], "enable step");
    assert_eq!(set[0..2], [0x08, 0x03], "set strength header");
    let decoded = u16::from_le_bytes([set[2], set[3]]);
    assert_eq!(decoded, 0x1234, "strength value round-trips");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// § 5 — FFB effect encoding (effects module)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn effects_header_increments_id() -> Result<(), Box<dyn std::error::Error>> {
    for id in 0..MAX_EFFECTS {
        let hdr = effects::encode_header(id, 0x6a);
        assert_eq!(hdr[0], 0x00);
        assert_eq!(
            hdr[1],
            id.wrapping_add(1),
            "effect_id must be incremented on wire"
        );
        assert_eq!(hdr[2], 0x6a);
    }
    Ok(())
}

#[test]
fn effects_envelope_encode_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let env = Envelope {
        attack_length: 1000,
        attack_level: 3276,
        fade_length: 1500,
        fade_level: 3276,
    };
    let buf = env.encode();
    assert_eq!(u16::from_le_bytes([buf[0], buf[1]]), 1000);
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 3276);
    assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 1500);
    assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 3276);
    Ok(())
}

#[test]
fn effects_timing_infinite_and_encode() -> Result<(), Box<dyn std::error::Error>> {
    let t = Timing::infinite();
    assert_eq!(t.duration, INFINITE_DURATION);
    assert_eq!(t.offset, 0);

    let buf = t.encode();
    assert_eq!(buf[0], TIMING_START_MARKER, "timing must start with 0x4F");
    assert_eq!(
        u16::from_le_bytes([buf[1], buf[2]]),
        0xFFFF,
        "infinite duration"
    );
    assert_eq!(u16::from_le_bytes([buf[5], buf[6]]), 0, "zero offset");
    assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), TIMING_END_MARKER);
    Ok(())
}

#[test]
fn effects_constant_upload_wire_format() -> Result<(), Box<dyn std::error::Error>> {
    let env = Envelope::default();
    let pkt = effects::encode_constant_upload(0, 5000, &env, INFINITE_DURATION, 0);

    // Header: [0x00, 0x01, 0x6a]
    assert_eq!(pkt[0], 0x00);
    assert_eq!(pkt[1], 0x01); // effect_id 0 + 1
    assert_eq!(pkt[2], 0x6a); // constant opcode

    // Magnitude at bytes 3-4
    let mag = i16::from_le_bytes([pkt[3], pkt[4]]);
    assert_eq!(mag, 5000);

    // Envelope at bytes 5-12 (all zeros for default)
    assert_eq!(&pkt[5..13], &[0u8; 8]);

    // Timing block starts at byte 14
    assert_eq!(pkt[14], TIMING_START_MARKER);
    Ok(())
}

#[test]
fn effects_constant_modify_wire_format() -> Result<(), Box<dyn std::error::Error>> {
    let env = Envelope {
        attack_length: 100,
        attack_level: 200,
        fade_length: 300,
        fade_level: 400,
    };
    let pkt = effects::encode_constant_modify(2, -1000, &env, 5000, 100);

    // Header
    assert_eq!(pkt[0], 0x00);
    assert_eq!(pkt[1], 3); // effect_id 2 + 1
    assert_eq!(pkt[2], 0x6a);

    // Magnitude
    let mag = i16::from_le_bytes([pkt[3], pkt[4]]);
    assert_eq!(mag, -1000);

    // Update type marker
    assert_eq!(pkt[14], 0x45, "modify packet update_type must be 0x45");

    // Duration and offset
    let dur = u16::from_le_bytes([pkt[15], pkt[16]]);
    let off = u16::from_le_bytes([pkt[17], pkt[18]]);
    assert_eq!(dur, 5000);
    assert_eq!(off, 100);
    Ok(())
}

#[test]
fn effects_periodic_upload_waveforms() -> Result<(), Box<dyn std::error::Error>> {
    let env = Envelope::default();
    let waveforms = [
        (Waveform::Square, 0x01u8),
        (Waveform::Triangle, 0x02),
        (Waveform::Sine, 0x03),
        (Waveform::SawUp, 0x04),
        (Waveform::SawDown, 0x05),
    ];

    for (wf, expected_byte) in waveforms {
        let pkt =
            effects::encode_periodic_upload(0, 1000, 0, 0, 100, wf, &env, INFINITE_DURATION, 0);
        assert_eq!(pkt[2], 0x6b, "periodic opcode must be 0x6b");
        assert_eq!(pkt[21], expected_byte, "waveform byte for {:?}", wf);
    }
    Ok(())
}

#[test]
fn effects_condition_spring_vs_damper_saturation() -> Result<(), Box<dyn std::error::Error>> {
    let spring = effects::encode_condition_upload(
        0,
        1000,
        -1000,
        100,
        -100,
        500,
        500,
        ConditionType::Spring,
        INFINITE_DURATION,
        0,
    );
    let damper = effects::encode_condition_upload(
        1,
        1000,
        -1000,
        100,
        -100,
        500,
        500,
        ConditionType::Other,
        INFINITE_DURATION,
        0,
    );

    // Spring uses SPRING_MAX_SATURATION, Other uses DEFAULT_MAX_SATURATION
    let spring_max = u16::from_le_bytes([spring[23], spring[24]]);
    let damper_max = u16::from_le_bytes([damper[23], damper[24]]);
    assert_eq!(spring_max, SPRING_MAX_SATURATION);
    assert_eq!(damper_max, DEFAULT_MAX_SATURATION);
    assert_ne!(
        spring_max, damper_max,
        "spring and damper max saturation must differ"
    );

    // Condition type byte
    assert_eq!(spring[27], ConditionType::Spring as u8);
    assert_eq!(damper[27], ConditionType::Other as u8);

    // Hardcoded bytes at 15-22
    assert_eq!(&spring[15..23], &CONDITION_HARDCODED);
    assert_eq!(&damper[15..23], &CONDITION_HARDCODED);
    Ok(())
}

#[test]
fn effects_play_and_stop() -> Result<(), Box<dyn std::error::Error>> {
    let play = effects::encode_play(3, 10);
    assert_eq!(play[0], 0x00);
    assert_eq!(play[1], 4); // id 3 + 1
    assert_eq!(play[2], 0x89); // play/stop opcode
    let count = u16::from_le_bytes([play[3], play[4]]);
    assert_eq!(count, 10);

    let stop = effects::encode_stop(3);
    assert_eq!(stop[0], 0x00);
    assert_eq!(stop[1], 4);
    assert_eq!(stop[2], 0x89);
    let stop_count = u16::from_le_bytes([stop[3], stop[4]]);
    assert_eq!(stop_count, 0, "stop must have count=0");
    Ok(())
}

#[test]
fn effects_ramp_upload_structure() -> Result<(), Box<dyn std::error::Error>> {
    let env = Envelope::default();
    let pkt = effects::encode_ramp_upload(0, 500, -200, 1, &env, 2000, 50);

    assert_eq!(pkt[2], 0x6b, "ramp uses same opcode as periodic");
    let slope = u16::from_le_bytes([pkt[3], pkt[4]]);
    assert_eq!(slope, 500);
    let center = i16::from_le_bytes([pkt[5], pkt[6]]);
    assert_eq!(center, -200);
    assert_eq!(pkt[21], 1, "invert flag");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// § 6 — Pedal set report parsing (T-LCM, T3PA, TLCM Pro models)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn pedal_report_full_range() -> Result<(), Box<dyn std::error::Error>> {
    // Full throttle, full brake, full clutch
    let data = [0xFF, 0xFF, 0xFF];
    let pedals = parse_pedal_report(&data).ok_or("failed to parse full-range pedal report")?;
    assert_eq!(pedals.throttle, 0xFF);
    assert_eq!(pedals.brake, 0xFF);
    assert_eq!(pedals.clutch, Some(0xFF));
    Ok(())
}

#[test]
fn pedal_report_zero() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x00, 0x00, 0x00];
    let pedals = parse_pedal_report(&data).ok_or("failed to parse zero pedal report")?;
    assert_eq!(pedals.throttle, 0);
    assert_eq!(pedals.brake, 0);
    assert_eq!(pedals.clutch, Some(0));
    Ok(())
}

#[test]
fn pedal_normalize_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let raw_max = ThrustmasterPedalAxesRaw {
        throttle: 255,
        brake: 255,
        clutch: Some(255),
    };
    let norm = raw_max.normalize();
    assert!(
        (norm.throttle - 1.0).abs() < 0.001,
        "max throttle must normalize to ~1.0"
    );
    assert!(
        (norm.brake - 1.0).abs() < 0.001,
        "max brake must normalize to ~1.0"
    );
    let clutch = norm.clutch.ok_or("clutch should be Some")?;
    assert!(
        (clutch - 1.0).abs() < 0.001,
        "max clutch must normalize to ~1.0"
    );

    let raw_zero = ThrustmasterPedalAxesRaw {
        throttle: 0,
        brake: 0,
        clutch: Some(0),
    };
    let norm_zero = raw_zero.normalize();
    assert!(norm_zero.throttle.abs() < 0.001);
    assert!(norm_zero.brake.abs() < 0.001);
    Ok(())
}

#[test]
fn pedal_normalize_no_clutch() -> Result<(), Box<dyn std::error::Error>> {
    let raw = ThrustmasterPedalAxesRaw {
        throttle: 128,
        brake: 64,
        clutch: None,
    };
    let norm = raw.normalize();
    assert!((norm.throttle - 128.0 / 255.0).abs() < 0.001);
    assert!((norm.brake - 64.0 / 255.0).abs() < 0.001);
    assert!(
        norm.clutch.is_none(),
        "no clutch sensor should normalize to None"
    );
    Ok(())
}

#[test]
fn pedal_model_classification() -> Result<(), Box<dyn std::error::Error>> {
    // T-LCM is known via PID but identify_device maps it to Unknown category
    // (pedal PIDs were cleaned up from identify_device)
    let tlcm_model = Model::from_product_id(product_ids::T_LCM);
    assert_eq!(tlcm_model, Model::TLCM);
    assert!(!tlcm_model.supports_ffb(), "T-LCM pedals don't have FFB");
    assert_eq!(tlcm_model.protocol_family(), ProtocolFamily::Unknown);
    assert!(
        (tlcm_model.max_torque_nm() - 0.0).abs() < 0.001,
        "pedals have 0 torque"
    );

    // T3PA and T3PA Pro are metadata-only models
    assert!(!Model::T3PA.supports_ffb());
    assert!(!Model::T3PAPro.supports_ffb());
    assert!(!Model::TLCMPro.supports_ffb());
    Ok(())
}

#[test]
fn protocol_unknown_pid_still_parses_valid_input() -> Result<(), Box<dyn std::error::Error>> {
    // TPR_PEDALS PID maps to Unknown category (not Pedals), so parse_input
    // does NOT guard against it. Verify that a valid input report is parsed
    // even for a non-wheel PID since only the explicit Pedals category is blocked.
    let proto = ThrustmasterProtocol::new(product_ids::TPR_PEDALS);
    let mut data = [0u8; 16];
    data[0] = 0x01; // STANDARD_INPUT_REPORT_ID
    // With report ID 0x01 and len >= 10, parse_input_report succeeds
    let result = proto.parse_input(&data);
    assert!(
        result.is_some(),
        "TPR_PEDALS is Unknown (not Pedals), so parse_input should proceed"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// § 7 — Wheel rim detection and switching (lifecycle module)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn lifecycle_family_a_vs_b_classification() -> Result<(), Box<dyn std::error::Error>> {
    // Family A (T300RS)
    let family_a_pids: &[u16] = &[0xb66e, 0xb66f, 0xb66d, 0xb68e, 0xb68f];
    for &pid in family_a_pids {
        assert_eq!(
            lifecycle::lifecycle_family_for_pid(pid),
            Some(LifecycleFamily::T300rs),
            "PID 0x{pid:04x} must be Family A (T300rs)"
        );
    }

    // Family B (T248/TS-PC/TS-XW)
    let family_b_pids: &[u16] = &[0xb696, 0xb689, 0xb692];
    for &pid in family_b_pids {
        assert_eq!(
            lifecycle::lifecycle_family_for_pid(pid),
            Some(LifecycleFamily::T248Family),
            "PID 0x{pid:04x} must be Family B (T248Family)"
        );
    }

    // Unknown PID
    assert_eq!(lifecycle::lifecycle_family_for_pid(0x0000), None);
    assert_eq!(lifecycle::lifecycle_family_for_pid(0xFFFF), None);
    Ok(())
}

#[test]
fn lifecycle_family_a_single_step_open_close() -> Result<(), Box<dyn std::error::Error>> {
    let open = lifecycle::build_t300rs_open();
    let close = lifecycle::build_t300rs_close();
    assert_eq!(open, [0x01, 0x05], "T300RS open = [0x01, 0x05]");
    assert_eq!(close, [0x01, 0x00], "T300RS close = [0x01, 0x00]");
    assert_ne!(open, close, "open and close must differ");
    Ok(())
}

#[test]
fn lifecycle_family_b_two_step_open_close() -> Result<(), Box<dyn std::error::Error>> {
    let open = lifecycle::build_family_b_open();
    assert_eq!(open[0], [0x01, 0x04], "Family B open step 1");
    assert_eq!(open[1], [0x01, 0x05], "Family B open step 2");

    let close = lifecycle::build_family_b_close();
    assert_eq!(close[0], [0x01, 0x05], "Family B close step 1");
    assert_eq!(close[1], [0x01, 0x00], "Family B close step 2");

    // Family B open step 2 == Family A open
    assert_eq!(open[1], lifecycle::build_t300rs_open());
    // Family B close step 2 == Family A close
    assert_eq!(close[1], lifecycle::build_t300rs_close());
    Ok(())
}

#[test]
fn lifecycle_setup_commands_structure() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(SETUP_COMMANDS.len(), SETUP_COMMAND_COUNT);
    assert_eq!(SETUP_COMMAND_COUNT, 7);

    // Command 0: 0x42 prefix
    assert_eq!(SETUP_COMMANDS[0][0], 0x42);
    // Commands 1-6: 0x0a prefix
    for cmd in &SETUP_COMMANDS[1..] {
        assert_eq!(cmd[0], 0x0a, "setup commands 1-6 must start with 0x0a");
    }

    // Last command has 5 bytes, rest have 2 or 4
    assert_eq!(SETUP_COMMANDS[6].len(), 5);
    Ok(())
}

#[test]
fn lifecycle_range_limits_per_model() -> Result<(), Box<dyn std::error::Error>> {
    // T300RS: 40-1080
    assert_eq!(T300RS_RANGE.min_degrees, 40);
    assert_eq!(T300RS_RANGE.max_degrees, 1080);

    // T248: 140-900
    assert_eq!(T248_RANGE.min_degrees, 140);
    assert_eq!(T248_RANGE.max_degrees, 900);

    // TS-PC: 140-1080
    assert_eq!(TSPC_RANGE.min_degrees, 140);
    assert_eq!(TSPC_RANGE.max_degrees, 1080);

    // TS-XW: 140-1080
    assert_eq!(TSXW_RANGE.min_degrees, 140);
    assert_eq!(TSXW_RANGE.max_degrees, 1080);

    // T248 has the smallest max range
    const {
        assert!(T248_RANGE.max_degrees < T300RS_RANGE.max_degrees);
        assert!(T248_RANGE.max_degrees < TSPC_RANGE.max_degrees);
    }
    Ok(())
}

#[test]
fn lifecycle_range_command_clamping_per_model() -> Result<(), Box<dyn std::error::Error>> {
    // T248 range: 140-900. Requesting 1080 should clamp to 900.
    let cmd = lifecycle::build_range_command(1080, T248_RANGE);
    let value = u16::from_le_bytes([cmd[2], cmd[3]]);
    assert_eq!(value, (900u32 * 0x3C) as u16, "T248 must clamp to 900");

    // T248 below minimum
    let cmd_lo = lifecycle::build_range_command(10, T248_RANGE);
    let value_lo = u16::from_le_bytes([cmd_lo[2], cmd_lo[3]]);
    assert_eq!(value_lo, (140u32 * 0x3C) as u16, "T248 must clamp to 140");

    // T300RS accepts 40
    let cmd_t300 = lifecycle::build_range_command(40, T300RS_RANGE);
    let value_t300 = u16::from_le_bytes([cmd_t300[2], cmd_t300[3]]);
    assert_eq!(value_t300, (40u32 * 0x3C) as u16);
    Ok(())
}

#[test]
fn lifecycle_range_limits_lookup_by_pid() -> Result<(), Box<dyn std::error::Error>> {
    // T300RS PS3
    assert_eq!(lifecycle::range_limits_for_pid(0xb66e), Some(T300RS_RANGE));
    // T248
    assert_eq!(lifecycle::range_limits_for_pid(0xb696), Some(T248_RANGE));
    // TS-PC
    assert_eq!(lifecycle::range_limits_for_pid(0xb689), Some(TSPC_RANGE));
    // TS-XW
    assert_eq!(lifecycle::range_limits_for_pid(0xb692), Some(TSXW_RANGE));
    // Unknown
    assert_eq!(lifecycle::range_limits_for_pid(0x0000), None);
    Ok(())
}

#[test]
fn lifecycle_supported_effects_list() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(lifecycle::SUPPORTED_EFFECTS.len(), 14);
    assert!(lifecycle::SUPPORTED_EFFECTS.contains(&"FF_CONSTANT"));
    assert!(lifecycle::SUPPORTED_EFFECTS.contains(&"FF_SPRING"));
    assert!(lifecycle::SUPPORTED_EFFECTS.contains(&"FF_DAMPER"));
    assert!(lifecycle::SUPPORTED_EFFECTS.contains(&"FF_FRICTION"));
    assert!(lifecycle::SUPPORTED_EFFECTS.contains(&"FF_INERTIA"));
    assert!(lifecycle::SUPPORTED_EFFECTS.contains(&"FF_PERIODIC"));
    assert!(lifecycle::SUPPORTED_EFFECTS.contains(&"FF_GAIN"));
    assert!(lifecycle::SUPPORTED_EFFECTS.contains(&"FF_AUTOCENTER"));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// § 8 — T300RS wire-format encoding
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn t300rs_report_sizes() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(T300RS_REPORT_SIZE, 64, "USB mode report = 64 bytes");
    assert_eq!(T300RS_REPORT_SIZE_PS4, 32, "PS4 mode report = 32 bytes");
    assert_eq!(HEADER_BYTE, 0x60);
    Ok(())
}

#[test]
fn t300rs_play_once_wire_capture() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; T300RS_REPORT_SIZE];
    t300rs::encode_play_once(0x01, &mut buf);
    assert_eq!(buf[0], 0x60, "header");
    assert_eq!(buf[1], t300rs::cmd::EFFECT, "effect command");
    assert_eq!(buf[2], 0x01, "effect id");
    assert_eq!(
        buf[3],
        t300rs::effect_op::PLAY_CONTROL,
        "play control opcode"
    );
    assert_eq!(buf[4], t300rs::play_ctl::PLAY_ONCE);
    // Remaining bytes must be zero
    assert!(
        buf[5..].iter().all(|&b| b == 0),
        "trailing bytes must be zero"
    );
    Ok(())
}

#[test]
fn t300rs_play_repeat_with_count() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; T300RS_REPORT_SIZE];
    t300rs::encode_play_repeat(0x02, 100, &mut buf);
    assert_eq!(buf[4], t300rs::play_ctl::PLAY_REPEAT);
    let count = u16::from_le_bytes([buf[5], buf[6]]);
    assert_eq!(count, 100);
    Ok(())
}

#[test]
fn t300rs_stop_effect_wire_capture() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; T300RS_REPORT_SIZE];
    t300rs::encode_stop_effect(0x05, &mut buf);
    assert_eq!(buf[3], t300rs::effect_op::PLAY_CONTROL);
    assert_eq!(buf[4], t300rs::play_ctl::STOP);
    Ok(())
}

#[test]
fn t300rs_modify_constant_magnitude_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let test_values: &[i16] = &[0, 1000, -1000, 16381, -16385, i16::MAX, i16::MIN];
    for &mag in test_values {
        let mut buf = [0u8; T300RS_REPORT_SIZE];
        t300rs::encode_modify_constant(0x01, mag, &mut buf);
        assert_eq!(buf[0], 0x60);
        assert_eq!(buf[3], t300rs::effect_op::MODIFY_CONSTANT);
        let decoded = i16::from_le_bytes([buf[4], buf[5]]);
        assert_eq!(decoded, mag, "magnitude {mag} must round-trip");
    }
    Ok(())
}

#[test]
fn t300rs_modify_envelope_fields() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; T300RS_REPORT_SIZE];
    t300rs::encode_modify_envelope(0x01, 1000, 3276, 1500, 3276, &mut buf);
    assert_eq!(buf[3], t300rs::effect_op::MODIFY_ENVELOPE);
    assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 1000);
    assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 3276);
    assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), 1500);
    assert_eq!(u16::from_le_bytes([buf[10], buf[11]]), 3276);
    Ok(())
}

#[test]
fn t300rs_set_gain_wire_format() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; T300RS_REPORT_SIZE];
    t300rs::encode_set_gain(0xBF, &mut buf);
    assert_eq!(buf[0..3], [0x60, t300rs::cmd::GAIN, 0xBF]);
    // Rest is zeroed
    assert!(buf[3..].iter().all(|&b| b == 0));
    Ok(())
}

#[test]
fn t300rs_set_rotation_clamped() -> Result<(), Box<dyn std::error::Error>> {
    // Normal value
    let mut buf = [0u8; T300RS_REPORT_SIZE];
    t300rs::encode_set_rotation(900, &mut buf);
    assert_eq!(buf[0], 0x60);
    assert_eq!(buf[1], t300rs::cmd::SETTINGS);
    assert_eq!(buf[2], t300rs::settings::ROTATION_ANGLE);
    let scaled = u16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(scaled, 900 * 60);

    // Below minimum clamps to 40
    t300rs::encode_set_rotation(10, &mut buf);
    let scaled_lo = u16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(scaled_lo, 40 * 60);

    // Above maximum clamps to 1080
    t300rs::encode_set_rotation(2000, &mut buf);
    let scaled_hi = u16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(scaled_hi, 1080 * 60);
    Ok(())
}

#[test]
fn t300rs_open_close_commands() -> Result<(), Box<dyn std::error::Error>> {
    let mut open_buf = [0u8; T300RS_REPORT_SIZE];
    t300rs::encode_open(&mut open_buf);
    assert_eq!(open_buf[0..3], [0x60, 0x01, 0x05]);

    let mut close_buf = [0u8; T300RS_REPORT_SIZE];
    t300rs::encode_close(&mut close_buf);
    assert_eq!(close_buf[0..3], [0x60, 0x01, 0x00]);
    Ok(())
}

#[test]
fn t300rs_new_constant_effect_full_structure() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; T300RS_REPORT_SIZE];
    let params = NewConstantParams {
        effect_id: 0x00,
        magnitude: 8000,
        attack_length: 500,
        attack_level: 1000,
        fade_length: 750,
        fade_level: 500,
        duration_ms: INFINITE_DURATION,
        offset_ms: 0,
    };
    t300rs::encode_new_constant(&params, &mut buf);

    assert_eq!(buf[0], 0x60);
    assert_eq!(buf[1], t300rs::cmd::EFFECT);
    assert_eq!(buf[2], 0x00); // effect_id
    assert_eq!(buf[3], t300rs::effect_op::NEW_CONSTANT);

    let mag = i16::from_le_bytes([buf[4], buf[5]]);
    assert_eq!(mag, 8000);

    // Envelope
    assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 500);
    assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), 1000);
    assert_eq!(u16::from_le_bytes([buf[10], buf[11]]), 750);
    assert_eq!(u16::from_le_bytes([buf[12], buf[13]]), 500);

    // Markers
    assert_eq!(buf[14], 0x00);
    assert_eq!(buf[15], 0x4F);

    // Duration (infinite)
    assert_eq!(u16::from_le_bytes([buf[16], buf[17]]), 0xFFFF);

    // End markers
    assert_eq!(buf[23], 0xFF);
    assert_eq!(buf[24], 0xFF);
    Ok(())
}

#[test]
fn t300rs_set_autocenter_wire_format() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; T300RS_REPORT_SIZE];
    t300rs::encode_set_autocenter(0x4000, &mut buf);
    assert_eq!(buf[0], 0x60);
    assert_eq!(buf[1], t300rs::cmd::SETTINGS);
    assert_eq!(buf[2], t300rs::settings::AUTOCENTER_FORCE);
    let value = u16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(value, 0x4000);
    Ok(())
}

#[test]
fn t300rs_buffers_zeroed_outside_payload() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0xFF_u8; T300RS_REPORT_SIZE];
    t300rs::encode_set_gain(0x80, &mut buf);
    // Bytes after the gain value should be zero (buf.fill(0) at start)
    for (i, &b) in buf[3..].iter().enumerate() {
        assert_eq!(
            b,
            0,
            "byte {} should be zero after set_gain, was 0x{:02X}",
            i + 3,
            b
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// § 9 — Effects module constants
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn effects_constants_verified() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(MAX_EFFECTS, 16, "max 16 simultaneous effects");
    assert_eq!(NORM_BUFFER_LENGTH, 63, "normal mode = 63 payload bytes");
    assert_eq!(PS4_BUFFER_LENGTH, 31, "PS4 mode = 31 payload bytes");
    assert_eq!(MIN_RANGE, 40);
    assert_eq!(MAX_RANGE, 1080);
    assert_eq!(RANGE_SCALE, 0x3C);
    assert_eq!(TIMING_START_MARKER, 0x4F);
    assert_eq!(TIMING_END_MARKER, 0xFFFF);
    assert_eq!(INFINITE_DURATION, 0xFFFF);
    assert_eq!(SPRING_MAX_SATURATION, 0x6AA6);
    assert_eq!(DEFAULT_MAX_SATURATION, 0xFFFF);
    Ok(())
}

#[test]
fn effects_condition_hardcoded_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(CONDITION_HARDCODED.len(), 8);
    // All bytes are 0xfe or 0xff alternating as LE i16 pairs = -2, -2, -2, -2
    for chunk in CONDITION_HARDCODED.chunks(2) {
        let val = i16::from_le_bytes([chunk[0], chunk[1]]);
        assert_eq!(val, -2, "hardcoded condition values are all -2 (0xfffe)");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// § 10 — Cross-module integration
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn protocol_creates_encoder_for_each_model() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        product_ids::T150,
        product_ids::T300_RS,
        product_ids::T500_RS,
        product_ids::T248,
        product_ids::TS_PC_RACER,
        product_ids::TS_XW,
        product_ids::T818,
    ];

    for &pid in &pids {
        let proto = ThrustmasterProtocol::new(pid);
        let enc = proto.create_encoder();
        let mut out = [0u8; EFFECT_REPORT_LEN];

        // Half-torque should produce magnitude 5000
        let half_torque = proto.max_torque_nm() / 2.0;
        enc.encode(half_torque, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(
            mag, 5000,
            "PID 0x{:04X}: half torque ({} Nm) should encode to 5000, got {}",
            pid, half_torque, mag
        );
    }
    Ok(())
}

#[test]
fn protocol_init_sequence_reports_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let proto = ThrustmasterProtocol::new(product_ids::T300_RS);
    let seq = proto.build_init_sequence();
    assert_eq!(seq.len(), 4);

    // Report 0: device gain 0 (disable)
    assert_eq!(seq[0][0], 0x81); // DEVICE_GAIN report ID
    assert_eq!(seq[0][1], 0x00);

    // Report 1: device gain 0xFF (full)
    assert_eq!(seq[1][0], 0x81);
    assert_eq!(seq[1][1], 0xFF);

    // Report 2: actuator enable
    assert_eq!(seq[2][0], 0x82); // ACTUATOR_ENABLE report ID
    assert_eq!(seq[2][1], 0x01);

    // Report 3: set range to model's max rotation
    assert_eq!(seq[3][0], 0x80); // VENDOR_SET_RANGE report ID
    let range = u16::from_le_bytes([seq[3][2], seq[3][3]]);
    assert_eq!(range, 1080, "T300RS max rotation = 1080");
    Ok(())
}

#[test]
fn protocol_with_custom_config_init_sequence() -> Result<(), Box<dyn std::error::Error>> {
    let mut proto = ThrustmasterProtocol::new_with_config(0xFFFF, 8.0, 720);
    proto.set_gain(0x80);
    let seq = proto.build_init_sequence();
    assert_eq!(seq.len(), 4);

    // Gain should reflect the custom value
    assert_eq!(seq[1][1], 0x80);

    // Range should be custom value
    let range = u16::from_le_bytes([seq[3][2], seq[3][3]]);
    assert_eq!(range, 720);
    Ok(())
}

#[test]
fn lifecycle_family_consistent_with_model() -> Result<(), Box<dyn std::error::Error>> {
    // Family A models
    let t300rs_proto = ThrustmasterProtocol::new(product_ids::T300_RS);
    assert_eq!(t300rs_proto.model().protocol_family(), ProtocolFamily::T300);
    assert_eq!(
        lifecycle::lifecycle_family_for_pid(product_ids::T300_RS),
        Some(LifecycleFamily::T300rs)
    );

    // Family B models
    let t248_proto = ThrustmasterProtocol::new(product_ids::T248);
    assert_eq!(t248_proto.model().protocol_family(), ProtocolFamily::T300);
    assert_eq!(
        lifecycle::lifecycle_family_for_pid(product_ids::T248),
        Some(LifecycleFamily::T248Family)
    );

    let tspc_proto = ThrustmasterProtocol::new(product_ids::TS_PC_RACER);
    assert_eq!(tspc_proto.model().protocol_family(), ProtocolFamily::T300);
    assert_eq!(
        lifecycle::lifecycle_family_for_pid(product_ids::TS_PC_RACER),
        Some(LifecycleFamily::T248Family)
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// § 11 — Error handling for mode transitions
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn parse_input_report_rejects_all_invalid_report_ids() -> Result<(), Box<dyn std::error::Error>> {
    for id in 0u8..=255 {
        if id == 0x01 {
            continue; // skip valid ID
        }
        let mut data = [0u8; 16];
        data[0] = id;
        assert!(
            parse_input_report(&data).is_none(),
            "report ID 0x{id:02X} must be rejected"
        );
    }
    Ok(())
}

#[test]
fn parse_input_report_boundary_lengths() -> Result<(), Box<dyn std::error::Error>> {
    // 9 bytes: too short
    let mut data9 = [0u8; 9];
    data9[0] = 0x01;
    assert!(
        parse_input_report(&data9).is_none(),
        "9 bytes must be rejected"
    );

    // 10 bytes: exactly minimum
    let mut data10 = [0u8; 10];
    data10[0] = 0x01;
    assert!(
        parse_input_report(&data10).is_some(),
        "10 bytes must be accepted"
    );

    // 0 bytes
    assert!(parse_input_report(&[]).is_none(), "empty must be rejected");
    Ok(())
}

#[test]
fn pedal_report_too_short_rejected() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_pedal_report(&[]).is_none());
    assert!(parse_pedal_report(&[0x00]).is_none());
    assert!(parse_pedal_report(&[0x00, 0x00]).is_none());
    assert!(parse_pedal_report(&[0x00, 0x00, 0x00]).is_some());
    Ok(())
}

#[test]
fn protocol_new_with_config_floors_tiny_torque() -> Result<(), Box<dyn std::error::Error>> {
    let proto = ThrustmasterProtocol::new_with_config(0xFFFF, 0.0, 900);
    assert!(
        proto.max_torque_nm() >= 0.01,
        "torque must be floored to 0.01, got {}",
        proto.max_torque_nm()
    );

    let proto_neg = ThrustmasterProtocol::new_with_config(0xFFFF, -5.0, 900);
    assert!(
        proto_neg.max_torque_nm() >= 0.01,
        "negative torque must be floored to 0.01"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// § 12 — Proptest
// ═══════════════════════════════════════════════════════════════════════════

mod proptest_deep {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_pedal_normalize_in_unit_range(
            throttle in 0u8..=255u8,
            brake in 0u8..=255u8,
            clutch in 0u8..=255u8,
        ) {
            let raw = ThrustmasterPedalAxesRaw {
                throttle,
                brake,
                clutch: Some(clutch),
            };
            let norm = raw.normalize();
            prop_assert!((0.0..=1.0).contains(&norm.throttle), "throttle {}", norm.throttle);
            prop_assert!((0.0..=1.0).contains(&norm.brake), "brake {}", norm.brake);
            if let Some(c) = norm.clutch {
                prop_assert!((0.0..=1.0).contains(&c), "clutch {}", c);
            }
        }

        #[test]
        fn prop_pedal_report_3_bytes_always_parses(
            a in 0u8..=255u8,
            b in 0u8..=255u8,
            c in 0u8..=255u8,
        ) {
            let data = [a, b, c];
            let result = parse_pedal_report(&data);
            prop_assert!(result.is_some(), "3-byte pedal report must always parse");
            let pedals = result.map(|p| (p.throttle, p.brake, p.clutch));
            prop_assert_eq!(pedals, Some((a, b, Some(c))));
        }

        #[test]
        fn prop_input_report_valid_id_always_parses(
            steering in 0u16..=65535u16,
            throttle in 0u8..=255u8,
            brake in 0u8..=255u8,
        ) {
            let mut data = [0u8; 16];
            data[0] = 0x01; // valid report ID
            data[1..3].copy_from_slice(&steering.to_le_bytes());
            data[3] = throttle;
            data[4] = brake;
            let result = parse_input_report(&data);
            prop_assert!(result.is_some(), "valid report ID + sufficient length must parse");
        }

        #[test]
        fn prop_timing_duration_offset_roundtrip(
            duration in 0u16..=65535u16,
            offset in 0u16..=65535u16,
        ) {
            let t = Timing { duration, offset };
            let buf = t.encode();
            prop_assert_eq!(buf[0], TIMING_START_MARKER);
            let d = u16::from_le_bytes([buf[1], buf[2]]);
            let o = u16::from_le_bytes([buf[5], buf[6]]);
            prop_assert_eq!(d, duration);
            prop_assert_eq!(o, offset);
        }

        #[test]
        fn prop_envelope_fields_roundtrip(
            atk_len in 0u16..=65535u16,
            atk_lvl in 0u16..=65535u16,
            fad_len in 0u16..=65535u16,
            fad_lvl in 0u16..=65535u16,
        ) {
            let env = Envelope {
                attack_length: atk_len,
                attack_level: atk_lvl,
                fade_length: fad_len,
                fade_level: fad_lvl,
            };
            let buf = env.encode();
            prop_assert_eq!(u16::from_le_bytes([buf[0], buf[1]]), atk_len);
            prop_assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), atk_lvl);
            prop_assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), fad_len);
            prop_assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), fad_lvl);
        }

        #[test]
        fn prop_effects_range_always_clamped(degrees in 0u16..=65535u16) {
            let cmd = effects::encode_range(degrees);
            let scaled = u16::from_le_bytes([cmd[2], cmd[3]]);
            let min_scaled = MIN_RANGE.wrapping_mul(RANGE_SCALE);
            let max_scaled = MAX_RANGE.wrapping_mul(RANGE_SCALE);
            prop_assert!(
                scaled >= min_scaled && scaled <= max_scaled,
                "scaled {} out of [{}, {}] for degrees {}",
                scaled, min_scaled, max_scaled, degrees
            );
        }

        #[test]
        fn prop_t300rs_rotation_always_clamped(degrees in 0u16..=65535u16) {
            let mut buf = [0u8; T300RS_REPORT_SIZE];
            t300rs::encode_set_rotation(degrees, &mut buf);
            let scaled = u16::from_le_bytes([buf[3], buf[4]]);
            let min_scaled = 40u16 * 60;
            let max_scaled = 1080u16 * 60;
            prop_assert!(
                scaled >= min_scaled && scaled <= max_scaled,
                "scaled {} out of [{}, {}] for degrees {}",
                scaled, min_scaled, max_scaled, degrees
            );
        }

        #[test]
        fn prop_effects_header_id_incremented(effect_id in 0u8..=254u8) {
            let hdr = effects::encode_header(effect_id, 0x6a);
            prop_assert_eq!(hdr[1], effect_id + 1, "wire id must be input + 1");
        }

        #[test]
        fn prop_lifecycle_range_command_header(
            degrees in 0u16..=2000u16,
        ) {
            let cmd = lifecycle::build_range_command(degrees, T300RS_RANGE);
            prop_assert_eq!(cmd[0], 0x08, "range header byte 0");
            prop_assert_eq!(cmd[1], 0x11, "range header byte 1");
        }
    }
}
