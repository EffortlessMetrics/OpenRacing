// This test ensures that correct schema usage compiles successfully

use racing_wheel_schemas::{
    telemetry::TelemetryData,
    config::FilterConfig,
    device::DeviceId,
};

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
    
    // Correct DeviceId usage
    let device_id = DeviceId::new("test-device".to_string()).unwrap();
    
    println!("All schema usage is correct!");
    println!("Telemetry angle: {}", telemetry.wheel_angle_deg);
    println!("Config reconstruction: {}", config.reconstruction);
}