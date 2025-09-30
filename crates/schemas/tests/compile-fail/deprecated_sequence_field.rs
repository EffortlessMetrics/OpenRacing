//! Test that deprecated sequence field fails to compile

use racing_wheel_schemas::telemetry::TelemetryData;

fn main() {
    // This should fail because sequence field was removed from TelemetryData
    let _data = TelemetryData {
        wheel_angle_deg: 0.0,
        wheel_speed_rad_s: 0.0,
        temperature_c: 45,
        fault_flags: 0,
        hands_on: true,
        timestamp: 0,
        sequence: 123,  // This field was removed
    };
}