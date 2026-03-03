//! Test that SafetyService internal state cannot be mutated directly.
//!
//! The safety state machine must only transition through the public API
//! methods (request_high_torque, provide_ui_consent, report_fault, etc.)
//! to enforce the interlock protocol.

use racing_wheel_engine::safety::SafetyService;

fn main() {
    let mut service = SafetyService::default();

    // This should fail: `state` field is pub(crate), not pub
    service.state = racing_wheel_engine::safety::SafetyState::HighTorqueActive {
        //~^ ERROR
        since: std::time::Instant::now(),
        device_token: 0xDEAD,
        last_hands_on: std::time::Instant::now(),
    };
}
