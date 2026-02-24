//! Tracing manager for coordinating trace event emission

use crate::{
    AppTraceEvent, RTTraceEvent, TracingError, TracingMetrics, TracingProvider,
    provider::create_platform_provider,
};

/// Global tracing manager
///
/// Coordinates trace event emission through a platform-specific provider.
/// Provides a unified interface for both RT and application-level events.
///
/// # Example
///
/// ```rust,ignore
/// use openracing_tracing::{TracingManager, RTTraceEvent};
///
/// let mut manager = TracingManager::new()?;
/// manager.initialize()?;
///
/// manager.emit_rt_event(RTTraceEvent::TickStart {
///     tick_count: 1,
///     timestamp_ns: 1_000_000,
/// });
///
/// manager.shutdown();
/// ```
///
/// # Thread Safety
///
/// The manager is `Send + Sync` when the underlying provider is.
/// RT events can be emitted from any thread.
pub struct TracingManager {
    provider: Box<dyn TracingProvider>,
    enabled: bool,
}

impl TracingManager {
    /// Create a new tracing manager with platform-specific provider
    ///
    /// # Errors
    ///
    /// Returns an error if the platform provider cannot be created.
    pub fn new() -> Result<Self, TracingError> {
        let provider = create_platform_provider()?;
        Ok(Self {
            provider,
            enabled: true,
        })
    }

    /// Create a new tracing manager with a custom provider
    ///
    /// Use this for testing or custom tracing implementations.
    pub fn with_provider(provider: Box<dyn TracingProvider>) -> Self {
        Self {
            provider,
            enabled: true,
        }
    }

    /// Initialize the tracing provider
    ///
    /// Must be called before emitting events.
    ///
    /// # Errors
    ///
    /// Returns an error if initialization fails.
    pub fn initialize(&mut self) -> Result<(), TracingError> {
        self.provider.initialize()
    }

    /// Enable or disable tracing
    ///
    /// When disabled, events are silently dropped.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if tracing is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled && self.provider.is_enabled()
    }

    /// Emit an RT trace event
    ///
    /// This method is RT-safe when the underlying provider is RT-safe.
    /// Events are silently dropped if tracing is disabled.
    #[inline]
    pub fn emit_rt_event(&self, event: RTTraceEvent) {
        if self.enabled {
            self.provider.emit_rt_event(event);
        }
    }

    /// Emit an application trace event
    ///
    /// This method is NOT RT-safe.
    pub fn emit_app_event(&self, event: AppTraceEvent) {
        if self.enabled {
            self.provider.emit_app_event(event);
        }
    }

    /// Get current tracing metrics
    pub fn metrics(&self) -> TracingMetrics {
        self.provider.metrics()
    }

    /// Shutdown the tracing provider
    ///
    /// Called during graceful shutdown.
    pub fn shutdown(&mut self) {
        self.provider.shutdown();
    }
}

impl core::fmt::Debug for TracingManager {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TracingManager")
            .field("enabled", &self.enabled)
            .field(
                "provider_type",
                &core::any::type_name_of_val(&*self.provider),
            )
            .finish()
    }
}

impl Default for TracingManager {
    fn default() -> Self {
        Self::new().expect("failed to create default TracingManager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct MockProvider {
        rt_events: Arc<Mutex<Vec<RTTraceEvent>>>,
        app_events: Arc<Mutex<Vec<AppTraceEvent>>>,
        initialized: bool,
    }

    impl MockProvider {
        fn new() -> Self {
            Self {
                rt_events: Arc::new(Mutex::new(Vec::new())),
                app_events: Arc::new(Mutex::new(Vec::new())),
                initialized: false,
            }
        }
    }

    impl TracingProvider for MockProvider {
        fn initialize(&mut self) -> Result<(), TracingError> {
            self.initialized = true;
            Ok(())
        }

        fn emit_rt_event(&self, event: RTTraceEvent) {
            if let Ok(mut events) = self.rt_events.lock() {
                events.push(event);
            }
        }

        fn emit_app_event(&self, event: AppTraceEvent) {
            if let Ok(mut events) = self.app_events.lock() {
                events.push(event);
            }
        }

        fn metrics(&self) -> TracingMetrics {
            TracingMetrics::default()
        }

        fn shutdown(&mut self) {
            self.initialized = false;
        }
    }

    #[test]
    fn test_tracing_manager_lifecycle() -> Result<(), TracingError> {
        let mut manager = TracingManager::new()?;
        manager.initialize()?;

        manager.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 1000,
        });

        manager.shutdown();
        Ok(())
    }

    #[test]
    fn test_tracing_manager_enable_disable() {
        let provider = MockProvider::new();
        let rt_events = provider.rt_events.clone();

        let mut manager = TracingManager::with_provider(Box::new(provider));
        manager.initialize().ok();

        manager.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 1000,
        });

        assert_eq!(rt_events.lock().map(|e| e.len()).unwrap_or(0), 1);

        manager.set_enabled(false);

        manager.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 2,
            timestamp_ns: 2000,
        });

        assert_eq!(rt_events.lock().map(|e| e.len()).unwrap_or(0), 1);
    }

    #[test]
    fn test_tracing_manager_with_provider() {
        let provider = MockProvider::new();
        let rt_events = provider.rt_events.clone();

        let mut manager = TracingManager::with_provider(Box::new(provider));
        assert!(manager.is_enabled());

        manager.initialize().ok();
        manager.emit_rt_event(RTTraceEvent::TickEnd {
            tick_count: 1,
            timestamp_ns: 1000,
            processing_time_ns: 100,
        });

        assert_eq!(rt_events.lock().map(|e| e.len()).unwrap_or(0), 1);
    }
}
