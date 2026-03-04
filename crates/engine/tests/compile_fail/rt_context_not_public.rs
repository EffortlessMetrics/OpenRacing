//! Test that RTContext (the RT thread's internal state) is not publicly accessible.
//!
//! The RT loop context must remain private to prevent external code from
//! bypassing the engine's command channel and directly mutating RT state.

use racing_wheel_engine::engine::RTContext;
//~^ ERROR

fn main() {}
