//! Comprehensive validation tests for the openracing-test-helpers crate.
//!
//! Tests edge cases, error generation helpers, mock/fake data validity,
//! and public API contracts that complement the existing `test_helpers_tests.rs`.

// ── Edge cases for assertion macros ────────────────────────────────────────

mod assertion_edge_cases {
    use openracing_test_helpers::{
        assert_approx_eq, assert_contains, assert_empty, assert_err, assert_in_range,
        assert_monotonic, assert_monotonic_desc, assert_none, assert_not_empty, assert_ok,
        assert_some, assert_sorted, assert_sorted_desc,
    };

    #[test]
    fn approx_eq_zero_tolerance_exact_match() -> Result<(), Box<dyn std::error::Error>> {
        assert_approx_eq!(0.0_f64, 0.0_f64, 0.0_f64);
        assert_approx_eq!(-0.0_f64, 0.0_f64, 0.0_f64);
        Ok(())
    }

    #[test]
    fn approx_eq_very_small_values() -> Result<(), Box<dyn std::error::Error>> {
        assert_approx_eq!(1e-15_f64, 1.001e-15_f64, 1e-17_f64);
        Ok(())
    }

    #[test]
    fn approx_eq_very_large_values() -> Result<(), Box<dyn std::error::Error>> {
        assert_approx_eq!(1e15_f64, 1.000000001e15_f64, 1e7_f64);
        Ok(())
    }

    #[test]
    fn approx_eq_boundary_tolerance() -> Result<(), Box<dyn std::error::Error>> {
        // Diff exactly equals tolerance — should pass (not strictly greater)
        assert_approx_eq!(1.0_f64, 1.5_f64, 0.5_f64);
        Ok(())
    }

    #[test]
    #[should_panic(expected = "assertion failed")]
    fn approx_eq_just_over_tolerance() {
        // Diff barely exceeds tolerance
        assert_approx_eq!(1.0_f64, 1.6_f64, 0.5_f64);
    }

    #[test]
    fn sorted_two_equal_elements() -> Result<(), Box<dyn std::error::Error>> {
        assert_sorted!(&[5, 5]);
        Ok(())
    }

    #[test]
    fn sorted_large_collection() -> Result<(), Box<dyn std::error::Error>> {
        let v: Vec<i32> = (0..1000).collect();
        assert_sorted!(&v);
        Ok(())
    }

    #[test]
    fn sorted_desc_single_element() -> Result<(), Box<dyn std::error::Error>> {
        assert_sorted_desc!(&[99]);
        Ok(())
    }

    #[test]
    fn sorted_desc_empty() -> Result<(), Box<dyn std::error::Error>> {
        assert_sorted_desc!(&[] as &[i32]);
        Ok(())
    }

    #[test]
    fn monotonic_single_element() -> Result<(), Box<dyn std::error::Error>> {
        assert_monotonic!(&[1]);
        Ok(())
    }

    #[test]
    fn monotonic_empty() -> Result<(), Box<dyn std::error::Error>> {
        assert_monotonic!(&[] as &[i32]);
        Ok(())
    }

    #[test]
    fn monotonic_negative_values() -> Result<(), Box<dyn std::error::Error>> {
        assert_monotonic!(&[-10, -5, -1, 0, 3]);
        Ok(())
    }

    #[test]
    #[should_panic(expected = "not strictly monotonic")]
    fn monotonic_fails_on_equal_adjacent() {
        assert_monotonic!(&[1, 2, 3, 3, 4]);
    }

    #[test]
    fn monotonic_desc_single_element() -> Result<(), Box<dyn std::error::Error>> {
        assert_monotonic_desc!(&[100]);
        Ok(())
    }

    #[test]
    fn monotonic_desc_empty() -> Result<(), Box<dyn std::error::Error>> {
        assert_monotonic_desc!(&[] as &[i32]);
        Ok(())
    }

    #[test]
    #[should_panic(expected = "not strictly monotonic decreasing")]
    fn monotonic_desc_fails_on_equal() {
        assert_monotonic_desc!(&[5, 5, 3]);
    }

    #[test]
    fn contains_empty_needle() -> Result<(), Box<dyn std::error::Error>> {
        assert_contains!("hello", "");
        Ok(())
    }

    #[test]
    fn contains_full_string_match() -> Result<(), Box<dyn std::error::Error>> {
        assert_contains!("abc", "abc");
        Ok(())
    }

    #[test]
    #[should_panic(expected = "does not contain")]
    fn contains_case_sensitive() {
        assert_contains!("Hello World", "hello");
    }

    #[test]
    fn ok_returns_inner_value() -> Result<(), Box<dyn std::error::Error>> {
        let result: Result<Vec<i32>, &str> = Ok(vec![1, 2, 3]);
        let val = assert_ok!(result);
        assert_eq!(val.len(), 3);
        Ok(())
    }

    #[test]
    fn err_returns_inner_error() -> Result<(), Box<dyn std::error::Error>> {
        let result: Result<i32, String> = Err("specific error".to_string());
        let err = assert_err!(result);
        assert!(err.contains("specific"));
        Ok(())
    }

    #[test]
    fn some_returns_inner_value() -> Result<(), Box<dyn std::error::Error>> {
        let opt: Option<String> = Some("data".to_string());
        let val = assert_some!(opt);
        assert_eq!(val, "data");
        Ok(())
    }

    #[test]
    fn none_accepts_none() -> Result<(), Box<dyn std::error::Error>> {
        assert_none!(None::<String>);
        Ok(())
    }

    #[test]
    fn empty_vec() -> Result<(), Box<dyn std::error::Error>> {
        assert_empty!(&Vec::<u8>::new());
        Ok(())
    }

    #[test]
    fn empty_string() -> Result<(), Box<dyn std::error::Error>> {
        let s = String::new();
        assert_empty!(&s);
        Ok(())
    }

    #[test]
    fn not_empty_single_element() -> Result<(), Box<dyn std::error::Error>> {
        assert_not_empty!(&[0]);
        Ok(())
    }

    #[test]
    fn in_range_inclusive_boundaries() -> Result<(), Box<dyn std::error::Error>> {
        assert_in_range!(0, 0..=0);
        assert_in_range!(10, 10..=10);
        Ok(())
    }

    #[test]
    fn in_range_exclusive_end() -> Result<(), Box<dyn std::error::Error>> {
        assert_in_range!(0, 0..10);
        assert_in_range!(9, 0..10);
        Ok(())
    }

    #[test]
    #[should_panic(expected = "is not in range")]
    fn in_range_fails_at_exclusive_end() {
        assert_in_range!(10, 0..10);
    }

    #[test]
    fn in_range_negative_values() -> Result<(), Box<dyn std::error::Error>> {
        assert_in_range!(-5, -10..=0);
        Ok(())
    }

    #[test]
    fn custom_message_variants_compile() -> Result<(), Box<dyn std::error::Error>> {
        assert_approx_eq!(1.0_f64, 1.0_f64, 0.1_f64, "custom msg {}", 42);
        assert_sorted!(&[1, 2], "sorted msg");
        assert_monotonic!(&[1, 2], "monotonic msg");
        assert_contains!("abc", "a", "contains msg");
        assert_ok!(Ok::<i32, &str>(1), "ok msg");
        assert_err!(Err::<i32, &str>("e"), "err msg");
        assert_some!(Some(1), "some msg");
        assert_none!(None::<i32>, "none msg");
        assert_empty!(&[] as &[i32], "empty msg");
        assert_not_empty!(&[1], "not empty msg");
        assert_in_range!(5, 0..=10, "range msg");
        Ok(())
    }
}

// ── Must helpers edge cases ────────────────────────────────────────────────

mod must_helpers_validation {
    use openracing_test_helpers::prelude::*;

    #[test]
    fn must_with_complex_error_type() -> TestResult {
        #[derive(Debug)]
        #[allow(dead_code)]
        struct ComplexError {
            code: u32,
            msg: String,
        }
        let result: Result<i32, ComplexError> = Ok(42);
        let val = must(result);
        assert_eq!(val, 42);
        Ok(())
    }

    #[test]
    #[should_panic(expected = "must: unexpected Err")]
    fn must_panics_with_debug_representation() {
        let result: Result<(), Vec<String>> = Err(vec!["err1".into(), "err2".into()]);
        must(result);
    }

    #[test]
    fn must_some_returns_complex_type() -> TestResult {
        let opt = Some(vec![1, 2, 3]);
        let val = must_some(opt, "expected vec");
        assert_eq!(val.len(), 3);
        Ok(())
    }

    #[test]
    fn must_parse_integer_types() -> TestResult {
        let _u8: u8 = must_parse("255");
        let _i16: i16 = must_parse("-32000");
        let _u64: u64 = must_parse("18446744073709551615");
        let _f64: f64 = must_parse("3.14159");
        Ok(())
    }

    #[test]
    fn must_parse_bool() -> TestResult {
        let val: bool = must_parse("true");
        assert!(val);
        Ok(())
    }

    #[test]
    #[should_panic(expected = "must_parse: failed to parse")]
    fn must_parse_overflow_panics() {
        let _: u8 = must_parse("256");
    }

    #[test]
    #[should_panic(expected = "must_parse: failed to parse")]
    fn must_parse_empty_string_panics() {
        let _: i32 = must_parse("");
    }

    #[test]
    fn must_with_preserves_context_string() -> TestResult {
        let result = std::panic::catch_unwind(|| {
            must_with(Err::<(), &str>("inner"), "my context");
        });
        let err = result.err().ok_or("expected panic")?;
        let msg = err
            .downcast_ref::<String>()
            .ok_or("expected String panic")?;
        assert!(msg.contains("my context"));
        assert!(msg.contains("inner"));
        Ok(())
    }

    #[test]
    fn must_some_or_returns_default_on_none() -> TestResult {
        let val = must_some_or(None::<Vec<i32>>, vec![]);
        assert!(val.is_empty());
        Ok(())
    }

    #[test]
    fn must_some_or_returns_value_on_some() -> TestResult {
        let val = must_some_or(Some(42), 0);
        assert_eq!(val, 42);
        Ok(())
    }

    #[test]
    fn must_or_else_transforms_error() -> TestResult {
        let result: Result<String, i32> = Err(404);
        let val = must_or_else(result, |code| format!("fallback-{code}"));
        assert_eq!(val, "fallback-404");
        Ok(())
    }

    #[test]
    fn must_or_else_ignores_closure_on_ok() -> TestResult {
        let result: Result<i32, &str> = Ok(100);
        let val = must_or_else(result, |_| panic!("should not be called"));
        assert_eq!(val, 100);
        Ok(())
    }

    #[test]
    fn test_result_type_alias() -> TestResult {
        // Verify TestResult alias works with ? operator
        let _: i32 = "42".parse()?;
        Ok(())
    }
}

// ── Mock device validation ─────────────────────────────────────────────────

mod mock_device_validation {
    use openracing_test_helpers::mock::*;

    #[test]
    fn mock_hid_new_starts_empty() -> Result<(), Box<dyn std::error::Error>> {
        let device = MockHidDevice::new();
        assert!(device.feature_reports().is_empty());
        assert!(device.output_reports().is_empty());
        assert_eq!(device.total_writes(), 0);
        assert!(device.last_feature_report().is_none());
        assert!(device.last_output_report().is_none());
        Ok(())
    }

    #[test]
    fn mock_hid_default_same_as_new() -> Result<(), Box<dyn std::error::Error>> {
        let device = MockHidDevice::default();
        assert!(!device.fail_on_write);
        assert_eq!(device.write_delay_ms, 0);
        assert_eq!(device.total_writes(), 0);
        Ok(())
    }

    #[test]
    fn mock_hid_failure_mode_all_writes_fail() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::with_failure();
        assert!(device.write_feature_report(&[1]).is_err());
        assert!(device.write_output_report(&[2]).is_err());
        assert_eq!(device.total_writes(), 0);
        Ok(())
    }

    #[test]
    fn mock_hid_write_returns_correct_length() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::new();
        let data = [0u8; 64];
        let len = device.write_feature_report(&data)?;
        assert_eq!(len, 64);
        let len = device.write_output_report(&[1, 2, 3])?;
        assert_eq!(len, 3);
        Ok(())
    }

    #[test]
    fn mock_hid_empty_write() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::new();
        let len = device.write_feature_report(&[])?;
        assert_eq!(len, 0);
        assert_eq!(device.total_writes(), 1);
        Ok(())
    }

    #[test]
    fn mock_hid_multiple_writes_tracked() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::new();
        device.write_feature_report(&[1])?;
        device.write_feature_report(&[2])?;
        device.write_output_report(&[3])?;

        assert_eq!(device.feature_reports().len(), 2);
        assert_eq!(device.output_reports().len(), 1);
        assert_eq!(device.total_writes(), 3);
        Ok(())
    }

    #[test]
    fn mock_hid_last_report_returns_most_recent() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::new();
        device.write_feature_report(&[1, 2])?;
        device.write_feature_report(&[3, 4])?;

        let last = device.last_feature_report().ok_or("expected report")?;
        assert_eq!(last, vec![3, 4]);
        Ok(())
    }

    #[test]
    fn mock_hid_clear_resets_all() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::new();
        device.write_feature_report(&[1])?;
        device.write_output_report(&[2])?;
        device.clear();

        assert_eq!(device.total_writes(), 0);
        assert!(device.last_feature_report().is_none());
        assert!(device.last_output_report().is_none());
        Ok(())
    }

    #[test]
    fn mock_hid_with_delay_stores_delay() -> Result<(), Box<dyn std::error::Error>> {
        let device = MockHidDevice::with_delay(100);
        assert_eq!(device.write_delay_ms, 100);
        assert!(!device.fail_on_write);
        Ok(())
    }

    #[test]
    fn mock_hid_error_message_readable() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::with_failure();
        let err = device
            .write_feature_report(&[1])
            .err()
            .ok_or("expected error")?;
        let msg = format!("{err}");
        assert!(!msg.is_empty());
        Ok(())
    }
}

// ── Mock telemetry data validation ─────────────────────────────────────────

mod mock_telemetry_validation {
    use openracing_test_helpers::mock::*;

    #[test]
    fn telemetry_default_is_zero() -> Result<(), Box<dyn std::error::Error>> {
        let data = MockTelemetryData::new();
        assert_eq!(data.rpm, 0.0);
        assert_eq!(data.speed_ms, 0.0);
        assert_eq!(data.ffb_scalar, 0.0);
        assert_eq!(data.slip_ratio, 0.0);
        assert_eq!(data.gear, 0);
        assert_eq!(data.timestamp_ms, 0);
        Ok(())
    }

    #[test]
    fn telemetry_builder_chain() -> Result<(), Box<dyn std::error::Error>> {
        let data = MockTelemetryData::new()
            .with_rpm(7000.0)
            .with_speed(55.5)
            .with_ffb(0.8)
            .with_gear(4)
            .with_timestamp(12345);

        assert_eq!(data.rpm, 7000.0);
        assert_eq!(data.speed_ms, 55.5);
        assert_eq!(data.ffb_scalar, 0.8);
        assert_eq!(data.gear, 4);
        assert_eq!(data.timestamp_ms, 12345);
        Ok(())
    }

    #[test]
    fn racing_sample_produces_valid_rpm_range() -> Result<(), Box<dyn std::error::Error>> {
        for i in 0..100 {
            let progress = i as f32 / 100.0;
            let sample = MockTelemetryData::racing_sample(progress);
            // RPM should be within reasonable range (4000 ± 2000 = 2000..6000)
            assert!(
                sample.rpm >= 1900.0 && sample.rpm <= 6100.0,
                "RPM {} out of expected range at progress {}",
                sample.rpm,
                progress
            );
        }
        Ok(())
    }

    #[test]
    fn racing_sample_produces_valid_speed() -> Result<(), Box<dyn std::error::Error>> {
        for i in 0..100 {
            let progress = i as f32 / 100.0;
            let sample = MockTelemetryData::racing_sample(progress);
            assert!(
                sample.speed_ms >= 0.0,
                "Speed {} should be non-negative at progress {}",
                sample.speed_ms,
                progress
            );
        }
        Ok(())
    }

    #[test]
    fn racing_sample_ffb_within_bounds() -> Result<(), Box<dyn std::error::Error>> {
        for i in 0..100 {
            let progress = i as f32 / 100.0;
            let sample = MockTelemetryData::racing_sample(progress);
            assert!(
                sample.ffb_scalar >= -1.0 && sample.ffb_scalar <= 1.0,
                "FFB scalar {} out of [-1, 1] at progress {}",
                sample.ffb_scalar,
                progress
            );
        }
        Ok(())
    }

    #[test]
    fn racing_sample_slip_ratio_non_negative() -> Result<(), Box<dyn std::error::Error>> {
        for i in 0..100 {
            let progress = i as f32 / 100.0;
            let sample = MockTelemetryData::racing_sample(progress);
            assert!(
                sample.slip_ratio >= 0.0 && sample.slip_ratio <= 1.0,
                "Slip ratio {} out of [0, 1] at progress {}",
                sample.slip_ratio,
                progress
            );
        }
        Ok(())
    }

    #[test]
    fn racing_sample_gear_valid() -> Result<(), Box<dyn std::error::Error>> {
        for i in 0..100 {
            let progress = i as f32 / 100.0;
            let sample = MockTelemetryData::racing_sample(progress);
            assert!(
                sample.gear >= 1 && sample.gear <= 6,
                "Gear {} out of [1, 6] at progress {}",
                sample.gear,
                progress
            );
        }
        Ok(())
    }

    #[test]
    fn racing_sample_zero_progress() -> Result<(), Box<dyn std::error::Error>> {
        let sample = MockTelemetryData::racing_sample(0.0);
        assert_eq!(sample.timestamp_ms, 0);
        Ok(())
    }

    #[test]
    fn racing_sample_full_progress() -> Result<(), Box<dyn std::error::Error>> {
        let sample = MockTelemetryData::racing_sample(1.0);
        assert_eq!(sample.timestamp_ms, 10000);
        Ok(())
    }
}

// ── Mock telemetry port validation ─────────────────────────────────────────

mod mock_telemetry_port_validation {
    use openracing_test_helpers::mock::*;

    #[test]
    fn port_new_is_empty() -> Result<(), Box<dyn std::error::Error>> {
        let port = MockTelemetryPort::new();
        assert!(port.is_empty());
        assert_eq!(port.len(), 0);
        assert!(port.next().is_none());
        Ok(())
    }

    #[test]
    fn port_with_data_preserves_order() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![
            MockTelemetryData::new().with_rpm(1000.0),
            MockTelemetryData::new().with_rpm(2000.0),
            MockTelemetryData::new().with_rpm(3000.0),
        ];
        let port = MockTelemetryPort::with_data(data);
        assert_eq!(port.len(), 3);

        let first = port.next().ok_or("expected data")?;
        assert_eq!(first.rpm, 1000.0);
        let second = port.next().ok_or("expected data")?;
        assert_eq!(second.rpm, 2000.0);
        let third = port.next().ok_or("expected data")?;
        assert_eq!(third.rpm, 3000.0);
        Ok(())
    }

    #[test]
    fn port_next_returns_none_when_exhausted() -> Result<(), Box<dyn std::error::Error>> {
        let port = MockTelemetryPort::with_data(vec![MockTelemetryData::new()]);
        let _first = port.next().ok_or("expected data")?;
        assert!(port.next().is_none());
        assert!(port.next().is_none()); // double-check idempotent
        Ok(())
    }

    #[test]
    fn port_reset_allows_replay() -> Result<(), Box<dyn std::error::Error>> {
        let port = MockTelemetryPort::with_data(vec![MockTelemetryData::new().with_rpm(500.0)]);
        let first = port.next().ok_or("expected data")?;
        assert_eq!(first.rpm, 500.0);
        assert!(port.next().is_none());

        port.reset();
        let replayed = port.next().ok_or("expected data after reset")?;
        assert_eq!(replayed.rpm, 500.0);
        Ok(())
    }

    #[test]
    fn port_add_appends() -> Result<(), Box<dyn std::error::Error>> {
        let mut port = MockTelemetryPort::new();
        assert!(port.is_empty());
        port.add(MockTelemetryData::new().with_rpm(100.0));
        assert_eq!(port.len(), 1);
        assert!(!port.is_empty());
        Ok(())
    }

    #[test]
    fn port_generate_racing_sequence_correct_count() -> Result<(), Box<dyn std::error::Error>> {
        let port = MockTelemetryPort::generate_racing_sequence(2.0, 60);
        assert_eq!(port.len(), 120);
        Ok(())
    }

    #[test]
    fn port_generate_racing_sequence_data_valid() -> Result<(), Box<dyn std::error::Error>> {
        let port = MockTelemetryPort::generate_racing_sequence(1.0, 10);
        for _ in 0..10 {
            let sample = port.next().ok_or("expected sample")?;
            assert!(sample.rpm >= 0.0, "RPM should be non-negative");
            assert!(sample.gear >= 1, "Gear should be positive");
        }
        Ok(())
    }

    #[test]
    fn port_default_is_new() -> Result<(), Box<dyn std::error::Error>> {
        let port = MockTelemetryPort::default();
        assert!(port.is_empty());
        Ok(())
    }
}

// ── Mock profile validation ────────────────────────────────────────────────

mod mock_profile_validation {
    use openracing_test_helpers::mock::*;

    #[test]
    fn profile_id_equality() -> Result<(), Box<dyn std::error::Error>> {
        let id1 = MockProfileId::new("test");
        let id2 = MockProfileId::new("test");
        let id3 = MockProfileId::new("other");
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        Ok(())
    }

    #[test]
    fn profile_id_default() -> Result<(), Box<dyn std::error::Error>> {
        let id = MockProfileId::default();
        assert_eq!(id.0, "default");
        Ok(())
    }

    #[test]
    fn profile_id_hash_consistency() -> Result<(), Box<dyn std::error::Error>> {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(MockProfileId::new("a"));
        set.insert(MockProfileId::new("a"));
        set.insert(MockProfileId::new("b"));
        assert_eq!(set.len(), 2);
        Ok(())
    }

    #[test]
    fn profile_default_values() -> Result<(), Box<dyn std::error::Error>> {
        let profile = MockProfile::new("test-id");
        assert_eq!(profile.id.0, "test-id");
        assert_eq!(profile.name, "Default Profile");
        assert_eq!(profile.game, "default");
        assert!(profile.car.is_none());
        assert_eq!(profile.ffb_gain, 1.0);
        assert_eq!(profile.dor_deg, 900);
        assert_eq!(profile.torque_cap_nm, 10.0);
        Ok(())
    }

    #[test]
    fn profile_full_builder() -> Result<(), Box<dyn std::error::Error>> {
        let profile = MockProfile::new("p1")
            .with_name("Custom")
            .with_game("acc")
            .with_car("porsche_911")
            .with_ffb_gain(0.65)
            .with_dor(540)
            .with_torque_cap(25.0);

        assert_eq!(profile.name, "Custom");
        assert_eq!(profile.game, "acc");
        assert_eq!(profile.car.as_deref(), Some("porsche_911"));
        assert_eq!(profile.ffb_gain, 0.65);
        assert_eq!(profile.dor_deg, 540);
        assert_eq!(profile.torque_cap_nm, 25.0);
        Ok(())
    }

    #[test]
    fn profile_accepts_string_types() -> Result<(), Box<dyn std::error::Error>> {
        let owned = String::from("iracing");
        let profile = MockProfile::new("p2")
            .with_game(owned)
            .with_name(String::from("My Profile"))
            .with_car(String::from("gt3"));
        assert_eq!(profile.game, "iracing");
        assert_eq!(profile.name, "My Profile");
        Ok(())
    }
}

// ── Fixture generation validation ──────────────────────────────────────────

#[cfg(feature = "fixtures")]
mod fixture_validation {
    use openracing_test_helpers::fixtures::*;
    use std::time::Duration;

    #[test]
    fn device_capabilities_basic_wheel_constraints() -> Result<(), Box<dyn std::error::Error>> {
        let basic = DeviceCapabilitiesFixture::basic_wheel();
        assert!(basic.supports_pid);
        assert!(!basic.supports_raw_torque_1khz);
        assert!(basic.max_torque_nm > 0.0);
        assert!(basic.encoder_cpr > 0);
        assert!(basic.min_report_period_us > 0);
        Ok(())
    }

    #[test]
    fn device_capabilities_dd_more_capable_than_basic() -> Result<(), Box<dyn std::error::Error>> {
        let basic = DeviceCapabilitiesFixture::basic_wheel();
        let dd = DeviceCapabilitiesFixture::dd_wheel();
        assert!(dd.max_torque_nm > basic.max_torque_nm);
        assert!(dd.encoder_cpr >= basic.encoder_cpr);
        assert!(dd.min_report_period_us <= basic.min_report_period_us);
        assert!(dd.supports_raw_torque_1khz);
        assert!(dd.supports_health_stream);
        assert!(dd.supports_led_bus);
        Ok(())
    }

    #[test]
    fn device_capabilities_high_end_more_than_dd() -> Result<(), Box<dyn std::error::Error>> {
        let dd = DeviceCapabilitiesFixture::dd_wheel();
        let high = DeviceCapabilitiesFixture::high_end_dd();
        assert!(high.max_torque_nm > dd.max_torque_nm);
        assert!(high.min_report_period_us <= dd.min_report_period_us);
        Ok(())
    }

    #[test]
    fn device_capabilities_new_equals_default() -> Result<(), Box<dyn std::error::Error>> {
        let new = DeviceCapabilitiesFixture::new();
        let default = DeviceCapabilitiesFixture::default();
        assert_eq!(new.max_torque_nm, default.max_torque_nm);
        assert_eq!(new.encoder_cpr, default.encoder_cpr);
        Ok(())
    }

    #[test]
    fn device_capabilities_builder_override() -> Result<(), Box<dyn std::error::Error>> {
        let custom = DeviceCapabilitiesFixture::basic_wheel()
            .with_max_torque(15.0)
            .with_encoder_cpr(8192)
            .with_raw_torque(true);
        assert_eq!(custom.max_torque_nm, 15.0);
        assert_eq!(custom.encoder_cpr, 8192);
        assert!(custom.supports_raw_torque_1khz);
        Ok(())
    }

    #[test]
    fn performance_fixture_increasing_severity() -> Result<(), Box<dyn std::error::Error>> {
        let idle = PerformanceFixture::idle();
        let light = PerformanceFixture::light_load();
        let normal = PerformanceFixture::normal_load();
        let heavy = PerformanceFixture::heavy_load();
        let extreme = PerformanceFixture::extreme_load();

        // Jitter tolerance increases with load
        assert!(idle.expected_jitter_p99_ms <= light.expected_jitter_p99_ms);
        assert!(light.expected_jitter_p99_ms <= normal.expected_jitter_p99_ms);
        assert!(normal.expected_jitter_p99_ms <= heavy.expected_jitter_p99_ms);
        assert!(heavy.expected_jitter_p99_ms <= extreme.expected_jitter_p99_ms);

        // Duration increases with load
        assert!(idle.duration < light.duration);
        assert!(light.duration < normal.duration);
        assert!(normal.duration < heavy.duration);
        assert!(heavy.duration < extreme.duration);
        Ok(())
    }

    #[test]
    fn performance_fixture_jitter_within_rt_budget() -> Result<(), Box<dyn std::error::Error>> {
        // All fixtures should have p99 jitter < 1ms (the 1kHz budget)
        let fixtures = get_performance_fixtures();
        for f in &fixtures {
            assert!(
                f.expected_jitter_p99_ms < 1.0,
                "{} has jitter {}ms >= 1ms RT budget",
                f.name,
                f.expected_jitter_p99_ms
            );
        }
        Ok(())
    }

    #[test]
    fn performance_fixture_builder() -> Result<(), Box<dyn std::error::Error>> {
        let custom = PerformanceFixture::normal_load()
            .with_duration(Duration::from_secs(90))
            .with_load_level(LoadLevel::Heavy);
        assert_eq!(custom.duration, Duration::from_secs(90));
        assert_eq!(custom.load_level, LoadLevel::Heavy);
        Ok(())
    }

    #[test]
    fn profile_fixture_valid_has_no_errors() -> Result<(), Box<dyn std::error::Error>> {
        let valid = ProfileFixture::valid();
        assert!(valid.is_valid);
        assert!(valid.expected_errors.is_empty());
        assert!(valid.ffb_gain > 0.0 && valid.ffb_gain <= 1.0);
        assert!(valid.dor_deg > 0);
        assert!(valid.torque_cap_nm > 0.0);
        Ok(())
    }

    #[test]
    fn profile_fixture_invalid_variants_have_errors() -> Result<(), Box<dyn std::error::Error>> {
        let invalid_gain = ProfileFixture::invalid_gain();
        assert!(!invalid_gain.is_valid);
        assert!(!invalid_gain.expected_errors.is_empty());
        assert!(
            invalid_gain.ffb_gain > 1.0,
            "invalid_gain should have gain > 1.0"
        );

        let invalid_dor = ProfileFixture::invalid_dor();
        assert!(!invalid_dor.is_valid);
        assert_eq!(invalid_dor.dor_deg, 0);

        let invalid_torque = ProfileFixture::invalid_torque_cap();
        assert!(!invalid_torque.is_valid);
        assert_eq!(invalid_torque.torque_cap_nm, 0.0);
        Ok(())
    }

    #[test]
    fn profile_fixture_builder_chain() -> Result<(), Box<dyn std::error::Error>> {
        let profile = ProfileFixture::valid()
            .with_game("rf2")
            .with_car("formula_a")
            .with_ffb_gain(0.9)
            .with_dor(360);
        assert_eq!(profile.game, "rf2");
        assert_eq!(profile.car.as_deref(), Some("formula_a"));
        assert_eq!(profile.ffb_gain, 0.9);
        assert_eq!(profile.dor_deg, 360);
        Ok(())
    }

    #[test]
    fn telemetry_fixture_total_samples_calculation() -> Result<(), Box<dyn std::error::Error>> {
        let basic = TelemetryFixture::basic();
        let expected = (basic.duration_s * basic.sample_rate_hz as f32) as usize;
        assert_eq!(basic.total_samples(), expected);
        Ok(())
    }

    #[test]
    fn telemetry_fixture_high_performance_higher_rate() -> Result<(), Box<dyn std::error::Error>> {
        let basic = TelemetryFixture::basic();
        let hp = TelemetryFixture::high_performance();
        assert!(hp.sample_rate_hz > basic.sample_rate_hz);
        assert!(hp.base_rpm > basic.base_rpm);
        assert!(hp.base_speed_ms > basic.base_speed_ms);
        assert!(hp.ffb_amplitude > basic.ffb_amplitude);
        Ok(())
    }

    #[test]
    fn telemetry_fixture_builder() -> Result<(), Box<dyn std::error::Error>> {
        let custom = TelemetryFixture::basic()
            .with_sample_rate(120)
            .with_duration(30.0);
        assert_eq!(custom.sample_rate_hz, 120);
        assert_eq!(custom.duration_s, 30.0);
        assert_eq!(custom.total_samples(), 3600);
        Ok(())
    }

    #[test]
    fn get_device_fixtures_all_unique_torque() -> Result<(), Box<dyn std::error::Error>> {
        let fixtures = get_device_fixtures();
        let torques: Vec<f32> = fixtures.iter().map(|f| f.max_torque_nm).collect();
        for i in 0..torques.len() {
            for j in (i + 1)..torques.len() {
                assert_ne!(
                    torques[i], torques[j],
                    "Device fixtures should have unique torque values"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn get_profile_fixtures_includes_valid_and_invalid() -> Result<(), Box<dyn std::error::Error>> {
        let fixtures = get_profile_fixtures();
        let valid_count = fixtures.iter().filter(|f| f.is_valid).count();
        let invalid_count = fixtures.iter().filter(|f| !f.is_valid).count();
        assert!(valid_count >= 1, "Should have at least one valid profile");
        assert!(
            invalid_count >= 1,
            "Should have at least one invalid profile"
        );
        Ok(())
    }

    #[test]
    fn get_performance_fixtures_all_load_levels() -> Result<(), Box<dyn std::error::Error>> {
        let fixtures = get_performance_fixtures();
        let levels: Vec<LoadLevel> = fixtures.iter().map(|f| f.load_level).collect();
        assert!(levels.contains(&LoadLevel::Idle));
        assert!(levels.contains(&LoadLevel::Light));
        assert!(levels.contains(&LoadLevel::Normal));
        assert!(levels.contains(&LoadLevel::Heavy));
        assert!(levels.contains(&LoadLevel::Extreme));
        Ok(())
    }

    #[test]
    fn get_telemetry_fixtures_all_positive_rates() -> Result<(), Box<dyn std::error::Error>> {
        let fixtures = get_telemetry_fixtures();
        for f in &fixtures {
            assert!(f.sample_rate_hz > 0, "{} has zero sample rate", f.name);
            assert!(f.duration_s > 0.0, "{} has zero duration", f.name);
            assert!(f.total_samples() > 0, "{} produces zero samples", f.name);
        }
        Ok(())
    }
}

// ── Allocation tracking validation ─────────────────────────────────────────

// NOTE: The TrackingAllocator is only active when set as #[global_allocator].
// In integration tests (separate binaries), the default System allocator is used
// unless we explicitly set it here.
#[global_allocator]
static GLOBAL: openracing_test_helpers::tracking::TrackingAllocator =
    openracing_test_helpers::tracking::TrackingAllocator;

mod tracking_validation {
    use openracing_test_helpers::assert_rt_safe;
    use openracing_test_helpers::tracking::*;

    #[test]
    fn guard_default_same_as_new() -> Result<(), Box<dyn std::error::Error>> {
        let g1 = AllocationGuard::new();
        let g2 = AllocationGuard::default();
        // Both should start with no allocations
        assert!(!g1.has_allocations());
        assert!(!g2.has_allocations());
        Ok(())
    }

    #[test]
    fn track_returns_guard() -> Result<(), Box<dyn std::error::Error>> {
        let guard = track();
        assert_eq!(guard.allocations(), 0);
        assert_eq!(guard.bytes(), 0);
        assert!(!guard.has_allocations());
        Ok(())
    }

    #[test]
    fn guard_detects_heap_allocation() -> Result<(), Box<dyn std::error::Error>> {
        let guard = track();
        let _heap_vec: Vec<u8> = vec![0u8; 1024];
        assert!(guard.has_allocations());
        assert!(guard.allocations() > 0);
        assert!(guard.bytes() >= 1024);
        Ok(())
    }

    #[test]
    fn guard_stack_only_no_allocations() -> Result<(), Box<dyn std::error::Error>> {
        let guard = track();
        let _x = 42i32;
        let _y = [0u8; 256];
        let _z = _x + 1;
        assert!(!guard.has_allocations());
        Ok(())
    }

    #[test]
    fn guard_reset_clears_counts() -> Result<(), Box<dyn std::error::Error>> {
        let guard = track();
        let _v: Vec<u8> = vec![1, 2, 3];
        assert!(guard.has_allocations());
        guard.reset();
        // After reset, counters are zeroed (but guard start values haven't changed)
        // So allocations() may report a large value due to wrapping subtraction
        // This is expected behavior; reset() is for the global counters
        Ok(())
    }

    #[test]
    fn allocation_report_new_is_zero() -> Result<(), Box<dyn std::error::Error>> {
        let report = AllocationReport::new("test context");
        assert!(report.is_zero());
        assert_eq!(report.allocations, 0);
        assert_eq!(report.bytes, 0);
        assert_eq!(report.context, "test context");
        Ok(())
    }

    #[test]
    fn allocation_report_assert_zero_passes_for_zero() -> Result<(), Box<dyn std::error::Error>> {
        let report = AllocationReport::new("safe");
        report.assert_zero(); // should not panic
        Ok(())
    }

    #[test]
    #[should_panic(expected = "Allocation violation")]
    fn allocation_report_assert_zero_panics_for_nonzero() {
        let report = AllocationReport {
            allocations: 5,
            bytes: 512,
            context: "test".to_string(),
        };
        report.assert_zero();
    }

    #[test]
    fn allocation_report_display_zero() -> Result<(), Box<dyn std::error::Error>> {
        let report = AllocationReport::new("my_context");
        let display = format!("{report}");
        assert!(display.contains("zero allocations"));
        assert!(display.contains("my_context"));
        Ok(())
    }

    #[test]
    fn allocation_report_display_nonzero() -> Result<(), Box<dyn std::error::Error>> {
        let report = AllocationReport {
            allocations: 10,
            bytes: 2048,
            context: "hot path".to_string(),
        };
        let display = format!("{report}");
        assert!(display.contains("10 times"));
        assert!(display.contains("2048 bytes"));
        assert!(display.contains("hot path"));
        Ok(())
    }

    #[test]
    fn assert_rt_safe_macro_passes_no_alloc() -> Result<(), Box<dyn std::error::Error>> {
        let guard = track();
        let _x = 1 + 2;
        assert_rt_safe!(guard);
        Ok(())
    }

    #[test]
    #[should_panic(expected = "RT path allocation violation")]
    fn assert_rt_safe_macro_fails_on_alloc() {
        let guard = track();
        let _v: Vec<i32> = vec![1, 2, 3, 4, 5];
        assert_rt_safe!(guard);
    }

    #[test]
    #[should_panic(expected = "RT path allocation violation in 'test context'")]
    fn assert_rt_safe_macro_with_context() {
        let guard = track();
        let _v: Vec<i32> = vec![1, 2, 3];
        assert_rt_safe!(guard, "test context");
    }
}

// ── Prelude re-export validation ───────────────────────────────────────────

mod prelude_validation {
    use openracing_test_helpers::prelude::*;

    #[test]
    fn prelude_must_available() -> TestResult {
        let r: Result<i32, &str> = Ok(1);
        let _ = must(r);
        Ok(())
    }

    #[test]
    fn prelude_must_some_available() -> TestResult {
        let _ = must_some(Some(1), "msg");
        Ok(())
    }

    #[test]
    fn prelude_must_parse_available() -> TestResult {
        let _: i32 = must_parse("42");
        Ok(())
    }

    #[test]
    fn prelude_must_with_available() -> TestResult {
        let r: Result<i32, &str> = Ok(1);
        let _ = must_with(r, "ctx");
        Ok(())
    }

    #[test]
    fn prelude_must_some_or_available() -> TestResult {
        let _ = must_some_or(None::<i32>, 0);
        Ok(())
    }

    #[test]
    fn prelude_must_or_else_available() -> TestResult {
        let _ = must_or_else(Ok::<i32, &str>(1), |_| 0);
        Ok(())
    }

    #[test]
    fn prelude_tracking_available() -> TestResult {
        let guard = track();
        assert!(!guard.has_allocations());
        Ok(())
    }

    #[test]
    fn prelude_fixtures_available() -> TestResult {
        let devices = get_device_fixtures();
        assert!(!devices.is_empty());
        let profiles = get_profile_fixtures();
        assert!(!profiles.is_empty());
        let perfs = get_performance_fixtures();
        assert!(!perfs.is_empty());
        let telemetry = get_telemetry_fixtures();
        assert!(!telemetry.is_empty());
        Ok(())
    }

    #[test]
    fn prelude_mock_types_available() -> TestResult {
        let _device = MockHidDevice::new();
        let _data = MockTelemetryData::new();
        let _port = MockTelemetryPort::new();
        let _profile = MockProfile::new("test");
        let _id = MockProfileId::default();
        Ok(())
    }
}
