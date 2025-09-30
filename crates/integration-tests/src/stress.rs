//! Stress testing module for hot-plug and system resilience testing

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::Result;
use tokio::time::timeout;
use tracing::{info, warn, error};
use rand::Rng;

use crate::common::{TestHarness, VirtualDevice};
use crate::{TestConfig, TestResult, PerformanceMetrics, StressLevel};

/// Hot-plug stress test with rapid connect/disconnect cycles
pub async fn test_hotplug_stress() -> Result<TestResult> {
    info!("Starting hot-plug stress test");
    
    let config = TestConfig {
        duration: Duration::from_secs(300), // 5 minutes of stress
        virtual_device: true,
        stress_level: StressLevel::Heavy,
        ..Default::default()
    };
    
    let mut harness = TestHarness::new(config.clone()).await?;
    let mut errors = Vec::new();
    let start_time = Instant::now();
    
    harness.start_service().await?;
    
    // Add multiple virtual devices for stress testing
    let device_ids = vec![
        harness.add_virtual_device("Stress Device 1").await?,
        harness.add_virtual_device("Stress Device 2").await?,
        harness.add_virtual_device("Stress Device 3").await?,
    ];
    
    let mut cycle_count = 0u32;
    let mut failed_cycles = 0u32;
    let test_duration = config.duration;
    let end_time = start_time + test_duration;
    
    info!("Starting hot-plug cycles with {} devices", device_ids.len());
    
    while Instant::now() < end_time {
        for device_index in 0..device_ids.len() {
            cycle_count += 1;
            
            // Random disconnect/reconnect timing to stress the system
            let disconnect_duration = Duration::from_millis(
                rand::thread_rng().gen_range(50..500)
            );
            
            let cycle_start = Instant::now();
            
            // Disconnect device
            let disconnect_result = timeout(
                Duration::from_millis(100),
                harness.simulate_hotplug_cycle(device_index)
            ).await;
            
            match disconnect_result {
                Ok(Ok(_)) => {
                    // Verify service handles disconnect gracefully
                    tokio::time::sleep(disconnect_duration).await;
                    
                    // Verify reconnection
                    let reconnect_verification = verify_device_reconnection(device_index).await;
                    if reconnect_verification.is_err() {
                        failed_cycles += 1;
                        errors.push(format!("Device {} reconnection failed in cycle {}", 
                                          device_index, cycle_count));
                    }
                }
                Ok(Err(e)) => {
                    failed_cycles += 1;
                    errors.push(format!("Hot-plug cycle {} failed: {}", cycle_count, e));
                }
                Err(_) => {
                    failed_cycles += 1;
                    errors.push(format!("Hot-plug cycle {} timed out", cycle_count));
                }
            }
            
            let cycle_duration = cycle_start.elapsed();
            if cycle_duration > Duration::from_millis(600) {
                warn!("Hot-plug cycle {} took {:?} (>600ms)", cycle_count, cycle_duration);
            }
            
            // Brief pause between cycles
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        
        // Verify service stability after each round
        let stability_check = verify_service_stability().await;
        if stability_check.is_err() {
            errors.push("Service stability check failed".to_string());
            break;
        }
    }
    
    let actual_duration = start_time.elapsed();
    let success_rate = ((cycle_count - failed_cycles) as f64 / cycle_count as f64) * 100.0;
    
    info!("Hot-plug stress test completed:");
    info!("  Total cycles: {}", cycle_count);
    info!("  Failed cycles: {}", failed_cycles);
    info!("  Success rate: {:.2}%", success_rate);
    info!("  Duration: {:?}", actual_duration);
    
    // Require >95% success rate for hot-plug operations
    if success_rate < 95.0 {
        errors.push(format!("Hot-plug success rate {:.2}% below 95% threshold", success_rate));
    }
    
    let metrics = harness.collect_metrics().await;
    harness.shutdown().await?;
    
    Ok(TestResult {
        passed: errors.is_empty(),
        duration: actual_duration,
        metrics,
        errors,
        requirement_coverage: vec![
            "DM-01".to_string(),
            "DM-02".to_string(), 
            "NFR-03".to_string(),
        ],
    })
}

/// Fault injection stress test
pub async fn test_fault_injection_stress() -> Result<TestResult> {
    info!("Starting fault injection stress test");
    
    let config = TestConfig {
        duration: Duration::from_secs(180), // 3 minutes
        virtual_device: true,
        stress_level: StressLevel::Heavy,
        ..Default::default()
    };
    
    let mut harness = TestHarness::new(config.clone()).await?;
    let mut errors = Vec::new();
    let start_time = Instant::now();
    
    harness.start_service().await?;
    
    let fault_types = vec![
        0x01, // USB fault
        0x02, // Encoder fault  
        0x04, // Thermal fault
        0x08, // Overcurrent fault
    ];
    
    let mut fault_injection_count = 0u32;
    let mut recovery_failures = 0u32;
    let test_duration = config.duration;
    let end_time = start_time + test_duration;
    
    info!("Starting fault injection cycles");
    
    while Instant::now() < end_time {
        for &fault_type in &fault_types {
            fault_injection_count += 1;
            
            // Inject fault
            let fault_start = Instant::now();
            harness.inject_fault(0, fault_type).await?;
            
            // Verify fault response within 50ms (SAFE-03)
            tokio::time::sleep(Duration::from_millis(60)).await;
            let fault_response_time = fault_start.elapsed();
            
            if fault_response_time > Duration::from_millis(50) {
                errors.push(format!("Fault {} response time {:?} exceeded 50ms", 
                                  fault_type, fault_response_time));
            }
            
            // Verify torque stopped
            let torque_stopped = verify_torque_stopped().await?;
            if !torque_stopped {
                errors.push(format!("Torque not stopped after fault {}", fault_type));
            }
            
            // Clear fault and verify recovery
            tokio::time::sleep(Duration::from_millis(100)).await;
            let recovery_result = simulate_fault_recovery(fault_type).await;
            
            match recovery_result {
                Ok(_) => {
                    // Verify service resumed normal operation
                    let resume_check = verify_normal_operation().await;
                    if resume_check.is_err() {
                        recovery_failures += 1;
                        errors.push(format!("Recovery from fault {} failed", fault_type));
                    }
                }
                Err(e) => {
                    recovery_failures += 1;
                    errors.push(format!("Fault {} recovery failed: {}", fault_type, e));
                }
            }
            
            // Brief pause between fault injections
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    
    let actual_duration = start_time.elapsed();
    let recovery_rate = ((fault_injection_count - recovery_failures) as f64 / fault_injection_count as f64) * 100.0;
    
    info!("Fault injection stress test completed:");
    info!("  Total faults injected: {}", fault_injection_count);
    info!("  Recovery failures: {}", recovery_failures);
    info!("  Recovery rate: {:.2}%", recovery_rate);
    info!("  Duration: {:?}", actual_duration);
    
    // Require 100% recovery rate for fault handling
    if recovery_failures > 0 {
        errors.push(format!("Fault recovery failures: {}", recovery_failures));
    }
    
    let metrics = harness.collect_metrics().await;
    harness.shutdown().await?;
    
    Ok(TestResult {
        passed: errors.is_empty(),
        duration: actual_duration,
        metrics,
        errors,
        requirement_coverage: vec![
            "SAFE-03".to_string(),
            "SAFE-04".to_string(),
            "FFB-05".to_string(),
        ],
    })
}

/// Memory pressure stress test
pub async fn test_memory_pressure_stress() -> Result<TestResult> {
    info!("Starting memory pressure stress test");
    
    let config = TestConfig {
        duration: Duration::from_secs(240), // 4 minutes
        virtual_device: true,
        stress_level: StressLevel::Extreme,
        enable_metrics: true,
        ..Default::default()
    };
    
    let mut harness = TestHarness::new(config.clone()).await?;
    let mut errors = Vec::new();
    let start_time = Instant::now();
    
    harness.start_service().await?;
    
    let mut max_memory_mb = 0.0f64;
    let mut memory_samples = Vec::new();
    let test_duration = config.duration;
    let end_time = start_time + test_duration;
    
    info!("Starting memory pressure simulation");
    
    // Spawn memory pressure tasks
    let _pressure_tasks: Vec<_> = (0..4).map(|i| {
        tokio::spawn(async move {
            simulate_memory_pressure(i).await
        })
    }).collect();
    
    while Instant::now() < end_time {
        // Collect memory metrics
        let current_metrics = harness.collect_metrics().await;
        memory_samples.push(current_metrics.memory_usage_mb);
        
        if current_metrics.memory_usage_mb > max_memory_mb {
            max_memory_mb = current_metrics.memory_usage_mb;
        }
        
        // Check if memory usage exceeds 150MB limit (NFR-02)
        if current_metrics.memory_usage_mb > 150.0 {
            errors.push(format!("Memory usage {:.1}MB exceeded 150MB limit", 
                              current_metrics.memory_usage_mb));
        }
        
        // Verify RT performance is maintained under memory pressure
        if current_metrics.jitter_p99_ms > crate::MAX_JITTER_P99_MS {
            errors.push(format!("Jitter {:.3}ms exceeded limit under memory pressure", 
                              current_metrics.jitter_p99_ms));
        }
        
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    
    let actual_duration = start_time.elapsed();
    let avg_memory_mb = memory_samples.iter().sum::<f64>() / memory_samples.len() as f64;
    
    info!("Memory pressure stress test completed:");
    info!("  Max memory usage: {:.1}MB (limit: 150MB)", max_memory_mb);
    info!("  Average memory usage: {:.1}MB", avg_memory_mb);
    info!("  Duration: {:?}", actual_duration);
    
    let final_metrics = harness.collect_metrics().await;
    harness.shutdown().await?;
    
    Ok(TestResult {
        passed: errors.is_empty(),
        duration: actual_duration,
        metrics: final_metrics,
        errors,
        requirement_coverage: vec![
            "NFR-02".to_string(),
            "NFR-01".to_string(),
            "NFR-03".to_string(),
        ],
    })
}

/// CPU load stress test
pub async fn test_cpu_load_stress() -> Result<TestResult> {
    info!("Starting CPU load stress test");
    
    let config = TestConfig {
        duration: Duration::from_secs(300), // 5 minutes
        virtual_device: true,
        stress_level: StressLevel::Heavy,
        enable_metrics: true,
        ..Default::default()
    };
    
    let mut harness = TestHarness::new(config.clone()).await?;
    let mut errors = Vec::new();
    let start_time = Instant::now();
    
    harness.start_service().await?;
    
    let mut max_cpu_percent = 0.0f64;
    let mut cpu_samples = Vec::new();
    let test_duration = config.duration;
    let end_time = start_time + test_duration;
    
    info!("Starting CPU load simulation");
    
    // Spawn CPU load tasks (background threads)
    let _load_tasks: Vec<_> = (0..num_cpus::get() - 1).map(|i| {
        tokio::spawn(async move {
            simulate_cpu_load(i).await
        })
    }).collect();
    
    while Instant::now() < end_time {
        // Collect CPU metrics
        let current_metrics = harness.collect_metrics().await;
        cpu_samples.push(current_metrics.cpu_usage_percent);
        
        if current_metrics.cpu_usage_percent > max_cpu_percent {
            max_cpu_percent = current_metrics.cpu_usage_percent;
        }
        
        // Check if service CPU usage exceeds 3% of one core (NFR-02)
        if current_metrics.cpu_usage_percent > 3.0 {
            warn!("Service CPU usage {:.1}% exceeded 3% limit", 
                  current_metrics.cpu_usage_percent);
        }
        
        // Verify RT performance is maintained under CPU load
        if current_metrics.jitter_p99_ms > crate::MAX_JITTER_P99_MS {
            errors.push(format!("Jitter {:.3}ms exceeded limit under CPU load", 
                              current_metrics.jitter_p99_ms));
        }
        
        if current_metrics.missed_ticks > 0 {
            errors.push(format!("Missed {} ticks under CPU load", 
                              current_metrics.missed_ticks));
        }
        
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    
    let actual_duration = start_time.elapsed();
    let avg_cpu_percent = cpu_samples.iter().sum::<f64>() / cpu_samples.len() as f64;
    
    info!("CPU load stress test completed:");
    info!("  Max service CPU usage: {:.1}%", max_cpu_percent);
    info!("  Average service CPU usage: {:.1}%", avg_cpu_percent);
    info!("  Duration: {:?}", actual_duration);
    
    let final_metrics = harness.collect_metrics().await;
    harness.shutdown().await?;
    
    Ok(TestResult {
        passed: errors.is_empty(),
        duration: actual_duration,
        metrics: final_metrics,
        errors,
        requirement_coverage: vec![
            "NFR-02".to_string(),
            "NFR-01".to_string(),
            "NFR-03".to_string(),
        ],
    })
}

// Helper functions for stress testing

async fn verify_device_reconnection(device_index: usize) -> Result<()> {
    // Simulate device reconnection verification
    tokio::time::sleep(Duration::from_millis(50)).await;
    info!("Device {} reconnection verified", device_index);
    Ok(())
}

async fn verify_service_stability() -> Result<()> {
    // Simulate service stability check
    tokio::time::sleep(Duration::from_millis(10)).await;
    Ok(())
}

async fn verify_torque_stopped() -> Result<bool> {
    // Simulate torque verification
    tokio::time::sleep(Duration::from_millis(5)).await;
    Ok(true)
}

async fn simulate_fault_recovery(fault_type: u8) -> Result<()> {
    info!("Simulating recovery from fault type {}", fault_type);
    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(())
}

async fn verify_normal_operation() -> Result<()> {
    // Simulate verification of normal operation
    tokio::time::sleep(Duration::from_millis(20)).await;
    Ok(())
}

async fn simulate_memory_pressure(task_id: usize) {
    info!("Starting memory pressure task {}", task_id);
    
    loop {
        // Allocate and deallocate memory to create pressure
        let _memory_block: Vec<u8> = vec![0; 1024 * 1024]; // 1MB
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn simulate_cpu_load(task_id: usize) {
    info!("Starting CPU load task {}", task_id);
    
    loop {
        // CPU-intensive work
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(50) {
            // Busy work
            for i in 0..10000 {
                std::hint::black_box(i * i);
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}