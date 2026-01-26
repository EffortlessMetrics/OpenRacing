//! LED and Haptics System Tests
//!
//! This module contains comprehensive tests for the LED and haptics output system,
//! including pattern generation, timing validation, and rate independence verification.

// Test helper functions to replace unwrap
#[track_caller]
fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("unexpected Err: {e:?}"),
    }
}

use racing_wheel_engine::led_haptics::*;
use racing_wheel_engine::ports::{NormalizedTelemetry, TelemetryFlags};
use racing_wheel_schemas::prelude::{DeviceId, FrequencyHz, Gain, HapticsConfig, LedConfig};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::timeout;

#[track_caller]
fn must_some<T>(o: Option<T>, msg: &str) -> T {
    match o {
        Some(v) => v,
        None => panic!("expected Some: {}", msg),
    }
}

/// Helper function to create test telemetry data
fn create_test_telemetry(
    rpm: f32,
    speed_ms: f32,
    slip_ratio: f32,
    gear: i8,
) -> NormalizedTelemetry {
    NormalizedTelemetry {
        ffb_scalar: 0.5,
        rpm,
        speed_ms,
        slip_ratio,
        gear,
        flags: TelemetryFlags {
            yellow_flag: false,
            red_flag: false,
            blue_flag: false,
            checkered_flag: false,
            pit_limiter: false,
            drs_enabled: false,
            ers_available: false,
            in_pit: false,
        },
        car_id: Some("test_car".to_string()),
        track_id: Some("test_track".to_string()),
        timestamp: std::time::Instant::now(),
    }
}

/// Helper function to create test LED configuration
fn create_test_led_config() -> LedConfig {
    let mut colors = HashMap::new();
    colors.insert("green".to_string(), [0, 255, 0]);
    colors.insert("yellow".to_string(), [255, 255, 0]);
    colors.insert("red".to_string(), [255, 0, 0]);
    colors.insert("blue".to_string(), [0, 0, 255]);

    must(LedConfig::new(
        vec![0.75, 0.82, 0.88, 0.92, 0.96],
        "progressive".to_string(),
        must(Gain::new(0.8)),
        colors,
    ))
}

/// Helper function to create test haptics configuration
fn create_test_haptics_config() -> HapticsConfig {
    let mut effects = HashMap::new();
    effects.insert("kerb".to_string(), true);
    effects.insert("slip".to_string(), true);
    effects.insert("gear_shift".to_string(), false);
    effects.insert("collision".to_string(), true);

    HapticsConfig::new(
        true,
        must(Gain::new(0.6)),
        must(FrequencyHz::new(80.0)),
        effects,
    )
}

#[cfg(test)]
mod led_mapping_engine_tests {
    use super::*;

    #[test]
    fn test_led_engine_creation() {
        let config = create_test_led_config();
        let engine = LedMappingEngine::new(config.clone());

        // Verify initial state
        assert_eq!(engine.current_pattern(), &LedPattern::Off);
    }

    #[test]
    fn test_rpm_pattern_generation() {
        let config = create_test_led_config();
        let mut engine = LedMappingEngine::new(config);

        // Test low RPM (should show minimal LEDs)
        let low_rpm_telemetry = create_test_telemetry(3000.0, 20.0, 0.0, 3);
        let colors = engine.update_pattern(&low_rpm_telemetry);

        // Should have 16 LEDs (typical wheel LED count)
        assert_eq!(colors.len(), 16);

        // At low RPM, most LEDs should be off
        let lit_leds = colors.iter().filter(|&c| *c != LedColor::OFF).count();
        assert!(lit_leds <= 4, "Too many LEDs lit at low RPM: {}", lit_leds);
    }

    #[test]
    fn test_rpm_hysteresis() {
        let config = create_test_led_config();
        let mut engine = LedMappingEngine::new(config);

        // Start at medium RPM
        let medium_rpm = create_test_telemetry(6000.0, 30.0, 0.0, 4);
        let colors1 = engine.update_pattern(&medium_rpm);
        let lit_count1 = colors1.iter().filter(|&c| *c != LedColor::OFF).count();

        // Slightly increase RPM (should not change due to hysteresis)
        let slightly_higher = create_test_telemetry(6100.0, 31.0, 0.0, 4);
        let colors2 = engine.update_pattern(&slightly_higher);
        let lit_count2 = colors2.iter().filter(|&c| *c != LedColor::OFF).count();

        // Should be the same due to hysteresis
        assert_eq!(lit_count1, lit_count2, "Hysteresis not working properly");

        // Significantly increase RPM (should change)
        let high_rpm = create_test_telemetry(7500.0, 40.0, 0.0, 5);
        let colors3 = engine.update_pattern(&high_rpm);
        let lit_count3 = colors3.iter().filter(|&c| *c != LedColor::OFF).count();

        // Should have more LEDs lit
        assert!(
            lit_count3 > lit_count1,
            "RPM increase should light more LEDs"
        );
    }

    #[test]
    fn test_flag_priority() {
        let config = create_test_led_config();
        let mut engine = LedMappingEngine::new(config);

        // Create telemetry with yellow flag
        let mut telemetry = create_test_telemetry(7000.0, 35.0, 0.0, 4);
        telemetry.flags.yellow_flag = true;

        let _colors = engine.update_pattern(&telemetry);

        // With yellow flag, pattern should be flag-based, not RPM-based
        match engine.current_pattern() {
            LedPattern::Flag { flag_type, .. } => {
                assert_eq!(*flag_type, FlagType::Yellow);
            }
            _ => panic!("Expected flag pattern when yellow flag is active"),
        }
    }

    #[test]
    fn test_pit_limiter_pattern() {
        let config = create_test_led_config();
        let mut engine = LedMappingEngine::new(config);

        // Create telemetry with pit limiter active
        let mut telemetry = create_test_telemetry(4000.0, 15.0, 0.0, 2);
        telemetry.flags.pit_limiter = true;

        let _colors = engine.update_pattern(&telemetry);

        // Should show pit limiter pattern
        match engine.current_pattern() {
            LedPattern::PitLimiter { color, .. } => {
                assert_eq!(*color, LedColor::BLUE);
            }
            _ => panic!("Expected pit limiter pattern when pit limiter is active"),
        }
    }

    #[tokio::test]
    async fn test_pattern_timing() {
        let config = create_test_led_config();
        let mut engine = LedMappingEngine::new(config);

        // Test blink pattern timing
        let mut telemetry = create_test_telemetry(5000.0, 25.0, 0.0, 3);
        telemetry.flags.yellow_flag = true;

        // Update pattern multiple times with small time intervals
        let _start_time = Instant::now();
        let mut blink_states = Vec::new();

        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let colors = engine.update_pattern(&telemetry);
            let is_lit = colors.iter().any(|&c| c != LedColor::OFF);
            blink_states.push(is_lit);
        }

        // Should have both on and off states (blinking)
        let has_on = blink_states.iter().any(|&state| state);
        let has_off = blink_states.iter().any(|&state| !state);

        assert!(
            has_on && has_off,
            "Blink pattern should alternate between on and off"
        );
    }

    #[test]
    fn test_config_update() {
        let config = create_test_led_config();
        let mut engine = LedMappingEngine::new(config);

        // Create new config with different RPM bands
        let mut new_config = create_test_led_config();
        new_config.rpm_bands = vec![0.6, 0.7, 0.8, 0.9, 0.95];

        engine.update_config(new_config);

        // Test that new config is applied
        let telemetry = create_test_telemetry(6000.0, 30.0, 0.0, 4);
        let colors = engine.update_pattern(&telemetry);

        // Should use new RPM bands (implementation detail test)
        assert_eq!(colors.len(), 16);
    }
}

#[cfg(test)]
mod haptics_router_tests {
    use super::*;

    #[test]
    fn test_haptics_router_creation() {
        let config = create_test_haptics_config();
        let router = HapticsRouter::new(config);

        assert!(router.active_patterns().is_empty());
    }

    #[test]
    fn test_slip_based_haptics() {
        let config = create_test_haptics_config();
        let mut router = HapticsRouter::new(config);

        // Test with high slip ratio
        let high_slip_telemetry = create_test_telemetry(6000.0, 30.0, 0.8, 4);
        let patterns = router.update_patterns(&high_slip_telemetry);

        // Should have rim vibration pattern
        assert!(patterns.contains_key("rim_slip"));

        if let Some(HapticsPattern::RimVibration {
            intensity,
            frequency_hz,
        }) = patterns.get("rim_slip")
        {
            assert!(*intensity > 0.0);
            assert!(*frequency_hz > 25.0);
        } else {
            panic!("Expected rim vibration pattern for high slip");
        }
    }

    #[test]
    fn test_engine_vibration() {
        let config = create_test_haptics_config();
        let mut router = HapticsRouter::new(config);

        // Test with high RPM
        let high_rpm_telemetry = create_test_telemetry(7500.0, 40.0, 0.1, 5);
        let patterns = router.update_patterns(&high_rpm_telemetry);

        // Should have engine vibration pattern
        assert!(patterns.contains_key("engine"));

        if let Some(HapticsPattern::EngineVibration {
            intensity,
            rpm_multiplier,
            ..
        }) = patterns.get("engine")
        {
            assert!(*intensity > 0.0);
            assert!(*rpm_multiplier > 0.0);
        } else {
            panic!("Expected engine vibration pattern for high RPM");
        }
    }

    #[test]
    fn test_abs_haptics() {
        let config = create_test_haptics_config();
        let mut router = HapticsRouter::new(config);

        // Test with very high slip (simulating ABS activation)
        let abs_telemetry = create_test_telemetry(5000.0, 25.0, 0.5, 3);
        let patterns = router.update_patterns(&abs_telemetry);

        // Should have brake ABS pattern
        assert!(patterns.contains_key("brake_abs"));

        if let Some(HapticsPattern::PedalFeedback {
            pedal, intensity, ..
        }) = patterns.get("brake_abs")
        {
            assert_eq!(*pedal, PedalType::Brake);
            assert!(*intensity > 0.0);
        } else {
            panic!("Expected brake ABS pattern for high slip");
        }
    }

    #[test]
    fn test_no_haptics_at_low_activity() {
        let config = create_test_haptics_config();
        let mut router = HapticsRouter::new(config);

        // Test with low activity (low RPM, no slip)
        let low_activity_telemetry = create_test_telemetry(800.0, 5.0, 0.0, 1);
        let patterns = router.update_patterns(&low_activity_telemetry);

        // Should have no active patterns
        assert!(patterns.is_empty() || patterns.values().all(|p| matches!(p, HapticsPattern::Off)));
    }

    #[test]
    fn test_config_update() {
        let config = create_test_haptics_config();
        let mut router = HapticsRouter::new(config);

        // Create new config with different settings
        let mut new_config = create_test_haptics_config();
        new_config.intensity = must(Gain::new(0.3));

        router.update_config(new_config);

        // Test that config is updated (implementation detail)
        let telemetry = create_test_telemetry(6000.0, 30.0, 0.5, 4);
        let patterns = router.update_patterns(&telemetry);

        // Should still generate patterns with new config
        assert!(!patterns.is_empty());
    }
}

#[cfg(test)]
mod dash_widget_tests {
    use super::*;

    #[test]
    fn test_dash_widget_creation() {
        let dash = DashWidgetSystem::new();
        assert!(dash.widgets().is_empty());
    }

    #[test]
    fn test_widget_updates() {
        let mut dash = DashWidgetSystem::new();

        let telemetry = create_test_telemetry(6500.0, 35.0, 0.2, 4);
        let widgets = dash.update_widgets(&telemetry);

        // Should have all expected widgets
        assert!(widgets.contains_key("gear"));
        assert!(widgets.contains_key("rpm"));
        assert!(widgets.contains_key("speed"));
        assert!(widgets.contains_key("flags"));
        assert!(widgets.contains_key("drs"));
        assert!(widgets.contains_key("ers"));
    }

    #[test]
    fn test_gear_widget() {
        let mut dash = DashWidgetSystem::new();

        let telemetry = create_test_telemetry(5000.0, 25.0, 0.0, 3);
        let widgets = dash.update_widgets(&telemetry);

        if let Some(DashWidget::Gear { current_gear, .. }) = widgets.get("gear") {
            assert_eq!(*current_gear, 3);
        } else {
            panic!("Expected gear widget");
        }
    }

    #[test]
    fn test_rpm_widget() {
        let mut dash = DashWidgetSystem::new();

        let telemetry = create_test_telemetry(6500.0, 35.0, 0.0, 4);
        let widgets = dash.update_widgets(&telemetry);

        if let Some(DashWidget::Rpm {
            current_rpm,
            max_rpm,
            redline_rpm,
        }) = widgets.get("rpm")
        {
            assert_eq!(*current_rpm, 6500.0);
            assert_eq!(*max_rpm, 8000.0);
            assert_eq!(*redline_rpm, 7500.0);
        } else {
            panic!("Expected RPM widget");
        }
    }

    #[test]
    fn test_speed_widget() {
        let mut dash = DashWidgetSystem::new();

        let telemetry = create_test_telemetry(5000.0, 30.0, 0.0, 3); // 30 m/s = 108 km/h
        let widgets = dash.update_widgets(&telemetry);

        if let Some(DashWidget::Speed { speed_kmh, unit }) = widgets.get("speed") {
            assert!((speed_kmh - 108.0).abs() < 0.1); // 30 m/s * 3.6 = 108 km/h
            assert_eq!(*unit, SpeedUnit::Kmh);
        } else {
            panic!("Expected speed widget");
        }
    }

    #[test]
    fn test_flags_widget() {
        let mut dash = DashWidgetSystem::new();

        let mut telemetry = create_test_telemetry(5000.0, 25.0, 0.0, 3);
        telemetry.flags.yellow_flag = true;
        telemetry.flags.blue_flag = true;

        let widgets = dash.update_widgets(&telemetry);

        if let Some(DashWidget::Flags { active_flags }) = widgets.get("flags") {
            assert!(active_flags.contains(&FlagType::Yellow));
            assert!(active_flags.contains(&FlagType::Blue));
            assert!(!active_flags.contains(&FlagType::Red));
        } else {
            panic!("Expected flags widget");
        }
    }
}

#[cfg(test)]
mod rate_independence_tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_led_haptics_system_creation() {
        let device_id = must(DeviceId::new("test-device".to_string()));
        let led_config = create_test_led_config();
        let haptics_config = create_test_haptics_config();

        let (system, _output_rx) = LedHapticsSystem::new(
            device_id.clone(),
            led_config,
            haptics_config,
            100.0, // 100 Hz update rate
        );

        // Note: device_id is private, so we can't test it directly
        // This is acceptable as the functionality is tested through the output
        assert!(!system.is_running());
    }

    #[tokio::test]
    async fn test_rate_independence_60hz() {
        let device_id = must(DeviceId::new("test-device".to_string()));
        let led_config = create_test_led_config();
        let haptics_config = create_test_haptics_config();

        let (mut system, mut output_rx) = LedHapticsSystem::new(
            device_id,
            led_config,
            haptics_config,
            60.0, // 60 Hz update rate
        );

        let (telemetry_tx, telemetry_rx) = mpsc::channel(100);

        // Start the system
        must(system.start(telemetry_rx).await);
        assert!(system.is_running());

        // Send telemetry at a different rate (30 Hz)
        let telemetry_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(33)); // ~30 Hz
            for i in 0..10 {
                let telemetry = create_test_telemetry(
                    3000.0 + (i as f32 * 100.0), // Varying RPM
                    20.0,
                    0.0,
                    3,
                );
                let _ = telemetry_tx.send(telemetry).await;
                interval.tick().await;
            }
        });

        // Collect outputs for timing analysis
        let output_times = Arc::new(std::sync::Mutex::new(Vec::new()));
        let output_times_clone = Arc::clone(&output_times);

        let output_handle = tokio::spawn(async move {
            let start_time = Instant::now();
            let mut count = 0;

            while count < 20 {
                if let Ok(output) = timeout(Duration::from_millis(100), output_rx.recv()).await {
                    if let Some(_output) = output {
                        let elapsed = start_time.elapsed();
                        output_times_clone.lock().unwrap().push(elapsed);
                        count += 1;
                    }
                } else {
                    break;
                }
            }
        });

        // Wait for test completion
        let _ = tokio::join!(telemetry_handle, output_handle);

        // Stop the system
        system.stop();

        // Analyze timing
        let times = must(output_times.lock());
        if times.len() >= 2 {
            let intervals: Vec<Duration> = times.windows(2).map(|w| w[1] - w[0]).collect();

            // Check that intervals are approximately 1/60 second (16.67ms)
            let expected_interval = Duration::from_millis(16); // ~60 Hz
            let tolerance = Duration::from_millis(5); // 5ms tolerance

            for interval in &intervals {
                let diff = if *interval > expected_interval {
                    *interval - expected_interval
                } else {
                    expected_interval - *interval
                };

                assert!(
                    diff <= tolerance,
                    "Output interval {:?} too far from expected {:?}",
                    interval,
                    expected_interval
                );
            }
        }
    }

    #[tokio::test]
    async fn test_rate_independence_200hz() {
        let device_id = must(DeviceId::new("test-device".to_string()));
        let led_config = create_test_led_config();
        let haptics_config = create_test_haptics_config();

        let (mut system, mut output_rx) = LedHapticsSystem::new(
            device_id,
            led_config,
            haptics_config,
            200.0, // 200 Hz update rate
        );

        let (telemetry_tx, telemetry_rx) = mpsc::channel(100);

        // Start the system
        must(system.start(telemetry_rx).await);

        // Send telemetry at 50 Hz
        let telemetry_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(20)); // 50 Hz
            for i in 0..10 {
                let telemetry = create_test_telemetry(4000.0 + (i as f32 * 200.0), 25.0, 0.1, 4);
                let _ = telemetry_tx.send(telemetry).await;
                interval.tick().await;
            }
        });

        // Count outputs in a fixed time window
        let output_count = Arc::new(AtomicU64::new(0));
        let output_count_clone = Arc::clone(&output_count);

        let output_handle = tokio::spawn(async move {
            let start_time = Instant::now();

            while start_time.elapsed() < Duration::from_millis(500) {
                // 0.5 second window
                if let Ok(output) = timeout(Duration::from_millis(10), output_rx.recv()).await {
                    if output.is_some() {
                        output_count_clone.fetch_add(1, Ordering::Relaxed);
                    }
                } else {
                    break;
                }
            }
        });

        // Wait for test completion
        let _ = tokio::join!(telemetry_handle, output_handle);

        // Stop the system
        system.stop();

        // Check output rate (should be approximately 200 Hz for 0.5 seconds = ~100 outputs)
        let count = output_count.load(Ordering::Relaxed);
        assert!(
            (80..=120).contains(&count),
            "Expected ~100 outputs in 0.5s at 200Hz, got {}",
            count
        );
    }

    #[tokio::test]
    async fn test_output_structure() {
        let device_id = must(DeviceId::new("test-device".to_string()));
        let led_config = create_test_led_config();
        let haptics_config = create_test_haptics_config();

        let (mut system, mut output_rx) =
            LedHapticsSystem::new(device_id.clone(), led_config, haptics_config, 100.0);

        let (telemetry_tx, telemetry_rx) = mpsc::channel(100);

        // Start the system
        must(system.start(telemetry_rx).await);

        // Send test telemetry
        let telemetry = create_test_telemetry(6000.0, 30.0, 0.3, 4);
        must(telemetry_tx.send(telemetry).await);

        // Get output
        if let Ok(Some(output)) = timeout(Duration::from_millis(100), output_rx.recv()).await {
            // Verify output structure
            assert_eq!(output.device_id, device_id);
            assert_eq!(output.led_colors.len(), 16); // Standard LED count
            assert!(!output.haptics_patterns.is_empty()); // Should have haptics for slip
            assert!(!output.dash_widgets.is_empty()); // Should have widgets

            // Verify timestamp is recent
            let age = output.timestamp.elapsed();
            assert!(age < Duration::from_millis(100));
        } else {
            panic!("Failed to receive output within timeout");
        }

        system.stop();
    }

    #[tokio::test]
    async fn test_config_updates_during_operation() {
        let device_id = must(DeviceId::new("test-device".to_string()));
        let led_config = create_test_led_config();
        let haptics_config = create_test_haptics_config();

        let (mut system, mut output_rx) =
            LedHapticsSystem::new(device_id, led_config, haptics_config, 100.0);

        let (telemetry_tx, telemetry_rx) = mpsc::channel(100);

        // Start the system
        must(system.start(telemetry_rx).await);

        // Send initial telemetry
        let telemetry = create_test_telemetry(5000.0, 25.0, 0.0, 3);
        must(telemetry_tx.send(telemetry.clone()).await);

        // Get initial output
        let _initial_output = must_some(
            must(timeout(Duration::from_millis(100), output_rx.recv()).await),
            "timeout",
        );

        // Update LED config during operation
        let mut new_led_config = create_test_led_config();
        new_led_config.brightness = must(Gain::new(0.5)); // Different brightness
        system.update_led_config(new_led_config);

        // Update haptics config during operation
        let mut new_haptics_config = create_test_haptics_config();
        new_haptics_config.intensity = must(Gain::new(0.3)); // Different intensity
        system.update_haptics_config(new_haptics_config);

        // Send more telemetry
        must(telemetry_tx.send(telemetry).await);

        // Get updated output
        let _updated_output = must_some(
            must(timeout(Duration::from_millis(100), output_rx.recv()).await),
            "timeout",
        );

        // System should still be running and producing output
        assert!(system.is_running());

        system.stop();
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_led_pattern_generation_performance() {
        let config = create_test_led_config();
        let mut engine = LedMappingEngine::new(config);

        let telemetry = create_test_telemetry(6000.0, 30.0, 0.2, 4);

        // Measure time for 1000 pattern updates
        let start = Instant::now();
        for _ in 0..1000 {
            let _colors = engine.update_pattern(&telemetry);
        }
        let elapsed = start.elapsed();

        // Should complete 1000 updates in reasonable time (< 10ms)
        assert!(
            elapsed < Duration::from_millis(10),
            "LED pattern generation too slow: {:?} for 1000 updates",
            elapsed
        );

        // Calculate per-update time
        let per_update = elapsed.as_nanos() / 1000;
        println!("LED pattern generation: {} ns per update", per_update);

        // Should be well under 1ms per update for real-time performance
        assert!(
            per_update < 1_000_000,
            "Per-update time too high: {} ns",
            per_update
        );
    }

    #[test]
    fn test_haptics_pattern_generation_performance() {
        let config = create_test_haptics_config();
        let mut router = HapticsRouter::new(config);

        let telemetry = create_test_telemetry(7000.0, 35.0, 0.5, 5);

        // Measure time for 1000 pattern updates
        let start = Instant::now();
        for _ in 0..1000 {
            let _patterns = router.update_patterns(&telemetry);
        }
        let elapsed = start.elapsed();

        // Should complete 1000 updates in reasonable time (< 5ms)
        assert!(
            elapsed < Duration::from_millis(5),
            "Haptics pattern generation too slow: {:?} for 1000 updates",
            elapsed
        );

        let per_update = elapsed.as_nanos() / 1000;
        println!("Haptics pattern generation: {} ns per update", per_update);

        // Should be well under 1ms per update
        assert!(
            per_update < 1_000_000,
            "Per-update time too high: {} ns",
            per_update
        );
    }

    #[test]
    fn test_dash_widget_update_performance() {
        let mut dash = DashWidgetSystem::new();

        let telemetry = create_test_telemetry(6500.0, 32.0, 0.1, 4);

        // Measure time for 1000 widget updates
        let start = Instant::now();
        for _ in 0..1000 {
            let _widgets = dash.update_widgets(&telemetry);
        }
        let elapsed = start.elapsed();

        // Should complete 1000 updates in reasonable time (< 5ms)
        assert!(
            elapsed < Duration::from_millis(5),
            "Dash widget updates too slow: {:?} for 1000 updates",
            elapsed
        );

        let per_update = elapsed.as_nanos() / 1000;
        println!("Dash widget update: {} ns per update", per_update);

        // Should be well under 1ms per update
        assert!(
            per_update < 1_000_000,
            "Per-update time too high: {} ns",
            per_update
        );
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_complete_system_integration() {
        let device_id = must(DeviceId::new("integration-test-device".to_string()));
        let led_config = create_test_led_config();
        let haptics_config = create_test_haptics_config();

        let (mut system, mut output_rx) = LedHapticsSystem::new(
            device_id.clone(),
            led_config,
            haptics_config,
            120.0, // 120 Hz
        );

        let (telemetry_tx, telemetry_rx) = mpsc::channel(100);

        // Start the system
        must(system.start(telemetry_rx).await);

        // Simulate a racing scenario with changing conditions
        let scenario_handle = tokio::spawn(async move {
            let scenarios = vec![
                // Scenario 1: Normal driving
                create_test_telemetry(4000.0, 20.0, 0.05, 3),
                // Scenario 2: High RPM
                create_test_telemetry(7500.0, 40.0, 0.1, 5),
                // Scenario 3: High slip (tire sliding)
                create_test_telemetry(6000.0, 30.0, 0.6, 4),
                // Scenario 4: Yellow flag
                {
                    let mut t = create_test_telemetry(5000.0, 25.0, 0.0, 3);
                    t.flags.yellow_flag = true;
                    t
                },
                // Scenario 5: Pit limiter
                {
                    let mut t = create_test_telemetry(3000.0, 15.0, 0.0, 2);
                    t.flags.pit_limiter = true;
                    t
                },
            ];

            for scenario in scenarios {
                let _ = telemetry_tx.send(scenario).await;
                sleep(Duration::from_millis(100)).await;
            }
        });

        // Collect and validate outputs
        let mut outputs = Vec::new();
        let collection_handle = tokio::spawn(async move {
            while outputs.len() < 10 {
                if let Ok(output) = timeout(Duration::from_millis(200), output_rx.recv()).await {
                    if let Some(output) = output {
                        outputs.push(output);
                    }
                } else {
                    break;
                }
            }
            outputs
        });

        // Wait for scenario completion
        must(scenario_handle.await);
        let outputs = must(collection_handle.await);

        // Stop the system
        system.stop();

        // Validate outputs
        assert!(!outputs.is_empty(), "Should have received outputs");

        for output in &outputs {
            // Validate basic structure
            assert_eq!(output.device_id, device_id);
            assert_eq!(output.led_colors.len(), 16);

            // Validate timing (outputs should be recent)
            let age = output.timestamp.elapsed();
            assert!(age < Duration::from_secs(5));

            // Validate that we have some form of output
            let has_led_output = output.led_colors.iter().any(|&c| c != LedColor::OFF);
            let has_haptics_output = !output.haptics_patterns.is_empty();
            let has_dash_output = !output.dash_widgets.is_empty();

            // At least one type of output should be active
            assert!(
                has_led_output || has_haptics_output || has_dash_output,
                "Output should have at least one active component"
            );
        }

        println!("Integration test completed with {} outputs", outputs.len());
    }
}
