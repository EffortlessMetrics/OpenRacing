//! Prelude module for common scheduler types.
//!
//! This module provides a convenient way to import the most commonly used
//! types from the scheduler crate.

pub use crate::adaptive::{AdaptiveSchedulingConfig, AdaptiveSchedulingState};
pub use crate::error::{RTError, RTResult};
pub use crate::jitter::JitterMetrics;
pub use crate::pll::PLL;
pub use crate::rt_setup::RTSetup;
pub use crate::scheduler::AbsoluteScheduler;
pub use crate::{MAX_JITTER_NS, PERIOD_1KHZ_NS};
