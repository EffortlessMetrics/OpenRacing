// This test ensures that deprecated field names fail to compile
// preventing accidental usage of old schema field names

use racing_wheel_schemas::prelude::*;

fn main() {
    let telemetry = TelemetryData::default();
    
    // These should fail to compile - deprecated field names
    let _old_angle = telemetry.wheel_angle_mdeg; //~ ERROR
    let _old_speed = telemetry.wheel_speed_mrad_s; //~ ERROR  
    let _old_temp = telemetry.temp_c; //~ ERROR
    let _old_faults = telemetry.faults; //~ ERROR
    let _old_sequence = telemetry.sequence; //~ ERROR
}