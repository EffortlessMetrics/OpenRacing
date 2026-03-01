//! KS control-surface parsing and normalization.
//!
//! The parser is map-driven: report offsets and encoding are described by
//! `KsReportMap` rather than hard-coded per firmware layout.

#![deny(static_mut_refs)]

/// Number of encoder-like slots in a generic KS snapshot.
pub const KS_ENCODER_COUNT: usize = 8;
/// Number of packed button bytes in a normalized KS snapshot.
pub const KS_BUTTON_BYTES: usize = 16;

/// Supported clutch interpretation modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KsClutchMode {
    /// No reliable clutch layout discovered.
    #[default]
    Unknown,
    /// Combined clutch axis is mapped directly.
    CombinedAxis,
    /// Independent left/right clutch axes are mapped.
    IndependentAxis,
    /// Clutch values are exposed as two buttons.
    Button,
}

/// Supported rotary modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KsRotaryMode {
    /// No reliable rotary layout discovered.
    #[default]
    Unknown,
    /// Rotary input reports as discrete button edges/deltas.
    Button,
    /// Rotary input reports as continuous knobs.
    Knob,
}

/// Supported joystick modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KsJoystickMode {
    /// No reliable joystick mode discovered.
    #[default]
    Unknown,
    /// Joystick reports as buttons.
    Button,
    /// Joystick reports as D-pad hat direction.
    DPad,
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
        Some(i16::from_le_bytes(bytes))
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
        if let Some(expected_report_id) = self.report_id
            && report.first().copied() != Some(expected_report_id)
        {
            return None;
        }

        let mut snapshot = KsReportSnapshot {
            tick,
            clutch_mode: self.clutch_mode_hint,
            rotary_mode: self.rotary_mode_hint,
            joystick_mode: self.joystick_mode_hint,
            ..Default::default()
        };

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
        let snapshot = KsReportSnapshot {
            clutch_mode: KsClutchMode::CombinedAxis,
            clutch_combined: Some(31_000),
            ..Default::default()
        };

        assert_eq!(snapshot.both_clutches_pressed(30_000), Some(true));
        assert_eq!(snapshot.both_clutches_pressed(32_000), Some(false));
    }

    #[test]
    fn ks_snapshot_independent_axis_mode_detects_pressed() {
        let snapshot = KsReportSnapshot {
            clutch_mode: KsClutchMode::IndependentAxis,
            clutch_left: Some(31_000),
            clutch_right: Some(40_000),
            ..Default::default()
        };

        assert_eq!(snapshot.both_clutches_pressed(30_000), Some(true));
        assert_eq!(snapshot.both_clutches_pressed(32_000), Some(false));
    }

    #[test]
    fn ks_snapshot_button_mode_detects_pressed() {
        let mut snapshot = KsReportSnapshot {
            clutch_mode: KsClutchMode::Button,
            clutch_left_button: Some(true),
            clutch_right_button: Some(true),
            ..Default::default()
        };

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
    fn ks_report_map_parses_axes_and_buttons() -> Result<(), Box<dyn std::error::Error>> {
        let mut map = KsReportMap::empty();
        map.report_id = Some(0x01);
        map.buttons_offset = Some(12);
        map.hat_offset = Some(30);
        map.clutch_mode_hint = KsClutchMode::CombinedAxis;
        map.clutch_combined_axis = Some(KsAxisSource::new(7, false));

        let report = [
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x88, 0x77, 0x66, 0x55, 0x11, 0xAB, 0xCD,
            0xEF, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
            0x0E, 0x0F, 0x33,
        ];

        let snapshot = map
            .parse(9, &report)
            .ok_or("report should match configured map")?;

        assert_eq!(snapshot.tick, 9);
        assert_eq!(snapshot.hat, 0x33);
        assert_eq!(snapshot.clutch_mode, KsClutchMode::CombinedAxis);
        assert_eq!(snapshot.clutch_combined, Some(0x7788));
        assert_eq!(snapshot.buttons[0], 0xAB);
        assert_eq!(snapshot.buttons[3], 0x01);
        Ok(())
    }

    #[test]
    fn ks_report_map_rejects_wrong_report_id() {
        let mut map = KsReportMap::empty();
        map.report_id = Some(0x01);
        let report = [0x02, 0x00, 0x00, 0x00, 0x00];
        assert!(map.parse(0, &report).is_none());
    }

    #[test]
    fn ks_report_map_no_report_id_accepts_any() -> Result<(), Box<dyn std::error::Error>> {
        let map = KsReportMap::empty();
        let report = [0xFF, 0x00, 0x00, 0x00, 0x00];
        let snapshot = map
            .parse(1, &report)
            .ok_or("map with no report_id should accept any")?;
        assert_eq!(snapshot.tick, 1);
        Ok(())
    }

    #[test]
    fn ks_axis_source_parse_u16() -> Result<(), Box<dyn std::error::Error>> {
        let src = KsAxisSource::new(1, false);
        let data = [0x00, 0x34, 0x12];
        let val = src.parse_u16(&data).ok_or("expected u16 parse")?;
        assert_eq!(val, 0x1234);
        Ok(())
    }

    #[test]
    fn ks_axis_source_parse_i16() -> Result<(), Box<dyn std::error::Error>> {
        let src = KsAxisSource::new(0, true);
        let data = (-32768i16).to_le_bytes();
        let val = src.parse_i16(&data).ok_or("expected i16 parse")?;
        assert_eq!(val, -32768);
        Ok(())
    }

    #[test]
    fn ks_axis_source_returns_none_for_short_data() {
        let src = KsAxisSource::new(5, false);
        let data = [0x00, 0x01];
        assert!(src.parse_u16(&data).is_none());
        assert!(src.parse_i16(&data).is_none());
    }

    #[test]
    fn ks_bit_source_parse_active() -> Result<(), Box<dyn std::error::Error>> {
        let src = KsBitSource::new(0, 0x04);
        let data = [0x07];
        let val = src.parse(&data).ok_or("expected bit parse")?;
        assert!(val);
        Ok(())
    }

    #[test]
    fn ks_bit_source_parse_inactive() -> Result<(), Box<dyn std::error::Error>> {
        let src = KsBitSource::new(0, 0x04);
        let data = [0x03];
        let val = src.parse(&data).ok_or("expected bit parse")?;
        assert!(!val);
        Ok(())
    }

    #[test]
    fn ks_bit_source_inverted() -> Result<(), Box<dyn std::error::Error>> {
        let src = KsBitSource::inverted(0, 0x04);
        let data = [0x04];
        let val = src.parse(&data).ok_or("expected bit parse")?;
        assert!(!val, "inverted source should return false when bit is set");
        Ok(())
    }

    #[test]
    fn ks_bit_source_inverted_inactive() -> Result<(), Box<dyn std::error::Error>> {
        let src = KsBitSource::with_invert(0, 0x04);
        let data = [0x00];
        let val = src.parse(&data).ok_or("expected bit parse")?;
        assert!(val, "inverted source should return true when bit is clear");
        Ok(())
    }

    #[test]
    fn ks_bit_source_returns_none_for_short_data() {
        let src = KsBitSource::new(5, 0x01);
        let data = [0x00];
        assert!(src.parse(&data).is_none());
    }

    #[test]
    fn ks_byte_source_parse() -> Result<(), Box<dyn std::error::Error>> {
        let src = KsByteSource::new(2);
        let data = [0x00, 0x11, 0xAB, 0x00];
        let val = src.parse(&data).ok_or("expected byte parse")?;
        assert_eq!(val, 0xAB);
        Ok(())
    }

    #[test]
    fn ks_byte_source_returns_none_for_short_data() {
        let src = KsByteSource::new(5);
        let data = [0x00, 0x01];
        assert!(src.parse(&data).is_none());
    }

    #[test]
    fn ks_report_map_empty_has_no_bindings() {
        let map = KsReportMap::empty();
        assert_eq!(map.report_id, None);
        assert_eq!(map.buttons_offset, None);
        assert_eq!(map.hat_offset, None);
        assert_eq!(map.clutch_mode_hint, KsClutchMode::Unknown);
        assert_eq!(map.rotary_mode_hint, KsRotaryMode::Unknown);
        assert_eq!(map.joystick_mode_hint, KsJoystickMode::Unknown);
    }

    #[test]
    fn ks_report_snapshot_default_is_zeroed() {
        let snapshot = KsReportSnapshot::default();
        assert_eq!(snapshot.tick, 0);
        assert_eq!(snapshot.buttons, [0u8; KS_BUTTON_BYTES]);
        assert_eq!(snapshot.hat, 0);
        assert_eq!(snapshot.encoders, [0i16; KS_ENCODER_COUNT]);
        assert_eq!(snapshot.clutch_combined, None);
        assert_eq!(snapshot.clutch_left, None);
        assert_eq!(snapshot.clutch_right, None);
    }

    #[test]
    fn ks_report_map_parses_encoders() -> Result<(), Box<dyn std::error::Error>> {
        let mut map = KsReportMap::empty();
        map.encoders[0] = Some(KsAxisSource::new(0, true));
        map.encoders[1] = Some(KsAxisSource::new(2, true));

        let mut data = [0u8; 8];
        data[0..2].copy_from_slice(&500i16.to_le_bytes());
        data[2..4].copy_from_slice(&(-300i16).to_le_bytes());

        let snapshot = map.parse(1, &data).ok_or("expected encoder parse")?;
        assert_eq!(snapshot.encoders[0], 500);
        assert_eq!(snapshot.encoders[1], -300);
        Ok(())
    }

    #[test]
    fn ks_report_map_parses_rotary_axes() -> Result<(), Box<dyn std::error::Error>> {
        let mut map = KsReportMap::empty();
        map.left_rotary_axis = Some(KsAxisSource::new(0, true));
        map.right_rotary_axis = Some(KsAxisSource::new(2, true));

        let mut data = [0u8; 8];
        data[0..2].copy_from_slice(&100i16.to_le_bytes());
        data[2..4].copy_from_slice(&(-50i16).to_le_bytes());

        let snapshot = map.parse(2, &data).ok_or("expected rotary parse")?;
        assert_eq!(snapshot.encoders[0], 100);
        assert_eq!(snapshot.encoders[1], -50);
        Ok(())
    }

    #[test]
    fn ks_report_map_parses_clutch_buttons() -> Result<(), Box<dyn std::error::Error>> {
        let mut map = KsReportMap::empty();
        map.clutch_mode_hint = KsClutchMode::Button;
        map.clutch_left_button = Some(KsBitSource::new(0, 0x01));
        map.clutch_right_button = Some(KsBitSource::new(0, 0x02));

        let data = [0x03u8]; // both bits set
        let snapshot = map.parse(0, &data).ok_or("expected clutch button parse")?;
        assert_eq!(snapshot.clutch_left_button, Some(true));
        assert_eq!(snapshot.clutch_right_button, Some(true));
        assert_eq!(snapshot.clutch_mode, KsClutchMode::Button);
        Ok(())
    }

    #[test]
    fn ks_report_map_parses_hat_from_joystick_hat_source() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut map = KsReportMap::empty();
        map.joystick_hat = Some(KsByteSource::new(0));
        let data = [0x42];
        let snapshot = map.parse(0, &data).ok_or("expected hat parse")?;
        assert_eq!(snapshot.hat, 0x42);
        Ok(())
    }

    #[test]
    fn ks_report_map_partial_buttons_fill() -> Result<(), Box<dyn std::error::Error>> {
        let mut map = KsReportMap::empty();
        map.buttons_offset = Some(0);
        // Report has only 3 bytes, but buttons needs KS_BUTTON_BYTES (16)
        let data = [0xAA, 0xBB, 0xCC];
        let snapshot = map.parse(0, &data).ok_or("expected partial button parse")?;
        assert_eq!(snapshot.buttons[0], 0xAA);
        assert_eq!(snapshot.buttons[1], 0xBB);
        assert_eq!(snapshot.buttons[2], 0xCC);
        assert_eq!(snapshot.buttons[3..], [0u8; KS_BUTTON_BYTES - 3]);
        Ok(())
    }

    #[test]
    fn ks_clutch_mode_default_is_unknown() {
        assert_eq!(KsClutchMode::default(), KsClutchMode::Unknown);
    }

    #[test]
    fn ks_rotary_mode_default_is_unknown() {
        assert_eq!(KsRotaryMode::default(), KsRotaryMode::Unknown);
    }

    #[test]
    fn ks_joystick_mode_default_is_unknown() {
        assert_eq!(KsJoystickMode::default(), KsJoystickMode::Unknown);
    }

    #[test]
    fn ks_independent_axis_one_missing_returns_none() {
        let snapshot = KsReportSnapshot {
            clutch_mode: KsClutchMode::IndependentAxis,
            clutch_left: Some(31_000),
            clutch_right: None,
            ..Default::default()
        };
        assert_eq!(snapshot.both_clutches_pressed(30_000), None);
    }

    #[test]
    fn ks_button_mode_one_missing_returns_none() {
        let snapshot = KsReportSnapshot {
            clutch_mode: KsClutchMode::Button,
            clutch_left_button: Some(true),
            clutch_right_button: None,
            ..Default::default()
        };
        assert_eq!(snapshot.both_clutches_pressed(30_000), None);
    }

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn prop_axis_source_u16_round_trips(lo in 0u8..=255u8, hi in 0u8..=255u8) {
            let src = KsAxisSource::new(0, false);
            let data = [lo, hi];
            let expected = u16::from_le_bytes([lo, hi]);
            prop_assert_eq!(src.parse_u16(&data), Some(expected));
        }

        #[test]
        fn prop_axis_source_i16_round_trips(lo in 0u8..=255u8, hi in 0u8..=255u8) {
            let src = KsAxisSource::new(0, true);
            let data = [lo, hi];
            let expected = i16::from_le_bytes([lo, hi]);
            prop_assert_eq!(src.parse_i16(&data), Some(expected));
        }

        #[test]
        fn prop_bit_source_non_inverted_matches_mask(byte: u8, bit in 0u8..8u8) {
            let mask = 1u8 << bit;
            let src = KsBitSource::new(0, mask);
            let expected = byte & mask != 0;
            prop_assert_eq!(src.parse(&[byte]), Some(expected));
        }

        #[test]
        fn prop_bit_source_inverted_is_opposite(byte: u8, bit in 0u8..8u8) {
            let mask = 1u8 << bit;
            let normal = KsBitSource::new(0, mask);
            let inverted = KsBitSource::inverted(0, mask);
            let n = normal.parse(&[byte]);
            let i = inverted.parse(&[byte]);
            prop_assert_eq!(n.map(|v| !v), i);
        }

        #[test]
        fn prop_byte_source_matches_index(data in proptest::collection::vec(any::<u8>(), 1..=16)) {
            for (i, &expected) in data.iter().enumerate() {
                let src = KsByteSource::new(i);
                prop_assert_eq!(src.parse(&data), Some(expected));
            }
        }

        #[test]
        fn prop_empty_map_always_parses_non_empty_report(
            data in proptest::collection::vec(any::<u8>(), 1..=64),
            tick: u32,
        ) {
            let map = KsReportMap::empty();
            prop_assert!(map.parse(tick, &data).is_some());
        }
    }
}
