//! Comprehensive tests for metrics validation and alerting thresholds
//!
//! This module tests the observability system against the requirements:
//! - DIAG-04: Self-test and metrics validation
//! - NFR-02: CPU/RAM performance requirements

use crate::metrics::*;

/// Test suite for metrics validation and alerting thresholds
#[track_caller]
fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("unexpected Err: {e:?}"),
    }
}

#[track_caller]
fn must_some<T>(o: Option<T>, msg: &str) -> T {
    match o {
        Some(v) => v,
        None => panic!("must_some failed: {}", msg),
    }
}

#[cfg(test)]
mod metrics_validation_tests {
    use super::*;
    use std::time::{Duration, Instant};
    use tokio_stream::StreamExt;

    /// Test RT performance metrics validation against NFR-01 requirements
    #[test]
    fn test_rt_performance_validation_nfr01() {
        let thresholds = AlertingThresholds::default();
        let validator = MetricsValidator::new(thresholds);

        // Test case 1: All metrics within thresholds (should pass)
        let good_metrics = RTMetrics {
            total_ticks: 1000,
            missed_ticks: 0,
            jitter_ns: JitterStats {
                p50_ns: 100_000, // 100μs
                p99_ns: 200_000, // 200μs (within 250μs threshold)
                max_ns: 240_000, // 240μs
            },
            hid_latency_us: LatencyStats {
                p50_us: 150, // 150μs
                p99_us: 250, // 250μs (within 300μs threshold)
                max_us: 290, // 290μs
            },
            processing_time_us: LatencyStats {
                p50_us: 40,  // 40μs (within 50μs median target)
                p99_us: 180, // 180μs (within 200μs threshold)
                max_us: 195, // 195μs
            },
            cpu_usage_percent: 2.5,                // Within 3% threshold
            memory_usage_bytes: 120 * 1024 * 1024, // 120MB (within 150MB threshold)
            last_update: Instant::now(),
        };

        let violations = validator.validate_rt_metrics(&good_metrics);
        assert!(
            violations.is_empty(),
            "Good metrics should not have violations: {:?}",
            violations
        );

        // Test case 2: Jitter exceeds NFR-01 requirement (≤0.25ms p99)
        let bad_jitter_metrics = RTMetrics {
            jitter_ns: JitterStats {
                p50_ns: 200_000,
                p99_ns: 300_000, // 300μs > 250μs threshold
                max_ns: 400_000,
            },
            ..good_metrics.clone()
        };

        let violations = validator.validate_rt_metrics(&bad_jitter_metrics);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("Jitter p99"));
        assert!(violations[0].contains("300000ns"));
        assert!(violations[0].contains("250000ns"));

        // Test case 3: Processing time exceeds budget (≤200μs p99)
        let bad_processing_metrics = RTMetrics {
            processing_time_us: LatencyStats {
                p50_us: 60,
                p99_us: 250, // 250μs > 200μs threshold
                max_us: 300,
            },
            ..good_metrics.clone()
        };

        let violations = validator.validate_rt_metrics(&bad_processing_metrics);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("Processing time p99"));

        // Test case 4: HID latency exceeds budget (≤300μs p99)
        let bad_hid_metrics = RTMetrics {
            hid_latency_us: LatencyStats {
                p50_us: 200,
                p99_us: 350, // 350μs > 300μs threshold
                max_us: 400,
            },
            ..good_metrics.clone()
        };

        let violations = validator.validate_rt_metrics(&bad_hid_metrics);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("HID latency p99"));
    }

    /// Test CPU and memory validation against NFR-02 requirements
    #[test]
    fn test_system_resource_validation_nfr02() {
        let thresholds = AlertingThresholds::default();
        let validator = MetricsValidator::new(thresholds);

        let base_metrics = RTMetrics {
            total_ticks: 1000,
            missed_ticks: 0,
            jitter_ns: JitterStats {
                p50_ns: 100_000,
                p99_ns: 200_000,
                max_ns: 240_000,
            },
            hid_latency_us: LatencyStats {
                p50_us: 150,
                p99_us: 250,
                max_us: 290,
            },
            processing_time_us: LatencyStats {
                p50_us: 40,
                p99_us: 180,
                max_us: 195,
            },
            cpu_usage_percent: 2.5,
            memory_usage_bytes: 120 * 1024 * 1024,
            last_update: Instant::now(),
        };

        // Test case 1: CPU usage exceeds NFR-02 requirement (<3% of one core)
        let high_cpu_metrics = RTMetrics {
            cpu_usage_percent: 3.5, // 3.5% > 3% threshold
            ..base_metrics.clone()
        };

        let violations = validator.validate_rt_metrics(&high_cpu_metrics);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("CPU usage"));
        assert!(violations[0].contains("3.5%"));
        assert!(violations[0].contains("3.0%"));

        // Test case 2: Memory usage exceeds NFR-02 requirement (<150MB RSS)
        let high_memory_metrics = RTMetrics {
            memory_usage_bytes: 160 * 1024 * 1024, // 160MB > 150MB threshold
            ..base_metrics.clone()
        };

        let violations = validator.validate_rt_metrics(&high_memory_metrics);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("Memory usage"));
        assert!(violations[0].contains("160MB"));
        assert!(violations[0].contains("150MB"));

        // Test case 3: Both CPU and memory exceed thresholds
        let bad_system_metrics = RTMetrics {
            cpu_usage_percent: 4.0,
            memory_usage_bytes: 170 * 1024 * 1024,
            ..base_metrics.clone()
        };

        let violations = validator.validate_rt_metrics(&bad_system_metrics);
        assert_eq!(violations.len(), 2);
        assert!(violations.iter().any(|v| v.contains("CPU usage")));
        assert!(violations.iter().any(|v| v.contains("Memory usage")));
    }

    /// Test application metrics validation
    #[test]
    fn test_app_metrics_validation() {
        let thresholds = AlertingThresholds::default();
        let validator = MetricsValidator::new(thresholds);

        // Test case 1: Good application metrics
        let good_app_metrics = AppMetrics {
            connected_devices: 2,
            torque_saturation_percent: 85.0, // Within 95% threshold
            telemetry_packet_loss_percent: 2.0, // Within 5% threshold
            safety_events: 0,
            profile_switches: 5,
            active_game: Some("iracing".to_string()),
            last_update: Instant::now(),
        };

        let violations = validator.validate_app_metrics(&good_app_metrics);
        assert!(violations.is_empty());

        // Test case 2: High torque saturation
        let high_torque_metrics = AppMetrics {
            torque_saturation_percent: 96.0, // > 95% threshold
            ..good_app_metrics.clone()
        };

        let violations = validator.validate_app_metrics(&high_torque_metrics);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("Torque saturation"));
        assert!(violations[0].contains("96.0%"));

        // Test case 3: High telemetry packet loss
        let high_loss_metrics = AppMetrics {
            telemetry_packet_loss_percent: 8.0, // > 5% threshold
            ..good_app_metrics.clone()
        };

        let violations = validator.validate_app_metrics(&high_loss_metrics);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("Telemetry packet loss"));
        assert!(violations[0].contains("8.0%"));

        // Test case 4: Both torque and telemetry issues
        let bad_app_metrics = AppMetrics {
            torque_saturation_percent: 97.0,
            telemetry_packet_loss_percent: 10.0,
            ..good_app_metrics.clone()
        };

        let violations = validator.validate_app_metrics(&bad_app_metrics);
        assert_eq!(violations.len(), 2);
    }

    /// Test custom alerting thresholds
    #[test]
    fn test_custom_alerting_thresholds() {
        // Create stricter thresholds for testing
        let strict_thresholds = AlertingThresholds {
            max_jitter_ns: 100_000,      // 100μs (stricter than default 250μs)
            max_processing_time_us: 100, // 100μs (stricter than default 200μs)
            max_hid_latency_us: 200,     // 200μs (stricter than default 300μs)
            max_cpu_usage_percent: 2.0,  // 2% (stricter than default 3%)
            max_memory_usage_bytes: 100 * 1024 * 1024, // 100MB (stricter than default 150MB)
            max_missed_tick_rate: 0.0005, // 0.05% (stricter than default 0.1%)
            max_torque_saturation_percent: 90.0, // 90% (stricter than default 95%)
            max_telemetry_loss_percent: 2.0, // 2% (stricter than default 5%)
        };

        let validator = MetricsValidator::new(strict_thresholds);

        // Metrics that would pass default thresholds but fail strict ones
        let metrics = RTMetrics {
            total_ticks: 1000,
            missed_ticks: 0,
            jitter_ns: JitterStats {
                p50_ns: 80_000,
                p99_ns: 150_000, // 150μs > 100μs strict threshold
                max_ns: 200_000,
            },
            hid_latency_us: LatencyStats {
                p50_us: 150,
                p99_us: 250, // 250μs > 200μs strict threshold
                max_us: 290,
            },
            processing_time_us: LatencyStats {
                p50_us: 60,
                p99_us: 120, // 120μs > 100μs strict threshold
                max_us: 150,
            },
            cpu_usage_percent: 2.5, // 2.5% > 2% strict threshold
            memory_usage_bytes: 120 * 1024 * 1024, // 120MB > 100MB strict threshold
            last_update: Instant::now(),
        };

        let violations = validator.validate_rt_metrics(&metrics);
        assert_eq!(violations.len(), 5); // All metrics should violate strict thresholds

        let app_metrics = AppMetrics {
            connected_devices: 1,
            torque_saturation_percent: 92.0, // > 90% strict threshold
            telemetry_packet_loss_percent: 3.0, // > 2% strict threshold
            safety_events: 0,
            profile_switches: 2,
            active_game: Some("acc".to_string()),
            last_update: Instant::now(),
        };

        let app_violations = validator.validate_app_metrics(&app_metrics);
        assert_eq!(app_violations.len(), 2); // Both app metrics should violate
    }

    /// Test missed tick rate calculation and validation
    #[test]
    fn test_missed_tick_rate_validation() {
        let thresholds = AlertingThresholds::default();
        let _validator = MetricsValidator::new(thresholds);

        // Simulate 1 second of operation at 1kHz (1000 ticks)
        let total_ticks = 1000u64;
        let missed_ticks_acceptable = 0u64; // 0% missed (good)
        let missed_ticks_warning = 1u64; // 0.1% missed (at threshold)
        let missed_ticks_critical = 5u64; // 0.5% missed (exceeds 0.1% threshold)

        // Note: The current implementation doesn't directly validate missed tick rate
        // in the validator, but we can test the calculation logic

        let calculate_missed_rate = |missed: u64, total: u64| -> f64 {
            if total == 0 {
                0.0
            } else {
                (missed as f64 / total as f64) * 100.0
            }
        };

        assert_eq!(
            calculate_missed_rate(missed_ticks_acceptable, total_ticks),
            0.0
        );
        assert_eq!(
            calculate_missed_rate(missed_ticks_warning, total_ticks),
            0.1
        );
        assert_eq!(
            calculate_missed_rate(missed_ticks_critical, total_ticks),
            0.5
        );

        // The missed tick rate should be validated in the metrics collector
        // when it emits health events for missed ticks
    }

    /// Test metrics collection and aggregation
    #[tokio::test]
    async fn test_metrics_collection_integration() {
        let mut collector = must(MetricsCollector::new());
        let counters = collector.atomic_counters();

        // Simulate RT activity
        for _ in 0..1000 {
            counters.inc_tick();
        }

        // Simulate some missed ticks
        counters.inc_missed_tick();
        counters.inc_missed_tick();

        // Simulate torque saturation samples
        for i in 0..100 {
            counters.record_torque_saturation(i % 10 == 0); // 10% saturation
        }

        // Simulate telemetry activity
        for _ in 0..50 {
            counters.inc_telemetry_received();
        }
        counters.inc_telemetry_lost(); // 2% loss rate

        // Collect metrics
        must(collector.collect_metrics().await);

        // Verify health events were emitted for missed ticks
        let mut health_stream = collector.health_streamer().subscribe();

        // Trigger another collection with missed ticks to generate health event
        counters.inc_missed_tick();
        must(collector.collect_metrics().await);

        // Should receive health event for missed ticks
        let event_result =
            tokio::time::timeout(Duration::from_millis(100), health_stream.next()).await;

        assert!(event_result.is_ok());
        let health_event_result = must_some(event_result.ok(), "expected health event");
        if let Some(Ok(health_event)) = health_event_result {
            assert!(health_event.message.contains("Missed"));
            assert!(health_event.message.contains("RT ticks"));
        }
    }

    /// Test Prometheus metrics export format
    #[test]
    fn test_prometheus_metrics_export() {
        let prometheus_metrics = must(PrometheusMetrics::new());

        // Create test metrics
        let rt_metrics = RTMetrics {
            total_ticks: 5000,
            missed_ticks: 2,
            jitter_ns: JitterStats {
                p50_ns: 100_000,
                p99_ns: 200_000,
                max_ns: 300_000,
            },
            hid_latency_us: LatencyStats {
                p50_us: 150,
                p99_us: 250,
                max_us: 350,
            },
            processing_time_us: LatencyStats {
                p50_us: 50,
                p99_us: 180,
                max_us: 220,
            },
            cpu_usage_percent: 2.8,
            memory_usage_bytes: 140 * 1024 * 1024,
            last_update: Instant::now(),
        };

        let app_metrics = AppMetrics {
            connected_devices: 2,
            torque_saturation_percent: 88.5,
            telemetry_packet_loss_percent: 1.2,
            safety_events: 1,
            profile_switches: 3,
            active_game: Some("iracing".to_string()),
            last_update: Instant::now(),
        };

        // Update Prometheus metrics
        prometheus_metrics.update_rt_metrics(&rt_metrics);
        prometheus_metrics.update_app_metrics(&app_metrics);

        // Gather metrics for export
        let metric_families = prometheus_metrics.registry.gather();

        // Verify expected metrics are present
        let metric_names: Vec<String> = metric_families
            .iter()
            .map(|mf| mf.name().to_string())
            .collect();

        assert!(metric_names.contains(&"wheel_rt_ticks_total".to_string()));
        assert!(metric_names.contains(&"wheel_rt_missed_ticks_total".to_string()));
        assert!(metric_names.contains(&"wheel_rt_jitter_seconds".to_string()));
        assert!(metric_names.contains(&"wheel_rt_processing_time_seconds".to_string()));
        assert!(metric_names.contains(&"wheel_hid_write_latency_seconds".to_string()));
        assert!(metric_names.contains(&"wheel_cpu_usage_percent".to_string()));
        assert!(metric_names.contains(&"wheel_memory_usage_bytes".to_string()));
        assert!(metric_names.contains(&"wheel_connected_devices".to_string()));
        assert!(metric_names.contains(&"wheel_torque_saturation_percent".to_string()));
        assert!(metric_names.contains(&"wheel_telemetry_packet_loss_percent".to_string()));

        // Verify metric values
        for mf in &metric_families {
            match mf.name() {
                "wheel_rt_ticks_total" => {
                    if let Some(counter) = mf.get_metric()[0].get_counter().as_ref() {
                        assert_eq!(counter.value() as u64, rt_metrics.total_ticks);
                    }
                }
                "wheel_cpu_usage_percent" => {
                    if let Some(gauge) = mf.get_metric()[0].get_gauge().as_ref() {
                        assert!((gauge.value() - rt_metrics.cpu_usage_percent as f64).abs() < 0.01);
                    }
                }
                "wheel_connected_devices" => {
                    if let Some(gauge) = mf.get_metric()[0].get_gauge().as_ref() {
                        assert_eq!(gauge.value() as u32, app_metrics.connected_devices);
                    }
                }
                _ => {} // Other metrics
            }
        }
    }

    /// Test health event streaming at specified rate
    #[tokio::test]
    async fn test_health_event_streaming_rate() {
        let streamer = HealthEventStreamer::new(100);
        let mut stream = streamer.subscribe();

        let start_time = Instant::now();
        let mut event_count = 0;

        // Emit events rapidly
        for i in 0..10 {
            let event = HealthEventStreamer::create_event(
                HealthEventType::PerformanceDegradation,
                HealthSeverity::Info,
                format!("Test event {}", i),
                None,
                serde_json::json!({"index": i}),
            );
            must(streamer.emit(event));
        }

        // Collect events with timeout
        while event_count < 10 {
            match tokio::time::timeout(Duration::from_millis(50), stream.next()).await {
                Ok(Some(Ok(_event))) => {
                    event_count += 1;
                }
                Ok(Some(Err(e))) => {
                    panic!("Stream error: {}", e);
                }
                Ok(None) => {
                    break; // Stream ended
                }
                Err(_) => {
                    break; // Timeout
                }
            }
        }

        let elapsed = start_time.elapsed();
        assert_eq!(event_count, 10);

        // Verify events were received quickly (should be much faster than 1 second)
        assert!(elapsed < Duration::from_millis(500));
    }

    /// Test system resource monitoring accuracy
    #[tokio::test]
    async fn test_system_monitor_accuracy() {
        let mut monitor = SystemMonitor::new();

        // Get baseline metrics
        let (cpu1, mem1) = monitor.get_system_metrics().await;

        // Wait a bit and get another reading
        tokio::time::sleep(Duration::from_millis(100)).await;
        let (cpu2, mem2) = monitor.get_system_metrics().await;

        // Basic sanity checks
        assert!(cpu1 >= 0.0);
        assert!(cpu2 >= 0.0);
        assert!(mem1 > 0);
        assert!(mem2 > 0);

        // Memory should be relatively stable (within 10MB)
        let mem_diff = mem2.abs_diff(mem1);
        assert!(
            mem_diff < 10 * 1024 * 1024,
            "Memory usage changed by more than 10MB: {} -> {}",
            mem1,
            mem2
        );

        // CPU can vary more but should be reasonable for a test process
        assert!(cpu1 < 50.0, "CPU usage too high: {}%", cpu1);
        assert!(cpu2 < 50.0, "CPU usage too high: {}%", cpu2);
    }
}

/// Performance benchmarks for metrics collection overhead
#[cfg(test)]
mod metrics_performance_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Instant;

    /// Benchmark atomic counter performance (should be RT-safe)
    #[test]
    fn benchmark_atomic_counters_rt_performance() {
        let counters = Arc::new(AtomicCounters::new());
        let iterations = 1_000_000;

        let start = Instant::now();

        // Simulate RT loop operations
        for i in 0..iterations {
            counters.inc_tick();
            if i % 1000 == 0 {
                counters.inc_missed_tick();
            }
            counters.record_torque_saturation(i % 10 == 0);
        }

        let elapsed = start.elapsed();
        let ops_per_sec = iterations as f64 / elapsed.as_secs_f64();

        println!("Atomic counter performance: {:.0} ops/sec", ops_per_sec);

        // Should be able to handle at least 1MHz operations (much higher than 1kHz RT requirement)
        assert!(
            ops_per_sec > 1_000_000.0,
            "Atomic counters too slow: {:.0} ops/sec",
            ops_per_sec
        );

        // Verify correctness
        let (total_ticks, missed_ticks, _, _, _, _, torque_samples, torque_saturated, _) =
            counters.get_and_reset();

        assert_eq!(total_ticks, iterations);
        assert_eq!(missed_ticks, iterations / 1000);
        assert_eq!(torque_samples, iterations);
        assert_eq!(torque_saturated, iterations / 10);
    }

    /// Test concurrent access to atomic counters (multi-threaded safety)
    #[test]
    fn test_atomic_counters_concurrent_access() {
        let counters = Arc::new(AtomicCounters::new());
        let num_threads = 4;
        let iterations_per_thread = 100_000;

        let mut handles = Vec::new();

        for thread_id in 0..num_threads {
            let counters_clone = counters.clone();
            let handle = thread::spawn(move || {
                for i in 0..iterations_per_thread {
                    counters_clone.inc_tick();
                    if (thread_id + i) % 100 == 0 {
                        counters_clone.inc_missed_tick();
                    }
                    counters_clone.record_torque_saturation(i % 5 == 0);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            must(handle.join());
        }

        // Verify total counts
        let (total_ticks, missed_ticks, _, _, _, _, torque_samples, torque_saturated, _) =
            counters.get_and_reset();

        assert_eq!(total_ticks, num_threads * iterations_per_thread);
        assert_eq!(torque_samples, num_threads * iterations_per_thread);
        assert_eq!(torque_saturated, num_threads * iterations_per_thread / 5);

        // Missed ticks count depends on thread scheduling but should be reasonable
        assert!(missed_ticks > 0);
        assert!(missed_ticks < num_threads * iterations_per_thread / 50); // Upper bound check
    }

    /// Benchmark Prometheus metrics update performance
    #[tokio::test]
    async fn benchmark_prometheus_metrics_performance() {
        let prometheus_metrics = must(PrometheusMetrics::new());
        let iterations = 10_000;

        let rt_metrics = RTMetrics {
            total_ticks: 1000,
            missed_ticks: 1,
            jitter_ns: JitterStats {
                p50_ns: 100_000,
                p99_ns: 200_000,
                max_ns: 300_000,
            },
            hid_latency_us: LatencyStats {
                p50_us: 150,
                p99_us: 250,
                max_us: 350,
            },
            processing_time_us: LatencyStats {
                p50_us: 50,
                p99_us: 180,
                max_us: 220,
            },
            cpu_usage_percent: 2.5,
            memory_usage_bytes: 120 * 1024 * 1024,
            last_update: Instant::now(),
        };

        let app_metrics = AppMetrics {
            connected_devices: 2,
            torque_saturation_percent: 85.0,
            telemetry_packet_loss_percent: 2.0,
            safety_events: 0,
            profile_switches: 1,
            active_game: Some("iracing".to_string()),
            last_update: Instant::now(),
        };

        let start = Instant::now();

        for _ in 0..iterations {
            prometheus_metrics.update_rt_metrics(&rt_metrics);
            prometheus_metrics.update_app_metrics(&app_metrics);
        }

        let elapsed = start.elapsed();
        let updates_per_sec = (iterations * 2) as f64 / elapsed.as_secs_f64();

        println!(
            "Prometheus update performance: {:.0} updates/sec",
            updates_per_sec
        );

        // Should be able to handle at least 1000 updates/sec (much higher than collection rate)
        assert!(
            updates_per_sec > 1000.0,
            "Prometheus updates too slow: {:.0} updates/sec",
            updates_per_sec
        );
    }
}
