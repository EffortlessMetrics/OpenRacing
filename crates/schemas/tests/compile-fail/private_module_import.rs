//! Test that cross-crate private module imports fail to compile

// This should fail: cannot import private modules from other crates
use racing_wheel_schemas::validation_tests;

fn main() {}