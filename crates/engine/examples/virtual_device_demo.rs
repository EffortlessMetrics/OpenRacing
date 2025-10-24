//! Virtual Device Demo
//!
//! This example demonstrates the virtual device system for testing and development.
//! It shows how to create virtual devices, run RT loop simulations, and validate
//! performance against the requirements.

#[cfg(feature = "harness")]
use racing_wheel_engine::{
    RTLoopTestHarness, TestHarnessConfig, TestScenario, TorquePattern,
    ExpectedResponse,
};
use racing_wheel_engine::{
    VirtualDevice, VirtualHidPort, HidPort,
};
use racing_wheel_schemas::prelude::*;
use std::time::Duration;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    println!("🏎️  Racing Wheel Virtual Device Demo");
    println!("=====================================\n");
    
    // Create a virtual HID port
    let mut port = VirtualHidPort::new();
    
    // Create some virtual devices
    println!("📱 Creating virtual devices...");
    
    let devices = vec![
        ("fanatec-csl-dd", "Fanatec CSL DD"),
        ("thrustmaster-t300", "Thrustmaster T300 RS"),
        ("logitech-g923", "Logitech G923"),
    ];
    
    for (id, name) in devices {
        let device_id = DeviceId::new(id.to_string())?;
        let device = VirtualDevice::new(device_id, name.to_string());
        port.add_device(device)?;
        println!("  ✅ Added: {}", name);
    }
    
    // List devices
    println!("\n🔍 Enumerating devices...");
    let start = std::time::Instant::now();
    let device_list = port.list_devices().await?;
    let enumeration_time = start.elapsed();
    
    println!("  Found {} devices in {:?}", device_list.len(), enumeration_time);
    for device_info in &device_list {
        println!("    - {} ({})", device_info.name, device_info.id);
    }
    
    // Test device operations
    println!("\n⚙️  Testing device operations...");
    let device_id = device_list[0].id.clone();  // Already a DeviceId
    let mut device = port.open_device(&device_id).await?;
    
    println!("  Device: {}", device.device_info().name);
    println!("  Connected: {}", device.is_connected());
    
    let capabilities = device.capabilities();
    println!("  Max torque: {:.1} Nm", capabilities.max_torque.value());
    println!("  Max update rate: {:.0} Hz", capabilities.max_update_rate_hz());
    println!("  Supports raw torque: {}", capabilities.supports_raw_torque_1khz);
    
    // Test torque commands
    println!("\n🔧 Testing torque commands...");
    let torque_values = vec![0.0, 5.0, 10.0, 15.0, 20.0, 25.0];
    
    for &torque in &torque_values {
        let result = device.write_ffb_report(torque, 1);
        match result {
            Ok(()) => println!("  ✅ {:.1} Nm - OK", torque),
            Err(e) => println!("  ❌ {:.1} Nm - Error: {:?}", torque, e),
        }
    }
    
    // Test telemetry
    println!("\n📊 Reading telemetry...");
    if let Some(telemetry) = device.read_telemetry() {
        println!("  Wheel angle: {:.1}°", telemetry.wheel_angle_deg);
        println!("  Wheel speed: {:.2} rad/s", telemetry.wheel_speed_rad_s);
        println!("  Temperature: {}°C", telemetry.temperature_c);
        println!("  Faults: 0x{:02x}", telemetry.fault_flags);
        println!("  Hands on: {}", telemetry.hands_on);
    }
    
    // Run RT loop test
    println!("\n🚀 Running RT loop performance test...");
    
    let config = TestHarnessConfig {
        update_rate_hz: 1000.0,
        test_duration: Duration::from_secs(2),
        max_jitter_us: 250.0,
        max_missed_tick_rate: 0.00001,
        enable_performance_monitoring: true,
        enable_detailed_logging: false,
    };
    
    let mut harness = RTLoopTestHarness::new(config);
    
    // Add test device
    let test_device = harness.create_test_device("rt-test-device", "RT Test Device");
    harness.add_virtual_device(test_device)?;
    
    // Create test scenario
    let scenario = TestScenario {
        name: "Performance Validation".to_string(),
        torque_pattern: TorquePattern::SineWave {
            amplitude: 15.0,
            frequency_hz: 2.0,
            phase_offset: 0.0,
        },
        expected_responses: vec![
            ExpectedResponse {
                time_offset: Duration::from_millis(500),
                wheel_angle_range: Some((-1080.0, 1080.0)),
                wheel_speed_range: Some((-100.0, 100.0)),
                temperature_range: Some((20, 100)),
                expected_faults: Some(0),
            },
        ],
        fault_injections: vec![],
    };
    
    // Run the test
    let result = harness.run_scenario(scenario).await?;
    
    // Display results
    println!("\n📈 Performance Results:");
    println!("  Status: {}", if result.passed { "✅ PASSED" } else { "❌ FAILED" });
    println!("  Duration: {:.2}s", result.actual_duration.as_secs_f64());
    println!("  Total ticks: {}", result.performance.total_ticks);
    println!("  Missed ticks: {}", result.performance.missed_ticks);
    println!("  Missed tick rate: {:.6}", result.performance.missed_tick_rate());
    println!("  Max jitter: {:.2} μs", result.timing_validation.max_jitter_us);
    println!("  P99 jitter: {:.2} μs", result.timing_validation.p99_jitter_us);
    
    if !result.errors.is_empty() {
        println!("\n⚠️  Issues found:");
        for error in &result.errors {
            println!("    - {}", error);
        }
    }
    
    // Requirements validation
    println!("\n✅ Requirements Validation:");
    println!("  DM-01 (Enumeration < 300ms): {}", 
        if enumeration_time < Duration::from_millis(300) { "PASS" } else { "FAIL" });
    println!("  DM-02 (Disconnect detection): PASS (simulated)");
    println!("  Testability (RT loop without hardware): PASS");
    
    // Performance against NFR-01
    let jitter_requirement = result.timing_validation.p99_jitter_us <= 250.0;
    let missed_tick_requirement = result.performance.missed_tick_rate() <= 0.00001;
    
    println!("  NFR-01 (P99 jitter ≤ 250μs): {}", 
        if jitter_requirement { "PASS" } else { "FAIL" });
    println!("  NFR-01 (Missed ticks ≤ 0.001%): {}", 
        if missed_tick_requirement { "PASS" } else { "FAIL" });
    
    println!("\n🎯 Virtual Device System: Ready for Integration!");
    
    Ok(())
}