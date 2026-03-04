//! Test that SafetyInterlockSystem private fields cannot be constructed directly.
//!
//! The safety interlock system must be constructed through its public API
//! (SafetyInterlockSystem::new) to ensure a valid watchdog is always provided.

use racing_wheel_engine::safety::{SafetyInterlockState, SafetyInterlockSystem};

fn main() {
    // This should fail: private fields prevent direct construction
    let _system = SafetyInterlockSystem {
        //~^ ERROR
        watchdog: todo!(),
        timeout_handler: todo!(),
        safety_state: SafetyInterlockState::Normal,
        torque_limit: todo!(),
        fault_log: vec![],
        max_fault_log_entries: 100,
        fault_log_next_index: 0,
        communication_timeout: std::time::Duration::from_millis(50),
        last_communication: None,
    };
}
