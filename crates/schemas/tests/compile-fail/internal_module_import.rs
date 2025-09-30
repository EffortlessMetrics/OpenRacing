//! Test that cross-crate internal module imports fail to compile

// This should fail: cannot import internal modules from other crates
use racing_wheel_schemas::ipc_conversion_tests;

fn main() {}