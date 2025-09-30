// This test ensures that DeviceId literal construction hacks fail to compile
// preventing unsafe construction that bypasses validation

use racing_wheel_schemas::prelude::DeviceId;

fn main() {
    // These should fail to compile - literal construction hacks
    let _device_id1 = DeviceId("literal-string".to_string()); //~ ERROR
    let _device_id2 = DeviceId("another-literal"); //~ ERROR
    
    // This should also fail - direct struct construction
    let _device_id3 = DeviceId { 0: "field-access".to_string() }; //~ ERROR
}