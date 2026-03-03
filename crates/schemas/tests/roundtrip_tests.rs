//! Comprehensive schema roundtrip and serialization tests.
//!
//! Tests all serializable types in the schemas crate for:
//! 1. JSON serialize → deserialize roundtrip preserves all fields
//! 2. Default values are correct and stable
//! 3. Unknown field handling (forward compatibility)
//! 4. Empty/minimal payloads deserialize without panic
//! 5. Field rename attributes work correctly
//! 6. Enum variant serialization matches expected strings
//! 7. Optional field omission behavior
//! 8. Nested type composition roundtrip
//! 9. Large payload handling (many items in arrays)
//! 10. Cross-version compatibility (old format → new format migration)

#![deny(clippy::unwrap_used)]

use std::collections::HashMap;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ─── Helper: roundtrip through JSON and assert equality ───

fn json_roundtrip<T>(value: &T) -> Result<T, serde_json::Error>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let json = serde_json::to_string(value)?;
    serde_json::from_str(&json)
}

fn json_roundtrip_pretty<T>(value: &T) -> Result<T, serde_json::Error>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let json = serde_json::to_string_pretty(value)?;
    serde_json::from_str(&json)
}

// ═══════════════════════════════════════════════════════════
// 1. JSON serialize → deserialize roundtrip preserves all fields
// ═══════════════════════════════════════════════════════════

mod roundtrip {
    use super::*;
    use racing_wheel_schemas::domain::*;
    use racing_wheel_schemas::entities::*;
    use racing_wheel_schemas::telemetry::*;

    #[test]
    fn torque_nm_roundtrip() -> TestResult {
        let torque = TorqueNm::new(12.5)?;
        let rt = json_roundtrip(&torque)?;
        assert!((rt.value() - torque.value()).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn degrees_roundtrip() -> TestResult {
        let deg = Degrees::new_dor(900.0)?;
        let rt = json_roundtrip(&deg)?;
        assert!((rt.value() - deg.value()).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn device_id_roundtrip() -> TestResult {
        let id: DeviceId = "moza-r9".parse()?;
        let rt = json_roundtrip(&id)?;
        assert_eq!(rt.as_str(), id.as_str());
        Ok(())
    }

    #[test]
    fn profile_id_roundtrip() -> TestResult {
        let id: ProfileId = "iracing.gt3".parse()?;
        let rt = json_roundtrip(&id)?;
        assert_eq!(rt.as_str(), id.as_str());
        Ok(())
    }

    #[test]
    fn gain_roundtrip() -> TestResult {
        let gain = Gain::new(0.75)?;
        let rt = json_roundtrip(&gain)?;
        assert!((rt.value() - gain.value()).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn frequency_hz_roundtrip() -> TestResult {
        let freq = FrequencyHz::new(1000.0)?;
        let rt = json_roundtrip(&freq)?;
        assert!((rt.value() - freq.value()).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn curve_point_roundtrip() -> TestResult {
        let cp = CurvePoint::new(0.5, 0.7)?;
        let rt = json_roundtrip(&cp)?;
        assert!((rt.input - cp.input).abs() < f32::EPSILON);
        assert!((rt.output - cp.output).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn device_capabilities_roundtrip() -> TestResult {
        let caps = DeviceCapabilities::new(
            true,
            true,
            true,
            false,
            TorqueNm::new(20.0)?,
            4096,
            1000,
        );
        let rt = json_roundtrip(&caps)?;
        assert_eq!(rt.supports_pid, caps.supports_pid);
        assert_eq!(rt.supports_raw_torque_1khz, caps.supports_raw_torque_1khz);
        assert_eq!(rt.supports_health_stream, caps.supports_health_stream);
        assert_eq!(rt.supports_led_bus, caps.supports_led_bus);
        assert_eq!(rt.max_torque, caps.max_torque);
        assert_eq!(rt.encoder_cpr, caps.encoder_cpr);
        assert_eq!(rt.min_report_period_us, caps.min_report_period_us);
        Ok(())
    }

    #[test]
    fn device_state_roundtrip() -> TestResult {
        for state in [
            DeviceState::Disconnected,
            DeviceState::Connected,
            DeviceState::Active,
            DeviceState::Faulted,
            DeviceState::SafeMode,
        ] {
            let rt = json_roundtrip(&state)?;
            assert_eq!(rt, state);
        }
        Ok(())
    }

    #[test]
    fn calibration_type_roundtrip() -> TestResult {
        for ct in [
            CalibrationType::Center,
            CalibrationType::Range,
            CalibrationType::Pedals,
            CalibrationType::Full,
        ] {
            let rt = json_roundtrip(&ct)?;
            assert_eq!(rt, ct);
        }
        Ok(())
    }

    #[test]
    fn device_type_roundtrip() -> TestResult {
        for dt in [
            DeviceType::Other,
            DeviceType::WheelBase,
            DeviceType::SteeringWheel,
            DeviceType::Pedals,
            DeviceType::Shifter,
            DeviceType::Handbrake,
            DeviceType::ButtonBox,
        ] {
            let rt = json_roundtrip(&dt)?;
            assert_eq!(rt, dt);
        }
        Ok(())
    }

    #[test]
    fn calibration_data_roundtrip() -> TestResult {
        let cal = CalibrationData {
            center_position: Some(0.5),
            min_position: Some(-450.0),
            max_position: Some(450.0),
            pedal_ranges: Some(PedalCalibrationData {
                throttle: Some((0.0, 1.0)),
                brake: Some((0.1, 0.95)),
                clutch: None,
            }),
            calibrated_at: Some("2024-01-01T00:00:00Z".to_string()),
            calibration_type: CalibrationType::Full,
        };
        let rt = json_roundtrip(&cal)?;
        assert_eq!(rt, cal);
        Ok(())
    }

    #[test]
    fn filter_config_roundtrip() -> TestResult {
        let fc = FilterConfig::default();
        let rt = json_roundtrip(&fc)?;
        assert_eq!(rt, fc);
        Ok(())
    }

    #[test]
    fn base_settings_roundtrip() -> TestResult {
        let bs = BaseSettings::default();
        let rt = json_roundtrip(&bs)?;
        assert_eq!(rt, bs);
        Ok(())
    }

    #[test]
    fn profile_scope_roundtrip() -> TestResult {
        let scopes = [
            ProfileScope::global(),
            ProfileScope::for_game("iRacing".to_string()),
            ProfileScope::for_car("iRacing".to_string(), "MX-5".to_string()),
            ProfileScope::for_track(
                "iRacing".to_string(),
                "MX-5".to_string(),
                "Laguna Seca".to_string(),
            ),
        ];
        for scope in &scopes {
            let rt = json_roundtrip(scope)?;
            assert_eq!(&rt, scope);
        }
        Ok(())
    }

    #[test]
    fn profile_metadata_roundtrip() -> TestResult {
        let meta = ProfileMetadata {
            name: "Test Profile".to_string(),
            description: Some("A test".to_string()),
            author: Some("Test Author".to_string()),
            version: "1.0.0".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            modified_at: "2024-01-02T00:00:00Z".to_string(),
            tags: vec!["gt3".to_string(), "endurance".to_string()],
        };
        let rt = json_roundtrip(&meta)?;
        assert_eq!(rt, meta);
        Ok(())
    }

    #[test]
    fn entity_profile_roundtrip() -> TestResult {
        let profile = Profile::default_global()?;
        let rt = json_roundtrip_pretty(&profile)?;
        assert_eq!(rt.id.as_str(), profile.id.as_str());
        assert_eq!(rt.scope, profile.scope);
        assert_eq!(rt.base_settings, profile.base_settings);
        Ok(())
    }

    #[test]
    fn led_config_roundtrip() -> TestResult {
        let led = LedConfig::default();
        let rt = json_roundtrip(&led)?;
        assert_eq!(rt, led);
        Ok(())
    }

    #[test]
    fn haptics_config_roundtrip() -> TestResult {
        let hap = HapticsConfig::default();
        let rt = json_roundtrip(&hap)?;
        assert_eq!(rt, hap);
        Ok(())
    }

    #[test]
    fn bumpstop_config_roundtrip() -> TestResult {
        let bs = BumpstopConfig::default();
        let rt = json_roundtrip(&bs)?;
        assert_eq!(rt, bs);
        Ok(())
    }

    #[test]
    fn hands_off_config_roundtrip() -> TestResult {
        let ho = HandsOffConfig::default();
        let rt = json_roundtrip(&ho)?;
        assert_eq!(rt, ho);
        Ok(())
    }

    #[test]
    fn notch_filter_roundtrip() -> TestResult {
        let nf = NotchFilter::new(FrequencyHz::new(50.0)?, 2.0, -6.0)?;
        let rt = json_roundtrip(&nf)?;
        assert_eq!(rt, nf);
        Ok(())
    }

    #[test]
    fn telemetry_flags_roundtrip() -> TestResult {
        let flags = TelemetryFlags {
            yellow_flag: true,
            blue_flag: true,
            pit_limiter: true,
            abs_active: true,
            ..Default::default()
        };
        let rt = json_roundtrip(&flags)?;
        assert_eq!(rt, flags);
        Ok(())
    }

    #[test]
    fn telemetry_value_roundtrip() -> TestResult {
        let values = [
            TelemetryValue::Float(3.25),
            TelemetryValue::Integer(42),
            TelemetryValue::Boolean(true),
            TelemetryValue::String("test".to_string()),
        ];
        for v in &values {
            let rt = json_roundtrip(v)?;
            assert_eq!(&rt, v);
        }
        Ok(())
    }

    #[test]
    fn normalized_telemetry_roundtrip() -> TestResult {
        let telem = NormalizedTelemetry::builder()
            .speed_ms(45.0)
            .rpm(6500.0)
            .max_rpm(8000.0)
            .gear(4)
            .throttle(0.8)
            .brake(0.1)
            .lateral_g(1.5)
            .longitudinal_g(-0.3)
            .slip_ratio(0.05)
            .car_id("porsche-911-gt3")
            .track_id("laguna-seca")
            .session_id("sess-001")
            .position(3)
            .lap(12)
            .fuel_percent(0.65)
            .sequence(42)
            .build();
        let rt = json_roundtrip(&telem)?;
        assert!((rt.speed_ms - 45.0).abs() < f32::EPSILON);
        assert!((rt.rpm - 6500.0).abs() < f32::EPSILON);
        assert_eq!(rt.gear, 4);
        assert!((rt.throttle - 0.8).abs() < f32::EPSILON);
        assert_eq!(rt.car_id.as_deref(), Some("porsche-911-gt3"));
        assert_eq!(rt.track_id.as_deref(), Some("laguna-seca"));
        assert_eq!(rt.position, 3);
        assert_eq!(rt.lap, 12);
        assert_eq!(rt.sequence, 42);
        Ok(())
    }

    #[test]
    fn telemetry_snapshot_roundtrip() -> TestResult {
        let snap = TelemetrySnapshot {
            timestamp_ns: 123456789,
            speed_ms: 30.0,
            steering_angle: 0.1,
            throttle: 0.5,
            brake: 0.0,
            clutch: 0.0,
            rpm: 5000.0,
            max_rpm: 7000.0,
            gear: 3,
            num_gears: 6,
            lateral_g: 0.8,
            longitudinal_g: 0.2,
            vertical_g: 0.0,
            slip_ratio: 0.02,
            slip_angle_fl: 0.01,
            slip_angle_fr: 0.02,
            slip_angle_rl: 0.03,
            slip_angle_rr: 0.04,
            ffb_scalar: 0.6,
            ffb_torque_nm: 5.0,
            flags: TelemetryFlags::default(),
            position: 5,
            lap: 8,
            current_lap_time_s: 93.5,
            fuel_percent: 0.45,
            sequence: 100,
        };
        let rt = json_roundtrip(&snap)?;
        assert_eq!(rt.timestamp_ns, snap.timestamp_ns);
        assert!((rt.speed_ms - snap.speed_ms).abs() < f32::EPSILON);
        assert_eq!(rt.gear, snap.gear);
        assert_eq!(rt.sequence, snap.sequence);
        Ok(())
    }

    #[test]
    fn telemetry_frame_roundtrip() -> TestResult {
        let frame = TelemetryFrame::new(NormalizedTelemetry::default(), 999, 1, 256);
        let rt = json_roundtrip(&frame)?;
        assert_eq!(rt.timestamp_ns, 999);
        assert_eq!(rt.sequence, 1);
        assert_eq!(rt.raw_size, 256);
        Ok(())
    }

    #[test]
    fn telemetry_data_roundtrip() -> TestResult {
        let data = TelemetryData {
            wheel_angle_deg: 45.0,
            wheel_speed_rad_s: 1.5,
            temperature_c: 40,
            fault_flags: 0,
            hands_on: true,
            timestamp: 123456,
        };
        let rt = json_roundtrip(&data)?;
        assert_eq!(rt, data);
        Ok(())
    }

    #[test]
    fn schema_version_roundtrip() -> TestResult {
        let sv = racing_wheel_schemas::migration::SchemaVersion::new(1, 0);
        let rt = json_roundtrip(&sv)?;
        assert_eq!(rt, sv);
        Ok(())
    }

    #[test]
    fn backup_info_roundtrip() -> TestResult {
        let bi = racing_wheel_schemas::migration::BackupInfo::new(
            std::path::PathBuf::from("/profiles/test.json"),
            std::path::PathBuf::from("/backups/test.json.bak"),
            "wheel.profile/1".to_string(),
            "abc123".to_string(),
        );
        let rt = json_roundtrip(&bi)?;
        assert_eq!(rt.original_path, bi.original_path);
        assert_eq!(rt.backup_path, bi.backup_path);
        assert_eq!(rt.original_version, bi.original_version);
        assert_eq!(rt.content_hash, bi.content_hash);
        Ok(())
    }

    #[test]
    fn config_profile_schema_roundtrip() -> TestResult {
        let profile = racing_wheel_schemas::config::ProfileSchema {
            schema: "wheel.profile/1".to_string(),
            scope: racing_wheel_schemas::config::ProfileScope {
                game: Some("iRacing".to_string()),
                car: None,
                track: None,
            },
            base: racing_wheel_schemas::config::BaseConfig {
                ffb_gain: 0.8,
                dor_deg: 900,
                torque_cap_nm: 15.0,
                filters: racing_wheel_schemas::config::FilterConfig::default(),
            },
            leds: None,
            haptics: None,
            signature: None,
        };
        let rt = json_roundtrip(&profile)?;
        assert_eq!(rt.schema, profile.schema);
        assert_eq!(rt.scope.game, profile.scope.game);
        assert!((rt.base.ffb_gain - 0.8).abs() < f32::EPSILON);
        assert_eq!(rt.base.dor_deg, 900);
        Ok(())
    }

    #[test]
    fn config_filter_config_roundtrip() -> TestResult {
        let fc = racing_wheel_schemas::config::FilterConfig {
            reconstruction: 3,
            friction: 0.2,
            damper: 0.4,
            inertia: 0.1,
            bumpstop: racing_wheel_schemas::config::BumpstopConfig::default(),
            hands_off: racing_wheel_schemas::config::HandsOffConfig::default(),
            torque_cap: Some(12.0),
            notch_filters: vec![racing_wheel_schemas::config::NotchFilter {
                hz: 50.0,
                q: 2.0,
                gain_db: -6.0,
            }],
            slew_rate: 0.8,
            curve_points: vec![
                racing_wheel_schemas::config::CurvePoint {
                    input: 0.0,
                    output: 0.0,
                },
                racing_wheel_schemas::config::CurvePoint {
                    input: 1.0,
                    output: 1.0,
                },
            ],
        };
        let rt = json_roundtrip(&fc)?;
        assert_eq!(rt.reconstruction, 3);
        assert!((rt.friction - 0.2).abs() < f32::EPSILON);
        assert_eq!(rt.notch_filters.len(), 1);
        assert_eq!(rt.curve_points.len(), 2);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 2. Default values are correct and stable
// ═══════════════════════════════════════════════════════════

mod defaults {
    use super::*;
    use racing_wheel_schemas::entities::*;
    use racing_wheel_schemas::telemetry::*;

    #[test]
    fn filter_config_defaults_stable() -> TestResult {
        let fc1 = FilterConfig::default();
        let fc2 = FilterConfig::default();
        assert_eq!(fc1, fc2);
        assert_eq!(fc1.reconstruction, 0);
        assert!(fc1.is_linear());
        Ok(())
    }

    #[test]
    fn base_settings_defaults_stable() -> TestResult {
        let bs1 = BaseSettings::default();
        let bs2 = BaseSettings::default();
        assert_eq!(bs1, bs2);
        assert!((bs1.ffb_gain.value() - 0.7).abs() < f32::EPSILON);
        assert!((bs1.degrees_of_rotation.value() - 900.0).abs() < f32::EPSILON);
        assert!((bs1.torque_cap.value() - 15.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn bumpstop_defaults_stable() -> TestResult {
        let b1 = BumpstopConfig::default();
        let b2 = BumpstopConfig::default();
        assert_eq!(b1, b2);
        assert!(b1.enabled);
        assert!((b1.start_angle - 450.0).abs() < f32::EPSILON);
        assert!((b1.max_angle - 540.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn hands_off_defaults_stable() -> TestResult {
        let h1 = HandsOffConfig::default();
        let h2 = HandsOffConfig::default();
        assert_eq!(h1, h2);
        assert!(h1.enabled);
        assert!((h1.threshold - 0.05).abs() < f32::EPSILON);
        assert!((h1.timeout_seconds - 5.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn led_config_defaults_stable() -> TestResult {
        let l1 = LedConfig::default();
        let l2 = LedConfig::default();
        assert_eq!(l1, l2);
        assert_eq!(l1.pattern, "progressive");
        assert_eq!(l1.rpm_bands.len(), 5);
        Ok(())
    }

    #[test]
    fn haptics_config_defaults_stable() -> TestResult {
        let h1 = HapticsConfig::default();
        let h2 = HapticsConfig::default();
        assert_eq!(h1, h2);
        assert!(h1.enabled);
        assert!((h1.intensity.value() - 0.6).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn telemetry_flags_defaults_stable() -> TestResult {
        let f1 = TelemetryFlags::default();
        let f2 = TelemetryFlags::default();
        assert_eq!(f1, f2);
        assert!(f1.green_flag);
        assert!(!f1.yellow_flag);
        assert!(!f1.red_flag);
        assert!(!f1.blue_flag);
        assert!(!f1.checkered_flag);
        Ok(())
    }

    #[test]
    fn normalized_telemetry_defaults_stable() -> TestResult {
        let t1 = NormalizedTelemetry::default();
        let t2 = NormalizedTelemetry::default();
        assert!((t1.speed_ms - t2.speed_ms).abs() < f32::EPSILON);
        assert_eq!(t1.gear, 0);
        assert!(t1.car_id.is_none());
        assert!(t1.track_id.is_none());
        assert!(t1.session_id.is_none());
        Ok(())
    }

    #[test]
    fn telemetry_data_defaults_stable() -> TestResult {
        let d1 = TelemetryData::default();
        let d2 = TelemetryData::default();
        assert_eq!(d1, d2);
        assert!((d1.wheel_angle_deg - 0.0).abs() < f32::EPSILON);
        assert!(!d1.hands_on);
        Ok(())
    }

    #[test]
    fn config_bumpstop_defaults_stable() -> TestResult {
        let b1 = racing_wheel_schemas::config::BumpstopConfig::default();
        let b2 = racing_wheel_schemas::config::BumpstopConfig::default();
        assert!(b1.enabled && b2.enabled);
        assert!((b1.strength - 0.5).abs() < f32::EPSILON);
        assert!((b2.strength - 0.5).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn config_hands_off_defaults_stable() -> TestResult {
        let h1 = racing_wheel_schemas::config::HandsOffConfig::default();
        let h2 = racing_wheel_schemas::config::HandsOffConfig::default();
        assert!(h1.enabled && h2.enabled);
        assert!((h1.sensitivity - 0.3).abs() < f32::EPSILON);
        assert!((h2.sensitivity - 0.3).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn config_filter_config_defaults_stable() -> TestResult {
        let f1 = racing_wheel_schemas::config::FilterConfig::default();
        let f2 = racing_wheel_schemas::config::FilterConfig::default();
        assert_eq!(f1.reconstruction, f2.reconstruction);
        assert!((f1.friction - f2.friction).abs() < f32::EPSILON);
        assert_eq!(f1.curve_points.len(), 2);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 3. Unknown field handling (forward compatibility)
// ═══════════════════════════════════════════════════════════

mod unknown_fields {
    use super::*;
    use racing_wheel_schemas::entities::*;
    use racing_wheel_schemas::telemetry::*;

    #[test]
    fn telemetry_flags_ignores_unknown_fields() -> TestResult {
        let json = r#"{
            "yellow_flag": true,
            "red_flag": false,
            "blue_flag": false,
            "checkered_flag": false,
            "green_flag": true,
            "pit_limiter": false,
            "in_pits": false,
            "drs_available": false,
            "drs_active": false,
            "ers_available": false,
            "ers_active": false,
            "launch_control": false,
            "traction_control": false,
            "abs_active": false,
            "engine_limiter": false,
            "safety_car": false,
            "formation_lap": false,
            "session_paused": false,
            "future_flag_v2": true,
            "another_unknown": 42
        }"#;
        let flags: TelemetryFlags = serde_json::from_str(json)?;
        assert!(flags.yellow_flag);
        assert!(flags.green_flag);
        Ok(())
    }

    #[test]
    fn profile_scope_ignores_unknown_fields() -> TestResult {
        let json = r#"{
            "game": "iRacing",
            "car": null,
            "track": null,
            "weather": "rainy"
        }"#;
        let scope: ProfileScope = serde_json::from_str(json)?;
        assert_eq!(scope.game.as_deref(), Some("iRacing"));
        Ok(())
    }

    #[test]
    fn telemetry_data_ignores_unknown_fields() -> TestResult {
        let json = r#"{
            "wheel_angle_deg": 10.0,
            "wheel_speed_rad_s": 0.5,
            "temperature_c": 35,
            "fault_flags": 0,
            "hands_on": true,
            "timestamp": 100,
            "future_field": "unknown"
        }"#;
        let data: TelemetryData = serde_json::from_str(json)?;
        assert!((data.wheel_angle_deg - 10.0).abs() < f32::EPSILON);
        assert!(data.hands_on);
        Ok(())
    }

    #[test]
    fn calibration_data_ignores_unknown_fields() -> TestResult {
        let json = r#"{
            "center_position": 0.0,
            "min_position": null,
            "max_position": null,
            "pedal_ranges": null,
            "calibrated_at": null,
            "calibration_type": "Center",
            "hardware_revision": "v3.0"
        }"#;
        let cal: CalibrationData = serde_json::from_str(json)?;
        assert!((cal.center_position.unwrap_or(1.0) - 0.0).abs() < f32::EPSILON);
        assert_eq!(cal.calibration_type, CalibrationType::Center);
        Ok(())
    }

    #[test]
    fn config_profile_schema_ignores_unknown_fields() -> TestResult {
        let json = r#"{
            "schema": "wheel.profile/1",
            "scope": { "game": null, "car": null, "track": null },
            "base": {
                "ffbGain": 0.7,
                "dorDeg": 900,
                "torqueCapNm": 10.0,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.0,
                    "damper": 0.0,
                    "inertia": 0.0,
                    "notchFilters": [],
                    "slewRate": 1.0,
                    "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
                }
            },
            "future_section": { "value": 123 }
        }"#;
        let profile: racing_wheel_schemas::config::ProfileSchema = serde_json::from_str(json)?;
        assert_eq!(profile.schema, "wheel.profile/1");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 4. Empty/minimal payloads deserialize without panic
// ═══════════════════════════════════════════════════════════

mod minimal_payloads {
    use super::*;
    use racing_wheel_schemas::telemetry::*;

    #[test]
    fn normalized_telemetry_minimal_json() -> TestResult {
        // All fields with serde(default) should allow minimal payloads
        let json = r#"{
            "speed_ms": 0.0,
            "steering_angle": 0.0,
            "throttle": 0.0,
            "brake": 0.0,
            "rpm": 0.0,
            "gear": 0,
            "flags": {}
        }"#;
        let t: NormalizedTelemetry = serde_json::from_str(json)?;
        assert!((t.speed_ms - 0.0).abs() < f32::EPSILON);
        assert_eq!(t.gear, 0);
        // Defaults should kick in for omitted fields
        assert!(t.green_flag_is_default_true(&t.flags));
        Ok(())
    }

    #[test]
    fn telemetry_flags_empty_object() -> TestResult {
        let json = r#"{}"#;
        let flags: TelemetryFlags = serde_json::from_str(json)?;
        // green_flag defaults to true via default_true
        assert!(flags.green_flag);
        assert!(!flags.yellow_flag);
        Ok(())
    }

    #[test]
    fn telemetry_snapshot_minimal() -> TestResult {
        let json = r#"{
            "timestamp_ns": 0,
            "speed_ms": 0.0,
            "steering_angle": 0.0,
            "throttle": 0.0,
            "brake": 0.0,
            "rpm": 0.0,
            "gear": 0
        }"#;
        let snap: TelemetrySnapshot = serde_json::from_str(json)?;
        assert_eq!(snap.timestamp_ns, 0);
        assert_eq!(snap.gear, 0);
        Ok(())
    }

    #[test]
    fn config_bumpstop_from_empty_object() -> TestResult {
        let json = r#"{}"#;
        let bs: racing_wheel_schemas::config::BumpstopConfig = serde_json::from_str(json)?;
        // defaults should apply
        assert!(bs.enabled);
        assert!((bs.strength - 0.5).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn config_hands_off_from_empty_object() -> TestResult {
        let json = r#"{}"#;
        let ho: racing_wheel_schemas::config::HandsOffConfig = serde_json::from_str(json)?;
        assert!(ho.enabled);
        assert!((ho.sensitivity - 0.3).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn telemetry_data_from_defaults() -> TestResult {
        let data = TelemetryData::default();
        let rt = json_roundtrip(&data)?;
        assert_eq!(rt, data);
        Ok(())
    }

    #[test]
    fn invalid_json_returns_error() {
        let result = serde_json::from_str::<TelemetryFlags>("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn empty_string_returns_error() {
        let result = serde_json::from_str::<TelemetryData>("");
        assert!(result.is_err());
    }
}

// Helper trait to check flags default
trait FlagDefaultCheck {
    fn green_flag_is_default_true(&self, flags: &racing_wheel_schemas::telemetry::TelemetryFlags) -> bool;
}

impl FlagDefaultCheck for racing_wheel_schemas::telemetry::NormalizedTelemetry {
    fn green_flag_is_default_true(&self, flags: &racing_wheel_schemas::telemetry::TelemetryFlags) -> bool {
        flags.green_flag
    }
}

// ═══════════════════════════════════════════════════════════
// 5. Field rename attributes work correctly
// ═══════════════════════════════════════════════════════════

mod field_renames {
    use super::*;

    #[test]
    fn config_base_config_renames() -> TestResult {
        let bc = racing_wheel_schemas::config::BaseConfig {
            ffb_gain: 0.7,
            dor_deg: 900,
            torque_cap_nm: 15.0,
            filters: racing_wheel_schemas::config::FilterConfig::default(),
        };
        let json = serde_json::to_string(&bc)?;
        // Verify renamed fields appear in JSON
        assert!(json.contains("\"ffbGain\""));
        assert!(json.contains("\"dorDeg\""));
        assert!(json.contains("\"torqueCapNm\""));
        // Verify Rust field names do NOT appear
        assert!(!json.contains("\"ffb_gain\""));
        assert!(!json.contains("\"dor_deg\""));
        assert!(!json.contains("\"torque_cap_nm\""));
        Ok(())
    }

    #[test]
    fn config_filter_config_renames() -> TestResult {
        let fc = racing_wheel_schemas::config::FilterConfig {
            reconstruction: 0,
            friction: 0.0,
            damper: 0.0,
            inertia: 0.0,
            bumpstop: racing_wheel_schemas::config::BumpstopConfig::default(),
            hands_off: racing_wheel_schemas::config::HandsOffConfig::default(),
            torque_cap: Some(10.0),
            notch_filters: vec![racing_wheel_schemas::config::NotchFilter {
                hz: 50.0,
                q: 2.0,
                gain_db: -3.0,
            }],
            slew_rate: 1.0,
            curve_points: vec![
                racing_wheel_schemas::config::CurvePoint {
                    input: 0.0,
                    output: 0.0,
                },
                racing_wheel_schemas::config::CurvePoint {
                    input: 1.0,
                    output: 1.0,
                },
            ],
        };
        let json = serde_json::to_string(&fc)?;
        assert!(json.contains("\"handsOff\""));
        assert!(json.contains("\"torqueCap\""));
        assert!(json.contains("\"notchFilters\""));
        assert!(json.contains("\"slewRate\""));
        assert!(json.contains("\"curvePoints\""));
        assert!(json.contains("\"gainDb\""));
        Ok(())
    }

    #[test]
    fn config_led_config_renames() -> TestResult {
        let led = racing_wheel_schemas::config::LedConfig {
            rpm_bands: vec![0.8, 0.9],
            pattern: "progressive".to_string(),
            brightness: 0.7,
            colors: None,
        };
        let json = serde_json::to_string(&led)?;
        assert!(json.contains("\"rpmBands\""));
        assert!(!json.contains("\"rpm_bands\""));
        Ok(())
    }

    #[test]
    fn config_haptics_config_renames() -> TestResult {
        let hap = racing_wheel_schemas::config::HapticsConfig {
            enabled: true,
            intensity: 0.5,
            frequency_hz: 80.0,
            effects: None,
        };
        let json = serde_json::to_string(&hap)?;
        assert!(json.contains("\"frequencyHz\""));
        assert!(!json.contains("\"frequency_hz\""));
        Ok(())
    }

    #[test]
    fn config_renames_deserialize_correctly() -> TestResult {
        let json = r#"{
            "ffbGain": 0.9,
            "dorDeg": 1080,
            "torqueCapNm": 20.0,
            "filters": {
                "reconstruction": 2,
                "friction": 0.1,
                "damper": 0.2,
                "inertia": 0.05,
                "handsOff": {},
                "notchFilters": [],
                "slewRate": 0.9,
                "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
            }
        }"#;
        let bc: racing_wheel_schemas::config::BaseConfig = serde_json::from_str(json)?;
        assert!((bc.ffb_gain - 0.9).abs() < f32::EPSILON);
        assert_eq!(bc.dor_deg, 1080);
        assert!((bc.torque_cap_nm - 20.0).abs() < f32::EPSILON);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 6. Enum variant serialization matches expected strings
// ═══════════════════════════════════════════════════════════

mod enum_variants {
    use super::*;
    use racing_wheel_schemas::entities::*;
    use racing_wheel_schemas::telemetry::*;

    #[test]
    fn device_state_variant_strings() -> TestResult {
        let cases = [
            (DeviceState::Disconnected, "\"Disconnected\""),
            (DeviceState::Connected, "\"Connected\""),
            (DeviceState::Active, "\"Active\""),
            (DeviceState::Faulted, "\"Faulted\""),
            (DeviceState::SafeMode, "\"SafeMode\""),
        ];
        for (variant, expected) in &cases {
            let json = serde_json::to_string(variant)?;
            assert_eq!(&json, expected, "DeviceState::{variant:?} serialization mismatch");
        }
        Ok(())
    }

    #[test]
    fn device_type_variant_strings() -> TestResult {
        let cases = [
            (DeviceType::Other, "\"Other\""),
            (DeviceType::WheelBase, "\"WheelBase\""),
            (DeviceType::SteeringWheel, "\"SteeringWheel\""),
            (DeviceType::Pedals, "\"Pedals\""),
            (DeviceType::Shifter, "\"Shifter\""),
            (DeviceType::Handbrake, "\"Handbrake\""),
            (DeviceType::ButtonBox, "\"ButtonBox\""),
        ];
        for (variant, expected) in &cases {
            let json = serde_json::to_string(variant)?;
            assert_eq!(&json, expected, "DeviceType::{variant:?} serialization mismatch");
        }
        Ok(())
    }

    #[test]
    fn calibration_type_variant_strings() -> TestResult {
        let cases = [
            (CalibrationType::Center, "\"Center\""),
            (CalibrationType::Range, "\"Range\""),
            (CalibrationType::Pedals, "\"Pedals\""),
            (CalibrationType::Full, "\"Full\""),
        ];
        for (variant, expected) in &cases {
            let json = serde_json::to_string(variant)?;
            assert_eq!(&json, expected, "CalibrationType::{variant:?} serialization mismatch");
        }
        Ok(())
    }

    #[test]
    fn telemetry_value_tagged_enum() -> TestResult {
        // TelemetryValue uses #[serde(tag = "type", content = "value")]
        let float_json = serde_json::to_string(&TelemetryValue::Float(1.5))?;
        assert!(float_json.contains("\"type\":\"Float\""));
        assert!(float_json.contains("\"value\":1.5"));

        let int_json = serde_json::to_string(&TelemetryValue::Integer(42))?;
        assert!(int_json.contains("\"type\":\"Integer\""));
        assert!(int_json.contains("\"value\":42"));

        let bool_json = serde_json::to_string(&TelemetryValue::Boolean(true))?;
        assert!(bool_json.contains("\"type\":\"Boolean\""));
        assert!(bool_json.contains("\"value\":true"));

        let str_json = serde_json::to_string(&TelemetryValue::String("hello".to_string()))?;
        assert!(str_json.contains("\"type\":\"String\""));
        assert!(str_json.contains("\"value\":\"hello\""));
        Ok(())
    }

    #[test]
    fn enum_variants_deserialize_from_strings() -> TestResult {
        let ds: DeviceState = serde_json::from_str("\"Active\"")?;
        assert_eq!(ds, DeviceState::Active);

        let dt: DeviceType = serde_json::from_str("\"WheelBase\"")?;
        assert_eq!(dt, DeviceType::WheelBase);

        let ct: CalibrationType = serde_json::from_str("\"Full\"")?;
        assert_eq!(ct, CalibrationType::Full);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 7. Optional field omission behavior
// ═══════════════════════════════════════════════════════════

mod optional_fields {
    use super::*;
    use racing_wheel_schemas::entities::*;
    use racing_wheel_schemas::telemetry::*;

    #[test]
    fn profile_parent_omitted_when_none() -> TestResult {
        let profile = Profile::default_global()?;
        let json = serde_json::to_string(&profile)?;
        assert!(!json.contains("\"parent\""));
        Ok(())
    }

    #[test]
    fn profile_parent_present_when_set() -> TestResult {
        use racing_wheel_schemas::domain::ProfileId;
        let parent_id: ProfileId = "parent".parse()?;
        let child_id: ProfileId = "child".parse()?;
        let profile = Profile::new_with_parent(
            child_id,
            parent_id.clone(),
            ProfileScope::global(),
            BaseSettings::default(),
            "Child".to_string(),
        );
        let json = serde_json::to_string(&profile)?;
        assert!(json.contains("\"parent\""));
        let rt: Profile = serde_json::from_str(&json)?;
        assert_eq!(rt.parent.as_ref().map(|p| p.as_str()), Some("parent"));
        Ok(())
    }

    #[test]
    fn normalized_telemetry_optional_ids_omitted() -> TestResult {
        let t = NormalizedTelemetry::default();
        let json = serde_json::to_string(&t)?;
        // car_id, track_id, session_id use skip_serializing_if = "Option::is_none"
        assert!(!json.contains("\"car_id\""));
        assert!(!json.contains("\"track_id\""));
        assert!(!json.contains("\"session_id\""));
        Ok(())
    }

    #[test]
    fn normalized_telemetry_optional_ids_present_when_set() -> TestResult {
        let t = NormalizedTelemetry::builder()
            .car_id("gt3")
            .track_id("spa")
            .session_id("s1")
            .build();
        let json = serde_json::to_string(&t)?;
        assert!(json.contains("\"car_id\""));
        assert!(json.contains("\"track_id\""));
        assert!(json.contains("\"session_id\""));
        Ok(())
    }

    #[test]
    fn normalized_telemetry_extended_omitted_when_empty() -> TestResult {
        let t = NormalizedTelemetry::default();
        let json = serde_json::to_string(&t)?;
        // extended uses skip_serializing_if = "BTreeMap::is_empty"
        assert!(!json.contains("\"extended\""));
        Ok(())
    }

    #[test]
    fn normalized_telemetry_extended_present_when_non_empty() -> TestResult {
        let t = NormalizedTelemetry::builder()
            .extended("custom", TelemetryValue::Float(1.0))
            .build();
        let json = serde_json::to_string(&t)?;
        assert!(json.contains("\"extended\""));
        assert!(json.contains("\"custom\""));
        Ok(())
    }

    #[test]
    fn config_profile_optional_sections_omitted() -> TestResult {
        let profile = racing_wheel_schemas::config::ProfileSchema {
            schema: "wheel.profile/1".to_string(),
            scope: racing_wheel_schemas::config::ProfileScope {
                game: None,
                car: None,
                track: None,
            },
            base: racing_wheel_schemas::config::BaseConfig {
                ffb_gain: 0.7,
                dor_deg: 900,
                torque_cap_nm: 10.0,
                filters: racing_wheel_schemas::config::FilterConfig::default(),
            },
            leds: None,
            haptics: None,
            signature: None,
        };
        let json = serde_json::to_string(&profile)?;
        assert!(!json.contains("\"leds\""));
        assert!(!json.contains("\"haptics\""));
        assert!(!json.contains("\"signature\""));
        Ok(())
    }

    #[test]
    fn config_torque_cap_omitted_when_none() -> TestResult {
        let fc = racing_wheel_schemas::config::FilterConfig {
            torque_cap: None,
            ..racing_wheel_schemas::config::FilterConfig::default()
        };
        let json = serde_json::to_string(&fc)?;
        assert!(!json.contains("\"torqueCap\""));
        Ok(())
    }

    #[test]
    fn calibration_optional_fields() -> TestResult {
        let cal = CalibrationData {
            center_position: None,
            min_position: None,
            max_position: None,
            pedal_ranges: None,
            calibrated_at: None,
            calibration_type: CalibrationType::Center,
        };
        let rt = json_roundtrip(&cal)?;
        assert!(rt.center_position.is_none());
        assert!(rt.pedal_ranges.is_none());
        assert!(rt.calibrated_at.is_none());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 8. Nested type composition roundtrip
// ═══════════════════════════════════════════════════════════

mod nested_composition {
    use super::*;
    use racing_wheel_schemas::domain::*;
    use racing_wheel_schemas::entities::*;
    use racing_wheel_schemas::telemetry::*;

    #[test]
    fn full_device_roundtrip() -> TestResult {
        let device = Device::new(
            "moza-r9".parse()?,
            "Moza R9".to_string(),
            DeviceType::WheelBase,
            DeviceCapabilities::new(
                true,
                true,
                true,
                true,
                TorqueNm::new(25.0)?,
                8192,
                1000,
            ),
        );
        let json = serde_json::to_string_pretty(&device)?;
        let rt: Device = serde_json::from_str(&json)?;
        assert_eq!(rt.id.as_str(), "moza-r9");
        assert_eq!(rt.name, "Moza R9");
        assert_eq!(rt.device_type, DeviceType::WheelBase);
        assert!(rt.capabilities.supports_pid);
        assert!(rt.capabilities.supports_led_bus);
        assert_eq!(rt.capabilities.encoder_cpr, 8192);
        Ok(())
    }

    #[test]
    fn full_profile_with_all_configs() -> TestResult {
        let profile_id: ProfileId = "iracing.gt3.spa".parse()?;
        let mut profile = Profile::new(
            profile_id,
            ProfileScope::for_track(
                "iRacing".to_string(),
                "GT3".to_string(),
                "Spa".to_string(),
            ),
            BaseSettings::default(),
            "iRacing GT3 at Spa".to_string(),
        );
        profile.metadata.tags = vec!["gt3".to_string(), "endurance".to_string()];
        profile.metadata.author = Some("TestUser".to_string());
        profile.metadata.description = Some("Optimized for Spa".to_string());

        let json = serde_json::to_string_pretty(&profile)?;
        let rt: Profile = serde_json::from_str(&json)?;
        assert_eq!(rt.id.as_str(), "iracing.gt3.spa");
        assert_eq!(rt.scope.game.as_deref(), Some("iRacing"));
        assert_eq!(rt.scope.car.as_deref(), Some("GT3"));
        assert_eq!(rt.scope.track.as_deref(), Some("Spa"));
        assert_eq!(rt.metadata.tags.len(), 2);
        assert_eq!(rt.metadata.author.as_deref(), Some("TestUser"));
        assert!(rt.led_config.is_some());
        assert!(rt.haptics_config.is_some());
        Ok(())
    }

    #[test]
    fn filter_config_with_notch_filters_and_curve() -> TestResult {
        let fc = FilterConfig::new(
            3,
            Gain::new(0.2)?,
            Gain::new(0.3)?,
            Gain::new(0.1)?,
            vec![
                NotchFilter::new(FrequencyHz::new(50.0)?, 2.0, -6.0)?,
                NotchFilter::new(FrequencyHz::new(60.0)?, 3.0, -12.0)?,
            ],
            Gain::new(0.9)?,
            vec![
                CurvePoint::new(0.0, 0.0)?,
                CurvePoint::new(0.3, 0.4)?,
                CurvePoint::new(0.7, 0.85)?,
                CurvePoint::new(1.0, 1.0)?,
            ],
        )?;
        let rt = json_roundtrip(&fc)?;
        assert_eq!(rt.notch_filters.len(), 2);
        assert_eq!(rt.curve_points.len(), 4);
        assert_eq!(rt.reconstruction, 3);
        assert!((rt.friction.value() - 0.2).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn normalized_telemetry_with_extended_data() -> TestResult {
        let t = NormalizedTelemetry::builder()
            .speed_ms(55.0)
            .rpm(7000.0)
            .gear(5)
            .extended("wind_speed", TelemetryValue::Float(12.5))
            .extended("rain_intensity", TelemetryValue::Integer(3))
            .extended("night_mode", TelemetryValue::Boolean(true))
            .extended("track_surface", TelemetryValue::String("asphalt".to_string()))
            .build();
        let rt = json_roundtrip(&t)?;
        assert_eq!(rt.extended.len(), 4);
        assert_eq!(
            rt.extended.get("wind_speed"),
            Some(&TelemetryValue::Float(12.5))
        );
        assert_eq!(
            rt.extended.get("rain_intensity"),
            Some(&TelemetryValue::Integer(3))
        );
        assert_eq!(
            rt.extended.get("night_mode"),
            Some(&TelemetryValue::Boolean(true))
        );
        assert_eq!(
            rt.extended.get("track_surface"),
            Some(&TelemetryValue::String("asphalt".to_string()))
        );
        Ok(())
    }

    #[test]
    fn telemetry_frame_with_populated_data() -> TestResult {
        let data = NormalizedTelemetry::builder()
            .speed_ms(40.0)
            .rpm(5500.0)
            .gear(3)
            .flags(TelemetryFlags {
                yellow_flag: true,
                ..Default::default()
            })
            .build();
        let frame = TelemetryFrame::new(data, 1_000_000, 42, 512);
        let rt = json_roundtrip(&frame)?;
        assert!((rt.data.speed_ms - 40.0).abs() < f32::EPSILON);
        assert!(rt.data.flags.yellow_flag);
        assert_eq!(rt.timestamp_ns, 1_000_000);
        assert_eq!(rt.sequence, 42);
        assert_eq!(rt.raw_size, 512);
        Ok(())
    }

    #[test]
    fn config_full_profile_schema() -> TestResult {
        let mut colors = HashMap::new();
        colors.insert("shift".to_string(), [255, 0, 0]);

        let profile = racing_wheel_schemas::config::ProfileSchema {
            schema: "wheel.profile/1".to_string(),
            scope: racing_wheel_schemas::config::ProfileScope {
                game: Some("ACC".to_string()),
                car: Some("Ferrari 488".to_string()),
                track: Some("Monza".to_string()),
            },
            base: racing_wheel_schemas::config::BaseConfig {
                ffb_gain: 0.85,
                dor_deg: 720,
                torque_cap_nm: 18.0,
                filters: racing_wheel_schemas::config::FilterConfig {
                    reconstruction: 2,
                    friction: 0.15,
                    damper: 0.25,
                    inertia: 0.1,
                    bumpstop: racing_wheel_schemas::config::BumpstopConfig::default(),
                    hands_off: racing_wheel_schemas::config::HandsOffConfig::default(),
                    torque_cap: Some(16.0),
                    notch_filters: vec![racing_wheel_schemas::config::NotchFilter {
                        hz: 50.0,
                        q: 2.0,
                        gain_db: -6.0,
                    }],
                    slew_rate: 0.85,
                    curve_points: vec![
                        racing_wheel_schemas::config::CurvePoint {
                            input: 0.0,
                            output: 0.0,
                        },
                        racing_wheel_schemas::config::CurvePoint {
                            input: 0.5,
                            output: 0.6,
                        },
                        racing_wheel_schemas::config::CurvePoint {
                            input: 1.0,
                            output: 1.0,
                        },
                    ],
                },
            },
            leds: Some(racing_wheel_schemas::config::LedConfig {
                rpm_bands: vec![0.8, 0.9, 0.95],
                pattern: "sequential".to_string(),
                brightness: 0.9,
                colors: Some(colors),
            }),
            haptics: Some(racing_wheel_schemas::config::HapticsConfig {
                enabled: true,
                intensity: 0.7,
                frequency_hz: 100.0,
                effects: Some({
                    let mut m = HashMap::new();
                    m.insert("kerb".to_string(), true);
                    m
                }),
            }),
            signature: Some("sig-abc123".to_string()),
        };
        let rt = json_roundtrip(&profile)?;
        assert_eq!(rt.schema, "wheel.profile/1");
        assert_eq!(rt.scope.car.as_deref(), Some("Ferrari 488"));
        assert!(rt.leds.is_some());
        assert!(rt.haptics.is_some());
        assert_eq!(rt.signature.as_deref(), Some("sig-abc123"));
        Ok(())
    }

    #[test]
    fn profile_with_parent_child_relationship() -> TestResult {
        let parent_id: ProfileId = "base-profile".parse()?;
        let child_id: ProfileId = "child-profile".parse()?;

        let parent = Profile::new(
            parent_id.clone(),
            ProfileScope::global(),
            BaseSettings::default(),
            "Base".to_string(),
        );
        let child = Profile::new_with_parent(
            child_id,
            parent_id.clone(),
            ProfileScope::for_game("iRacing".to_string()),
            BaseSettings::default(),
            "Child".to_string(),
        );

        let parent_rt = json_roundtrip(&parent)?;
        let child_rt = json_roundtrip(&child)?;

        assert!(parent_rt.parent.is_none());
        assert_eq!(
            child_rt.parent.as_ref().map(|p| p.as_str()),
            Some("base-profile")
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 9. Large payload handling (many items in arrays)
// ═══════════════════════════════════════════════════════════

mod large_payloads {
    use super::*;
    use racing_wheel_schemas::entities::*;
    use racing_wheel_schemas::telemetry::*;

    #[test]
    fn many_telemetry_snapshots() -> TestResult {
        let snapshots: Vec<TelemetrySnapshot> = (0..1000)
            .map(|i| TelemetrySnapshot {
                timestamp_ns: i as u64 * 1_000_000,
                speed_ms: (i as f32) * 0.1,
                steering_angle: 0.0,
                throttle: 0.5,
                brake: 0.0,
                clutch: 0.0,
                rpm: 3000.0 + (i as f32) * 5.0,
                max_rpm: 8000.0,
                gear: ((i % 6) + 1) as i8,
                num_gears: 6,
                lateral_g: 0.0,
                longitudinal_g: 0.0,
                vertical_g: 0.0,
                slip_ratio: 0.0,
                slip_angle_fl: 0.0,
                slip_angle_fr: 0.0,
                slip_angle_rl: 0.0,
                slip_angle_rr: 0.0,
                ffb_scalar: 0.0,
                ffb_torque_nm: 0.0,
                flags: TelemetryFlags::default(),
                position: 1,
                lap: 0,
                current_lap_time_s: 0.0,
                fuel_percent: 1.0,
                sequence: i as u64,
            })
            .collect();

        let json = serde_json::to_string(&snapshots)?;
        let rt: Vec<TelemetrySnapshot> = serde_json::from_str(&json)?;
        assert_eq!(rt.len(), 1000);
        assert_eq!(rt[0].sequence, 0);
        assert_eq!(rt[999].sequence, 999);
        Ok(())
    }

    #[test]
    fn many_extended_telemetry_values() -> TestResult {
        let mut t = NormalizedTelemetry::default();
        for i in 0..100 {
            t.extended
                .insert(format!("field_{i}"), TelemetryValue::Float(i as f32));
        }
        let rt = json_roundtrip(&t)?;
        assert_eq!(rt.extended.len(), 100);
        assert_eq!(
            rt.extended.get("field_50"),
            Some(&TelemetryValue::Float(50.0))
        );
        Ok(())
    }

    #[test]
    fn many_profiles() -> TestResult {
        let profiles: Vec<Profile> = (0..100)
            .map(|i| {
                let id: racing_wheel_schemas::domain::ProfileId = format!("profile-{i}")
                    .parse()
                    .map_err(|e: racing_wheel_schemas::domain::DomainError| {
                        Box::new(e) as Box<dyn std::error::Error>
                    })
                    .ok()
                    .unwrap_or_else(|| {
                        // Fallback - this should never happen for valid inputs
                        panic!("Failed to parse valid profile ID: profile-{i}")
                    });
                Profile::new(
                    id,
                    ProfileScope::global(),
                    BaseSettings::default(),
                    format!("Profile {i}"),
                )
            })
            .collect();

        let json = serde_json::to_string(&profiles)?;
        let rt: Vec<Profile> = serde_json::from_str(&json)?;
        assert_eq!(rt.len(), 100);
        assert_eq!(rt[0].id.as_str(), "profile-0");
        assert_eq!(rt[99].id.as_str(), "profile-99");
        Ok(())
    }

    #[test]
    fn many_notch_filters() -> TestResult {
        use racing_wheel_schemas::domain::*;

        let filters: Vec<NotchFilter> = (1..=50)
            .map(|i| {
                let freq = FrequencyHz::new(i as f32 * 10.0);
                match freq {
                    Ok(f) => NotchFilter::new(f, 2.0, -6.0),
                    Err(e) => Err(e),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        let json = serde_json::to_string(&filters)?;
        let rt: Vec<NotchFilter> = serde_json::from_str(&json)?;
        assert_eq!(rt.len(), 50);
        Ok(())
    }

    #[test]
    fn large_led_colors_map() -> TestResult {
        let mut colors = HashMap::new();
        for i in 0..50 {
            colors.insert(
                format!("color_{i}"),
                [(i as u8) % 255, ((i * 3) as u8) % 255, ((i * 7) as u8) % 255],
            );
        }
        let led = LedConfig::new(
            vec![0.5, 0.7, 0.9],
            "custom".to_string(),
            racing_wheel_schemas::domain::Gain::new(0.8)?,
            colors.clone(),
        )?;
        let rt = json_roundtrip(&led)?;
        assert_eq!(rt.colors.len(), 50);
        Ok(())
    }

    #[test]
    fn many_metadata_tags() -> TestResult {
        let meta = ProfileMetadata {
            name: "Tagged Profile".to_string(),
            description: None,
            author: None,
            version: "1.0.0".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            modified_at: "2024-01-01T00:00:00Z".to_string(),
            tags: (0..200).map(|i| format!("tag-{i}")).collect(),
        };
        let rt = json_roundtrip(&meta)?;
        assert_eq!(rt.tags.len(), 200);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 10. Cross-version compatibility (old format → new format)
// ═══════════════════════════════════════════════════════════

mod cross_version {
    use super::*;
    use racing_wheel_schemas::migration::*;

    #[test]
    fn schema_version_parsing() -> TestResult {
        let v1 = SchemaVersion::parse("wheel.profile/1")?;
        assert_eq!(v1.major, 1);
        assert_eq!(v1.minor, 0);
        assert!(v1.is_current());

        let v1_1 = SchemaVersion::parse("wheel.profile/1.1")?;
        assert_eq!(v1_1.major, 1);
        assert_eq!(v1_1.minor, 1);

        let v2 = SchemaVersion::parse("wheel.profile/2")?;
        assert_eq!(v2.major, 2);
        assert!(!v2.is_current());
        Ok(())
    }

    #[test]
    fn schema_version_ordering() -> TestResult {
        let v1 = SchemaVersion::parse("wheel.profile/1")?;
        let v1_1 = SchemaVersion::parse("wheel.profile/1.1")?;
        let v2 = SchemaVersion::parse("wheel.profile/2")?;

        assert!(v1.is_older_than(&v1_1));
        assert!(v1.is_older_than(&v2));
        assert!(v1_1.is_older_than(&v2));
        assert!(!v2.is_older_than(&v1));
        Ok(())
    }

    #[test]
    fn invalid_schema_version_rejected() {
        assert!(SchemaVersion::parse("invalid").is_err());
        assert!(SchemaVersion::parse("other.schema/1").is_err());
        assert!(SchemaVersion::parse("wheel.profile/").is_err());
    }

    #[test]
    fn current_schema_version_constant() {
        assert_eq!(CURRENT_SCHEMA_VERSION, "wheel.profile/1");
    }

    #[test]
    fn old_profile_without_new_optional_fields() -> TestResult {
        // Simulate a v1 profile that doesn't have clutch, max_rpm, num_gears, etc.
        // (which were added later with serde(default))
        let json = r#"{
            "speed_ms": 30.0,
            "steering_angle": 0.1,
            "throttle": 0.5,
            "brake": 0.2,
            "rpm": 5000.0,
            "gear": 3,
            "flags": {}
        }"#;
        let t: racing_wheel_schemas::telemetry::NormalizedTelemetry =
            serde_json::from_str(json)?;
        // Fields with serde(default) should use defaults
        assert!((t.clutch - 0.0).abs() < f32::EPSILON);
        assert!((t.max_rpm - 0.0).abs() < f32::EPSILON);
        assert_eq!(t.num_gears, 0);
        assert!(t.car_id.is_none());
        assert!(t.extended.is_empty());
        Ok(())
    }

    #[test]
    fn old_telemetry_snapshot_without_new_fields() -> TestResult {
        let json = r#"{
            "timestamp_ns": 100,
            "speed_ms": 10.0,
            "steering_angle": 0.0,
            "throttle": 0.3,
            "brake": 0.0,
            "rpm": 3000.0,
            "gear": 2
        }"#;
        let snap: racing_wheel_schemas::telemetry::TelemetrySnapshot =
            serde_json::from_str(json)?;
        assert_eq!(snap.timestamp_ns, 100);
        // serde(default) fields
        assert!((snap.clutch - 0.0).abs() < f32::EPSILON);
        assert_eq!(snap.num_gears, 0);
        assert_eq!(snap.sequence, 0);
        Ok(())
    }

    #[test]
    fn old_config_filter_without_bumpstop_and_hands_off() -> TestResult {
        // Simulate old config that didn't have bumpstop or handsOff sections
        let json = r#"{
            "reconstruction": 0,
            "friction": 0.0,
            "damper": 0.0,
            "inertia": 0.0,
            "notchFilters": [],
            "slewRate": 1.0,
            "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
        }"#;
        let fc: racing_wheel_schemas::config::FilterConfig = serde_json::from_str(json)?;
        // bumpstop and hands_off should use serde(default) values
        assert!(fc.bumpstop.enabled);
        assert!((fc.bumpstop.strength - 0.5).abs() < f32::EPSILON);
        assert!(fc.hands_off.enabled);
        assert!((fc.hands_off.sensitivity - 0.3).abs() < f32::EPSILON);
        assert!(fc.torque_cap.is_none());
        Ok(())
    }

    #[test]
    fn schema_version_roundtrip_stability() -> TestResult {
        let v = SchemaVersion::new(1, 0);
        let json = serde_json::to_string(&v)?;
        let rt: SchemaVersion = serde_json::from_str(&json)?;
        assert_eq!(rt.version, v.version);
        assert_eq!(rt.major, v.major);
        assert_eq!(rt.minor, v.minor);
        Ok(())
    }

    #[test]
    fn profile_migrator_current_version() -> TestResult {
        let json = r#"{
            "schema": "wheel.profile/1",
            "scope": { "game": null, "car": null, "track": null },
            "base": {
                "ffbGain": 0.7,
                "dorDeg": 900,
                "torqueCapNm": 10.0,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.0,
                    "damper": 0.0,
                    "inertia": 0.0,
                    "notchFilters": [],
                    "slewRate": 1.0,
                    "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
                }
            }
        }"#;
        let result = racing_wheel_schemas::config::ProfileMigrator::migrate_profile(json);
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn profile_migrator_rejects_unknown_version() {
        let json = r#"{
            "schema": "wheel.profile/99",
            "scope": { "game": null, "car": null, "track": null },
            "base": {
                "ffbGain": 0.7,
                "dorDeg": 900,
                "torqueCapNm": 10.0,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.0,
                    "damper": 0.0,
                    "inertia": 0.0,
                    "notchFilters": [],
                    "slewRate": 1.0,
                    "curvePoints": [{"input":0.0,"output":0.0},{"input":1.0,"output":1.0}]
                }
            }
        }"#;
        let result = racing_wheel_schemas::config::ProfileMigrator::migrate_profile(json);
        assert!(result.is_err());
    }
}
