//! Additional coverage tests for racing-wheel-telemetry-contracts.
//!
//! Targets edge cases in builder validation, flag combinations, conversions,
//! and serialization not already covered by unit tests or comprehensive.rs.

use racing_wheel_telemetry_contracts::{
    FlagCoverage, NormalizedTelemetry, TelemetryFieldCoverage, TelemetryFlags, TelemetryFrame,
    TelemetryValue,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// Builder chaining — overwrite semantics
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn with_ffb_scalar_last_write_wins() -> TestResult {
    let t = NormalizedTelemetry::new()
        .with_ffb_scalar(0.5)
        .with_ffb_scalar(-0.3);
    assert_eq!(t.ffb_scalar, Some(-0.3));
    Ok(())
}

#[test]
fn with_rpm_last_write_wins() -> TestResult {
    let t = NormalizedTelemetry::new().with_rpm(3000.0).with_rpm(7000.0);
    assert_eq!(t.rpm, Some(7000.0));
    Ok(())
}

#[test]
fn with_gear_overwrites_previous() -> TestResult {
    let t = NormalizedTelemetry::new().with_gear(1).with_gear(5);
    assert_eq!(t.gear, Some(5));
    Ok(())
}

#[test]
fn with_car_id_overwrites_previous() -> TestResult {
    let t = NormalizedTelemetry::new()
        .with_car_id("car_a".to_string())
        .with_car_id("car_b".to_string());
    assert_eq!(t.car_id.as_deref(), Some("car_b"));
    Ok(())
}

#[test]
fn with_track_id_overwrites_previous() -> TestResult {
    let t = NormalizedTelemetry::new()
        .with_track_id("track_a".to_string())
        .with_track_id("track_b".to_string());
    assert_eq!(t.track_id.as_deref(), Some("track_b"));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Flag combinations
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn multiple_hazard_flags_all_detected() -> TestResult {
    let flags = TelemetryFlags {
        yellow_flag: true,
        red_flag: true,
        blue_flag: true,
        checkered_flag: true,
        ..TelemetryFlags::default()
    };
    assert!(
        NormalizedTelemetry::new()
            .with_flags(flags)
            .has_active_flags()
    );
    Ok(())
}

#[test]
fn with_flags_replaces_previous_flags() -> TestResult {
    let flags1 = TelemetryFlags {
        yellow_flag: true,
        ..TelemetryFlags::default()
    };
    let flags2 = TelemetryFlags {
        red_flag: true,
        ..TelemetryFlags::default()
    };
    let t = NormalizedTelemetry::new()
        .with_flags(flags1)
        .with_flags(flags2);
    assert!(!t.flags.yellow_flag);
    assert!(t.flags.red_flag);
    Ok(())
}

#[test]
fn all_assist_flags_set_simultaneously() -> TestResult {
    let flags = TelemetryFlags {
        pit_limiter: true,
        in_pits: true,
        drs_available: true,
        drs_active: true,
        ers_available: true,
        launch_control: true,
        traction_control: true,
        abs_active: true,
        ..TelemetryFlags::default()
    };
    let t = NormalizedTelemetry::new().with_flags(flags);
    // Assist flags do not trigger has_active_flags
    assert!(!t.has_active_flags());
    assert!(t.flags.pit_limiter);
    assert!(t.flags.abs_active);
    assert!(t.flags.ers_available);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Conversion edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rpm_fraction_with_very_small_redline() -> TestResult {
    let t = NormalizedTelemetry::new().with_rpm(1000.0);
    let frac = t.rpm_fraction(0.001);
    // Should clamp to 1.0
    assert_eq!(frac, Some(1.0));
    Ok(())
}

#[test]
fn speed_conversions_at_high_speed() -> TestResult {
    let t = NormalizedTelemetry::new().with_speed_ms(100.0);
    let kmh = t.speed_kmh().unwrap_or(0.0);
    let mph = t.speed_mph().unwrap_or(0.0);
    assert!((kmh - 360.0).abs() < 0.1);
    assert!((mph - 223.7).abs() < 0.1);
    Ok(())
}

#[test]
fn rpm_fraction_zero_rpm_with_nonzero_redline() -> TestResult {
    let t = NormalizedTelemetry::new().with_rpm(0.0);
    let frac = t.rpm_fraction(8000.0);
    assert_eq!(frac, Some(0.0));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryFrame edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn telemetry_frame_zero_raw_size() -> TestResult {
    let frame = TelemetryFrame::new(NormalizedTelemetry::new(), 0, 0, 0);
    assert_eq!(frame.raw_size, 0);
    Ok(())
}

#[test]
fn telemetry_frame_max_timestamp() -> TestResult {
    let frame = TelemetryFrame::new(NormalizedTelemetry::new(), u64::MAX, u64::MAX, usize::MAX);
    assert_eq!(frame.timestamp_ns, u64::MAX);
    assert_eq!(frame.sequence, u64::MAX);
    assert_eq!(frame.raw_size, usize::MAX);
    Ok(())
}

#[test]
fn telemetry_frame_clone_equals() -> TestResult {
    let data = NormalizedTelemetry::new().with_rpm(5000.0).with_gear(3);
    let frame = TelemetryFrame::new(data, 42, 7, 128);
    let cloned = frame.clone();
    assert_eq!(frame.timestamp_ns, cloned.timestamp_ns);
    assert_eq!(frame.sequence, cloned.sequence);
    assert_eq!(frame.raw_size, cloned.raw_size);
    assert_eq!(frame.data, cloned.data);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryValue edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn telemetry_value_debug_all_variants() -> TestResult {
    let variants = [
        TelemetryValue::Float(0.0),
        TelemetryValue::Integer(i32::MIN),
        TelemetryValue::Integer(i32::MAX),
        TelemetryValue::Boolean(false),
        TelemetryValue::String(String::new()),
    ];
    for v in &variants {
        let debug = format!("{v:?}");
        assert!(!debug.is_empty());
    }
    Ok(())
}

#[test]
fn telemetry_value_serde_empty_string() -> TestResult {
    let v = TelemetryValue::String(String::new());
    let json = serde_json::to_string(&v)?;
    let decoded: TelemetryValue = serde_json::from_str(&json)?;
    assert_eq!(v, decoded);
    Ok(())
}

#[test]
fn telemetry_value_serde_negative_float() -> TestResult {
    let v = TelemetryValue::Float(-999.5);
    let json = serde_json::to_string(&v)?;
    let decoded: TelemetryValue = serde_json::from_str(&json)?;
    assert_eq!(v, decoded);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// TelemetryFieldCoverage / FlagCoverage
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn field_coverage_with_no_extended_fields() -> TestResult {
    let coverage = TelemetryFieldCoverage {
        game_id: "minimal".to_string(),
        game_version: "0.1".to_string(),
        ffb_scalar: false,
        rpm: false,
        speed: false,
        slip_ratio: false,
        gear: false,
        flags: FlagCoverage {
            yellow_flag: false,
            red_flag: false,
            blue_flag: false,
            checkered_flag: false,
            green_flag: false,
            pit_limiter: false,
            in_pits: false,
            drs_available: false,
            drs_active: false,
            ers_available: false,
            launch_control: false,
            traction_control: false,
            abs_active: false,
        },
        car_id: false,
        track_id: false,
        extended_fields: vec![],
    };
    let json = serde_json::to_string(&coverage)?;
    let decoded: TelemetryFieldCoverage = serde_json::from_str(&json)?;
    assert_eq!(decoded.game_id, "minimal");
    assert!(!decoded.ffb_scalar);
    assert!(decoded.extended_fields.is_empty());
    Ok(())
}

#[test]
fn field_coverage_with_all_flags_true() -> TestResult {
    let coverage = TelemetryFieldCoverage {
        game_id: "full".to_string(),
        game_version: "2.0".to_string(),
        ffb_scalar: true,
        rpm: true,
        speed: true,
        slip_ratio: true,
        gear: true,
        flags: FlagCoverage {
            yellow_flag: true,
            red_flag: true,
            blue_flag: true,
            checkered_flag: true,
            green_flag: true,
            pit_limiter: true,
            in_pits: true,
            drs_available: true,
            drs_active: true,
            ers_available: true,
            launch_control: true,
            traction_control: true,
            abs_active: true,
        },
        car_id: true,
        track_id: true,
        extended_fields: vec!["a".to_string(), "b".to_string(), "c".to_string()],
    };
    let json = serde_json::to_string(&coverage)?;
    let decoded: TelemetryFieldCoverage = serde_json::from_str(&json)?;
    assert!(decoded.flags.yellow_flag);
    assert!(decoded.flags.abs_active);
    assert_eq!(decoded.extended_fields.len(), 3);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// NormalizedTelemetry partial equality
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn telemetry_with_different_extended_maps_not_equal() -> TestResult {
    let a = NormalizedTelemetry::new().with_extended("k".to_string(), TelemetryValue::Integer(1));
    let b = NormalizedTelemetry::new().with_extended("k".to_string(), TelemetryValue::Integer(2));
    assert_ne!(a, b);
    Ok(())
}

#[test]
fn telemetry_with_different_flags_not_equal() -> TestResult {
    let a = NormalizedTelemetry::new().with_flags(TelemetryFlags {
        yellow_flag: true,
        ..TelemetryFlags::default()
    });
    let b = NormalizedTelemetry::new().with_flags(TelemetryFlags::default());
    assert_ne!(a, b);
    Ok(())
}
