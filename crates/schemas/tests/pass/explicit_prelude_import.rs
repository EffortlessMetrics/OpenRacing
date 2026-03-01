//! Test that explicit prelude imports compile successfully

// This should compile successfully: explicit prelude usage
use racing_wheel_schemas::prelude::*;

fn main() {
    // Use types from the prelude
    let _device_id = match DeviceId::try_from("test-device".to_string()) {
        Ok(id) => id,
        Err(e) => panic!("Failed to create device_id: {}", e),
    };
    let _torque = match TorqueNm::new(5.0) {
        Ok(t) => t,
        Err(e) => panic!("Failed to create torque: {}", e),
    };
    let _filter_config = FilterConfig::default();
}