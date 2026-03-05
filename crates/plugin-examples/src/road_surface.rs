//! Road surface simulation plugin — a DSP-style FFB effect.
//!
//! This plugin modifies the force-feedback signal to simulate road texture.
//! It layers a deterministic pseudo-random roughness pattern on top of the
//! incoming FFB value, scaled by vehicle speed and a configurable intensity.
//!
//! # Algorithm
//!
//! ```text
//! output = clamp(input + intensity * roughness(position), -1.0, 1.0)
//! ```
//!
//! The roughness function uses a simple hash of the discretised wheel
//! position so that the same road position always produces the same bump,
//! giving a convincing "textured" feel.
//!
//! # Real-time safety
//!
//! * No heap allocations after [`RoadSurfacePlugin::new`].
//! * All arithmetic is bounded and deterministic.
//! * Suitable for the 1 kHz native plugin path.

use openracing_plugin_abi::TelemetryFrame;

/// Configuration for the road-surface effect.
#[derive(Debug, Clone, Copy)]
pub struct RoadSurfaceConfig {
    /// Overall roughness intensity (0.0 = off, 1.0 = full).
    pub intensity: f32,
    /// Spatial frequency — bumps per radian of wheel travel.
    pub spatial_freq: f32,
    /// Speed scaling: at this speed (rad/s) the effect reaches full strength.
    pub full_speed_rad_s: f32,
}

impl Default for RoadSurfaceConfig {
    fn default() -> Self {
        Self {
            intensity: 0.3,
            spatial_freq: 20.0,
            full_speed_rad_s: 10.0,
        }
    }
}

/// Road surface FFB effect plugin.
///
/// Instantiate with [`RoadSurfacePlugin::new`], then call
/// [`process`](RoadSurfacePlugin::process) once per tick.
#[derive(Debug)]
pub struct RoadSurfacePlugin {
    config: RoadSurfaceConfig,
    /// Accumulated wheel position (radians) for deterministic roughness.
    accumulated_position: f64,
}

impl RoadSurfacePlugin {
    /// Create a new plugin with the given configuration.
    #[must_use]
    pub fn new(config: RoadSurfaceConfig) -> Self {
        Self {
            config,
            accumulated_position: 0.0,
        }
    }

    /// Process one tick of force feedback.
    ///
    /// * `ffb_input` — incoming FFB signal in the range `[-1.0, 1.0]`.
    /// * `telemetry` — current telemetry frame (used for wheel speed).
    /// * `dt` — time delta in seconds since the last call.
    ///
    /// Returns the modified FFB value, clamped to `[-1.0, 1.0]`.
    pub fn process(&mut self, ffb_input: f32, telemetry: &TelemetryFrame, dt: f32) -> f32 {
        // Integrate wheel position.
        self.accumulated_position += telemetry.wheel_speed_rad_s as f64 * dt as f64;

        // Speed-based scaling (0..1).
        let speed_factor = if self.config.full_speed_rad_s.abs() < f32::EPSILON {
            0.0
        } else {
            (telemetry.wheel_speed_rad_s.abs() / self.config.full_speed_rad_s).clamp(0.0, 1.0)
        };

        // Deterministic roughness from position hash.
        let roughness = roughness_at(self.accumulated_position, self.config.spatial_freq);

        let effect = self.config.intensity * speed_factor * roughness;
        (ffb_input + effect).clamp(-1.0, 1.0)
    }

    /// Reset accumulated position (e.g. on session restart).
    pub fn reset(&mut self) {
        self.accumulated_position = 0.0;
    }

    /// Read current accumulated wheel position (radians).
    #[must_use]
    pub fn accumulated_position(&self) -> f64 {
        self.accumulated_position
    }
}

/// Deterministic roughness value in `[-1.0, 1.0]` for a given position and
/// spatial frequency. Uses a simple integer hash for speed (no `std` required).
fn roughness_at(position: f64, spatial_freq: f32) -> f32 {
    let quantised = (position * spatial_freq as f64) as i64;
    let hash = simple_hash(quantised);
    // Map u32 → [-1.0, 1.0]
    (hash as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// Minimal integer hash (splitmix-style) — fast, no allocations.
fn simple_hash(value: i64) -> u32 {
    let mut x = value as u64;
    x = x.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (x ^ (x >> 31)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_telemetry(wheel_speed: f32) -> TelemetryFrame {
        TelemetryFrame {
            wheel_speed_rad_s: wheel_speed,
            ..TelemetryFrame::default()
        }
    }

    #[test]
    fn zero_intensity_passes_through() {
        let config = RoadSurfaceConfig {
            intensity: 0.0,
            ..Default::default()
        };
        let mut plugin = RoadSurfacePlugin::new(config);
        let telem = default_telemetry(5.0);
        let out = plugin.process(0.5, &telem, 0.001);
        assert!((out - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn zero_speed_produces_no_effect() {
        let mut plugin = RoadSurfacePlugin::new(RoadSurfaceConfig::default());
        let telem = default_telemetry(0.0);
        let out = plugin.process(0.5, &telem, 0.001);
        assert!((out - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn output_is_clamped() {
        let config = RoadSurfaceConfig {
            intensity: 1.0,
            full_speed_rad_s: 1.0,
            spatial_freq: 1.0,
        };
        let mut plugin = RoadSurfacePlugin::new(config);
        let telem = default_telemetry(100.0);
        for _ in 0..100 {
            let out = plugin.process(0.99, &telem, 0.001);
            assert!((-1.0..=1.0).contains(&out));
        }
    }

    #[test]
    fn deterministic_for_same_position() {
        let config = RoadSurfaceConfig::default();
        let mut p1 = RoadSurfacePlugin::new(config);
        let mut p2 = RoadSurfacePlugin::new(config);
        let telem = default_telemetry(3.0);
        let dt = 0.001;

        let a = p1.process(0.0, &telem, dt);
        let b = p2.process(0.0, &telem, dt);
        assert!((a - b).abs() < f32::EPSILON);
    }

    #[test]
    fn reset_clears_position() {
        let mut plugin = RoadSurfacePlugin::new(RoadSurfaceConfig::default());
        let telem = default_telemetry(5.0);
        let _ = plugin.process(0.0, &telem, 0.01);
        assert!(plugin.accumulated_position().abs() > f64::EPSILON);
        plugin.reset();
        assert!(plugin.accumulated_position().abs() < f64::EPSILON);
    }

    #[test]
    fn roughness_varies_with_position() {
        let a = roughness_at(0.0, 20.0);
        let b = roughness_at(1.0, 20.0);
        // Different positions should (almost certainly) produce different values.
        assert!((a - b).abs() > f32::EPSILON);
    }

    #[test]
    fn simple_hash_is_deterministic() {
        assert_eq!(simple_hash(42), simple_hash(42));
        assert_ne!(simple_hash(0), simple_hash(1));
    }

    #[test]
    fn full_speed_zero_does_not_panic() {
        let config = RoadSurfaceConfig {
            intensity: 1.0,
            full_speed_rad_s: 0.0,
            spatial_freq: 10.0,
        };
        let mut plugin = RoadSurfacePlugin::new(config);
        let telem = default_telemetry(5.0);
        let out = plugin.process(0.5, &telem, 0.001);
        assert!((out - 0.5).abs() < f32::EPSILON);
    }
}
