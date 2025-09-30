// This test ensures that correct schema usage compiles successfully

use racing_wheel_schemas::prelude::*;

fn main() {
    // Correct TelemetryData usage with new field names
    let telemetry = TelemetryData {
        wheel_angle_deg: 45.0,
        wheel_speed_rad_s: 2.5,
        temperature_c: 25,
        fault_flags: 0,
        hands_on: true,
        timestamp: 1234567890,
    };
    
    // Correct FilterConfig usage with all required fields
    let config = FilterConfig::default();
    
    // Correct DeviceId usage - must use fallible construction
    let device_id: DeviceId = "test-device".parse().unwrap();
    let device_id2 = DeviceId::try_from("another-device".to_string()).unwrap();
    
    println!("All schema usage is correct!");
    println!("Telemetry angle: {}", telemetry.wheel_angle_deg);
    println!("Config reconstruction: {}", config.reconstruction);
    println!("Device ID: {}", device_id);
    println!("Device ID 2: {}", device_id2.as_ref());
}