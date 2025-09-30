//! Test that deprecated telemetry field temp_c fails to compile

use racing_wheel_schemas::telemetry::TelemetryData;

fn main() {
    // This should fail because temp_c is not a field in TelemetryData
    let _data = TelemetryData {
        wheel_angle_deg: 0.0,
        wheel_speed_rad_s: 0.0,
        temp_c: 45,  // Should be temperature_c
        fault_flags: 0,
        hands_on: true,
        timestamp: 0,
    };
}