//! CI Performance Gates
//! 
//! This module implements the performance gates that must pass in CI:
//! - p99 jitter ≤0.25ms at 1kHz
//! - HID write latency p99 ≤300μs
//! - No missed ticks over test duration

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use anyhow::Result;
use hdrhistogram::Histogram;
use tracing::{info, warn, error};

use crate::common::{TestHarness, TimingUtils, RTTimer};
use crate::{TestConfig, TestResult, PerformanceMetrics, MAX_JITTER_P99_MS, MAX_HID_LATENCY_P99_US, FFB_FREQUENCY_HZ};

/// Run all CI performance gates
pub async fn run_ci_performance_gates() -> Result<Vec<TestResult>> {
    info!("Running CI performance gates");
    
    let mut results = Vec::new();
    
    // Gate 1: FFB Jitter Test
    results.push(test_ffb_jitter_gate().await?);
    
    // Gate 2: HID Write Latency Test  
    results.push(test_hid_latency_gate().await?);
    
    // Gate 3: Zero Missed Ticks Test
    results.push(test_zero_missed_ticks_gate().await?);
    
    // Gate 4: Combined Load Test
    results.push(test_combined_load_gate().await?);
    
    Ok(results)
}

/// Test FFB jitter performance gate: p99 ≤0.25ms at 1kHz
pub async fn test_ffb_jitter_gate() -> Result<TestResult> {
    info!("Testing FFB jitter performance gate (p99 ≤{}ms)", MAX_JITTER_P99_MS);
    
    let config = TestConfig {
        duration: Duration::from_secs(60), // 1 minute = 60,000 samples
        sample_rate_hz: FFB_FREQUENCY_HZ,
        virtual_device: true,
        enable_tracing: false, // Reduce overhead
        ..Default::default()
    };
    
    let mut harness = TestHarness::new(config.clone()).await?;
    let mut errors = Vec::new();
    let start_time = Instant::now();
    
    harness.start_service().await?;
    
    // Create high-resolution histogram for jitter measurements
    let mut jitter_histogram = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3)?; // 1ns to 10ms
    let mut timer = RTTimer::new(FFB_FREQUENCY_HZ);
    let mut missed_ticks = 0u64;
    let mut total_ticks = 0u64;
    
    let test_duration = config.duration;
    let end_time = start_time + test_duration;
    
    info!("Starting jitter measurement for {:?}", test_duration);
    
    while Instant::now() < end_time {
        let jitter = timer.wait_for_next_tick().await;
        total_ticks += 1;
        
        let jitter_ns = jitter.as_nanos() as u64;
        
        if jitter > Duration::from_micros(500) { // Consider >500μs as missed tick
            missed_ticks += 1;
        }
        
        // Record jitter in histogram (convert to nanoseconds)
        if let Err(e) = jitter_histogram.record(jitter_ns) {
            warn!("Failed to record jitter sample: {}", e);
        }
        
        // Simulate FFB processing work
        simulate_ffb_processing().await;
    }
    
    let actual_duration = start_time.elapsed();
    info!("Jitter test completed: {} ticks in {:?}", total_ticks, actual_duration);
    
    // Calculate statistics
    let p50_jitter_ns = jitter_histogram.value_at_quantile(0.5);
    let p99_jitter_ns = jitter_histogram.value_at_quantile(0.99);
    let max_jitter_ns = jitter_histogram.max();
    
    let p50_jitter_ms = p50_jitter_ns as f64 / 1_000_000.0;
    let p99_jitter_ms = p99_jitter_ns as f64 / 1_000_000.0;
    let max_jitter_ms = max_jitter_ns as f64 / 1_000_000.0;
    
    info!("Jitter Statistics:");
    info!("  P50: {:.3}ms", p50_jitter_ms);
    info!("  P99: {:.3}ms (gate: ≤{:.3}ms)", p99_jitter_ms, MAX_JITTER_P99_MS);
    info!("  Max: {:.3}ms", max_jitter_ms);
    info!("  Missed ticks: {} / {} ({:.6}%)", missed_ticks, total_ticks, 
          (missed_ticks as f64 / total_ticks as f64) * 100.0);
    
    // Check performance gate
    if p99_jitter_ms > MAX_JITTER_P99_MS {
        errors.push(format!("P99 jitter {:.3}ms exceeds gate of {:.3}ms", 
                           p99_jitter_ms, MAX_JITTER_P99_MS));
    }
    
    if missed_ticks > 0 {
        errors.push(format!("Missed {} ticks out of {}", missed_ticks, total_ticks));
    }
    
    let metrics = PerformanceMetrics {
        jitter_p50_ms: p50_jitter_ms,
        jitter_p99_ms: p99_jitter_ms,
        missed_ticks,
        total_ticks,
        ..Default::default()
    };
    
    harness.shutdown().await?;
    
    Ok(TestResult {
        passed: errors.is_empty(),
        duration: actual_duration,
        metrics,
        errors,
        requirement_coverage: vec!["FFB-01".to_string(), "NFR-01".to_string()],
    })
}

/// Test HID write latency performance gate: p99 ≤300μs
pub async fn test_hid_latency_gate() -> Result<TestResult> {
    info!("Testing HID write latency performance gate (p99 ≤{}μs)", MAX_HID_LATENCY_P99_US);
    
    let config = TestConfig {
        duration: Duration::from_secs(30),
        sample_rate_hz: FFB_FREQUENCY_HZ,
        virtual_device: true,
        enable_tracing: false,
        ..Default::default()
    };
    
    let mut harness = TestHarness::new(config.clone()).await?;
    let mut errors = Vec::new();
    let start_time = Instant::now();
    
    harness.start_service().await?;
    
    // Create histogram for HID write latency measurements
    let mut latency_histogram = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3)?; // 1ns to 10ms
    let mut timer = RTTimer::new(FFB_FREQUENCY_HZ);
    let mut total_writes = 0u64;
    
    let test_duration = config.duration;
    let end_time = start_time + test_duration;
    
    info!("Starting HID write latency measurement for {:?}", test_duration);
    
    while Instant::now() < end_time {
        timer.wait_for_next_tick().await;
        
        // Measure HID write latency
        let write_start = Instant::now();
        simulate_hid_write().await;
        let write_latency = write_start.elapsed();
        
        total_writes += 1;
        
        let latency_ns = write_latency.as_nanos() as u64;
        if let Err(e) = latency_histogram.record(latency_ns) {
            warn!("Failed to record latency sample: {}", e);
        }
    }
    
    let actual_duration = start_time.elapsed();
    info!("HID latency test completed: {} writes in {:?}", total_writes, actual_duration);
    
    // Calculate statistics
    let p50_latency_ns = latency_histogram.value_at_quantile(0.5);
    let p99_latency_ns = latency_histogram.value_at_quantile(0.99);
    let max_latency_ns = latency_histogram.max();
    
    let p50_latency_us = p50_latency_ns as f64 / 1_000.0;
    let p99_latency_us = p99_latency_ns as f64 / 1_000.0;
    let max_latency_us = max_latency_ns as f64 / 1_000.0;
    
    info!("HID Write Latency Statistics:");
    info!("  P50: {:.1}μs", p50_latency_us);
    info!("  P99: {:.1}μs (gate: ≤{:.1}μs)", p99_latency_us, MAX_HID_LATENCY_P99_US);
    info!("  Max: {:.1}μs", max_latency_us);
    
    // Check performance gate
    if p99_latency_us > MAX_HID_LATENCY_P99_US {
        errors.push(format!("P99 HID latency {:.1}μs exceeds gate of {:.1}μs", 
                           p99_latency_us, MAX_HID_LATENCY_P99_US));
    }
    
    let metrics = PerformanceMetrics {
        hid_latency_p50_us: p50_latency_us,
        hid_latency_p99_us: p99_latency_us,
        total_ticks: total_writes,
        ..Default::default()
    };
    
    harness.shutdown().await?;
    
    Ok(TestResult {
        passed: errors.is_empty(),
        duration: actual_duration,
        metrics,
        errors,
        requirement_coverage: vec!["FFB-01".to_string(), "NFR-01".to_string()],
    })
}

/// Test zero missed ticks gate
pub async fn test_zero_missed_ticks_gate() -> Result<TestResult> {
    info!("Testing zero missed ticks performance gate");
    
    let config = TestConfig {
        duration: Duration::from_secs(120), // 2 minutes for thorough test
        sample_rate_hz: FFB_FREQUENCY_HZ,
        virtual_device: true,
        enable_tracing: false,
        ..Default::default()
    };
    
    let mut harness = TestHarness::new(config.clone()).await?;
    let mut errors = Vec::new();
    let start_time = Instant::now();
    
    harness.start_service().await?;
    
    let mut timer = RTTimer::new(FFB_FREQUENCY_HZ);
    let mut missed_ticks = 0u64;
    let mut total_ticks = 0u64;
    let mut max_jitter = Duration::ZERO;
    
    let test_duration = config.duration;
    let end_time = start_time + test_duration;
    
    info!("Starting zero missed ticks test for {:?}", test_duration);
    
    while Instant::now() < end_time {
        let jitter = timer.wait_for_next_tick().await;
        total_ticks += 1;
        
        if jitter > max_jitter {
            max_jitter = jitter;
        }
        
        // Consider a tick "missed" if jitter exceeds 1ms (very conservative)
        if jitter > Duration::from_millis(1) {
            missed_ticks += 1;
            warn!("Missed tick {} with jitter: {:?}", total_ticks, jitter);
        }
        
        // Simulate realistic FFB processing load
        simulate_ffb_processing().await;
        simulate_hid_write().await;
    }
    
    let actual_duration = start_time.elapsed();
    info!("Zero missed ticks test completed: {} ticks in {:?}", total_ticks, actual_duration);
    info!("Missed ticks: {} / {} ({:.6}%)", missed_ticks, total_ticks, 
          (missed_ticks as f64 / total_ticks as f64) * 100.0);
    info!("Maximum jitter: {:?}", max_jitter);
    
    // Check performance gate
    if missed_ticks > 0 {
        errors.push(format!("Missed {} ticks out of {} (gate: 0 missed ticks)", 
                           missed_ticks, total_ticks));
    }
    
    let metrics = PerformanceMetrics {
        missed_ticks,
        total_ticks,
        jitter_p99_ms: max_jitter.as_secs_f64() * 1000.0,
        ..Default::default()
    };
    
    harness.shutdown().await?;
    
    Ok(TestResult {
        passed: errors.is_empty(),
        duration: actual_duration,
        metrics,
        errors,
        requirement_coverage: vec!["FFB-01".to_string(), "NFR-01".to_string(), "NFR-03".to_string()],
    })
}

/// Test combined load performance gate (all systems active)
pub async fn test_combined_load_gate() -> Result<TestResult> {
    info!("Testing combined load performance gate");
    
    let config = TestConfig {
        duration: Duration::from_secs(180), // 3 minutes under full load
        sample_rate_hz: FFB_FREQUENCY_HZ,
        virtual_device: true,
        enable_tracing: true, // Enable all systems
        enable_metrics: true,
        ..Default::default()
    };
    
    let mut harness = TestHarness::new(config.clone()).await?;
    let mut errors = Vec::new();
    let start_time = Instant::now();
    
    harness.start_service().await?;
    
    let mut jitter_histogram = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3)?;
    let mut latency_histogram = Histogram::<u64>::new_with_bounds(1, 10_000_000, 3)?;
    let mut timer = RTTimer::new(FFB_FREQUENCY_HZ);
    let mut missed_ticks = 0u64;
    let mut total_ticks = 0u64;
    
    let test_duration = config.duration;
    let end_time = start_time + test_duration;
    
    info!("Starting combined load test for {:?}", test_duration);
    
    // Simulate background load (telemetry, LEDs, diagnostics)
    let _background_tasks = tokio::spawn(async {
        loop {
            simulate_telemetry_processing().await;
            simulate_led_updates().await;
            simulate_diagnostics_collection().await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    
    while Instant::now() < end_time {
        let jitter = timer.wait_for_next_tick().await;
        total_ticks += 1;
        
        let jitter_ns = jitter.as_nanos() as u64;
        jitter_histogram.record(jitter_ns).ok();
        
        if jitter > Duration::from_micros(500) {
            missed_ticks += 1;
        }
        
        // Measure HID write under load
        let write_start = Instant::now();
        simulate_ffb_processing().await;
        simulate_hid_write().await;
        let write_latency = write_start.elapsed();
        
        let latency_ns = write_latency.as_nanos() as u64;
        latency_histogram.record(latency_ns).ok();
    }
    
    let actual_duration = start_time.elapsed();
    info!("Combined load test completed: {} ticks in {:?}", total_ticks, actual_duration);
    
    // Calculate statistics
    let p99_jitter_ms = jitter_histogram.value_at_quantile(0.99) as f64 / 1_000_000.0;
    let p99_latency_us = latency_histogram.value_at_quantile(0.99) as f64 / 1_000.0;
    
    info!("Combined Load Statistics:");
    info!("  P99 Jitter: {:.3}ms (gate: ≤{:.3}ms)", p99_jitter_ms, MAX_JITTER_P99_MS);
    info!("  P99 HID Latency: {:.1}μs (gate: ≤{:.1}μs)", p99_latency_us, MAX_HID_LATENCY_P99_US);
    info!("  Missed ticks: {} / {}", missed_ticks, total_ticks);
    
    // Check all performance gates under load
    if p99_jitter_ms > MAX_JITTER_P99_MS {
        errors.push(format!("P99 jitter under load {:.3}ms exceeds gate of {:.3}ms", 
                           p99_jitter_ms, MAX_JITTER_P99_MS));
    }
    
    if p99_latency_us > MAX_HID_LATENCY_P99_US {
        errors.push(format!("P99 HID latency under load {:.1}μs exceeds gate of {:.1}μs", 
                           p99_latency_us, MAX_HID_LATENCY_P99_US));
    }
    
    if missed_ticks > 0 {
        errors.push(format!("Missed {} ticks under load", missed_ticks));
    }
    
    let metrics = PerformanceMetrics {
        jitter_p99_ms: p99_jitter_ms,
        hid_latency_p99_us: p99_latency_us,
        missed_ticks,
        total_ticks,
        ..Default::default()
    };
    
    harness.shutdown().await?;
    
    Ok(TestResult {
        passed: errors.is_empty(),
        duration: actual_duration,
        metrics,
        errors,
        requirement_coverage: vec![
            "FFB-01".to_string(), 
            "NFR-01".to_string(), 
            "NFR-02".to_string(),
            "NFR-03".to_string(),
        ],
    })
}

// Simulation functions for realistic load testing

async fn simulate_ffb_processing() {
    // Simulate ~50μs of FFB filter processing
    let start = Instant::now();
    while start.elapsed() < Duration::from_micros(50) {
        // Busy wait to simulate CPU-intensive work
        std::hint::spin_loop();
    }
}

async fn simulate_hid_write() {
    // Simulate HID write operation (~100μs typical)
    tokio::time::sleep(Duration::from_micros(100)).await;
}

async fn simulate_telemetry_processing() {
    // Simulate telemetry parsing and normalization
    tokio::time::sleep(Duration::from_micros(200)).await;
}

async fn simulate_led_updates() {
    // Simulate LED pattern updates
    tokio::time::sleep(Duration::from_micros(50)).await;
}

async fn simulate_diagnostics_collection() {
    // Simulate metrics collection
    tokio::time::sleep(Duration::from_micros(30)).await;
}