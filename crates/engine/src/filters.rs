//! Filter Node Library for Real-Time Force Feedback Processing
//!
//! This module provides filter types and functions for the FFB pipeline.
//!
//! See the `openracing-filters` crate for detailed filter documentation.

pub use openracing_filters::{
    BumpstopState, CurveState, DamperState, FilterState, FrictionState, HandsOffState,
    InertiaState, NotchState, ReconstructionState, ResponseCurveState, SlewRateState,
};

use crate::rt::Frame;

/// Reconstruction filter (anti-aliasing) - smooths high-frequency content
pub fn reconstruction_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &mut *(state as *mut ReconstructionState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::reconstruction_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Friction filter with speed adaptation - simulates tire/road friction
pub fn friction_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &*(state as *const FrictionState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::friction_filter(&mut filter_frame, &state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Damper filter with speed adaptation - velocity-proportional resistance
pub fn damper_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &*(state as *const DamperState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::damper_filter(&mut filter_frame, &state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Inertia filter - simulates rotational inertia
pub fn inertia_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &mut *(state as *mut InertiaState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::inertia_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Notch filter (biquad implementation) - eliminates specific frequencies
pub fn notch_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &mut *(state as *mut NotchState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::notch_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Slew rate limiter - limits rate of change
pub fn slew_rate_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &mut *(state as *mut SlewRateState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::slew_rate_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Curve mapping filter using lookup table - applies force curve
pub fn curve_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &*(state as *const CurveState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::curve_filter(&mut filter_frame, &state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Response curve filter using CurveLut - applies response curve transformation
pub fn response_curve_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &*(state as *const ResponseCurveState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::response_curve_filter(&mut filter_frame, &state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Torque cap filter (safety) - limits maximum torque
pub fn torque_cap_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let max_torque = *(state as *const f32);
        frame.torque_out = if frame.torque_out.is_finite() {
            frame.torque_out.clamp(-max_torque, max_torque)
        } else {
            max_torque
        };
    }
}

/// Bumpstop model filter - simulates physical steering stops
pub fn bumpstop_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &mut *(state as *mut BumpstopState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::bumpstop_filter(&mut filter_frame, state);
        frame.torque_out = filter_frame.torque_out;
    }
}

/// Hands-off detector - detects when user is not holding the wheel
pub fn hands_off_detector(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &mut *(state as *mut HandsOffState);
        let mut filter_frame = openracing_filters::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        openracing_filters::hands_off_detector(&mut filter_frame, state);
        frame.hands_off = filter_frame.hands_off;
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
    fn test_reconstruction_filter() {
        let mut state = ReconstructionState::new(4);
        let mut frame = create_test_frame(1.0, 0.0);
        reconstruction_filter(&mut frame, &mut state as *mut _ as *mut u8);

        assert!(frame.torque_out < 1.0);
        assert!(frame.torque_out > 0.0);
    }

    #[test]
    fn test_friction_filter() {
        let state = FrictionState::new(0.1, true);
        let mut frame = create_test_frame(0.0, 1.0);
        friction_filter(&mut frame, &state as *const _ as *mut u8);

        assert!(frame.torque_out.abs() > 0.0);
    }

    #[test]
    fn test_damper_filter() {
        let state = DamperState::new(0.1, true);
        let mut frame = create_test_frame(0.0, 1.0);
        damper_filter(&mut frame, &state as *const _ as *mut u8);

        assert!(frame.torque_out.abs() > 0.0);
    }

    #[test]
    fn test_torque_cap_filter() {
        let mut frame = create_test_frame(1.0, 0.0);
        frame.torque_out = 1.0;
        let max_torque = 0.8f32;
        torque_cap_filter(&mut frame, &max_torque as *const _ as *mut u8);

        assert!((frame.torque_out - 0.8).abs() < 0.001);
    }
}
