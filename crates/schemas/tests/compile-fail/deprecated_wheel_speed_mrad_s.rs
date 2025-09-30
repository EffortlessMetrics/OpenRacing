//! Test that deprecated wheel_speed_mrad_s field fails to compile

use racing_wheel_schemas::telemetry::TelemetryData;

fn main() {
    // This should fail because wheel_speed_mrad_s is not a field in TelemetryData
    let _data = TelemetryData {
        wheel_angle_deg: 0.0,
        wheel_speed_mrad_s: 0,  // Should be wheel_speed_rad_s
        temperature_c: 45,
        fault_flags: 0,
        hands_on: true,
        timestamp: 0,
    };
}