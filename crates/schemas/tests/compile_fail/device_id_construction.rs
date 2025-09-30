// This test ensures that DeviceId construction is used correctly
// preventing infallible construction that bypasses validation

use racing_wheel_schemas::prelude::DeviceId;

fn main() {
    // This should fail to compile - no infallible constructor
    let _device_id = DeviceId::new("test-device".to_string()); //~ ERROR
    
    // This should also fail - no from_raw method
    let _device_id2 = DeviceId::from_raw("test-device".to_string()); //~ ERROR
}