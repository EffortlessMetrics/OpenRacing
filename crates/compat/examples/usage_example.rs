//! Example usage of the TelemetryCompat trait
//!
//! This example demonstrates how the compatibility layer allows gradual migration
//! from old field names to new field names in test code.

fn main() {
    println!("This is an example of the TelemetryCompat trait usage.");
    println!("Run `cargo test -p compat` to see the compatibility layer in action.");
}

#[cfg(test)]
mod example {
    use std::time::Instant;

    // This would normally be imported from the engine crate
    #[derive(Debug, Clone)]
    pub struct TelemetryData {
        pub wheel_angle_deg: f32,
        pub wheel_speed_rad_s: f32,
        pub temperature_c: u8,
        pub fault_flags: u8,
        pub hands_on: bool,
        pub timestamp: Instant,
    }

    // Import the compat trait
    use crate::TelemetryCompat;

    // Implement the trait for our TelemetryData
    impl TelemetryCompat for TelemetryData {
        fn temp_c(&self) -> u8 {
            self.temperature_c
        }
        fn faults(&self) -> u8 {
            self.fault_flags
        }
        fn wheel_angle_mdeg(&self) -> i32 {
            (self.wheel_angle_deg * 1000.0) as i32
        }
        fn wheel_speed_mrad_s(&self) -> i32 {
            (self.wheel_speed_rad_s * 1000.0) as i32
        }
        fn sequence(&self) -> u32 {
            0
        } // Removed field
    }

    #[test]
    fn example_migration_usage() {
        let telemetry = TelemetryData {
            wheel_angle_deg: 90.0,
            wheel_speed_rad_s: 5.0,
            temperature_c: 45,
            fault_flags: 0x02,
            hands_on: true,
            timestamp: Instant::now(),
        };

        // Old code using compat layer (to be migrated)
        assert_eq!(telemetry.temp_c(), 45);
        assert_eq!(telemetry.faults(), 0x02);
        assert_eq!(telemetry.wheel_angle_mdeg(), 90000);
        assert_eq!(telemetry.wheel_speed_mrad_s(), 5000);
        assert_eq!(telemetry.sequence(), 0);

        // New code using direct field access (migration target)
        assert_eq!(telemetry.temperature_c, 45);
        assert_eq!(telemetry.fault_flags, 0x02);
        assert_eq!((telemetry.wheel_angle_deg * 1000.0) as i32, 90000);
        assert_eq!((telemetry.wheel_speed_rad_s * 1000.0) as i32, 5000);
        // sequence field was removed - no direct equivalent
    }
}
