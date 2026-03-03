//! Integration tests for openracing-test-helpers.
//!
//! Covers: assertion macros, must helpers, mock device creation,
//! fixture generation, telemetry data, and allocation tracking API.

// ── Assertion macro tests ──────────────────────────────────────────────────

mod assertion_macros {
    use openracing_test_helpers::{
        assert_approx_eq, assert_contains, assert_empty, assert_err, assert_in_range,
        assert_monotonic, assert_monotonic_desc, assert_none, assert_not_empty, assert_ok,
        assert_some, assert_sorted, assert_sorted_desc,
    };

    // ── assert_approx_eq ────────────────────────────────────────────────

    #[test]
    fn approx_eq_f64_within_tolerance() {
        assert_approx_eq!(1.0_f64, 1.0001_f64, 0.001_f64);
    }

    #[test]
    fn approx_eq_f32_within_tolerance() {
        assert_approx_eq!(2.5_f32, 2.501_f32, 0.01_f32);
    }

    #[test]
    fn approx_eq_exact_values() {
        assert_approx_eq!(42.0_f64, 42.0_f64, 0.0_f64);
    }

    #[test]
    fn approx_eq_negative_values() {
        assert_approx_eq!(-1.0_f64, -1.0005_f64, 0.001_f64);
    }

    #[test]
    fn approx_eq_with_custom_message() {
        assert_approx_eq!(1.0_f64, 1.0001_f64, 0.001_f64, "values should be close");
    }

    #[test]
    #[should_panic(expected = "assertion failed")]
    fn approx_eq_fails_outside_tolerance() {
        assert_approx_eq!(1.0_f64, 2.0_f64, 0.001_f64);
    }

    // ── assert_sorted ───────────────────────────────────────────────────

    #[test]
    fn sorted_ascending_integers() {
        assert_sorted!(&[1, 2, 3, 4, 5]);
    }

    #[test]
    fn sorted_with_duplicates() {
        assert_sorted!(&[1, 1, 2, 2, 3]);
    }

    #[test]
    fn sorted_empty_collection() {
        assert_sorted!(&[] as &[i32]);
    }

    #[test]
    fn sorted_single_element() {
        assert_sorted!(&[42]);
    }

    #[test]
    fn sorted_strings() {
        assert_sorted!(&["a", "b", "c"]);
    }

    #[test]
    fn sorted_with_custom_message() {
        assert_sorted!(&[1, 2, 3], "should be sorted");
    }

    #[test]
    #[should_panic(expected = "not sorted")]
    fn sorted_fails_for_unsorted() {
        assert_sorted!(&[3, 1, 2]);
    }

    // ── assert_sorted_desc ──────────────────────────────────────────────

    #[test]
    fn sorted_desc_integers() {
        assert_sorted_desc!(&[5, 4, 3, 2, 1]);
    }

    #[test]
    fn sorted_desc_with_duplicates() {
        assert_sorted_desc!(&[5, 5, 3, 3, 1]);
    }

    #[test]
    fn sorted_desc_empty() {
        assert_sorted_desc!(&[] as &[i32]);
    }

    #[test]
    #[should_panic(expected = "not sorted in descending")]
    fn sorted_desc_fails_for_ascending() {
        assert_sorted_desc!(&[1, 2, 3]);
    }

    // ── assert_monotonic ────────────────────────────────────────────────

    #[test]
    fn monotonic_strictly_increasing() {
        assert_monotonic!(&[1, 2, 3, 4, 5]);
    }

    #[test]
    fn monotonic_empty_and_single() {
        assert_monotonic!(&[] as &[i32]);
        assert_monotonic!(&[1]);
    }

    #[test]
    fn monotonic_with_custom_message() {
        assert_monotonic!(&[10, 20, 30], "timestamps should increase");
    }

    #[test]
    #[should_panic(expected = "not strictly monotonic")]
    fn monotonic_fails_with_duplicates() {
        assert_monotonic!(&[1, 2, 2, 3]);
    }

    #[test]
    #[should_panic(expected = "not strictly monotonic")]
    fn monotonic_fails_with_decrease() {
        assert_monotonic!(&[1, 3, 2]);
    }

    // ── assert_monotonic_desc ───────────────────────────────────────────

    #[test]
    fn monotonic_desc_strictly_decreasing() {
        assert_monotonic_desc!(&[5, 4, 3, 2, 1]);
    }

    #[test]
    fn monotonic_desc_empty_and_single() {
        assert_monotonic_desc!(&[] as &[i32]);
        assert_monotonic_desc!(&[99]);
    }

    #[test]
    #[should_panic(expected = "not strictly monotonic decreasing")]
    fn monotonic_desc_fails_with_duplicates() {
        assert_monotonic_desc!(&[5, 4, 4, 3]);
    }

    // ── assert_contains ─────────────────────────────────────────────────

    #[test]
    fn contains_substring() {
        assert_contains!("hello world", "world");
    }

    #[test]
    fn contains_full_string() {
        assert_contains!("exact", "exact");
    }

    #[test]
    fn contains_empty_needle() {
        assert_contains!("anything", "");
    }

    #[test]
    fn contains_with_custom_message() {
        assert_contains!("hello", "hell", "prefix check");
    }

    #[test]
    #[should_panic(expected = "does not contain")]
    fn contains_fails_when_missing() {
        assert_contains!("hello", "xyz");
    }

    // ── assert_ok / assert_err ──────────────────────────────────────────

    #[test]
    fn assert_ok_extracts_value() {
        let result: Result<i32, &str> = Ok(42);
        let val = assert_ok!(result);
        assert_eq!(val, 42);
    }

    #[test]
    fn assert_ok_with_message() {
        let result: Result<i32, &str> = Ok(99);
        let val = assert_ok!(result, "should be ok");
        assert_eq!(val, 99);
    }

    #[test]
    #[should_panic(expected = "expected Ok")]
    fn assert_ok_fails_on_err() {
        let result: Result<i32, &str> = Err("fail");
        assert_ok!(result);
    }

    #[test]
    fn assert_err_extracts_error() {
        let result: Result<i32, &str> = Err("oops");
        let e = assert_err!(result);
        assert_eq!(e, "oops");
    }

    #[test]
    fn assert_err_with_message() {
        let result: Result<i32, String> = Err("bad".to_string());
        let e = assert_err!(result, "should be err");
        assert_eq!(e, "bad");
    }

    #[test]
    #[should_panic(expected = "expected Err")]
    fn assert_err_fails_on_ok() {
        let result: Result<i32, &str> = Ok(1);
        assert_err!(result);
    }

    // ── assert_some / assert_none ───────────────────────────────────────

    #[test]
    fn assert_some_extracts_value() {
        let val = assert_some!(Some(42));
        assert_eq!(val, 42);
    }

    #[test]
    fn assert_some_with_message() {
        let val = assert_some!(Some("hello"), "should have value");
        assert_eq!(val, "hello");
    }

    #[test]
    #[should_panic(expected = "expected Some")]
    fn assert_some_fails_on_none() {
        assert_some!(None::<i32>);
    }

    #[test]
    fn assert_none_passes_on_none() {
        assert_none!(None::<i32>);
    }

    #[test]
    fn assert_none_with_message() {
        assert_none!(None::<i32>, "should be none");
    }

    #[test]
    #[should_panic(expected = "expected None")]
    fn assert_none_fails_on_some() {
        assert_none!(Some(42));
    }

    // ── assert_empty / assert_not_empty ─────────────────────────────────

    #[test]
    fn assert_empty_vec() {
        assert_empty!(&Vec::<i32>::new());
    }

    #[test]
    fn assert_empty_string() {
        assert_empty!(&String::new());
    }

    #[test]
    fn assert_empty_slice() {
        assert_empty!(&[] as &[i32]);
    }

    #[test]
    fn assert_empty_with_message() {
        assert_empty!(&Vec::<i32>::new(), "should be empty");
    }

    #[test]
    #[should_panic(expected = "not empty")]
    fn assert_empty_fails_on_nonempty() {
        assert_empty!(&[1, 2]);
    }

    #[test]
    fn assert_not_empty_vec() {
        assert_not_empty!(&[1, 2, 3]);
    }

    #[test]
    fn assert_not_empty_string() {
        assert_not_empty!(&"hello".to_string());
    }

    #[test]
    fn assert_not_empty_with_message() {
        assert_not_empty!(&[1], "should have items");
    }

    #[test]
    #[should_panic(expected = "collection is empty")]
    fn assert_not_empty_fails_on_empty() {
        assert_not_empty!(&[] as &[i32]);
    }

    // ── assert_in_range ─────────────────────────────────────────────────

    #[test]
    fn in_range_inclusive() {
        assert_in_range!(5, 0..=10);
        assert_in_range!(0, 0..=10);
        assert_in_range!(10, 0..=10);
    }

    #[test]
    fn in_range_exclusive_end() {
        assert_in_range!(0, 0..10);
        assert_in_range!(9, 0..10);
    }

    #[test]
    fn in_range_with_message() {
        assert_in_range!(5, 0..=10, "value in bounds");
    }

    #[test]
    #[should_panic(expected = "not in range")]
    fn in_range_fails_outside() {
        assert_in_range!(11, 0..=10);
    }

    #[test]
    fn in_range_f64_values() {
        assert_in_range!(0.5_f64, 0.0_f64..=1.0_f64);
    }
}

// ── Must helpers ───────────────────────────────────────────────────────────

mod must_helpers {
    use openracing_test_helpers::{
        must, must_or_else, must_parse, must_some, must_some_or, must_with,
    };

    #[test]
    fn must_unwraps_ok() {
        let result: Result<i32, &str> = Ok(42);
        assert_eq!(must(result), 42);
    }

    #[test]
    #[should_panic(expected = "must: unexpected Err")]
    fn must_panics_on_err() {
        let result: Result<i32, &str> = Err("boom");
        let _ = must(result);
    }

    #[test]
    fn must_some_unwraps_some() {
        assert_eq!(must_some(Some(99), "expected value"), 99);
    }

    #[test]
    #[should_panic(expected = "must_some: missing")]
    fn must_some_panics_on_none() {
        let _ = must_some(None::<i32>, "missing");
    }

    #[test]
    fn must_parse_integer() {
        let val: i32 = must_parse("123");
        assert_eq!(val, 123);
    }

    #[test]
    fn must_parse_float() {
        let val: f64 = must_parse("1.5");
        assert!((val - 1.5_f64).abs() < 1e-10);
    }

    #[test]
    fn must_parse_bool() {
        let val: bool = must_parse("true");
        assert!(val);
    }

    #[test]
    #[should_panic(expected = "must_parse: failed to parse")]
    fn must_parse_fails_on_invalid() {
        let _: i32 = must_parse("abc");
    }

    #[test]
    fn must_with_unwraps_ok_with_context() {
        let result: Result<String, &str> = Ok("hello".to_string());
        assert_eq!(must_with(result, "loading config"), "hello");
    }

    #[test]
    #[should_panic(expected = "must_with: loading config")]
    fn must_with_includes_context_in_panic() {
        let result: Result<i32, &str> = Err("not found");
        let _ = must_with(result, "loading config");
    }

    #[test]
    fn must_some_or_returns_value_when_some() {
        assert_eq!(must_some_or(Some(42), 0), 42);
    }

    #[test]
    fn must_some_or_returns_default_when_none() {
        assert_eq!(must_some_or(None::<i32>, -1), -1);
    }

    #[test]
    fn must_or_else_returns_ok_value() {
        let result: Result<i32, &str> = Ok(42);
        assert_eq!(must_or_else(result, |_| 0), 42);
    }

    #[test]
    fn must_or_else_computes_from_error() {
        let result: Result<usize, &str> = Err("hello");
        assert_eq!(must_or_else(result, |e| e.len()), 5);
    }
}

// ── Mock device creation ───────────────────────────────────────────────────

#[cfg(feature = "mock")]
mod mock_device {
    use openracing_test_helpers::mock::{MockDeviceWriter, MockHidDevice};

    #[test]
    fn new_device_has_no_reports() {
        let device = MockHidDevice::new();
        assert!(device.feature_reports().is_empty());
        assert!(device.output_reports().is_empty());
        assert_eq!(device.total_writes(), 0);
    }

    #[test]
    fn default_device_is_same_as_new() {
        let device = MockHidDevice::default();
        assert_eq!(device.total_writes(), 0);
        assert!(!device.fail_on_write);
    }

    #[test]
    fn write_feature_report_records_data() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::new();
        let data = [0x01, 0x02, 0x03];
        let written = device.write_feature_report(&data)?;
        assert_eq!(written, 3);
        assert_eq!(device.feature_reports().len(), 1);
        assert_eq!(device.feature_reports()[0], vec![0x01, 0x02, 0x03]);
        Ok(())
    }

    #[test]
    fn write_output_report_records_data() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::new();
        let data = [0xAA, 0xBB];
        let written = device.write_output_report(&data)?;
        assert_eq!(written, 2);
        assert_eq!(device.output_reports().len(), 1);
        assert_eq!(device.output_reports()[0], vec![0xAA, 0xBB]);
        Ok(())
    }

    #[test]
    fn multiple_writes_accumulate() -> Result<(), Box<dyn std::error::Error>> {
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
    fn last_report_accessors() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::new();
        assert!(device.last_feature_report().is_none());
        assert!(device.last_output_report().is_none());

        device.write_feature_report(&[1, 2])?;
        device.write_feature_report(&[3, 4])?;
        device.write_output_report(&[5, 6])?;

        assert_eq!(device.last_feature_report(), Some(vec![3, 4]));
        assert_eq!(device.last_output_report(), Some(vec![5, 6]));
        Ok(())
    }

    #[test]
    fn clear_removes_all_reports() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::new();
        device.write_feature_report(&[1])?;
        device.write_output_report(&[2])?;
        assert_eq!(device.total_writes(), 2);

        device.clear();
        assert_eq!(device.total_writes(), 0);
        assert!(device.feature_reports().is_empty());
        assert!(device.output_reports().is_empty());
        Ok(())
    }

    #[test]
    fn fail_on_write_mode() {
        let mut device = MockHidDevice::with_failure();
        assert!(device.fail_on_write);
        assert!(device.write_feature_report(&[1]).is_err());
        assert!(device.write_output_report(&[2]).is_err());
        assert_eq!(device.total_writes(), 0);
    }

    #[test]
    fn with_delay_configures_delay() {
        let device = MockHidDevice::with_delay(50);
        assert_eq!(device.write_delay_ms, 50);
        assert!(!device.fail_on_write);
    }

    #[test]
    fn empty_data_write() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::new();
        let written = device.write_feature_report(&[])?;
        assert_eq!(written, 0);
        assert_eq!(device.feature_reports().len(), 1);
        assert!(device.feature_reports()[0].is_empty());
        Ok(())
    }
}

// ── Mock telemetry data ────────────────────────────────────────────────────

#[cfg(feature = "mock")]
mod mock_telemetry {
    use openracing_test_helpers::mock::{MockTelemetryData, MockTelemetryPort};

    #[test]
    fn default_telemetry_data_is_zeroed() {
        let data = MockTelemetryData::default();
        assert_eq!(data.rpm, 0.0);
        assert_eq!(data.speed_ms, 0.0);
        assert_eq!(data.ffb_scalar, 0.0);
        assert_eq!(data.slip_ratio, 0.0);
        assert_eq!(data.gear, 0);
        assert_eq!(data.timestamp_ms, 0);
    }

    #[test]
    fn telemetry_builder_chain() {
        let data = MockTelemetryData::new()
            .with_rpm(6000.0)
            .with_speed(90.0)
            .with_ffb(0.8)
            .with_gear(4)
            .with_timestamp(5000);

        assert_eq!(data.rpm, 6000.0);
        assert_eq!(data.speed_ms, 90.0);
        assert_eq!(data.ffb_scalar, 0.8);
        assert_eq!(data.gear, 4);
        assert_eq!(data.timestamp_ms, 5000);
    }

    #[test]
    fn racing_sample_produces_valid_ranges() {
        for i in 0..100 {
            let progress = i as f32 / 100.0;
            let sample = MockTelemetryData::racing_sample(progress);
            // RPM should be in a reasonable range (base 4000 ± 2000)
            assert!(
                sample.rpm >= 2000.0 && sample.rpm <= 6000.0,
                "rpm out of range at progress {progress}: {}",
                sample.rpm
            );
            // Speed should be positive
            assert!(
                sample.speed_ms >= 0.0,
                "speed negative at progress {progress}: {}",
                sample.speed_ms
            );
            // FFB should be bounded
            assert!(
                sample.ffb_scalar >= -1.0 && sample.ffb_scalar <= 1.0,
                "ffb out of range at progress {progress}: {}",
                sample.ffb_scalar
            );
            // Slip ratio should be non-negative and clamped to 1.0
            assert!(
                sample.slip_ratio >= 0.0 && sample.slip_ratio <= 1.0,
                "slip_ratio out of range at progress {progress}: {}",
                sample.slip_ratio
            );
            // Gear should be positive
            assert!(
                sample.gear >= 1,
                "gear should be >= 1 at progress {progress}: {}",
                sample.gear
            );
        }
    }

    #[test]
    fn racing_samples_at_different_progress_differ() {
        let s1 = MockTelemetryData::racing_sample(0.0);
        let s2 = MockTelemetryData::racing_sample(0.25);
        let s3 = MockTelemetryData::racing_sample(0.5);
        // At least speed should differ since it's linear
        assert_ne!(s1.speed_ms, s2.speed_ms);
        assert_ne!(s2.speed_ms, s3.speed_ms);
    }

    #[test]
    fn telemetry_data_clone_and_debug() {
        let data = MockTelemetryData::new().with_rpm(5000.0);
        let cloned = data.clone();
        assert_eq!(data.rpm, cloned.rpm);
        let debug = format!("{data:?}");
        assert!(debug.contains("5000"));
    }

    // ── MockTelemetryPort ───────────────────────────────────────────────

    #[test]
    fn empty_port_returns_none() {
        let port = MockTelemetryPort::new();
        assert!(port.is_empty());
        assert_eq!(port.len(), 0);
        assert!(port.next().is_none());
    }

    #[test]
    fn port_iterates_in_order() {
        let mut port = MockTelemetryPort::new();
        port.add(MockTelemetryData::new().with_rpm(1000.0));
        port.add(MockTelemetryData::new().with_rpm(2000.0));
        port.add(MockTelemetryData::new().with_rpm(3000.0));

        assert_eq!(port.len(), 3);
        assert!(!port.is_empty());

        let d1 = port.next();
        let d2 = port.next();
        let d3 = port.next();
        let d4 = port.next();

        assert!(d1.is_some());
        assert!(d2.is_some());
        assert!(d3.is_some());
        assert!(d4.is_none());

        // Safe access after checking is_some
        if let (Some(v1), Some(v2), Some(v3)) = (d1, d2, d3) {
            assert_eq!(v1.rpm, 1000.0);
            assert_eq!(v2.rpm, 2000.0);
            assert_eq!(v3.rpm, 3000.0);
        }
    }

    #[test]
    fn port_reset_restarts_iteration() {
        let port = MockTelemetryPort::with_data(vec![
            MockTelemetryData::new().with_rpm(100.0),
            MockTelemetryData::new().with_rpm(200.0),
        ]);

        let first_pass = port.next();
        assert!(first_pass.is_some());

        port.reset();

        let after_reset = port.next();
        assert!(after_reset.is_some());
        if let (Some(f), Some(r)) = (first_pass, after_reset) {
            assert_eq!(f.rpm, r.rpm);
        }
    }

    #[test]
    fn generate_racing_sequence_correct_count() {
        let port = MockTelemetryPort::generate_racing_sequence(2.0, 60);
        assert_eq!(port.len(), 120);
        assert!(!port.is_empty());
    }

    #[test]
    fn generate_racing_sequence_all_valid() {
        let port = MockTelemetryPort::generate_racing_sequence(1.0, 100);
        for _ in 0..port.len() {
            let sample = port.next();
            assert!(sample.is_some());
        }
        // Should be exhausted
        assert!(port.next().is_none());
    }

    #[test]
    fn with_data_constructor() {
        let data = vec![
            MockTelemetryData::new().with_rpm(500.0),
            MockTelemetryData::new().with_rpm(600.0),
        ];
        let port = MockTelemetryPort::with_data(data);
        assert_eq!(port.len(), 2);
    }

    #[test]
    fn default_port_is_empty() {
        let port = MockTelemetryPort::default();
        assert!(port.is_empty());
    }
}

// ── Mock profile ───────────────────────────────────────────────────────────

#[cfg(feature = "mock")]
mod mock_profile {
    use openracing_test_helpers::mock::{MockProfile, MockProfileId};

    #[test]
    fn profile_id_creation() {
        let id = MockProfileId::new("test-profile");
        assert_eq!(id.0, "test-profile");
    }

    #[test]
    fn profile_id_default() {
        let id = MockProfileId::default();
        assert_eq!(id.0, "default");
    }

    #[test]
    fn profile_id_equality() {
        let id1 = MockProfileId::new("a");
        let id2 = MockProfileId::new("a");
        let id3 = MockProfileId::new("b");
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn profile_id_hash_consistency() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(MockProfileId::new("profile1"));
        set.insert(MockProfileId::new("profile2"));
        set.insert(MockProfileId::new("profile1")); // duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn profile_default_values() {
        let profile = MockProfile::new("test");
        assert_eq!(profile.id.0, "test");
        assert_eq!(profile.name, "Default Profile");
        assert_eq!(profile.game, "default");
        assert!(profile.car.is_none());
        assert_eq!(profile.ffb_gain, 1.0);
        assert_eq!(profile.dor_deg, 900);
        assert_eq!(profile.torque_cap_nm, 10.0);
    }

    #[test]
    fn profile_full_builder() {
        let profile = MockProfile::new("iracing-gt3")
            .with_name("iRacing GT3")
            .with_game("iracing")
            .with_car("ferrari_488_gt3")
            .with_ffb_gain(0.75)
            .with_dor(540)
            .with_torque_cap(25.0);

        assert_eq!(profile.id.0, "iracing-gt3");
        assert_eq!(profile.name, "iRacing GT3");
        assert_eq!(profile.game, "iracing");
        assert_eq!(profile.car, Some("ferrari_488_gt3".to_string()));
        assert_eq!(profile.ffb_gain, 0.75);
        assert_eq!(profile.dor_deg, 540);
        assert_eq!(profile.torque_cap_nm, 25.0);
    }

    #[test]
    fn profile_clone_preserves_all_fields() {
        let profile = MockProfile::new("orig")
            .with_name("Original")
            .with_car("gt4");
        let cloned = profile.clone();
        assert_eq!(cloned.id.0, profile.id.0);
        assert_eq!(cloned.name, profile.name);
        assert_eq!(cloned.car, profile.car);
    }

    #[test]
    fn profile_debug_format() {
        let profile = MockProfile::new("debug-test");
        let debug = format!("{profile:?}");
        assert!(debug.contains("debug-test"));
    }
}

// ── Test fixture generation ────────────────────────────────────────────────

#[cfg(feature = "fixtures")]
mod fixture_generation {
    use openracing_test_helpers::fixtures::{
        DeviceCapabilitiesFixture, LoadLevel, PerformanceFixture, ProfileFixture, TelemetryFixture,
        get_device_fixtures, get_performance_fixtures, get_profile_fixtures,
        get_telemetry_fixtures,
    };
    use std::time::Duration;

    // ── DeviceCapabilitiesFixture ───────────────────────────────────────

    #[test]
    fn basic_wheel_fixture_values() {
        let basic = DeviceCapabilitiesFixture::basic_wheel();
        assert!(basic.supports_pid);
        assert!(!basic.supports_raw_torque_1khz);
        assert!(!basic.supports_health_stream);
        assert!(!basic.supports_led_bus);
        assert_eq!(basic.max_torque_nm, 8.0);
        assert_eq!(basic.encoder_cpr, 4096);
        assert_eq!(basic.min_report_period_us, 2000);
    }

    #[test]
    fn dd_wheel_fixture_values() {
        let dd = DeviceCapabilitiesFixture::dd_wheel();
        assert!(dd.supports_raw_torque_1khz);
        assert!(dd.supports_health_stream);
        assert!(dd.supports_led_bus);
        assert_eq!(dd.max_torque_nm, 25.0);
        assert_eq!(dd.encoder_cpr, 65535);
        assert_eq!(dd.min_report_period_us, 1000);
    }

    #[test]
    fn high_end_dd_fixture_values() {
        let high = DeviceCapabilitiesFixture::high_end_dd();
        assert_eq!(high.max_torque_nm, 50.0);
        assert_eq!(high.min_report_period_us, 500);
    }

    #[test]
    fn device_default_is_basic_wheel() {
        let def = DeviceCapabilitiesFixture::default();
        let basic = DeviceCapabilitiesFixture::basic_wheel();
        assert_eq!(def.max_torque_nm, basic.max_torque_nm);
        assert_eq!(def.encoder_cpr, basic.encoder_cpr);
    }

    #[test]
    fn device_new_is_default() {
        let new = DeviceCapabilitiesFixture::new();
        let def = DeviceCapabilitiesFixture::default();
        assert_eq!(new.max_torque_nm, def.max_torque_nm);
    }

    #[test]
    fn device_builder_with_max_torque() {
        let custom = DeviceCapabilitiesFixture::dd_wheel().with_max_torque(35.0);
        assert_eq!(custom.max_torque_nm, 35.0);
        // Other fields preserved
        assert!(custom.supports_raw_torque_1khz);
    }

    #[test]
    fn device_builder_with_encoder_cpr() {
        let custom = DeviceCapabilitiesFixture::basic_wheel().with_encoder_cpr(8192);
        assert_eq!(custom.encoder_cpr, 8192);
    }

    #[test]
    fn device_builder_with_raw_torque() {
        let custom = DeviceCapabilitiesFixture::basic_wheel().with_raw_torque(true);
        assert!(custom.supports_raw_torque_1khz);
    }

    // ── PerformanceFixture ──────────────────────────────────────────────

    #[test]
    fn performance_fixture_load_levels() {
        let idle = PerformanceFixture::idle();
        assert_eq!(idle.load_level, LoadLevel::Idle);

        let light = PerformanceFixture::light_load();
        assert_eq!(light.load_level, LoadLevel::Light);

        let normal = PerformanceFixture::normal_load();
        assert_eq!(normal.load_level, LoadLevel::Normal);

        let heavy = PerformanceFixture::heavy_load();
        assert_eq!(heavy.load_level, LoadLevel::Heavy);

        let extreme = PerformanceFixture::extreme_load();
        assert_eq!(extreme.load_level, LoadLevel::Extreme);
    }

    #[test]
    fn performance_fixture_durations_increase_with_load() {
        let idle = PerformanceFixture::idle();
        let light = PerformanceFixture::light_load();
        let normal = PerformanceFixture::normal_load();
        let heavy = PerformanceFixture::heavy_load();
        let extreme = PerformanceFixture::extreme_load();

        assert!(idle.duration < light.duration);
        assert!(light.duration < normal.duration);
        assert!(normal.duration < heavy.duration);
        assert!(heavy.duration < extreme.duration);
    }

    #[test]
    fn performance_fixture_jitter_increases_with_load() {
        let idle = PerformanceFixture::idle();
        let extreme = PerformanceFixture::extreme_load();
        assert!(idle.expected_jitter_p99_ms < extreme.expected_jitter_p99_ms);
    }

    #[test]
    fn performance_default_is_normal_load() {
        let def = PerformanceFixture::default();
        assert_eq!(def.load_level, LoadLevel::Normal);
    }

    #[test]
    fn performance_builder_with_duration() {
        let custom = PerformanceFixture::idle().with_duration(Duration::from_secs(10));
        assert_eq!(custom.duration, Duration::from_secs(10));
        assert_eq!(custom.load_level, LoadLevel::Idle);
    }

    #[test]
    fn performance_builder_with_load_level() {
        let custom = PerformanceFixture::idle().with_load_level(LoadLevel::Heavy);
        assert_eq!(custom.load_level, LoadLevel::Heavy);
    }

    // ── ProfileFixture ──────────────────────────────────────────────────

    #[test]
    fn valid_profile_fixture() {
        let profile = ProfileFixture::valid();
        assert!(profile.is_valid);
        assert!(profile.expected_errors.is_empty());
        assert_eq!(profile.ffb_gain, 0.68);
        assert_eq!(profile.dor_deg, 540);
    }

    #[test]
    fn invalid_gain_profile_fixture() {
        let profile = ProfileFixture::invalid_gain();
        assert!(!profile.is_valid);
        assert!(!profile.expected_errors.is_empty());
        assert!(profile.ffb_gain > 1.0);
    }

    #[test]
    fn invalid_dor_profile_fixture() {
        let profile = ProfileFixture::invalid_dor();
        assert!(!profile.is_valid);
        assert_eq!(profile.dor_deg, 0);
    }

    #[test]
    fn invalid_torque_cap_profile_fixture() {
        let profile = ProfileFixture::invalid_torque_cap();
        assert!(!profile.is_valid);
        assert_eq!(profile.torque_cap_nm, 0.0);
    }

    #[test]
    fn profile_default_is_valid() {
        let def = ProfileFixture::default();
        assert!(def.is_valid);
    }

    #[test]
    fn profile_builder_chain() {
        let profile = ProfileFixture::valid()
            .with_game("acc")
            .with_car("porsche_992_gt3")
            .with_ffb_gain(0.9)
            .with_dor(720);
        assert_eq!(profile.game, "acc");
        assert_eq!(profile.car, Some("porsche_992_gt3".to_string()));
        assert_eq!(profile.ffb_gain, 0.9);
        assert_eq!(profile.dor_deg, 720);
    }

    // ── TelemetryFixture ────────────────────────────────────────────────

    #[test]
    fn telemetry_basic_fixture() {
        let basic = TelemetryFixture::basic();
        assert_eq!(basic.sample_rate_hz, 60);
        assert_eq!(basic.duration_s, 10.0);
    }

    #[test]
    fn telemetry_racing_fixture() {
        let racing = TelemetryFixture::racing();
        assert!(racing.base_rpm > TelemetryFixture::basic().base_rpm);
        assert!(racing.base_speed_ms > TelemetryFixture::basic().base_speed_ms);
    }

    #[test]
    fn telemetry_high_performance_fixture() {
        let hp = TelemetryFixture::high_performance();
        assert_eq!(hp.sample_rate_hz, 200);
        assert!(hp.ffb_amplitude > TelemetryFixture::basic().ffb_amplitude);
    }

    #[test]
    fn telemetry_total_samples_calculation() {
        let t = TelemetryFixture::basic()
            .with_sample_rate(100)
            .with_duration(5.0);
        assert_eq!(t.total_samples(), 500);
    }

    #[test]
    fn telemetry_total_samples_default_basic() {
        let basic = TelemetryFixture::basic();
        // 60 Hz * 10 seconds = 600
        assert_eq!(basic.total_samples(), 600);
    }

    #[test]
    fn telemetry_default_is_basic() {
        let def = TelemetryFixture::default();
        let basic = TelemetryFixture::basic();
        assert_eq!(def.sample_rate_hz, basic.sample_rate_hz);
    }

    // ── Collection helpers ──────────────────────────────────────────────

    #[test]
    fn get_device_fixtures_returns_three() {
        assert_eq!(get_device_fixtures().len(), 3);
    }

    #[test]
    fn get_profile_fixtures_returns_four() {
        let profiles = get_profile_fixtures();
        assert_eq!(profiles.len(), 4);
        // At least one valid and one invalid
        assert!(profiles.iter().any(|p| p.is_valid));
        assert!(profiles.iter().any(|p| !p.is_valid));
    }

    #[test]
    fn get_performance_fixtures_returns_five() {
        let perf = get_performance_fixtures();
        assert_eq!(perf.len(), 5);
    }

    #[test]
    fn get_telemetry_fixtures_returns_three() {
        assert_eq!(get_telemetry_fixtures().len(), 3);
    }

    #[test]
    fn all_load_levels_represented() {
        let fixtures = get_performance_fixtures();
        let levels: Vec<LoadLevel> = fixtures.iter().map(|f| f.load_level).collect();
        assert!(levels.contains(&LoadLevel::Idle));
        assert!(levels.contains(&LoadLevel::Light));
        assert!(levels.contains(&LoadLevel::Normal));
        assert!(levels.contains(&LoadLevel::Heavy));
        assert!(levels.contains(&LoadLevel::Extreme));
    }
}

// ── Allocation tracking API ────────────────────────────────────────────────

#[cfg(feature = "tracking")]
mod tracking_api {
    use openracing_test_helpers::tracking::AllocationReport;

    #[test]
    fn allocation_report_new_is_zero() {
        let report = AllocationReport::new("test context");
        assert!(report.is_zero());
        assert_eq!(report.allocations, 0);
        assert_eq!(report.bytes, 0);
        assert_eq!(report.context, "test context");
    }

    #[test]
    fn allocation_report_assert_zero_passes_for_zero() {
        let report = AllocationReport::new("safe");
        report.assert_zero(); // should not panic
    }

    #[test]
    #[should_panic(expected = "Allocation violation")]
    fn allocation_report_assert_zero_panics_for_nonzero() {
        let report = AllocationReport {
            allocations: 5,
            bytes: 1024,
            context: "rt_path".to_string(),
        };
        report.assert_zero();
    }

    #[test]
    fn allocation_report_is_zero_check() {
        let zero = AllocationReport::new("zero");
        assert!(zero.is_zero());

        let nonzero = AllocationReport {
            allocations: 1,
            bytes: 8,
            context: "nonzero".to_string(),
        };
        assert!(!nonzero.is_zero());
    }

    #[test]
    fn allocation_report_display_zero() {
        let report = AllocationReport::new("idle path");
        let display = report.to_string();
        assert!(display.contains("zero allocations"), "got: {display}");
        assert!(display.contains("idle path"), "got: {display}");
    }

    #[test]
    fn allocation_report_display_nonzero() {
        let report = AllocationReport {
            allocations: 3,
            bytes: 256,
            context: "hot loop".to_string(),
        };
        let display = report.to_string();
        assert!(display.contains("3 times"), "got: {display}");
        assert!(display.contains("256 bytes"), "got: {display}");
        assert!(display.contains("hot loop"), "got: {display}");
    }

    #[test]
    fn allocation_report_assert_zero_returns_self() {
        let report = AllocationReport::new("chainable");
        let returned = report.assert_zero();
        assert_eq!(returned.context, "chainable");
    }
}

// ── Prelude re-exports ─────────────────────────────────────────────────────

mod prelude_exports {
    use openracing_test_helpers::prelude::*;

    #[test]
    fn test_result_type_alias_works() -> TestResult {
        let _x = 42;
        Ok(())
    }

    #[test]
    fn prelude_must_available() {
        let result: Result<i32, &str> = Ok(10);
        assert_eq!(must(result), 10);
    }

    #[test]
    fn prelude_must_some_available() {
        let option = Some(20);
        assert_eq!(must_some(option, "test"), 20);
    }

    #[test]
    fn prelude_must_parse_available() {
        let val: u32 = must_parse("42");
        assert_eq!(val, 42);
    }

    #[test]
    fn prelude_must_some_or_available() {
        assert_eq!(must_some_or(None::<i32>, 7), 7);
    }

    #[test]
    fn prelude_must_or_else_available() {
        let r: Result<i32, &str> = Err("e");
        assert_eq!(must_or_else(r, |_| 0), 0);
    }

    #[test]
    fn prelude_must_with_available() {
        let r: Result<i32, &str> = Ok(5);
        assert_eq!(must_with(r, "ctx"), 5);
    }
}

// ── Async must helpers ─────────────────────────────────────────────────────

#[cfg(feature = "mock")]
mod async_must {
    use openracing_test_helpers::must::{must_async, must_some_async};

    #[tokio::test]
    async fn must_async_unwraps_ok() {
        async fn get_value() -> Result<i32, String> {
            Ok(42)
        }
        let val = must_async(get_value()).await;
        assert_eq!(val, 42);
    }

    #[tokio::test]
    #[should_panic(expected = "must_async: unexpected Err")]
    async fn must_async_panics_on_err() {
        async fn fail() -> Result<i32, String> {
            Err("async error".to_string())
        }
        let _ = must_async(fail()).await;
    }

    #[tokio::test]
    async fn must_some_async_unwraps_some() {
        async fn get_opt() -> Option<String> {
            Some("hello".to_string())
        }
        let val = must_some_async(get_opt(), "expected value").await;
        assert_eq!(val, "hello");
    }

    #[tokio::test]
    #[should_panic(expected = "must_some_async: no value")]
    async fn must_some_async_panics_on_none() {
        async fn get_none() -> Option<i32> {
            None
        }
        let _ = must_some_async(get_none(), "no value").await;
    }
}
