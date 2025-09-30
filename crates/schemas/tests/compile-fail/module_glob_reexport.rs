//! Test that module-level glob re-exports fail to compile

pub mod bad_module {
    // This should fail: attempting to re-export from a non-existent module
    pub use racing_wheel_schemas::nonexistent_module::*;
}

fn main() {}