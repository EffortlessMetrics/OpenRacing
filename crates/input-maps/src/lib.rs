//! Capture-driven input map schema used by non-production mapping workflow.
//!
//! The schema stays simple and versioned so `device_map.json` files produced
//! by the capture utility can be stored as portable assets and validated at
//! load-time.

#![deny(static_mut_refs)]

use racing_wheel_ks::{
    KS_ENCODER_COUNT, KsAxisSource, KsBitSource, KsByteSource, KsClutchMode, KsJoystickMode,
    KsReportMap, KsRotaryMode,
};
use serde::{Deserialize, Serialize};

/// Top-level versioned descriptor stored on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceInputMap {
    pub schema_version: u8,
    pub vendor_id: u16,
    pub product_id: u16,

    #[serde(default)]
    pub transport: DeviceTransportHint,

    #[serde(default)]
    pub report: ReportConstraint,

    #[serde(default)]
    pub axes: Vec<AxisBinding>,

    #[serde(default)]
    pub buttons: Vec<ButtonBinding>,

    #[serde(default)]
    pub clutch: Option<ClutchBinding>,

    #[serde(default)]
    pub joystick: Option<JsBinding>,

    #[serde(default)]
    pub rotaries: Vec<RotaryBinding>,

    #[serde(default)]
    pub handbrake: Option<AxisBinding>,

    #[serde(default)]
    pub mode_hints: Option<DeviceMapModeHints>,

    #[serde(default)]
    pub init_sequence: Vec<InitReportFrame>,
}

impl Default for DeviceInputMap {
    fn default() -> Self {
        Self {
            schema_version: 1,
            vendor_id: 0,
            product_id: 0,
            transport: DeviceTransportHint::Unknown,
            report: ReportConstraint::default(),
            axes: Vec::new(),
            buttons: Vec::new(),
            clutch: None,
            joystick: None,
            rotaries: Vec::new(),
            handbrake: None,
            mode_hints: None,
            init_sequence: Vec::new(),
        }
    }
}

impl DeviceInputMap {
    pub fn validate(&self) -> Result<(), DeviceInputMapError> {
        if self.schema_version == 0 {
            return Err(DeviceInputMapError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }

        if self.axes.is_empty()
            && self.buttons.is_empty()
            && self.rotaries.is_empty()
            && self.clutch.is_none()
        {
            return Err(DeviceInputMapError::NoInputsDefined);
        }

        if self.vendor_id == 0 || self.product_id == 0 {
            return Err(DeviceInputMapError::MissingIdentity);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeviceTransportHint {
    WheelbaseAggregated,
    StandaloneUsb,
    UniversalHub,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ReportConstraint {
    pub report_id: Option<u8>,
    pub report_len: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxisBinding {
    pub name: String,
    pub byte_offset: u16,
    pub bit_offset: Option<u8>,
    pub data_type: AxisDataType,
    pub signed: bool,
    pub invert: bool,
    pub min: Option<i32>,
    pub max: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonBinding {
    pub name: String,
    pub byte_offset: u16,
    pub bit_mask: u8,
    pub invert: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotaryBinding {
    pub name: String,
    pub byte_offset: u16,
    pub mode: RotaryModeHint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClutchBinding {
    pub left: Option<AxisBinding>,
    pub right: Option<AxisBinding>,
    pub combined: Option<AxisBinding>,
    pub left_button: Option<ButtonBinding>,
    pub right_button: Option<ButtonBinding>,
    #[serde(default)]
    pub mode_hint: ClutchModeHint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsBinding {
    pub mode_hint: JsModeHint,
    pub axis: Option<AxisBinding>,
    pub buttons: Vec<ButtonBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceMapModeHints {
    pub clutch: Option<ClutchModeHint>,
    pub joystick: Option<JsModeHint>,
    pub rotary: Option<RotaryModeHint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClutchModeHint {
    CombinedAxis,
    IndependentAxis,
    Button,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsModeHint {
    Buttons,
    DPad,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotaryModeHint {
    Button,
    Knob,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisDataType {
    U8,
    I8,
    U16Le,
    I16Le,
    U16Be,
    I16Be,
    Bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitReportFrame {
    pub report_id: u8,
    pub payload: Vec<u8>,
    pub direction: InitFrameDirection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InitFrameDirection {
    In,
    Out,
}

#[derive(Debug, Clone)]
pub enum DeviceInputMapError {
    UnsupportedSchemaVersion(u8),
    MissingIdentity,
    NoInputsDefined,
}

/// Compile a `DeviceInputMap` into a `KsReportMap` using the map's clutch,
/// joystick, and rotary bindings.
///
/// Returns `None` when the map has no KS-relevant bindings (no clutch,
/// no rotaries, and no joystick).
pub fn compile_ks_map(map: &DeviceInputMap) -> Option<KsReportMap> {
    let has_ks_content = map.clutch.is_some() || !map.rotaries.is_empty() || map.joystick.is_some();

    if !has_ks_content {
        return None;
    }

    let mut ks = KsReportMap::empty();
    ks.report_id = map.report.report_id;

    // Wire clutch bindings.
    if let Some(clutch) = &map.clutch {
        ks.clutch_mode_hint = match clutch.mode_hint {
            ClutchModeHint::CombinedAxis => KsClutchMode::CombinedAxis,
            ClutchModeHint::IndependentAxis => KsClutchMode::IndependentAxis,
            ClutchModeHint::Button => KsClutchMode::Button,
            ClutchModeHint::Unknown => KsClutchMode::Unknown,
        };

        if let Some(combined) = &clutch.combined {
            ks.clutch_combined_axis = axis_binding_to_ks_source(combined);
        }
        if let Some(left) = &clutch.left {
            ks.clutch_left_axis = axis_binding_to_ks_source(left);
        }
        if let Some(right) = &clutch.right {
            ks.clutch_right_axis = axis_binding_to_ks_source(right);
        }
        if let Some(lb) = &clutch.left_button {
            ks.clutch_left_button = Some(KsBitSource::new(lb.byte_offset as usize, lb.bit_mask));
        }
        if let Some(rb) = &clutch.right_button {
            ks.clutch_right_button = Some(KsBitSource::new(rb.byte_offset as usize, rb.bit_mask));
        }
    }

    // Wire rotaries (first two map to left/right rotary axes).
    for (i, rotary) in map.rotaries.iter().enumerate().take(KS_ENCODER_COUNT) {
        let source = KsAxisSource::new(rotary.byte_offset as usize, false);
        ks.encoders[i] = Some(source);
        match i {
            0 => ks.left_rotary_axis = Some(source),
            1 => ks.right_rotary_axis = Some(source),
            _ => {}
        }
        ks.rotary_mode_hint = match rotary.mode {
            RotaryModeHint::Button => KsRotaryMode::Button,
            RotaryModeHint::Knob => KsRotaryMode::Knob,
            RotaryModeHint::Unknown => KsRotaryMode::Unknown,
        };
    }

    // Wire joystick.
    if let Some(js) = &map.joystick {
        ks.joystick_mode_hint = match js.mode_hint {
            JsModeHint::Buttons => KsJoystickMode::Button,
            JsModeHint::DPad => KsJoystickMode::DPad,
            JsModeHint::Unknown => KsJoystickMode::Unknown,
        };
        if let Some(axis) = &js.axis {
            ks.joystick_hat = Some(KsByteSource::new(axis.byte_offset as usize));
        }
    }

    Some(ks)
}

fn axis_binding_to_ks_source(binding: &AxisBinding) -> Option<KsAxisSource> {
    let signed = matches!(
        binding.data_type,
        AxisDataType::I8 | AxisDataType::I16Le | AxisDataType::I16Be
    );
    Some(KsAxisSource::new(binding.byte_offset as usize, signed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_input_map_default_is_valid_shape() {
        let map = DeviceInputMap::default();
        assert_eq!(map.schema_version, 1);
        assert!(map.vendor_id == 0);
        assert_eq!(map.axes.len(), 0);
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
    fn validation_accepts_minimal_non_empty_descriptor() {
        let mut map = DeviceInputMap {
            vendor_id: 0x346E,
            product_id: 0x0002,
            ..Default::default()
        };
        map.axes.push(AxisBinding {
            name: "throttle".to_string(),
            byte_offset: 3,
            bit_offset: None,
            data_type: AxisDataType::U16Le,
            signed: false,
            invert: false,
            min: None,
            max: None,
        });

        assert!(map.validate().is_ok());
    }

    #[test]
    fn compile_ks_map_returns_none_for_no_ks_content() {
        let mut map = DeviceInputMap {
            vendor_id: 0x346E,
            product_id: 0x0002,
            ..Default::default()
        };
        map.axes.push(AxisBinding {
            name: "throttle".to_string(),
            byte_offset: 3,
            bit_offset: None,
            data_type: AxisDataType::U16Le,
            signed: false,
            invert: false,
            min: None,
            max: None,
        });

        assert!(compile_ks_map(&map).is_none());
    }

    #[test]
    fn compile_ks_map_wires_clutch_bindings() {
        let mut map = DeviceInputMap {
            vendor_id: 0x346E,
            product_id: 0x0002,
            ..Default::default()
        };
        map.clutch = Some(ClutchBinding {
            combined: Some(AxisBinding {
                name: "clutch_combined".to_string(),
                byte_offset: 7,
                bit_offset: None,
                data_type: AxisDataType::U16Le,
                signed: false,
                invert: false,
                min: None,
                max: None,
            }),
            left: None,
            right: None,
            left_button: None,
            right_button: None,
            mode_hint: ClutchModeHint::CombinedAxis,
        });

        let ks = compile_ks_map(&map).expect("should compile ks map for clutch binding");
        assert_eq!(ks.clutch_mode_hint, KsClutchMode::CombinedAxis);
        assert!(ks.clutch_combined_axis.is_some());
        assert_eq!(ks.clutch_combined_axis.map(|s| s.offset), Some(7));
    }

    #[test]
    fn compile_ks_map_wires_all_rotary_slots() {
        let mut map = DeviceInputMap {
            vendor_id: 0x346E,
            product_id: 0x0002,
            ..Default::default()
        };

        map.rotaries = vec![
            RotaryBinding {
                name: "rotary_left".to_string(),
                byte_offset: 29,
                mode: RotaryModeHint::Button,
            },
            RotaryBinding {
                name: "rotary_right".to_string(),
                byte_offset: 30,
                mode: RotaryModeHint::Button,
            },
            RotaryBinding {
                name: "thumb_left".to_string(),
                byte_offset: 31,
                mode: RotaryModeHint::Knob,
            },
        ];

        let compiled = compile_ks_map(&map);
        assert!(compiled.is_some());
        let ks = match compiled {
            Some(value) => value,
            None => panic!("expected compiled KS map"),
        };

        assert_eq!(ks.left_rotary_axis.map(|s| s.offset), Some(29));
        assert_eq!(ks.right_rotary_axis.map(|s| s.offset), Some(30));
        assert_eq!(ks.encoders[0].map(|s| s.offset), Some(29));
        assert_eq!(ks.encoders[1].map(|s| s.offset), Some(30));
        assert_eq!(ks.encoders[2].map(|s| s.offset), Some(31));
    }
}
