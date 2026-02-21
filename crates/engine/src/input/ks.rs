//! KS control-surface parsing and normalization.
//!
//! The parser is map-driven: report offsets and encoding are described by
//! `KsReportMap` rather than hard-coded per firmware layout.

/// Number of encoder-like slots in a generic KS snapshot.
pub const KS_ENCODER_COUNT: usize = 8;
/// Number of packed button bytes in a normalized KS snapshot.
pub const KS_BUTTON_BYTES: usize = 16;

/// Supported clutch interpretation modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KsClutchMode {
    /// No reliable clutch layout discovered.
    Unknown,
    /// Combined clutch axis is mapped directly.
    CombinedAxis,
    /// Independent left/right clutch axes are mapped.
    IndependentAxis,
    /// Clutch values are exposed as two buttons.
    Button,
}

impl Default for KsClutchMode {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Supported rotary modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KsRotaryMode {
    /// No reliable rotary layout discovered.
    Unknown,
    /// Rotary input reports as discrete button edges/deltas.
    Button,
    /// Rotary input reports as continuous knobs.
    Knob,
}

impl Default for KsRotaryMode {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Supported joystick modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KsJoystickMode {
    /// No reliable joystick mode discovered.
    Unknown,
    /// Joystick reports as buttons.
    Button,
    /// Joystick reports as D-pad hat direction.
    DPad,
}

impl Default for KsJoystickMode {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Source of a 16-bit integer payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KsAxisSource {
    /// Start byte offset in the report payload.
    pub offset: usize,
    /// True when the payload is signed 16-bit little-endian.
    pub signed: bool,
}

impl KsAxisSource {
    pub const fn new(offset: usize, signed: bool) -> Self {
        Self { offset, signed }
    }

    fn parse_bytes(report: &[u8], offset: usize) -> Option<[u8; 2]> {
        if report.len() < offset.saturating_add(2) {
            return None;
        }

        Some([report[offset], report[offset + 1]])
    }

    pub fn parse_u16(&self, report: &[u8]) -> Option<u16> {
        let bytes = Self::parse_bytes(report, self.offset)?;
        Some(u16::from_le_bytes(bytes))
    }

    pub fn parse_i16(&self, report: &[u8]) -> Option<i16> {
        let bytes = Self::parse_bytes(report, self.offset)?;
        if self.signed {
            Some(i16::from_le_bytes(bytes))
        } else {
            Some(i16::from_le_bytes(bytes))
        }
    }
}

/// Source of a single bit in a report byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KsBitSource {
    /// Source byte offset in the report payload.
    pub offset: usize,
    /// One-hot bit mask.
    pub mask: u8,
    /// Whether active state is represented by logical inversion.
    pub invert: bool,
}

impl KsBitSource {
    pub const fn new(offset: usize, mask: u8) -> Self {
        Self {
            offset,
            mask,
            invert: false,
        }
    }

    pub const fn with_invert(offset: usize, mask: u8) -> Self {
        Self {
            offset,
            mask,
            invert: true,
        }
    }

    pub const fn inverted(offset: usize, mask: u8) -> Self {
        Self::with_invert(offset, mask)
    }

    pub fn parse(&self, report: &[u8]) -> Option<bool> {
        if report.len() <= self.offset {
            return None;
        }

        let active = report[self.offset] & self.mask != 0;
        Some(if self.invert { !active } else { active })
    }
}

/// Source of a single report byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KsByteSource {
    /// Source byte offset in the report payload.
    pub offset: usize,
}

impl KsByteSource {
    pub const fn new(offset: usize) -> Self {
        Self { offset }
    }

    pub fn parse(&self, report: &[u8]) -> Option<u8> {
        report.get(self.offset).copied()
    }
}

/// Mode-driven normalization snapshot for a KS control surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KsReportSnapshot {
    /// Frame marker from a sequence counter at parse time.
    pub tick: u32,
    /// Raw packed button bitmap bytes (16 bytes for historical compatibility).
    pub buttons: [u8; KS_BUTTON_BYTES],
    /// Joystick/hat direction source.
    pub hat: u8,
    /// Encoders/rotary axis-like channels as signed 16-bit values.
    pub encoders: [i16; KS_ENCODER_COUNT],
    /// Combined clutch axis, when available.
    pub clutch_combined: Option<u16>,
    /// Left clutch axis, when available.
    pub clutch_left: Option<u16>,
    /// Right clutch axis, when available.
    pub clutch_right: Option<u16>,
    /// Left clutch button state, when mode maps to buttons.
    pub clutch_left_button: Option<bool>,
    /// Right clutch button state, when mode maps to buttons.
    pub clutch_right_button: Option<bool>,
    /// Resolved clutch interpretation mode.
    pub clutch_mode: KsClutchMode,
    /// Resolved rotary interpretation mode.
    pub rotary_mode: KsRotaryMode,
    /// Resolved joystick interpretation mode.
    pub joystick_mode: KsJoystickMode,
}

impl Default for KsReportSnapshot {
    fn default() -> Self {
        Self {
            tick: 0,
            buttons: [0u8; KS_BUTTON_BYTES],
            hat: 0,
            encoders: [0i16; KS_ENCODER_COUNT],
            clutch_combined: None,
            clutch_left: None,
            clutch_right: None,
            clutch_left_button: None,
            clutch_right_button: None,
            clutch_mode: KsClutchMode::Unknown,
            rotary_mode: KsRotaryMode::Unknown,
            joystick_mode: KsJoystickMode::Unknown,
        }
    }
}

impl KsReportSnapshot {
    /// Check whether both clutches are active in a mode-safe way.
    ///
    /// Returns `None` when no reliable clutch data is available in the snapshot.
    pub fn both_clutches_pressed(&self, threshold: u16) -> Option<bool> {
        match self.clutch_mode {
            KsClutchMode::CombinedAxis => self.clutch_combined.map(|value| value >= threshold),
            KsClutchMode::IndependentAxis => self
                .clutch_left
                .zip(self.clutch_right)
                .map(|(left, right)| left >= threshold && right >= threshold),
            KsClutchMode::Button => self
                .clutch_left_button
                .zip(self.clutch_right_button)
                .map(|(left, right)| left && right),
            KsClutchMode::Unknown => None,
        }
    }

    /// Conservative constructor for a populated wheelbase surface without KS map bindings.
    pub fn from_common_controls(tick: u32, buttons: [u8; KS_BUTTON_BYTES], hat: u8) -> Self {
        Self {
            tick,
            buttons,
            hat,
            ..Self::default()
        }
    }
}

/// Capture-driven map from raw report bytes to KS semantic channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KsReportMap {
    /// Expected report ID, when known.
    pub report_id: Option<u8>,
    /// Offset of 16-button bitfield bytes.
    pub buttons_offset: Option<usize>,
    /// Offset of hat-style byte.
    pub hat_offset: Option<usize>,
    /// Axis-like encoded fields.
    pub encoders: [Option<KsAxisSource>; KS_ENCODER_COUNT],
    /// Clutch layout map.
    pub clutch_left_axis: Option<KsAxisSource>,
    pub clutch_right_axis: Option<KsAxisSource>,
    pub clutch_combined_axis: Option<KsAxisSource>,
    pub clutch_left_button: Option<KsBitSource>,
    pub clutch_right_button: Option<KsBitSource>,
    /// In-band clutch mode hints (used when both axis and buttons are present).
    pub clutch_mode_hint: KsClutchMode,
    /// Rotary layout hints.
    pub rotary_mode_hint: KsRotaryMode,
    pub left_rotary_axis: Option<KsAxisSource>,
    pub right_rotary_axis: Option<KsAxisSource>,
    /// Joystick mode hints.
    pub joystick_mode_hint: KsJoystickMode,
    /// Optional joystick hat source.
    pub joystick_hat: Option<KsByteSource>,
}

impl KsReportMap {
    /// Empty map for unsupported layouts.
    pub const fn empty() -> Self {
        Self {
            report_id: None,
            buttons_offset: None,
            hat_offset: None,
            encoders: [None; KS_ENCODER_COUNT],
            clutch_left_axis: None,
            clutch_right_axis: None,
            clutch_combined_axis: None,
            clutch_left_button: None,
            clutch_right_button: None,
            clutch_mode_hint: KsClutchMode::Unknown,
            rotary_mode_hint: KsRotaryMode::Unknown,
            left_rotary_axis: None,
            right_rotary_axis: None,
            joystick_mode_hint: KsJoystickMode::Unknown,
            joystick_hat: None,
        }
    }

    /// Parse a raw report into a normalized snapshot using this map.
    pub fn parse(&self, tick: u32, report: &[u8]) -> Option<KsReportSnapshot> {
        if let Some(expected_report_id) = self.report_id {
            if report.first().copied() != Some(expected_report_id) {
                return None;
            }
        }

        let mut snapshot = KsReportSnapshot::default();
        snapshot.tick = tick;
        snapshot.clutch_mode = self.clutch_mode_hint;
        snapshot.rotary_mode = self.rotary_mode_hint;
        snapshot.joystick_mode = self.joystick_mode_hint;

        if let Some(offset) = self.buttons_offset {
            let button_len = snapshot.buttons.len();
            if report.len() >= offset.saturating_add(button_len) {
                snapshot
                    .buttons
                    .copy_from_slice(&report[offset..offset + button_len]);
            } else {
                let available = report.len().saturating_sub(offset);
                snapshot.buttons[..available].copy_from_slice(&report[offset..offset + available]);
            }
        }

        snapshot.hat = self
            .joystick_hat
            .and_then(|source| source.parse(report))
            .or_else(|| {
                self.hat_offset
                    .and_then(|offset| report.get(offset).copied())
            })
            .unwrap_or_default();

        for i in 0..KS_ENCODER_COUNT {
            if let Some(axis) = self.encoders[i] {
                snapshot.encoders[i] = axis.parse_i16(report).unwrap_or(0);
            }
        }

        if let Some(axis) = self.left_rotary_axis {
            snapshot.encoders[0] = axis.parse_i16(report).unwrap_or(0);
        }
        if let Some(axis) = self.right_rotary_axis {
            snapshot.encoders[1] = axis.parse_i16(report).unwrap_or(0);
        }

        snapshot.clutch_combined = self
            .clutch_combined_axis
            .and_then(|axis| axis.parse_u16(report));
        snapshot.clutch_left = self
            .clutch_left_axis
            .and_then(|axis| axis.parse_u16(report));
        snapshot.clutch_right = self
            .clutch_right_axis
            .and_then(|axis| axis.parse_u16(report));
        snapshot.clutch_left_button = self.clutch_left_button.and_then(|bit| bit.parse(report));
        snapshot.clutch_right_button = self.clutch_right_button.and_then(|bit| bit.parse(report));

        Some(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ks_snapshot_combined_axis_mode_detects_pressed() {
        let mut snapshot = KsReportSnapshot::default();
        snapshot.clutch_mode = KsClutchMode::CombinedAxis;
        snapshot.clutch_combined = Some(31_000);

        assert_eq!(snapshot.both_clutches_pressed(30_000), Some(true));
        assert_eq!(snapshot.both_clutches_pressed(32_000), Some(false));
    }

    #[test]
    fn ks_snapshot_independent_axis_mode_detects_pressed() {
        let mut snapshot = KsReportSnapshot::default();
        snapshot.clutch_mode = KsClutchMode::IndependentAxis;
        snapshot.clutch_left = Some(31_000);
        snapshot.clutch_right = Some(40_000);

        assert_eq!(snapshot.both_clutches_pressed(30_000), Some(true));
        assert_eq!(snapshot.both_clutches_pressed(32_000), Some(false));
    }

    #[test]
    fn ks_snapshot_button_mode_detects_pressed() {
        let mut snapshot = KsReportSnapshot::default();
        snapshot.clutch_mode = KsClutchMode::Button;
        snapshot.clutch_left_button = Some(true);
        snapshot.clutch_right_button = Some(true);

        assert_eq!(snapshot.both_clutches_pressed(30_000), Some(true));
        snapshot.clutch_right_button = Some(false);
        assert_eq!(snapshot.both_clutches_pressed(30_000), Some(false));
    }

    #[test]
    fn ks_snapshot_unknown_mode_defers_to_input_state() {
        let snapshot = KsReportSnapshot::default();

        assert_eq!(snapshot.both_clutches_pressed(30_000), None);
    }

    #[test]
    fn ks_snapshot_from_common_controls() {
        let buttons = [0x01u8; KS_BUTTON_BYTES];
        let snapshot = KsReportSnapshot::from_common_controls(7, buttons, 0x42);

        assert_eq!(snapshot.tick, 7);
        assert_eq!(snapshot.buttons, buttons);
        assert_eq!(snapshot.hat, 0x42);
        assert_eq!(snapshot.clutch_mode, KsClutchMode::Unknown);
        assert_eq!(snapshot.clutch_combined, None);
    }

    #[test]
    fn ks_report_map_parses_axes_and_buttons() {
        let mut map = KsReportMap::empty();
        map.report_id = Some(0x01);
        map.buttons_offset = Some(11);
        map.hat_offset = Some(27);
        map.clutch_mode_hint = KsClutchMode::CombinedAxis;
        map.clutch_combined_axis = Some(KsAxisSource::new(7, false));

        let report = [
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x88, 0x77, 0x66, 0x55, 0x11, 0xAB, 0xCD,
            0xEF, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
            0x0E, 0x0F, 0x33,
        ];

        let snapshot = match map.parse(9, &report) {
            Some(snapshot) => snapshot,
            None => {
                assert!(false, "report should match configured map");
                KsReportSnapshot::default()
            }
        };

        assert_eq!(snapshot.tick, 9);
        assert_eq!(snapshot.hat, 0x33);
        assert_eq!(snapshot.clutch_mode, KsClutchMode::CombinedAxis);
        assert_eq!(snapshot.clutch_combined, Some(0x7788));
        assert_eq!(snapshot.buttons[0], 0xAB);
        assert_eq!(snapshot.buttons[3], 0x01);
    }
}
