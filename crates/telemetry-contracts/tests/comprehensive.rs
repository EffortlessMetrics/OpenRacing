//! Comprehensive integration tests for racing-wheel-telemetry-contracts.
//!
//! Covers: trait object construction, adapter contract compliance patterns,
//! error types and conversion, serde round-trips, and builder validation.

use std::collections::HashMap;

use racing_wheel_telemetry_contracts::{
    FlagCoverage, NormalizedTelemetry, TelemetryFieldCoverage, TelemetryFlags, TelemetryFrame,
    TelemetryValue,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Trait object construction ────────────────────────────────────────────

#[test]
fn normalized_telemetry_is_send_and_sync() -> TestResult {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<NormalizedTelemetry>();
    assert_send_sync::<TelemetryFlags>();
    assert_send_sync::<TelemetryValue>();
    assert_send_sync::<TelemetryFrame>();
    assert_send_sync::<TelemetryFieldCoverage>();
    assert_send_sync::<FlagCoverage>();
    Ok(())
}

#[test]
fn normalized_telemetry_new_is_default() -> TestResult {
    let a = NormalizedTelemetry::new();
    let b = NormalizedTelemetry::default();
    assert_eq!(a, b);
    Ok(())
}

#[test]
fn default_telemetry_has_none_optional_fields() -> TestResult {
    let t = NormalizedTelemetry::new();
    assert!(t.ffb_scalar.is_none());
    assert!(t.rpm.is_none());
    assert!(t.speed_ms.is_none());
    assert!(t.slip_ratio.is_none());
    assert!(t.gear.is_none());
    assert!(t.car_id.is_none());
    assert!(t.track_id.is_none());
    assert!(t.extended.is_empty());
    Ok(())
}

// ── Builder / with_* chaining ────────────────────────────────────────────

#[test]
fn builder_chain_populates_all_fields() -> TestResult {
    let t = NormalizedTelemetry::new()
        .with_ffb_scalar(0.5)
        .with_rpm(6000.0)
        .with_speed_ms(40.0)
        .with_slip_ratio(0.2)
        .with_gear(4)
        .with_car_id("porsche_911".to_string())
        .with_track_id("spa".to_string())
        .with_extended("boost".to_string(), TelemetryValue::Float(1.2));

    assert_eq!(t.ffb_scalar, Some(0.5));
    assert_eq!(t.rpm, Some(6000.0));
    assert_eq!(t.speed_ms, Some(40.0));
    assert_eq!(t.slip_ratio, Some(0.2));
    assert_eq!(t.gear, Some(4));
    assert_eq!(t.car_id.as_deref(), Some("porsche_911"));
    assert_eq!(t.track_id.as_deref(), Some("spa"));
    assert_eq!(t.extended.len(), 1);
    Ok(())
}

// ── FFB scalar clamping ─────────────────────────────────────────────────

#[test]
fn ffb_scalar_clamps_above_one() -> TestResult {
    let t = NormalizedTelemetry::new().with_ffb_scalar(5.0);
    assert_eq!(t.ffb_scalar, Some(1.0));
    Ok(())
}

#[test]
fn ffb_scalar_clamps_below_negative_one() -> TestResult {
    let t = NormalizedTelemetry::new().with_ffb_scalar(-5.0);
    assert_eq!(t.ffb_scalar, Some(-1.0));
    Ok(())
}

#[test]
fn ffb_scalar_preserves_boundary_values() -> TestResult {
    assert_eq!(
        NormalizedTelemetry::new().with_ffb_scalar(-1.0).ffb_scalar,
        Some(-1.0)
    );
    assert_eq!(
        NormalizedTelemetry::new().with_ffb_scalar(0.0).ffb_scalar,
        Some(0.0)
    );
    assert_eq!(
        NormalizedTelemetry::new().with_ffb_scalar(1.0).ffb_scalar,
        Some(1.0)
    );
    Ok(())
}

#[test]
fn ffb_scalar_infinity_clamps() -> TestResult {
    assert_eq!(
        NormalizedTelemetry::new()
            .with_ffb_scalar(f32::INFINITY)
            .ffb_scalar,
        Some(1.0)
    );
    assert_eq!(
        NormalizedTelemetry::new()
            .with_ffb_scalar(f32::NEG_INFINITY)
            .ffb_scalar,
        Some(-1.0)
    );
    Ok(())
}

// ── RPM validation ──────────────────────────────────────────────────────

#[test]
fn rpm_rejects_negative_and_non_finite() -> TestResult {
    assert!(NormalizedTelemetry::new().with_rpm(-1.0).rpm.is_none());
    assert!(NormalizedTelemetry::new().with_rpm(f32::NAN).rpm.is_none());
    assert!(
        NormalizedTelemetry::new()
            .with_rpm(f32::INFINITY)
            .rpm
            .is_none()
    );
    Ok(())
}

#[test]
fn rpm_accepts_zero_and_positive() -> TestResult {
    assert_eq!(NormalizedTelemetry::new().with_rpm(0.0).rpm, Some(0.0));
    assert_eq!(
        NormalizedTelemetry::new().with_rpm(9000.0).rpm,
        Some(9000.0)
    );
    Ok(())
}

// ── Speed validation ────────────────────────────────────────────────────

#[test]
fn speed_rejects_negative_and_non_finite() -> TestResult {
    assert!(
        NormalizedTelemetry::new()
            .with_speed_ms(-1.0)
            .speed_ms
            .is_none()
    );
    assert!(
        NormalizedTelemetry::new()
            .with_speed_ms(f32::NAN)
            .speed_ms
            .is_none()
    );
    Ok(())
}

#[test]
fn speed_accepts_zero() -> TestResult {
    assert_eq!(
        NormalizedTelemetry::new().with_speed_ms(0.0).speed_ms,
        Some(0.0)
    );
    Ok(())
}

// ── Slip ratio validation ───────────────────────────────────────────────

#[test]
fn slip_ratio_clamps_to_zero_one() -> TestResult {
    assert_eq!(
        NormalizedTelemetry::new().with_slip_ratio(-0.5).slip_ratio,
        Some(0.0)
    );
    assert_eq!(
        NormalizedTelemetry::new().with_slip_ratio(1.5).slip_ratio,
        Some(1.0)
    );
    Ok(())
}

#[test]
fn slip_ratio_rejects_non_finite() -> TestResult {
    assert!(
        NormalizedTelemetry::new()
            .with_slip_ratio(f32::NAN)
            .slip_ratio
            .is_none()
    );
    assert!(
        NormalizedTelemetry::new()
            .with_slip_ratio(f32::INFINITY)
            .slip_ratio
            .is_none()
    );
    Ok(())
}

// ── String field validation ─────────────────────────────────────────────

#[test]
fn car_id_rejects_empty_string() -> TestResult {
    assert!(
        NormalizedTelemetry::new()
            .with_car_id(String::new())
            .car_id
            .is_none()
    );
    Ok(())
}

#[test]
fn track_id_rejects_empty_string() -> TestResult {
    assert!(
        NormalizedTelemetry::new()
            .with_track_id(String::new())
            .track_id
            .is_none()
    );
    Ok(())
}

// ── Flags ───────────────────────────────────────────────────────────────

#[test]
fn default_flags_only_green_flag_true() -> TestResult {
    let flags = TelemetryFlags::default();
    assert!(flags.green_flag);
    assert!(!flags.yellow_flag);
    assert!(!flags.red_flag);
    assert!(!flags.blue_flag);
    assert!(!flags.checkered_flag);
    assert!(!flags.pit_limiter);
    assert!(!flags.in_pits);
    assert!(!flags.drs_available);
    assert!(!flags.drs_active);
    assert!(!flags.ers_available);
    assert!(!flags.launch_control);
    assert!(!flags.traction_control);
    assert!(!flags.abs_active);
    Ok(())
}

#[test]
fn has_active_flags_detects_hazard_flags() -> TestResult {
    let t_default = NormalizedTelemetry::new();
    assert!(!t_default.has_active_flags());

    for make_flags in [
        |f: &mut TelemetryFlags| f.yellow_flag = true,
        |f: &mut TelemetryFlags| f.red_flag = true,
        |f: &mut TelemetryFlags| f.blue_flag = true,
        |f: &mut TelemetryFlags| f.checkered_flag = true,
    ] {
        let mut flags = TelemetryFlags::default();
        make_flags(&mut flags);
        let t = NormalizedTelemetry::new().with_flags(flags);
        assert!(t.has_active_flags());
    }
    Ok(())
}

#[test]
fn has_active_flags_ignores_assist_flags() -> TestResult {
    let flags = TelemetryFlags {
        pit_limiter: true,
        drs_active: true,
        abs_active: true,
        ..TelemetryFlags::default()
    };
    assert!(
        !NormalizedTelemetry::new()
            .with_flags(flags)
            .has_active_flags()
    );
    Ok(())
}

// ── Derived queries ─────────────────────────────────────────────────────

#[test]
fn has_ffb_data_and_has_rpm_data() -> TestResult {
    let t = NormalizedTelemetry::new();
    assert!(!t.has_ffb_data());
    assert!(!t.has_rpm_data());

    assert!(
        NormalizedTelemetry::new()
            .with_ffb_scalar(0.0)
            .has_ffb_data()
    );
    assert!(NormalizedTelemetry::new().with_rpm(1000.0).has_rpm_data());
    Ok(())
}

#[test]
fn rpm_fraction_computes_correctly() -> TestResult {
    let t = NormalizedTelemetry::new().with_rpm(4000.0);
    let frac = t.rpm_fraction(8000.0);
    assert!(frac.is_some());
    let val = frac.unwrap_or(0.0);
    assert!((val - 0.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn rpm_fraction_clamps_to_one() -> TestResult {
    let t = NormalizedTelemetry::new().with_rpm(10000.0);
    assert_eq!(t.rpm_fraction(8000.0), Some(1.0));
    Ok(())
}

#[test]
fn rpm_fraction_returns_none_without_rpm() -> TestResult {
    assert!(NormalizedTelemetry::new().rpm_fraction(8000.0).is_none());
    Ok(())
}

#[test]
fn speed_conversion_kmh_and_mph() -> TestResult {
    let t = NormalizedTelemetry::new().with_speed_ms(10.0);
    let kmh = t.speed_kmh().unwrap_or(0.0);
    let mph = t.speed_mph().unwrap_or(0.0);
    assert!((kmh - 36.0).abs() < 0.01);
    assert!((mph - 22.37).abs() < 0.01);
    Ok(())
}

#[test]
fn speed_conversions_none_without_speed() -> TestResult {
    let t = NormalizedTelemetry::new();
    assert!(t.speed_kmh().is_none());
    assert!(t.speed_mph().is_none());
    Ok(())
}

// ── Extended map ────────────────────────────────────────────────────────

#[test]
fn extended_map_insert_all_variants() -> TestResult {
    let t = NormalizedTelemetry::new()
        .with_extended("f".to_string(), TelemetryValue::Float(1.0))
        .with_extended("i".to_string(), TelemetryValue::Integer(42))
        .with_extended("b".to_string(), TelemetryValue::Boolean(true))
        .with_extended("s".to_string(), TelemetryValue::String("v".to_string()));
    assert_eq!(t.extended.len(), 4);
    assert_eq!(t.extended.get("f"), Some(&TelemetryValue::Float(1.0)));
    assert_eq!(t.extended.get("i"), Some(&TelemetryValue::Integer(42)));
    assert_eq!(t.extended.get("b"), Some(&TelemetryValue::Boolean(true)));
    assert_eq!(
        t.extended.get("s"),
        Some(&TelemetryValue::String("v".to_string()))
    );
    Ok(())
}

#[test]
fn extended_map_overwrites_same_key() -> TestResult {
    let t = NormalizedTelemetry::new()
        .with_extended("k".to_string(), TelemetryValue::Integer(1))
        .with_extended("k".to_string(), TelemetryValue::Integer(2));
    assert_eq!(t.extended.len(), 1);
    assert_eq!(t.extended.get("k"), Some(&TelemetryValue::Integer(2)));
    Ok(())
}

// ── TelemetryValue ──────────────────────────────────────────────────────

#[test]
fn telemetry_value_equality_and_inequality() -> TestResult {
    assert_eq!(TelemetryValue::Float(1.0), TelemetryValue::Float(1.0));
    assert_eq!(TelemetryValue::Integer(42), TelemetryValue::Integer(42));
    assert_eq!(TelemetryValue::Boolean(true), TelemetryValue::Boolean(true));
    assert_eq!(
        TelemetryValue::String("a".into()),
        TelemetryValue::String("a".into())
    );
    assert_ne!(TelemetryValue::Float(1.0), TelemetryValue::Integer(1));
    assert_ne!(
        TelemetryValue::Boolean(true),
        TelemetryValue::Boolean(false)
    );
    Ok(())
}

#[test]
fn telemetry_value_clone() -> TestResult {
    let original = TelemetryValue::String("test".to_string());
    let cloned = original.clone();
    assert_eq!(original, cloned);
    Ok(())
}

// ── TelemetryFrame ──────────────────────────────────────────────────────

#[test]
fn telemetry_frame_stores_all_fields() -> TestResult {
    let data = NormalizedTelemetry::new().with_rpm(5000.0);
    let frame = TelemetryFrame::new(data.clone(), 123_456_789, 42, 64);
    assert_eq!(frame.timestamp_ns, 123_456_789);
    assert_eq!(frame.sequence, 42);
    assert_eq!(frame.raw_size, 64);
    assert_eq!(frame.data.rpm, data.rpm);
    Ok(())
}

// ── Serde round-trips ───────────────────────────────────────────────────

#[test]
fn normalized_telemetry_serde_round_trip() -> TestResult {
    let t = NormalizedTelemetry::new()
        .with_ffb_scalar(0.75)
        .with_rpm(6500.0)
        .with_speed_ms(50.0)
        .with_slip_ratio(0.1)
        .with_gear(3)
        .with_car_id("test_car".to_string())
        .with_track_id("test_track".to_string())
        .with_flags(TelemetryFlags {
            yellow_flag: true,
            ..TelemetryFlags::default()
        })
        .with_extended("temp".to_string(), TelemetryValue::Float(95.0));

    let json = serde_json::to_string(&t)?;
    let decoded: NormalizedTelemetry = serde_json::from_str(&json)?;
    assert_eq!(t, decoded);
    Ok(())
}

#[test]
fn telemetry_flags_serde_round_trip() -> TestResult {
    let flags = TelemetryFlags {
        yellow_flag: true,
        blue_flag: true,
        pit_limiter: true,
        abs_active: true,
        ..TelemetryFlags::default()
    };
    let json = serde_json::to_string(&flags)?;
    let decoded: TelemetryFlags = serde_json::from_str(&json)?;
    assert_eq!(flags, decoded);
    Ok(())
}

#[test]
fn telemetry_frame_serde_round_trip() -> TestResult {
    let frame = TelemetryFrame::new(
        NormalizedTelemetry::new().with_rpm(3000.0).with_gear(2),
        1_000_000_000,
        99,
        128,
    );
    let json = serde_json::to_string(&frame)?;
    let decoded: TelemetryFrame = serde_json::from_str(&json)?;
    assert_eq!(decoded.timestamp_ns, frame.timestamp_ns);
    assert_eq!(decoded.sequence, frame.sequence);
    assert_eq!(decoded.raw_size, frame.raw_size);
    assert_eq!(decoded.data, frame.data);
    Ok(())
}

#[test]
fn telemetry_value_serde_all_variants() -> TestResult {
    let variants = vec![
        TelemetryValue::Float(3.125),
        TelemetryValue::Integer(-42),
        TelemetryValue::Boolean(false),
        TelemetryValue::String("hello".to_string()),
    ];
    for v in &variants {
        let json = serde_json::to_string(v)?;
        let decoded: TelemetryValue = serde_json::from_str(&json)?;
        assert_eq!(&decoded, v);
    }
    Ok(())
}

#[test]
fn telemetry_field_coverage_serde_round_trip() -> TestResult {
    let coverage = TelemetryFieldCoverage {
        game_id: "test_game".to_string(),
        game_version: "1.0".to_string(),
        ffb_scalar: true,
        rpm: true,
        speed: true,
        slip_ratio: false,
        gear: true,
        flags: FlagCoverage {
            yellow_flag: true,
            red_flag: true,
            blue_flag: false,
            checkered_flag: true,
            green_flag: true,
            pit_limiter: false,
            in_pits: true,
            drs_available: false,
            drs_active: false,
            ers_available: false,
            launch_control: false,
            traction_control: false,
            abs_active: true,
        },
        car_id: true,
        track_id: false,
        extended_fields: vec!["fuel".to_string(), "tire_temp".to_string()],
    };
    let json = serde_json::to_string(&coverage)?;
    let decoded: TelemetryFieldCoverage = serde_json::from_str(&json)?;
    assert_eq!(decoded.game_id, coverage.game_id);
    assert_eq!(decoded.ffb_scalar, coverage.ffb_scalar);
    assert_eq!(decoded.flags.yellow_flag, coverage.flags.yellow_flag);
    assert_eq!(decoded.flags.abs_active, coverage.flags.abs_active);
    assert_eq!(decoded.extended_fields, coverage.extended_fields);
    Ok(())
}

// ── Clone / PartialEq ──────────────────────────────────────────────────

#[test]
fn normalized_telemetry_clone_equals_original() -> TestResult {
    let t = NormalizedTelemetry::new()
        .with_ffb_scalar(0.5)
        .with_rpm(3000.0)
        .with_gear(2);
    let cloned = t.clone();
    assert_eq!(t, cloned);
    Ok(())
}

#[test]
fn default_extended_map_is_hashmap() -> TestResult {
    let t = NormalizedTelemetry::new();
    let empty: HashMap<String, TelemetryValue> = HashMap::new();
    assert_eq!(t.extended, empty);
    Ok(())
}

// ── Gear range ──────────────────────────────────────────────────────────

#[test]
fn gear_supports_reverse_neutral_and_forward() -> TestResult {
    assert_eq!(NormalizedTelemetry::new().with_gear(-1).gear, Some(-1));
    assert_eq!(NormalizedTelemetry::new().with_gear(0).gear, Some(0));
    assert_eq!(NormalizedTelemetry::new().with_gear(8).gear, Some(8));
    Ok(())
}

// ── Debug formatting ────────────────────────────────────────────────────

#[test]
fn all_types_implement_debug() -> TestResult {
    let t = NormalizedTelemetry::new();
    let flags = TelemetryFlags::default();
    let frame = TelemetryFrame::new(t.clone(), 0, 0, 0);
    let val = TelemetryValue::Float(1.0);

    assert!(!format!("{t:?}").is_empty());
    assert!(!format!("{flags:?}").is_empty());
    assert!(!format!("{frame:?}").is_empty());
    assert!(!format!("{val:?}").is_empty());
    Ok(())
}
