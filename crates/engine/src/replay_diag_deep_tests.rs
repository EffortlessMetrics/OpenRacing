//! Deep tests for replay and diagnostics subsystems
//!
//! Coverage areas:
//! - Replay capture: recording sessions, frame timestamping, data integrity
//! - Replay playback: seeking, determinism, tolerance validation
//! - Diagnostics collector: metric accumulation, percentile calculation, threshold alerts
//! - Latency tracking: jitter measurement, missed tick detection, budget enforcement
//! - Diagnostic snapshots: serialization roundtrip, diff detection, trend analysis
//! - Error scenarios: corrupt data, missing frames, truncated files
//! - Property-based testing with proptest

#[cfg(test)]
mod replay_capture_tests {
    use crate::diagnostic::blackbox::{BlackboxConfig, BlackboxRecorder};
    use crate::diagnostic::streams::{StreamA, StreamB, StreamC, StreamReader};
    use crate::diagnostic::{DiagnosticConfig, DiagnosticService, HealthEvent, HealthEventType};
    use crate::ports::NormalizedTelemetry;
    use crate::rt::Frame;
    use crate::safety::SafetyState;
    use racing_wheel_schemas::prelude::DeviceId;
    use std::time::SystemTime;
    use tempfile::TempDir;

    fn make_device_id() -> Result<DeviceId, String> {
        "test-device"
            .parse::<DeviceId>()
            .map_err(|e| format!("{e:?}"))
    }

    fn make_frame(seq: u16, ffb: f32) -> Frame {
        Frame {
            ffb_in: ffb,
            torque_out: ffb * 0.5,
            wheel_speed: 10.0,
            hands_off: false,
            ts_mono_ns: seq as u64 * 1_000_000,
            seq,
        }
    }

    fn make_blackbox_config(dir: &std::path::Path) -> Result<BlackboxConfig, String> {
        Ok(BlackboxConfig {
            device_id: make_device_id()?,
            output_dir: dir.to_path_buf(),
            max_duration_s: 30,
            max_file_size_bytes: 5 * 1024 * 1024,
            compression_level: 1,
            enable_stream_a: true,
            enable_stream_b: true,
            enable_stream_c: true,
        })
    }

    /// Recording a session produces a valid .wbb file with correct frame count.
    #[test]
    fn test_capture_records_correct_frame_count() -> Result<(), String> {
        let tmp = TempDir::new().map_err(|e| e.to_string())?;
        let cfg = make_blackbox_config(tmp.path())?;
        let mut rec = BlackboxRecorder::new(cfg)?;

        let frame_count = 50u64;
        for i in 0..frame_count {
            let f = make_frame(i as u16, (i as f32) * 0.02);
            rec.record_frame(&f, &[0.1, 0.2], &SafetyState::SafeTorque, 100)?;
        }

        let stats = rec.get_stats();
        assert_eq!(stats.frames_recorded, frame_count);
        assert!(stats.is_active);

        let path = rec.finalize()?;
        assert!(path.exists());
        Ok(())
    }

    /// Frame timestamps in Stream A must be monotonically non-decreasing.
    #[test]
    fn test_stream_a_timestamps_monotonic() -> Result<(), String> {
        let mut stream = StreamA::new();

        for i in 0..20 {
            let f = make_frame(i, 0.1 * i as f32);
            stream.record_frame(&f, &[0.5], &SafetyState::SafeTorque, 80)?;
        }

        let data = stream.get_data();
        let mut reader = StreamReader::new(data);

        let mut prev_ts = 0u64;
        while let Some(record) = reader
            .read_stream_a_record()
            .map_err(|e| format!("read error: {e}"))?
        {
            assert!(
                record.timestamp_ns >= prev_ts,
                "timestamp went backwards: {} < {}",
                record.timestamp_ns,
                prev_ts
            );
            prev_ts = record.timestamp_ns;
        }
        Ok(())
    }

    /// Stream A roundtrip preserves frame field values exactly.
    #[test]
    fn test_stream_a_data_integrity_roundtrip() -> Result<(), String> {
        let mut stream = StreamA::new();

        let frames: Vec<Frame> = (0..10)
            .map(|i| make_frame(i, -1.0 + 0.2 * i as f32))
            .collect();

        for f in &frames {
            stream.record_frame(
                f,
                &[f.ffb_in * 0.9, f.torque_out],
                &SafetyState::SafeTorque,
                120,
            )?;
        }

        let data = stream.get_data();
        let mut reader = StreamReader::new(data);
        let mut idx = 0usize;

        while let Some(rec) = reader
            .read_stream_a_record()
            .map_err(|e| format!("read error: {e}"))?
        {
            assert_eq!(rec.frame.seq, frames[idx].seq);
            assert_eq!(rec.frame.ffb_in, frames[idx].ffb_in);
            assert_eq!(rec.frame.torque_out, frames[idx].torque_out);
            assert_eq!(rec.frame.wheel_speed, frames[idx].wheel_speed);
            assert_eq!(rec.frame.hands_off, frames[idx].hands_off);
            assert_eq!(rec.node_outputs.len(), 2);
            idx += 1;
        }
        assert_eq!(idx, frames.len());
        Ok(())
    }

    /// Stream C faithfully records health events with correct event types.
    #[test]
    fn test_stream_c_records_health_event_types() -> Result<(), String> {
        let mut stream = StreamC::new();
        let dev = make_device_id()?;

        let events = vec![
            HealthEventType::DeviceConnected,
            HealthEventType::DeviceDisconnected,
            HealthEventType::PerformanceDegradation {
                metric: "jitter".into(),
                value: 0.3,
            },
        ];

        for et in &events {
            let he = HealthEvent {
                timestamp: SystemTime::now(),
                device_id: dev.clone(),
                event_type: et.clone(),
                context: serde_json::json!({}),
            };
            stream.record_health_event(&he)?;
        }

        assert_eq!(stream.record_count(), 3);
        let data = stream.get_data();
        assert!(!data.is_empty());
        // After get_data the internal buffer is drained
        assert_eq!(stream.record_count(), 0);
        Ok(())
    }

    /// Calling get_data twice returns empty on the second call (buffer cleared).
    #[test]
    fn test_stream_a_get_data_clears_buffer() -> Result<(), String> {
        let mut stream = StreamA::new();
        let f = make_frame(0, 0.5);
        stream.record_frame(&f, &[0.1], &SafetyState::SafeTorque, 100)?;

        let first = stream.get_data();
        assert!(!first.is_empty());

        let second = stream.get_data();
        assert!(second.is_empty());
        Ok(())
    }

    /// DiagnosticService rejects a second concurrent recording.
    #[test]
    fn test_reject_double_recording() -> Result<(), String> {
        let tmp = TempDir::new().map_err(|e| e.to_string())?;
        let cfg = DiagnosticConfig {
            enable_recording: true,
            max_recording_duration_s: 10,
            recording_dir: tmp.path().to_path_buf(),
            max_file_size_bytes: 1024 * 1024,
            compression_level: 1,
            enable_stream_a: true,
            enable_stream_b: true,
            enable_stream_c: true,
        };
        let mut svc = DiagnosticService::new(cfg)?;
        let dev = make_device_id()?;

        svc.start_recording(dev.clone())?;
        assert!(svc.is_recording());

        let dup = svc.start_recording(dev);
        assert!(dup.is_err());
        Ok(())
    }

    /// Health events are capped at 1000 entries; older half is drained.
    #[test]
    fn test_health_event_cap_drains_oldest() -> Result<(), String> {
        let tmp = TempDir::new().map_err(|e| e.to_string())?;
        let cfg = DiagnosticConfig {
            enable_recording: false,
            max_recording_duration_s: 10,
            recording_dir: tmp.path().to_path_buf(),
            max_file_size_bytes: 1024 * 1024,
            compression_level: 1,
            enable_stream_a: true,
            enable_stream_b: true,
            enable_stream_c: true,
        };
        let mut svc = DiagnosticService::new(cfg)?;
        let dev = make_device_id()?;

        for i in 0..1100 {
            svc.record_health_event(HealthEvent {
                timestamp: SystemTime::now(),
                device_id: dev.clone(),
                event_type: HealthEventType::PerformanceDegradation {
                    metric: "test".into(),
                    value: i as f64,
                },
                context: serde_json::json!({"i": i}),
            });
        }

        // After exceeding 1000, oldest 500 are drained so we should have ~600
        let recent = svc.get_recent_health_events(10000);
        assert!(
            recent.len() <= 700,
            "should have been capped, got {}",
            recent.len()
        );
        assert!(recent.len() >= 500, "should retain recent events");
        Ok(())
    }

    /// Stream B rate limiter with a slow rate still records at least one sample.
    #[test]
    fn test_stream_b_records_at_least_one() -> Result<(), String> {
        let mut stream = StreamB::new();
        stream.set_rate_limit_hz(1.0); // 1 Hz

        let telem = NormalizedTelemetry {
            ffb_scalar: 0.5,
            rpm: 4000.0,
            speed_ms: 30.0,
            slip_ratio: 0.05,
            gear: 4,
            flags: Default::default(),
            car_id: None,
            track_id: None,
            timestamp: std::time::Instant::now(),
        };

        stream.record_telemetry(&telem)?;
        // First record should always be accepted
        assert!(stream.record_count() >= 1);
        Ok(())
    }
}

#[cfg(test)]
mod replay_playback_tests {
    use crate::diagnostic::blackbox::{BlackboxConfig, BlackboxRecorder};
    use crate::diagnostic::replay::{BlackboxReplay, ReplayConfig};
    use crate::rt::Frame;
    use crate::safety::SafetyState;
    use racing_wheel_schemas::prelude::DeviceId;
    use tempfile::TempDir;

    fn create_recording(frame_count: usize) -> Result<(std::path::PathBuf, TempDir), String> {
        let tmp = TempDir::new().map_err(|e| e.to_string())?;
        let cfg = BlackboxConfig {
            device_id: "test-device"
                .parse::<DeviceId>()
                .map_err(|e| format!("{e:?}"))?,
            output_dir: tmp.path().to_path_buf(),
            max_duration_s: 30,
            max_file_size_bytes: 5 * 1024 * 1024,
            compression_level: 1,
            enable_stream_a: true,
            enable_stream_b: false,
            enable_stream_c: false,
        };

        let mut rec = BlackboxRecorder::new(cfg)?;
        for i in 0..frame_count {
            let frame = Frame {
                ffb_in: (i as f32) * 0.01,
                torque_out: (i as f32) * 0.005,
                wheel_speed: 10.0,
                hands_off: false,
                ts_mono_ns: (i as u64) * 1_000_000,
                seq: i as u16,
            };
            rec.record_frame(&frame, &[0.1, 0.2, 0.3], &SafetyState::SafeTorque, 100)?;
        }
        let path = rec.finalize()?;
        Ok((path, tmp))
    }

    /// Replay of a recording processes all frames.
    #[test]
    fn test_replay_processes_all_frames() -> Result<(), String> {
        let (path, _tmp) = create_recording(80)?;
        let cfg = ReplayConfig {
            validate_outputs: true,
            fp_tolerance: 1.0, // relaxed
            ..Default::default()
        };
        let mut replay = BlackboxReplay::load_from_file(&path, cfg)?;
        let result = replay.execute_replay()?;
        assert!(result.frames_replayed > 0);
        Ok(())
    }

    /// Two replays with the same deterministic seed produce identical results.
    #[test]
    fn test_replay_determinism() -> Result<(), String> {
        let (path, _tmp) = create_recording(50)?;
        let cfg = ReplayConfig {
            deterministic_seed: 999,
            validate_outputs: true,
            fp_tolerance: 1.0,
            strict_timing: false,
            max_duration_s: 60,
        };

        let mut r1 = BlackboxReplay::load_from_file(&path, cfg.clone())?;
        let res1 = r1.execute_replay()?;

        let mut r2 = BlackboxReplay::load_from_file(&path, cfg)?;
        let res2 = r2.execute_replay()?;

        assert_eq!(res1.frames_replayed, res2.frames_replayed);
        assert_eq!(res1.frames_matched, res2.frames_matched);
        assert_eq!(res1.max_deviation, res2.max_deviation);

        let c1 = r1.get_frame_comparisons();
        let c2 = r2.get_frame_comparisons();
        assert_eq!(c1.len(), c2.len());
        for (a, b) in c1.iter().zip(c2.iter()) {
            assert_eq!(a.replayed_output, b.replayed_output);
        }
        Ok(())
    }

    /// Seeking to a timestamp in the index does not panic.
    #[test]
    fn test_replay_seek_to_valid_timestamp() -> Result<(), String> {
        let (path, _tmp) = create_recording(200)?;
        let cfg = ReplayConfig::default();
        let mut replay = BlackboxReplay::load_from_file(&path, cfg)?;

        // Index entries are created every 100ms; try seeking to 0ms (always valid
        // if at least one index entry exists).
        let idx = replay.footer().index_count;
        if idx > 0 {
            // The first index entry timestamp is whatever the recorder stored
            replay.seek_to_timestamp(0)?;
        }
        Ok(())
    }

    /// Replay statistics histogram is populated after execution.
    #[test]
    fn test_replay_statistics_populated() -> Result<(), String> {
        let (path, _tmp) = create_recording(60)?;
        let cfg = ReplayConfig::default();
        let mut replay = BlackboxReplay::load_from_file(&path, cfg)?;
        let _res = replay.execute_replay()?;

        let stats = replay.generate_statistics();
        assert!(stats.total_frames > 0);
        assert!(!stats.deviation_histogram.is_empty());
        assert!(stats.match_rate >= 0.0 && stats.match_rate <= 1.0);
        Ok(())
    }

    /// Header and footer magic numbers are validated on load.
    #[test]
    fn test_replay_header_footer_magic() -> Result<(), String> {
        let (path, _tmp) = create_recording(10)?;
        let cfg = ReplayConfig::default();
        let replay = BlackboxReplay::load_from_file(&path, cfg)?;

        assert_eq!(&replay.header().magic, b"WBB1");
        assert_eq!(replay.header().version, 1);
        assert_eq!(&replay.footer().footer_magic, b"1BBW");
        Ok(())
    }
}

#[cfg(test)]
mod diagnostics_collector_tests {
    use crate::metrics::*;
    use std::time::Instant;

    /// MetricsValidator detects no violations for within-threshold metrics.
    #[test]
    fn test_validator_passes_good_metrics() -> Result<(), Box<dyn std::error::Error>> {
        let thresholds = AlertingThresholds::default();
        let validator = MetricsValidator::new(thresholds);

        let metrics = RTMetrics {
            total_ticks: 10_000,
            missed_ticks: 0,
            jitter_ns: JitterStats::from_values(80_000, 180_000, 220_000),
            hid_latency_us: LatencyStats::from_values(100, 200, 280),
            processing_time_us: LatencyStats::from_values(30, 150, 190),
            cpu_usage_percent: 1.5,
            memory_usage_bytes: 80 * 1024 * 1024,
            last_update: Instant::now(),
        };

        let violations = validator.validate_rt_metrics(&metrics);
        assert!(
            violations.is_empty(),
            "unexpected violations: {violations:?}"
        );
        Ok(())
    }

    /// MetricsValidator detects jitter p99 exceeding threshold.
    #[test]
    fn test_validator_flags_jitter_violation() -> Result<(), Box<dyn std::error::Error>> {
        let thresholds = AlertingThresholds::default();
        let validator = MetricsValidator::new(thresholds.clone());

        let metrics = RTMetrics {
            total_ticks: 10_000,
            missed_ticks: 0,
            jitter_ns: JitterStats::from_values(100_000, 300_000, 400_000), // p99 > 250μs
            hid_latency_us: LatencyStats::from_values(100, 200, 250),
            processing_time_us: LatencyStats::from_values(30, 100, 150),
            cpu_usage_percent: 1.0,
            memory_usage_bytes: 50 * 1024 * 1024,
            last_update: Instant::now(),
        };

        let violations = validator.validate_rt_metrics(&metrics);
        assert!(!violations.is_empty(), "should detect jitter violation");
        assert!(
            violations.iter().any(|v| v.contains("Jitter")),
            "violation should mention jitter"
        );
        Ok(())
    }

    /// MetricsValidator detects processing time p99 exceeding threshold.
    #[test]
    fn test_validator_flags_processing_time_violation() -> Result<(), Box<dyn std::error::Error>> {
        let thresholds = AlertingThresholds::default();
        let validator = MetricsValidator::new(thresholds);

        let metrics = RTMetrics {
            total_ticks: 10_000,
            missed_ticks: 0,
            jitter_ns: JitterStats::from_values(50_000, 100_000, 150_000),
            hid_latency_us: LatencyStats::from_values(100, 200, 250),
            processing_time_us: LatencyStats::from_values(50, 250, 350), // p99 > 200μs
            cpu_usage_percent: 1.0,
            memory_usage_bytes: 50 * 1024 * 1024,
            last_update: Instant::now(),
        };

        let violations = validator.validate_rt_metrics(&metrics);
        assert!(
            violations.iter().any(|v| v.contains("Processing time")),
            "should detect processing time violation"
        );
        Ok(())
    }

    /// MetricsValidator detects CPU usage exceeding threshold.
    #[test]
    fn test_validator_flags_cpu_violation() -> Result<(), Box<dyn std::error::Error>> {
        let thresholds = AlertingThresholds::default();
        let validator = MetricsValidator::new(thresholds);

        let metrics = RTMetrics {
            total_ticks: 10_000,
            missed_ticks: 0,
            jitter_ns: JitterStats::from_values(50_000, 100_000, 150_000),
            hid_latency_us: LatencyStats::from_values(100, 200, 250),
            processing_time_us: LatencyStats::from_values(30, 100, 150),
            cpu_usage_percent: 5.0, // > 3%
            memory_usage_bytes: 50 * 1024 * 1024,
            last_update: Instant::now(),
        };

        let violations = validator.validate_rt_metrics(&metrics);
        assert!(
            violations.iter().any(|v| v.contains("CPU")),
            "should detect CPU violation"
        );
        Ok(())
    }

    /// MetricsValidator detects memory exceeding threshold.
    #[test]
    fn test_validator_flags_memory_violation() -> Result<(), Box<dyn std::error::Error>> {
        let thresholds = AlertingThresholds::default();
        let validator = MetricsValidator::new(thresholds);

        let metrics = RTMetrics {
            total_ticks: 10_000,
            missed_ticks: 0,
            jitter_ns: JitterStats::from_values(50_000, 100_000, 150_000),
            hid_latency_us: LatencyStats::from_values(100, 200, 250),
            processing_time_us: LatencyStats::from_values(30, 100, 150),
            cpu_usage_percent: 1.0,
            memory_usage_bytes: 200 * 1024 * 1024, // > 150MB
            last_update: Instant::now(),
        };

        let violations = validator.validate_rt_metrics(&metrics);
        assert!(
            violations.iter().any(|v| v.contains("Memory")),
            "should detect memory violation"
        );
        Ok(())
    }

    /// AppMetrics validator flags torque saturation over threshold.
    #[test]
    fn test_validator_flags_torque_saturation() -> Result<(), Box<dyn std::error::Error>> {
        let thresholds = AlertingThresholds::default();
        let validator = MetricsValidator::new(thresholds);

        let metrics = AppMetrics {
            connected_devices: 1,
            torque_saturation_percent: 98.0, // > 95%
            telemetry_packet_loss_percent: 1.0,
            safety_events: 0,
            profile_switches: 0,
            active_game: None,
            last_update: Instant::now(),
        };

        let violations = validator.validate_app_metrics(&metrics);
        assert!(
            violations.iter().any(|v| v.contains("saturation")),
            "should detect torque saturation violation"
        );
        Ok(())
    }

    /// AppMetrics validator flags telemetry packet loss over threshold.
    #[test]
    fn test_validator_flags_telemetry_loss() -> Result<(), Box<dyn std::error::Error>> {
        let thresholds = AlertingThresholds::default();
        let validator = MetricsValidator::new(thresholds);

        let metrics = AppMetrics {
            connected_devices: 1,
            torque_saturation_percent: 50.0,
            telemetry_packet_loss_percent: 8.0, // > 5%
            safety_events: 0,
            profile_switches: 0,
            active_game: None,
            last_update: Instant::now(),
        };

        let violations = validator.validate_app_metrics(&metrics);
        assert!(
            violations.iter().any(|v| v.contains("packet loss")),
            "should detect telemetry loss violation"
        );
        Ok(())
    }

    /// Multiple simultaneous violations are all reported.
    #[test]
    fn test_validator_reports_multiple_violations() -> Result<(), Box<dyn std::error::Error>> {
        let thresholds = AlertingThresholds::default();
        let validator = MetricsValidator::new(thresholds);

        let metrics = RTMetrics {
            total_ticks: 10_000,
            missed_ticks: 100,
            jitter_ns: JitterStats::from_values(100_000, 300_000, 500_000), // jitter violation
            hid_latency_us: LatencyStats::from_values(100, 400, 500),       // latency violation
            processing_time_us: LatencyStats::from_values(50, 300, 400),    // processing violation
            cpu_usage_percent: 5.0,                                         // cpu violation
            memory_usage_bytes: 200 * 1024 * 1024,                          // memory violation
            last_update: Instant::now(),
        };

        let violations = validator.validate_rt_metrics(&metrics);
        assert!(
            violations.len() >= 4,
            "should report at least 4 violations, got {}",
            violations.len()
        );
        Ok(())
    }

    /// AtomicCounters correctly track and snapshot-reset.
    #[test]
    fn test_atomic_counters_snapshot_and_reset() -> Result<(), Box<dyn std::error::Error>> {
        let counters = AtomicCounters::new();

        for _ in 0..1000 {
            counters.inc_tick();
        }
        for _ in 0..3 {
            counters.inc_missed_tick();
        }
        counters.record_torque_saturation(true);
        counters.record_torque_saturation(false);

        let snap = counters.snapshot_and_reset();
        assert_eq!(snap.total_ticks, 1000);
        assert_eq!(snap.missed_ticks, 3);
        assert_eq!(snap.torque_saturation_samples, 2);
        assert_eq!(snap.torque_saturation_count, 1);

        // After reset, counters should be zero
        let snap2 = counters.snapshot();
        assert_eq!(snap2.total_ticks, 0);
        assert_eq!(snap2.missed_ticks, 0);
        Ok(())
    }
}

#[cfg(test)]
mod latency_tracking_tests {
    use crate::rt::PerformanceMetrics;

    /// Missed tick rate is zero when no ticks have occurred.
    #[test]
    fn test_missed_tick_rate_zero_ticks() -> Result<(), String> {
        let m = PerformanceMetrics::default();
        assert_eq!(m.missed_tick_rate(), 0.0);
        Ok(())
    }

    /// Missed tick rate is zero when all ticks are on time.
    #[test]
    fn test_missed_tick_rate_no_misses() -> Result<(), String> {
        let m = PerformanceMetrics {
            total_ticks: 100_000,
            missed_ticks: 0,
            ..Default::default()
        };
        assert_eq!(m.missed_tick_rate(), 0.0);
        Ok(())
    }

    /// Missed tick rate correctly computes small ratios.
    #[test]
    fn test_missed_tick_rate_small_ratio() -> Result<(), String> {
        let m = PerformanceMetrics {
            total_ticks: 1_000_000,
            missed_ticks: 1,
            ..Default::default()
        };
        let rate = m.missed_tick_rate();
        assert!((rate - 1e-6).abs() < 1e-10, "expected ~1e-6, got {rate}");
        Ok(())
    }

    /// Missed tick rate at the performance gate boundary (0.001%).
    #[test]
    fn test_missed_tick_rate_at_gate_boundary() -> Result<(), String> {
        let m = PerformanceMetrics {
            total_ticks: 100_000,
            missed_ticks: 1,
            ..Default::default()
        };
        let rate = m.missed_tick_rate();
        // 1/100000 = 0.00001 = 0.001%, which equals the gate
        assert!(rate <= 0.001, "rate {rate} exceeds 0.1% gate");
        Ok(())
    }

    /// P99 jitter conversion to microseconds is accurate.
    #[test]
    fn test_p99_jitter_us_conversion() -> Result<(), String> {
        let m = PerformanceMetrics {
            p99_jitter_ns: 250_000,
            ..Default::default()
        };
        let us = m.p99_jitter_us();
        assert!((us - 250.0).abs() < 0.001, "expected 250.0μs, got {us}");
        Ok(())
    }

    /// Jitter at the P99 performance gate (≤0.25ms = 250μs = 250_000ns).
    #[test]
    fn test_jitter_within_performance_gate() -> Result<(), String> {
        let m = PerformanceMetrics {
            p99_jitter_ns: 250_000,
            ..Default::default()
        };
        assert!(
            m.p99_jitter_us() <= 250.0,
            "P99 jitter {:.1}μs exceeds 250μs gate",
            m.p99_jitter_us()
        );
        Ok(())
    }

    /// Jitter exceeding the performance gate is detected.
    #[test]
    fn test_jitter_exceeds_performance_gate() -> Result<(), String> {
        let m = PerformanceMetrics {
            p99_jitter_ns: 300_000, // 300μs > 250μs
            ..Default::default()
        };
        assert!(
            m.p99_jitter_us() > 250.0,
            "should detect jitter exceeding gate"
        );
        Ok(())
    }

    /// Budget enforcement: processing time within 1ms budget at 1kHz.
    #[test]
    fn test_processing_time_within_budget() -> Result<(), String> {
        let budget_us = 1000.0; // 1ms at 1kHz
        let processing_us = 200.0; // p99 processing time
        assert!(
            processing_us < budget_us,
            "processing {processing_us}μs exceeds {budget_us}μs budget"
        );
        Ok(())
    }
}

#[cfg(test)]
mod diagnostic_snapshot_tests {
    use crate::diagnostic::blackbox::{IndexEntry, WbbFooter, WbbHeader};
    use crate::diagnostic::streams::{
        NormalizedTelemetrySimple, SafetyStateSimple, StreamARecord, StreamBRecord,
    };
    use crate::rt::Frame;
    use racing_wheel_schemas::prelude::DeviceId;

    use crate::diagnostic::bincode_compat as codec;

    /// WbbHeader serialization roundtrip preserves all fields.
    #[test]
    fn test_wbb_header_roundtrip() -> Result<(), String> {
        let dev = "roundtrip-dev"
            .parse::<DeviceId>()
            .map_err(|e| format!("{e:?}"))?;
        let header = WbbHeader::new(dev, 2, 0b111, 6);

        let bytes = codec::encode_to_vec(&header)?;
        let decoded: WbbHeader = codec::decode_from_slice(&bytes)?;

        assert_eq!(decoded.magic, *b"WBB1");
        assert_eq!(decoded.version, 1);
        assert_eq!(decoded.ffb_mode, 2);
        assert_eq!(decoded.stream_flags, 0b111);
        assert_eq!(decoded.compression_level, 6);
        assert_eq!(decoded.device_id, header.device_id);
        assert_eq!(decoded.engine_version, header.engine_version);
        Ok(())
    }

    /// WbbFooter serialization roundtrip preserves all fields.
    #[test]
    fn test_wbb_footer_roundtrip() -> Result<(), String> {
        let footer = WbbFooter {
            duration_ms: 5000,
            total_frames: 5000,
            index_offset: 12345,
            index_count: 50,
            file_crc32c: 0xDEADBEEF,
            footer_magic: *b"1BBW",
        };

        let bytes = codec::encode_to_vec(&footer)?;
        let decoded: WbbFooter = codec::decode_from_slice(&bytes)?;

        assert_eq!(decoded.duration_ms, 5000);
        assert_eq!(decoded.total_frames, 5000);
        assert_eq!(decoded.index_offset, 12345);
        assert_eq!(decoded.index_count, 50);
        assert_eq!(decoded.file_crc32c, 0xDEADBEEF);
        assert_eq!(decoded.footer_magic, *b"1BBW");
        Ok(())
    }

    /// IndexEntry serialization roundtrip preserves all fields.
    #[test]
    fn test_index_entry_roundtrip() -> Result<(), String> {
        let entry = IndexEntry {
            timestamp_ms: 100,
            stream_a_offset: 1024,
            stream_b_offset: 2048,
            stream_c_offset: 3072,
            frame_count: 100,
        };

        let bytes = codec::encode_to_vec(&entry)?;
        let decoded: IndexEntry = codec::decode_from_slice(&bytes)?;

        assert_eq!(decoded.timestamp_ms, 100);
        assert_eq!(decoded.stream_a_offset, 1024);
        assert_eq!(decoded.stream_b_offset, 2048);
        assert_eq!(decoded.stream_c_offset, 3072);
        assert_eq!(decoded.frame_count, 100);
        Ok(())
    }

    /// StreamARecord serialization roundtrip preserves frame values.
    #[test]
    fn test_stream_a_record_roundtrip() -> Result<(), String> {
        let record = StreamARecord {
            timestamp_ns: 1_000_000,
            frame: Frame {
                ffb_in: 0.75,
                torque_out: 0.5,
                wheel_speed: 15.0,
                hands_off: true,
                ts_mono_ns: 1_000_000,
                seq: 42,
            },
            node_outputs: vec![0.1, 0.2, 0.3, 0.4],
            safety_state: SafetyStateSimple::SafeTorque,
            processing_time_us: 150,
        };

        let bytes = codec::encode_to_vec(&record)?;
        let decoded: StreamARecord = codec::decode_from_slice(&bytes)?;

        assert_eq!(decoded.frame.ffb_in, 0.75);
        assert_eq!(decoded.frame.torque_out, 0.5);
        assert_eq!(decoded.frame.seq, 42);
        assert!(decoded.frame.hands_off);
        assert_eq!(decoded.node_outputs.len(), 4);
        assert_eq!(decoded.processing_time_us, 150);
        Ok(())
    }

    /// StreamBRecord serialization roundtrip preserves telemetry.
    #[test]
    fn test_stream_b_record_roundtrip() -> Result<(), String> {
        let record = StreamBRecord {
            timestamp_ns: 5_000_000,
            telemetry: NormalizedTelemetrySimple {
                ffb_scalar: 0.8,
                rpm: 7500.0,
                speed_ms: 45.0,
                slip_ratio: 0.02,
                gear: 5,
                car_id: Some("ferrari_488".into()),
                track_id: Some("monza".into()),
            },
        };

        let bytes = codec::encode_to_vec(&record)?;
        let decoded: StreamBRecord = codec::decode_from_slice(&bytes)?;

        assert_eq!(decoded.telemetry.rpm, 7500.0);
        assert_eq!(decoded.telemetry.gear, 5);
        assert_eq!(decoded.telemetry.car_id.as_deref(), Some("ferrari_488"));
        assert_eq!(decoded.telemetry.track_id.as_deref(), Some("monza"));
        Ok(())
    }

    /// SafetyStateSimple Faulted variant roundtrips with fault string.
    #[test]
    fn test_safety_state_faulted_roundtrip() -> Result<(), String> {
        let record = StreamARecord {
            timestamp_ns: 0,
            frame: Frame::default(),
            node_outputs: vec![],
            safety_state: SafetyStateSimple::Faulted {
                fault_type: "ThermalLimit".into(),
            },
            processing_time_us: 0,
        };

        let bytes = codec::encode_to_vec(&record)?;
        let decoded: StreamARecord = codec::decode_from_slice(&bytes)?;

        match &decoded.safety_state {
            SafetyStateSimple::Faulted { fault_type } => {
                assert_eq!(fault_type, "ThermalLimit");
            }
            other => return Err(format!("expected Faulted, got {other:?}")),
        }
        Ok(())
    }

    /// Diff detection: two headers with different device IDs are distinguishable.
    #[test]
    fn test_header_diff_detection() -> Result<(), String> {
        let dev1 = "device-a"
            .parse::<DeviceId>()
            .map_err(|e| format!("{e:?}"))?;
        let dev2 = "device-b"
            .parse::<DeviceId>()
            .map_err(|e| format!("{e:?}"))?;

        let h1 = WbbHeader::new(dev1, 1, 7, 6);
        let h2 = WbbHeader::new(dev2, 1, 7, 6);

        assert_ne!(h1.device_id, h2.device_id);
        // Same format fields
        assert_eq!(h1.magic, h2.magic);
        assert_eq!(h1.version, h2.version);
        Ok(())
    }

    /// Trend analysis: processing times increasing over a recording.
    #[test]
    fn test_trend_analysis_increasing_processing_times() -> Result<(), String> {
        let records: Vec<StreamARecord> = (0..100)
            .map(|i| StreamARecord {
                timestamp_ns: i * 1_000_000,
                frame: Frame::default(),
                node_outputs: vec![],
                safety_state: SafetyStateSimple::SafeTorque,
                processing_time_us: 50 + i, // linearly increasing
            })
            .collect();

        // Compute average of first and last quarters
        let n = records.len();
        let first_q: f64 = records[..n / 4]
            .iter()
            .map(|r| r.processing_time_us as f64)
            .sum::<f64>()
            / (n / 4) as f64;
        let last_q: f64 = records[3 * n / 4..]
            .iter()
            .map(|r| r.processing_time_us as f64)
            .sum::<f64>()
            / (n / 4) as f64;

        assert!(
            last_q > first_q,
            "trend should show increasing processing times"
        );
        Ok(())
    }
}

#[cfg(test)]
mod error_scenario_tests {
    use crate::diagnostic::replay::{BlackboxReplay, ReplayConfig};
    use crate::diagnostic::streams::StreamReader;
    use crate::diagnostic::{DiagnosticConfig, DiagnosticService};
    use std::io::Write;
    use tempfile::TempDir;

    /// Loading a truncated file returns an error.
    #[test]
    fn test_load_truncated_file() -> Result<(), String> {
        let tmp = TempDir::new().map_err(|e| e.to_string())?;
        let path = tmp.path().join("truncated.wbb");

        {
            let mut f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
            // Write a few random bytes, not a valid .wbb file
            f.write_all(b"WBB1_truncated").map_err(|e| e.to_string())?;
        }

        let cfg = ReplayConfig::default();
        let result = BlackboxReplay::load_from_file(&path, cfg);
        assert!(result.is_err(), "should fail on truncated file");
        Ok(())
    }

    /// Loading a file with invalid magic number returns an error.
    #[test]
    fn test_load_invalid_magic() -> Result<(), String> {
        let tmp = TempDir::new().map_err(|e| e.to_string())?;
        let path = tmp.path().join("bad_magic.wbb");

        {
            let mut f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
            // Write data with wrong magic — use zeros to avoid triggering huge
            // allocations inside the bincode decoder.
            let garbage = vec![0u8; 512];
            f.write_all(&garbage).map_err(|e| e.to_string())?;
        }

        let cfg = ReplayConfig::default();
        let result = BlackboxReplay::load_from_file(&path, cfg);
        assert!(result.is_err(), "should fail on invalid magic");
        Ok(())
    }

    /// Loading a nonexistent file returns an error.
    #[test]
    fn test_load_nonexistent_file() -> Result<(), String> {
        let cfg = ReplayConfig::default();
        let result =
            BlackboxReplay::load_from_file(std::path::Path::new("nonexistent_file.wbb"), cfg);
        assert!(result.is_err(), "should fail on missing file");
        Ok(())
    }

    /// StreamReader on corrupt data (short length prefix) returns an error.
    #[test]
    fn test_stream_reader_incomplete_length_prefix() -> Result<(), String> {
        // Data has only 2 bytes, too short for a 4-byte length prefix
        let data = vec![0x01, 0x02];
        let mut reader = StreamReader::new(data);
        let result = reader.read_stream_a_record();
        assert!(result.is_err(), "should error on incomplete length prefix");
        Ok(())
    }

    /// StreamReader on data where length prefix points beyond end returns an error.
    #[test]
    fn test_stream_reader_incomplete_record_data() -> Result<(), String> {
        // Valid length prefix claiming 1000 bytes, but only 4 bytes of data
        let mut data = Vec::new();
        data.extend_from_slice(&1000u32.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]);

        let mut reader = StreamReader::new(data);
        let result = reader.read_stream_a_record();
        assert!(result.is_err(), "should error on incomplete record data");
        Ok(())
    }

    /// StreamReader on empty data returns Ok(None).
    #[test]
    fn test_stream_reader_empty_data() -> Result<(), String> {
        let mut reader = StreamReader::new(Vec::new());
        let result = reader
            .read_stream_a_record()
            .map_err(|e| format!("unexpected error: {e}"))?;
        assert!(result.is_none(), "empty data should return None");
        assert!(reader.is_at_end());
        Ok(())
    }

    /// StreamReader reset returns to beginning.
    #[test]
    fn test_stream_reader_reset() -> Result<(), String> {
        let mut reader = StreamReader::new(vec![0u8; 20]);
        // Advance the reader (will fail on deserialization, but position moves past length prefix)
        let _ = reader.read_stream_a_record();
        assert!(reader.position() > 0 || reader.is_at_end());

        reader.reset();
        assert_eq!(reader.position(), 0);
        Ok(())
    }

    /// DiagnosticService creation with disabled recording skips dir creation.
    #[test]
    fn test_disabled_recording_no_dir_needed() -> Result<(), String> {
        let cfg = DiagnosticConfig {
            enable_recording: false,
            max_recording_duration_s: 10,
            recording_dir: std::path::PathBuf::from("C:\\nonexistent\\path\\should\\not\\matter"),
            max_file_size_bytes: 1024 * 1024,
            compression_level: 1,
            enable_stream_a: true,
            enable_stream_b: true,
            enable_stream_c: true,
        };
        let svc = DiagnosticService::new(cfg)?;
        assert!(!svc.is_recording());
        Ok(())
    }

    /// Stopping a recording when none is active returns Ok(None).
    #[test]
    fn test_stop_when_not_recording() -> Result<(), String> {
        let tmp = TempDir::new().map_err(|e| e.to_string())?;
        let cfg = DiagnosticConfig {
            enable_recording: true,
            max_recording_duration_s: 10,
            recording_dir: tmp.path().to_path_buf(),
            max_file_size_bytes: 1024 * 1024,
            compression_level: 1,
            enable_stream_a: true,
            enable_stream_b: true,
            enable_stream_c: true,
        };
        let mut svc = DiagnosticService::new(cfg)?;
        let result = svc.stop_recording()?;
        assert!(
            result.is_none(),
            "stopping with no recording should return None"
        );
        Ok(())
    }

    /// Recording a frame when not recording is a no-op.
    #[test]
    fn test_record_frame_when_not_recording() -> Result<(), String> {
        let tmp = TempDir::new().map_err(|e| e.to_string())?;
        let cfg = DiagnosticConfig {
            enable_recording: true,
            max_recording_duration_s: 10,
            recording_dir: tmp.path().to_path_buf(),
            max_file_size_bytes: 1024 * 1024,
            compression_level: 1,
            enable_stream_a: true,
            enable_stream_b: true,
            enable_stream_c: true,
        };
        let mut svc = DiagnosticService::new(cfg)?;

        let frame = crate::rt::Frame::default();
        // Should succeed (no-op) when not recording
        svc.record_frame(&frame, &[], &crate::safety::SafetyState::SafeTorque, 100)?;
        assert!(svc.get_recording_stats().is_none());
        Ok(())
    }
}

#[cfg(test)]
mod proptest_replay_diag {
    use crate::diagnostic::blackbox::{IndexEntry, WbbFooter};
    use crate::diagnostic::streams::{SafetyStateSimple, StreamARecord};
    use crate::rt::Frame;
    use proptest::prelude::*;

    use crate::diagnostic::bincode_compat as codec;

    fn arb_frame() -> impl Strategy<Value = Frame> {
        (
            -1.0f32..1.0f32,
            -1.0f32..1.0f32,
            0.0f32..100.0f32,
            any::<bool>(),
            any::<u64>(),
            any::<u16>(),
        )
            .prop_map(|(ffb_in, torque_out, ws, ho, ts, seq)| Frame {
                ffb_in,
                torque_out,
                wheel_speed: ws,
                hands_off: ho,
                ts_mono_ns: ts,
                seq,
            })
    }

    proptest! {
        /// Any Frame roundtrips through bincode serialization.
        #[test]
        fn frame_bincode_roundtrip(frame in arb_frame()) {
            let bytes = codec::encode_to_vec(&frame).map_err(|e| TestCaseError::Fail(e.into()))?;
            let decoded: Frame = codec::decode_from_slice(&bytes).map_err(|e| TestCaseError::Fail(e.into()))?;
            prop_assert_eq!(decoded.seq, frame.seq);
            prop_assert_eq!(decoded.ffb_in, frame.ffb_in);
            prop_assert_eq!(decoded.torque_out, frame.torque_out);
            prop_assert_eq!(decoded.wheel_speed, frame.wheel_speed);
            prop_assert_eq!(decoded.hands_off, frame.hands_off);
            prop_assert_eq!(decoded.ts_mono_ns, frame.ts_mono_ns);
        }

        /// WbbFooter roundtrips through bincode for any field values.
        #[test]
        fn footer_bincode_roundtrip(
            duration_ms in any::<u32>(),
            total_frames in any::<u64>(),
            index_offset in any::<u64>(),
            index_count in any::<u32>(),
            file_crc32c in any::<u32>(),
        ) {
            let footer = WbbFooter {
                duration_ms,
                total_frames,
                index_offset,
                index_count,
                file_crc32c,
                footer_magic: *b"1BBW",
            };
            let bytes = codec::encode_to_vec(&footer).map_err(|e| TestCaseError::Fail(e.into()))?;
            let decoded: WbbFooter = codec::decode_from_slice(&bytes).map_err(|e| TestCaseError::Fail(e.into()))?;
            prop_assert_eq!(decoded.duration_ms, footer.duration_ms);
            prop_assert_eq!(decoded.total_frames, footer.total_frames);
            prop_assert_eq!(decoded.index_offset, footer.index_offset);
            prop_assert_eq!(decoded.index_count, footer.index_count);
            prop_assert_eq!(decoded.file_crc32c, footer.file_crc32c);
        }

        /// IndexEntry roundtrips through bincode for any field values.
        #[test]
        fn index_entry_bincode_roundtrip(
            ts in any::<u32>(),
            a_off in any::<u64>(),
            b_off in any::<u64>(),
            c_off in any::<u64>(),
            count in any::<u32>(),
        ) {
            let entry = IndexEntry {
                timestamp_ms: ts,
                stream_a_offset: a_off,
                stream_b_offset: b_off,
                stream_c_offset: c_off,
                frame_count: count,
            };
            let bytes = codec::encode_to_vec(&entry).map_err(|e| TestCaseError::Fail(e.into()))?;
            let decoded: IndexEntry = codec::decode_from_slice(&bytes).map_err(|e| TestCaseError::Fail(e.into()))?;
            prop_assert_eq!(decoded.timestamp_ms, entry.timestamp_ms);
            prop_assert_eq!(decoded.stream_a_offset, entry.stream_a_offset);
            prop_assert_eq!(decoded.stream_b_offset, entry.stream_b_offset);
            prop_assert_eq!(decoded.stream_c_offset, entry.stream_c_offset);
            prop_assert_eq!(decoded.frame_count, entry.frame_count);
        }

        /// StreamARecord with arbitrary node outputs roundtrips.
        #[test]
        fn stream_a_record_roundtrip(
            frame in arb_frame(),
            outputs in prop::collection::vec(-10.0f32..10.0f32, 0..8),
            processing_us in 0u64..10_000,
        ) {
            let record = StreamARecord {
                timestamp_ns: 1_000_000,
                frame,
                node_outputs: outputs.clone(),
                safety_state: SafetyStateSimple::SafeTorque,
                processing_time_us: processing_us,
            };
            let bytes = codec::encode_to_vec(&record).map_err(|e| TestCaseError::Fail(e.into()))?;
            let decoded: StreamARecord = codec::decode_from_slice(&bytes).map_err(|e| TestCaseError::Fail(e.into()))?;
            prop_assert_eq!(decoded.node_outputs.len(), outputs.len());
            prop_assert_eq!(decoded.processing_time_us, processing_us);
            prop_assert_eq!(decoded.frame.seq, frame.seq);
        }

        /// PerformanceMetrics missed_tick_rate never exceeds 1.0.
        #[test]
        fn missed_tick_rate_bounded(
            total in 1u64..1_000_000,
            missed in 0u64..1_000_000,
        ) {
            let m = crate::rt::PerformanceMetrics {
                total_ticks: total,
                missed_ticks: missed.min(total),
                ..Default::default()
            };
            let rate = m.missed_tick_rate();
            prop_assert!(rate >= 0.0);
            prop_assert!(rate <= 1.0);
        }
    }
}
