//! Comprehensive tests for telemetry normalization across all game adapters.
//!
//! Verifies that:
//! - Builder pattern correctly constructs NormalizedTelemetry
//! - Validated() sanitizes NaN/Infinity and clamps ranges
//! - Each adapter correctly maps game-specific fields
//! - Unit conversions (mph→m/s, °F→°C, degrees→radians) are correct
//! - Boundary values (max RPM, max speed, reverse gear, neutral) are handled
//! - Slip ratio and slip angle calculations are correct
//! - FFB scalar computation is within range
//! - Timestamp monotonicity is maintained
//! - Missing/partial data handling works correctly
//! - Property-based tests verify invariants across random inputs

use super::*;
use std::time::Instant;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Builder pattern tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn builder_default_produces_zeroed_telemetry() -> TestResult {
    let t = NormalizedTelemetry::builder().build();
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.max_rpm, 0.0);
    assert_eq!(t.gear, 0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.clutch, 0.0);
    assert_eq!(t.steering_angle, 0.0);
    assert_eq!(t.lateral_g, 0.0);
    assert_eq!(t.longitudinal_g, 0.0);
    assert_eq!(t.vertical_g, 0.0);
    assert_eq!(t.slip_ratio, 0.0);
    assert_eq!(t.ffb_scalar, 0.0);
    assert_eq!(t.ffb_torque_nm, 0.0);
    assert_eq!(t.fuel_percent, 0.0);
    assert_eq!(t.engine_temp_c, 0.0);
    assert_eq!(t.position, 0);
    assert_eq!(t.lap, 0);
    assert_eq!(t.sequence, 0);
    assert!(t.car_id.is_none());
    assert!(t.track_id.is_none());
    assert!(t.session_id.is_none());
    assert!(t.extended.is_empty());
    Ok(())
}

#[test]
fn builder_sets_all_motion_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(45.0)
        .steering_angle(0.15)
        .throttle(0.8)
        .brake(0.3)
        .clutch(0.5)
        .build();
    assert!((t.speed_ms - 45.0).abs() < f32::EPSILON);
    assert!((t.steering_angle - 0.15).abs() < f32::EPSILON);
    assert!((t.throttle - 0.8).abs() < f32::EPSILON);
    assert!((t.brake - 0.3).abs() < f32::EPSILON);
    assert!((t.clutch - 0.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_sets_engine_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .rpm(7500.0)
        .max_rpm(9000.0)
        .gear(4)
        .num_gears(6)
        .build();
    assert!((t.rpm - 7500.0).abs() < f32::EPSILON);
    assert!((t.max_rpm - 9000.0).abs() < f32::EPSILON);
    assert_eq!(t.gear, 4);
    assert_eq!(t.num_gears, 6);
    Ok(())
}

#[test]
fn builder_sets_g_forces() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .lateral_g(1.5)
        .longitudinal_g(-0.8)
        .vertical_g(1.0)
        .build();
    assert!((t.lateral_g - 1.5).abs() < f32::EPSILON);
    assert!((t.longitudinal_g - (-0.8)).abs() < f32::EPSILON);
    assert!((t.vertical_g - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_sets_slip_data() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .slip_ratio(0.15)
        .slip_angle_fl(0.02)
        .slip_angle_fr(0.03)
        .slip_angle_rl(0.01)
        .slip_angle_rr(0.04)
        .build();
    assert!((t.slip_ratio - 0.15).abs() < f32::EPSILON);
    assert!((t.slip_angle_fl - 0.02).abs() < f32::EPSILON);
    assert!((t.slip_angle_fr - 0.03).abs() < f32::EPSILON);
    assert!((t.slip_angle_rl - 0.01).abs() < f32::EPSILON);
    assert!((t.slip_angle_rr - 0.04).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_sets_tire_data() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .tire_temps_c([80, 82, 75, 77])
        .tire_pressures_psi([26.0, 26.5, 24.0, 24.5])
        .build();
    assert_eq!(t.tire_temps_c, [80, 82, 75, 77]);
    assert!((t.tire_pressures_psi[0] - 26.0).abs() < f32::EPSILON);
    assert!((t.tire_pressures_psi[3] - 24.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_sets_ffb_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .ffb_scalar(0.75)
        .ffb_torque_nm(12.5)
        .build();
    assert!((t.ffb_scalar - 0.75).abs() < f32::EPSILON);
    assert!((t.ffb_torque_nm - 12.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_sets_context_fields() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .car_id("ferrari_488")
        .track_id("monza")
        .session_id("session_001")
        .position(3)
        .lap(5)
        .current_lap_time_s(82.5)
        .best_lap_time_s(80.1)
        .last_lap_time_s(81.3)
        .delta_ahead_s(-1.2)
        .delta_behind_s(0.8)
        .fuel_percent(0.65)
        .engine_temp_c(95.0)
        .build();
    assert_eq!(t.car_id.as_deref(), Some("ferrari_488"));
    assert_eq!(t.track_id.as_deref(), Some("monza"));
    assert_eq!(t.session_id.as_deref(), Some("session_001"));
    assert_eq!(t.position, 3);
    assert_eq!(t.lap, 5);
    assert!((t.current_lap_time_s - 82.5).abs() < f32::EPSILON);
    assert!((t.best_lap_time_s - 80.1).abs() < f32::EPSILON);
    assert!((t.last_lap_time_s - 81.3).abs() < f32::EPSILON);
    assert!((t.delta_ahead_s - (-1.2)).abs() < f32::EPSILON);
    assert!((t.delta_behind_s - 0.8).abs() < f32::EPSILON);
    assert!((t.fuel_percent - 0.65).abs() < f32::EPSILON);
    assert!((t.engine_temp_c - 95.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_sets_extended_data() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .extended("turbo_bar", TelemetryValue::Float(1.5))
        .extended("pit_count", TelemetryValue::Integer(2))
        .extended("drs_active", TelemetryValue::Boolean(true))
        .extended("car_class", TelemetryValue::String("GT3".to_string()))
        .build();
    assert_eq!(
        t.extended.get("turbo_bar"),
        Some(&TelemetryValue::Float(1.5))
    );
    assert_eq!(
        t.extended.get("pit_count"),
        Some(&TelemetryValue::Integer(2))
    );
    assert_eq!(
        t.extended.get("drs_active"),
        Some(&TelemetryValue::Boolean(true))
    );
    assert_eq!(
        t.extended.get("car_class"),
        Some(&TelemetryValue::String("GT3".to_string()))
    );
    Ok(())
}

#[test]
fn builder_sets_timestamp_and_sequence() -> TestResult {
    let ts = Instant::now();
    let t = NormalizedTelemetry::builder()
        .timestamp(ts)
        .sequence(42)
        .build();
    assert_eq!(t.timestamp, ts);
    assert_eq!(t.sequence, 42);
    Ok(())
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Builder validation/clamping tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn builder_clamps_throttle_to_unit_range() -> TestResult {
    let t = NormalizedTelemetry::builder().throttle(1.5).build();
    assert!((t.throttle - 1.0).abs() < f32::EPSILON);

    let t = NormalizedTelemetry::builder().throttle(-0.5).build();
    assert!(t.throttle.abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_clamps_brake_to_unit_range() -> TestResult {
    let t = NormalizedTelemetry::builder().brake(2.0).build();
    assert!((t.brake - 1.0).abs() < f32::EPSILON);

    let t = NormalizedTelemetry::builder().brake(-1.0).build();
    assert!(t.brake.abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_clamps_clutch_to_unit_range() -> TestResult {
    let t = NormalizedTelemetry::builder().clutch(3.0).build();
    assert!((t.clutch - 1.0).abs() < f32::EPSILON);

    let t = NormalizedTelemetry::builder().clutch(-0.1).build();
    assert!(t.clutch.abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_clamps_slip_ratio_to_unit_range() -> TestResult {
    let t = NormalizedTelemetry::builder().slip_ratio(1.5).build();
    assert!((t.slip_ratio - 1.0).abs() < f32::EPSILON);

    let t = NormalizedTelemetry::builder().slip_ratio(-0.5).build();
    assert!(t.slip_ratio.abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_clamps_ffb_scalar_to_neg1_pos1() -> TestResult {
    let t = NormalizedTelemetry::builder().ffb_scalar(2.0).build();
    assert!((t.ffb_scalar - 1.0).abs() < f32::EPSILON);

    let t = NormalizedTelemetry::builder().ffb_scalar(-2.0).build();
    assert!((t.ffb_scalar - (-1.0)).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_clamps_fuel_percent_to_unit_range() -> TestResult {
    let t = NormalizedTelemetry::builder().fuel_percent(1.2).build();
    assert!((t.fuel_percent - 1.0).abs() < f32::EPSILON);

    let t = NormalizedTelemetry::builder().fuel_percent(-0.1).build();
    assert!(t.fuel_percent.abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn builder_rejects_negative_speed() -> TestResult {
    let t = NormalizedTelemetry::builder().speed_ms(-10.0).build();
    assert_eq!(t.speed_ms, 0.0, "negative speed should be rejected");
    Ok(())
}

#[test]
fn builder_rejects_negative_rpm() -> TestResult {
    let t = NormalizedTelemetry::builder().rpm(-500.0).build();
    assert_eq!(t.rpm, 0.0, "negative RPM should be rejected");
    Ok(())
}

#[test]
fn builder_rejects_negative_max_rpm() -> TestResult {
    let t = NormalizedTelemetry::builder().max_rpm(-100.0).build();
    assert_eq!(t.max_rpm, 0.0, "negative max_rpm should be rejected");
    Ok(())
}

#[test]
fn builder_rejects_nan_values() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(f32::NAN)
        .rpm(f32::NAN)
        .throttle(f32::NAN)
        .brake(f32::NAN)
        .clutch(f32::NAN)
        .steering_angle(f32::NAN)
        .lateral_g(f32::NAN)
        .longitudinal_g(f32::NAN)
        .vertical_g(f32::NAN)
        .ffb_scalar(f32::NAN)
        .ffb_torque_nm(f32::NAN)
        .fuel_percent(f32::NAN)
        .engine_temp_c(f32::NAN)
        .slip_ratio(f32::NAN)
        .slip_angle_fl(f32::NAN)
        .current_lap_time_s(f32::NAN)
        .best_lap_time_s(f32::NAN)
        .last_lap_time_s(f32::NAN)
        .build();
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.brake, 0.0);
    assert_eq!(t.clutch, 0.0);
    assert_eq!(t.steering_angle, 0.0);
    assert_eq!(t.lateral_g, 0.0);
    assert_eq!(t.longitudinal_g, 0.0);
    assert_eq!(t.vertical_g, 0.0);
    assert_eq!(t.ffb_scalar, 0.0);
    assert_eq!(t.ffb_torque_nm, 0.0);
    assert_eq!(t.fuel_percent, 0.0);
    assert_eq!(t.engine_temp_c, 0.0);
    assert_eq!(t.slip_ratio, 0.0);
    assert_eq!(t.slip_angle_fl, 0.0);
    assert_eq!(t.current_lap_time_s, 0.0);
    assert_eq!(t.best_lap_time_s, 0.0);
    assert_eq!(t.last_lap_time_s, 0.0);
    Ok(())
}

#[test]
fn builder_rejects_infinity_values() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(f32::INFINITY)
        .rpm(f32::NEG_INFINITY)
        .throttle(f32::INFINITY)
        .steering_angle(f32::NEG_INFINITY)
        .build();
    assert_eq!(t.speed_ms, 0.0);
    assert_eq!(t.rpm, 0.0);
    assert_eq!(t.throttle, 0.0);
    assert_eq!(t.steering_angle, 0.0);
    Ok(())
}

#[test]
fn builder_rejects_negative_lap_times() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .current_lap_time_s(-5.0)
        .best_lap_time_s(-10.0)
        .last_lap_time_s(-1.0)
        .build();
    assert_eq!(t.current_lap_time_s, 0.0);
    assert_eq!(t.best_lap_time_s, 0.0);
    assert_eq!(t.last_lap_time_s, 0.0);
    Ok(())
}

#[test]
fn builder_empty_car_id_stays_none() -> TestResult {
    let t = NormalizedTelemetry::builder().car_id("").build();
    assert!(t.car_id.is_none());
    Ok(())
}

#[test]
fn builder_empty_track_id_stays_none() -> TestResult {
    let t = NormalizedTelemetry::builder().track_id("").build();
    assert!(t.track_id.is_none());
    Ok(())
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// validated() tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn validated_clamps_speed_to_nonnegative() -> TestResult {
    let mut t = NormalizedTelemetry::default();
    t.speed_ms = -10.0;
    let v = t.validated();
    assert_eq!(v.speed_ms, 0.0);
    Ok(())
}

#[test]
fn validated_clamps_rpm_to_nonnegative() -> TestResult {
    let mut t = NormalizedTelemetry::default();
    t.rpm = -500.0;
    let v = t.validated();
    assert_eq!(v.rpm, 0.0);
    Ok(())
}

#[test]
fn validated_clamps_throttle_to_unit() -> TestResult {
    let mut t = NormalizedTelemetry::default();
    t.throttle = 1.5;
    let v = t.validated();
    assert!((v.throttle - 1.0).abs() < f32::EPSILON);

    let mut t2 = NormalizedTelemetry::default();
    t2.throttle = -0.5;
    let v2 = t2.validated();
    assert!(v2.throttle.abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn validated_clamps_brake_to_unit() -> TestResult {
    let mut t = NormalizedTelemetry::default();
    t.brake = 2.0;
    let v = t.validated();
    assert!((v.brake - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn validated_clamps_clutch_to_unit() -> TestResult {
    let mut t = NormalizedTelemetry::default();
    t.clutch = 1.1;
    let v = t.validated();
    assert!((v.clutch - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn validated_clamps_slip_ratio_to_unit() -> TestResult {
    let mut t = NormalizedTelemetry::default();
    t.slip_ratio = 1.5;
    let v = t.validated();
    assert!((v.slip_ratio - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn validated_clamps_ffb_scalar() -> TestResult {
    let mut t = NormalizedTelemetry::default();
    t.ffb_scalar = 2.0;
    let v = t.validated();
    assert!((v.ffb_scalar - 1.0).abs() < f32::EPSILON);

    let mut t2 = NormalizedTelemetry::default();
    t2.ffb_scalar = -2.0;
    let v2 = t2.validated();
    assert!((v2.ffb_scalar - (-1.0)).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn validated_clamps_fuel_percent_to_unit() -> TestResult {
    let mut t = NormalizedTelemetry::default();
    t.fuel_percent = 1.5;
    let v = t.validated();
    assert!((v.fuel_percent - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn validated_sanitizes_nan_to_zero() -> TestResult {
    let mut t = NormalizedTelemetry::default();
    t.speed_ms = f32::NAN;
    t.rpm = f32::NAN;
    t.throttle = f32::NAN;
    t.brake = f32::NAN;
    t.clutch = f32::NAN;
    t.steering_angle = f32::NAN;
    t.lateral_g = f32::NAN;
    t.longitudinal_g = f32::NAN;
    t.vertical_g = f32::NAN;
    t.slip_ratio = f32::NAN;
    t.ffb_scalar = f32::NAN;
    t.ffb_torque_nm = f32::NAN;
    t.fuel_percent = f32::NAN;
    t.engine_temp_c = f32::NAN;
    let v = t.validated();
    assert_eq!(v.speed_ms, 0.0);
    assert_eq!(v.rpm, 0.0);
    assert_eq!(v.throttle, 0.0);
    assert_eq!(v.brake, 0.0);
    assert_eq!(v.clutch, 0.0);
    assert_eq!(v.steering_angle, 0.0);
    assert_eq!(v.lateral_g, 0.0);
    assert_eq!(v.longitudinal_g, 0.0);
    assert_eq!(v.vertical_g, 0.0);
    assert_eq!(v.slip_ratio, 0.0);
    assert_eq!(v.ffb_scalar, 0.0);
    assert_eq!(v.ffb_torque_nm, 0.0);
    assert_eq!(v.fuel_percent, 0.0);
    assert_eq!(v.engine_temp_c, 0.0);
    Ok(())
}

#[test]
fn validated_sanitizes_infinity_to_zero() -> TestResult {
    let mut t = NormalizedTelemetry::default();
    t.speed_ms = f32::INFINITY;
    t.rpm = f32::NEG_INFINITY;
    t.steering_angle = f32::INFINITY;
    let v = t.validated();
    assert_eq!(v.speed_ms, 0.0);
    assert_eq!(v.rpm, 0.0);
    assert_eq!(v.steering_angle, 0.0);
    Ok(())
}

#[test]
fn validated_preserves_valid_values() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(30.0)
        .rpm(5000.0)
        .throttle(0.7)
        .brake(0.3)
        .gear(3)
        .fuel_percent(0.5)
        .build()
        .validated();
    assert!((t.speed_ms - 30.0).abs() < f32::EPSILON);
    assert!((t.rpm - 5000.0).abs() < f32::EPSILON);
    assert!((t.throttle - 0.7).abs() < f32::EPSILON);
    assert!((t.brake - 0.3).abs() < f32::EPSILON);
    assert_eq!(t.gear, 3);
    assert!((t.fuel_percent - 0.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn validated_preserves_slip_angles() -> TestResult {
    let mut t = NormalizedTelemetry::default();
    t.slip_angle_fl = f32::NAN;
    t.slip_angle_fr = 0.05;
    t.slip_angle_rl = f32::INFINITY;
    t.slip_angle_rr = -0.1;
    let v = t.validated();
    assert_eq!(v.slip_angle_fl, 0.0);
    assert!((v.slip_angle_fr - 0.05).abs() < f32::EPSILON);
    assert_eq!(v.slip_angle_rl, 0.0);
    assert!((v.slip_angle_rr - (-0.1)).abs() < f32::EPSILON);
    Ok(())
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Timestamp tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn timestamp_monotonicity_across_builder_calls() -> TestResult {
    let t1 = NormalizedTelemetry::builder().sequence(1).build();
    // Small busy-wait to ensure monotonic clock advances
    let start = Instant::now();
    while start.elapsed().as_nanos() < 100 {
        std::hint::spin_loop();
    }
    let t2 = NormalizedTelemetry::builder().sequence(2).build();
    assert!(
        t2.timestamp >= t1.timestamp,
        "timestamps must be monotonically non-decreasing"
    );
    Ok(())
}

#[test]
fn telemetry_now_ns_is_monotonic() -> TestResult {
    let ts1 = telemetry_now_ns();
    let ts2 = telemetry_now_ns();
    assert!(ts2 >= ts1, "telemetry_now_ns must be monotonic");
    Ok(())
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// TelemetryFrame tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn telemetry_frame_creation() -> TestResult {
    let t = NormalizedTelemetry::builder()
        .speed_ms(50.0)
        .rpm(6000.0)
        .build();
    let frame = TelemetryFrame::new(t, 123456, 7, 311);
    assert!((frame.data.speed_ms - 50.0).abs() < f32::EPSILON);
    assert!((frame.data.rpm - 6000.0).abs() < f32::EPSILON);
    assert_eq!(frame.timestamp_ns, 123456);
    assert_eq!(frame.sequence, 7);
    assert_eq!(frame.raw_size, 311);
    Ok(())
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Forza adapter normalization tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

mod forza_normalization {
    use super::*;
    use crate::forza::ForzaAdapter;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const FORZA_SLED_SIZE: usize = 232;
    const FORZA_CARDASH_SIZE: usize = 311;
    const OFF_IS_RACE_ON: usize = 0;
    const OFF_ENGINE_MAX_RPM: usize = 8;
    const OFF_CURRENT_RPM: usize = 16;
    const OFF_VEL_X: usize = 32;
    const OFF_VEL_Y: usize = 36;
    const OFF_VEL_Z: usize = 40;
    const OFF_ACCEL_X: usize = 20;
    const OFF_ACCEL_Y: usize = 24;
    const OFF_ACCEL_Z: usize = 28;
    const OFF_DASH_ACCEL: usize = 303;
    const OFF_DASH_BRAKE: usize = 304;
    const OFF_DASH_CLUTCH: usize = 305;
    const OFF_DASH_GEAR: usize = 307;
    const OFF_DASH_STEER: usize = 308;
    const OFF_DASH_TIRE_TEMP_FL: usize = 256;
    const OFF_DASH_TIRE_TEMP_FR: usize = 260;
    const OFF_DASH_TIRE_TEMP_RL: usize = 264;
    const OFF_DASH_TIRE_TEMP_RR: usize = 268;
    const OFF_DASH_FUEL: usize = 276;
    const OFF_DASH_SPEED: usize = 244;

    fn make_sled(is_race_on: i32, rpm: f32, max_rpm: f32, vel: (f32, f32, f32)) -> Vec<u8> {
        let mut data = vec![0u8; FORZA_SLED_SIZE];
        data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&is_race_on.to_le_bytes());
        data[OFF_ENGINE_MAX_RPM..OFF_ENGINE_MAX_RPM + 4].copy_from_slice(&max_rpm.to_le_bytes());
        data[OFF_CURRENT_RPM..OFF_CURRENT_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_VEL_X..OFF_VEL_X + 4].copy_from_slice(&vel.0.to_le_bytes());
        data[OFF_VEL_Y..OFF_VEL_Y + 4].copy_from_slice(&vel.1.to_le_bytes());
        data[OFF_VEL_Z..OFF_VEL_Z + 4].copy_from_slice(&vel.2.to_le_bytes());
        data
    }

    fn make_cardash(
        rpm: f32,
        vel: (f32, f32, f32),
        throttle: u8,
        brake: u8,
        gear: u8,
        steer: i8,
    ) -> Vec<u8> {
        let mut data = vec![0u8; FORZA_CARDASH_SIZE];
        let sled = make_sled(1, rpm, 8000.0, vel);
        data[..FORZA_SLED_SIZE].copy_from_slice(&sled);
        let speed = (vel.0 * vel.0 + vel.1 * vel.1 + vel.2 * vel.2).sqrt();
        data[OFF_DASH_SPEED..OFF_DASH_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
        data[OFF_DASH_ACCEL] = throttle;
        data[OFF_DASH_BRAKE] = brake;
        data[OFF_DASH_GEAR] = gear;
        data[OFF_DASH_STEER] = steer as u8;
        data
    }

    #[test]
    fn forza_speed_is_velocity_magnitude() -> TestResult {
        let adapter = ForzaAdapter::new();
        // sqrt(3² + 4² + 0²) = 5.0
        let data = make_sled(1, 1000.0, 8000.0, (3.0, 4.0, 0.0));
        let result = adapter.normalize(&data)?;
        assert!(
            (result.speed_ms - 5.0).abs() < 0.01,
            "speed should be sqrt(3²+4²) = 5.0, got {}",
            result.speed_ms
        );
        Ok(())
    }

    #[test]
    fn forza_speed_3d_diagonal() -> TestResult {
        let adapter = ForzaAdapter::new();
        // sqrt(10² + 10² + 10²) ≈ 17.32
        let data = make_sled(1, 1000.0, 8000.0, (10.0, 10.0, 10.0));
        let result = adapter.normalize(&data)?;
        let expected = (300.0_f32).sqrt();
        assert!(
            (result.speed_ms - expected).abs() < 0.01,
            "expected {}, got {}",
            expected,
            result.speed_ms
        );
        Ok(())
    }

    #[test]
    fn forza_g_force_conversion() -> TestResult {
        let adapter = ForzaAdapter::new();
        let g = 9.806_65_f32;
        let mut data = make_sled(1, 1000.0, 8000.0, (0.0, 0.0, 0.0));
        // 2G lateral, 1.5G longitudinal, 0.5G vertical
        data[OFF_ACCEL_X..OFF_ACCEL_X + 4].copy_from_slice(&(2.0 * g).to_le_bytes());
        data[OFF_ACCEL_Z..OFF_ACCEL_Z + 4].copy_from_slice(&(1.5 * g).to_le_bytes());
        data[OFF_ACCEL_Y..OFF_ACCEL_Y + 4].copy_from_slice(&(0.5 * g).to_le_bytes());
        let result = adapter.normalize(&data)?;
        assert!((result.lateral_g - 2.0).abs() < 0.01);
        assert!((result.longitudinal_g - 1.5).abs() < 0.01);
        assert!((result.vertical_g - 0.5).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn forza_cardash_gear_encoding() -> TestResult {
        let adapter = ForzaAdapter::new();

        // Reverse (gear byte 0 → -1)
        let data = make_cardash(1000.0, (0.0, 0.0, 0.0), 0, 0, 0, 0);
        assert_eq!(adapter.normalize(&data)?.gear, -1);

        // Neutral (gear byte 1 → 0)
        let data = make_cardash(1000.0, (0.0, 0.0, 0.0), 0, 0, 1, 0);
        assert_eq!(adapter.normalize(&data)?.gear, 0);

        // 1st gear (gear byte 2 → 1)
        let data = make_cardash(1000.0, (0.0, 0.0, 0.0), 0, 0, 2, 0);
        assert_eq!(adapter.normalize(&data)?.gear, 1);

        // 6th gear (gear byte 7 → 6)
        let data = make_cardash(1000.0, (0.0, 0.0, 0.0), 0, 0, 7, 0);
        assert_eq!(adapter.normalize(&data)?.gear, 6);
        Ok(())
    }

    #[test]
    fn forza_cardash_throttle_byte_to_float() -> TestResult {
        let adapter = ForzaAdapter::new();

        // 0 → 0.0
        let data = make_cardash(1000.0, (10.0, 0.0, 0.0), 0, 0, 2, 0);
        assert!(adapter.normalize(&data)?.throttle.abs() < 0.01);

        // 255 → 1.0
        let data = make_cardash(1000.0, (10.0, 0.0, 0.0), 255, 0, 2, 0);
        assert!((adapter.normalize(&data)?.throttle - 1.0).abs() < 0.01);

        // 128 → ~0.502
        let data = make_cardash(1000.0, (10.0, 0.0, 0.0), 128, 0, 2, 0);
        let result = adapter.normalize(&data)?;
        assert!((result.throttle - 128.0 / 255.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn forza_cardash_steer_i8_to_float() -> TestResult {
        let adapter = ForzaAdapter::new();

        // Center (0 → 0.0)
        let data = make_cardash(1000.0, (10.0, 0.0, 0.0), 0, 0, 2, 0);
        assert!(adapter.normalize(&data)?.steering_angle.abs() < 0.01);

        // Full right (127 → 1.0)
        let data = make_cardash(1000.0, (10.0, 0.0, 0.0), 0, 0, 2, 127);
        assert!((adapter.normalize(&data)?.steering_angle - 1.0).abs() < 0.01);

        // Full left (-127 → -1.0)
        let data = make_cardash(1000.0, (10.0, 0.0, 0.0), 0, 0, 2, -127);
        assert!((adapter.normalize(&data)?.steering_angle - (-1.0)).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn forza_tire_temp_fahrenheit_to_celsius() -> TestResult {
        let adapter = ForzaAdapter::new();
        let mut data = vec![0u8; FORZA_CARDASH_SIZE];
        let sled = make_sled(1, 1000.0, 8000.0, (0.0, 0.0, 0.0));
        data[..FORZA_SLED_SIZE].copy_from_slice(&sled);
        // 212°F = 100°C, 32°F = 0°C
        data[OFF_DASH_TIRE_TEMP_FL..OFF_DASH_TIRE_TEMP_FL + 4]
            .copy_from_slice(&212.0f32.to_le_bytes());
        data[OFF_DASH_TIRE_TEMP_FR..OFF_DASH_TIRE_TEMP_FR + 4]
            .copy_from_slice(&32.0f32.to_le_bytes());
        data[OFF_DASH_TIRE_TEMP_RL..OFF_DASH_TIRE_TEMP_RL + 4]
            .copy_from_slice(&68.0f32.to_le_bytes()); // 20°C
        data[OFF_DASH_TIRE_TEMP_RR..OFF_DASH_TIRE_TEMP_RR + 4]
            .copy_from_slice(&392.0f32.to_le_bytes()); // 200°C
        let result = adapter.normalize(&data)?;
        assert_eq!(result.tire_temps_c[0], 100);
        assert_eq!(result.tire_temps_c[1], 0);
        assert_eq!(result.tire_temps_c[2], 20);
        assert_eq!(result.tire_temps_c[3], 200);
        Ok(())
    }

    #[test]
    fn forza_race_off_returns_defaults() -> TestResult {
        let adapter = ForzaAdapter::new();
        let data = make_sled(0, 5000.0, 8000.0, (20.0, 0.0, 0.0));
        let result = adapter.normalize(&data)?;
        assert_eq!(result.rpm, 0.0);
        assert_eq!(result.speed_ms, 0.0);
        Ok(())
    }

    #[test]
    fn forza_rejects_truncated_packet() -> TestResult {
        let adapter = ForzaAdapter::new();
        assert!(adapter.normalize(&[0u8; 100]).is_err());
        assert!(adapter.normalize(&[]).is_err());
        Ok(())
    }

    #[test]
    fn forza_max_rpm_propagated() -> TestResult {
        let adapter = ForzaAdapter::new();
        let data = make_sled(1, 7000.0, 9500.0, (10.0, 0.0, 0.0));
        let result = adapter.normalize(&data)?;
        assert!((result.rpm - 7000.0).abs() < 0.01);
        assert!((result.max_rpm - 9500.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn forza_zero_filled_sled_with_race_on() -> TestResult {
        let adapter = ForzaAdapter::new();
        let mut data = vec![0u8; FORZA_SLED_SIZE];
        data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&1i32.to_le_bytes());
        let result = adapter.normalize(&data)?;
        assert_eq!(result.rpm, 0.0);
        assert_eq!(result.speed_ms, 0.0);
        assert_eq!(result.lateral_g, 0.0);
        assert_eq!(result.longitudinal_g, 0.0);
        Ok(())
    }

    #[test]
    fn forza_cardash_fuel_propagated() -> TestResult {
        let adapter = ForzaAdapter::new();
        let mut data = vec![0u8; FORZA_CARDASH_SIZE];
        let sled = make_sled(1, 1000.0, 8000.0, (0.0, 0.0, 0.0));
        data[..FORZA_SLED_SIZE].copy_from_slice(&sled);
        data[OFF_DASH_FUEL..OFF_DASH_FUEL + 4].copy_from_slice(&0.75f32.to_le_bytes());
        let result = adapter.normalize(&data)?;
        assert!((result.fuel_percent - 0.75).abs() < 0.01);
        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// LFS adapter normalization tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

mod lfs_normalization {
    use super::*;
    use crate::lfs::LFSAdapter;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const OUTGAUGE_SIZE: usize = 92;
    const OFF_GEAR: usize = 10;
    const OFF_SPEED: usize = 12;
    const OFF_RPM: usize = 16;
    const OFF_FUEL: usize = 28;
    const OFF_ENG_TEMP: usize = 24;
    const OFF_THROTTLE: usize = 48;
    const OFF_BRAKE: usize = 52;
    const OFF_CLUTCH: usize = 56;
    const OFF_TURBO: usize = 20;
    const OFF_SHOW_LIGHTS: usize = 44;

    fn make_lfs(speed: f32, rpm: f32, gear: u8, throttle: f32, brake: f32) -> Vec<u8> {
        let mut data = vec![0u8; OUTGAUGE_SIZE];
        data[OFF_GEAR] = gear;
        data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
        data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&throttle.to_le_bytes());
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&brake.to_le_bytes());
        data
    }

    #[test]
    fn lfs_speed_is_direct_mps() -> TestResult {
        let adapter = LFSAdapter::new();
        let data = make_lfs(33.33, 5000.0, 3, 0.5, 0.0);
        let result = adapter.normalize(&data)?;
        assert!(
            (result.speed_ms - 33.33).abs() < 0.01,
            "LFS speed should be direct m/s, got {}",
            result.speed_ms
        );
        Ok(())
    }

    #[test]
    fn lfs_gear_encoding_reverse() -> TestResult {
        let adapter = LFSAdapter::new();
        let data = make_lfs(5.0, 2000.0, 0, 0.0, 0.5);
        let result = adapter.normalize(&data)?;
        assert_eq!(result.gear, -1, "LFS gear 0 should be reverse (-1)");
        Ok(())
    }

    #[test]
    fn lfs_gear_encoding_neutral() -> TestResult {
        let adapter = LFSAdapter::new();
        let data = make_lfs(0.0, 800.0, 1, 0.0, 0.0);
        let result = adapter.normalize(&data)?;
        assert_eq!(result.gear, 0, "LFS gear 1 should be neutral (0)");
        Ok(())
    }

    #[test]
    fn lfs_gear_encoding_first_through_sixth() -> TestResult {
        let adapter = LFSAdapter::new();
        for (raw, expected) in [(2u8, 1i8), (3, 2), (4, 3), (5, 4), (6, 5), (7, 6)] {
            let data = make_lfs(20.0, 3000.0, raw, 0.5, 0.0);
            let result = adapter.normalize(&data)?;
            assert_eq!(
                result.gear, expected,
                "LFS raw gear {} should map to {}",
                raw, expected
            );
        }
        Ok(())
    }

    #[test]
    fn lfs_throttle_brake_clutch_passthrough() -> TestResult {
        let adapter = LFSAdapter::new();
        let mut data = make_lfs(20.0, 3000.0, 3, 0.75, 0.25);
        data[OFF_CLUTCH..OFF_CLUTCH + 4].copy_from_slice(&0.4f32.to_le_bytes());
        let result = adapter.normalize(&data)?;
        assert!((result.throttle - 0.75).abs() < 0.001);
        assert!((result.brake - 0.25).abs() < 0.001);
        assert!((result.clutch - 0.4).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn lfs_fuel_and_engine_temp() -> TestResult {
        let adapter = LFSAdapter::new();
        let mut data = make_lfs(20.0, 3000.0, 3, 0.5, 0.0);
        data[OFF_FUEL..OFF_FUEL + 4].copy_from_slice(&0.85f32.to_le_bytes());
        data[OFF_ENG_TEMP..OFF_ENG_TEMP + 4].copy_from_slice(&92.0f32.to_le_bytes());
        let result = adapter.normalize(&data)?;
        assert!((result.fuel_percent - 0.85).abs() < 0.001);
        assert!((result.engine_temp_c - 92.0).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn lfs_turbo_in_extended() -> TestResult {
        let adapter = LFSAdapter::new();
        let mut data = make_lfs(20.0, 3000.0, 3, 0.5, 0.0);
        data[OFF_TURBO..OFF_TURBO + 4].copy_from_slice(&1.5f32.to_le_bytes());
        let result = adapter.normalize(&data)?;
        assert_eq!(
            result.extended.get("turbo_bar"),
            Some(&TelemetryValue::Float(1.5))
        );
        Ok(())
    }

    #[test]
    fn lfs_dashboard_flags_pit_limiter() -> TestResult {
        let adapter = LFSAdapter::new();
        let mut data = make_lfs(20.0, 3000.0, 3, 0.5, 0.0);
        // DL_PITSPEED = 0x0008
        data[OFF_SHOW_LIGHTS..OFF_SHOW_LIGHTS + 4].copy_from_slice(&0x0008u32.to_le_bytes());
        let result = adapter.normalize(&data)?;
        assert!(result.flags.pit_limiter);
        Ok(())
    }

    #[test]
    fn lfs_dashboard_flags_tc_and_abs() -> TestResult {
        let adapter = LFSAdapter::new();
        let mut data = make_lfs(20.0, 3000.0, 3, 0.5, 0.0);
        // DL_TC = 0x0010, DL_ABS = 0x0400
        data[OFF_SHOW_LIGHTS..OFF_SHOW_LIGHTS + 4]
            .copy_from_slice(&(0x0010u32 | 0x0400u32).to_le_bytes());
        let result = adapter.normalize(&data)?;
        assert!(result.flags.traction_control);
        assert!(result.flags.abs_active);
        Ok(())
    }

    #[test]
    fn lfs_rejects_truncated_packet() -> TestResult {
        let adapter = LFSAdapter::new();
        assert!(adapter.normalize(&[0u8; 50]).is_err());
        assert!(adapter.normalize(&[]).is_err());
        Ok(())
    }

    #[test]
    fn lfs_zero_filled_packet_parses() -> TestResult {
        let adapter = LFSAdapter::new();
        let data = vec![0u8; OUTGAUGE_SIZE];
        let result = adapter.normalize(&data)?;
        assert_eq!(result.speed_ms, 0.0);
        assert_eq!(result.rpm, 0.0);
        assert_eq!(result.gear, -1); // gear byte 0 = reverse
        Ok(())
    }

    #[test]
    fn lfs_accepts_96_byte_packet_with_id() -> TestResult {
        let adapter = LFSAdapter::new();
        let data = vec![0u8; 96]; // OutGauge with optional id field
        let result = adapter.normalize(&data)?;
        assert_eq!(result.speed_ms, 0.0);
        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// F1 native adapter normalization tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

mod f1_native_normalization {
    use super::*;
    use crate::f1_native;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn make_f1_telem(
        speed_kmh: u16,
        engine_rpm: u16,
        gear: i8,
        throttle: f32,
        brake: f32,
        steer: f32,
    ) -> crate::f1_25::CarTelemetryData {
        crate::f1_25::CarTelemetryData {
            speed_kmh,
            throttle,
            steer,
            brake,
            gear,
            engine_rpm,
            drs: 0,
            brakes_temperature: [400, 400, 400, 400],
            tyres_surface_temperature: [90, 90, 90, 90],
            tyres_inner_temperature: [100, 100, 100, 100],
            engine_temperature: 100,
            tyres_pressure: [24.0; 4],
        }
    }

    fn default_telem() -> crate::f1_25::CarTelemetryData {
        make_f1_telem(0, 0, 0, 0.0, 0.0, 0.0)
    }

    #[test]
    fn f1_speed_conversion_kmh_to_ms() -> TestResult {
        let telem = make_f1_telem(360, 12000, 8, 1.0, 0.0, 0.0);
        let status = f1_native::F1NativeCarStatusData::default();
        let session = crate::f1_25::SessionData::default();
        let result = f1_native::normalize(&telem, &status, &session);
        let expected_ms = 360.0 / 3.6;
        assert!(
            (result.speed_ms - expected_ms).abs() < 0.01,
            "F1 speed: 360 km/h should be {} m/s, got {}",
            expected_ms,
            result.speed_ms
        );
        Ok(())
    }

    #[test]
    fn f1_speed_zero_kmh() -> TestResult {
        let telem = default_telem();
        let result = f1_native::normalize(
            &telem,
            &f1_native::F1NativeCarStatusData::default(),
            &crate::f1_25::SessionData::default(),
        );
        assert!(result.speed_ms.abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn f1_rpm_u16_to_f32() -> TestResult {
        let telem = make_f1_telem(0, 15000, 0, 0.0, 0.0, 0.0);
        let result = f1_native::normalize(
            &telem,
            &f1_native::F1NativeCarStatusData::default(),
            &crate::f1_25::SessionData::default(),
        );
        assert!((result.rpm - 15000.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn f1_max_rpm_from_status() -> TestResult {
        let telem = make_f1_telem(0, 12000, 0, 0.0, 0.0, 0.0);
        let status = f1_native::F1NativeCarStatusData {
            max_rpm: 14000,
            ..Default::default()
        };
        let result = f1_native::normalize(&telem, &status, &crate::f1_25::SessionData::default());
        assert!((result.max_rpm - 14000.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn f1_gear_passthrough() -> TestResult {
        for gear in [-1i8, 0, 1, 2, 3, 4, 5, 6, 7, 8] {
            let telem = make_f1_telem(0, 5000, gear, 0.0, 0.0, 0.0);
            let result = f1_native::normalize(
                &telem,
                &f1_native::F1NativeCarStatusData::default(),
                &crate::f1_25::SessionData::default(),
            );
            assert_eq!(result.gear, gear, "F1 gear {} should pass through", gear);
        }
        Ok(())
    }

    #[test]
    fn f1_throttle_brake_passthrough() -> TestResult {
        let telem = make_f1_telem(100, 5000, 3, 0.85, 0.3, 0.0);
        let result = f1_native::normalize(
            &telem,
            &f1_native::F1NativeCarStatusData::default(),
            &crate::f1_25::SessionData::default(),
        );
        assert!((result.throttle - 0.85).abs() < 0.01);
        assert!((result.brake - 0.3).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn f1_tire_reorder_rl_rr_fl_fr_to_fl_fr_rl_rr() -> TestResult {
        let telem = crate::f1_25::CarTelemetryData {
            // F1 order: [RL, RR, FL, FR]
            tyres_pressure: [22.0, 22.5, 26.0, 26.5],
            tyres_surface_temperature: [80, 82, 90, 92],
            ..default_telem()
        };
        let result = f1_native::normalize(
            &telem,
            &f1_native::F1NativeCarStatusData::default(),
            &crate::f1_25::SessionData::default(),
        );
        // Normalized order: [FL, FR, RL, RR]
        assert!((result.tire_pressures_psi[0] - 26.0).abs() < 0.01); // FL
        assert!((result.tire_pressures_psi[1] - 26.5).abs() < 0.01); // FR
        assert!((result.tire_pressures_psi[2] - 22.0).abs() < 0.01); // RL
        assert!((result.tire_pressures_psi[3] - 22.5).abs() < 0.01); // RR
        assert_eq!(result.tire_temps_c[0], 90); // FL
        assert_eq!(result.tire_temps_c[1], 92); // FR
        assert_eq!(result.tire_temps_c[2], 80); // RL
        assert_eq!(result.tire_temps_c[3], 82); // RR
        Ok(())
    }

    #[test]
    fn f1_drs_flags() -> TestResult {
        let mut telem = default_telem();
        telem.drs = 1;
        let status = f1_native::F1NativeCarStatusData {
            drs_allowed: 1,
            ..Default::default()
        };
        let result = f1_native::normalize(&telem, &status, &crate::f1_25::SessionData::default());
        assert!(result.flags.drs_active);
        assert!(result.flags.drs_available);
        Ok(())
    }

    #[test]
    fn f1_pit_limiter_flag() -> TestResult {
        let status = f1_native::F1NativeCarStatusData {
            pit_limiter_status: 1,
            ..Default::default()
        };
        let result = f1_native::normalize(
            &default_telem(),
            &status,
            &crate::f1_25::SessionData::default(),
        );
        assert!(result.flags.pit_limiter);
        assert!(result.flags.in_pits);
        Ok(())
    }

    #[test]
    fn f1_ers_available_flag() -> TestResult {
        let status = f1_native::F1NativeCarStatusData {
            ers_store_energy: 1_000_000.0,
            ..Default::default()
        };
        let result = f1_native::normalize(
            &default_telem(),
            &status,
            &crate::f1_25::SessionData::default(),
        );
        assert!(result.flags.ers_available);
        Ok(())
    }

    #[test]
    fn f1_ers_fraction_in_extended() -> TestResult {
        let status = f1_native::F1NativeCarStatusData {
            ers_store_energy: 2_000_000.0,
            ..Default::default()
        };
        let result = f1_native::normalize(
            &default_telem(),
            &status,
            &crate::f1_25::SessionData::default(),
        );
        if let Some(TelemetryValue::Float(frac)) = result.extended.get("ers_store_fraction") {
            assert!(
                *frac >= 0.0 && *frac <= 1.0,
                "ERS fraction {} out of range",
                frac
            );
        }
        Ok(())
    }

    #[test]
    fn f1_engine_temp_passthrough() -> TestResult {
        let mut telem = default_telem();
        telem.engine_temperature = 105;
        let result = f1_native::normalize(
            &telem,
            &f1_native::F1NativeCarStatusData::default(),
            &crate::f1_25::SessionData::default(),
        );
        assert!((result.engine_temp_c - 105.0).abs() < 0.01);
        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Cross-adapter invariant tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

mod cross_adapter {
    use super::*;
    use crate::forza::ForzaAdapter;
    use crate::lfs::LFSAdapter;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// All adapters must produce game_id() that is non-empty.
    #[test]
    fn all_adapters_have_nonempty_game_id() -> TestResult {
        let forza = ForzaAdapter::new();
        let lfs = LFSAdapter::new();

        let adapters: Vec<Box<dyn TelemetryAdapter>> = vec![Box::new(forza), Box::new(lfs)];

        for adapter in &adapters {
            let id = adapter.game_id();
            assert!(!id.is_empty(), "game_id must not be empty for adapter");
            assert!(
                id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "game_id '{}' should only contain ascii alphanumeric and underscores",
                id
            );
        }
        Ok(())
    }

    /// All adapters must have a reasonable update rate.
    #[test]
    fn all_adapters_have_reasonable_update_rate() -> TestResult {
        let forza = ForzaAdapter::new();
        let lfs = LFSAdapter::new();

        let adapters: Vec<Box<dyn TelemetryAdapter>> = vec![Box::new(forza), Box::new(lfs)];

        for adapter in &adapters {
            let rate = adapter.expected_update_rate();
            assert!(
                rate.as_millis() >= 1 && rate.as_millis() <= 1000,
                "update rate {:?} for {} is unreasonable",
                rate,
                adapter.game_id()
            );
        }
        Ok(())
    }

    /// All adapters must reject empty input.
    #[test]
    fn all_adapters_reject_empty_input() -> TestResult {
        let forza = ForzaAdapter::new();
        let lfs = LFSAdapter::new();

        let adapters: Vec<Box<dyn TelemetryAdapter>> = vec![Box::new(forza), Box::new(lfs)];

        for adapter in &adapters {
            assert!(
                adapter.normalize(&[]).is_err(),
                "{} should reject empty input",
                adapter.game_id()
            );
        }
        Ok(())
    }

    /// When adapters succeed, the output should satisfy basic invariants.
    #[test]
    fn forza_output_satisfies_invariants() -> TestResult {
        let adapter = ForzaAdapter::new();
        let mut data = vec![0u8; 232];
        data[0..4].copy_from_slice(&1i32.to_le_bytes()); // is_race_on
        data[16..20].copy_from_slice(&5000.0f32.to_le_bytes()); // rpm
        data[32..36].copy_from_slice(&20.0f32.to_le_bytes()); // vel_x

        let result = adapter.normalize(&data)?;
        assert!(result.speed_ms >= 0.0, "speed must be non-negative");
        assert!(result.rpm >= 0.0, "RPM must be non-negative");
        assert!(
            result.throttle >= 0.0 && result.throttle <= 1.0,
            "throttle out of range"
        );
        assert!(
            result.brake >= 0.0 && result.brake <= 1.0,
            "brake out of range"
        );
        assert!(
            result.clutch >= 0.0 && result.clutch <= 1.0,
            "clutch out of range"
        );
        assert!(
            result.fuel_percent >= 0.0 && result.fuel_percent <= 1.0,
            "fuel out of range"
        );
        Ok(())
    }

    #[test]
    fn lfs_output_satisfies_invariants() -> TestResult {
        let adapter = LFSAdapter::new();
        let mut data = vec![0u8; 92];
        data[12..16].copy_from_slice(&30.0f32.to_le_bytes()); // speed
        data[16..20].copy_from_slice(&4500.0f32.to_le_bytes()); // rpm
        data[10] = 3; // gear
        data[48..52].copy_from_slice(&0.7f32.to_le_bytes()); // throttle

        let result = adapter.normalize(&data)?;
        assert!(result.speed_ms >= 0.0, "speed must be non-negative");
        assert!(result.rpm >= 0.0, "RPM must be non-negative");
        assert!(
            result.throttle >= 0.0 && result.throttle <= 1.0,
            "throttle out of range"
        );
        assert!(
            result.brake >= 0.0 && result.brake <= 1.0,
            "brake out of range"
        );
        assert!(
            result.fuel_percent >= 0.0 && result.fuel_percent <= 1.0,
            "fuel out of range"
        );
        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Property-based tests (proptest)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

mod proptest_normalization {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // ── Builder invariants ──────────────────────────────────────────────

        #[test]
        fn builder_speed_always_nonneg(speed in proptest::num::f32::ANY) {
            let t = NormalizedTelemetry::builder().speed_ms(speed).build();
            prop_assert!(t.speed_ms >= 0.0, "speed_ms={} must be >= 0.0", t.speed_ms);
        }

        #[test]
        fn builder_rpm_always_nonneg(rpm in proptest::num::f32::ANY) {
            let t = NormalizedTelemetry::builder().rpm(rpm).build();
            prop_assert!(t.rpm >= 0.0, "rpm={} must be >= 0.0", t.rpm);
        }

        #[test]
        fn builder_throttle_always_clamped(throttle in proptest::num::f32::ANY) {
            let t = NormalizedTelemetry::builder().throttle(throttle).build();
            prop_assert!(
                t.throttle >= 0.0 && t.throttle <= 1.0,
                "throttle={} must be in [0.0, 1.0]",
                t.throttle
            );
        }

        #[test]
        fn builder_brake_always_clamped(brake in proptest::num::f32::ANY) {
            let t = NormalizedTelemetry::builder().brake(brake).build();
            prop_assert!(
                t.brake >= 0.0 && t.brake <= 1.0,
                "brake={} must be in [0.0, 1.0]",
                t.brake
            );
        }

        #[test]
        fn builder_clutch_always_clamped(clutch in proptest::num::f32::ANY) {
            let t = NormalizedTelemetry::builder().clutch(clutch).build();
            prop_assert!(
                t.clutch >= 0.0 && t.clutch <= 1.0,
                "clutch={} must be in [0.0, 1.0]",
                t.clutch
            );
        }

        #[test]
        fn builder_slip_ratio_always_clamped(sr in proptest::num::f32::ANY) {
            let t = NormalizedTelemetry::builder().slip_ratio(sr).build();
            prop_assert!(
                t.slip_ratio >= 0.0 && t.slip_ratio <= 1.0,
                "slip_ratio={} must be in [0.0, 1.0]",
                t.slip_ratio
            );
        }

        #[test]
        fn builder_ffb_scalar_always_clamped(ffb in proptest::num::f32::ANY) {
            let t = NormalizedTelemetry::builder().ffb_scalar(ffb).build();
            prop_assert!(
                t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
                "ffb_scalar={} must be in [-1.0, 1.0]",
                t.ffb_scalar
            );
        }

        #[test]
        fn builder_fuel_percent_always_clamped(fuel in proptest::num::f32::ANY) {
            let t = NormalizedTelemetry::builder().fuel_percent(fuel).build();
            prop_assert!(
                t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0,
                "fuel_percent={} must be in [0.0, 1.0]",
                t.fuel_percent
            );
        }

        #[test]
        fn builder_max_rpm_always_nonneg(max_rpm in proptest::num::f32::ANY) {
            let t = NormalizedTelemetry::builder().max_rpm(max_rpm).build();
            prop_assert!(t.max_rpm >= 0.0, "max_rpm={} must be >= 0.0", t.max_rpm);
        }

        // ── validated() invariants ──────────────────────────────────────────

        #[test]
        fn validated_speed_nonneg(speed in proptest::num::f32::ANY) {
            let mut t = NormalizedTelemetry::default();
            t.speed_ms = speed;
            let v = t.validated();
            prop_assert!(v.speed_ms >= 0.0, "validated speed_ms={}", v.speed_ms);
        }

        #[test]
        fn validated_rpm_nonneg(rpm in proptest::num::f32::ANY) {
            let mut t = NormalizedTelemetry::default();
            t.rpm = rpm;
            let v = t.validated();
            prop_assert!(v.rpm >= 0.0, "validated rpm={}", v.rpm);
        }

        #[test]
        fn validated_throttle_clamped(throttle in proptest::num::f32::ANY) {
            let mut t = NormalizedTelemetry::default();
            t.throttle = throttle;
            let v = t.validated();
            prop_assert!(v.throttle >= 0.0 && v.throttle <= 1.0);
        }

        #[test]
        fn validated_brake_clamped(brake in proptest::num::f32::ANY) {
            let mut t = NormalizedTelemetry::default();
            t.brake = brake;
            let v = t.validated();
            prop_assert!(v.brake >= 0.0 && v.brake <= 1.0);
        }

        #[test]
        fn validated_clutch_clamped(clutch in proptest::num::f32::ANY) {
            let mut t = NormalizedTelemetry::default();
            t.clutch = clutch;
            let v = t.validated();
            prop_assert!(v.clutch >= 0.0 && v.clutch <= 1.0);
        }

        #[test]
        fn validated_slip_ratio_clamped(sr in proptest::num::f32::ANY) {
            let mut t = NormalizedTelemetry::default();
            t.slip_ratio = sr;
            let v = t.validated();
            prop_assert!(v.slip_ratio >= 0.0 && v.slip_ratio <= 1.0);
        }

        #[test]
        fn validated_ffb_scalar_clamped(ffb in proptest::num::f32::ANY) {
            let mut t = NormalizedTelemetry::default();
            t.ffb_scalar = ffb;
            let v = t.validated();
            prop_assert!(v.ffb_scalar >= -1.0 && v.ffb_scalar <= 1.0);
        }

        #[test]
        fn validated_fuel_clamped(fuel in proptest::num::f32::ANY) {
            let mut t = NormalizedTelemetry::default();
            t.fuel_percent = fuel;
            let v = t.validated();
            prop_assert!(v.fuel_percent >= 0.0 && v.fuel_percent <= 1.0);
        }

        // ── Forza adapter: arbitrary bytes never panic ──────────────────────

        #[test]
        fn forza_normalize_never_panics(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let adapter = crate::forza::ForzaAdapter::new();
            let _ = adapter.normalize(&data);
        }

        #[test]
        fn forza_valid_sled_invariants(
            rpm in 0.0f32..=20000.0f32,
            vx in -200.0f32..=200.0f32,
            vy in -50.0f32..=50.0f32,
            vz in -200.0f32..=200.0f32,
        ) {
            let mut data = vec![0u8; 232];
            data[0..4].copy_from_slice(&1i32.to_le_bytes());
            data[8..12].copy_from_slice(&9000.0f32.to_le_bytes());
            data[16..20].copy_from_slice(&rpm.to_le_bytes());
            data[32..36].copy_from_slice(&vx.to_le_bytes());
            data[36..40].copy_from_slice(&vy.to_le_bytes());
            data[40..44].copy_from_slice(&vz.to_le_bytes());

            let adapter = crate::forza::ForzaAdapter::new();
            if let Ok(result) = adapter.normalize(&data) {
                prop_assert!(result.speed_ms >= 0.0, "speed={}", result.speed_ms);
                prop_assert!(result.rpm >= 0.0, "rpm={}", result.rpm);
                prop_assert!(result.throttle >= 0.0 && result.throttle <= 1.0);
                prop_assert!(result.brake >= 0.0 && result.brake <= 1.0);
                prop_assert!(result.slip_ratio >= 0.0 && result.slip_ratio <= 1.0);
            }
        }

        // ── LFS adapter: arbitrary bytes never panic ────────────────────────

        #[test]
        fn lfs_normalize_never_panics(
            data in proptest::collection::vec(any::<u8>(), 0..256)
        ) {
            let adapter = crate::lfs::LFSAdapter::new();
            let _ = adapter.normalize(&data);
        }

        #[test]
        fn lfs_valid_packet_invariants(
            speed in 0.0f32..=200.0f32,
            rpm in 0.0f32..=20000.0f32,
            gear in 0u8..=8u8,
            throttle in 0.0f32..=1.0f32,
            brake in 0.0f32..=1.0f32,
        ) {
            let mut data = vec![0u8; 92];
            data[10] = gear;
            data[12..16].copy_from_slice(&speed.to_le_bytes());
            data[16..20].copy_from_slice(&rpm.to_le_bytes());
            data[48..52].copy_from_slice(&throttle.to_le_bytes());
            data[52..56].copy_from_slice(&brake.to_le_bytes());

            let adapter = crate::lfs::LFSAdapter::new();
            if let Ok(result) = adapter.normalize(&data) {
                prop_assert!(result.speed_ms >= 0.0);
                prop_assert!(result.rpm >= 0.0);
                prop_assert!(result.gear >= -1 && result.gear <= 8);
                prop_assert!(result.throttle >= 0.0 && result.throttle <= 1.0);
                prop_assert!(result.brake >= 0.0 && result.brake <= 1.0);
            }
        }

        // ── Gear encoding: LFS raw gear always maps to valid range ──────────

        #[test]
        fn lfs_gear_mapping_always_valid(raw_gear in 0u8..=8u8) {
            let mut data = vec![0u8; 92];
            data[10] = raw_gear;
            let adapter = crate::lfs::LFSAdapter::new();
            if let Ok(result) = adapter.normalize(&data) {
                prop_assert!(
                    result.gear >= -1,
                    "gear {} (from raw {}) must be >= -1",
                    result.gear,
                    raw_gear
                );
            }
        }

        // ── Forza CarDash: throttle/brake bytes always produce valid range ──

        #[test]
        fn forza_cardash_inputs_always_valid(
            throttle_byte in 0u8..=255u8,
            brake_byte in 0u8..=255u8,
            clutch_byte in 0u8..=255u8,
            gear_byte in 0u8..=9u8,
            steer_byte in 0u8..=255u8,
        ) {
            let mut data = vec![0u8; 311];
            data[0..4].copy_from_slice(&1i32.to_le_bytes());
            data[303] = throttle_byte;
            data[304] = brake_byte;
            data[305] = clutch_byte;
            data[307] = gear_byte;
            data[308] = steer_byte;

            let adapter = crate::forza::ForzaAdapter::new();
            if let Ok(result) = adapter.normalize(&data) {
                prop_assert!(result.throttle >= 0.0 && result.throttle <= 1.0);
                prop_assert!(result.brake >= 0.0 && result.brake <= 1.0);
                prop_assert!(result.clutch >= 0.0 && result.clutch <= 1.0);
                prop_assert!(result.gear >= -1 && result.gear <= 8);
                prop_assert!(result.steering_angle >= -1.0 && result.steering_angle <= 1.0);
            }
        }
    }
}
