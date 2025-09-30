//! Test that root-level glob re-exports fail to compile

// This should fail: attempting to re-export from a non-existent module
pub use racing_wheel_schemas::nonexistent::*;

fn main() {}