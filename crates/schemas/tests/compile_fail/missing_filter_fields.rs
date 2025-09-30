// This test ensures that FilterConfig requires all new fields
// preventing compilation when required fields are missing

use racing_wheel_schemas::config::FilterConfig;

fn main() {
    // This should fail to compile - missing required fields
    let _config = FilterConfig {
        reconstruction: 4,
        friction: 0.12,
        damper: 0.18,
        inertia: 0.08,
        // Missing: bumpstop, hands_off, torque_cap, notch_filters, slew_rate, curve_points
    }; //~ ERROR
}