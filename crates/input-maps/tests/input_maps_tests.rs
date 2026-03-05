//! Comprehensive integration tests for the input-maps crate covering
//! map compilation, button binding resolution, axis mapping, multi-device
//! merging, error handling, and property-based tests.

use racing_wheel_input_maps::{
    AxisBinding, AxisDataType, ButtonBinding, ClutchBinding, ClutchModeHint, DeviceInputMap,
    DeviceInputMapError, DeviceTransportHint, InitFrameDirection, InitReportFrame, JsBinding,
    JsModeHint, ReportConstraint, RotaryBinding, RotaryModeHint, compile_ks_map,
};
use racing_wheel_ks::{KS_ENCODER_COUNT, KsClutchMode, KsJoystickMode, KsRotaryMode};

type R = Result<(), Box<dyn std::error::Error>>;

// ── Helpers ──────────────────────────────────────────────────────────────

fn axis(name: &str, offset: u16, dt: AxisDataType, signed: bool) -> AxisBinding {
    AxisBinding {
        name: name.to_string(),
        byte_offset: offset,
        bit_offset: None,
        data_type: dt,
        signed,
        invert: false,
        min: None,
        max: None,
    }
}

fn button(name: &str, offset: u16, mask: u8) -> ButtonBinding {
    ButtonBinding {
        name: name.to_string(),
        byte_offset: offset,
        bit_mask: mask,
        invert: false,
    }
}

fn rotary(name: &str, offset: u16, mode: RotaryModeHint) -> RotaryBinding {
    RotaryBinding {
        name: name.to_string(),
        byte_offset: offset,
        mode,
    }
}



// ── Map compilation correctness ──────────────────────────────────────────

#[test]
fn compile_ks_map_none_when_only_axes_and_buttons() {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        axes: vec![axis("steering", 1, AxisDataType::U16Le, false)],
        buttons: vec![button("a", 5, 0x01)],
        ..Default::default()
    };
    assert!(compile_ks_map(&map).is_none());
}

#[test]
fn compile_ks_map_some_when_clutch_present() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        clutch: Some(ClutchBinding {
            combined: Some(axis("clutch", 7, AxisDataType::U16Le, false)),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should produce KS map for clutch")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::CombinedAxis);
    assert_eq!(ks.clutch_combined_axis.map(|s| s.offset), Some(7));
    Ok(())
}

#[test]
fn compile_ks_map_some_when_rotaries_present() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        rotaries: vec![rotary("r1", 20, RotaryModeHint::Knob)],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should produce KS map for rotaries")?;
    assert_eq!(ks.rotary_mode_hint, KsRotaryMode::Knob);
    Ok(())
}

#[test]
fn compile_ks_map_some_when_joystick_present() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::DPad,
            axis: Some(axis("hat", 27, AxisDataType::U8, false)),
            buttons: vec![],
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should produce KS map for joystick")?;
    assert_eq!(ks.joystick_mode_hint, KsJoystickMode::DPad);
    assert_eq!(ks.joystick_hat.map(|s| s.offset), Some(27));
    Ok(())
}

#[test]
fn compile_ks_map_clutch_left_right_buttons_wired() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        clutch: Some(ClutchBinding {
            combined: None,
            left: None,
            right: None,
            left_button: Some(button("lb", 10, 0x01)),
            right_button: Some(button("rb", 10, 0x02)),
            mode_hint: ClutchModeHint::Button,
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile clutch buttons")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::Button);
    let lb = ks.clutch_left_button.ok_or("left button missing")?;
    let rb = ks.clutch_right_button.ok_or("right button missing")?;
    assert_eq!(lb.offset, 10);
    assert_eq!(lb.mask, 0x01);
    assert_eq!(rb.offset, 10);
    assert_eq!(rb.mask, 0x02);
    Ok(())
}

#[test]
fn compile_ks_map_clutch_left_right_axes_wired() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        clutch: Some(ClutchBinding {
            combined: None,
            left: Some(axis("cl", 14, AxisDataType::U16Le, false)),
            right: Some(axis("cr", 16, AxisDataType::I16Le, true)),
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::IndependentAxis,
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile independent clutch")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::IndependentAxis);
    let la = ks.clutch_left_axis.ok_or("left axis missing")?;
    let ra = ks.clutch_right_axis.ok_or("right axis missing")?;
    assert_eq!(la.offset, 14);
    assert!(!la.signed);
    assert_eq!(ra.offset, 16);
    assert!(ra.signed);
    Ok(())
}

// ── Button binding resolution ────────────────────────────────────────────

#[test]
fn button_binding_inverted_flag_preserved_in_serde() -> R {
    let btn = ButtonBinding {
        name: "shift_up".to_string(),
        byte_offset: 11,
        bit_mask: 0x80,
        invert: true,
    };
    let json = serde_json::to_string(&btn)?;
    let rt: ButtonBinding = serde_json::from_str(&json)?;
    assert!(rt.invert);
    assert_eq!(rt.bit_mask, 0x80);
    Ok(())
}

#[test]
fn multiple_buttons_same_byte_different_masks_all_preserved() -> R {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x1234,
        product_id: 0x5678,
        buttons: vec![
            button("a", 5, 0x01),
            button("b", 5, 0x02),
            button("c", 5, 0x04),
            button("x", 5, 0x08),
            button("y", 5, 0x10),
            button("lb", 5, 0x20),
            button("rb", 5, 0x40),
            button("start", 5, 0x80),
        ],
        ..Default::default()
    };
    let json = serde_json::to_string(&map)?;
    let rt: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(rt.buttons.len(), 8);
    for (i, btn) in rt.buttons.iter().enumerate() {
        assert_eq!(btn.bit_mask, 1u8 << i);
        assert_eq!(btn.byte_offset, 5);
    }
    Ok(())
}

#[test]
fn button_binding_zero_mask_is_representable() -> R {
    let btn = button("edge_case", 0, 0x00);
    let json = serde_json::to_string(&btn)?;
    let rt: ButtonBinding = serde_json::from_str(&json)?;
    assert_eq!(rt.bit_mask, 0x00);
    Ok(())
}

// ── Axis mapping and scaling ─────────────────────────────────────────────

#[test]
fn axis_signed_types_produce_signed_ks_source() -> R {
    let signed_types = [AxisDataType::I8, AxisDataType::I16Le, AxisDataType::I16Be];
    for dt in signed_types {
        let map = DeviceInputMap {
            vendor_id: 0x1234,
            product_id: 0x5678,
            clutch: Some(ClutchBinding {
                combined: Some(axis("test", 5, dt, true)),
                left: None,
                right: None,
                left_button: None,
                right_button: None,
                mode_hint: ClutchModeHint::CombinedAxis,
            }),
            ..Default::default()
        };
        let ks = compile_ks_map(&map).ok_or("should compile signed axis")?;
        let src = ks.clutch_combined_axis.ok_or("combined axis missing")?;
        assert!(src.signed, "signed data type should produce signed source");
    }
    Ok(())
}

#[test]
fn axis_unsigned_types_produce_unsigned_ks_source() -> R {
    let unsigned_types = [
        AxisDataType::U8,
        AxisDataType::U16Le,
        AxisDataType::U16Be,
        AxisDataType::Bool,
    ];
    for dt in unsigned_types {
        let map = DeviceInputMap {
            vendor_id: 0x1234,
            product_id: 0x5678,
            clutch: Some(ClutchBinding {
                combined: Some(axis("test", 5, dt, false)),
                left: None,
                right: None,
                left_button: None,
                right_button: None,
                mode_hint: ClutchModeHint::CombinedAxis,
            }),
            ..Default::default()
        };
        let ks = compile_ks_map(&map).ok_or("should compile unsigned axis")?;
        let src = ks.clutch_combined_axis.ok_or("combined axis missing")?;
        assert!(
            !src.signed,
            "unsigned data type should produce unsigned source"
        );
    }
    Ok(())
}

#[test]
fn axis_with_min_max_range_round_trips() -> R {
    let a = AxisBinding {
        name: "throttle".to_string(),
        byte_offset: 3,
        bit_offset: Some(4),
        data_type: AxisDataType::I16Le,
        signed: true,
        invert: true,
        min: Some(-32768),
        max: Some(32767),
    };
    let json = serde_json::to_string(&a)?;
    let rt: AxisBinding = serde_json::from_str(&json)?;
    assert_eq!(rt.min, Some(-32768));
    assert_eq!(rt.max, Some(32767));
    assert!(rt.invert);
    assert_eq!(rt.bit_offset, Some(4));
    Ok(())
}

#[test]
fn axis_data_type_all_variants_serde_round_trip() -> R {
    let variants = [
        AxisDataType::U8,
        AxisDataType::I8,
        AxisDataType::U16Le,
        AxisDataType::I16Le,
        AxisDataType::U16Be,
        AxisDataType::I16Be,
        AxisDataType::Bool,
    ];
    for v in variants {
        let json = serde_json::to_string(&v)?;
        let rt: AxisDataType = serde_json::from_str(&json)?;
        assert_eq!(v, rt);
    }
    Ok(())
}

// ── Rotary → encoder array mapping ──────────────────────────────────────

#[test]
fn compile_rotary_first_slot_maps_to_left_axis() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        rotaries: vec![rotary("r0", 29, RotaryModeHint::Button)],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile single rotary")?;
    let left = ks.left_rotary_axis.ok_or("left rotary missing")?;
    assert_eq!(left.offset, 29);
    assert!(!left.signed);
    assert_eq!(ks.encoders[0].map(|s| s.offset), Some(29));
    assert!(ks.right_rotary_axis.is_none());
    Ok(())
}

#[test]
fn compile_rotary_two_slots_map_left_and_right() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        rotaries: vec![
            rotary("r0", 29, RotaryModeHint::Button),
            rotary("r1", 31, RotaryModeHint::Button),
        ],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile two rotaries")?;
    let left = ks.left_rotary_axis.ok_or("left rotary missing")?;
    let right = ks.right_rotary_axis.ok_or("right rotary missing")?;
    assert_eq!(left.offset, 29);
    assert_eq!(right.offset, 31);
    assert_eq!(ks.encoders[0].map(|s| s.offset), Some(29));
    assert_eq!(ks.encoders[1].map(|s| s.offset), Some(31));
    Ok(())
}

#[test]
fn compile_rotary_fills_up_to_encoder_count() -> R {
    let rotaries: Vec<RotaryBinding> = (0..KS_ENCODER_COUNT as u16)
        .map(|i| rotary(&format!("r{i}"), 10 + i * 2, RotaryModeHint::Knob))
        .collect();
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        rotaries,
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile max rotaries")?;
    for i in 0..KS_ENCODER_COUNT {
        let src = ks.encoders[i].ok_or("encoder slot should be populated")?;
        assert_eq!(src.offset, 10 + i * 2);
    }
    Ok(())
}

#[test]
fn compile_rotary_excess_beyond_encoder_count_truncated() -> R {
    let rotaries: Vec<RotaryBinding> = (0..KS_ENCODER_COUNT as u16 + 4)
        .map(|i| rotary(&format!("r{i}"), 10 + i * 2, RotaryModeHint::Knob))
        .collect();
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        rotaries,
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile excess rotaries")?;
    // Only first KS_ENCODER_COUNT are mapped
    for i in 0..KS_ENCODER_COUNT {
        assert!(ks.encoders[i].is_some());
    }
    Ok(())
}

#[test]
fn compile_rotary_third_slot_not_mapped_to_left_right() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        rotaries: vec![
            rotary("r0", 10, RotaryModeHint::Knob),
            rotary("r1", 12, RotaryModeHint::Knob),
            rotary("r2", 14, RotaryModeHint::Knob),
        ],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile three rotaries")?;
    // Third rotary goes to encoders[2] but not to left/right rotary axis
    assert!(ks.encoders[2].is_some());
    assert_eq!(ks.left_rotary_axis.map(|s| s.offset), Some(10));
    assert_eq!(ks.right_rotary_axis.map(|s| s.offset), Some(12));
    Ok(())
}

#[test]
fn compile_rotary_mode_last_wins() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        rotaries: vec![
            rotary("r0", 10, RotaryModeHint::Button),
            rotary("r1", 12, RotaryModeHint::Knob),
        ],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile mixed rotary modes")?;
    // The loop overwrites rotary_mode_hint each iteration, so last wins
    assert_eq!(ks.rotary_mode_hint, KsRotaryMode::Knob);
    Ok(())
}

// ── Multi-device map merging ─────────────────────────────────────────────

#[test]
fn two_independent_maps_compile_to_independent_ks_maps() -> R {
    let map_a = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x0001,
        rotaries: vec![rotary("r0", 20, RotaryModeHint::Button)],
        ..Default::default()
    };
    let map_b = DeviceInputMap {
        vendor_id: 0x5678,
        product_id: 0x0002,
        clutch: Some(ClutchBinding {
            combined: Some(axis("c", 7, AxisDataType::U16Le, false)),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        }),
        ..Default::default()
    };
    let ks_a = compile_ks_map(&map_a).ok_or("map_a should compile")?;
    let ks_b = compile_ks_map(&map_b).ok_or("map_b should compile")?;

    // map_a has rotary, no clutch
    assert!(ks_a.encoders[0].is_some());
    assert!(ks_a.clutch_combined_axis.is_none());

    // map_b has clutch, no rotary
    assert!(ks_b.clutch_combined_axis.is_some());
    assert!(ks_b.encoders[0].is_none());
    Ok(())
}

#[test]
fn maps_with_same_device_ids_compile_independently() -> R {
    let map_a = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        rotaries: vec![rotary("r0", 20, RotaryModeHint::Button)],
        ..Default::default()
    };
    let map_b = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::Buttons,
            axis: None,
            buttons: vec![button("up", 3, 0x01)],
        }),
        ..Default::default()
    };
    let ks_a = compile_ks_map(&map_a).ok_or("map_a should compile")?;
    let ks_b = compile_ks_map(&map_b).ok_or("map_b should compile")?;
    assert_eq!(ks_a.rotary_mode_hint, KsRotaryMode::Button);
    assert_eq!(ks_b.joystick_mode_hint, KsJoystickMode::Button);
    Ok(())
}

#[test]
fn compile_all_ks_sections_simultaneously() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        report: ReportConstraint {
            report_id: Some(0x03),
            report_len: Some(64),
        },
        clutch: Some(ClutchBinding {
            combined: Some(axis("cc", 7, AxisDataType::U16Le, false)),
            left: Some(axis("cl", 9, AxisDataType::U16Le, false)),
            right: Some(axis("cr", 11, AxisDataType::I16Le, true)),
            left_button: Some(button("clb", 13, 0x01)),
            right_button: Some(button("crb", 13, 0x02)),
            mode_hint: ClutchModeHint::IndependentAxis,
        }),
        rotaries: vec![
            rotary("r0", 15, RotaryModeHint::Knob),
            rotary("r1", 17, RotaryModeHint::Knob),
        ],
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::DPad,
            axis: Some(axis("hat", 19, AxisDataType::U8, false)),
            buttons: vec![],
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile full map")?;

    assert_eq!(ks.report_id, Some(0x03));
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::IndependentAxis);
    assert_eq!(ks.clutch_combined_axis.map(|s| s.offset), Some(7));
    assert_eq!(ks.clutch_left_axis.map(|s| s.offset), Some(9));
    assert_eq!(ks.clutch_right_axis.map(|s| s.offset), Some(11));
    assert_eq!(ks.clutch_left_button.map(|b| b.mask), Some(0x01));
    assert_eq!(ks.clutch_right_button.map(|b| b.mask), Some(0x02));
    assert_eq!(ks.rotary_mode_hint, KsRotaryMode::Knob);
    assert_eq!(ks.left_rotary_axis.map(|s| s.offset), Some(15));
    assert_eq!(ks.right_rotary_axis.map(|s| s.offset), Some(17));
    assert_eq!(ks.joystick_mode_hint, KsJoystickMode::DPad);
    assert_eq!(ks.joystick_hat.map(|s| s.offset), Some(19));
    Ok(())
}

// ── Error handling for invalid configurations ────────────────────────────

#[test]
fn validate_rejects_schema_version_zero() {
    let map = DeviceInputMap {
        schema_version: 0,
        vendor_id: 0x1234,
        product_id: 0x5678,
        axes: vec![axis("a", 1, AxisDataType::U8, false)],
        ..Default::default()
    };
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::UnsupportedSchemaVersion(0))
    ));
}

#[test]
fn validate_rejects_no_inputs() {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x1234,
        product_id: 0x5678,
        ..Default::default()
    };
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::NoInputsDefined)
    ));
}

#[test]
fn validate_rejects_zero_vendor_id() {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0,
        product_id: 0x5678,
        axes: vec![axis("a", 1, AxisDataType::U8, false)],
        ..Default::default()
    };
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::MissingIdentity)
    ));
}

#[test]
fn validate_rejects_zero_product_id() {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x1234,
        product_id: 0,
        axes: vec![axis("a", 1, AxisDataType::U8, false)],
        ..Default::default()
    };
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::MissingIdentity)
    ));
}

#[test]
fn validate_rejects_both_ids_zero() {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0,
        product_id: 0,
        axes: vec![axis("a", 1, AxisDataType::U8, false)],
        ..Default::default()
    };
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::MissingIdentity)
    ));
}

#[test]
fn validate_accepts_buttons_only_map() {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x1234,
        product_id: 0x5678,
        buttons: vec![button("a", 5, 0x01)],
        ..Default::default()
    };
    assert!(map.validate().is_ok());
}

#[test]
fn validate_accepts_rotaries_only_map() {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x1234,
        product_id: 0x5678,
        rotaries: vec![rotary("r0", 10, RotaryModeHint::Knob)],
        ..Default::default()
    };
    assert!(map.validate().is_ok());
}

#[test]
fn validate_accepts_clutch_only_map() {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x1234,
        product_id: 0x5678,
        clutch: Some(ClutchBinding {
            combined: Some(axis("c", 7, AxisDataType::U16Le, false)),
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
fn validate_rejects_schema_version_255_no_inputs() {
    let map = DeviceInputMap {
        schema_version: 255,
        vendor_id: 0x1234,
        product_id: 0x5678,
        ..Default::default()
    };
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::NoInputsDefined)
    ));
}

#[test]
fn serde_rejects_unknown_fields() {
    let json = r#"{
        "schema_version": 1,
        "vendor_id": 1,
        "product_id": 2,
        "unknown_field": 42
    }"#;
    let result: Result<DeviceInputMap, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn serde_defaults_populate_omitted_optional_fields() -> R {
    let json = r#"{
        "schema_version": 1,
        "vendor_id": 1234,
        "product_id": 5678
    }"#;
    let map: DeviceInputMap = serde_json::from_str(json)?;
    assert!(map.axes.is_empty());
    assert!(map.buttons.is_empty());
    assert!(map.rotaries.is_empty());
    assert!(map.clutch.is_none());
    assert!(map.joystick.is_none());
    assert!(map.handbrake.is_none());
    assert!(map.mode_hints.is_none());
    assert!(map.init_sequence.is_empty());
    assert_eq!(map.transport, DeviceTransportHint::Unknown);
    Ok(())
}

// ── Init sequence round-trip ─────────────────────────────────────────────

#[test]
fn init_sequence_serde_round_trip() -> R {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x1234,
        product_id: 0x5678,
        axes: vec![axis("a", 1, AxisDataType::U8, false)],
        init_sequence: vec![
            InitReportFrame {
                report_id: 0x01,
                payload: vec![0xAA, 0xBB],
                direction: InitFrameDirection::Out,
            },
            InitReportFrame {
                report_id: 0x02,
                payload: vec![0xCC],
                direction: InitFrameDirection::In,
            },
        ],
        ..Default::default()
    };
    let json = serde_json::to_string(&map)?;
    let rt: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(rt.init_sequence.len(), 2);
    assert_eq!(rt.init_sequence[0].report_id, 0x01);
    assert_eq!(rt.init_sequence[1].direction, InitFrameDirection::In);
    Ok(())
}

// ── Transport hint round-trip ────────────────────────────────────────────

#[test]
fn all_transport_hints_serde_round_trip() -> R {
    let variants = [
        DeviceTransportHint::WheelbaseAggregated,
        DeviceTransportHint::StandaloneUsb,
        DeviceTransportHint::UniversalHub,
        DeviceTransportHint::Unknown,
    ];
    for v in variants {
        let json = serde_json::to_string(&v)?;
        let rt: DeviceTransportHint = serde_json::from_str(&json)?;
        assert_eq!(v, rt);
    }
    Ok(())
}

// ── Full map JSON round-trip ─────────────────────────────────────────────

#[test]
fn full_map_json_round_trip() -> R {
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
            axis("steering", 1, AxisDataType::I16Le, true),
            axis("throttle", 3, AxisDataType::U16Le, false),
            axis("brake", 5, AxisDataType::U16Le, false),
        ],
        buttons: vec![
            button("paddle_left", 11, 0x01),
            button("paddle_right", 11, 0x02),
        ],
        clutch: Some(ClutchBinding {
            combined: None,
            left: Some(axis("cl", 18, AxisDataType::U16Le, false)),
            right: Some(axis("cr", 20, AxisDataType::U16Le, false)),
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::IndependentAxis,
        }),
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::DPad,
            axis: Some(axis("hat", 27, AxisDataType::U8, false)),
            buttons: vec![],
        }),
        rotaries: vec![
            rotary("r0", 29, RotaryModeHint::Button),
            rotary("r1", 30, RotaryModeHint::Button),
        ],
        handbrake: Some(axis("handbrake", 31, AxisDataType::U16Le, false)),
        mode_hints: None,
        init_sequence: vec![],
    };
    let json = serde_json::to_string(&map)?;
    let rt: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(map, rt);
    Ok(())
}

// ── Report constraint propagation ────────────────────────────────────────

#[test]
fn compile_propagates_report_id_and_ignores_len() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        report: ReportConstraint {
            report_id: Some(0x42),
            report_len: Some(128),
        },
        rotaries: vec![rotary("r", 10, RotaryModeHint::Knob)],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile")?;
    assert_eq!(ks.report_id, Some(0x42));
    Ok(())
}

#[test]
fn compile_none_report_id_when_not_set() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        rotaries: vec![rotary("r", 10, RotaryModeHint::Knob)],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile")?;
    assert_eq!(ks.report_id, None);
    Ok(())
}

// ── Joystick mode variants ──────────────────────────────────────────────

#[test]
fn compile_joystick_buttons_mode_no_hat() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::Buttons,
            axis: None,
            buttons: vec![button("up", 3, 0x01)],
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile joystick buttons")?;
    assert_eq!(ks.joystick_mode_hint, KsJoystickMode::Button);
    assert!(ks.joystick_hat.is_none());
    Ok(())
}

#[test]
fn compile_joystick_unknown_mode() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::Unknown,
            axis: None,
            buttons: vec![],
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile unknown joystick")?;
    assert_eq!(ks.joystick_mode_hint, KsJoystickMode::Unknown);
    Ok(())
}

// ── Clutch mode variants ────────────────────────────────────────────────

#[test]
fn compile_clutch_unknown_mode() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
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
    let ks = compile_ks_map(&map).ok_or("should compile unknown clutch")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::Unknown);
    Ok(())
}

// ── Default map invariants ──────────────────────────────────────────────

#[test]
fn default_map_has_expected_defaults() {
    let map = DeviceInputMap::default();
    assert_eq!(map.schema_version, 1);
    assert_eq!(map.vendor_id, 0);
    assert_eq!(map.product_id, 0);
    assert_eq!(map.transport, DeviceTransportHint::Unknown);
    assert!(map.axes.is_empty());
    assert!(map.buttons.is_empty());
    assert!(map.rotaries.is_empty());
    assert!(map.clutch.is_none());
    assert!(map.joystick.is_none());
    assert!(map.handbrake.is_none());
    assert!(map.mode_hints.is_none());
    assert!(map.init_sequence.is_empty());
}

#[test]
fn default_map_is_not_valid() {
    let map = DeviceInputMap::default();
    assert!(map.validate().is_err());
}

#[test]
fn default_map_compiles_to_none() {
    let map = DeviceInputMap::default();
    assert!(compile_ks_map(&map).is_none());
}

// ── Handbrake binding does not affect KS compilation ─────────────────────

#[test]
fn handbrake_binding_does_not_affect_ks() {
    let map = DeviceInputMap {
        vendor_id: 0x1234,
        product_id: 0x5678,
        handbrake: Some(axis("hb", 33, AxisDataType::U16Le, false)),
        ..Default::default()
    };
    assert!(compile_ks_map(&map).is_none());
}

// ── Proptest ─────────────────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(200))]

    #[test]
    fn prop_valid_map_serde_round_trips(
        vendor_id in 1u16..=0xFFFFu16,
        product_id in 1u16..=0xFFFFu16,
        byte_offset in 0u16..=500u16,
        schema_version in 1u8..=10u8,
    ) {
        let map = DeviceInputMap {
            schema_version,
            vendor_id,
            product_id,
            axes: vec![axis("test", byte_offset, AxisDataType::U16Le, false)],
            ..Default::default()
        };
        let json = serde_json::to_string(&map).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let rt: DeviceInputMap = serde_json::from_str(&json).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        prop_assert_eq!(map, rt);
    }

    #[test]
    fn prop_rotary_count_determines_encoder_fill(
        count in 1usize..=KS_ENCODER_COUNT,
    ) {
        let rotaries: Vec<RotaryBinding> = (0..count as u16)
            .map(|i| rotary(&format!("r{i}"), 10 + i * 2, RotaryModeHint::Knob))
            .collect();
        let map = DeviceInputMap {
            vendor_id: 0x1234,
            product_id: 0x5678,
            rotaries,
            ..Default::default()
        };
        let ks = compile_ks_map(&map).ok_or_else(|| TestCaseError::Fail("should compile".into()))?;
        for i in 0..count {
            prop_assert!(ks.encoders[i].is_some());
        }
        for i in count..KS_ENCODER_COUNT {
            prop_assert!(ks.encoders[i].is_none());
        }
        if count >= 1 {
            prop_assert!(ks.left_rotary_axis.is_some());
        }
        if count >= 2 {
            prop_assert!(ks.right_rotary_axis.is_some());
        }
    }

    #[test]
    fn prop_compile_preserves_rotary_offsets(
        offsets in proptest::collection::vec(0u16..=500u16, 1..=KS_ENCODER_COUNT),
    ) {
        let rotaries: Vec<RotaryBinding> = offsets
            .iter()
            .enumerate()
            .map(|(i, &off)| rotary(&format!("r{i}"), off, RotaryModeHint::Button))
            .collect();
        let map = DeviceInputMap {
            vendor_id: 0x1234,
            product_id: 0x5678,
            rotaries,
            ..Default::default()
        };
        let ks = compile_ks_map(&map).ok_or_else(|| TestCaseError::Fail("should compile".into()))?;
        for (i, &expected_offset) in offsets.iter().enumerate() {
            let src = ks.encoders[i].ok_or_else(|| TestCaseError::Fail(format!("encoder {i} missing").into()))?;
            prop_assert_eq!(src.offset, expected_offset as usize);
        }
    }

    #[test]
    fn prop_validation_succeeds_with_valid_ids_and_input(
        vendor in 1u16..=0xFFFFu16,
        product in 1u16..=0xFFFFu16,
        sv in 1u8..=255u8,
    ) {
        let map = DeviceInputMap {
            schema_version: sv,
            vendor_id: vendor,
            product_id: product,
            axes: vec![axis("a", 1, AxisDataType::U8, false)],
            ..Default::default()
        };
        prop_assert!(map.validate().is_ok());
    }
}
