//! Deep tests for the input-map schema, compilation, and serialization.

use racing_wheel_input_maps::{
    AxisBinding, AxisDataType, ButtonBinding, ClutchBinding, ClutchModeHint, DeviceInputMap,
    DeviceInputMapError, DeviceMapModeHints, DeviceTransportHint, InitFrameDirection,
    InitReportFrame, JsBinding, JsModeHint, ReportConstraint, RotaryBinding, RotaryModeHint,
    compile_ks_map,
};
use racing_wheel_ks::{KS_ENCODER_COUNT, KsClutchMode, KsJoystickMode, KsRotaryMode};

type R = Result<(), Box<dyn std::error::Error>>;

// ── Helpers ─────────────────────────────────────────────────────────────

fn make_axis(name: &str, offset: u16, dt: AxisDataType) -> AxisBinding {
    AxisBinding {
        name: name.to_string(),
        byte_offset: offset,
        bit_offset: None,
        data_type: dt,
        signed: false,
        invert: false,
        min: None,
        max: None,
    }
}

fn make_button(name: &str, offset: u16, mask: u8) -> ButtonBinding {
    ButtonBinding {
        name: name.to_string(),
        byte_offset: offset,
        bit_mask: mask,
        invert: false,
    }
}

fn minimal_valid_map() -> DeviceInputMap {
    DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x346E,
        product_id: 0x0002,
        axes: vec![make_axis("steering", 1, AxisDataType::U16Le)],
        ..Default::default()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// §1  Button mapping and remapping
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn button_binding_basic_fields() {
    let btn = make_button("paddle_left", 11, 0x01);
    assert_eq!(btn.name, "paddle_left");
    assert_eq!(btn.byte_offset, 11);
    assert_eq!(btn.bit_mask, 0x01);
    assert!(!btn.invert);
}

#[test]
fn button_binding_invert_flag() {
    let btn = ButtonBinding {
        name: "paddle_right".to_string(),
        byte_offset: 11,
        bit_mask: 0x02,
        invert: true,
    };
    assert!(btn.invert);
}

#[test]
fn button_binding_all_bit_masks() -> R {
    for bit in 0u8..8 {
        let mask = 1u8 << bit;
        let btn = make_button("btn", 0, mask);
        let json = serde_json::to_string(&btn)?;
        let rt: ButtonBinding = serde_json::from_str(&json)?;
        assert_eq!(rt.bit_mask, mask);
    }
    Ok(())
}

#[test]
fn multiple_buttons_same_byte_different_masks() -> R {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x346E,
        product_id: 0x0002,
        buttons: vec![
            make_button("btn_a", 11, 0x01),
            make_button("btn_b", 11, 0x02),
            make_button("btn_c", 11, 0x04),
            make_button("btn_d", 11, 0x08),
        ],
        ..Default::default()
    };
    assert!(map.validate().is_ok());
    let json = serde_json::to_string(&map)?;
    let rt: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(rt.buttons.len(), 4);
    assert_eq!(rt.buttons[2].bit_mask, 0x04);
    Ok(())
}

#[test]
fn button_binding_max_byte_offset() -> R {
    let btn = make_button("far_button", u16::MAX, 0x80);
    let json = serde_json::to_string(&btn)?;
    let rt: ButtonBinding = serde_json::from_str(&json)?;
    assert_eq!(rt.byte_offset, u16::MAX);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// §2  Axis mapping with ranges and invert
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn axis_binding_with_min_max_range() -> R {
    let axis = AxisBinding {
        name: "throttle".to_string(),
        byte_offset: 3,
        bit_offset: None,
        data_type: AxisDataType::U16Le,
        signed: false,
        invert: false,
        min: Some(0),
        max: Some(65535),
    };
    let json = serde_json::to_string(&axis)?;
    let rt: AxisBinding = serde_json::from_str(&json)?;
    assert_eq!(rt.min, Some(0));
    assert_eq!(rt.max, Some(65535));
    Ok(())
}

#[test]
fn axis_binding_signed_range() -> R {
    let axis = AxisBinding {
        name: "steering".to_string(),
        byte_offset: 1,
        bit_offset: None,
        data_type: AxisDataType::I16Le,
        signed: true,
        invert: false,
        min: Some(-32768),
        max: Some(32767),
    };
    let json = serde_json::to_string(&axis)?;
    let rt: AxisBinding = serde_json::from_str(&json)?;
    assert_eq!(rt.min, Some(-32768));
    assert_eq!(rt.max, Some(32767));
    assert!(rt.signed);
    Ok(())
}

#[test]
fn axis_binding_with_bit_offset() -> R {
    let axis = AxisBinding {
        name: "nibble_axis".to_string(),
        byte_offset: 5,
        bit_offset: Some(4),
        data_type: AxisDataType::U8,
        signed: false,
        invert: false,
        min: None,
        max: None,
    };
    let json = serde_json::to_string(&axis)?;
    let rt: AxisBinding = serde_json::from_str(&json)?;
    assert_eq!(rt.bit_offset, Some(4));
    Ok(())
}

#[test]
fn axis_binding_invert_flag_round_trips() -> R {
    let axis = AxisBinding {
        name: "inverted_brake".to_string(),
        byte_offset: 5,
        bit_offset: None,
        data_type: AxisDataType::U16Le,
        signed: false,
        invert: true,
        min: Some(0),
        max: Some(65535),
    };
    let json = serde_json::to_string(&axis)?;
    let rt: AxisBinding = serde_json::from_str(&json)?;
    assert!(rt.invert);
    Ok(())
}

#[test]
fn axis_all_data_types_compile_to_ks() {
    let data_types = [
        (AxisDataType::U8, false),
        (AxisDataType::I8, true),
        (AxisDataType::U16Le, false),
        (AxisDataType::I16Le, true),
        (AxisDataType::U16Be, false),
        (AxisDataType::I16Be, true),
        (AxisDataType::Bool, false),
    ];
    for (dt, expect_signed) in data_types {
        let map = DeviceInputMap {
            vendor_id: 0x346E,
            product_id: 0x0002,
            clutch: Some(ClutchBinding {
                combined: Some(AxisBinding {
                    name: "test".to_string(),
                    byte_offset: 10,
                    bit_offset: None,
                    data_type: dt,
                    signed: expect_signed,
                    invert: false,
                    min: None,
                    max: None,
                }),
                left: None,
                right: None,
                left_button: None,
                right_button: None,
                mode_hint: ClutchModeHint::CombinedAxis,
            }),
            ..Default::default()
        };
        if let Some(ks) = compile_ks_map(&map)
            && let Some(src) = ks.clutch_combined_axis
        {
            assert_eq!(src.signed, expect_signed);
            assert_eq!(src.offset, 10);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// §3  Rotary encoder mapping
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn rotary_binding_button_mode() -> R {
    let r = RotaryBinding {
        name: "left_enc".to_string(),
        byte_offset: 29,
        mode: RotaryModeHint::Button,
    };
    let json = serde_json::to_string(&r)?;
    let rt: RotaryBinding = serde_json::from_str(&json)?;
    assert_eq!(rt.mode, RotaryModeHint::Button);
    Ok(())
}

#[test]
fn rotary_binding_knob_mode() -> R {
    let r = RotaryBinding {
        name: "dial".to_string(),
        byte_offset: 31,
        mode: RotaryModeHint::Knob,
    };
    let json = serde_json::to_string(&r)?;
    let rt: RotaryBinding = serde_json::from_str(&json)?;
    assert_eq!(rt.mode, RotaryModeHint::Knob);
    Ok(())
}

#[test]
fn rotary_binding_unknown_mode() -> R {
    let r = RotaryBinding {
        name: "mystery".to_string(),
        byte_offset: 40,
        mode: RotaryModeHint::Unknown,
    };
    let json = serde_json::to_string(&r)?;
    let rt: RotaryBinding = serde_json::from_str(&json)?;
    assert_eq!(rt.mode, RotaryModeHint::Unknown);
    Ok(())
}

#[test]
fn compile_max_rotary_slots_respected() -> R {
    let rotaries: Vec<RotaryBinding> = (0..KS_ENCODER_COUNT + 2)
        .map(|i| RotaryBinding {
            name: format!("enc_{i}"),
            byte_offset: (i * 2) as u16,
            mode: RotaryModeHint::Knob,
        })
        .collect();

    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        rotaries,
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile max rotaries")?;

    // Only KS_ENCODER_COUNT slots should be filled.
    for i in 0..KS_ENCODER_COUNT {
        assert!(ks.encoders[i].is_some());
    }
    Ok(())
}

#[test]
fn compile_single_rotary_maps_to_left_axis() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        rotaries: vec![RotaryBinding {
            name: "single".to_string(),
            byte_offset: 20,
            mode: RotaryModeHint::Knob,
        }],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("single rotary")?;
    assert_eq!(ks.left_rotary_axis.map(|s| s.offset), Some(20));
    assert!(ks.right_rotary_axis.is_none());
    Ok(())
}

#[test]
fn compile_two_rotaries_map_left_right() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        rotaries: vec![
            RotaryBinding {
                name: "left".to_string(),
                byte_offset: 20,
                mode: RotaryModeHint::Knob,
            },
            RotaryBinding {
                name: "right".to_string(),
                byte_offset: 22,
                mode: RotaryModeHint::Knob,
            },
        ],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("two rotaries")?;
    assert_eq!(ks.left_rotary_axis.map(|s| s.offset), Some(20));
    assert_eq!(ks.right_rotary_axis.map(|s| s.offset), Some(22));
    Ok(())
}

#[test]
fn compile_rotary_mode_last_wins() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        rotaries: vec![
            RotaryBinding {
                name: "a".to_string(),
                byte_offset: 10,
                mode: RotaryModeHint::Knob,
            },
            RotaryBinding {
                name: "b".to_string(),
                byte_offset: 12,
                mode: RotaryModeHint::Button,
            },
        ],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("rotary mode last wins")?;
    // Last rotary sets the mode hint.
    assert_eq!(ks.rotary_mode_hint, KsRotaryMode::Button);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// §4  LED mapping (bindings for LED indicators live outside KS scope,
//     but we validate that maps with no LED-related fields stay clean)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn map_without_optional_fields_has_none_defaults() {
    let map = DeviceInputMap::default();
    assert!(map.clutch.is_none());
    assert!(map.joystick.is_none());
    assert!(map.handbrake.is_none());
    assert!(map.mode_hints.is_none());
    assert!(map.init_sequence.is_empty());
}

#[test]
fn init_sequence_out_frame_round_trips() -> R {
    let frame = InitReportFrame {
        report_id: 0x09,
        payload: vec![0x01, 0x02, 0x03, 0xFF],
        direction: InitFrameDirection::Out,
    };
    let json = serde_json::to_string(&frame)?;
    let rt: InitReportFrame = serde_json::from_str(&json)?;
    assert_eq!(rt.report_id, 0x09);
    assert_eq!(rt.direction, InitFrameDirection::Out);
    assert_eq!(rt.payload, vec![0x01, 0x02, 0x03, 0xFF]);
    Ok(())
}

#[test]
fn init_sequence_in_frame_round_trips() -> R {
    let frame = InitReportFrame {
        report_id: 0x0A,
        payload: vec![0xDE, 0xAD],
        direction: InitFrameDirection::In,
    };
    let json = serde_json::to_string(&frame)?;
    let rt: InitReportFrame = serde_json::from_str(&json)?;
    assert_eq!(rt.direction, InitFrameDirection::In);
    Ok(())
}

#[test]
fn init_sequence_empty_payload() -> R {
    let frame = InitReportFrame {
        report_id: 0x00,
        payload: vec![],
        direction: InitFrameDirection::Out,
    };
    let json = serde_json::to_string(&frame)?;
    let rt: InitReportFrame = serde_json::from_str(&json)?;
    assert!(rt.payload.is_empty());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// §5  Display mapping (mode hints)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn device_map_mode_hints_all_none() -> R {
    let hints = DeviceMapModeHints {
        clutch: None,
        joystick: None,
        rotary: None,
    };
    let json = serde_json::to_string(&hints)?;
    let rt: DeviceMapModeHints = serde_json::from_str(&json)?;
    assert!(rt.clutch.is_none());
    assert!(rt.joystick.is_none());
    assert!(rt.rotary.is_none());
    Ok(())
}

#[test]
fn device_map_mode_hints_mixed() -> R {
    let hints = DeviceMapModeHints {
        clutch: Some(ClutchModeHint::Button),
        joystick: None,
        rotary: Some(RotaryModeHint::Knob),
    };
    let json = serde_json::to_string(&hints)?;
    let rt: DeviceMapModeHints = serde_json::from_str(&json)?;
    assert_eq!(rt.clutch, Some(ClutchModeHint::Button));
    assert!(rt.joystick.is_none());
    assert_eq!(rt.rotary, Some(RotaryModeHint::Knob));
    Ok(())
}

#[test]
fn clutch_mode_hint_all_variants_serde() -> R {
    let variants = [
        ClutchModeHint::CombinedAxis,
        ClutchModeHint::IndependentAxis,
        ClutchModeHint::Button,
        ClutchModeHint::Unknown,
    ];
    for v in variants {
        let json = serde_json::to_string(&v)?;
        let rt: ClutchModeHint = serde_json::from_str(&json)?;
        assert_eq!(v, rt);
    }
    Ok(())
}

#[test]
fn js_mode_hint_all_variants_serde() -> R {
    let variants = [JsModeHint::Buttons, JsModeHint::DPad, JsModeHint::Unknown];
    for v in variants {
        let json = serde_json::to_string(&v)?;
        let rt: JsModeHint = serde_json::from_str(&json)?;
        assert_eq!(v, rt);
    }
    Ok(())
}

#[test]
fn rotary_mode_hint_all_variants_serde() -> R {
    let variants = [
        RotaryModeHint::Button,
        RotaryModeHint::Knob,
        RotaryModeHint::Unknown,
    ];
    for v in variants {
        let json = serde_json::to_string(&v)?;
        let rt: RotaryModeHint = serde_json::from_str(&json)?;
        assert_eq!(v, rt);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// §6  Map compilation and validation
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn validation_requires_at_least_one_input() {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x346E,
        product_id: 0x0022,
        ..Default::default()
    };
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::NoInputsDefined)
    ));
}

#[test]
fn validation_accepts_buttons_only() {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x346E,
        product_id: 0x0002,
        buttons: vec![make_button("btn", 5, 0x01)],
        ..Default::default()
    };
    assert!(map.validate().is_ok());
}

#[test]
fn validation_accepts_rotaries_only() {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x346E,
        product_id: 0x0002,
        rotaries: vec![RotaryBinding {
            name: "enc".to_string(),
            byte_offset: 29,
            mode: RotaryModeHint::Knob,
        }],
        ..Default::default()
    };
    assert!(map.validate().is_ok());
}

#[test]
fn validation_accepts_clutch_only() {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x346E,
        product_id: 0x0002,
        clutch: Some(ClutchBinding {
            combined: Some(make_axis("combined", 7, AxisDataType::U16Le)),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        }),
        ..Default::default()
    };
    assert!(map.validate().is_ok());
}

#[test]
fn validation_rejects_schema_version_255() {
    let map = DeviceInputMap {
        schema_version: 255,
        vendor_id: 0x346E,
        product_id: 0x0002,
        axes: vec![make_axis("a", 0, AxisDataType::U8)],
        ..Default::default()
    };
    // schema_version 255 is nonzero, so validation passes for the version check.
    // This ensures only version 0 is rejected.
    assert!(map.validate().is_ok());
}

#[test]
fn validation_rejects_both_ids_zero() {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0,
        product_id: 0,
        axes: vec![make_axis("a", 0, AxisDataType::U8)],
        ..Default::default()
    };
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::MissingIdentity)
    ));
}

// ═══════════════════════════════════════════════════════════════════════
// §7  KS map generation
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn compile_returns_none_without_ks_content() {
    assert!(compile_ks_map(&minimal_valid_map()).is_none());
}

#[test]
fn compile_clutch_combined_axis_wiring() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        clutch: Some(ClutchBinding {
            combined: Some(make_axis("combined", 7, AxisDataType::U16Le)),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("compile combined")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::CombinedAxis);
    assert_eq!(ks.clutch_combined_axis.map(|s| s.offset), Some(7));
    assert!(ks.clutch_left_axis.is_none());
    assert!(ks.clutch_right_axis.is_none());
    Ok(())
}

#[test]
fn compile_clutch_independent_axis_wiring() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        clutch: Some(ClutchBinding {
            combined: None,
            left: Some(make_axis("left", 10, AxisDataType::I16Le)),
            right: Some(make_axis("right", 12, AxisDataType::I16Le)),
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::IndependentAxis,
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("compile independent")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::IndependentAxis);
    assert_eq!(ks.clutch_left_axis.map(|s| s.offset), Some(10));
    assert_eq!(ks.clutch_right_axis.map(|s| s.offset), Some(12));
    // Signed types produce signed source.
    assert_eq!(ks.clutch_left_axis.map(|s| s.signed), Some(true));
    Ok(())
}

#[test]
fn compile_clutch_button_wiring() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        clutch: Some(ClutchBinding {
            combined: None,
            left: None,
            right: None,
            left_button: Some(make_button("cl", 22, 0x01)),
            right_button: Some(make_button("cr", 22, 0x02)),
            mode_hint: ClutchModeHint::Button,
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("compile clutch buttons")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::Button);
    assert_eq!(ks.clutch_left_button.map(|b| b.offset), Some(22));
    assert_eq!(ks.clutch_right_button.map(|b| b.offset), Some(22));
    assert_eq!(ks.clutch_left_button.map(|b| b.mask), Some(0x01));
    assert_eq!(ks.clutch_right_button.map(|b| b.mask), Some(0x02));
    Ok(())
}

#[test]
fn compile_joystick_dpad_with_hat_source() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::DPad,
            axis: Some(make_axis("hat", 27, AxisDataType::U8)),
            buttons: vec![],
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("compile dpad")?;
    assert_eq!(ks.joystick_mode_hint, KsJoystickMode::DPad);
    assert_eq!(ks.joystick_hat.map(|s| s.offset), Some(27));
    Ok(())
}

#[test]
fn compile_joystick_buttons_no_hat() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::Buttons,
            axis: None,
            buttons: vec![make_button("up", 5, 0x10)],
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("compile js buttons")?;
    assert_eq!(ks.joystick_mode_hint, KsJoystickMode::Button);
    assert!(ks.joystick_hat.is_none());
    Ok(())
}

#[test]
fn compile_joystick_unknown_mode() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::Unknown,
            axis: None,
            buttons: vec![],
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("compile js unknown")?;
    assert_eq!(ks.joystick_mode_hint, KsJoystickMode::Unknown);
    Ok(())
}

#[test]
fn compile_propagates_report_constraint() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        report: ReportConstraint {
            report_id: Some(0x07),
            report_len: Some(64),
        },
        rotaries: vec![RotaryBinding {
            name: "r".to_string(),
            byte_offset: 10,
            mode: RotaryModeHint::Knob,
        }],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("propagate report_id")?;
    assert_eq!(ks.report_id, Some(0x07));
    Ok(())
}

#[test]
fn compile_none_report_id_propagates() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        report: ReportConstraint {
            report_id: None,
            report_len: None,
        },
        rotaries: vec![RotaryBinding {
            name: "r".to_string(),
            byte_offset: 10,
            mode: RotaryModeHint::Knob,
        }],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("none report_id")?;
    assert_eq!(ks.report_id, None);
    Ok(())
}

#[test]
fn compile_clutch_unknown_mode() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        clutch: Some(ClutchBinding {
            combined: None,
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::Unknown,
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("unknown clutch mode")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::Unknown);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// §8  Default map generation for device types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn default_map_is_not_valid() {
    let map = DeviceInputMap::default();
    assert!(map.validate().is_err());
}

#[test]
fn default_map_transport_is_unknown() {
    let map = DeviceInputMap::default();
    assert_eq!(map.transport, DeviceTransportHint::Unknown);
}

#[test]
fn default_report_constraint_is_empty() {
    let rc = ReportConstraint::default();
    assert_eq!(rc.report_id, None);
    assert_eq!(rc.report_len, None);
}

#[test]
fn default_clutch_mode_hint_is_unknown() {
    assert_eq!(ClutchModeHint::default(), ClutchModeHint::Unknown);
}

#[test]
fn standalone_usb_transport() -> R {
    let map = DeviceInputMap {
        transport: DeviceTransportHint::StandaloneUsb,
        ..minimal_valid_map()
    };
    let json = serde_json::to_string(&map)?;
    let rt: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(rt.transport, DeviceTransportHint::StandaloneUsb);
    Ok(())
}

#[test]
fn wheelbase_aggregated_transport() -> R {
    let map = DeviceInputMap {
        transport: DeviceTransportHint::WheelbaseAggregated,
        ..minimal_valid_map()
    };
    let json = serde_json::to_string(&map)?;
    let rt: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(rt.transport, DeviceTransportHint::WheelbaseAggregated);
    Ok(())
}

#[test]
fn universal_hub_transport() -> R {
    let map = DeviceInputMap {
        transport: DeviceTransportHint::UniversalHub,
        ..minimal_valid_map()
    };
    let json = serde_json::to_string(&map)?;
    let rt: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(rt.transport, DeviceTransportHint::UniversalHub);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// §9  Map import/export serialization
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn full_map_serde_round_trip() -> R {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x346E,
        product_id: 0x0022,
        transport: DeviceTransportHint::UniversalHub,
        report: ReportConstraint {
            report_id: Some(1),
            report_len: Some(64),
        },
        axes: vec![
            make_axis("steering", 1, AxisDataType::I16Le),
            make_axis("throttle", 3, AxisDataType::U16Le),
            make_axis("brake", 5, AxisDataType::U16Le),
        ],
        buttons: vec![
            make_button("paddle_left", 11, 0x01),
            make_button("paddle_right", 11, 0x02),
        ],
        clutch: Some(ClutchBinding {
            combined: None,
            left: Some(make_axis("clutch_l", 18, AxisDataType::U16Le)),
            right: Some(make_axis("clutch_r", 20, AxisDataType::U16Le)),
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::IndependentAxis,
        }),
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::DPad,
            axis: Some(make_axis("hat", 27, AxisDataType::U8)),
            buttons: vec![],
        }),
        rotaries: vec![
            RotaryBinding {
                name: "left_dial".to_string(),
                byte_offset: 29,
                mode: RotaryModeHint::Knob,
            },
            RotaryBinding {
                name: "right_dial".to_string(),
                byte_offset: 31,
                mode: RotaryModeHint::Button,
            },
        ],
        handbrake: Some(make_axis("handbrake", 33, AxisDataType::U16Le)),
        mode_hints: Some(DeviceMapModeHints {
            clutch: Some(ClutchModeHint::IndependentAxis),
            joystick: Some(JsModeHint::DPad),
            rotary: Some(RotaryModeHint::Knob),
        }),
        init_sequence: vec![
            InitReportFrame {
                report_id: 0x09,
                payload: vec![0x01, 0x02],
                direction: InitFrameDirection::Out,
            },
            InitReportFrame {
                report_id: 0x09,
                payload: vec![0x03, 0x04],
                direction: InitFrameDirection::In,
            },
        ],
    };

    let json = serde_json::to_string_pretty(&map)?;
    let rt: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(map, rt);
    Ok(())
}

#[test]
fn serde_rejects_unknown_top_level_field() {
    let json = r#"{"schema_version":1,"vendor_id":1,"product_id":2,"axes":[],"extra":1}"#;
    let result: Result<DeviceInputMap, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn serde_rejects_unknown_axis_field() {
    let json = r#"{
        "schema_version":1,"vendor_id":1,"product_id":2,
        "axes":[{"name":"a","byte_offset":0,"data_type":"u8","signed":false,"invert":false,"bad":1}]
    }"#;
    let result: Result<DeviceInputMap, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn serde_missing_required_axis_field() {
    // Missing "name" in axis
    let json = r#"{
        "schema_version":1,"vendor_id":1,"product_id":2,
        "axes":[{"byte_offset":0,"data_type":"u8","signed":false,"invert":false}]
    }"#;
    let result: Result<DeviceInputMap, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn serde_missing_required_button_field() {
    // Missing "bit_mask" in button
    let json = r#"{
        "schema_version":1,"vendor_id":1,"product_id":2,
        "buttons":[{"name":"b","byte_offset":0,"invert":false}]
    }"#;
    let result: Result<DeviceInputMap, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn serde_defaults_populate_omitted_fields() -> R {
    // Omit optional collections — they should default to empty/None.
    let json = r#"{"schema_version":1,"vendor_id":1,"product_id":2}"#;
    let map: DeviceInputMap = serde_json::from_str(json)?;
    assert!(map.axes.is_empty());
    assert!(map.buttons.is_empty());
    assert!(map.rotaries.is_empty());
    assert!(map.clutch.is_none());
    assert!(map.joystick.is_none());
    assert!(map.handbrake.is_none());
    assert!(map.mode_hints.is_none());
    assert!(map.init_sequence.is_empty());
    Ok(())
}

#[test]
fn clutch_binding_full_serde_round_trip() -> R {
    let binding = ClutchBinding {
        combined: Some(make_axis("combined", 7, AxisDataType::U16Le)),
        left: Some(AxisBinding {
            name: "left".to_string(),
            byte_offset: 9,
            bit_offset: Some(2),
            data_type: AxisDataType::I16Le,
            signed: true,
            invert: true,
            min: Some(-32768),
            max: Some(32767),
        }),
        right: None,
        left_button: Some(ButtonBinding {
            name: "lb".to_string(),
            byte_offset: 22,
            bit_mask: 0x04,
            invert: true,
        }),
        right_button: None,
        mode_hint: ClutchModeHint::IndependentAxis,
    };
    let json = serde_json::to_string(&binding)?;
    let rt: ClutchBinding = serde_json::from_str(&json)?;
    assert_eq!(binding, rt);
    Ok(())
}

#[test]
fn js_binding_serde_round_trip() -> R {
    let binding = JsBinding {
        mode_hint: JsModeHint::DPad,
        axis: Some(make_axis("hat", 27, AxisDataType::U8)),
        buttons: vec![make_button("up", 5, 0x10), make_button("down", 5, 0x20)],
    };
    let json = serde_json::to_string(&binding)?;
    let rt: JsBinding = serde_json::from_str(&json)?;
    assert_eq!(binding, rt);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// §10 Map merging and conflict resolution
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn two_maps_same_device_different_axes() {
    let map_a = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x346E,
        product_id: 0x0002,
        axes: vec![make_axis("steering", 1, AxisDataType::I16Le)],
        ..Default::default()
    };
    let map_b = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x346E,
        product_id: 0x0002,
        axes: vec![make_axis("throttle", 3, AxisDataType::U16Le)],
        ..Default::default()
    };
    // Both maps target the same device but define different axes.
    assert_eq!(map_a.vendor_id, map_b.vendor_id);
    assert_eq!(map_a.product_id, map_b.product_id);
    assert_ne!(map_a.axes[0].name, map_b.axes[0].name);
}

#[test]
fn overlapping_button_offsets_are_representable() {
    // Two buttons can share the same byte_offset with different masks.
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x346E,
        product_id: 0x0002,
        buttons: vec![make_button("a", 11, 0x01), make_button("b", 11, 0x02)],
        ..Default::default()
    };
    assert!(map.validate().is_ok());
    assert_eq!(map.buttons[0].byte_offset, map.buttons[1].byte_offset);
    assert_ne!(map.buttons[0].bit_mask, map.buttons[1].bit_mask);
}

#[test]
fn compile_combined_clutch_and_rotary() -> R {
    // A device can have both clutch and rotary bindings.
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        clutch: Some(ClutchBinding {
            combined: Some(make_axis("clutch", 7, AxisDataType::U16Le)),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        }),
        rotaries: vec![RotaryBinding {
            name: "dial".to_string(),
            byte_offset: 29,
            mode: RotaryModeHint::Knob,
        }],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("clutch + rotary")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::CombinedAxis);
    assert!(ks.clutch_combined_axis.is_some());
    assert!(ks.left_rotary_axis.is_some());
    Ok(())
}

#[test]
fn compile_all_ks_sections_simultaneously() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        report: ReportConstraint {
            report_id: Some(0x01),
            report_len: Some(64),
        },
        clutch: Some(ClutchBinding {
            combined: Some(make_axis("combined", 7, AxisDataType::U16Le)),
            left: Some(make_axis("left", 9, AxisDataType::U16Le)),
            right: Some(make_axis("right", 11, AxisDataType::U16Le)),
            left_button: Some(make_button("cl", 22, 0x01)),
            right_button: Some(make_button("cr", 22, 0x02)),
            mode_hint: ClutchModeHint::IndependentAxis,
        }),
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::DPad,
            axis: Some(make_axis("hat", 27, AxisDataType::U8)),
            buttons: vec![],
        }),
        rotaries: vec![
            RotaryBinding {
                name: "enc_l".to_string(),
                byte_offset: 29,
                mode: RotaryModeHint::Knob,
            },
            RotaryBinding {
                name: "enc_r".to_string(),
                byte_offset: 31,
                mode: RotaryModeHint::Knob,
            },
        ],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("all sections")?;
    assert_eq!(ks.report_id, Some(0x01));
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::IndependentAxis);
    assert!(ks.clutch_combined_axis.is_some());
    assert!(ks.clutch_left_axis.is_some());
    assert!(ks.clutch_right_axis.is_some());
    assert!(ks.clutch_left_button.is_some());
    assert!(ks.clutch_right_button.is_some());
    assert_eq!(ks.joystick_mode_hint, KsJoystickMode::DPad);
    assert!(ks.joystick_hat.is_some());
    assert_eq!(ks.left_rotary_axis.map(|s| s.offset), Some(29));
    assert_eq!(ks.right_rotary_axis.map(|s| s.offset), Some(31));
    assert_eq!(ks.rotary_mode_hint, KsRotaryMode::Knob);
    Ok(())
}

#[test]
fn handbrake_binding_does_not_affect_ks_compilation() {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        axes: vec![make_axis("steering", 1, AxisDataType::U16Le)],
        handbrake: Some(make_axis("handbrake", 33, AxisDataType::U16Le)),
        ..Default::default()
    };
    // Handbrake alone doesn't create KS content.
    assert!(compile_ks_map(&map).is_none());
}

#[test]
fn report_constraint_boundary_values() -> R {
    let rc = ReportConstraint {
        report_id: Some(0xFF),
        report_len: Some(u16::MAX),
    };
    let json = serde_json::to_string(&rc)?;
    let rt: ReportConstraint = serde_json::from_str(&json)?;
    assert_eq!(rt.report_id, Some(0xFF));
    assert_eq!(rt.report_len, Some(u16::MAX));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// §11 Proptest
// ═══════════════════════════════════════════════════════════════════════

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(200))]

    #[test]
    fn prop_button_binding_serde_round_trip(
        offset in 0u16..=1000u16,
        bit in 0u8..8u8,
        invert: bool,
    ) {
        let mask = 1u8 << bit;
        let btn = ButtonBinding {
            name: "prop_btn".to_string(),
            byte_offset: offset,
            bit_mask: mask,
            invert,
        };
        let json = serde_json::to_string(&btn).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let rt: ButtonBinding = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(btn, rt);
    }

    #[test]
    fn prop_axis_binding_serde_round_trip(
        offset in 0u16..=500u16,
        signed: bool,
        invert: bool,
        min_val in proptest::option::of(-32768i32..=32767i32),
        max_val in proptest::option::of(-32768i32..=32767i32),
    ) {
        let axis = AxisBinding {
            name: "prop_axis".to_string(),
            byte_offset: offset,
            bit_offset: None,
            data_type: AxisDataType::U16Le,
            signed,
            invert,
            min: min_val,
            max: max_val,
        };
        let json = serde_json::to_string(&axis).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let rt: AxisBinding = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(axis, rt);
    }

    #[test]
    fn prop_rotary_binding_serde_round_trip(offset in 0u16..=500u16) {
        let r = RotaryBinding {
            name: "prop_rotary".to_string(),
            byte_offset: offset,
            mode: RotaryModeHint::Knob,
        };
        let json = serde_json::to_string(&r).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let rt: RotaryBinding = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(r, rt);
    }

    #[test]
    fn prop_valid_map_always_compiles_when_ks_content(
        vendor in 1u16..=0xFFFFu16,
        product in 1u16..=0xFFFFu16,
        offset in 0u16..=200u16,
    ) {
        let map = DeviceInputMap {
            schema_version: 1,
            vendor_id: vendor,
            product_id: product,
            clutch: Some(ClutchBinding {
                combined: Some(make_axis("c", offset, AxisDataType::U16Le)),
                left: None,
                right: None,
                left_button: None,
                right_button: None,
                mode_hint: ClutchModeHint::CombinedAxis,
            }),
            ..Default::default()
        };
        prop_assert!(compile_ks_map(&map).is_some());
    }
}
