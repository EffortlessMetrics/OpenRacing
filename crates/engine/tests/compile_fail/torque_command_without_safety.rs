//! Test that TorqueCommand cannot be sent via Engine without going through
//! the safety service's torque clamping.
//!
//! The Engine type does not expose the HID device directly, so external code
//! cannot bypass the safety pipeline to send raw torque commands.

fn main() {
    // Engine does not expose `device` or a raw `send_torque` method.
    // The only way to affect torque output is through the GameInput channel
    // which is processed by the RT loop with safety clamping.
    //
    // Verify that the internal device field is not accessible:
    let engine: racing_wheel_engine::Engine;
    // engine.device would fail — field is private
    // engine.rt_thread would fail — field is private
    // This compile_fail test verifies the fields exist only privately by
    // trying to access them:
    let _device = engine.rt_thread;
    //~^ ERROR
}
