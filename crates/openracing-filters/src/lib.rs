//! RT-Safe Filter Implementations for OpenRacing
//!
//! This crate provides real-time safe filter implementations for the force feedback
//! pipeline. All filters are designed to operate at 1kHz with strict timing requirements.
//!
//! # Overview
//!
//! The filter system includes:
//! - **Reconstruction**: Anti-aliasing filter for smoothing high-frequency content
//! - **Friction**: Speed-adaptive friction simulation
//! - **Damper**: Speed-adaptive velocity-proportional resistance
//! - **Inertia**: Rotational inertia simulation
//! - **Notch**: Biquad notch filter for eliminating specific frequencies
//! - **Slew Rate**: Rate-of-change limiter
//! - **Curve**: Lookup table-based curve mapping
//! - **Response Curve**: Response curve transformation
//! - **Bumpstop**: Physical steering stop simulation
//! - **Hands-Off**: Detection of user hands-off condition
//!
//! # RT Safety Guarantees
//!
//! All filter implementations are RT-safe:
//! - No heap allocations in filter hot paths
//! - O(1) time complexity for all operations
//! - Bounded execution time
//! - No syscalls or I/O in filter functions
//! - All state types are `#[repr(C)]` for stable ABI
//!
//! # Example
//!
//! ```
//! use openracing_filters::prelude::*;
//!
//! // Create filter states at initialization time
//! let mut recon_state = ReconstructionState::new(4);
//! let mut slew_state = SlewRateState::new(0.5);
//!
//! // In the RT loop (1kHz):
//! let mut frame = Frame::default();
//! frame.ffb_in = 0.5;
//! frame.torque_out = 0.5;
//!
//! // Apply filters (RT-safe)
//! reconstruction_filter(&mut frame, &mut recon_state);
//! slew_rate_filter(&mut frame, &mut slew_state);
//! ```

#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![warn(missing_docs)]
#![warn(missing_debug_implementations)]

pub mod bumpstop;
pub mod curve;
pub mod damper;
pub mod friction;
pub mod hands_off;
pub mod inertia;
pub mod notch;
pub mod prelude;
pub mod reconstruction;
pub mod response_curve;
pub mod slew_rate;
pub mod state;

pub use bumpstop::{BumpstopState, bumpstop_filter};
pub use curve::{CurveState, curve_filter};
pub use damper::{DamperState, damper_filter};
pub use friction::{FrictionState, friction_filter};
pub use hands_off::{HandsOffState, hands_off_detector};
pub use inertia::{InertiaState, inertia_filter};
pub use notch::{NotchState, notch_filter};
pub use reconstruction::{ReconstructionState, reconstruction_filter};
pub use response_curve::{ResponseCurveState, response_curve_filter};
pub use slew_rate::{SlewRateState, slew_rate_filter};
pub use state::*;

/// Real-time frame data processed at 1kHz
///
/// This is a minimal frame type for filter processing.
/// The full frame type is defined in the engine crate.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Frame {
    /// Force feedback input from game (-1.0 to 1.0)
    pub ffb_in: f32,
    /// Torque output after filtering (-1.0 to 1.0)
    pub torque_out: f32,
    /// Wheel angular velocity in rad/s for speed-adaptive filters
    pub wheel_speed: f32,
    /// Hands-off detection flag
    pub hands_off: bool,
    /// Monotonic timestamp in nanoseconds
    pub ts_mono_ns: u64,
    /// Sequence number for device communication
    pub seq: u16,
}

/// Torque cap filter (safety) - limits maximum torque
///
/// # RT Safety
///
/// - No heap allocations
/// - O(1) time complexity
/// - Bounded execution time
///
/// # Arguments
///
/// * `frame` - The frame to process
/// * `max_torque` - Maximum allowed torque magnitude (must be positive)
#[inline]
pub fn torque_cap_filter(frame: &mut Frame, max_torque: f32) {
    if frame.torque_out.is_finite() {
        frame.torque_out = frame.torque_out.clamp(-max_torque, max_torque);
    } else {
        // Handle non-finite values: clamp to max torque with sign
        frame.torque_out = max_torque;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_frame(ffb_in: f32, wheel_speed: f32) -> Frame {
        Frame {
            ffb_in,
            torque_out: ffb_in,
            wheel_speed,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        }
    }

    #[test]
    fn test_torque_cap_filter() {
        let max_torque = 0.8f32;

        let mut frame_ok = create_test_frame(0.5, 0.0);
        frame_ok.torque_out = 0.5;
        torque_cap_filter(&mut frame_ok, max_torque);
        assert!((frame_ok.torque_out - 0.5).abs() < 0.001);

        let mut frame_over = create_test_frame(1.0, 0.0);
        frame_over.torque_out = 1.0;
        torque_cap_filter(&mut frame_over, max_torque);
        assert!((frame_over.torque_out - 0.8).abs() < 0.001);

        let mut frame_neg = create_test_frame(-1.0, 0.0);
        frame_neg.torque_out = -1.0;
        torque_cap_filter(&mut frame_neg, max_torque);
        assert!((frame_neg.torque_out - (-0.8)).abs() < 0.001);
    }
}
