//! FFBeast device settings and configuration protocol.
//!
//! FFBeast exposes device configuration through HID feature reports and
//! generic I/O reports (Report ID 0xA3). Settings include motor parameters,
//! force feedback tuning, GPIO/ADC extension configuration, and device
//! commands (reboot, save, DFU mode, center reset).
//!
//! # Report structure
//!
//! ## Generic I/O reports (Report ID 0xA3, 64 bytes)
//!
//! Used for both commands and settings writes:
//!
//! ```text
//! Byte 0: Command/data type (see ReportCmd)
//! Byte 1+: Payload (command-specific)
//! ```
//!
//! ## Feature reports (read-only settings)
//!
//! | Report ID | Settings group     |
//! |-----------|--------------------|
//! | 0x21      | Hardware settings  |
//! | 0x22      | Effect settings    |
//! | 0x25      | Firmware/license   |
//! | 0xA1      | GPIO extension     |
//! | 0xA2      | ADC extension      |
//!
//! # Sources
//!
//! - `shubham0x13/ffbeast-wheel-webhid-api` (MIT licensed)
//! - `wheel-api.ts`, `enums.ts`, `types.ts`, `constants.ts`

/// Report ID for generic input/output (commands + settings + state).
pub const GENERIC_IO_REPORT_ID: u8 = 0xA3;

/// Total size of HID reports in bytes.
pub const REPORT_SIZE: usize = 64;

/// Feature report ID for hardware settings.
pub const HARDWARE_SETTINGS_REPORT_ID: u8 = 0x21;

/// Feature report ID for effect settings.
pub const EFFECT_SETTINGS_REPORT_ID: u8 = 0x22;

/// Feature report ID for firmware license info.
pub const FIRMWARE_LICENSE_REPORT_ID: u8 = 0x25;

/// Feature report ID for GPIO extension settings.
pub const GPIO_SETTINGS_REPORT_ID: u8 = 0xA1;

/// Feature report ID for ADC extension settings.
pub const ADC_SETTINGS_REPORT_ID: u8 = 0xA2;

// ---------------------------------------------------------------------------
// Commands (sent via generic I/O report)
// ---------------------------------------------------------------------------

/// Command bytes placed at byte 0 of a generic I/O output report.
///
/// Source: `ReportData` enum in `enums.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ReportCmd {
    /// Reboot the controller (no save).
    Reboot = 0x01,
    /// Save settings to flash and reboot.
    SaveSettings = 0x02,
    /// Enter DFU mode for firmware update.
    DfuMode = 0x03,
    /// Reset wheel center to current position.
    ResetCenter = 0x04,
    /// Direct force override data.
    OverrideData = 0x10,
    /// Firmware activation/license data.
    FirmwareActivation = 0x13,
    /// Write a single settings field.
    SettingsField = 0x14,
}

// ---------------------------------------------------------------------------
// Settings field identifiers
// ---------------------------------------------------------------------------

/// Identifies a specific configuration field on the device.
///
/// Source: `SettingField` enum in `enums.ts` + `FIELD_TYPE_MAP`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SettingField {
    // -- Effect settings --
    /// DirectInput constant force direction (-1/0/+1).
    DirectXConstantDirection = 0,
    /// DirectInput spring strength scaler (0-255).
    DirectXSpringStrength = 1,
    /// DirectInput constant force strength scaler (0-255).
    DirectXConstantStrength = 2,
    /// DirectInput periodic strength scaler (0-255).
    DirectXPeriodicStrength = 3,
    /// Total effect strength scaler (0-255).
    TotalEffectStrength = 4,
    /// Motion range in degrees (u16).
    MotionRange = 5,
    /// Soft stop strength (0-255).
    SoftStopStrength = 6,
    /// Soft stop range in degrees (0-255).
    SoftStopRange = 7,
    /// Static dampening strength (u16).
    StaticDampeningStrength = 8,
    /// Soft stop dampening strength (u16).
    SoftStopDampeningStrength = 9,
    /// Force enabled flag (0/1).
    ForceEnabled = 11,
    /// Debug torque output flag (0/1).
    DebugTorque = 12,
    /// Amplifier gain setting (0-3, see AmplifierGain).
    AmplifierGain = 13,
    /// Motor calibration magnitude (0-255).
    CalibrationMagnitude = 15,
    /// Motor calibration speed (0-255).
    CalibrationSpeed = 16,
    /// Power limit percentage (0-255).
    PowerLimit = 17,
    /// Braking limit percentage (0-255).
    BrakingLimit = 18,
    /// Position smoothing (0-255).
    PositionSmoothing = 19,
    /// Speed buffer size (0-255).
    SpeedBufferSize = 20,
    /// Encoder direction (-1/+1).
    EncoderDirection = 21,
    /// Force direction (-1/+1).
    ForceDirection = 22,
    /// Motor pole pairs (u8).
    PolePairs = 23,
    /// Encoder counts per revolution (u16).
    EncoderCPR = 24,
    /// PID proportional gain (0-255).
    PGain = 25,
    /// PID integral gain (u16).
    IGain = 26,
    /// Extension mode (None=0, Custom=1).
    ExtensionMode = 27,
    /// GPIO pin mode (see PinMode).
    PinMode = 28,
    /// Button mode (see ButtonMode).
    ButtonMode = 29,
    /// SPI communication mode (0-3).
    SpiMode = 30,
    /// SPI latch direction (0=up, 1=down).
    SpiLatchMode = 31,
    /// SPI latch delay (0-255).
    SpiLatchDelay = 32,
    /// SPI clock pulse length (0-255).
    SpiClkPulseLength = 33,
    /// ADC minimum dead zone (u16).
    AdcMinDeadZone = 34,
    /// ADC maximum dead zone (u16).
    AdcMaxDeadZone = 35,
    /// ADC-to-button low threshold (0-255).
    AdcToButtonLow = 36,
    /// ADC-to-button high threshold (0-255).
    AdcToButtonHigh = 37,
    /// ADC smoothing (0-255).
    AdcSmoothing = 38,
    /// ADC axis invert flag (0/1).
    AdcInvert = 39,
    /// Reset center on Z=0 flag.
    ResetCenterOnZ0 = 41,
    /// Integrated spring strength (0-255).
    IntegratedSpringStrength = 43,
}

/// The data type used to encode a settings field value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// Signed 8-bit.
    I8,
    /// Unsigned 8-bit.
    U8,
    /// Signed 16-bit little-endian.
    I16,
    /// Unsigned 16-bit little-endian.
    U16,
}

impl SettingField {
    /// Get the wire type for this field.
    ///
    /// Source: `FIELD_TYPE_MAP` in `enums.ts`.
    pub fn field_type(self) -> FieldType {
        match self {
            // Signed 8-bit
            Self::DirectXConstantDirection | Self::EncoderDirection | Self::ForceDirection => {
                FieldType::I8
            }

            // Unsigned 16-bit
            Self::MotionRange
            | Self::StaticDampeningStrength
            | Self::SoftStopDampeningStrength
            | Self::EncoderCPR
            | Self::IGain
            | Self::AdcMinDeadZone
            | Self::AdcMaxDeadZone => FieldType::U16,

            // Everything else is unsigned 8-bit
            _ => FieldType::U8,
        }
    }
}

// ---------------------------------------------------------------------------
// Amplifier gain enum
// ---------------------------------------------------------------------------

/// Amplifier gain presets.
///
/// Source: `AmplifierGain` enum in `enums.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AmplifierGain {
    /// 80V/V gain.
    Gain80 = 0,
    /// 40V/V gain.
    Gain40 = 1,
    /// 20V/V gain.
    Gain20 = 2,
    /// 10V/V gain.
    Gain10 = 3,
}

// ---------------------------------------------------------------------------
// Command builders
// ---------------------------------------------------------------------------

/// Build a "reboot" command (64-byte generic I/O report).
pub fn build_reboot_command() -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = ReportCmd::Reboot as u8;
    buf
}

/// Build a "save settings and reboot" command.
pub fn build_save_and_reboot() -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = ReportCmd::SaveSettings as u8;
    buf
}

/// Build a "switch to DFU mode" command.
pub fn build_dfu_mode() -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = ReportCmd::DfuMode as u8;
    buf
}

/// Build a "reset wheel center" command.
pub fn build_reset_center() -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = ReportCmd::ResetCenter as u8;
    buf
}

// ---------------------------------------------------------------------------
// Settings field write
// ---------------------------------------------------------------------------

/// Build a settings field write command.
///
/// # Wire format
/// ```text
/// Byte 0: 0x14 (SettingsField command)
/// Byte 1: field ID
/// Byte 2: sub-index (for array fields like pin modes)
/// Byte 3+: value (i8, u8, i16 LE, or u16 LE depending on field type)
/// ```
pub fn build_settings_write(field: SettingField, index: u8, value: i32) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = ReportCmd::SettingsField as u8;
    buf[1] = field as u8;
    buf[2] = index;

    match field.field_type() {
        FieldType::I8 => {
            let clamped = value.clamp(-128, 127) as i8;
            buf[3] = clamped as u8;
        }
        FieldType::U8 => {
            let clamped = value.clamp(0, 255) as u8;
            buf[3] = clamped;
        }
        FieldType::I16 => {
            let clamped = value.clamp(-32768, 32767) as i16;
            let bytes = clamped.to_le_bytes();
            buf[3] = bytes[0];
            buf[4] = bytes[1];
        }
        FieldType::U16 => {
            let clamped = value.clamp(0, 65535) as u16;
            let bytes = clamped.to_le_bytes();
            buf[3] = bytes[0];
            buf[4] = bytes[1];
        }
    }

    buf
}

// ---------------------------------------------------------------------------
// Direct force override
// ---------------------------------------------------------------------------

/// Direct force control values for real-time override.
///
/// All force values are in range [-10000, 10000].
/// `force_drop` is in range [0, 100] (percentage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DirectControl {
    /// Spring force component.
    pub spring_force: i16,
    /// Constant force component.
    pub constant_force: i16,
    /// Periodic force component.
    pub periodic_force: i16,
    /// Force drop percentage (0-100).
    pub force_drop: u8,
}

/// Build a direct force override report.
///
/// # Wire format
/// ```text
/// Byte 0: 0x10 (OverrideData command)
/// Bytes 1-2: spring force (i16 LE, [-10000, 10000])
/// Bytes 3-4: constant force (i16 LE, [-10000, 10000])
/// Bytes 5-6: periodic force (i16 LE, [-10000, 10000])
/// Byte 7: force drop (0-100)
/// ```
pub fn build_direct_control(ctrl: &DirectControl) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = ReportCmd::OverrideData as u8;

    let spring = ctrl.spring_force.clamp(-10000, 10000);
    let constant = ctrl.constant_force.clamp(-10000, 10000);
    let periodic = ctrl.periodic_force.clamp(-10000, 10000);
    let drop = ctrl.force_drop.min(100);

    buf[1..3].copy_from_slice(&spring.to_le_bytes());
    buf[3..5].copy_from_slice(&constant.to_le_bytes());
    buf[5..7].copy_from_slice(&periodic.to_le_bytes());
    buf[7] = drop;

    buf
}

// ---------------------------------------------------------------------------
// Feature report parsers
// ---------------------------------------------------------------------------

/// Parsed effect settings (from feature report 0x22).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectSettings {
    /// Motion range in degrees.
    pub motion_range: u16,
    /// Static dampening strength.
    pub static_dampening_strength: u16,
    /// Soft stop dampening strength.
    pub soft_stop_dampening_strength: u16,
    /// Total effect strength scaler (0-255).
    pub total_effect_strength: u8,
    /// Integrated spring strength (0-255).
    pub integrated_spring_strength: u8,
    /// Soft stop range in degrees.
    pub soft_stop_range: u8,
    /// Soft stop strength.
    pub soft_stop_strength: u8,
    /// DirectX constant direction (-1/0/+1).
    pub dx_constant_direction: i8,
    /// DirectX spring strength scaler.
    pub dx_spring_strength: u8,
    /// DirectX constant strength scaler.
    pub dx_constant_strength: u8,
    /// DirectX periodic strength scaler.
    pub dx_periodic_strength: u8,
}

/// Parse effect settings from a feature report payload.
///
/// Expects the payload **after** stripping the report ID byte.
/// Minimum 13 bytes required.
pub fn parse_effect_settings(buf: &[u8]) -> Option<EffectSettings> {
    if buf.len() < 13 {
        return None;
    }
    Some(EffectSettings {
        motion_range: u16::from_le_bytes([buf[0], buf[1]]),
        static_dampening_strength: u16::from_le_bytes([buf[2], buf[3]]),
        soft_stop_dampening_strength: u16::from_le_bytes([buf[4], buf[5]]),
        total_effect_strength: buf[6],
        integrated_spring_strength: buf[7],
        soft_stop_range: buf[8],
        soft_stop_strength: buf[9],
        dx_constant_direction: buf[10] as i8,
        dx_spring_strength: buf[11],
        dx_constant_strength: buf[12],
        dx_periodic_strength: if buf.len() > 13 { buf[13] } else { 0 },
    })
}

/// Parsed hardware settings (from feature report 0x21).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardwareSettings {
    /// Encoder counts per revolution.
    pub encoder_cpr: u16,
    /// PID integral gain.
    pub integral_gain: u16,
    /// PID proportional gain (0-255).
    pub proportional_gain: u8,
    /// Force output enabled flag.
    pub force_enabled: u8,
    /// Debug torque output flag.
    pub debug_torque: u8,
    /// Amplifier gain preset (0-3).
    pub amplifier_gain: u8,
    /// Calibration magnitude.
    pub calibration_magnitude: u8,
    /// Calibration speed.
    pub calibration_speed: u8,
    /// Power limit percentage.
    pub power_limit: u8,
    /// Braking limit percentage.
    pub braking_limit: u8,
    /// Position smoothing.
    pub position_smoothing: u8,
    /// Speed buffer size.
    pub speed_buffer_size: u8,
    /// Encoder direction (-1/+1).
    pub encoder_direction: i8,
    /// Force direction (-1/+1).
    pub force_direction: i8,
    /// Motor pole pairs.
    pub pole_pairs: u8,
}

/// Parse hardware settings from a feature report payload.
///
/// Expects the payload **after** stripping the report ID byte.
/// Minimum 15 bytes required.
pub fn parse_hardware_settings(buf: &[u8]) -> Option<HardwareSettings> {
    if buf.len() < 15 {
        return None;
    }
    Some(HardwareSettings {
        encoder_cpr: u16::from_le_bytes([buf[0], buf[1]]),
        integral_gain: u16::from_le_bytes([buf[2], buf[3]]),
        proportional_gain: buf[4],
        force_enabled: buf[5],
        debug_torque: buf[6],
        amplifier_gain: buf[7],
        calibration_magnitude: buf[8],
        calibration_speed: buf[9],
        power_limit: buf[10],
        braking_limit: buf[11],
        position_smoothing: buf[12],
        speed_buffer_size: buf[13],
        encoder_direction: buf[14] as i8,
        force_direction: if buf.len() > 15 { buf[15] as i8 } else { 0 },
        pole_pairs: if buf.len() > 16 { buf[16] } else { 0 },
    })
}

/// Parsed firmware license info (from feature report 0x25).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareLicense {
    /// Firmware release type.
    pub release_type: u8,
    /// Firmware major version.
    pub major: u8,
    /// Firmware minor version.
    pub minor: u8,
    /// Firmware patch version.
    pub patch: u8,
    /// Serial key (3 × u32).
    pub serial_key: [u32; 3],
    /// Device ID (3 × u32).
    pub device_id: [u32; 3],
    /// Registration status (0 = unregistered, 1 = registered).
    pub is_registered: u8,
}

/// Parse firmware license from a feature report payload.
///
/// Minimum 29 bytes required (4 + 12 + 12 + 1).
pub fn parse_firmware_license(buf: &[u8]) -> Option<FirmwareLicense> {
    if buf.len() < 29 {
        return None;
    }

    let read_u32 = |offset: usize| -> u32 {
        u32::from_le_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
        ])
    };

    Some(FirmwareLicense {
        release_type: buf[0],
        major: buf[1],
        minor: buf[2],
        patch: buf[3],
        serial_key: [read_u32(4), read_u32(8), read_u32(12)],
        device_id: [read_u32(16), read_u32(20), read_u32(24)],
        is_registered: buf[28],
    })
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Constants match WebHID API
    // -----------------------------------------------------------------------

    #[test]
    fn report_ids_match_webhid_api() {
        assert_eq!(GENERIC_IO_REPORT_ID, 0xA3);
        assert_eq!(HARDWARE_SETTINGS_REPORT_ID, 0x21);
        assert_eq!(EFFECT_SETTINGS_REPORT_ID, 0x22);
        assert_eq!(FIRMWARE_LICENSE_REPORT_ID, 0x25);
        assert_eq!(GPIO_SETTINGS_REPORT_ID, 0xA1);
        assert_eq!(ADC_SETTINGS_REPORT_ID, 0xA2);
    }

    #[test]
    fn report_cmd_values_match_webhid() {
        assert_eq!(ReportCmd::Reboot as u8, 0x01);
        assert_eq!(ReportCmd::SaveSettings as u8, 0x02);
        assert_eq!(ReportCmd::DfuMode as u8, 0x03);
        assert_eq!(ReportCmd::ResetCenter as u8, 0x04);
        assert_eq!(ReportCmd::OverrideData as u8, 0x10);
        assert_eq!(ReportCmd::FirmwareActivation as u8, 0x13);
        assert_eq!(ReportCmd::SettingsField as u8, 0x14);
    }

    #[test]
    fn report_size_is_64() {
        assert_eq!(REPORT_SIZE, 64);
    }

    // -----------------------------------------------------------------------
    // Setting field types
    // -----------------------------------------------------------------------

    #[test]
    fn signed_fields_are_i8() {
        assert_eq!(
            SettingField::DirectXConstantDirection.field_type(),
            FieldType::I8
        );
        assert_eq!(SettingField::EncoderDirection.field_type(), FieldType::I8);
        assert_eq!(SettingField::ForceDirection.field_type(), FieldType::I8);
    }

    #[test]
    fn u16_fields() {
        assert_eq!(SettingField::MotionRange.field_type(), FieldType::U16);
        assert_eq!(SettingField::EncoderCPR.field_type(), FieldType::U16);
        assert_eq!(SettingField::IGain.field_type(), FieldType::U16);
        assert_eq!(
            SettingField::StaticDampeningStrength.field_type(),
            FieldType::U16
        );
    }

    #[test]
    fn default_fields_are_u8() {
        assert_eq!(
            SettingField::TotalEffectStrength.field_type(),
            FieldType::U8
        );
        assert_eq!(SettingField::PowerLimit.field_type(), FieldType::U8);
        assert_eq!(SettingField::SpiMode.field_type(), FieldType::U8);
    }

    // -----------------------------------------------------------------------
    // Command builders
    // -----------------------------------------------------------------------

    #[test]
    fn reboot_command_byte_0() {
        let buf = build_reboot_command();
        assert_eq!(buf[0], ReportCmd::Reboot as u8);
        assert_eq!(buf.len(), REPORT_SIZE);
    }

    #[test]
    fn save_and_reboot_command() {
        let buf = build_save_and_reboot();
        assert_eq!(buf[0], ReportCmd::SaveSettings as u8);
    }

    #[test]
    fn dfu_mode_command() {
        let buf = build_dfu_mode();
        assert_eq!(buf[0], ReportCmd::DfuMode as u8);
    }

    #[test]
    fn reset_center_command() {
        let buf = build_reset_center();
        assert_eq!(buf[0], ReportCmd::ResetCenter as u8);
    }

    #[test]
    fn commands_are_64_bytes() {
        assert_eq!(build_reboot_command().len(), 64);
        assert_eq!(build_save_and_reboot().len(), 64);
        assert_eq!(build_dfu_mode().len(), 64);
        assert_eq!(build_reset_center().len(), 64);
    }

    #[test]
    fn commands_zero_padded() {
        let buf = build_reboot_command();
        for b in &buf[1..] {
            assert_eq!(*b, 0);
        }
    }

    // -----------------------------------------------------------------------
    // Settings field write
    // -----------------------------------------------------------------------

    #[test]
    fn settings_write_header() {
        let buf = build_settings_write(SettingField::MotionRange, 0, 900);
        assert_eq!(buf[0], ReportCmd::SettingsField as u8);
        assert_eq!(buf[1], SettingField::MotionRange as u8);
        assert_eq!(buf[2], 0); // index
    }

    #[test]
    fn settings_write_u16_value() {
        let buf = build_settings_write(SettingField::MotionRange, 0, 900);
        let val = u16::from_le_bytes([buf[3], buf[4]]);
        assert_eq!(val, 900);
    }

    #[test]
    fn settings_write_u8_value() {
        let buf = build_settings_write(SettingField::PowerLimit, 0, 80);
        assert_eq!(buf[3], 80);
    }

    #[test]
    fn settings_write_i8_value() {
        let buf = build_settings_write(SettingField::EncoderDirection, 0, -1);
        assert_eq!(buf[3] as i8, -1);
    }

    #[test]
    fn settings_write_clamps_u8_overflow() {
        let buf = build_settings_write(SettingField::PowerLimit, 0, 999);
        assert_eq!(buf[3], 255);
    }

    #[test]
    fn settings_write_clamps_u16_overflow() {
        let buf = build_settings_write(SettingField::MotionRange, 0, 70000);
        let val = u16::from_le_bytes([buf[3], buf[4]]);
        assert_eq!(val, 65535);
    }

    #[test]
    fn settings_write_clamps_i8_negative() {
        let buf = build_settings_write(SettingField::DirectXConstantDirection, 0, -200);
        assert_eq!(buf[3] as i8, -128);
    }

    #[test]
    fn settings_write_with_index() {
        let buf = build_settings_write(SettingField::PinMode, 5, 6);
        assert_eq!(buf[2], 5);
        assert_eq!(buf[3], 6);
    }

    // -----------------------------------------------------------------------
    // Direct control
    // -----------------------------------------------------------------------

    #[test]
    fn direct_control_header() {
        let buf = build_direct_control(&DirectControl::default());
        assert_eq!(buf[0], ReportCmd::OverrideData as u8);
    }

    #[test]
    fn direct_control_forces() {
        let ctrl = DirectControl {
            spring_force: 5000,
            constant_force: -3000,
            periodic_force: 1000,
            force_drop: 50,
        };
        let buf = build_direct_control(&ctrl);
        assert_eq!(i16::from_le_bytes([buf[1], buf[2]]), 5000);
        assert_eq!(i16::from_le_bytes([buf[3], buf[4]]), -3000);
        assert_eq!(i16::from_le_bytes([buf[5], buf[6]]), 1000);
        assert_eq!(buf[7], 50);
    }

    #[test]
    fn direct_control_clamps_spring() {
        let ctrl = DirectControl {
            spring_force: 20000,
            ..DirectControl::default()
        };
        let buf = build_direct_control(&ctrl);
        assert_eq!(i16::from_le_bytes([buf[1], buf[2]]), 10000);
    }

    #[test]
    fn direct_control_clamps_drop() {
        let ctrl = DirectControl {
            force_drop: 200,
            ..DirectControl::default()
        };
        let buf = build_direct_control(&ctrl);
        assert_eq!(buf[7], 100);
    }

    #[test]
    fn direct_control_size() {
        let buf = build_direct_control(&DirectControl::default());
        assert_eq!(buf.len(), REPORT_SIZE);
    }

    // -----------------------------------------------------------------------
    // Effect settings parser
    // -----------------------------------------------------------------------

    #[test]
    fn parse_effect_settings_valid() {
        let mut buf = [0u8; 14];
        // motion_range = 900
        buf[0..2].copy_from_slice(&900u16.to_le_bytes());
        // static_dampening = 500
        buf[2..4].copy_from_slice(&500u16.to_le_bytes());
        // soft_stop_dampening = 300
        buf[4..6].copy_from_slice(&300u16.to_le_bytes());
        buf[6] = 80; // total_effect_strength
        buf[7] = 40; // integrated_spring
        buf[8] = 10; // soft_stop_range
        buf[9] = 60; // soft_stop_strength
        buf[10] = (-1i8) as u8; // dx_constant_direction
        buf[11] = 100; // dx_spring
        buf[12] = 100; // dx_constant
        buf[13] = 100; // dx_periodic

        let settings = parse_effect_settings(&buf);
        assert!(settings.is_some());
        let s = settings.as_ref();
        assert_eq!(s.map(|v| v.motion_range), Some(900));
        assert_eq!(s.map(|v| v.total_effect_strength), Some(80));
        assert_eq!(s.map(|v| v.dx_constant_direction), Some(-1));
    }

    #[test]
    fn parse_effect_settings_too_short() {
        assert_eq!(parse_effect_settings(&[0u8; 12]), None);
    }

    // -----------------------------------------------------------------------
    // Hardware settings parser
    // -----------------------------------------------------------------------

    #[test]
    fn parse_hardware_settings_valid() {
        let mut buf = [0u8; 17];
        // encoder_cpr = 4096
        buf[0..2].copy_from_slice(&4096u16.to_le_bytes());
        // integral_gain = 200
        buf[2..4].copy_from_slice(&200u16.to_le_bytes());
        buf[4] = 50; // proportional_gain
        buf[5] = 1; // force_enabled
        buf[6] = 0; // debug_torque
        buf[7] = 0; // amplifier_gain (80V)
        buf[8] = 30; // cal_magnitude
        buf[9] = 10; // cal_speed
        buf[10] = 100; // power_limit
        buf[11] = 80; // braking_limit
        buf[12] = 5; // position_smoothing
        buf[13] = 8; // speed_buffer_size
        buf[14] = (-1i8) as u8; // encoder_direction
        buf[15] = 1; // force_direction
        buf[16] = 7; // pole_pairs

        let settings = parse_hardware_settings(&buf);
        assert!(settings.is_some());
        let s = settings.as_ref();
        assert_eq!(s.map(|v| v.encoder_cpr), Some(4096));
        assert_eq!(s.map(|v| v.force_enabled), Some(1));
        assert_eq!(s.map(|v| v.encoder_direction), Some(-1));
        assert_eq!(s.map(|v| v.pole_pairs), Some(7));
    }

    #[test]
    fn parse_hardware_settings_too_short() {
        assert_eq!(parse_hardware_settings(&[0u8; 14]), None);
    }

    // -----------------------------------------------------------------------
    // Firmware license parser
    // -----------------------------------------------------------------------

    #[test]
    fn parse_firmware_license_valid() {
        let mut buf = [0u8; 29];
        buf[0] = 1; // release_type
        buf[1] = 2; // major
        buf[2] = 3; // minor
        buf[3] = 4; // patch
        buf[4..8].copy_from_slice(&0xAABBCCDDu32.to_le_bytes());
        buf[8..12].copy_from_slice(&0x11223344u32.to_le_bytes());
        buf[12..16].copy_from_slice(&0x55667788u32.to_le_bytes());
        buf[16..20].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
        buf[20..24].copy_from_slice(&0xCAFEBABEu32.to_le_bytes());
        buf[24..28].copy_from_slice(&0x12345678u32.to_le_bytes());
        buf[28] = 1; // registered

        let license = parse_firmware_license(&buf);
        assert!(license.is_some());
        let l = license.as_ref();
        assert_eq!(l.map(|v| v.major), Some(2));
        assert_eq!(l.map(|v| v.minor), Some(3));
        assert_eq!(l.map(|v| v.patch), Some(4));
        assert_eq!(l.map(|v| v.serial_key[0]), Some(0xAABBCCDD));
        assert_eq!(l.map(|v| v.device_id[0]), Some(0xDEADBEEF));
        assert_eq!(l.map(|v| v.is_registered), Some(1));
    }

    #[test]
    fn parse_firmware_license_too_short() {
        assert_eq!(parse_firmware_license(&[0u8; 28]), None);
    }

    // -----------------------------------------------------------------------
    // Amplifier gain enum values
    // -----------------------------------------------------------------------

    #[test]
    fn amplifier_gain_values() {
        assert_eq!(AmplifierGain::Gain80 as u8, 0);
        assert_eq!(AmplifierGain::Gain40 as u8, 1);
        assert_eq!(AmplifierGain::Gain20 as u8, 2);
        assert_eq!(AmplifierGain::Gain10 as u8, 3);
    }
}
