//! Deep tests for input-maps crate: button mapping compilation, axis mapping,
//! rotary encoder mapping, multi-device composite mapping, conflict detection,
//! and import/export.

use racing_wheel_input_maps::{
    AxisBinding, AxisDataType, ButtonBinding, ClutchBinding, ClutchModeHint, DeviceInputMap,
    DeviceInputMapError, DeviceMapModeHints, DeviceTransportHint, InitFrameDirection,
    InitReportFrame, JsBinding, JsModeHint, ReportConstraint, RotaryBinding, RotaryModeHint,
    compile_ks_map,
};
use racing_wheel_ks::{KsClutchMode, KsJoystickMode, KsRotaryMode};

type R = Result<(), Box<dyn std::error::Error>>;

// ── Helpers ──────────────────────────────────────────────────────────────────

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

fn base_map() -> DeviceInputMap {
    DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x346E,
        product_id: 0x0022,
        ..Default::default()
    }
}

// ── 1. Button mapping compilation for all device types ──────────────────────

#[test]
fn compile_button_map_standalone_usb_device() -> R {
    let mut map = base_map();
    map.transport = DeviceTransportHint::StandaloneUsb;
    map.buttons = vec![button("paddle_l", 11, 0x01), button("paddle_r", 11, 0x02)];
    map.clutch = Some(ClutchBinding {
        combined: None,
        left: None,
        right: None,
        left_button: Some(button("cl", 12, 0x04)),
        right_button: Some(button("cr", 12, 0x08)),
        mode_hint: ClutchModeHint::Button,
    });

    let ks = compile_ks_map(&map).ok_or("expected ks map for standalone usb")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::Button);
    assert_eq!(ks.clutch_left_button.map(|b| b.offset), Some(12));
    assert_eq!(ks.clutch_right_button.map(|b| b.offset), Some(12));
    Ok(())
}

#[test]
fn compile_button_map_wheelbase_aggregated_device() -> R {
    let mut map = base_map();
    map.transport = DeviceTransportHint::WheelbaseAggregated;
    map.buttons = vec![button("shift_up", 5, 0x10), button("shift_down", 5, 0x20)];
    map.rotaries = vec![rotary("mode_dial", 20, RotaryModeHint::Knob)];

    let ks = compile_ks_map(&map).ok_or("expected ks map for wheelbase")?;
    assert_eq!(ks.rotary_mode_hint, KsRotaryMode::Knob);
    assert!(ks.encoders[0].is_some());
    Ok(())
}

#[test]
fn compile_button_map_universal_hub_device() -> R {
    let mut map = base_map();
    map.transport = DeviceTransportHint::UniversalHub;
    map.clutch = Some(ClutchBinding {
        combined: Some(axis("clutch", 7, AxisDataType::U16Le, false)),
        left: None,
        right: None,
        left_button: None,
        right_button: None,
        mode_hint: ClutchModeHint::CombinedAxis,
    });

    let ks = compile_ks_map(&map).ok_or("expected ks map for hub")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::CombinedAxis);
    Ok(())
}

#[test]
fn compile_button_map_unknown_transport_still_works() -> R {
    let mut map = base_map();
    map.transport = DeviceTransportHint::Unknown;
    map.rotaries = vec![rotary("enc", 10, RotaryModeHint::Button)];

    let ks = compile_ks_map(&map).ok_or("expected ks map for unknown transport")?;
    assert_eq!(ks.rotary_mode_hint, KsRotaryMode::Button);
    Ok(())
}

// ── 2. Axis mapping with dead zones and curves ──────────────────────────────

#[test]
fn axis_binding_u16le_produces_unsigned_source() -> R {
    let map = DeviceInputMap {
        clutch: Some(ClutchBinding {
            combined: Some(axis("throttle", 3, AxisDataType::U16Le, false)),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        }),
        ..base_map()
    };

    let ks = compile_ks_map(&map).ok_or("expected ks map")?;
    let src = ks.clutch_combined_axis.ok_or("expected axis source")?;
    assert!(!src.signed);
    assert_eq!(src.offset, 3);
    Ok(())
}

#[test]
fn axis_binding_i16le_produces_signed_source() -> R {
    let map = DeviceInputMap {
        clutch: Some(ClutchBinding {
            combined: Some(axis("steering", 1, AxisDataType::I16Le, true)),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        }),
        ..base_map()
    };

    let ks = compile_ks_map(&map).ok_or("expected ks map")?;
    let src = ks.clutch_combined_axis.ok_or("expected axis source")?;
    assert!(src.signed);
    Ok(())
}

#[test]
fn axis_binding_i8_produces_signed_source() -> R {
    let map = DeviceInputMap {
        clutch: Some(ClutchBinding {
            combined: Some(axis("small_axis", 5, AxisDataType::I8, true)),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        }),
        ..base_map()
    };

    let ks = compile_ks_map(&map).ok_or("expected ks map")?;
    let src = ks.clutch_combined_axis.ok_or("expected axis source")?;
    assert!(src.signed);
    Ok(())
}

#[test]
fn axis_binding_with_min_max_range_validates() -> R {
    let mut map = base_map();
    map.axes.push(AxisBinding {
        name: "brake".to_string(),
        byte_offset: 5,
        bit_offset: None,
        data_type: AxisDataType::U16Le,
        signed: false,
        invert: false,
        min: Some(0),
        max: Some(65535),
    });
    assert!(map.validate().is_ok());
    Ok(())
}

#[test]
fn axis_binding_inverted_flag_preserved_in_serde() -> R {
    let binding = AxisBinding {
        name: "pedal".to_string(),
        byte_offset: 4,
        bit_offset: Some(0),
        data_type: AxisDataType::U16Le,
        signed: false,
        invert: true,
        min: Some(0),
        max: Some(1023),
    };

    let json = serde_json::to_string(&binding)?;
    let round_tripped: AxisBinding = serde_json::from_str(&json)?;
    assert!(round_tripped.invert);
    assert_eq!(round_tripped.bit_offset, Some(0));
    Ok(())
}

#[test]
fn axis_data_type_all_variants_round_trip_serde() -> R {
    let variants = [
        AxisDataType::U8,
        AxisDataType::I8,
        AxisDataType::U16Le,
        AxisDataType::I16Le,
        AxisDataType::U16Be,
        AxisDataType::I16Be,
        AxisDataType::Bool,
    ];
    for dt in variants {
        let json = serde_json::to_string(&dt)?;
        let round_tripped: AxisDataType = serde_json::from_str(&json)?;
        assert_eq!(dt, round_tripped);
    }
    Ok(())
}

// ── 3. Rotary encoder mapping ───────────────────────────────────────────────

#[test]
fn compile_single_rotary_binds_left_slot() -> R {
    let map = DeviceInputMap {
        rotaries: vec![rotary("left_enc", 20, RotaryModeHint::Button)],
        ..base_map()
    };

    let ks = compile_ks_map(&map).ok_or("expected ks map")?;
    assert_eq!(ks.left_rotary_axis.map(|s| s.offset), Some(20));
    assert!(ks.right_rotary_axis.is_none());
    assert_eq!(ks.encoders[0].map(|s| s.offset), Some(20));
    Ok(())
}

#[test]
fn compile_two_rotaries_binds_both_slots() -> R {
    let map = DeviceInputMap {
        rotaries: vec![
            rotary("left_enc", 20, RotaryModeHint::Button),
            rotary("right_enc", 22, RotaryModeHint::Knob),
        ],
        ..base_map()
    };

    let ks = compile_ks_map(&map).ok_or("expected ks map")?;
    assert_eq!(ks.left_rotary_axis.map(|s| s.offset), Some(20));
    assert_eq!(ks.right_rotary_axis.map(|s| s.offset), Some(22));
    Ok(())
}

#[test]
fn compile_eight_rotaries_fills_all_encoder_slots() -> R {
    let mut map = base_map();
    for i in 0..8 {
        map.rotaries.push(rotary(
            &format!("enc_{i}"),
            (10 + i * 2) as u16,
            RotaryModeHint::Knob,
        ));
    }

    let ks = compile_ks_map(&map).ok_or("expected ks map")?;
    for i in 0..8 {
        let expected_offset = 10 + i * 2;
        assert_eq!(ks.encoders[i].map(|s| s.offset), Some(expected_offset));
    }
    Ok(())
}

#[test]
fn compile_rotary_mode_button_propagates() -> R {
    let map = DeviceInputMap {
        rotaries: vec![rotary("dial", 15, RotaryModeHint::Button)],
        ..base_map()
    };

    let ks = compile_ks_map(&map).ok_or("expected ks map")?;
    assert_eq!(ks.rotary_mode_hint, KsRotaryMode::Button);
    Ok(())
}

#[test]
fn compile_rotary_mode_unknown_propagates() -> R {
    let map = DeviceInputMap {
        rotaries: vec![rotary("dial", 15, RotaryModeHint::Unknown)],
        ..base_map()
    };

    let ks = compile_ks_map(&map).ok_or("expected ks map")?;
    assert_eq!(ks.rotary_mode_hint, KsRotaryMode::Unknown);
    Ok(())
}

// ── 4. LED feedback mapping (via mode_hints and init_sequence) ──────────────

#[test]
fn init_sequence_out_frame_preserved() -> R {
    let map = DeviceInputMap {
        axes: vec![axis("steer", 1, AxisDataType::U16Le, false)],
        init_sequence: vec![InitReportFrame {
            report_id: 0x09,
            payload: vec![0x01, 0x02, 0xFF],
            direction: InitFrameDirection::Out,
        }],
        ..base_map()
    };

    let json = serde_json::to_string(&map)?;
    let rt: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(rt.init_sequence.len(), 1);
    assert_eq!(rt.init_sequence[0].report_id, 0x09);
    assert_eq!(rt.init_sequence[0].direction, InitFrameDirection::Out);
    Ok(())
}

#[test]
fn init_sequence_in_frame_preserved() -> R {
    let map = DeviceInputMap {
        axes: vec![axis("steer", 1, AxisDataType::U16Le, false)],
        init_sequence: vec![InitReportFrame {
            report_id: 0x0A,
            payload: vec![0xAA, 0xBB],
            direction: InitFrameDirection::In,
        }],
        ..base_map()
    };

    let json = serde_json::to_string(&map)?;
    let rt: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(rt.init_sequence[0].direction, InitFrameDirection::In);
    Ok(())
}

#[test]
fn init_sequence_multiple_frames_round_trip() -> R {
    let map = DeviceInputMap {
        axes: vec![axis("steer", 1, AxisDataType::U16Le, false)],
        init_sequence: vec![
            InitReportFrame {
                report_id: 0x09,
                payload: vec![0x01],
                direction: InitFrameDirection::Out,
            },
            InitReportFrame {
                report_id: 0x09,
                payload: vec![0x02],
                direction: InitFrameDirection::In,
            },
            InitReportFrame {
                report_id: 0x0A,
                payload: vec![0x03, 0x04, 0x05],
                direction: InitFrameDirection::Out,
            },
        ],
        ..base_map()
    };

    let json = serde_json::to_string(&map)?;
    let rt: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(rt.init_sequence.len(), 3);
    assert_eq!(rt.init_sequence[2].payload, vec![0x03, 0x04, 0x05]);
    Ok(())
}

// ── 5. Multi-device composite mapping ───────────────────────────────────────

#[test]
fn compile_full_composite_map_with_all_bindings() -> R {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x346E,
        product_id: 0x0022,
        transport: DeviceTransportHint::WheelbaseAggregated,
        report: ReportConstraint {
            report_id: Some(0x01),
            report_len: Some(64),
        },
        axes: vec![
            axis("steering", 1, AxisDataType::I16Le, true),
            axis("throttle", 3, AxisDataType::U16Le, false),
            axis("brake", 5, AxisDataType::U16Le, false),
        ],
        buttons: vec![
            button("paddle_l", 11, 0x01),
            button("paddle_r", 11, 0x02),
            button("shift_up", 11, 0x04),
            button("shift_down", 11, 0x08),
        ],
        clutch: Some(ClutchBinding {
            combined: None,
            left: Some(axis("clutch_l", 18, AxisDataType::U16Le, false)),
            right: Some(axis("clutch_r", 20, AxisDataType::U16Le, false)),
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
            rotary("left_dial", 29, RotaryModeHint::Knob),
            rotary("right_dial", 31, RotaryModeHint::Button),
        ],
        handbrake: Some(axis("handbrake", 33, AxisDataType::U16Le, false)),
        mode_hints: Some(DeviceMapModeHints {
            clutch: Some(ClutchModeHint::IndependentAxis),
            joystick: Some(JsModeHint::DPad),
            rotary: Some(RotaryModeHint::Knob),
        }),
        init_sequence: vec![],
    };

    assert!(map.validate().is_ok());
    let ks = compile_ks_map(&map).ok_or("expected full composite ks map")?;
    assert_eq!(ks.report_id, Some(0x01));
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::IndependentAxis);
    assert_eq!(ks.joystick_mode_hint, KsJoystickMode::DPad);
    assert_eq!(ks.left_rotary_axis.map(|s| s.offset), Some(29));
    assert_eq!(ks.right_rotary_axis.map(|s| s.offset), Some(31));
    assert!(ks.joystick_hat.is_some());
    Ok(())
}

#[test]
fn two_independent_device_maps_compile_separately() -> R {
    let map_a = DeviceInputMap {
        vendor_id: 0x0001,
        product_id: 0x0001,
        rotaries: vec![rotary("enc_a", 10, RotaryModeHint::Knob)],
        ..base_map()
    };
    let map_b = DeviceInputMap {
        vendor_id: 0x0002,
        product_id: 0x0002,
        clutch: Some(ClutchBinding {
            combined: Some(axis("clutch", 7, AxisDataType::U16Le, false)),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        }),
        ..base_map()
    };

    let ks_a = compile_ks_map(&map_a).ok_or("expected ks_a")?;
    let ks_b = compile_ks_map(&map_b).ok_or("expected ks_b")?;

    assert_eq!(ks_a.rotary_mode_hint, KsRotaryMode::Knob);
    assert!(ks_a.clutch_combined_axis.is_none());
    assert_eq!(ks_b.clutch_mode_hint, KsClutchMode::CombinedAxis);
    assert!(ks_b.left_rotary_axis.is_none());
    Ok(())
}

// ── 6. Mapping conflict detection (validation) ─────────────────────────────

#[test]
fn validation_rejects_no_inputs_defined() {
    let map = base_map();
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::NoInputsDefined)
    ));
}

#[test]
fn validation_rejects_zero_vendor_id() {
    let mut map = base_map();
    map.vendor_id = 0;
    map.axes.push(axis("steer", 1, AxisDataType::U16Le, false));
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::MissingIdentity)
    ));
}

#[test]
fn validation_rejects_zero_product_id() {
    let mut map = base_map();
    map.product_id = 0;
    map.axes.push(axis("steer", 1, AxisDataType::U16Le, false));
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::MissingIdentity)
    ));
}

#[test]
fn validation_rejects_schema_version_zero() {
    let mut map = base_map();
    map.schema_version = 0;
    map.axes.push(axis("steer", 1, AxisDataType::U16Le, false));
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::UnsupportedSchemaVersion(0))
    ));
}

#[test]
fn validation_accepts_buttons_only_map() {
    let mut map = base_map();
    map.buttons.push(button("btn0", 5, 0x01));
    assert!(map.validate().is_ok());
}

#[test]
fn validation_accepts_rotaries_only_map() {
    let mut map = base_map();
    map.rotaries.push(rotary("enc0", 10, RotaryModeHint::Knob));
    assert!(map.validate().is_ok());
}

#[test]
fn validation_accepts_clutch_only_map() {
    let map = DeviceInputMap {
        clutch: Some(ClutchBinding {
            combined: Some(axis("clutch", 7, AxisDataType::U16Le, false)),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        }),
        ..base_map()
    };
    assert!(map.validate().is_ok());
}

#[test]
fn compile_returns_none_when_no_ks_relevant_bindings() {
    let mut map = base_map();
    map.axes
        .push(axis("throttle", 3, AxisDataType::U16Le, false));
    map.buttons.push(button("btn", 5, 0x01));
    assert!(compile_ks_map(&map).is_none());
}

// ── 7. Mapping import/export (JSON round-trips) ────────────────────────────

#[test]
fn json_round_trip_minimal_map() -> R {
    let mut map = base_map();
    map.axes
        .push(axis("throttle", 3, AxisDataType::U16Le, false));

    let json = serde_json::to_string(&map)?;
    let rt: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(map, rt);
    Ok(())
}

#[test]
fn json_round_trip_with_clutch_and_joystick() -> R {
    let map = DeviceInputMap {
        clutch: Some(ClutchBinding {
            combined: None,
            left: Some(axis("cl", 10, AxisDataType::U16Le, false)),
            right: Some(axis("cr", 12, AxisDataType::U16Le, false)),
            left_button: Some(button("clb", 22, 0x01)),
            right_button: Some(button("crb", 22, 0x02)),
            mode_hint: ClutchModeHint::IndependentAxis,
        }),
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::Buttons,
            axis: None,
            buttons: vec![button("js_up", 5, 0x10), button("js_down", 5, 0x20)],
        }),
        ..base_map()
    };

    let json = serde_json::to_string_pretty(&map)?;
    let rt: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(map, rt);
    Ok(())
}

#[test]
fn json_round_trip_all_transport_variants() -> R {
    let transports = [
        DeviceTransportHint::WheelbaseAggregated,
        DeviceTransportHint::StandaloneUsb,
        DeviceTransportHint::UniversalHub,
        DeviceTransportHint::Unknown,
    ];
    for t in transports {
        let json = serde_json::to_string(&t)?;
        let rt: DeviceTransportHint = serde_json::from_str(&json)?;
        assert_eq!(t, rt);
    }
    Ok(())
}

#[test]
fn json_round_trip_mode_hints() -> R {
    let hints = DeviceMapModeHints {
        clutch: Some(ClutchModeHint::Button),
        joystick: Some(JsModeHint::DPad),
        rotary: Some(RotaryModeHint::Knob),
    };
    let json = serde_json::to_string(&hints)?;
    let rt: DeviceMapModeHints = serde_json::from_str(&json)?;
    assert_eq!(hints, rt);
    Ok(())
}

#[test]
fn json_rejects_unknown_fields() {
    let json = r#"{"schema_version":1,"vendor_id":1,"product_id":2,"axes":[],"surprise":true}"#;
    let result: Result<DeviceInputMap, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn json_deserializes_minimal_required_fields() -> R {
    let json = r#"{
        "schema_version": 1,
        "vendor_id": 100,
        "product_id": 200,
        "axes": [{"name":"a","byte_offset":0,"bit_offset":null,"data_type":"u8","signed":false,"invert":false,"min":null,"max":null}]
    }"#;
    let map: DeviceInputMap = serde_json::from_str(json)?;
    assert_eq!(map.vendor_id, 100);
    assert_eq!(map.product_id, 200);
    assert_eq!(map.axes.len(), 1);
    Ok(())
}

// ── Additional edge-case tests ──────────────────────────────────────────────

#[test]
fn report_constraint_default_is_none() {
    let rc = ReportConstraint::default();
    assert_eq!(rc.report_id, None);
    assert_eq!(rc.report_len, None);
}

#[test]
fn compile_propagates_report_id_from_constraint() -> R {
    let map = DeviceInputMap {
        report: ReportConstraint {
            report_id: Some(0x42),
            report_len: Some(32),
        },
        rotaries: vec![rotary("r", 5, RotaryModeHint::Knob)],
        ..base_map()
    };

    let ks = compile_ks_map(&map).ok_or("expected ks map")?;
    assert_eq!(ks.report_id, Some(0x42));
    Ok(())
}

#[test]
fn compile_joystick_unknown_mode() -> R {
    let map = DeviceInputMap {
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::Unknown,
            axis: None,
            buttons: vec![],
        }),
        ..base_map()
    };

    let ks = compile_ks_map(&map).ok_or("expected ks map")?;
    assert_eq!(ks.joystick_mode_hint, KsJoystickMode::Unknown);
    assert!(ks.joystick_hat.is_none());
    Ok(())
}

#[test]
fn compile_clutch_combined_axis_offset_preserved() -> R {
    let map = DeviceInputMap {
        clutch: Some(ClutchBinding {
            combined: Some(axis("clutch", 42, AxisDataType::U16Le, false)),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        }),
        ..base_map()
    };

    let ks = compile_ks_map(&map).ok_or("expected ks map")?;
    assert_eq!(ks.clutch_combined_axis.map(|s| s.offset), Some(42));
    Ok(())
}

#[test]
fn axis_binding_be_types_produce_correct_signedness() -> R {
    let binding_u16be = axis("test_u16be", 5, AxisDataType::U16Be, false);
    let binding_i16be = axis("test_i16be", 7, AxisDataType::I16Be, true);

    let map_unsigned = DeviceInputMap {
        clutch: Some(ClutchBinding {
            combined: Some(binding_u16be),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        }),
        ..base_map()
    };
    let ks_u = compile_ks_map(&map_unsigned).ok_or("expected ks map unsigned")?;
    let src_u = ks_u.clutch_combined_axis.ok_or("expected source")?;
    assert!(!src_u.signed);

    let map_signed = DeviceInputMap {
        clutch: Some(ClutchBinding {
            combined: Some(binding_i16be),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        }),
        ..base_map()
    };
    let ks_s = compile_ks_map(&map_signed).ok_or("expected ks map signed")?;
    let src_s = ks_s.clutch_combined_axis.ok_or("expected source")?;
    assert!(src_s.signed);
    Ok(())
}

#[test]
fn default_device_input_map_fields() {
    let map = DeviceInputMap::default();
    assert_eq!(map.schema_version, 1);
    assert_eq!(map.vendor_id, 0);
    assert_eq!(map.product_id, 0);
    assert_eq!(map.transport, DeviceTransportHint::Unknown);
    assert!(map.axes.is_empty());
    assert!(map.buttons.is_empty());
    assert!(map.clutch.is_none());
    assert!(map.joystick.is_none());
    assert!(map.rotaries.is_empty());
    assert!(map.handbrake.is_none());
    assert!(map.mode_hints.is_none());
    assert!(map.init_sequence.is_empty());
}

#[test]
fn clutch_mode_hint_default_is_unknown() {
    assert_eq!(ClutchModeHint::default(), ClutchModeHint::Unknown);
}

#[test]
fn button_binding_invert_flag_round_trips() -> R {
    let btn = ButtonBinding {
        name: "inverted_btn".to_string(),
        byte_offset: 8,
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
fn rotary_binding_serde_round_trip() -> R {
    let r = rotary("enc_test", 42, RotaryModeHint::Knob);
    let json = serde_json::to_string(&r)?;
    let rt: RotaryBinding = serde_json::from_str(&json)?;
    assert_eq!(r, rt);
    Ok(())
}
