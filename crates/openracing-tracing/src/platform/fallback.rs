//! Fallback provider for unsupported platforms

use crate::{AppTraceEvent, RTTraceEvent, TracingError, TracingMetrics, TracingProvider};

/// Fallback provider for unsupported platforms
///
/// Uses structured logging for critical events only.
/// Non-critical RT events are silently dropped to avoid performance impact.
///
/// # RT Safety
///
/// This provider is RT-safe but intentionally limited:
/// - Only logs `DeadlineMiss` and `PipelineFault` events
/// - Other RT events are dropped to avoid logging overhead
/// - No buffering or async processing
pub struct FallbackProvider {
    metrics: TracingMetrics,
}

impl Default for FallbackProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl FallbackProvider {
    /// Create a new fallback provider
    pub fn new() -> Self {
        Self {
            metrics: TracingMetrics::default(),
        }
    }
}

impl TracingProvider for FallbackProvider {
    fn initialize(&mut self) -> Result<(), TracingError> {
        tracing::info!("Using fallback tracing provider (structured logging only)");
        Ok(())
    }

    fn emit_rt_event(&self, event: RTTraceEvent) {
        match event {
            RTTraceEvent::DeadlineMiss {
                tick_count,
                jitter_ns,
                ..
            } => {
                tracing::warn!(
                    tick_count = tick_count,
                    jitter_ns = jitter_ns,
                    "RT deadline miss"
                );
            }
            RTTraceEvent::PipelineFault {
                tick_count,
                error_code,
                ..
            } => {
                tracing::error!(
                    tick_count = tick_count,
                    error_code = error_code,
                    "RT pipeline fault"
                );
            }
            _ => {}
        }
    }

    fn emit_app_event(&self, event: AppTraceEvent) {
        match &event {
            AppTraceEvent::DeviceConnected {
                device_id,
                device_name,
                capabilities,
            } => {
                tracing::info!(
                    device_id = %device_id,
                    device_name = %device_name,
                    capabilities = %capabilities,
                    "Device connected"
                );
            }
            AppTraceEvent::DeviceDisconnected { device_id, reason } => {
                tracing::warn!(
                    device_id = %device_id,
                    reason = %reason,
                    "Device disconnected"
                );
            }
            AppTraceEvent::TelemetryStarted {
                game_id,
                telemetry_rate_hz,
            } => {
                tracing::info!(
                    game_id = %game_id,
                    telemetry_rate_hz = %telemetry_rate_hz,
                    "Telemetry started"
                );
            }
            AppTraceEvent::ProfileApplied {
                device_id,
                profile_name,
                profile_hash,
            } => {
                tracing::info!(
                    device_id = %device_id,
                    profile_name = %profile_name,
                    profile_hash = %profile_hash,
                    "Profile applied"
                );
            }
            AppTraceEvent::SafetyStateChanged {
                device_id,
                old_state,
                new_state,
                reason,
            } => {
                tracing::warn!(
                    device_id = %device_id,
                    old_state = %old_state,
                    new_state = %new_state,
                    reason = %reason,
                    "Safety state changed"
                );
            }
        }
    }

    fn metrics(&self) -> TracingMetrics {
        self.metrics.clone()
    }

    fn shutdown(&mut self) {
        tracing::info!("Fallback tracing provider shutdown");
    }
}

impl core::fmt::Debug for FallbackProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FallbackProvider")
            .field("metrics", &self.metrics)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_provider_initialization() {
        let mut provider = FallbackProvider::new();
        assert!(provider.initialize().is_ok());
    }

    #[test]
    fn test_fallback_provider_rt_events() {
        let provider = FallbackProvider::new();

        provider.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 1000,
        });

        provider.emit_rt_event(RTTraceEvent::DeadlineMiss {
            tick_count: 2,
            timestamp_ns: 2000,
            jitter_ns: 250,
        });

        provider.emit_rt_event(RTTraceEvent::PipelineFault {
            tick_count: 3,
            timestamp_ns: 3000,
            error_code: 5,
        });
    }

    #[test]
    fn test_fallback_provider_app_events() {
        let provider = FallbackProvider::new();

        provider.emit_app_event(AppTraceEvent::DeviceConnected {
            device_id: "dev1".to_string(),
            device_name: "Test".to_string(),
            capabilities: "caps".to_string(),
        });

        provider.emit_app_event(AppTraceEvent::SafetyStateChanged {
            device_id: "dev1".to_string(),
            old_state: "safe".to_string(),
            new_state: "warning".to_string(),
            reason: "test".to_string(),
        });
    }
}
