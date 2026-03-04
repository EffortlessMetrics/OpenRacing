#![allow(clippy::redundant_closure)]

use racing_wheel_input_maps::{
    AxisBinding, AxisDataType, ButtonBinding, ClutchBinding, ClutchModeHint, DeviceInputMap,
    DeviceInputMapError, DeviceMapModeHints, DeviceTransportHint, InitFrameDirection,
    InitReportFrame, JsBinding, JsModeHint, ReportConstraint, RotaryBinding, RotaryModeHint,
    compile_ks_map,
};
use racing_wheel_ks::{KsClutchMode, KsJoystickMode, KsRotaryMode};

type R = Result<(), Box<dyn std::error::Error>>;

// ── Helper ──────────────────────────────────────────────────────────────

fn make_axis(name: &str, byte_offset: u16, data_type: AxisDataType) -> AxisBinding {
    AxisBinding {
        name: name.to_string(),
        byte_offset,
        bit_offset: None,
        data_type,
        signed: false,
        invert: false,
        min: None,
        max: None,
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

// ── Validation ──────────────────────────────────────────────────────────

#[test]
fn default_has_schema_version_one() {
    let map = DeviceInputMap::default();
    assert_eq!(map.schema_version, 1);
}

#[test]
fn validation_rejects_empty_descriptor() {
    let map = DeviceInputMap {
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
fn validation_rejects_schema_version_zero() {
    let mut map = minimal_valid_map();
    map.schema_version = 0;
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::UnsupportedSchemaVersion(0))
    ));
}

#[test]
fn validation_rejects_missing_vendor_id() {
    let mut map = minimal_valid_map();
    map.vendor_id = 0;
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::MissingIdentity)
    ));
}

#[test]
fn validation_rejects_missing_product_id() {
    let mut map = minimal_valid_map();
    map.product_id = 0;
    assert!(matches!(
        map.validate(),
        Err(DeviceInputMapError::MissingIdentity)
    ));
}

#[test]
fn validation_accepts_minimal_map() {
    assert!(minimal_valid_map().validate().is_ok());
}

// ── compile_ks_map: no KS content ───────────────────────────────────────

#[test]
fn compile_returns_none_for_no_ks_content() {
    assert!(compile_ks_map(&minimal_valid_map()).is_none());
}

// ── compile_ks_map: clutch bindings ─────────────────────────────────────

#[test]
fn compile_wires_combined_clutch() -> R {
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
    let ks = compile_ks_map(&map).ok_or("should compile combined clutch")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::CombinedAxis);
    assert_eq!(ks.clutch_combined_axis.map(|s| s.offset), Some(7));
    Ok(())
}

#[test]
fn compile_wires_independent_clutch() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        clutch: Some(ClutchBinding {
            combined: None,
            left: Some(make_axis("left", 10, AxisDataType::U16Le)),
            right: Some(make_axis("right", 12, AxisDataType::U16Le)),
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::IndependentAxis,
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile independent clutch")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::IndependentAxis);
    assert_eq!(ks.clutch_left_axis.map(|s| s.offset), Some(10));
    assert_eq!(ks.clutch_right_axis.map(|s| s.offset), Some(12));
    Ok(())
}

#[test]
fn compile_wires_clutch_buttons() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        clutch: Some(ClutchBinding {
            combined: None,
            left: None,
            right: None,
            left_button: Some(ButtonBinding {
                name: "left_btn".to_string(),
                byte_offset: 22,
                bit_mask: 0x01,
                invert: false,
            }),
            right_button: Some(ButtonBinding {
                name: "right_btn".to_string(),
                byte_offset: 22,
                bit_mask: 0x02,
                invert: false,
            }),
            mode_hint: ClutchModeHint::Button,
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile clutch buttons")?;
    assert_eq!(ks.clutch_mode_hint, KsClutchMode::Button);
    assert_eq!(ks.clutch_left_button.map(|b| b.mask), Some(0x01));
    assert_eq!(ks.clutch_right_button.map(|b| b.mask), Some(0x02));
    Ok(())
}

// ── compile_ks_map: rotary bindings ─────────────────────────────────────

#[test]
fn compile_wires_rotary_slots() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        rotaries: vec![
            RotaryBinding {
                name: "left".to_string(),
                byte_offset: 29,
                mode: RotaryModeHint::Button,
            },
            RotaryBinding {
                name: "right".to_string(),
                byte_offset: 30,
                mode: RotaryModeHint::Button,
            },
            RotaryBinding {
                name: "third".to_string(),
                byte_offset: 31,
                mode: RotaryModeHint::Knob,
            },
        ],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("should compile rotaries")?;
    assert_eq!(ks.left_rotary_axis.map(|s| s.offset), Some(29));
    assert_eq!(ks.right_rotary_axis.map(|s| s.offset), Some(30));
    assert_eq!(ks.encoders[2].map(|s| s.offset), Some(31));
    Ok(())
}

#[test]
fn compile_rotary_mode_propagates() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        rotaries: vec![RotaryBinding {
            name: "dial".to_string(),
            byte_offset: 15,
            mode: RotaryModeHint::Knob,
        }],
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("rotary mode")?;
    assert_eq!(ks.rotary_mode_hint, KsRotaryMode::Knob);
    Ok(())
}

// ── compile_ks_map: joystick bindings ───────────────────────────────────

#[test]
fn compile_wires_joystick_dpad() -> R {
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
    let ks = compile_ks_map(&map).ok_or("joystick dpad")?;
    assert_eq!(ks.joystick_mode_hint, KsJoystickMode::DPad);
    assert_eq!(ks.joystick_hat.map(|s| s.offset), Some(27));
    Ok(())
}

#[test]
fn compile_wires_joystick_buttons_mode() -> R {
    let map = DeviceInputMap {
        vendor_id: 0x346E,
        product_id: 0x0002,
        joystick: Some(JsBinding {
            mode_hint: JsModeHint::Buttons,
            axis: None,
            buttons: vec![ButtonBinding {
                name: "js_up".to_string(),
                byte_offset: 5,
                bit_mask: 0x10,
                invert: false,
            }],
        }),
        ..Default::default()
    };
    let ks = compile_ks_map(&map).ok_or("joystick buttons")?;
    assert_eq!(ks.joystick_mode_hint, KsJoystickMode::Button);
    assert!(ks.joystick_hat.is_none());
    Ok(())
}

// ── compile_ks_map: report_id propagation ───────────────────────────────

#[test]
fn compile_propagates_report_id() -> R {
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

// ── Axis binding signed/unsigned ────────────────────────────────────────

#[test]
fn signed_types_produce_signed_source() {
    let signed_types = [AxisDataType::I8, AxisDataType::I16Le, AxisDataType::I16Be];
    for dt in signed_types {
        let map = DeviceInputMap {
            vendor_id: 0x346E,
            product_id: 0x0002,
            clutch: Some(ClutchBinding {
                combined: Some(AxisBinding {
                    name: "test".to_string(),
                    byte_offset: 5,
                    bit_offset: None,
                    data_type: dt,
                    signed: true,
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
            assert!(src.signed);
            assert_eq!(src.offset, 5);
        }
    }
}

// ── Serde round-trips ───────────────────────────────────────────────────

#[test]
fn json_round_trip_minimal() -> R {
    let map = minimal_valid_map();
    let json = serde_json::to_string(&map)?;
    let round_tripped: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(map, round_tripped);
    Ok(())
}

#[test]
fn json_round_trip_full() -> R {
    let map = DeviceInputMap {
        schema_version: 1,
        vendor_id: 0x346E,
        product_id: 0x0022,
        transport: DeviceTransportHint::UniversalHub,
        report: ReportConstraint {
            report_id: Some(1),
            report_len: Some(64),
        },
        axes: vec![make_axis("steering", 1, AxisDataType::I16Le)],
        buttons: vec![ButtonBinding {
            name: "paddle_left".to_string(),
            byte_offset: 11,
            bit_mask: 0x01,
            invert: false,
        }],
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
        rotaries: vec![RotaryBinding {
            name: "left_dial".to_string(),
            byte_offset: 29,
            mode: RotaryModeHint::Knob,
        }],
        handbrake: Some(make_axis("handbrake", 33, AxisDataType::U16Le)),
        mode_hints: Some(DeviceMapModeHints {
            clutch: Some(ClutchModeHint::IndependentAxis),
            joystick: Some(JsModeHint::DPad),
            rotary: Some(RotaryModeHint::Knob),
        }),
        init_sequence: vec![InitReportFrame {
            report_id: 0x09,
            payload: vec![0x01, 0x02],
            direction: InitFrameDirection::Out,
        }],
    };
    let json = serde_json::to_string_pretty(&map)?;
    let round_tripped: DeviceInputMap = serde_json::from_str(&json)?;
    assert_eq!(map, round_tripped);
    Ok(())
}

#[test]
fn rejects_unknown_fields() {
    let json = r#"{
        "schema_version": 1,
        "vendor_id": 1,
        "product_id": 2,
        "axes": [],
        "unknown_field": true
    }"#;
    let result: Result<DeviceInputMap, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn deserializes_minimal_json() -> R {
    let json = r#"{
        "schema_version": 1,
        "vendor_id": 13422,
        "product_id": 34,
        "axes": [{
            "name": "throttle",
            "byte_offset": 3,
            "bit_offset": null,
            "data_type": "u16_le",
            "signed": false,
            "invert": false,
            "min": null,
            "max": null
        }]
    }"#;
    let map: DeviceInputMap = serde_json::from_str(json)?;
    assert_eq!(map.schema_version, 1);
    assert_eq!(map.vendor_id, 13422);
    assert_eq!(map.axes.len(), 1);
    Ok(())
}

#[test]
fn transport_hint_all_variants_round_trip() -> R {
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

#[test]
fn axis_data_type_all_variants_round_trip() -> R {
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

// ── Proptest ────────────────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(200))]

    #[test]
    fn prop_json_round_trip_varied_offsets(
        byte_offset in 0u16..=255u16,
        vendor_id in 1u16..=0xFFFFu16,
        product_id in 1u16..=0xFFFFu16,
    ) {
        let map = DeviceInputMap {
            schema_version: 1,
            vendor_id,
            product_id,
            axes: vec![make_axis("test", byte_offset, AxisDataType::U16Le)],
            ..Default::default()
        };
        let json = serde_json::to_string(&map).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let rt: DeviceInputMap = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(map, rt);
    }
}
