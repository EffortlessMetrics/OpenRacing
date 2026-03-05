//! Integration tests that exercise multiple plugin examples together.

use openracing_plugin_abi::TelemetryFrame;
use openracing_plugin_examples::dashboard_overlay::{
    DashboardConfig, DashboardOverlayPlugin, RaceFlag,
};
use openracing_plugin_examples::road_surface::{RoadSurfaceConfig, RoadSurfacePlugin};
use openracing_plugin_examples::telemetry_logger::{TelemetryLoggerConfig, TelemetryLoggerPlugin};

/// Simulate a short session: road-surface DSP feeding telemetry into the
/// logger while the dashboard computes display data each tick.
#[test]
fn combined_session_simulation() -> Result<(), Box<dyn std::error::Error>> {
    let mut road = RoadSurfacePlugin::new(RoadSurfaceConfig::default());
    let mut logger = TelemetryLoggerPlugin::new(TelemetryLoggerConfig {
        decimation: 5,
        capacity: 64,
    });
    let dash = DashboardOverlayPlugin::new(DashboardConfig::default());

    let dt = 0.001_f32; // 1 kHz
    let ticks = 100_u64;

    for i in 0..ticks {
        let wheel_speed = (i as f32 * 0.1).sin() * 10.0;
        let telem = TelemetryFrame::with_values(i * 1000, 0.0, wheel_speed, 35.0, 0);

        // DSP: modify FFB signal.
        let ffb_out = road.process(0.5, &telem, dt);
        assert!((-1.0..=1.0).contains(&ffb_out));

        // Logger: record telemetry.
        logger.record(&telem);

        // Dashboard: compute display data.
        let rpm = 3000.0 + (i as f32 * 50.0).min(5000.0);
        let gear = if rpm > 6000.0 { 4 } else { 3 };
        let data = dash.compute(&telem, rpm, gear, 0b0001);
        assert!(data.speed_kmh >= 0.0);
        assert_eq!(data.flag, RaceFlag::Green);
    }

    // Verify logger captured the expected number of entries.
    // 100 ticks / decimation 5 = 20 entries.
    assert_eq!(logger.total_written(), 20);

    let entries = logger.drain();
    assert_eq!(entries.len(), 20);
    // Entries should be chronologically ordered.
    for pair in entries.windows(2) {
        assert!(pair[0].tick < pair[1].tick);
    }

    Ok(())
}

/// Ensure road-surface plugin produces different output at different speeds
/// over multiple ticks.
#[test]
fn road_surface_speed_sensitivity() -> Result<(), Box<dyn std::error::Error>> {
    let config = RoadSurfaceConfig {
        intensity: 0.5,
        spatial_freq: 30.0,
        full_speed_rad_s: 5.0,
    };
    let mut slow = RoadSurfacePlugin::new(config);
    let mut fast = RoadSurfacePlugin::new(config);

    let dt = 0.001;
    let slow_telem = TelemetryFrame {
        wheel_speed_rad_s: 1.0,
        ..TelemetryFrame::default()
    };
    let fast_telem = TelemetryFrame {
        wheel_speed_rad_s: 10.0,
        ..TelemetryFrame::default()
    };

    let mut slow_sum = 0.0_f64;
    let mut fast_sum = 0.0_f64;
    let n = 500;
    for _ in 0..n {
        slow_sum += (slow.process(0.0, &slow_telem, dt) as f64).abs();
        fast_sum += (fast.process(0.0, &fast_telem, dt) as f64).abs();
    }

    // The fast scenario should produce a larger average deviation.
    let slow_avg = slow_sum / n as f64;
    let fast_avg = fast_sum / n as f64;
    assert!(
        fast_avg > slow_avg,
        "Expected fast ({fast_avg}) > slow ({slow_avg})"
    );

    Ok(())
}

/// Verify the dashboard correctly reacts to all gear values.
#[test]
fn dashboard_all_gears() -> Result<(), Box<dyn std::error::Error>> {
    let dash = DashboardOverlayPlugin::new(DashboardConfig::default());
    let telem = TelemetryFrame::default();

    let expected = [
        (-1_i8, 'R'),
        (0, 'N'),
        (1, '1'),
        (2, '2'),
        (3, '3'),
        (4, '4'),
        (5, '5'),
        (6, '6'),
        (7, '7'),
        (8, '8'),
        (9, '9'),
    ];

    for (gear, ch) in expected {
        let data = dash.compute(&telem, 4000.0, gear, 0);
        assert_eq!(data.gear_char, ch, "gear={gear}");
    }

    Ok(())
}
