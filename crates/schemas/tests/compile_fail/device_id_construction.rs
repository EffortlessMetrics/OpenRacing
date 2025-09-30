// This test ensures that DeviceId construction is used correctly
// preventing double-wrapping and type mismatches

use racing_wheel_schemas::device::DeviceId;

fn main() {
    let device_id = DeviceId::new("test-device".to_string()).unwrap();
    
    // This should fail to compile - double wrapping DeviceId
    let _wrapped = DeviceId::new(device_id); //~ ERROR
}