//! Moza device types: models, categories, compatibility, normalization.

#![deny(static_mut_refs)]

use crate::ids::product_ids;
use racing_wheel_ks::KsReportSnapshot;

/// ES control-surface dimensions documented by Moza.
pub const ES_BUTTON_COUNT: usize = 22;
pub const ES_LED_COUNT: usize = 10;

/// High-level category for Moza USB products.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MozaDeviceCategory {
    Wheelbase,
    Pedals,
    Shifter,
    Handbrake,
    Unknown,
}

/// Integration topology hint for runtime handling and capture strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MozaTopologyHint {
    /// USB-facing wheelbase that aggregates connected peripherals (e.g. KS on quick-release).
    WheelbaseAggregated,
    /// Standalone USB peripheral connected directly to host.
    StandaloneUsb,
    /// Product not yet identified from verified captures.
    Unknown,
}

/// Identity metadata for a Moza product ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MozaDeviceIdentity {
    pub product_id: u16,
    pub name: &'static str,
    pub category: MozaDeviceCategory,
    pub topology_hint: MozaTopologyHint,
    pub supports_ffb: bool,
}

/// Identify a Moza product and provide conservative runtime hints.
pub fn identify_device(product_id: u16) -> MozaDeviceIdentity {
    match product_id {
        product_ids::R3_V1 | product_ids::R3_V2 => MozaDeviceIdentity {
            product_id,
            name: "Moza R3",
            category: MozaDeviceCategory::Wheelbase,
            topology_hint: MozaTopologyHint::WheelbaseAggregated,
            supports_ffb: true,
        },
        product_ids::R5_V1 | product_ids::R5_V2 => MozaDeviceIdentity {
            product_id,
            name: "Moza R5",
            category: MozaDeviceCategory::Wheelbase,
            topology_hint: MozaTopologyHint::WheelbaseAggregated,
            supports_ffb: true,
        },
        product_ids::R9_V1 | product_ids::R9_V2 => MozaDeviceIdentity {
            product_id,
            name: "Moza R9",
            category: MozaDeviceCategory::Wheelbase,
            topology_hint: MozaTopologyHint::WheelbaseAggregated,
            supports_ffb: true,
        },
        product_ids::R12_V1 | product_ids::R12_V2 => MozaDeviceIdentity {
            product_id,
            name: "Moza R12",
            category: MozaDeviceCategory::Wheelbase,
            topology_hint: MozaTopologyHint::WheelbaseAggregated,
            supports_ffb: true,
        },
        product_ids::R16_R21_V1 | product_ids::R16_R21_V2 => MozaDeviceIdentity {
            product_id,
            name: "Moza R16/R21",
            category: MozaDeviceCategory::Wheelbase,
            topology_hint: MozaTopologyHint::WheelbaseAggregated,
            supports_ffb: true,
        },
        product_ids::SR_P_PEDALS => MozaDeviceIdentity {
            product_id,
            name: "Moza SR-P Pedals",
            category: MozaDeviceCategory::Pedals,
            topology_hint: MozaTopologyHint::StandaloneUsb,
            supports_ffb: false,
        },
        product_ids::HGP_SHIFTER | product_ids::SGP_SHIFTER => MozaDeviceIdentity {
            product_id,
            name: "Moza Shifter",
            category: MozaDeviceCategory::Shifter,
            topology_hint: MozaTopologyHint::StandaloneUsb,
            supports_ffb: false,
        },
        product_ids::HBP_HANDBRAKE => MozaDeviceIdentity {
            product_id,
            name: "Moza HBP Handbrake",
            category: MozaDeviceCategory::Handbrake,
            topology_hint: MozaTopologyHint::StandaloneUsb,
            supports_ffb: false,
        },
        _ => MozaDeviceIdentity {
            product_id,
            name: "Moza Unknown",
            category: MozaDeviceCategory::Unknown,
            topology_hint: MozaTopologyHint::Unknown,
            supports_ffb: false,
        },
    }
}

/// Return true when the product ID is a known Moza wheelbase.
pub fn is_wheelbase_product(product_id: u16) -> bool {
    matches!(
        identify_device(product_id).category,
        MozaDeviceCategory::Wheelbase
    )
}

/// ES compatibility status derived from known wheelbase compatibility rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MozaEsCompatibility {
    /// Compatibility is known and expected to work.
    Supported,
    /// Hardware revision is known to be incompatible (R9 V1).
    UnsupportedHardwareRevision,
    /// Device is a wheelbase, but compatibility has not been capture-validated.
    UnknownWheelbase,
    /// Product is not a wheelbase, so ES compatibility does not apply.
    NotWheelbase,
}

impl MozaEsCompatibility {
    /// Returns true when ES usage is expected to work on this product.
    pub const fn is_supported(self) -> bool {
        matches!(self, Self::Supported)
    }

    /// Human-readable compatibility diagnostic for operators and logs.
    pub const fn diagnostic_message(self) -> Option<&'static str> {
        match self {
            Self::Supported => Some("ES compatibility supported"),
            Self::UnsupportedHardwareRevision => Some(
                "R9 V1 is not compatible with the ES wheel; use R9 V2 or another supported base",
            ),
            Self::UnknownWheelbase => {
                Some("ES compatibility for this wheelbase is not capture-validated yet")
            }
            Self::NotWheelbase => None,
        }
    }
}

/// Determine ES compatibility from a Moza USB product ID.
pub fn es_compatibility(product_id: u16) -> MozaEsCompatibility {
    match product_id {
        product_ids::R9_V1 => MozaEsCompatibility::UnsupportedHardwareRevision,
        product_ids::R5_V1 | product_ids::R5_V2 | product_ids::R9_V2 => {
            MozaEsCompatibility::Supported
        }
        product_ids::R3_V1
        | product_ids::R3_V2
        | product_ids::R12_V1
        | product_ids::R12_V2
        | product_ids::R16_R21_V1
        | product_ids::R16_R21_V2 => MozaEsCompatibility::UnknownWheelbase,
        _ => MozaEsCompatibility::NotWheelbase,
    }
}

/// ES joystick mode as configured in Moza Pit House.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MozaEsJoystickMode {
    /// Joystick directions are exposed as discrete button bits.
    Buttons,
    /// Joystick directions are exposed as a HID hat / D-pad semantic.
    DPad,
}

impl MozaEsJoystickMode {
    /// Parse mode value from persisted configuration/probe metadata.
    ///
    /// `0` => buttons mode, `1` => D-pad mode.
    pub const fn from_config_value(mode: u8) -> Option<Self> {
        match mode {
            0 => Some(Self::Buttons),
            1 => Some(Self::DPad),
            _ => None,
        }
    }
}

/// Normalized 8-way hat direction used by ES joystick D-pad mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MozaHatDirection {
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft,
    Center,
}

impl MozaHatDirection {
    /// Parse a HID hat value (0..=8) into normalized direction.
    pub const fn from_hid_hat_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Up),
            1 => Some(Self::UpRight),
            2 => Some(Self::Right),
            3 => Some(Self::DownRight),
            4 => Some(Self::Down),
            5 => Some(Self::DownLeft),
            6 => Some(Self::Left),
            7 => Some(Self::UpLeft),
            8 => Some(Self::Center),
            _ => None,
        }
    }
}

/// Moza device model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MozaModel {
    R3,
    R5,
    R9,
    R12,
    R16,
    R21,
    SrpPedals,
    Unknown,
}

impl MozaModel {
    pub fn from_pid(pid: u16) -> Self {
        match pid {
            product_ids::R3_V1 | product_ids::R3_V2 => Self::R3,
            product_ids::R5_V1 | product_ids::R5_V2 => Self::R5,
            product_ids::R9_V1 | product_ids::R9_V2 => Self::R9,
            product_ids::R12_V1 | product_ids::R12_V2 => Self::R12,
            // R16/R21 share PID; differentiate by torque query if needed.
            product_ids::R16_R21_V1 | product_ids::R16_R21_V2 => Self::R16,
            product_ids::SR_P_PEDALS => Self::SrpPedals,
            _ => Self::Unknown,
        }
    }

    pub fn max_torque_nm(&self) -> f32 {
        match self {
            Self::R3 => 3.9,
            Self::R5 => 5.5,
            Self::R9 => 9.0,
            Self::R12 => 12.0,
            Self::R16 => 16.0,
            Self::R21 => 21.0,
            Self::SrpPedals => 0.0,
            Self::Unknown => 10.0,
        }
    }
}

/// Raw pedal axis samples parsed from an aggregated wheelbase input report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MozaPedalAxesRaw {
    pub throttle: u16,
    pub brake: u16,
    pub clutch: Option<u16>,
    pub handbrake: Option<u16>,
}

/// Normalized pedal axis samples in the `[0.0, 1.0]` range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MozaPedalAxes {
    pub throttle: f32,
    pub brake: f32,
    pub clutch: Option<f32>,
    pub handbrake: Option<f32>,
}

impl MozaPedalAxesRaw {
    /// Normalize 16-bit raw samples to `[0.0, 1.0]`.
    pub fn normalize(self) -> MozaPedalAxes {
        const MAX: f32 = u16::MAX as f32;
        MozaPedalAxes {
            throttle: self.throttle as f32 / MAX,
            brake: self.brake as f32 / MAX,
            clutch: self.clutch.map(|value| value as f32 / MAX),
            handbrake: self.handbrake.map(|value| value as f32 / MAX),
        }
    }
}

/// Raw input snapshot for Moza reports after report normalization.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MozaInputState {
    pub steering_u16: u16,
    pub throttle_u16: u16,
    pub brake_u16: u16,
    pub clutch_u16: u16,
    pub handbrake_u16: u16,
    pub buttons: [u8; 16],
    pub hat: u8,
    pub funky: u8,
    pub rotary: [u8; 2],
    pub ks_snapshot: KsReportSnapshot,
    pub tick: u32,
}

impl MozaInputState {
    /// Return a zero-initialized state with a tick marker.
    pub fn empty(tick: u32) -> Self {
        Self {
            steering_u16: 0,
            throttle_u16: 0,
            brake_u16: 0,
            clutch_u16: 0,
            handbrake_u16: 0,
            buttons: [0u8; 16],
            hat: 0,
            funky: 0,
            rotary: [0u8; 2],
            ks_snapshot: KsReportSnapshot::default(),
            tick,
        }
    }

    pub fn both_clutches_pressed(&self, threshold: u16) -> bool {
        if let Some(pressed) = self.ks_snapshot.both_clutches_pressed(threshold) {
            return pressed;
        }
        self.clutch_u16 >= threshold && self.handbrake_u16 >= threshold
    }
}
