//! Test that deprecated faults field fails to compile

use racing_wheel_schemas::telemetry::TelemetryData;

fn main() {
    // This should fail because faults is not a field in TelemetryData
    let _data = TelemetryData {
        wheel_angle_deg: 0.0,
        wheel_speed_rad_s: 0.0,
        temperature_c: 45,
        faults: 0,  // Should be fault_flags
        hands_on: true,
        timestamp: 0,
    };
}