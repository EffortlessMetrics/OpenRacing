//! Capture-driven input map schema used by non-production mapping workflow.
//!
//! The schema intentionally stays simple and versioned so `device_map.json` files
//! produced by the capture utility can be stored as portable assets and validated at
//! load-time.

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
            return Err(DeviceInputMapError::UnsupportedSchemaVersion(self.schema_version));
        }

        if self.axes.is_empty() && self.buttons.is_empty() && self.rotaries.is_empty() && self.clutch.is_none()
        {
            return Err(DeviceInputMapError::NoInputsDefined);
        }

        if self.vendor_id == 0 || self.product_id == 0 {
            return Err(DeviceInputMapError::MissingIdentity);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceTransportHint {
    WheelbaseAggregated,
    StandaloneUsb,
    UniversalHub,
    Unknown,
}

impl Default for DeviceTransportHint {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportConstraint {
    pub report_id: Option<u8>,
    pub report_len: Option<u16>,
}

impl Default for ReportConstraint {
    fn default() -> Self {
        Self {
            report_id: None,
            report_len: None,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClutchModeHint {
    CombinedAxis,
    IndependentAxis,
    Button,
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
        let mut map = DeviceInputMap::default();
        map.vendor_id = 0x346E;
        map.product_id = 0x0022;

        assert!(matches!(
            map.validate(),
            Err(DeviceInputMapError::NoInputsDefined)
        ));
    }

    #[test]
    fn validation_accepts_minimal_non_empty_descriptor() {
        let mut map = DeviceInputMap::default();
        map.vendor_id = 0x346E;
        map.product_id = 0x0002;
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
}
