//! Math utilities for HID protocol handling.

/// A safe version of clamp that handles NaN by returning the minimum value (or 0.0).
///
/// In Rust, `f32::clamp` panics if the value is NaN. This is a safety hazard in
/// real-time code paths where a buggy plugin might return NaN torque.
#[inline(always)]
pub fn safe_clamp(val: f32, min: f32, max: f32) -> f32 {
    if val.is_nan() {
        return min.max(0.0).min(max); // Default to 0.0 if within range, else min
    }
    val.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_clamp_nan() {
        assert_eq!(safe_clamp(f32::NAN, -1.0, 1.0), 0.0);
        assert_eq!(safe_clamp(f32::NAN, 2.0, 5.0), 2.0);
        assert_eq!(safe_clamp(f32::NAN, -10.0, -5.0), -5.0); // 0.0 is not in range, returns max(-10).min(-5) -> -5? wait
        // min.max(0.0) -> 0.0. 0.0.min(-5.0) -> -5.0. Correct.
    }

    #[test]
    fn test_safe_clamp_normal() {
        assert_eq!(safe_clamp(0.5, -1.0, 1.0), 0.5);
        assert_eq!(safe_clamp(2.0, -1.0, 1.0), 1.0);
        assert_eq!(safe_clamp(-2.0, -1.0, 1.0), -1.0);
    }
}
