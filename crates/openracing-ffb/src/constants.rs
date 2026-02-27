//! FFB constants and limits

/// Maximum torque value in cNm (centi-Newton-meters)
pub const MAX_TORQUE_CNM: i32 = 2500;

/// Maximum torque value in Nm (Newton-meters)
pub const MAX_TORQUE_NM: f32 = 25.0;

/// Default HID report size
pub const DEFAULT_REPORT_SIZE: usize = 64;

/// Maximum number of concurrent effects
pub const MAX_EFFECTS: usize = 8;

/// Sample rate for FFB calculations (Hz)
pub const FFB_SAMPLE_RATE_HZ: u32 = 1000;

/// FFB update period in microseconds
pub const FFB_PERIOD_US: u32 = 1000;

/// Maximum spring coefficient
pub const MAX_SPRING_COEFFICIENT: i16 = 10000;

/// Maximum damper coefficient
pub const MAX_DAMPER_COEFFICIENT: i16 = 10000;

/// Maximum friction coefficient
pub const MAX_FRICTION_COEFFICIENT: i16 = 10000;

/// Deadzone range (in device units)
pub const DEFAULT_DEADZONE: i16 = 0;

/// Maximum effect duration in milliseconds
pub const MAX_EFFECT_DURATION_MS: u32 = 10000;

/// Effect gain range
pub const MAX_GAIN: u8 = 255;
pub const MIN_GAIN: u8 = 0;

/// Direction range
pub const MAX_DIRECTION_DEGREES: u16 = 36000; // 0.1 degree precision
pub const DIRECTION_NEUTRAL: u16 = 0;

/// Common effect IDs
pub const EFFECT_ID_NONE: u8 = 0;
pub const EFFECT_ID_CONSTANT: u8 = 1;
pub const EFFECT_ID_RAMP: u8 = 2;
pub const EFFECT_ID_SQUARE: u8 = 3;
pub const EFFECT_ID_SINE: u8 = 4;
pub const EFFECT_ID_TRIANGLE: u8 = 5;
pub const EFFECT_ID_SAWTOOTH_UP: u8 = 6;
pub const EFFECT_ID_SAWTOOTH_DOWN: u8 = 7;
pub const EFFECT_ID_SPRING: u8 = 8;
pub const EFFECT_ID_DAMPER: u8 = 9;
pub const EFFECT_ID_FRICTION: u8 = 10;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert!(MAX_TORQUE_CNM > 0);
        assert!(MAX_TORQUE_NM > 0.0);
        assert!(FFB_SAMPLE_RATE_HZ > 0);
        assert!(MAX_EFFECTS > 0);
    }
}
