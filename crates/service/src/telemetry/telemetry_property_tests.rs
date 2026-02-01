//! Property-based tests for telemetry parsing performance
//!
//! Feature: release-roadmap-v1, Property 22: Telemetry Parsing Performance
//! **Validates: Requirements 12.5**
//!
//! For any valid telemetry packet from a supported game, parsing SHALL complete within 1ms.

use proptest::prelude::*;
use std::time::Instant;

use crate::telemetry::TelemetryAdapter;
use crate::telemetry::adapters::{
    acc::ACCAdapter, ams2::AMS2Adapter, iracing::IRacingAdapter, rfactor2::RFactor2Adapter,
};

/// Maximum allowed parsing time in nanoseconds (1ms = 1,000,000ns)
const MAX_PARSING_TIME_NS: u128 = 1_000_000;

/// Type alias for test results
type TestResult = Result<(), TestCaseError>;

// ============================================================================
// iRacing Telemetry Data Generator
// ============================================================================

/// iRacing shared memory data structure (must match iracing.rs)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct IRacingData {
    session_time: f32,
    session_flags: u32,
    speed: f32,
    rpm: f32,
    gear: i8,
    throttle: f32,
    brake: f32,
    steering_wheel_angle: f32,
    steering_wheel_torque: f32,
    lf_tire_rps: f32,
    rf_tire_rps: f32,
    lr_tire_rps: f32,
    rr_tire_rps: f32,
    lap_current: i32,
    lap_best_time: f32,
    fuel_level: f32,
    on_pit_road: i32,
    car_path: [u8; 64],
    track_name: [u8; 64],
}

impl Default for IRacingData {
    fn default() -> Self {
        Self {
            session_time: 0.0,
            session_flags: 0,
            speed: 0.0,
            rpm: 0.0,
            gear: 0,
            throttle: 0.0,
            brake: 0.0,
            steering_wheel_angle: 0.0,
            steering_wheel_torque: 0.0,
            lf_tire_rps: 0.0,
            rf_tire_rps: 0.0,
            lr_tire_rps: 0.0,
            rr_tire_rps: 0.0,
            lap_current: 0,
            lap_best_time: 0.0,
            fuel_level: 0.0,
            on_pit_road: 0,
            car_path: [0; 64],
            track_name: [0; 64],
        }
    }
}

/// Strategy for generating valid iRacing telemetry data
/// Uses nested tuples to work around proptest's 12-element tuple limit
fn iracing_data_strategy() -> impl Strategy<Value = IRacingData> {
    // Group 1: Core dynamics (8 elements)
    let dynamics = (
        0.0f32..10000.0,  // session_time
        any::<u32>(),     // session_flags
        0.0f32..100.0,    // speed (m/s)
        0.0f32..15000.0,  // rpm
        -1i8..8,          // gear
        0.0f32..1.0,      // throttle
        0.0f32..1.0,      // brake
        -720.0f32..720.0, // steering_wheel_angle
    );

    // Group 2: Additional data (8 elements)
    let additional = (
        -100.0f32..100.0, // steering_wheel_torque
        0.0f32..200.0,    // lf_tire_rps
        0.0f32..200.0,    // rf_tire_rps
        0.0f32..200.0,    // lr_tire_rps
        0.0f32..200.0,    // rr_tire_rps
        0i32..100,        // lap_current
        0.0f32..300.0,    // lap_best_time
        0.0f32..120.0,    // fuel_level
    );

    (dynamics, additional).prop_map(|(d, a)| {
        let mut data = IRacingData {
            session_time: d.0,
            session_flags: d.1,
            speed: d.2,
            rpm: d.3,
            gear: d.4,
            throttle: d.5,
            brake: d.6,
            steering_wheel_angle: d.7,
            steering_wheel_torque: a.0,
            lf_tire_rps: a.1,
            rf_tire_rps: a.2,
            lr_tire_rps: a.3,
            rr_tire_rps: a.4,
            lap_current: a.5,
            lap_best_time: a.6,
            fuel_level: a.7,
            ..Default::default()
        };
        // Add car and track names
        let car_name = b"gt3_test_car\0";
        let track_name = b"test_track\0";
        data.car_path[..car_name.len()].copy_from_slice(car_name);
        data.track_name[..track_name.len()].copy_from_slice(track_name);
        data
    })
}

// ============================================================================
// ACC Telemetry Data Generator
// ============================================================================

/// ACC telemetry packet ID
const ACC_TELEMETRY_PACKET_ID: u32 = 0x12345678;

/// ACC telemetry data structure (must match acc.rs)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct ACCTelemetryData {
    packet_id: u32,
    speed: f32,
    rpm: f32,
    gear: i8,
    gas: f32,
    brake: f32,
    steer_angle: f32,
    wheel_slip: [f32; 4],
    wheel_load: [f32; 4],
    wheel_pressure: [f32; 4],
    wheel_angular_speed: [f32; 4],
    completed_laps: i32,
    current_lap_time: f32,
    last_lap: f32,
    best_lap: f32,
    fuel: f32,
    tc: u8,
    abs: u8,
    ers_recovery_level: f32,
    ers_power_level: f32,
    drs_available: u8,
    drs_enabled: u8,
    flag: u8,
    is_in_pits: u8,
    pit_limiter_on: u8,
    car_model: [u8; 32],
    track: [u8; 32],
    _padding: [u8; 64],
}

impl Default for ACCTelemetryData {
    fn default() -> Self {
        Self {
            packet_id: ACC_TELEMETRY_PACKET_ID,
            speed: 0.0,
            rpm: 0.0,
            gear: 0,
            gas: 0.0,
            brake: 0.0,
            steer_angle: 0.0,
            wheel_slip: [0.0; 4],
            wheel_load: [0.0; 4],
            wheel_pressure: [0.0; 4],
            wheel_angular_speed: [0.0; 4],
            completed_laps: 0,
            current_lap_time: 0.0,
            last_lap: 0.0,
            best_lap: 0.0,
            fuel: 0.0,
            tc: 0,
            abs: 0,
            ers_recovery_level: 0.0,
            ers_power_level: 0.0,
            drs_available: 0,
            drs_enabled: 0,
            flag: 0,
            is_in_pits: 0,
            pit_limiter_on: 0,
            car_model: [0; 32],
            track: [0; 32],
            _padding: [0; 64],
        }
    }
}

/// Strategy for generating valid ACC telemetry data
fn acc_data_strategy() -> impl Strategy<Value = ACCTelemetryData> {
    // Group 1: Core dynamics (8 elements)
    // Note: ACC validates speed in 0.0..=200.0 km/h and rpm in 0.0..=20000.0
    let dynamics = (
        0.0f32..200.0,    // speed (km/h) - valid range per ACC validation
        0.0f32..20000.0,  // rpm - valid range per ACC validation
        -1i8..8,          // gear
        0.0f32..1.0,      // gas
        0.0f32..1.0,      // brake
        -450.0f32..450.0, // steer_angle (degrees)
        0i32..100,        // completed_laps
        0.0f32..300.0,    // current_lap_time
    );

    // Group 2: Additional data (8 elements)
    let additional = (
        0.0f32..300.0,                      // last_lap
        0.0f32..300.0,                      // best_lap
        0.0f32..120.0,                      // fuel
        0u8..12,                            // tc level
        0u8..12,                            // abs level
        any::<u8>(),                        // flag
        0u8..2,                             // is_in_pits
        prop::array::uniform4(0.0f32..1.0), // wheel_slip
    );

    (dynamics, additional).prop_map(|(d, a)| {
        let mut data = ACCTelemetryData {
            packet_id: ACC_TELEMETRY_PACKET_ID,
            speed: d.0,
            rpm: d.1,
            gear: d.2,
            gas: d.3,
            brake: d.4,
            steer_angle: d.5,
            completed_laps: d.6,
            current_lap_time: d.7,
            last_lap: a.0,
            best_lap: a.1,
            fuel: a.2,
            tc: a.3,
            abs: a.4,
            flag: a.5,
            is_in_pits: a.6,
            wheel_slip: a.7,
            ..Default::default()
        };
        // Add car and track names
        let car_name = b"ferrari_488_gt3\0";
        let track_name = b"monza\0";
        data.car_model[..car_name.len()].copy_from_slice(car_name);
        data.track[..track_name.len()].copy_from_slice(track_name);
        data
    })
}

// ============================================================================
// AMS2 Telemetry Data Generator
// ============================================================================

/// AMS2 shared memory data structure (simplified, must match ams2.rs)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct AMS2SharedMemory {
    version: u32,
    build_version_number: u32,
    game_state: u32,
    session_state: u32,
    race_state: u32,
    viewed_participant_index: i32,
    num_participants: i32,
    num_active_participants: i32,
    laps_completed: u32,
    laps_in_event: u32,
    current_time: f32,
    split_time_ahead: f32,
    split_time_behind: f32,
    split_time: f32,
    best_lap_time: f32,
    last_lap_time: f32,
    current_sector1_time: f32,
    current_sector2_time: f32,
    current_sector3_time: f32,
    fastest_sector1_time: f32,
    fastest_sector2_time: f32,
    fastest_sector3_time: f32,
    personal_fastest_lap_time: f32,
    personal_fastest_sector1_time: f32,
    personal_fastest_sector2_time: f32,
    personal_fastest_sector3_time: f32,
    highest_flag: u32,
    pit_mode: u32,
    pit_schedule: u32,
    car_flags: u32,
    oil_temp_celsius: f32,
    oil_pressure_kpa: f32,
    water_temp_celsius: f32,
    water_pressure_kpa: f32,
    fuel_pressure_kpa: f32,
    fuel_level: f32,
    fuel_capacity: f32,
    speed: f32,
    rpm: f32,
    max_rpm: f32,
    gear: i8,
    num_gears: i8,
    boost_amount: f32,
    boost_pressure: f32,
    crash_state: u32,
    odometer_km: f32,
    orientation: [f32; 3],
    local_velocity: [f32; 3],
    world_velocity: [f32; 3],
    angular_velocity: [f32; 3],
    local_acceleration: [f32; 3],
    world_acceleration: [f32; 3],
    extents_centre: [f32; 3],
    tyre_flags: [u32; 4],
    tyre_terrain: [u32; 4],
    tyre_y: [f32; 4],
    tyre_rps: [f32; 4],
    tyre_slip_speed: [f32; 4],
    tyre_temp: [f32; 4],
    tyre_grip: [f32; 4],
    tyre_height_above_ground: [f32; 4],
    tyre_lateral_stiffness: [f32; 4],
    tyre_wear: [f32; 4],
    brake_damage: [f32; 4],
    suspension_damage: [f32; 4],
    brake_temp_celsius: [f32; 4],
    tyre_tread_temp: [f32; 4],
    tyre_layer_temp: [f32; 4],
    tyre_carcass_temp: [f32; 4],
    tyre_rim_temp: [f32; 4],
    tyre_internal_air_temp: [f32; 4],
    wheel_local_position_y: [f32; 4],
    ride_height: [f32; 4],
    suspension_travel: [f32; 4],
    suspension_velocity: [f32; 4],
    air_pressure: [f32; 4],
    tyre_slip: [f32; 4],
    throttle: f32,
    brake: f32,
    clutch: f32,
    steering: f32,
    tc_setting: u8,
    abs_setting: u8,
    drs_state: u32,
    ers_deployment_mode: u32,
    update_index: u32,
    car_name: [u8; 64],
    car_class_name: [u8; 64],
    track_location: [u8; 64],
    track_variation: [u8; 64],
    _padding: [u8; 128],
}

impl Default for AMS2SharedMemory {
    fn default() -> Self {
        // Use zeroed memory for safety
        unsafe { std::mem::zeroed() }
    }
}

/// Strategy for generating valid AMS2 telemetry data
fn ams2_data_strategy() -> impl Strategy<Value = AMS2SharedMemory> {
    // Group 1: Core dynamics (8 elements)
    let dynamics = (
        0.0f32..150.0,   // speed (m/s)
        0.0f32..18000.0, // rpm
        -1i8..8,         // gear
        0.0f32..1.0,     // throttle
        0.0f32..1.0,     // brake
        0.0f32..1.0,     // clutch
        -1.0f32..1.0,    // steering
        0.0f32..120.0,   // fuel_level
    );

    // Group 2: Additional data (8 elements)
    let additional = (
        0.0f32..150.0,                      // fuel_capacity
        0u32..100,                          // laps_completed
        0u32..12,                           // highest_flag
        0u32..7,                            // pit_mode
        0u8..12,                            // tc_setting
        0u8..12,                            // abs_setting
        prop::array::uniform4(0.0f32..1.0), // tyre_slip
        any::<u32>(),                       // update_index
    );

    (dynamics, additional).prop_map(|(d, a)| {
        let mut data = AMS2SharedMemory {
            speed: d.0,
            rpm: d.1,
            gear: d.2,
            throttle: d.3,
            brake: d.4,
            clutch: d.5,
            steering: d.6,
            fuel_level: d.7,
            fuel_capacity: a.0,
            laps_completed: a.1,
            highest_flag: a.2,
            pit_mode: a.3,
            tc_setting: a.4,
            abs_setting: a.5,
            tyre_slip: a.6,
            update_index: a.7,
            ..Default::default()
        };
        // Add car and track names
        let car_name = b"formula_ultimate\0";
        let track_name = b"interlagos\0";
        data.car_name[..car_name.len()].copy_from_slice(car_name);
        data.track_location[..track_name.len()].copy_from_slice(track_name);
        data
    })
}

// ============================================================================
// rFactor 2 Telemetry Data Generator
// ============================================================================

/// Maximum number of wheels per vehicle
const RF2_MAX_WHEELS: usize = 4;

/// rFactor 2 wheel telemetry data
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct RF2WheelTelemetry {
    suspension_deflection: f64,
    ride_height: f64,
    suspension_force: f64,
    brake_temp: f64,
    brake_pressure: f64,
    rotation: f64,
    lateral_patch_vel: f64,
    longitudinal_patch_vel: f64,
    lateral_ground_vel: f64,
    longitudinal_ground_vel: f64,
    camber: f64,
    lateral_force: f64,
    longitudinal_force: f64,
    tire_load: f64,
    grip_fract: f64,
    pressure: f64,
    temperature: [f64; 3],
    wear: f64,
    lateral_patch_slip: f32,
    longitudinal_patch_slip: f32,
}

/// rFactor 2 vehicle telemetry data
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct RF2VehicleTelemetry {
    id: i32,
    delta_time: f64,
    elapsed_time: f64,
    lap_number: i32,
    lap_start_et: f64,
    vehicle_name: [u8; 64],
    track_name: [u8; 64],
    pos: [f64; 3],
    local_vel: [f64; 3],
    local_accel: [f64; 3],
    ori: [f64; 3],
    local_rot: [f64; 3],
    local_rot_accel: [f64; 3],
    speed: f32,
    gear: i8,
    _pad1: [u8; 3],
    engine_rpm: f32,
    engine_water_temp: f32,
    engine_oil_temp: f32,
    clutch_rpm: f32,
    unfiltered_throttle: f32,
    unfiltered_brake: f32,
    unfiltered_steering: f32,
    unfiltered_clutch: f32,
    steering_shaft_torque: f32,
    fuel: f32,
    engine_max_rpm: f32,
    wheels: [RF2WheelTelemetry; RF2_MAX_WHEELS],
}

impl Default for RF2VehicleTelemetry {
    fn default() -> Self {
        Self {
            id: 0,
            delta_time: 0.0,
            elapsed_time: 0.0,
            lap_number: 0,
            lap_start_et: 0.0,
            vehicle_name: [0; 64],
            track_name: [0; 64],
            pos: [0.0; 3],
            local_vel: [0.0; 3],
            local_accel: [0.0; 3],
            ori: [0.0; 3],
            local_rot: [0.0; 3],
            local_rot_accel: [0.0; 3],
            speed: 0.0,
            gear: 0,
            _pad1: [0; 3],
            engine_rpm: 0.0,
            engine_water_temp: 0.0,
            engine_oil_temp: 0.0,
            clutch_rpm: 0.0,
            unfiltered_throttle: 0.0,
            unfiltered_brake: 0.0,
            unfiltered_steering: 0.0,
            unfiltered_clutch: 0.0,
            steering_shaft_torque: 0.0,
            fuel: 0.0,
            engine_max_rpm: 0.0,
            wheels: [RF2WheelTelemetry::default(); RF2_MAX_WHEELS],
        }
    }
}

/// Strategy for generating valid rFactor 2 telemetry data
fn rfactor2_data_strategy() -> impl Strategy<Value = RF2VehicleTelemetry> {
    // Group 1: Core dynamics (8 elements)
    let dynamics = (
        0.0f32..100.0,    // speed (m/s)
        0.0f32..18000.0,  // engine_rpm
        -1i8..8,          // gear
        0.0f32..1.0,      // unfiltered_throttle
        0.0f32..1.0,      // unfiltered_brake
        -1.0f32..1.0,     // unfiltered_steering
        0.0f32..1.0,      // unfiltered_clutch
        -100.0f32..100.0, // steering_shaft_torque (Nm)
    );

    // Group 2: Additional data (6 elements)
    let additional = (
        0.0f32..120.0,                       // fuel
        0.0f32..20000.0,                     // engine_max_rpm
        0.0f32..120.0,                       // engine_water_temp
        0.0f32..150.0,                       // engine_oil_temp
        0i32..100,                           // lap_number
        prop::array::uniform4(-1.0f32..1.0), // wheel lateral_patch_slip
    );

    (dynamics, additional).prop_map(|(d, a)| {
        let mut data = RF2VehicleTelemetry {
            speed: d.0,
            engine_rpm: d.1,
            gear: d.2,
            unfiltered_throttle: d.3,
            unfiltered_brake: d.4,
            unfiltered_steering: d.5,
            unfiltered_clutch: d.6,
            steering_shaft_torque: d.7,
            fuel: a.0,
            engine_max_rpm: a.1,
            engine_water_temp: a.2,
            engine_oil_temp: a.3,
            lap_number: a.4,
            ..Default::default()
        };
        // Set wheel slip values
        for (i, slip) in a.5.iter().enumerate() {
            data.wheels[i].lateral_patch_slip = *slip;
        }
        // Add vehicle and track names
        let vehicle_name = b"formula_renault\0";
        let track_name = b"spa_francorchamps\0";
        data.vehicle_name[..vehicle_name.len()].copy_from_slice(vehicle_name);
        data.track_name[..track_name.len()].copy_from_slice(track_name);
        data
    })
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert a struct to raw bytes for parsing
fn to_raw_bytes<T: Copy>(data: &T) -> Vec<u8> {
    let size = std::mem::size_of::<T>();
    let ptr = data as *const T as *const u8;
    // SAFETY: We're reading a valid struct as bytes
    unsafe { std::slice::from_raw_parts(ptr, size).to_vec() }
}

/// Measure parsing time and verify it's within the 1ms budget
fn verify_parsing_time<F>(parse_fn: F, adapter_name: &str) -> TestResult
where
    F: FnOnce() -> anyhow::Result<crate::telemetry::NormalizedTelemetry>,
{
    let start = Instant::now();
    let result = parse_fn();
    let elapsed_ns = start.elapsed().as_nanos();

    // First verify parsing succeeded
    if let Err(e) = result {
        return Err(TestCaseError::fail(format!(
            "{} parsing failed: {}",
            adapter_name, e
        )));
    }

    // Then verify timing requirement
    if elapsed_ns > MAX_PARSING_TIME_NS {
        return Err(TestCaseError::fail(format!(
            "{} parsing took {}ns, exceeds 1ms ({}ns) budget",
            adapter_name, elapsed_ns, MAX_PARSING_TIME_NS
        )));
    }

    Ok(())
}

// ============================================================================
// Property Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: release-roadmap-v1, Property 22: Telemetry Parsing Performance
    /// **Validates: Requirements 12.5**
    ///
    /// For any valid iRacing telemetry packet, parsing SHALL complete within 1ms.
    #[test]
    fn prop_iracing_parsing_performance(data in iracing_data_strategy()) {
        let adapter = IRacingAdapter::new();
        let raw_bytes = to_raw_bytes(&data);

        verify_parsing_time(
            || adapter.normalize(&raw_bytes),
            "iRacing"
        )?;
    }

    /// Feature: release-roadmap-v1, Property 22: Telemetry Parsing Performance
    /// **Validates: Requirements 12.5**
    ///
    /// For any valid ACC telemetry packet, parsing SHALL complete within 1ms.
    #[test]
    fn prop_acc_parsing_performance(data in acc_data_strategy()) {
        let adapter = ACCAdapter::new();
        let raw_bytes = to_raw_bytes(&data);

        verify_parsing_time(
            || adapter.normalize(&raw_bytes),
            "ACC"
        )?;
    }

    /// Feature: release-roadmap-v1, Property 22: Telemetry Parsing Performance
    /// **Validates: Requirements 12.5**
    ///
    /// For any valid AMS2 telemetry packet, parsing SHALL complete within 1ms.
    #[test]
    fn prop_ams2_parsing_performance(data in ams2_data_strategy()) {
        let adapter = AMS2Adapter::new();
        let raw_bytes = to_raw_bytes(&data);

        verify_parsing_time(
            || adapter.normalize(&raw_bytes),
            "AMS2"
        )?;
    }

    /// Feature: release-roadmap-v1, Property 22: Telemetry Parsing Performance
    /// **Validates: Requirements 12.5**
    ///
    /// For any valid rFactor 2 telemetry packet, parsing SHALL complete within 1ms.
    #[test]
    fn prop_rfactor2_parsing_performance(data in rfactor2_data_strategy()) {
        let adapter = RFactor2Adapter::new();
        let raw_bytes = to_raw_bytes(&data);

        verify_parsing_time(
            || adapter.normalize(&raw_bytes),
            "rFactor2"
        )?;
    }
}

// ============================================================================
// Additional Unit Tests for Edge Cases
// ============================================================================

#[cfg(test)]
mod unit_tests {
    use super::*;

    type UnitTestResult = Result<(), Box<dyn std::error::Error>>;

    /// Test that iRacing parsing with default data completes within 1ms
    #[test]
    fn test_iracing_default_parsing_time() -> UnitTestResult {
        let adapter = IRacingAdapter::new();
        let data = IRacingData::default();
        let raw_bytes = to_raw_bytes(&data);

        let start = Instant::now();
        let result = adapter.normalize(&raw_bytes);
        let elapsed_ns = start.elapsed().as_nanos();

        if result.is_err() {
            return Err(format!("iRacing parsing failed: {:?}", result.err()).into());
        }

        if elapsed_ns > MAX_PARSING_TIME_NS {
            return Err(
                format!("iRacing parsing took {}ns, exceeds 1ms budget", elapsed_ns).into(),
            );
        }

        Ok(())
    }

    /// Test that ACC parsing with default data completes within 1ms
    #[test]
    fn test_acc_default_parsing_time() -> UnitTestResult {
        let adapter = ACCAdapter::new();
        let data = ACCTelemetryData::default();
        let raw_bytes = to_raw_bytes(&data);

        let start = Instant::now();
        let result = adapter.normalize(&raw_bytes);
        let elapsed_ns = start.elapsed().as_nanos();

        if result.is_err() {
            return Err(format!("ACC parsing failed: {:?}", result.err()).into());
        }

        if elapsed_ns > MAX_PARSING_TIME_NS {
            return Err(format!("ACC parsing took {}ns, exceeds 1ms budget", elapsed_ns).into());
        }

        Ok(())
    }

    /// Test that AMS2 parsing with default data completes within 1ms
    #[test]
    fn test_ams2_default_parsing_time() -> UnitTestResult {
        let adapter = AMS2Adapter::new();
        let data = AMS2SharedMemory::default();
        let raw_bytes = to_raw_bytes(&data);

        let start = Instant::now();
        let result = adapter.normalize(&raw_bytes);
        let elapsed_ns = start.elapsed().as_nanos();

        if result.is_err() {
            return Err(format!("AMS2 parsing failed: {:?}", result.err()).into());
        }

        if elapsed_ns > MAX_PARSING_TIME_NS {
            return Err(format!("AMS2 parsing took {}ns, exceeds 1ms budget", elapsed_ns).into());
        }

        Ok(())
    }

    /// Test that rFactor 2 parsing with default data completes within 1ms
    #[test]
    fn test_rfactor2_default_parsing_time() -> UnitTestResult {
        let adapter = RFactor2Adapter::new();
        let data = RF2VehicleTelemetry::default();
        let raw_bytes = to_raw_bytes(&data);

        let start = Instant::now();
        let result = adapter.normalize(&raw_bytes);
        let elapsed_ns = start.elapsed().as_nanos();

        if result.is_err() {
            return Err(format!("rFactor2 parsing failed: {:?}", result.err()).into());
        }

        if elapsed_ns > MAX_PARSING_TIME_NS {
            return Err(
                format!("rFactor2 parsing took {}ns, exceeds 1ms budget", elapsed_ns).into(),
            );
        }

        Ok(())
    }

    /// Test parsing performance with extreme but valid values
    #[test]
    fn test_extreme_values_parsing_time() -> UnitTestResult {
        // Test iRacing with extreme values
        let iracing_adapter = IRacingAdapter::new();
        let mut iracing_data = IRacingData::default();
        iracing_data.rpm = 15000.0;
        iracing_data.speed = 100.0;
        iracing_data.steering_wheel_torque = 100.0;
        let iracing_bytes = to_raw_bytes(&iracing_data);

        let start = Instant::now();
        let _ = iracing_adapter.normalize(&iracing_bytes);
        let elapsed = start.elapsed().as_nanos();

        if elapsed > MAX_PARSING_TIME_NS {
            return Err(format!(
                "iRacing extreme values parsing took {}ns, exceeds 1ms budget",
                elapsed
            )
            .into());
        }

        // Test ACC with extreme values
        let acc_adapter = ACCAdapter::new();
        let mut acc_data = ACCTelemetryData::default();
        acc_data.rpm = 15000.0;
        acc_data.speed = 350.0;
        acc_data.steer_angle = 450.0;
        let acc_bytes = to_raw_bytes(&acc_data);

        let start = Instant::now();
        let _ = acc_adapter.normalize(&acc_bytes);
        let elapsed = start.elapsed().as_nanos();

        if elapsed > MAX_PARSING_TIME_NS {
            return Err(format!(
                "ACC extreme values parsing took {}ns, exceeds 1ms budget",
                elapsed
            )
            .into());
        }

        Ok(())
    }
}

// ============================================================================
// Property 23: Telemetry Disconnection Handling
// ============================================================================
//
// Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
// **Validates: Requirements 12.6**
//
// For any game disconnection event, the telemetry system SHALL transition to
// disconnected state without crashing and notify the FFB engine.

mod disconnection_property_tests {
    use proptest::prelude::*;
    use crate::telemetry::game_telemetry::{
        ConnectionState, ConnectionStateEvent, DisconnectionConfig, DisconnectionTracker,
    };

    /// Strategy for generating valid disconnection timeout values
    /// Timeouts should be positive and reasonable (1ms to 10 seconds)
    fn timeout_strategy() -> impl Strategy<Value = u64> {
        1u64..10_000
    }

/// Strategy for generating valid reconnection configuration
    fn disconnection_config_strategy() -> impl Strategy<Value = DisconnectionConfig> {
        (
            timeout_strategy(), // timeout_ms
            any::<bool>(),      // auto_reconnect
            0u32..10,           // max_reconnect_attempts (0 = unlimited)
            100u64..5000,       // reconnect_delay_ms
        )
            .prop_map(
                |(timeout_ms, auto_reconnect, max_reconnect_attempts, reconnect_delay_ms)| {
                    DisconnectionConfig {
                        timeout_ms,
                        auto_reconnect,
                        max_reconnect_attempts,
                        reconnect_delay_ms,
                    }
                },
            )
    }

    /// Strategy for generating game IDs
    fn game_id_strategy() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-z][a-z0-9_]{0,15}")
            .expect("valid regex")
            .prop_filter("non-empty game id", |s| !s.is_empty())
    }

    /// Strategy for generating connection states
    fn connection_state_strategy() -> impl Strategy<Value = ConnectionState> {
        prop_oneof![
            Just(ConnectionState::Disconnected),
            Just(ConnectionState::Connecting),
            Just(ConnectionState::Connected),
            Just(ConnectionState::Reconnecting),
            Just(ConnectionState::Error),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    /// **Validates: Requirements 12.6**
    ///
    /// For any timeout configuration, when telemetry stops arriving for longer
    /// than the timeout, the tracker SHALL transition to Disconnected state.
    #[test]
    fn prop_disconnection_timeout_triggers_state_change(
        game_id in game_id_strategy(),
        timeout_ms in 10u64..100, // Use small timeouts for testing
    ) {
        let config = DisconnectionConfig::with_timeout(timeout_ms);
        let mut tracker = DisconnectionTracker::new(game_id, config);

        // Record initial data to become connected
        tracker.record_data_received();
        prop_assert_eq!(
            tracker.state(),
            ConnectionState::Connected,
            "Should be connected after receiving data"
        );

        // Wait for timeout to elapse
        std::thread::sleep(std::time::Duration::from_millis(timeout_ms + 20));

        // Check disconnection - should transition to Disconnected
        let state = tracker.check_disconnection();
        prop_assert_eq!(
            state,
            ConnectionState::Disconnected,
            "Should transition to Disconnected after timeout"
        );
    }

    /// Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    /// **Validates: Requirements 12.6**
    ///
    /// For any valid configuration, the DisconnectionTracker SHALL be created
    /// without crashing and start in Disconnected state.
    #[test]
    fn prop_tracker_creation_never_crashes(
        game_id in game_id_strategy(),
        config in disconnection_config_strategy(),
    ) {
        // Creating a tracker should never crash
        let tracker = DisconnectionTracker::new(game_id.clone(), config);

        // Initial state should always be Disconnected
        prop_assert_eq!(
            tracker.state(),
            ConnectionState::Disconnected,
            "Initial state should be Disconnected"
        );

        // Reconnect attempts should be zero
        prop_assert_eq!(
            tracker.reconnect_attempts(),
            0,
            "Initial reconnect attempts should be 0"
        );
    }

    /// Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    /// **Validates: Requirements 12.6**
    ///
    /// For any sequence of record_data_received calls, the tracker SHALL
    /// transition to Connected state and remain there while data keeps arriving.
    #[test]
    fn prop_data_received_maintains_connected_state(
        game_id in game_id_strategy(),
        num_data_events in 1usize..50,
    ) {
        let config = DisconnectionConfig::with_timeout(1000); // 1 second timeout
        let mut tracker = DisconnectionTracker::new(game_id, config);

        for i in 0..num_data_events {
            tracker.record_data_received();

            prop_assert_eq!(
                tracker.state(),
                ConnectionState::Connected,
                "Should remain Connected after data event {} of {}",
                i + 1,
                num_data_events
            );

            // Check disconnection should not change state while data is arriving
            let state = tracker.check_disconnection();
            prop_assert_eq!(
                state,
                ConnectionState::Connected,
                "check_disconnection should not change state while data is fresh"
            );
        }
    }

    /// Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    /// **Validates: Requirements 12.6**
    ///
    /// For any state transition, the tracker SHALL emit a ConnectionStateEvent
    /// with correct previous and new states.
    #[test]
    fn prop_state_events_have_correct_states(
        game_id in game_id_strategy(),
        timeout_ms in 10u64..50,
    ) -> Result<(), TestCaseError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        rt.block_on(async {
            let config = DisconnectionConfig::with_timeout(timeout_ms);
            let mut tracker = DisconnectionTracker::new(game_id.clone(), config);
            let mut receiver = tracker.subscribe();

            // Trigger connection by receiving data
            tracker.record_data_received();

            // Should receive a connection event
            let event = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                receiver.recv()
            )
            .await
            .map_err(|_| TestCaseError::fail("Timeout waiting for connection event"))?
            .ok_or_else(|| TestCaseError::fail("Channel closed unexpectedly"))?;

            prop_assert_eq!(
                event.previous_state,
                ConnectionState::Disconnected,
                "Connection event should have Disconnected as previous state"
            );
            prop_assert_eq!(
                event.new_state,
                ConnectionState::Connected,
                "Connection event should have Connected as new state"
            );
            prop_assert_eq!(
                event.game_id,
                game_id,
                "Event should have correct game_id"
            );
            prop_assert!(
                event.is_connection(),
                "Event should be identified as a connection"
            );

            // Wait for timeout and trigger disconnection
            std::thread::sleep(std::time::Duration::from_millis(timeout_ms + 20));
            tracker.check_disconnection();

            // Should receive a disconnection event
            let event = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                receiver.recv()
            )
            .await
            .map_err(|_| TestCaseError::fail("Timeout waiting for disconnection event"))?
            .ok_or_else(|| TestCaseError::fail("Channel closed unexpectedly"))?;

            prop_assert_eq!(
                event.previous_state,
                ConnectionState::Connected,
                "Disconnection event should have Connected as previous state"
            );
            prop_assert_eq!(
                event.new_state,
                ConnectionState::Disconnected,
                "Disconnection event should have Disconnected as new state"
            );
            prop_assert!(
                event.is_disconnection(),
                "Event should be identified as a disconnection"
            );

            Ok(())
        })
    }

    /// Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    /// **Validates: Requirements 12.6**
    ///
    /// For any reconnection configuration, the should_reconnect method SHALL
    /// correctly respect the auto_reconnect and max_reconnect_attempts settings.
    #[test]
    fn prop_reconnection_respects_configuration(
        game_id in game_id_strategy(),
        auto_reconnect in any::<bool>(),
        max_attempts in 1u32..10,
    ) {
        let config = DisconnectionConfig {
            timeout_ms: 100,
            auto_reconnect,
            max_reconnect_attempts: max_attempts,
            reconnect_delay_ms: 100,
        };
        let mut tracker = DisconnectionTracker::new(game_id, config);

        if !auto_reconnect {
            // When auto_reconnect is false, should never reconnect
            prop_assert!(
                !tracker.should_reconnect(),
                "should_reconnect should be false when auto_reconnect is disabled"
            );
        } else {
            // When auto_reconnect is true, should reconnect until max attempts
            prop_assert!(
                tracker.should_reconnect(),
                "should_reconnect should be true initially when auto_reconnect is enabled"
            );

            // Exhaust reconnection attempts
            for _ in 0..max_attempts {
                tracker.mark_reconnecting();
            }
            tracker.set_state(ConnectionState::Disconnected, None);

            prop_assert!(
                !tracker.should_reconnect(),
                "should_reconnect should be false after exhausting max_reconnect_attempts"
            );

            // Reset should allow reconnection again
            tracker.reset_reconnect_attempts();
            prop_assert!(
                tracker.should_reconnect(),
                "should_reconnect should be true after reset_reconnect_attempts"
            );
        }
    }

    /// Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    /// **Validates: Requirements 12.6**
    ///
    /// For any error condition, marking the tracker as error SHALL transition
    /// to Error state without crashing.
    #[test]
    fn prop_error_state_transition_never_crashes(
        game_id in game_id_strategy(),
        error_reason in ".*",
    ) {
        let mut tracker = DisconnectionTracker::with_defaults(game_id);

        // Mark error should never crash
        tracker.mark_error(error_reason.clone());

        prop_assert_eq!(
            tracker.state(),
            ConnectionState::Error,
            "State should be Error after mark_error"
        );

        // Error state should be considered disconnected
        prop_assert!(
            tracker.state().is_disconnected(),
            "Error state should be considered disconnected"
        );
    }

    /// Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    /// **Validates: Requirements 12.6**
    ///
    /// For any ConnectionStateEvent, the is_disconnection and is_connection
    /// methods SHALL correctly identify the event type.
    #[test]
    fn prop_event_type_identification_is_correct(
        game_id in game_id_strategy(),
        previous_state in connection_state_strategy(),
        new_state in connection_state_strategy(),
    ) {
        let event = ConnectionStateEvent::new(
            game_id,
            previous_state,
            new_state,
            None,
        );

        // is_disconnection: previous was connected, new is disconnected
        let expected_disconnection = previous_state.is_connected() && new_state.is_disconnected();
        prop_assert_eq!(
            event.is_disconnection(),
            expected_disconnection,
            "is_disconnection should be {} for {:?} -> {:?}",
            expected_disconnection,
            previous_state,
            new_state
        );

        // is_connection: previous was not connected, new is connected
        let expected_connection = !previous_state.is_connected() && new_state.is_connected();
        prop_assert_eq!(
            event.is_connection(),
            expected_connection,
            "is_connection should be {} for {:?} -> {:?}",
            expected_connection,
            previous_state,
            new_state
        );
    }

    /// Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    /// **Validates: Requirements 12.6**
    ///
    /// For any sequence of state transitions, the tracker SHALL maintain
    /// consistent state and never crash.
    #[test]
    fn prop_state_transitions_are_consistent(
        game_id in game_id_strategy(),
        operations in prop::collection::vec(0u8..5, 1..30),
    ) {
        let mut tracker = DisconnectionTracker::with_defaults(game_id);

        for op in operations {
            match op {
                0 => {
                    // Record data received
                    tracker.record_data_received();
                    prop_assert_eq!(
                        tracker.state(),
                        ConnectionState::Connected,
                        "Should be Connected after record_data_received"
                    );
                }
                1 => {
                    // Mark connecting
                    tracker.mark_connecting();
                    prop_assert_eq!(
                        tracker.state(),
                        ConnectionState::Connecting,
                        "Should be Connecting after mark_connecting"
                    );
                }
                2 => {
                    // Mark reconnecting
                    let prev_attempts = tracker.reconnect_attempts();
                    tracker.mark_reconnecting();
                    prop_assert_eq!(
                        tracker.state(),
                        ConnectionState::Reconnecting,
                        "Should be Reconnecting after mark_reconnecting"
                    );
                    prop_assert_eq!(
                        tracker.reconnect_attempts(),
                        prev_attempts + 1,
                        "Reconnect attempts should increment"
                    );
                }
                3 => {
                    // Mark error
                    tracker.mark_error("test error".to_string());
                    prop_assert_eq!(
                        tracker.state(),
                        ConnectionState::Error,
                        "Should be Error after mark_error"
                    );
                }
                4 => {
                    // Set disconnected
                    tracker.set_state(ConnectionState::Disconnected, None);
                    prop_assert_eq!(
                        tracker.state(),
                        ConnectionState::Disconnected,
                        "Should be Disconnected after set_state"
                    );
                }
                _ => {}
            }
        }
    }

    /// Feature: release-roadmap-v1, Property 23: Telemetry Disconnection Handling
    /// **Validates: Requirements 12.6**
    ///
    /// For any DisconnectionConfig, the timeout and reconnect_delay methods
    /// SHALL return correct Duration values.
    #[test]
    fn prop_config_duration_methods_are_correct(
        config in disconnection_config_strategy(),
    ) {
        let expected_timeout = std::time::Duration::from_millis(config.timeout_ms);
        let expected_delay = std::time::Duration::from_millis(config.reconnect_delay_ms);

        prop_assert_eq!(
            config.timeout(),
            expected_timeout,
            "timeout() should return Duration from timeout_ms"
        );
        prop_assert_eq!(
            config.reconnect_delay(),
            expected_delay,
            "reconnect_delay() should return Duration from reconnect_delay_ms"
        );
    }
}

// ============================================================================
// Additional Unit Tests for Disconnection Handling Edge Cases
// ============================================================================

#[cfg(test)]
mod disconnection_unit_tests {
    use super::*;

    type UnitTestResult = Result<(), Box<dyn std::error::Error>>;

    /// Test that duplicate state transitions don't emit duplicate events
    #[tokio::test]
    async fn test_no_duplicate_events_on_same_state() -> UnitTestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        let mut receiver = tracker.subscribe();

        // First data received - should emit event
        tracker.record_data_received();

        let event = tokio::time::timeout(std::time::Duration::from_millis(100), receiver.recv())
            .await?
            .ok_or("Expected event")?;

        assert!(event.is_connection());

        // Second data received - should NOT emit event (already connected)
        tracker.record_data_received();

        // Try to receive - should timeout (no event)
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(50), receiver.recv()).await;

        assert!(result.is_err(), "Should not receive duplicate event");

        Ok(())
    }

    /// Test that time_since_last_data returns None before any data
    #[test]
    fn test_time_since_last_data_initially_none() -> UnitTestResult {
        let tracker = DisconnectionTracker::with_defaults("test_game");

        assert!(
            tracker.time_since_last_data().is_none(),
            "time_since_last_data should be None before any data received"
        );

        Ok(())
    }

    /// Test that time_since_last_data returns Some after data received
    #[test]
    fn test_time_since_last_data_after_data() -> UnitTestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");

        tracker.record_data_received();
        std::thread::sleep(std::time::Duration::from_millis(10));

        let elapsed = tracker.time_since_last_data();
        assert!(
            elapsed.is_some(),
            "time_since_last_data should be Some after data received"
        );
        assert!(
            elapsed.map(|d| d.as_millis() >= 10).unwrap_or(false),
            "Elapsed time should be at least 10ms"
        );

        Ok(())
    }

    /// Test that is_timed_out returns false before any data
    #[test]
    fn test_is_timed_out_false_before_data() -> UnitTestResult {
        let config = DisconnectionConfig::with_timeout(10);
        let tracker = DisconnectionTracker::new("test_game", config);

        // Wait longer than timeout
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Should not be timed out because no data was ever received
        assert!(
            !tracker.is_timed_out(),
            "is_timed_out should be false when no data has ever been received"
        );

        Ok(())
    }

    /// Test unlimited reconnection attempts (max_reconnect_attempts = 0)
    #[test]
    fn test_unlimited_reconnection_attempts() -> UnitTestResult {
        let config = DisconnectionConfig {
            timeout_ms: 100,
            auto_reconnect: true,
            max_reconnect_attempts: 0, // Unlimited
            reconnect_delay_ms: 100,
        };
        let mut tracker = DisconnectionTracker::new("test_game", config);

        // Should always allow reconnection
        for i in 0..100 {
            assert!(
                tracker.should_reconnect(),
                "should_reconnect should be true for attempt {}",
                i
            );
            tracker.mark_reconnecting();
            tracker.set_state(ConnectionState::Disconnected, None);
        }

        Ok(())
    }

    /// Test ConnectionState helper methods
    #[test]
    fn test_connection_state_helpers() -> UnitTestResult {
        // is_connected
        assert!(ConnectionState::Connected.is_connected());
        assert!(!ConnectionState::Disconnected.is_connected());
        assert!(!ConnectionState::Connecting.is_connected());
        assert!(!ConnectionState::Reconnecting.is_connected());
        assert!(!ConnectionState::Error.is_connected());

        // is_disconnected
        assert!(ConnectionState::Disconnected.is_disconnected());
        assert!(ConnectionState::Error.is_disconnected());
        assert!(!ConnectionState::Connected.is_disconnected());
        assert!(!ConnectionState::Connecting.is_disconnected());
        assert!(!ConnectionState::Reconnecting.is_disconnected());

        // is_transitioning
        assert!(ConnectionState::Connecting.is_transitioning());
        assert!(ConnectionState::Reconnecting.is_transitioning());
        assert!(!ConnectionState::Connected.is_transitioning());
        assert!(!ConnectionState::Disconnected.is_transitioning());
        assert!(!ConnectionState::Error.is_transitioning());

        Ok(())
    }

    /// Test that events have valid timestamps
    #[test]
    fn test_event_timestamps_are_valid() -> UnitTestResult {
        let event = ConnectionStateEvent::new(
            "test_game",
            ConnectionState::Disconnected,
            ConnectionState::Connected,
            Some("test".to_string()),
        );

        // Timestamp should be non-zero (after UNIX epoch)
        assert!(event.timestamp_ns > 0, "Event timestamp should be positive");

        // Timestamp should be reasonable (after year 2020)
        let year_2020_ns: u64 = 1577836800_000_000_000; // 2020-01-01 00:00:00 UTC
        assert!(
            event.timestamp_ns > year_2020_ns,
            "Event timestamp should be after year 2020"
        );

        Ok(())
    }
}
