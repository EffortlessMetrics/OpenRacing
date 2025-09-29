//! Observability system demonstration
//!
//! This example shows how to integrate the observability system with the racing wheel service,
//! including metrics collection, health event streaming, and Prometheus export.

use racing_wheel_engine::{MetricsCollector, HealthEventType, HealthSeverity};
use racing_wheel_service::{
    ObservabilityService, ObservabilityConfig, LoggingConfig, init_logging,
    DeviceContext, GameContext, LoggingSpans,
};
use std::time::Duration;
use tokio_stream::StreamExt;
use tracing::{info, warn, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize structured logging with device and game context
    let logging_config = LoggingConfig {
        level: tracing::Level::INFO,
        json_format: false,
        include_device_context: true,
        include_game_context: true,
    };
    
    init_logging(logging_config)?;
    
    info!("Starting observability system demonstration");
    
    // Create observability configuration
    let mut obs_config = ObservabilityConfig::default();
    obs_config.metrics_addr = "127.0.0.1:9090".parse().unwrap();
    obs_config.health_stream_rate_hz = 10.0; // 10Hz for demo
    obs_config.collection_interval_ms = 500; // Collect every 500ms for demo
    
    // Start observability service
    let mut obs_service = ObservabilityService::new(obs_config).await?;
    obs_service.start().await?;
    
    info!("Observability service started on http://127.0.0.1:9090/metrics");
    
    // Get metrics collector for simulation
    let metrics_collector = obs_service.metrics_collector();
    
    // Simulate device and game contexts
    let device_context = DeviceContext {
        device_id: "fanatec-dd1".to_string(),
        device_name: "Fanatec DD1".to_string(),
        device_type: "direct_drive".to_string(),
    };
    
    let game_context = GameContext {
        game_id: "iracing".to_string(),
        game_version: Some("2024.1".to_string()),
        car_id: Some("gt3_bmw").to_string(),
        track_id: Some("spa_francorchamps").to_string(),
    };
    
    // Create health event stream
    let mut health_stream = obs_service.health_events();
    
    // Spawn task to monitor health events
    let health_monitor = tokio::spawn(async move {
        let mut event_count = 0;
        while let Some(event_result) = health_stream.next().await {
            match event_result {
                Ok(event) => {
                    event_count += 1;
                    match event.severity {
                        HealthSeverity::Info => {
                            info!("Health event #{}: {}", event_count, event.message);
                        }
                        HealthSeverity::Warning => {
                            warn!("Health event #{}: {}", event_count, event.message);
                        }
                        HealthSeverity::Error | HealthSeverity::Critical => {
                            error!("Health event #{}: {}", event_count, event.message);
                        }
                    }
                    
                    if event_count >= 20 {
                        break; // Stop after 20 events for demo
                    }
                }
                Err(e) => {
                    error!("Health stream error: {}", e);
                    break;
                }
            }
        }
        info!("Health monitor finished after {} events", event_count);
    });
    
    // Simulate racing wheel activity with structured logging
    let simulation_task = tokio::spawn(async move {
        for i in 0..60 {
            // Create device context span
            let _device_span = LoggingSpans::device_span(&device_context).entered();
            
            // Create game context span
            let _game_span = LoggingSpans::game_span(&game_context).entered();
            
            // Simulate different types of activity
            match i % 10 {
                0 => {
                    info!("Device connected and calibrated");
                }
                1 => {
                    info!("Profile applied: GT3 setup for Spa");
                }
                2 => {
                    info!("Telemetry started at 60Hz");
                }
                3 => {
                    info!("Force feedback active, torque: 8.5Nm");
                }
                4 => {
                    warn!("High torque saturation detected: 92%");
                }
                5 => {
                    info!("Lap completed: 2:18.456");
                }
                6 => {
                    warn!("Minor telemetry packet loss: 2.1%");
                }
                7 => {
                    info!("Profile switched to wet weather setup");
                }
                8 => {
                    error!("Temporary USB communication error (recovered)");
                }
                9 => {
                    info!("Session ended, saving profile");
                }
                _ => {}
            }
            
            // Simulate RT activity by updating atomic counters
            {
                let collector = metrics_collector.read().await;
                let counters = collector.atomic_counters();
                
                // Simulate 1000 ticks per iteration (1 second at 1kHz)
                for tick in 0..1000 {
                    counters.inc_tick();
                    
                    // Simulate occasional missed ticks
                    if tick % 500 == 0 && i > 10 {
                        counters.inc_missed_tick();
                    }
                    
                    // Simulate torque saturation
                    let is_saturated = (tick % 100) < 15; // 15% saturation
                    counters.record_torque_saturation(is_saturated);
                }
                
                // Simulate telemetry activity
                for _ in 0..60 {
                    counters.inc_telemetry_received();
                }
                
                // Simulate occasional packet loss
                if i % 7 == 0 {
                    counters.inc_telemetry_lost();
                }
                
                // Simulate safety events
                if i % 15 == 0 {
                    counters.inc_safety_event();
                }
                
                // Simulate profile switches
                if i % 20 == 0 {
                    counters.inc_profile_switch();
                }
            }
            
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        
        info!("Simulation completed");
    });
    
    // Run simulation and health monitoring concurrently
    let (sim_result, health_result) = tokio::join!(simulation_task, health_monitor);
    
    sim_result?;
    health_result?;
    
    // Give some time for final metrics collection
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Stop observability service
    obs_service.stop().await?;
    
    info!("Observability demonstration completed");
    info!("Metrics were available at: http://127.0.0.1:9090/metrics");
    info!("Health check was available at: http://127.0.0.1:9090/health");
    
    Ok(())
}