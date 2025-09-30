//! Soak testing for 48-hour continuous operation validation

use std::time::{Duration, Instant};
use anyhow::Result;
use tracing::{info, warn, error};
use tokio::fs;
use serde::{Serialize, Deserialize};

use crate::common::{TestHarness, RTTimer};
use crate::{TestConfig, TestResult, PerformanceMetrics, SOAK_TEST_DURATION};

/// Soak test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoakTestConfig {
    pub duration: Duration,
    pub checkpoint_interval: Duration,
    pub metrics_collection_interval: Duration,
    pub enable_full_logging: bool,
    pub enable_blackbox_recording: bool,
    pub max_memory_mb: f64,
    pub max_cpu_percent: f64,
}

impl Default for SoakTestConfig {
    fn default() -> Self {
        Self {
            duration: SOAK_TEST_DURATION,
            checkpoint_interval: Duration::from_secs(3600), // 1 hour checkpoints
            metrics_collection_interval: Duration::from_secs(60), // 1 minute metrics
            enable_full_logging: false, // Reduce I/O overhead
            enable_blackbox_recording: true,
            max_memory_mb: 150.0,
            max_cpu_percent: 3.0,
        }
    }
}

/// Soak test checkpoint data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoakCheckpoint {
    pub timestamp: std::time::SystemTime,
    pub elapsed: Duration,
    pub metrics: PerformanceMetrics,
    pub total_ticks: u64,
    pub missed_ticks: u64,
    pub errors: Vec<String>,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
}

/// Run 48-hour soak test with continuous monitoring
pub async fn run_soak_test() -> Result<TestResult> {
    info!("Starting 48-hour soak test");
    
    let soak_config = SoakTestConfig::default();
    let test_config = TestConfig {
        duration: soak_config.duration,
        virtual_device: true,
        enable_tracing: soak_config.enable_full_logging,
        enable_metrics: true,
        ..Default::default()
    };
    
    let mut harness = TestHarness::new(test_config.clone()).await?;
    let mut errors = Vec::new();
    let start_time = Instant::now();
    
    // Create soak test directory for checkpoints
    let soak_dir = "target/soak-test";
    fs::create_dir_all(soak_dir).await?;
    
    harness.start_service().await?;
    
    let mut checkpoints = Vec::new();
    let mut total_ticks = 0u64;
    let mut total_missed_ticks = 0u64;
    let mut last_checkpoint = start_time;
    let mut last_metrics_collection = start_time;
    
    let mut timer = RTTimer::new(crate::FFB_FREQUENCY_HZ);
    let end_time = start_time + soak_config.duration;
    
    info!("Soak test will run for {:?} ({:.1} hours)", 
          soak_config.duration, 
          soak_config.duration.as_secs_f64() / 3600.0);
    
    while Instant::now() < end_time {
        // RT loop simulation
        let jitter = timer.wait_for_next_tick().await;
        total_ticks += 1;
        
        if jitter > Duration::from_micros(500) {
            total_missed_ticks += 1;
        }
        
        // Simulate FFB processing
        simulate_soak_ffb_processing().await;
        
        let now = Instant::now();
        
        // Collect metrics periodically
        if now.duration_since(last_metrics_collection) >= soak_config.metrics_collection_interval {
            let current_metrics = harness.collect_metrics().await;
            
            // Check for performance degradation
            if current_metrics.jitter_p99_ms > crate::MAX_JITTER_P99_MS {
                warn!("Jitter degradation detected: {:.3}ms at {:?}", 
                      current_metrics.jitter_p99_ms, now.duration_since(start_time));
            }
            
            if current_metrics.memory_usage_mb > soak_config.max_memory_mb {
                errors.push(format!("Memory usage {:.1}MB exceeded limit at {:?}", 
                                   current_metrics.memory_usage_mb, 
                                   now.duration_since(start_time)));
            }
            
            if current_metrics.cpu_usage_percent > soak_config.max_cpu_percent {
                warn!("CPU usage {:.1}% exceeded limit at {:?}", 
                      current_metrics.cpu_usage_percent, 
                      now.duration_since(start_time));
            }
            
            last_metrics_collection = now;
        }
        
        // Create checkpoint periodically
        if now.duration_since(last_checkpoint) >= soak_config.checkpoint_interval {
            let checkpoint = create_checkpoint(
                &mut harness,
                start_time,
                total_ticks,
                total_missed_ticks,
                &errors
            ).await?;
            
            // Save checkpoint to disk
            let checkpoint_file = format!("{}/checkpoint_{:04}.json", 
                                        soak_dir, checkpoints.len());
            let checkpoint_json = serde_json::to_string_pretty(&checkpoint)?;
            fs::write(&checkpoint_file, checkpoint_json).await?;
            
            info!("Checkpoint {}: {:?} elapsed, {} ticks, {} missed, {:.1}MB memory", 
                  checkpoints.len(),
                  checkpoint.elapsed,
                  checkpoint.total_ticks,
                  checkpoint.missed_ticks,
                  checkpoint.memory_usage_mb);
            
            checkpoints.push(checkpoint);
            last_checkpoint = now;
            
            // Check for early termination conditions
            if should_terminate_early(&checkpoints) {
                error!("Early termination due to performance degradation");
                errors.push("Early termination due to performance issues".to_string());
                break;
            }
        }
        
        // Simulate background activities periodically
        if total_ticks % 60000 == 0 { // Every minute at 1kHz
            simulate_background_activities().await;
        }
    }
    
    let actual_duration = start_time.elapsed();
    let final_metrics = harness.collect_metrics().await;
    
    // Create final summary
    let summary = create_soak_summary(&checkpoints, actual_duration, total_ticks, total_missed_ticks);
    let summary_file = format!("{}/soak_summary.json", soak_dir);
    let summary_json = serde_json::to_string_pretty(&summary)?;
    fs::write(&summary_file, summary_json).await?;
    
    info!("Soak test completed:");
    info!("  Duration: {:?} ({:.1} hours)", actual_duration, actual_duration.as_secs_f64() / 3600.0);
    info!("  Total ticks: {}", total_ticks);
    info!("  Missed ticks: {} ({:.6}%)", total_missed_ticks, 
          (total_missed_ticks as f64 / total_ticks as f64) * 100.0);
    info!("  Checkpoints created: {}", checkpoints.len());
    info!("  Final memory usage: {:.1}MB", final_metrics.memory_usage_mb);
    
    // Validate soak test success criteria
    let success_criteria_met = validate_soak_success_criteria(
        &checkpoints, 
        actual_duration, 
        total_ticks, 
        total_missed_ticks
    );
    
    if !success_criteria_met {
        errors.push("Soak test success criteria not met".to_string());
    }
    
    harness.shutdown().await?;
    
    Ok(TestResult {
        passed: errors.is_empty() && success_criteria_met,
        duration: actual_duration,
        metrics: final_metrics,
        errors,
        requirement_coverage: vec![
            "NFR-03".to_string(),
            "FFB-01".to_string(),
            "NFR-01".to_string(),
            "NFR-02".to_string(),
        ],
    })
}

/// Run abbreviated soak test for CI (1 hour instead of 48 hours)
pub async fn run_ci_soak_test() -> Result<TestResult> {
    info!("Starting CI soak test (1 hour)");
    
    let soak_config = SoakTestConfig {
        duration: Duration::from_secs(3600), // 1 hour for CI
        checkpoint_interval: Duration::from_secs(300), // 5 minute checkpoints
        metrics_collection_interval: Duration::from_secs(30), // 30 second metrics
        enable_full_logging: false,
        enable_blackbox_recording: false, // Reduce I/O for CI
        ..Default::default()
    };
    
    let test_config = TestConfig {
        duration: soak_config.duration,
        virtual_device: true,
        enable_tracing: false,
        enable_metrics: true,
        ..Default::default()
    };
    
    let mut harness = TestHarness::new(test_config.clone()).await?;
    let mut errors = Vec::new();
    let start_time = Instant::now();
    
    harness.start_service().await?;
    
    let mut total_ticks = 0u64;
    let mut total_missed_ticks = 0u64;
    let mut timer = RTTimer::new(crate::FFB_FREQUENCY_HZ);
    let end_time = start_time + soak_config.duration;
    
    info!("CI soak test will run for {:?}", soak_config.duration);
    
    while Instant::now() < end_time {
        let jitter = timer.wait_for_next_tick().await;
        total_ticks += 1;
        
        if jitter > Duration::from_micros(500) {
            total_missed_ticks += 1;
        }
        
        simulate_soak_ffb_processing().await;
        
        // Check for performance issues every 10 seconds
        if total_ticks % 10000 == 0 {
            let current_metrics = harness.collect_metrics().await;
            
            if current_metrics.jitter_p99_ms > crate::MAX_JITTER_P99_MS {
                errors.push(format!("Jitter {:.3}ms exceeded limit during CI soak", 
                                  current_metrics.jitter_p99_ms));
            }
            
            if current_metrics.memory_usage_mb > soak_config.max_memory_mb {
                errors.push(format!("Memory usage {:.1}MB exceeded limit during CI soak", 
                                  current_metrics.memory_usage_mb));
            }
        }
    }
    
    let actual_duration = start_time.elapsed();
    let final_metrics = harness.collect_metrics().await;
    
    info!("CI soak test completed:");
    info!("  Duration: {:?}", actual_duration);
    info!("  Total ticks: {}", total_ticks);
    info!("  Missed ticks: {} ({:.6}%)", total_missed_ticks, 
          (total_missed_ticks as f64 / total_ticks as f64) * 100.0);
    
    // CI soak test passes if no missed ticks and no performance degradation
    let ci_success = total_missed_ticks == 0 && errors.is_empty();
    
    if !ci_success {
        errors.push("CI soak test failed performance criteria".to_string());
    }
    
    harness.shutdown().await?;
    
    Ok(TestResult {
        passed: ci_success,
        duration: actual_duration,
        metrics: final_metrics,
        errors,
        requirement_coverage: vec![
            "NFR-03".to_string(),
            "FFB-01".to_string(),
            "NFR-01".to_string(),
        ],
    })
}

// Helper functions for soak testing

async fn create_checkpoint(
    harness: &mut TestHarness,
    start_time: Instant,
    total_ticks: u64,
    missed_ticks: u64,
    errors: &[String],
) -> Result<SoakCheckpoint> {
    let metrics = harness.collect_metrics().await;
    
    Ok(SoakCheckpoint {
        timestamp: std::time::SystemTime::now(),
        elapsed: start_time.elapsed(),
        metrics: metrics.clone(),
        total_ticks,
        missed_ticks,
        errors: errors.to_vec(),
        memory_usage_mb: metrics.memory_usage_mb,
        cpu_usage_percent: metrics.cpu_usage_percent,
    })
}

fn should_terminate_early(checkpoints: &[SoakCheckpoint]) -> bool {
    if checkpoints.len() < 3 {
        return false;
    }
    
    // Check for consistent performance degradation
    let recent_checkpoints = &checkpoints[checkpoints.len()-3..];
    
    // Terminate if jitter consistently exceeds limits
    let jitter_violations = recent_checkpoints.iter()
        .filter(|cp| cp.metrics.jitter_p99_ms > crate::MAX_JITTER_P99_MS)
        .count();
    
    if jitter_violations >= 2 {
        return true;
    }
    
    // Terminate if memory usage is growing consistently
    let memory_trend = recent_checkpoints.windows(2)
        .all(|pair| pair[1].memory_usage_mb > pair[0].memory_usage_mb);
    
    if memory_trend && recent_checkpoints.last().unwrap().memory_usage_mb > 200.0 {
        return true;
    }
    
    false
}

#[derive(Serialize, Deserialize)]
struct SoakSummary {
    duration: Duration,
    total_ticks: u64,
    missed_ticks: u64,
    miss_rate_percent: f64,
    checkpoints_count: usize,
    max_jitter_ms: f64,
    max_memory_mb: f64,
    max_cpu_percent: f64,
    success: bool,
}

fn create_soak_summary(
    checkpoints: &[SoakCheckpoint],
    duration: Duration,
    total_ticks: u64,
    missed_ticks: u64,
) -> SoakSummary {
    let miss_rate_percent = (missed_ticks as f64 / total_ticks as f64) * 100.0;
    
    let max_jitter_ms = checkpoints.iter()
        .map(|cp| cp.metrics.jitter_p99_ms)
        .fold(0.0f64, f64::max);
    
    let max_memory_mb = checkpoints.iter()
        .map(|cp| cp.memory_usage_mb)
        .fold(0.0f64, f64::max);
    
    let max_cpu_percent = checkpoints.iter()
        .map(|cp| cp.cpu_usage_percent)
        .fold(0.0f64, f64::max);
    
    let success = validate_soak_success_criteria(checkpoints, duration, total_ticks, missed_ticks);
    
    SoakSummary {
        duration,
        total_ticks,
        missed_ticks,
        miss_rate_percent,
        checkpoints_count: checkpoints.len(),
        max_jitter_ms,
        max_memory_mb,
        max_cpu_percent,
        success,
    }
}

fn validate_soak_success_criteria(
    checkpoints: &[SoakCheckpoint],
    duration: Duration,
    _total_ticks: u64,
    missed_ticks: u64,
) -> bool {
    // Success criteria for 48-hour soak test:
    // 1. Zero missed ticks
    // 2. No memory leaks (stable memory usage)
    // 3. Consistent performance (jitter within limits)
    // 4. Minimum duration achieved
    
    if missed_ticks > 0 {
        return false;
    }
    
    if duration < Duration::from_secs(47 * 3600) { // At least 47 hours
        return false;
    }
    
    // Check for memory leaks (memory should not grow consistently)
    if checkpoints.len() >= 10 {
        let first_half_avg = checkpoints[..checkpoints.len()/2].iter()
            .map(|cp| cp.memory_usage_mb)
            .sum::<f64>() / (checkpoints.len() / 2) as f64;
        
        let second_half_avg = checkpoints[checkpoints.len()/2..].iter()
            .map(|cp| cp.memory_usage_mb)
            .sum::<f64>() / (checkpoints.len() / 2) as f64;
        
        // Memory growth >20% indicates potential leak
        if second_half_avg > first_half_avg * 1.2 {
            return false;
        }
    }
    
    // Check for performance consistency
    let jitter_violations = checkpoints.iter()
        .filter(|cp| cp.metrics.jitter_p99_ms > crate::MAX_JITTER_P99_MS)
        .count();
    
    // Allow up to 5% of checkpoints to have jitter violations
    if jitter_violations > checkpoints.len() / 20 {
        return false;
    }
    
    true
}

async fn simulate_soak_ffb_processing() {
    // Lightweight FFB simulation for soak test
    let start = Instant::now();
    while start.elapsed() < Duration::from_micros(30) {
        std::hint::spin_loop();
    }
}

async fn simulate_background_activities() {
    // Simulate periodic background activities like:
    // - Profile saves
    // - Telemetry processing
    // - LED updates
    // - Diagnostics collection
    tokio::time::sleep(Duration::from_micros(100)).await;
}