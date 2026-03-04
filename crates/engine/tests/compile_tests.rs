//! Trybuild compile-fail tests for engine API safety contracts.
//!
//! These tests ensure that safety-critical invariants are enforced at
//! compile time:
//! - Private fields prevent direct construction of safety types
//! - Internal state cannot be mutated to bypass the safety protocol
//! - RT-internal types are not publicly accessible

#[test]
fn safety_compile_fail_tests() {
    let t = trybuild::TestCases::new();

    // SafetyInterlockSystem cannot be constructed without going through ::new()
    t.compile_fail("tests/compile_fail/safety_interlock_private_fields.rs");

    // SafetyService.state is pub(crate) — cannot be set from outside the crate
    t.compile_fail("tests/compile_fail/safety_service_private_state.rs");

    // RTContext is a private struct — cannot be imported from outside
    t.compile_fail("tests/compile_fail/rt_context_not_public.rs");

    // Engine private fields prevent bypassing safety pipeline
    t.compile_fail("tests/compile_fail/torque_command_without_safety.rs");
}
