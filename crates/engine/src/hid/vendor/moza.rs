//! Moza Racing protocol handler
//!
//! Implements the initialization handshake and configuration for Moza wheelbases.
//! Supports both V1 (0x000x) and V2 (0x001x) hardware revisions.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol, MozaInputState};
use crate::input::{
    KsAxisSource, KsByteSource, KsClutchMode, KsJoystickMode, KsReportMap, KsRotaryMode,
    KS_ENCODER_COUNT,
};
use super::moza_direct::REPORT_LEN;
use std::sync::atomic::{AtomicU8, Ordering};
use tracing::{debug, info, warn};

const MOZA_INIT_STATE_UNINITIALIZED: u8 = 0;
const MOZA_INIT_STATE_INITIALIZING: u8 = 1;
const MOZA_INIT_STATE_READY: u8 = 2;
const MOZA_INIT_STATE_FAILED: u8 = 3;

/// Moza initialization lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MozaInitState {
    /// No handshake attempt has run on this protocol instance.
    Uninitialized,
    /// Handshake currently in progress.
    Initializing,
    /// Handshake completed successfully.
    Ready,
    /// Handshake attempted and not fully successful.
    Failed,
}

impl MozaInitState {
    fn from_u8(value: u8) -> Self {
        match value {
            MOZA_INIT_STATE_INITIALIZING => Self::Initializing,
            MOZA_INIT_STATE_READY => Self::Ready,
            MOZA_INIT_STATE_FAILED => Self::Failed,
            _ => Self::Uninitialized,
        }
    }
}

/// Report ID and axis offsets for aggregated wheelbase input reports.
///
/// These offsets are based on the current Moza protocol document in this
/// repository and should be validated against per-firmware capture traces.
pub mod input_report {
    pub const REPORT_ID: u8 = 0x01;
    pub const STEERING_START: usize = 1;
    pub const THROTTLE_START: usize = 3;
    pub const BRAKE_START: usize = 5;
    pub const CLUTCH_START: usize = 7;
    pub const HANDBRAKE_START: usize = 9;
    pub const BUTTONS_START: usize = 11;
    pub const BUTTONS_LEN: usize = 16;
    pub const HAT_START: usize = BUTTONS_START + BUTTONS_LEN;
    pub const FUNKY_START: usize = HAT_START + 1;
    pub const ROTARY_START: usize = FUNKY_START + 1;
    pub const ROTARY_LEN: usize = 2;
}

/// Moza HID Report IDs
pub mod report_ids {
    /// Device info query
    pub const DEVICE_INFO: u8 = 0x01;
    /// High torque enable
    pub const HIGH_TORQUE: u8 = 0x02;
    /// Start input reports
    pub const START_REPORTS: u8 = 0x03;
    /// Set rotation range
    pub const ROTATION_RANGE: u8 = 0x10;
    /// Set FFB mode
    pub const FFB_MODE: u8 = 0x11;
    /// Direct torque output
    pub const DIRECT_TORQUE: u8 = 0x20;
    /// Device gain
    pub const DEVICE_GAIN: u8 = 0x21;
}

/// Best-effort layouts for direct USB HBP handbrake reports.
pub mod hbp_report {
    /// Handbrake axis with report-id prefix.
    pub const WITH_REPORT_ID_AXIS_START: usize = 1;
    /// Optional button-style byte with report-id prefix.
    pub const WITH_REPORT_ID_BUTTON: usize = 3;
    /// Handbrake axis with no report-id prefix.
    pub const RAW_AXIS_START: usize = 0;
    /// Optional button-style byte with no report-id prefix.
    pub const RAW_BUTTON: usize = 2;
}

/// Known Moza product IDs.
pub mod product_ids {
    // Wheelbases (V1)
    pub const R16_R21_V1: u16 = 0x0000;
    pub const R9_V1: u16 = 0x0002;
    pub const R5_V1: u16 = 0x0004;
    pub const R3_V1: u16 = 0x0005;
    pub const R12_V1: u16 = 0x0006;

    // Wheelbases (V2)
    pub const R16_R21_V2: u16 = 0x0010;
    pub const R9_V2: u16 = 0x0012;
    pub const R5_V2: u16 = 0x0014;
    pub const R3_V2: u16 = 0x0015;
    pub const R12_V2: u16 = 0x0016;

    // Peripherals
    pub const SR_P_PEDALS: u16 = 0x0003;
    pub const HGP_SHIFTER: u16 = 0x0020;
    pub const SGP_SHIFTER: u16 = 0x0021;
    pub const HBP_HANDBRAKE: u16 = 0x0022;
}

fn default_wheelbase_ks_map() -> KsReportMap {
    KsReportMap {
        report_id: Some(input_report::REPORT_ID),
        buttons_offset: Some(input_report::BUTTONS_START),
        hat_offset: Some(input_report::HAT_START),
        encoders: [None; KS_ENCODER_COUNT],
        clutch_left_axis: None,
        clutch_right_axis: None,
        clutch_combined_axis: None,
        clutch_left_button: None,
        clutch_right_button: None,
        clutch_mode_hint: KsClutchMode::Unknown,
        rotary_mode_hint: KsRotaryMode::Unknown,
        left_rotary_axis: Some(KsAxisSource::new(input_report::ROTARY_START, false)),
        right_rotary_axis: Some(KsAxisSource::new(input_report::ROTARY_START + 1, false)),
        joystick_mode_hint: KsJoystickMode::Unknown,
        joystick_hat: Some(KsByteSource::new(input_report::HAT_START)),
    }
}

/// Known Moza rim IDs when attached to a compatible wheelbase.
///
/// These are rim identity values reported through the wheelbase transport,
/// not standalone USB product IDs.
pub mod rim_ids {
    pub const CS_V2: u8 = 0x01;
    pub const GS_V2: u8 = 0x02;
    pub const RS_V2: u8 = 0x03;
    pub const FSR: u8 = 0x04;
    pub const KS: u8 = 0x05;
    pub const ES: u8 = 0x06;
}

/// ES control-surface dimensions documented by Moza.
pub const ES_BUTTON_COUNT: usize = 22;
pub const ES_LED_COUNT: usize = 10;

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
        // Vendor-documented incompatibility gate.
        product_ids::R9_V1 => MozaEsCompatibility::UnsupportedHardwareRevision,

        // Known compatible pairings used by Moza bundles and support guidance.
        product_ids::R5_V1 | product_ids::R5_V2 | product_ids::R9_V2 => {
            MozaEsCompatibility::Supported
        }

        // Wheelbases that require descriptor/capture confirmation in this codebase.
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

/// FFB mode options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfbMode {
    /// Keep FFB disabled.
    Off = 0xFF,
    /// Use vendor PID/PIDFF reporting mode.
    Standard = 0x00,
    /// Use raw direct torque mode.
    Direct = 0x02,
}

const MOZA_FFB_MODE_ENV: &str = "OPENRACING_MOZA_FFB_MODE";

fn parse_ffb_mode(value: &str) -> Option<FfbMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Some(FfbMode::Off),
        "standard" | "pidff" | "pid" => Some(FfbMode::Standard),
        "direct" | "raw" => Some(FfbMode::Direct),
        "0" => Some(FfbMode::Standard),
        "2" => Some(FfbMode::Direct),
        _ => None,
    }
}

fn default_ffb_mode() -> FfbMode {
    std::env::var(MOZA_FFB_MODE_ENV)
        .ok()
        .and_then(|value| parse_ffb_mode(&value))
        .unwrap_or(FfbMode::Standard)
}

/// Moza device model
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
    pub(crate) fn from_pid(pid: u16) -> Self {
        match pid {
            product_ids::R3_V1 | product_ids::R3_V2 => Self::R3,
            product_ids::R5_V1 | product_ids::R5_V2 => Self::R5,
            product_ids::R9_V1 | product_ids::R9_V2 => Self::R9,
            product_ids::R12_V1 | product_ids::R12_V2 => Self::R12,
            product_ids::R16_R21_V1 | product_ids::R16_R21_V2 => Self::R16, // R16/R21 share PID, differentiate by torque query
            product_ids::SR_P_PEDALS => Self::SrpPedals,
            _ => Self::Unknown,
        }
    }

    pub(crate) fn max_torque_nm(&self) -> f32 {
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
///
/// `throttle` and `brake` are required for SR-P Lite integration. `clutch` and
/// `handbrake` are optional and only present when the report length includes the
/// corresponding fields.
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

fn parse_axis(report: &[u8], start: usize) -> Option<u16> {
    if report.len() < start.saturating_add(2) {
        return None;
    }
    Some(u16::from_le_bytes([report[start], report[start + 1]]))
}

fn parse_axis_or_zero(report: &[u8], start: usize) -> u16 {
    parse_axis(report, start).unwrap_or(0)
}

/// Moza protocol handler
pub struct MozaProtocol {
    product_id: u16,
    model: MozaModel,
    is_v2: bool,
    init_state: AtomicU8,
    ffb_mode: FfbMode,
}

impl MozaProtocol {
    /// Create a new Moza protocol handler
    pub fn new(product_id: u16) -> Self {
        Self::new_with_ffb_mode(product_id, default_ffb_mode())
    }

    /// Create a new Moza protocol handler with explicit FFB mode.
    pub fn new_with_ffb_mode(product_id: u16, ffb_mode: FfbMode) -> Self {
        let is_v2 = (product_id & 0x0010) != 0;
        let model = MozaModel::from_pid(product_id);

        debug!(
            "Created MozaProtocol for PID 0x{:04X}, model: {:?}, V2: {}",
            product_id, model, is_v2
        );

        Self {
            product_id,
            model,
            is_v2,
            init_state: AtomicU8::new(MOZA_INIT_STATE_UNINITIALIZED),
            ffb_mode,
        }
    }

    /// Get current protocol init state.
    pub fn init_state(&self) -> MozaInitState {
        MozaInitState::from_u8(self.init_state.load(Ordering::Acquire))
    }

    /// Whether this protocol can emit native Moza output commands.
    fn is_output_capable(&self) -> bool {
        is_wheelbase_product(self.product_id)
    }

    fn try_enter_initialization(&self) -> bool {
        let mut state = self.init_state.load(Ordering::Acquire);
        loop {
            match state {
                MOZA_INIT_STATE_READY | MOZA_INIT_STATE_INITIALIZING => return false,
                _ => {
                    match self.init_state.compare_exchange(
                        state,
                        MOZA_INIT_STATE_INITIALIZING,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => return true,
                        Err(observed) => state = observed,
                    }
                }
            }
        }
    }

    fn finalize_initialization(&self, success: bool) {
        let next_state = if success {
            MOZA_INIT_STATE_READY
        } else {
            MOZA_INIT_STATE_FAILED
        };
        self.init_state.store(next_state, Ordering::Release);
    }

    /// Get the product ID
    pub fn product_id(&self) -> u16 {
        self.product_id
    }

    /// Get the device model
    pub fn model(&self) -> MozaModel {
        self.model
    }

    /// Selected mode for FFB initialization and reporting.
    pub fn ffb_mode(&self) -> FfbMode {
        self.ffb_mode
    }

    /// Get ES compatibility state for this wheelbase/product.
    pub fn es_compatibility(&self) -> MozaEsCompatibility {
        es_compatibility(self.product_id)
    }

    /// Parse pedal axis data from a wheelbase input report.
    ///
    /// SR-P Lite pedals are typically connected to the wheelbase pedal port,
    /// so their axis values are carried in the wheelbase input report rather
    /// than a standalone USB pedal device.
    pub fn parse_aggregated_pedal_axes(&self, report: &[u8]) -> Option<MozaPedalAxesRaw> {
        if report.first().copied() != Some(input_report::REPORT_ID) {
            return None;
        }

        let throttle = parse_axis(report, input_report::THROTTLE_START)?;
        let brake = parse_axis(report, input_report::BRAKE_START)?;
        let clutch = parse_axis(report, input_report::CLUTCH_START);
        let handbrake = parse_axis(report, input_report::HANDBRAKE_START);

        Some(MozaPedalAxesRaw {
            throttle,
            brake,
            clutch,
            handbrake,
        })
    }

    /// Parse a full Moza input report into `MozaInputState`.
    ///
    /// Axis offsets are stable in current repository documentation, while
    /// button/metadata fields are parsed conservatively for forward compatibility.
    pub fn parse_input_state(&self, report: &[u8]) -> Option<MozaInputState> {
        if report.first().copied() != Some(input_report::REPORT_ID) {
            if let Some(state) = self.parse_standalone_handbrake_state(report) {
                return Some(state);
            }

            return None;
        }

        let steering_u16 = parse_axis(report, input_report::STEERING_START)?;
        let throttle_u16 = parse_axis(report, input_report::THROTTLE_START)?;
        let brake_u16 = parse_axis(report, input_report::BRAKE_START)?;
        let clutch_u16 = parse_axis_or_zero(report, input_report::CLUTCH_START);
        let handbrake_u16 = parse_axis_or_zero(report, input_report::HANDBRAKE_START);

        let mut buttons = [0u8; 16];
        if report.len() >= input_report::BUTTONS_START + input_report::BUTTONS_LEN {
            buttons.copy_from_slice(
                &report[input_report::BUTTONS_START
                    ..input_report::BUTTONS_START + input_report::BUTTONS_LEN],
            );
        }

        let hat = report.get(input_report::HAT_START).copied().unwrap_or(0);
        let funky = report.get(input_report::FUNKY_START).copied().unwrap_or(0);

        let mut rotary = [0u8; 2];
        if report.len() >= input_report::ROTARY_START + input_report::ROTARY_LEN {
            rotary.copy_from_slice(
                &report[input_report::ROTARY_START
                    ..input_report::ROTARY_START + input_report::ROTARY_LEN],
            );
        }

        let ks_snapshot = if self.is_wheelbase() {
            default_wheelbase_ks_map()
                .parse(0, report)
                .unwrap_or_default()
        } else {
            crate::input::KsReportSnapshot::default()
        };

        Some(MozaInputState {
            steering_u16,
            throttle_u16,
            brake_u16,
            clutch_u16,
            handbrake_u16,
            buttons,
            hat,
            funky,
            rotary,
            ks_snapshot,
            tick: 0,
        })
    }

    fn is_standalone_handbrake(&self) -> bool {
        identify_device(self.product_id).category == MozaDeviceCategory::Handbrake
    }

    fn is_wheelbase(&self) -> bool {
        identify_device(self.product_id).category == MozaDeviceCategory::Wheelbase
    }

    fn parse_standalone_handbrake_state(&self, report: &[u8]) -> Option<MozaInputState> {
        if !self.is_standalone_handbrake() {
            return None;
        }

        let mut handbrake_u16 = None;
        let mut button_hint = None;

        if report.len() >= hbp_report::WITH_REPORT_ID_BUTTON + 1 && report[0] != input_report::REPORT_ID {
            handbrake_u16 = parse_axis(report, hbp_report::WITH_REPORT_ID_AXIS_START);
            button_hint = Some(report[hbp_report::WITH_REPORT_ID_BUTTON]);
        } else if report.len() == 2 && report[0] != input_report::REPORT_ID {
            handbrake_u16 = Some(u16::from_le_bytes([report[0], report[1]]));
        } else if report.len() >= hbp_report::RAW_BUTTON + 1 {
            handbrake_u16 = Some(u16::from_le_bytes([report[0], report[1]]));
            button_hint = Some(report[hbp_report::RAW_BUTTON]);
        }

        let mut state = MozaInputState::empty(0);
        state.handbrake_u16 = handbrake_u16?;
        if let Some(buttons) = button_hint {
            state.buttons[0] = buttons;
        }

        Some(state)
    }

    /// Enable high torque mode
    pub fn enable_high_torque(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Confirmed wheelbase handshake frame.
        let report = [report_ids::HIGH_TORQUE, 0x00, 0x00, 0x00];

        writer.write_feature_report(&report)?;
        info!("Enabled high torque mode for Moza {:?}", self.model);
        Ok(())
    }

    /// Start input reports
    pub fn start_input_reports(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Confirmed wheelbase handshake frame.
        let report = [report_ids::START_REPORTS, 0x00, 0x00, 0x00];

        writer.write_feature_report(&report)?;
        debug!("Started input reports for Moza {:?}", self.model);
        Ok(())
    }

    /// Set FFB mode
    pub fn set_ffb_mode(
        &self,
        writer: &mut dyn DeviceWriter,
        mode: FfbMode,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let report = [report_ids::FFB_MODE, mode as u8, 0x00, 0x00];

        writer.write_feature_report(&report)?;
        debug!("Set FFB mode to {:?} for Moza {:?}", mode, self.model);
        Ok(())
    }

    /// Set rotation range in degrees
    pub fn set_rotation_range(
        &self,
        writer: &mut dyn DeviceWriter,
        degrees: u16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let range_bytes = degrees.to_le_bytes();
        let report = [
            report_ids::ROTATION_RANGE,
            0x01, // Command: Set Range
            range_bytes[0],
            range_bytes[1],
        ];

        writer.write_feature_report(&report)?;
        debug!(
            "Set rotation range to {} degrees for Moza {:?}",
            degrees, self.model
        );
        Ok(())
    }

    /// Get encoder CPR based on model and hardware version
    fn encoder_cpr(&self) -> u32 {
        if self.is_v2 {
            match self.model {
                MozaModel::R16 | MozaModel::R21 => 2097152, // 21-bit
                _ => 262144,                                // 18-bit
            }
        } else {
            32768 // 15-bit for V1
        }
    }
}

impl VendorProtocol for MozaProtocol {
    fn initialize_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.is_output_capable() {
            debug!(
                "Skipping initialization for non-wheelbase Moza product: pid=0x{:04X}, model={:?}",
                self.product_id, self.model
            );
            return Ok(());
        }

        if !self.try_enter_initialization() {
            debug!(
                "Skipping Moza initialize while in-flight or already initialized: pid=0x{:04X}",
                self.product_id
            );
            return Ok(());
        }

        info!(
            "Initializing Moza {:?} (V{})",
            self.model,
            if self.is_v2 { 2 } else { 1 }
        );

        let mut success = true;

        match self.es_compatibility() {
            MozaEsCompatibility::UnsupportedHardwareRevision => warn!(
                "Moza PID 0x{:04X} is R9 V1; ES wheel compatibility is not supported",
                self.product_id
            ),
            MozaEsCompatibility::UnknownWheelbase => debug!(
                "Moza PID 0x{:04X} ES compatibility is not capture-validated",
                self.product_id
            ),
            MozaEsCompatibility::Supported | MozaEsCompatibility::NotWheelbase => {}
        }

        // Step 1: Enable high torque mode (unlocks FFB)
        if let Err(e) = self.enable_high_torque(writer) {
            warn!("Failed to enable high torque: {}", e);
            success = false;
        }

        // Step 2: Start input reports
        if let Err(e) = self.start_input_reports(writer) {
            warn!("Failed to start input reports: {}", e);
            success = false;
        }

        // Step 3: Set FFB to the configured mode.
        if let Err(e) = self.set_ffb_mode(writer, self.ffb_mode) {
            warn!("Failed to set FFB mode: {}", e);
            success = false;
        }

        self.finalize_initialization(success);
        if success {
            info!("Moza {:?} initialization complete", self.model);
        } else {
            warn!("Moza {:?} initialization incomplete; device not ready for native output", self.model);
        }
        Ok(())
    }

    fn send_feature_report(
        &self,
        writer: &mut dyn DeviceWriter,
        report_id: u8,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        const MAX_REPORT_BYTES: usize = 64;

        if data.len() + 1 > MAX_REPORT_BYTES {
            return Err(format!(
                "feature report payload too large: {} > {} bytes",
                data.len() + 1,
                MAX_REPORT_BYTES
            )
            .into());
        }

        let mut report = [0u8; MAX_REPORT_BYTES];
        report[0] = report_id;
        let end = data.len() + 1;
        report[1..end].copy_from_slice(data);
        writer.write_feature_report(&report[..end])?;
        Ok(())
    }

    fn get_ffb_config(&self) -> FfbConfig {
        FfbConfig {
            // Moza devices need conditional direction fix
            fix_conditional_direction: true,
            uses_vendor_usage_page: true,
            required_b_interval: Some(1), // 1ms for 1kHz
            max_torque_nm: self.model.max_torque_nm(),
            encoder_cpr: self.encoder_cpr(),
        }
    }

    fn is_v2_hardware(&self) -> bool {
        self.is_v2
    }

    fn output_report_id(&self) -> Option<u8> {
        if self.is_output_capable() {
            Some(report_ids::DIRECT_TORQUE)
        } else {
            None
        }
    }

    fn output_report_len(&self) -> Option<usize> {
        if self.is_output_capable() {
            Some(REPORT_LEN)
        } else {
            None
        }
    }
}
