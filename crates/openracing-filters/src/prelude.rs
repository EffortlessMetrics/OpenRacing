//! Prelude for the filters crate.
//!
//! This module re-exports the most commonly used types and traits.
//!
//! # Example
//!
//! ```
//! use openracing_filters::prelude::*;
//!
//! let mut state = ReconstructionState::new(4);
//! let mut frame = Frame::default();
//! frame.ffb_in = 0.5;
//! frame.torque_out = 0.5;
//!
//! reconstruction_filter(&mut frame, &mut state);
//! ```

pub use crate::Frame;
pub use crate::bumpstop::{BumpstopState, bumpstop_filter};
pub use crate::curve::{CurveState, curve_filter};
pub use crate::damper::{DamperState, damper_filter};
pub use crate::friction::{FrictionState, friction_filter};
pub use crate::hands_off::{HandsOffState, hands_off_detector};
pub use crate::inertia::{InertiaState, inertia_filter};
pub use crate::notch::{NotchState, notch_filter};
pub use crate::reconstruction::{ReconstructionState, reconstruction_filter};
pub use crate::response_curve::{ResponseCurveState, response_curve_filter};
pub use crate::slew_rate::{SlewRateState, slew_rate_filter};
pub use crate::state::FilterState;
pub use crate::torque_cap_filter;
