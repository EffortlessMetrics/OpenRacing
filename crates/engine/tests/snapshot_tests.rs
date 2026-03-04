//! Snapshot tests for engine serialization formats

use racing_wheel_engine::{FFBMode, Frame, TorqueCommand};

// --- Frame snapshots ---

#[test]
fn snapshot_frame_default() {
    insta::assert_json_snapshot!("frame_default", Frame::default());
}

#[test]
fn snapshot_frame_typical_values() {
    let frame = Frame {
        ffb_in: 0.75,
        torque_out: 0.68,
        wheel_speed: 3.5,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 42,
    };
    insta::assert_json_snapshot!("frame_typical", frame);
}

#[test]
fn snapshot_frame_negative_input() {
    let frame = Frame {
        ffb_in: -0.5,
        torque_out: -0.45,
        wheel_speed: -1.2,
        hands_off: true,
        ts_mono_ns: 5_000_000,
        seq: 100,
    };
    insta::assert_json_snapshot!("frame_negative_input", frame);
}

// --- FFBMode snapshots ---

#[test]
fn snapshot_ffb_mode_pid_passthrough() {
    insta::assert_debug_snapshot!("ffb_mode_pid_passthrough", FFBMode::PidPassthrough);
}

#[test]
fn snapshot_ffb_mode_raw_torque() {
    insta::assert_debug_snapshot!("ffb_mode_raw_torque", FFBMode::RawTorque);
}

#[test]
fn snapshot_ffb_mode_telemetry_synth() {
    insta::assert_debug_snapshot!("ffb_mode_telemetry_synth", FFBMode::TelemetrySynth);
}

// --- TorqueCommand snapshots ---

#[test]
fn snapshot_torque_command_zero() {
    let cmd = TorqueCommand {
        report_id: 0x20,
        torque_mnm: 0,
        flags: 0,
        sequence: 0,
        crc8: 0,
    };
    insta::assert_debug_snapshot!("torque_command_zero", cmd);
}

#[test]
fn snapshot_torque_command_typical() {
    let cmd = TorqueCommand {
        report_id: 0x20,
        torque_mnm: 5000,
        flags: 0x01,
        sequence: 42,
        crc8: 0xAB,
    };
    insta::assert_debug_snapshot!("torque_command_typical", cmd);
}
