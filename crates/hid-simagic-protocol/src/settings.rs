//! Simagic wheelbase settings: Feature Report 0x80 (set) / 0x81 (get).
//!
//! # Wire protocol (verified from JacKeTUs/simagic-ff)
//!
//! Simagic wheelbases expose device settings via HID Feature Reports.
//! All reports are 64 bytes (zero-padded). The same Report ID 0x80 is
//! used for all "set" operations; bytes 1–2 select which settings page.
//! Report ID 0x81 is used to read the combined status.
//!
//! ## Settings pages
//!
//! | Page | Byte\[1\] | Byte\[2\] | Contents |
//! |------|-----------|-----------|----------|
//! | 1    | `0x01`    | —         | Angle, FFB strength, rotation speed, dampers, centering |
//! | 2    | `0x02`    | —         | Angle lock, feedback detail, lock strength, inertia |
//! | 3    | `0x10`    | `0x38`    | Ring light (enable + brightness) |
//! | 4    | `0x10`    | `0x39`    | Filter level, slew rate control |
//!
//! ## Value ranges (from sanitization in hid-simagic-settings.c)
//!
//! | Field                | Type   | Range          |
//! |----------------------|--------|----------------|
//! | `max_angle`          | LE16   | 90–2520°       |
//! | `ff_strength`        | LE16   | −100–100       |
//! | `wheel_rotation_speed` | u8   | 0–100          |
//! | `mechanical_centering` | u8   | 0–100          |
//! | `mechanical_damper`  | u8     | 0–100          |
//! | `center_damper`      | u8     | 0–100          |
//! | `mechanical_friction` | u8    | 0–100          |
//! | `game_centering`     | u8     | 0–200          |
//! | `game_inertia`       | u8     | 0–200          |
//! | `game_damper`        | u8     | 0–200          |
//! | `game_friction`      | u8     | 0–200          |
//! | `angle_lock`         | LE16   | 90–max\_angle  |
//! | `feedback_detail`    | u8     | 0–100          |
//! | `angle_lock_strength` | u8    | 0–2 (Soft/Normal/Firm) |
//! | `mechanical_inertia` | u8     | 0–100          |
//! | `ring_light`         | u8     | bit 7 = enable, bits 0–6 = 0–100 brightness |
//! | `filter_level`       | u8     | 0–20           |
//! | `slew_rate_control`  | u8     | 0–100          |

/// HID Feature Report ID for writing settings.
pub const SET_REPORT_ID: u8 = 0x80;

/// HID Feature Report ID for reading status.
pub const GET_REPORT_ID: u8 = 0x81;

/// Total size of a settings Feature Report (zero-padded).
pub const REPORT_SIZE: usize = 64;

/// Minimum steering angle in degrees.
pub const MIN_ANGLE: u16 = 90;

/// Maximum steering angle in degrees.
pub const MAX_ANGLE: u16 = 2520;

// ---------------------------------------------------------------------------
// Angle-lock strength enum
// ---------------------------------------------------------------------------

/// Angle-lock strength setting.
///
/// Source: `angle_lock_strength` field, clamp 0–2 in hid-simagic-settings.c.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AngleLockStrength {
    /// Soft stop at angle limit.
    Soft = 0,
    /// Normal stop at angle limit.
    Normal = 1,
    /// Firm stop at angle limit.
    Firm = 2,
}

impl AngleLockStrength {
    /// Convert from raw byte. Returns `None` for values > 2.
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Soft),
            1 => Some(Self::Normal),
            2 => Some(Self::Firm),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Ring light helper
// ---------------------------------------------------------------------------

/// Encode a ring-light byte: bit 7 = enable, bits 0–6 = brightness (0–100).
///
/// `brightness` is clamped to 0–100.
pub fn encode_ring_light(enabled: bool, brightness: u8) -> u8 {
    let b = if brightness > 100 { 100 } else { brightness };
    if enabled { 0x80 | b } else { b }
}

/// Decode a ring-light byte into (enabled, brightness).
pub fn decode_ring_light(raw: u8) -> (bool, u8) {
    let enabled = raw & 0x80 != 0;
    let brightness = raw & 0x7F;
    (enabled, brightness)
}

// ---------------------------------------------------------------------------
// Settings page 1: core FFB and mechanical settings
// ---------------------------------------------------------------------------

/// Settings page 1: core FFB and mechanical parameters.
///
/// Sent as Feature Report 0x80 with byte\[1\] = 0x01.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings1 {
    /// Maximum steering angle in degrees (90–2520).
    pub max_angle: u16,
    /// FFB strength (−100 to +100). Negative inverts FFB direction.
    pub ff_strength: i16,
    /// Wheel rotation speed (0–100).
    pub wheel_rotation_speed: u8,
    /// Mechanical centering force (0–100).
    pub mechanical_centering: u8,
    /// Mechanical damper strength (0–100).
    pub mechanical_damper: u8,
    /// Center-position damper (0–100).
    pub center_damper: u8,
    /// Mechanical friction (0–100).
    pub mechanical_friction: u8,
    /// Game centering multiplier (0–200).
    pub game_centering: u8,
    /// Game inertia multiplier (0–200).
    pub game_inertia: u8,
    /// Game damper multiplier (0–200).
    pub game_damper: u8,
    /// Game friction multiplier (0–200).
    pub game_friction: u8,
}

impl Settings1 {
    /// Clamp all fields to valid ranges (matching kernel sanitization).
    pub fn sanitize(&mut self) {
        self.max_angle = self.max_angle.clamp(MIN_ANGLE, MAX_ANGLE);
        self.ff_strength = self.ff_strength.clamp(-100, 100);
        self.wheel_rotation_speed = self.wheel_rotation_speed.min(100);
        self.mechanical_centering = self.mechanical_centering.min(100);
        self.mechanical_damper = self.mechanical_damper.min(100);
        self.center_damper = self.center_damper.min(100);
        self.mechanical_friction = self.mechanical_friction.min(100);
        self.game_centering = self.game_centering.min(200);
        self.game_inertia = self.game_inertia.min(200);
        self.game_damper = self.game_damper.min(200);
        self.game_friction = self.game_friction.min(200);
    }
}

/// Encode Settings page 1 into a 64-byte Feature Report.
///
/// The report is sanitized before encoding.
pub fn encode_settings1(settings: &Settings1) -> [u8; REPORT_SIZE] {
    let mut s = *settings;
    s.sanitize();
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = SET_REPORT_ID;
    buf[1] = 0x01;
    buf[2..4].copy_from_slice(&s.max_angle.to_le_bytes());
    buf[4..6].copy_from_slice(&(s.ff_strength as u16).to_le_bytes());
    buf[6] = 0x02; // unknown_offset_06, always 0x02
    buf[7] = s.wheel_rotation_speed;
    buf[8] = s.mechanical_centering;
    buf[9] = s.mechanical_damper;
    buf[10] = s.center_damper;
    buf[11] = s.mechanical_friction;
    buf[12] = s.game_centering;
    buf[13] = s.game_inertia;
    buf[14] = s.game_damper;
    buf[15] = s.game_friction;
    buf
}

// ---------------------------------------------------------------------------
// Settings page 2: angle lock and detail
// ---------------------------------------------------------------------------

/// Settings page 2: angle lock and feedback detail.
///
/// Sent as Feature Report 0x80 with byte\[1\] = 0x02.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings2 {
    /// Angle lock in degrees (90 to max_angle).
    pub angle_lock: u16,
    /// Feedback detail level (0–100).
    pub feedback_detail: u8,
    /// Angle lock strength (Soft/Normal/Firm → 0/1/2).
    pub angle_lock_strength: u8,
    /// Mechanical inertia (0–100).
    pub mechanical_inertia: u8,
}

impl Settings2 {
    /// Clamp all fields to valid ranges.
    ///
    /// `max_angle` is used as the upper bound for `angle_lock`.
    pub fn sanitize(&mut self, max_angle: u16) {
        let upper = max_angle.clamp(MIN_ANGLE, MAX_ANGLE);
        self.angle_lock = self.angle_lock.clamp(MIN_ANGLE, upper);
        self.feedback_detail = self.feedback_detail.min(100);
        self.angle_lock_strength = self.angle_lock_strength.min(2);
        self.mechanical_inertia = self.mechanical_inertia.min(100);
    }
}

/// Encode Settings page 2 into a 64-byte Feature Report.
///
/// `max_angle` is the current max angle, used to clamp `angle_lock`.
pub fn encode_settings2(settings: &Settings2, max_angle: u16) -> [u8; REPORT_SIZE] {
    let mut s = *settings;
    s.sanitize(max_angle);
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = SET_REPORT_ID;
    buf[1] = 0x02;
    buf[2..4].copy_from_slice(&s.angle_lock.to_le_bytes());
    buf[4] = s.feedback_detail;
    // buf[5] = unknown_offset_06 (copied from status, left as 0)
    buf[6] = s.angle_lock_strength;
    // buf[7] = unknown_offset_08 (copied from status, left as 0)
    buf[8] = s.mechanical_inertia;
    // buf[9] = unknown_offset_10 (copied from status, left as 0)
    buf
}

// ---------------------------------------------------------------------------
// Settings page 3: ring light
// ---------------------------------------------------------------------------

/// Settings page 3: ring light.
///
/// Sent as Feature Report 0x80 with byte\[1\] = 0x10, byte\[2\] = 0x38.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings3 {
    /// Ring light enabled.
    pub ring_light_enabled: bool,
    /// Ring light brightness (0–100).
    pub ring_light_brightness: u8,
}

/// Encode Settings page 3 into a 64-byte Feature Report.
pub fn encode_settings3(settings: &Settings3) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = SET_REPORT_ID;
    buf[1] = 0x10;
    buf[2] = 0x38;
    buf[3] = 0x00;
    buf[4] = 0x01;
    buf[5] = encode_ring_light(settings.ring_light_enabled, settings.ring_light_brightness);
    buf
}

// ---------------------------------------------------------------------------
// Settings page 4: filter and slew rate
// ---------------------------------------------------------------------------

/// Settings page 4: filter level and slew rate control.
///
/// Sent as Feature Report 0x80 with byte\[1\] = 0x10, byte\[2\] = 0x39.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings4 {
    /// Filter level (0–20).
    pub filter_level: u8,
    /// Slew rate control (0–100).
    pub slew_rate_control: u8,
}

impl Settings4 {
    /// Clamp all fields to valid ranges.
    pub fn sanitize(&mut self) {
        self.filter_level = self.filter_level.min(20);
        self.slew_rate_control = self.slew_rate_control.min(100);
    }
}

/// Encode Settings page 4 into a 64-byte Feature Report.
pub fn encode_settings4(settings: &Settings4) -> [u8; REPORT_SIZE] {
    let mut s = *settings;
    s.sanitize();
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = SET_REPORT_ID;
    buf[1] = 0x10;
    buf[2] = 0x39;
    buf[3] = 0x00;
    buf[4] = 0x07;
    // buf[5] = unknown_offset_05 (from status, left as 0)
    // buf[6] = unknown_offset_06 (from status, left as 0)
    buf[7] = s.filter_level;
    // buf[8] = unknown_offset_08 (from status, left as 0)
    buf[9] = s.slew_rate_control;
    buf
}

// ---------------------------------------------------------------------------
// Status report 0x81 parser
// ---------------------------------------------------------------------------

/// Parsed status from Feature Report 0x81.
///
/// This maps the full 64-byte status report into typed fields.
/// Unknown offsets are preserved for round-tripping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status1 {
    /// Maximum steering angle (90–2520°).
    pub max_angle: u16,
    /// FFB strength (−100 to +100).
    pub ff_strength: i16,
    /// Wheel rotation speed (0–100).
    pub wheel_rotation_speed: u8,
    /// Mechanical centering (0–100).
    pub mechanical_centering: u8,
    /// Mechanical damper (0–100).
    pub mechanical_damper: u8,
    /// Center damper (0–100).
    pub center_damper: u8,
    /// Mechanical friction (0–100).
    pub mechanical_friction: u8,
    /// Game centering (0–200).
    pub game_centering: u8,
    /// Game inertia (0–200).
    pub game_inertia: u8,
    /// Game damper (0–200).
    pub game_damper: u8,
    /// Game friction (0–200).
    pub game_friction: u8,
    /// Angle lock (90–max_angle).
    pub angle_lock: u16,
    /// Feedback detail (0–100).
    pub feedback_detail: u8,
    /// Angle lock strength (0–2).
    pub angle_lock_strength: u8,
    /// Mechanical inertia (0–100).
    pub mechanical_inertia: u8,
    /// Ring light (raw byte: bit 7 = enable, bits 0–6 = brightness).
    pub ring_light: u8,
    /// Filter level (0–20).
    pub filter_level: u8,
    /// Slew rate control (0–100).
    pub slew_rate_control: u8,
}

/// Parse a 64-byte Feature Report 0x81 into a [`Status1`].
///
/// Returns `None` if `data` is too short or the report ID doesn't match.
pub fn parse_status1(data: &[u8]) -> Option<Status1> {
    if data.len() < 53 || data[0] != GET_REPORT_ID {
        return None;
    }
    Some(Status1 {
        max_angle: u16::from_le_bytes([data[2], data[3]]),
        ff_strength: i16::from_le_bytes([data[4], data[5]]),
        wheel_rotation_speed: data[7],
        mechanical_centering: data[8],
        mechanical_damper: data[9],
        center_damper: data[10],
        mechanical_friction: data[11],
        game_centering: data[12],
        game_inertia: data[13],
        game_damper: data[14],
        game_friction: data[15],
        angle_lock: u16::from_le_bytes([data[16], data[17]]),
        feedback_detail: data[18],
        angle_lock_strength: data[20],
        mechanical_inertia: data[22],
        ring_light: data[47],
        filter_level: data[50],
        slew_rate_control: data[52],
    })
}

/// Convert a [`Status1`] into [`Settings1`] (page 1 round-trip).
impl From<&Status1> for Settings1 {
    fn from(s: &Status1) -> Self {
        Self {
            max_angle: s.max_angle,
            ff_strength: s.ff_strength,
            wheel_rotation_speed: s.wheel_rotation_speed,
            mechanical_centering: s.mechanical_centering,
            mechanical_damper: s.mechanical_damper,
            center_damper: s.center_damper,
            mechanical_friction: s.mechanical_friction,
            game_centering: s.game_centering,
            game_inertia: s.game_inertia,
            game_damper: s.game_damper,
            game_friction: s.game_friction,
        }
    }
}

/// Convert a [`Status1`] into [`Settings2`] (page 2 round-trip).
impl From<&Status1> for Settings2 {
    fn from(s: &Status1) -> Self {
        Self {
            angle_lock: s.angle_lock,
            feedback_detail: s.feedback_detail,
            angle_lock_strength: s.angle_lock_strength,
            mechanical_inertia: s.mechanical_inertia,
        }
    }
}

/// Convert a [`Status1`] into [`Settings3`] (page 3 round-trip).
impl From<&Status1> for Settings3 {
    fn from(s: &Status1) -> Self {
        let (enabled, brightness) = decode_ring_light(s.ring_light);
        Self {
            ring_light_enabled: enabled,
            ring_light_brightness: brightness,
        }
    }
}

/// Convert a [`Status1`] into [`Settings4`] (page 4 round-trip).
impl From<&Status1> for Settings4 {
    fn from(s: &Status1) -> Self {
        Self {
            filter_level: s.filter_level,
            slew_rate_control: s.slew_rate_control,
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Constants
    // -----------------------------------------------------------------------

    #[test]
    fn report_ids() {
        assert_eq!(SET_REPORT_ID, 0x80);
        assert_eq!(GET_REPORT_ID, 0x81);
    }

    #[test]
    fn report_size_is_64() {
        assert_eq!(REPORT_SIZE, 64);
    }

    // -----------------------------------------------------------------------
    // Ring light encoding
    // -----------------------------------------------------------------------

    #[test]
    fn ring_light_encode_enabled() {
        assert_eq!(encode_ring_light(true, 50), 0x80 | 50);
    }

    #[test]
    fn ring_light_encode_disabled() {
        assert_eq!(encode_ring_light(false, 50), 50);
    }

    #[test]
    fn ring_light_encode_clamps_brightness() {
        assert_eq!(encode_ring_light(true, 200), 0x80 | 100);
    }

    #[test]
    fn ring_light_roundtrip() {
        for enabled in [true, false] {
            for b in 0..=100u8 {
                let encoded = encode_ring_light(enabled, b);
                let (dec_en, dec_br) = decode_ring_light(encoded);
                assert_eq!(dec_en, enabled, "enabled mismatch for b={b}");
                assert_eq!(dec_br, b, "brightness mismatch for b={b}");
            }
        }
    }

    // -----------------------------------------------------------------------
    // AngleLockStrength
    // -----------------------------------------------------------------------

    #[test]
    fn angle_lock_strength_from_byte() {
        assert_eq!(AngleLockStrength::from_byte(0), Some(AngleLockStrength::Soft));
        assert_eq!(AngleLockStrength::from_byte(1), Some(AngleLockStrength::Normal));
        assert_eq!(AngleLockStrength::from_byte(2), Some(AngleLockStrength::Firm));
        assert_eq!(AngleLockStrength::from_byte(3), None);
    }

    // -----------------------------------------------------------------------
    // Settings1 sanitize
    // -----------------------------------------------------------------------

    #[test]
    fn settings1_sanitize_clamps_angle() {
        let mut s = Settings1 {
            max_angle: 50, // below min
            ff_strength: 0,
            wheel_rotation_speed: 0,
            mechanical_centering: 0,
            mechanical_damper: 0,
            center_damper: 0,
            mechanical_friction: 0,
            game_centering: 0,
            game_inertia: 0,
            game_damper: 0,
            game_friction: 0,
        };
        s.sanitize();
        assert_eq!(s.max_angle, MIN_ANGLE);
    }

    #[test]
    fn settings1_sanitize_clamps_angle_high() {
        let mut s = Settings1 {
            max_angle: 5000,
            ff_strength: 0,
            wheel_rotation_speed: 0,
            mechanical_centering: 0,
            mechanical_damper: 0,
            center_damper: 0,
            mechanical_friction: 0,
            game_centering: 0,
            game_inertia: 0,
            game_damper: 0,
            game_friction: 0,
        };
        s.sanitize();
        assert_eq!(s.max_angle, MAX_ANGLE);
    }

    #[test]
    fn settings1_sanitize_clamps_ff_strength() {
        let mut s = Settings1 {
            max_angle: 900,
            ff_strength: -200,
            wheel_rotation_speed: 255,
            mechanical_centering: 200,
            mechanical_damper: 200,
            center_damper: 200,
            mechanical_friction: 200,
            game_centering: 255,
            game_inertia: 255,
            game_damper: 255,
            game_friction: 255,
        };
        s.sanitize();
        assert_eq!(s.ff_strength, -100);
        assert_eq!(s.wheel_rotation_speed, 100);
        assert_eq!(s.mechanical_centering, 100);
        assert_eq!(s.game_centering, 200);
    }

    // -----------------------------------------------------------------------
    // Settings1 encoding
    // -----------------------------------------------------------------------

    #[test]
    fn encode_settings1_header() {
        let s = Settings1 {
            max_angle: 900,
            ff_strength: 50,
            wheel_rotation_speed: 30,
            mechanical_centering: 40,
            mechanical_damper: 50,
            center_damper: 60,
            mechanical_friction: 70,
            game_centering: 100,
            game_inertia: 110,
            game_damper: 120,
            game_friction: 130,
        };
        let buf = encode_settings1(&s);
        assert_eq!(buf[0], SET_REPORT_ID);
        assert_eq!(buf[1], 0x01);
        assert_eq!(buf[6], 0x02); // unknown_offset_06
    }

    #[test]
    fn encode_settings1_angle_le16() {
        let s = Settings1 {
            max_angle: 900,
            ff_strength: 0,
            wheel_rotation_speed: 0,
            mechanical_centering: 0,
            mechanical_damper: 0,
            center_damper: 0,
            mechanical_friction: 0,
            game_centering: 0,
            game_inertia: 0,
            game_damper: 0,
            game_friction: 0,
        };
        let buf = encode_settings1(&s);
        let angle = u16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(angle, 900);
    }

    #[test]
    fn encode_settings1_ffb_strength_negative() {
        let s = Settings1 {
            max_angle: 900,
            ff_strength: -50,
            wheel_rotation_speed: 0,
            mechanical_centering: 0,
            mechanical_damper: 0,
            center_damper: 0,
            mechanical_friction: 0,
            game_centering: 0,
            game_inertia: 0,
            game_damper: 0,
            game_friction: 0,
        };
        let buf = encode_settings1(&s);
        let strength = i16::from_le_bytes([buf[4], buf[5]]);
        assert_eq!(strength, -50);
    }

    #[test]
    fn encode_settings1_mechanical_fields() {
        let s = Settings1 {
            max_angle: 900,
            ff_strength: 0,
            wheel_rotation_speed: 10,
            mechanical_centering: 20,
            mechanical_damper: 30,
            center_damper: 40,
            mechanical_friction: 50,
            game_centering: 100,
            game_inertia: 150,
            game_damper: 180,
            game_friction: 200,
        };
        let buf = encode_settings1(&s);
        assert_eq!(buf[7], 10);
        assert_eq!(buf[8], 20);
        assert_eq!(buf[9], 30);
        assert_eq!(buf[10], 40);
        assert_eq!(buf[11], 50);
        assert_eq!(buf[12], 100);
        assert_eq!(buf[13], 150);
        assert_eq!(buf[14], 180);
        assert_eq!(buf[15], 200);
    }

    #[test]
    fn encode_settings1_report_length() {
        let s = Settings1 {
            max_angle: 900,
            ff_strength: 0,
            wheel_rotation_speed: 0,
            mechanical_centering: 0,
            mechanical_damper: 0,
            center_damper: 0,
            mechanical_friction: 0,
            game_centering: 0,
            game_inertia: 0,
            game_damper: 0,
            game_friction: 0,
        };
        assert_eq!(encode_settings1(&s).len(), 64);
    }

    // -----------------------------------------------------------------------
    // Settings2 encoding
    // -----------------------------------------------------------------------

    #[test]
    fn encode_settings2_header() {
        let s = Settings2 {
            angle_lock: 540,
            feedback_detail: 50,
            angle_lock_strength: 1,
            mechanical_inertia: 30,
        };
        let buf = encode_settings2(&s, 900);
        assert_eq!(buf[0], SET_REPORT_ID);
        assert_eq!(buf[1], 0x02);
    }

    #[test]
    fn encode_settings2_angle_lock_clamped() {
        let s = Settings2 {
            angle_lock: 5000, // above max_angle
            feedback_detail: 50,
            angle_lock_strength: 1,
            mechanical_inertia: 30,
        };
        let buf = encode_settings2(&s, 900);
        let lock = u16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(lock, 900); // clamped to max_angle
    }

    #[test]
    fn encode_settings2_fields() {
        let s = Settings2 {
            angle_lock: 540,
            feedback_detail: 75,
            angle_lock_strength: 2,
            mechanical_inertia: 40,
        };
        let buf = encode_settings2(&s, 900);
        assert_eq!(buf[4], 75); // feedback_detail
        assert_eq!(buf[6], 2);  // angle_lock_strength
        assert_eq!(buf[8], 40); // mechanical_inertia
    }

    // -----------------------------------------------------------------------
    // Settings3 encoding
    // -----------------------------------------------------------------------

    #[test]
    fn encode_settings3_header() {
        let s = Settings3 {
            ring_light_enabled: true,
            ring_light_brightness: 50,
        };
        let buf = encode_settings3(&s);
        assert_eq!(buf[0], SET_REPORT_ID);
        assert_eq!(buf[1], 0x10);
        assert_eq!(buf[2], 0x38);
        assert_eq!(buf[3], 0x00);
        assert_eq!(buf[4], 0x01);
    }

    #[test]
    fn encode_settings3_ring_light() {
        let s = Settings3 {
            ring_light_enabled: true,
            ring_light_brightness: 75,
        };
        let buf = encode_settings3(&s);
        assert_eq!(buf[5], 0x80 | 75);
    }

    #[test]
    fn encode_settings3_ring_light_disabled() {
        let s = Settings3 {
            ring_light_enabled: false,
            ring_light_brightness: 30,
        };
        let buf = encode_settings3(&s);
        assert_eq!(buf[5], 30);
    }

    // -----------------------------------------------------------------------
    // Settings4 encoding
    // -----------------------------------------------------------------------

    #[test]
    fn encode_settings4_header() {
        let s = Settings4 {
            filter_level: 10,
            slew_rate_control: 50,
        };
        let buf = encode_settings4(&s);
        assert_eq!(buf[0], SET_REPORT_ID);
        assert_eq!(buf[1], 0x10);
        assert_eq!(buf[2], 0x39);
        assert_eq!(buf[3], 0x00);
        assert_eq!(buf[4], 0x07);
    }

    #[test]
    fn encode_settings4_fields() {
        let s = Settings4 {
            filter_level: 15,
            slew_rate_control: 80,
        };
        let buf = encode_settings4(&s);
        assert_eq!(buf[7], 15); // filter_level
        assert_eq!(buf[9], 80); // slew_rate_control
    }

    #[test]
    fn encode_settings4_sanitize_filter_level() {
        let s = Settings4 {
            filter_level: 50, // max is 20
            slew_rate_control: 200, // max is 100
        };
        let buf = encode_settings4(&s);
        assert_eq!(buf[7], 20);
        assert_eq!(buf[9], 100);
    }

    // -----------------------------------------------------------------------
    // Status1 parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_status1_too_short() {
        assert!(parse_status1(&[0x81; 10]).is_none());
    }

    #[test]
    fn parse_status1_wrong_report_id() {
        let mut data = [0u8; 64];
        data[0] = 0x01; // wrong ID
        assert!(parse_status1(&data).is_none());
    }

    #[test]
    fn parse_status1_valid() {
        let mut data = [0u8; 64];
        data[0] = GET_REPORT_ID;
        // max_angle = 900 (LE16 at offset 2-3)
        data[2] = 0x84;
        data[3] = 0x03;
        // ff_strength = -50 (LE16 signed at offset 4-5)
        let neg: u16 = (-50i16) as u16;
        data[4] = neg as u8;
        data[5] = (neg >> 8) as u8;
        data[7] = 30;  // wheel_rotation_speed
        data[8] = 40;  // mechanical_centering
        data[9] = 50;  // mechanical_damper
        data[10] = 60; // center_damper
        data[11] = 70; // mechanical_friction
        data[12] = 100; // game_centering
        data[13] = 110; // game_inertia
        data[14] = 120; // game_damper
        data[15] = 130; // game_friction
        // angle_lock = 540 at offset 16-17
        data[16] = 0x1C;
        data[17] = 0x02;
        data[18] = 75;  // feedback_detail
        data[20] = 2;   // angle_lock_strength
        data[22] = 45;  // mechanical_inertia
        data[47] = 0x80 | 50; // ring_light: enabled, brightness 50
        data[50] = 10;  // filter_level
        data[52] = 80;  // slew_rate_control

        if let Some(s) = parse_status1(&data) {
            assert_eq!(s.max_angle, 900);
            assert_eq!(s.ff_strength, -50);
            assert_eq!(s.wheel_rotation_speed, 30);
            assert_eq!(s.mechanical_centering, 40);
            assert_eq!(s.mechanical_damper, 50);
            assert_eq!(s.center_damper, 60);
            assert_eq!(s.mechanical_friction, 70);
            assert_eq!(s.game_centering, 100);
            assert_eq!(s.game_inertia, 110);
            assert_eq!(s.game_damper, 120);
            assert_eq!(s.game_friction, 130);
            assert_eq!(s.angle_lock, 540);
            assert_eq!(s.feedback_detail, 75);
            assert_eq!(s.angle_lock_strength, 2);
            assert_eq!(s.mechanical_inertia, 45);
            assert_eq!(s.ring_light, 0x80 | 50);
            assert_eq!(s.filter_level, 10);
            assert_eq!(s.slew_rate_control, 80);
        } else {
            panic!("parse_status1 returned None for valid data");
        }
    }

    // -----------------------------------------------------------------------
    // Status → Settings round-trips
    // -----------------------------------------------------------------------

    #[test]
    fn status_to_settings1_roundtrip() {
        let status = Status1 {
            max_angle: 900,
            ff_strength: -50,
            wheel_rotation_speed: 30,
            mechanical_centering: 40,
            mechanical_damper: 50,
            center_damper: 60,
            mechanical_friction: 70,
            game_centering: 100,
            game_inertia: 110,
            game_damper: 120,
            game_friction: 130,
            angle_lock: 540,
            feedback_detail: 75,
            angle_lock_strength: 2,
            mechanical_inertia: 45,
            ring_light: 0xB2,
            filter_level: 10,
            slew_rate_control: 80,
        };
        let s1: Settings1 = (&status).into();
        assert_eq!(s1.max_angle, 900);
        assert_eq!(s1.ff_strength, -50);
        assert_eq!(s1.wheel_rotation_speed, 30);
        assert_eq!(s1.game_friction, 130);
    }

    #[test]
    fn status_to_settings2_roundtrip() {
        let status = Status1 {
            max_angle: 900,
            ff_strength: 0,
            wheel_rotation_speed: 0,
            mechanical_centering: 0,
            mechanical_damper: 0,
            center_damper: 0,
            mechanical_friction: 0,
            game_centering: 0,
            game_inertia: 0,
            game_damper: 0,
            game_friction: 0,
            angle_lock: 540,
            feedback_detail: 75,
            angle_lock_strength: 1,
            mechanical_inertia: 45,
            ring_light: 0,
            filter_level: 0,
            slew_rate_control: 0,
        };
        let s2: Settings2 = (&status).into();
        assert_eq!(s2.angle_lock, 540);
        assert_eq!(s2.feedback_detail, 75);
        assert_eq!(s2.angle_lock_strength, 1);
        assert_eq!(s2.mechanical_inertia, 45);
    }

    #[test]
    fn status_to_settings3_roundtrip() {
        let status = Status1 {
            max_angle: 900,
            ff_strength: 0,
            wheel_rotation_speed: 0,
            mechanical_centering: 0,
            mechanical_damper: 0,
            center_damper: 0,
            mechanical_friction: 0,
            game_centering: 0,
            game_inertia: 0,
            game_damper: 0,
            game_friction: 0,
            angle_lock: 0,
            feedback_detail: 0,
            angle_lock_strength: 0,
            mechanical_inertia: 0,
            ring_light: 0x80 | 75,
            filter_level: 0,
            slew_rate_control: 0,
        };
        let s3: Settings3 = (&status).into();
        assert!(s3.ring_light_enabled);
        assert_eq!(s3.ring_light_brightness, 75);
    }

    #[test]
    fn status_to_settings4_roundtrip() {
        let status = Status1 {
            max_angle: 900,
            ff_strength: 0,
            wheel_rotation_speed: 0,
            mechanical_centering: 0,
            mechanical_damper: 0,
            center_damper: 0,
            mechanical_friction: 0,
            game_centering: 0,
            game_inertia: 0,
            game_damper: 0,
            game_friction: 0,
            angle_lock: 0,
            feedback_detail: 0,
            angle_lock_strength: 0,
            mechanical_inertia: 0,
            ring_light: 0,
            filter_level: 15,
            slew_rate_control: 80,
        };
        let s4: Settings4 = (&status).into();
        assert_eq!(s4.filter_level, 15);
        assert_eq!(s4.slew_rate_control, 80);
    }

    // -----------------------------------------------------------------------
    // Full encode→parse round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn settings1_encode_preserves_values() {
        let s = Settings1 {
            max_angle: 1080,
            ff_strength: 75,
            wheel_rotation_speed: 50,
            mechanical_centering: 60,
            mechanical_damper: 70,
            center_damper: 80,
            mechanical_friction: 90,
            game_centering: 150,
            game_inertia: 160,
            game_damper: 170,
            game_friction: 180,
        };
        let buf = encode_settings1(&s);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 1080);
        assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), 75);
        assert_eq!(buf[7], 50);
        assert_eq!(buf[15], 180);
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn settings2_angle_lock_below_min() {
        let s = Settings2 {
            angle_lock: 10, // below 90
            feedback_detail: 0,
            angle_lock_strength: 0,
            mechanical_inertia: 0,
        };
        let buf = encode_settings2(&s, 900);
        let lock = u16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(lock, MIN_ANGLE);
    }

    #[test]
    fn all_reports_are_64_bytes() {
        let s1 = Settings1 {
            max_angle: 900, ff_strength: 0, wheel_rotation_speed: 0,
            mechanical_centering: 0, mechanical_damper: 0, center_damper: 0,
            mechanical_friction: 0, game_centering: 0, game_inertia: 0,
            game_damper: 0, game_friction: 0,
        };
        let s2 = Settings2 { angle_lock: 540, feedback_detail: 0, angle_lock_strength: 0, mechanical_inertia: 0 };
        let s3 = Settings3 { ring_light_enabled: true, ring_light_brightness: 50 };
        let s4 = Settings4 { filter_level: 10, slew_rate_control: 50 };

        assert_eq!(encode_settings1(&s1).len(), REPORT_SIZE);
        assert_eq!(encode_settings2(&s2, 900).len(), REPORT_SIZE);
        assert_eq!(encode_settings3(&s3).len(), REPORT_SIZE);
        assert_eq!(encode_settings4(&s4).len(), REPORT_SIZE);
    }
}
