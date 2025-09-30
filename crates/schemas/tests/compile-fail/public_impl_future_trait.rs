//! Test that deprecated telemetry field names fail to compile

use racing_wheel_schemas::telemetry::TelemetryData;

fn main() {
    // This should fail because wheel_angle_mdeg is not a field in TelemetryData
    let _data = TelemetryData {
        wheel_angle_mdeg: 1000,  // Should be wheel_angle_deg
        wheel_speed_rad_s: 0.0,
        temperature_c: 45,
        fault_flags: 0,
        hands_on: true,
        timestamp: 0,
    };
}