//! Observability service for metrics export and health monitoring
//!
//! This module provides:
//! - Prometheus metrics HTTP endpoint
//! - Health event streaming at 10-20Hz
//! - Integration with the engine metrics collector
//! - Structured logging configuration

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use prometheus::TextEncoder;
use racing_wheel_engine::{
    MetricsCollector, HealthEvent, HealthEventStreamer, AlertingThresholds, MetricsValidator
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::interval;
use tokio_stream::{Stream, StreamExt};
use tracing::{info, warn, error};

/// Observability service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Prometheus metrics endpoint address
    pub metrics_addr: SocketAddr,
    /// Health event streaming rate in Hz
    pub health_stream_rate_hz: f32,
    /// Metrics collection interval in milliseconds
    pub collection_interval_ms: u64,
    /// Enable Prometheus metrics export
    pub enable_prometheus: bool,
    /// Enable health event streaming
    pub enable_health_streaming: bool,
    /// Alerting thresholds
    pub alerting_thresholds: AlertingThresholds,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            metrics_addr: "127.0.0.1:9090".parse()
                .expect("Default metrics address should be valid"),
            health_stream_rate_hz: 15.0, // 15Hz for real-time monitoring
            collection_interval_ms: 1000, // Collect every second
            enable_prometheus: true,
            enable_health_streaming: true,
            alerting_thresholds: AlertingThresholds::default(),
        }
    }
}

/// Observability service state
#[derive(Clone)]
pub struct ObservabilityState {
    metrics_collector: Arc<RwLock<MetricsCollector>>,
    health_streamer: Arc<HealthEventStreamer>,
    #[allow(dead_code)]
    validator: Arc<MetricsValidator>,
    config: ObservabilityConfig,
}

/// Observability service for metrics and health monitoring
pub struct ObservabilityService {
    state: ObservabilityState,
    metrics_server_handle: Option<tokio::task::JoinHandle<()>>,
    collection_task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ObservabilityService {
    /// Create new observability service
    pub async fn new(config: ObservabilityConfig) -> Result<Self> {
        let metrics_collector = Arc::new(RwLock::new(
            MetricsCollector::new()
                .context("Failed to create metrics collector")?
        ));
        
        let health_streamer = {
            let collector = metrics_collector.read().await;
            collector.health_streamer()
        };
        
        let validator = Arc::new(MetricsValidator::new(config.alerting_thresholds.clone()));
        
        let state = ObservabilityState {
            metrics_collector,
            health_streamer,
            validator,
            config: config.clone(),
        };
        
        Ok(Self {
            state,
            metrics_server_handle: None,
            collection_task_handle: None,
        })
    }
    
    /// Start the observability service
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting observability service");
        
        // Start metrics collection task
        if self.state.config.enable_prometheus || self.state.config.enable_health_streaming {
            let collection_handle = self.start_metrics_collection().await?;
            self.collection_task_handle = Some(collection_handle);
        }
        
        // Start Prometheus metrics server
        if self.state.config.enable_prometheus {
            let server_handle = self.start_metrics_server().await?;
            self.metrics_server_handle = Some(server_handle);
        }
        
        info!(
            "Observability service started - metrics: {}, health streaming: {}",
            self.state.config.enable_prometheus,
            self.state.config.enable_health_streaming
        );
        
        Ok(())
    }
    
    /// Stop the observability service
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping observability service");
        
        // Stop metrics server
        if let Some(handle) = self.metrics_server_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        
        // Stop collection task
        if let Some(handle) = self.collection_task_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        
        info!("Observability service stopped");
        Ok(())
    }
    
    /// Get health event stream
    pub fn health_events(&self) -> impl Stream<Item = Result<HealthEvent, tokio_stream::wrappers::errors::BroadcastStreamRecvError>> {
        self.state.health_streamer.subscribe()
    }
    
    /// Get metrics collector for engine integration
    pub fn metrics_collector(&self) -> Arc<RwLock<MetricsCollector>> {
        self.state.metrics_collector.clone()
    }
    
    /// Start metrics collection task
    async fn start_metrics_collection(&self) -> Result<tokio::task::JoinHandle<()>> {
        let state = self.state.clone();
        let collection_interval = Duration::from_millis(state.config.collection_interval_ms);
        
        let handle = tokio::spawn(async move {
            let mut interval = interval(collection_interval);
            
            loop {
                interval.tick().await;
                
                // Collect metrics
                match state.metrics_collector.write().await.collect_metrics().await {
                    Ok(()) => {
                        // Metrics collected successfully
                    }
                    Err(e) => {
                        error!("Failed to collect metrics: {}", e);
                        
                        // Emit health event for collection failure
                        let event = HealthEventStreamer::create_event(
                            racing_wheel_engine::HealthEventType::Error,
                            racing_wheel_engine::HealthSeverity::Error,
                            format!("Metrics collection failed: {}", e),
                            None,
                            serde_json::json!({
                                "error": e.to_string(),
                                "timestamp": chrono::Utc::now()
                            }),
                        );
                        
                        if let Err(e) = state.health_streamer.emit(event) {
                            warn!("Failed to emit health event for metrics collection failure: {}", e);
                        }
                    }
                }
            }
        });
        
        Ok(handle)
    }
    
    /// Start Prometheus metrics HTTP server
    async fn start_metrics_server(&self) -> Result<tokio::task::JoinHandle<()>> {
        let state = self.state.clone();
        let addr = state.config.metrics_addr;
        
        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .route("/health", get(health_handler))
            .with_state(state);
        
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .context("Failed to bind metrics server")?;
        
        info!("Prometheus metrics server listening on {}", addr);
        
        let handle = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("Metrics server error: {}", e);
            }
        });
        
        Ok(handle)
    }
}

/// Prometheus metrics endpoint handler
async fn metrics_handler(State(state): State<ObservabilityState>) -> Response {
    let collector = state.metrics_collector.read().await;
    let registry = collector.prometheus_registry();
    
    let encoder = TextEncoder::new();
    let metric_families = registry.gather();
    
    match encoder.encode_to_string(&metric_families) {
        Ok(output) => {
            (
                StatusCode::OK,
                [("content-type", "text/plain; version=0.0.4")],
                output,
            ).into_response()
        }
        Err(e) => {
            error!("Failed to encode Prometheus metrics: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to encode metrics: {}", e),
            ).into_response()
        }
    }
}

/// Health check endpoint handler
async fn health_handler() -> Response {
    let health_status = serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now(),
        "service": "racing-wheel-observability"
    });
    
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        health_status.to_string(),
    ).into_response()
}

/// Health event streaming service for real-time monitoring
pub struct HealthStreamingService {
    streamer: Arc<HealthEventStreamer>,
    rate_hz: f32,
}

impl HealthStreamingService {
    /// Create new health streaming service
    pub fn new(streamer: Arc<HealthEventStreamer>, rate_hz: f32) -> Self {
        Self { streamer, rate_hz }
    }
    
    /// Start streaming health events at specified rate
    pub async fn start_streaming(&self) -> impl Stream<Item = HealthEvent> {
        let mut stream = self.streamer.subscribe();
        let interval_duration = Duration::from_secs_f32(1.0 / self.rate_hz);
        let mut interval = interval(interval_duration);
        
        async_stream::stream! {
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Periodic health check - emit system status
                        let event = HealthEventStreamer::create_event(
                            racing_wheel_engine::HealthEventType::PerformanceDegradation,
                            racing_wheel_engine::HealthSeverity::Info,
                            "System health check".to_string(),
                            None,
                            serde_json::json!({
                                "timestamp": chrono::Utc::now(),
                                "type": "periodic_health_check"
                            }),
                        );
                        yield event;
                    }
                    
                    event = stream.next() => {
                        match event {
                            Some(Ok(event)) => yield event,
                            Some(Err(e)) => {
                                warn!("Health stream error: {}", e);
                                // Emit error event
                                let error_event = HealthEventStreamer::create_event(
                                    racing_wheel_engine::HealthEventType::Error,
                                    racing_wheel_engine::HealthSeverity::Warning,
                                    format!("Health stream error: {}", e),
                                    None,
                                    serde_json::json!({
                                        "error": e.to_string(),
                                        "timestamp": chrono::Utc::now()
                                    }),
                                );
                                yield error_event;
                            }
                            None => break,
                        }
                    }
                }
            }
        }
    }
}

/// Structured logging configuration
pub struct LoggingConfig {
    /// Log level filter
    pub level: tracing::Level,
    /// Enable JSON formatting
    pub json_format: bool,
    /// Enable device context in logs
    pub include_device_context: bool,
    /// Enable game context in logs
    pub include_game_context: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: tracing::Level::INFO,
            json_format: false,
            include_device_context: true,
            include_game_context: true,
        }
    }
}

/// Initialize structured logging with device and game context
pub fn init_logging(config: LoggingConfig) -> Result<()> {
    use tracing_subscriber::{
        fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
    };
    
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.level.to_string()));
    
    let fmt_layer = if config.json_format {
        fmt::layer()
            .boxed()
    } else {
        fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_thread_names(true)
            .boxed()
    };
    
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
    
    info!(
        "Structured logging initialized - level: {}, json: {}, device_context: {}, game_context: {}",
        config.level,
        config.json_format,
        config.include_device_context,
        config.include_game_context
    );
    
    Ok(())
}

/// Device context for structured logging
#[derive(Debug, Clone)]
pub struct DeviceContext {
    pub device_id: String,
    pub device_name: String,
    pub device_type: String,
}

/// Game context for structured logging
#[derive(Debug, Clone)]
pub struct GameContext {
    pub game_id: String,
    pub game_version: Option<String>,
    pub car_id: Option<String>,
    pub track_id: Option<String>,
}

/// Logging span helpers for adding context
pub struct LoggingSpans;

impl LoggingSpans {
    /// Create device context span
    pub fn device_span(context: &DeviceContext) -> tracing::Span {
        tracing::info_span!(
            "device",
            device_id = %context.device_id,
            device_name = %context.device_name,
            device_type = %context.device_type
        )
    }
    
    /// Create game context span
    pub fn game_span(context: &GameContext) -> tracing::Span {
        tracing::info_span!(
            "game",
            game_id = %context.game_id,
            game_version = ?context.game_version,
            car_id = ?context.car_id,
            track_id = ?context.track_id
        )
    }
    
    /// Create RT operation span
    pub fn rt_span(operation: &str, tick_count: u64) -> tracing::Span {
        tracing::debug_span!(
            "rt_operation",
            operation = operation,
            tick_count = tick_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn test_observability_service_creation() {
        let config = ObservabilityConfig::default();
        let service = ObservabilityService::new(config).await;
        assert!(service.is_ok());
    }

    #[tokio::test]
    async fn test_health_streaming_service() {
        let streamer = Arc::new(HealthEventStreamer::new(100));
        let streaming_service = HealthStreamingService::new(streamer.clone(), 10.0);
        
        let mut stream = streaming_service.start_streaming().await;
        
        // Emit a test event
        let test_event = HealthEventStreamer::create_event(
            racing_wheel_engine::HealthEventType::DeviceStatus,
            racing_wheel_engine::HealthSeverity::Info,
            "Test event".to_string(),
            Some("test-device".to_string()),
            serde_json::json!({"test": true}),
        );
        
        streamer.emit(test_event.clone()).unwrap();
        
        // Should receive the event
        let received = tokio::time::timeout(
            Duration::from_millis(100),
            stream.next()
        ).await;
        
        assert!(received.is_ok());
        let event = received.unwrap().unwrap();
        assert_eq!(event.message, "Test event");
    }

    #[test]
    fn test_logging_config() {
        let config = LoggingConfig::default();
        assert_eq!(config.level, tracing::Level::INFO);
        assert!(!config.json_format);
        assert!(config.include_device_context);
        assert!(config.include_game_context);
    }

    #[test]
    fn test_device_context_span() {
        let context = DeviceContext {
            device_id: "test-device".to_string(),
            device_name: "Test Device".to_string(),
            device_type: "wheel".to_string(),
        };
        
        let span = LoggingSpans::device_span(&context);
        assert_eq!(span.name(), "device");
    }

    #[test]
    fn test_game_context_span() {
        let context = GameContext {
            game_id: "iracing".to_string(),
            game_version: Some("2024.1".to_string()),
            car_id: Some("gt3".to_string()),
            track_id: Some("spa".to_string()),
        };
        
        let span = LoggingSpans::game_span(&context);
        assert_eq!(span.name(), "game");
    }

    #[tokio::test]
    async fn test_observability_config_serialization() {
        let config = ObservabilityConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ObservabilityConfig = serde_json::from_str(&json).unwrap();
        
        assert_eq!(config.health_stream_rate_hz, deserialized.health_stream_rate_hz);
        assert_eq!(config.collection_interval_ms, deserialized.collection_interval_ms);
    }
}