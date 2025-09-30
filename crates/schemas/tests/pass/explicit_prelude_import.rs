//! Test that explicit prelude imports compile successfully

// This should compile successfully: explicit prelude usage
use racing_wheel_schemas::prelude::*;

fn main() {
    // Use types from the prelude
    let _device_id = DeviceId::try_from("test-device".to_string()).unwrap();
    let _torque = TorqueNm::new(5.0).unwrap();
    let _filter_config = FilterConfig::default();
}