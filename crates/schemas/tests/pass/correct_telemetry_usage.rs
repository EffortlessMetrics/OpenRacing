//! Test that correct telemetry field usage compiles successfully

use racing_wheel_schemas::telemetry::TelemetryData;

fn main() {
    // This should compile successfully with correct field names
    let _data = TelemetryData {
        wheel_angle_deg: 0.0,      // Correct field name
        wheel_speed_rad_s: 0.0,    // Correct field name
        temperature_c: 45,         // Correct field name
        fault_flags: 0,            // Correct field name
        hands_on: true,
        timestamp: 0,
    };
    
    // Test default construction
    let _default_data = TelemetryData::default();
}