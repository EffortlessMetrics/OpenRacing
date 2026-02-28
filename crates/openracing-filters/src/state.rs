//! Filter State Types
//!
//! This module aggregates all filter state types for convenient access.
//! All state types are `#[repr(C)]` for stable ABI and RT-safe usage.

pub use crate::bumpstop::BumpstopState;
pub use crate::curve::CurveState;
pub use crate::damper::DamperState;
pub use crate::friction::FrictionState;
pub use crate::hands_off::HandsOffState;
pub use crate::inertia::InertiaState;
pub use crate::notch::NotchState;
pub use crate::reconstruction::ReconstructionState;
pub use crate::response_curve::ResponseCurveState;
pub use crate::slew_rate::SlewRateState;

/// Filter trait for common filter operations.
///
/// All filters implement this trait for consistent interface.
pub trait FilterState: Copy + Clone + std::fmt::Debug {
    /// Reset the filter state to initial values.
    fn reset(&mut self);
}

impl FilterState for ReconstructionState {
    fn reset(&mut self) {
        self.prev_output = 0.0;
    }
}

impl FilterState for FrictionState {
    fn reset(&mut self) {
        // Friction state has no dynamic state to reset
    }
}

impl FilterState for DamperState {
    fn reset(&mut self) {
        // Damper state has no dynamic state to reset
    }
}

impl FilterState for InertiaState {
    fn reset(&mut self) {
        self.prev_wheel_speed = 0.0;
    }
}

impl FilterState for NotchState {
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

impl FilterState for SlewRateState {
    fn reset(&mut self) {
        self.prev_output = 0.0;
    }
}

impl FilterState for CurveState {
    fn reset(&mut self) {
        // Curve state is a LUT, no dynamic state to reset
    }
}

impl FilterState for ResponseCurveState {
    fn reset(&mut self) {
        // Response curve state is a LUT, no dynamic state to reset
    }
}

impl FilterState for BumpstopState {
    fn reset(&mut self) {
        self.current_angle = 0.0;
    }
}

impl FilterState for HandsOffState {
    fn reset(&mut self) {
        self.counter = 0;
        self.last_torque = 0.0;
    }
}
