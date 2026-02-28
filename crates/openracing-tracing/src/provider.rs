//! Tracing provider trait definition

use crate::{AppTraceEvent, RTTraceEvent, TracingError, TracingMetrics};

/// Platform-specific tracing provider trait
///
/// Implementations must provide RT-safe event emission for [`RTTraceEvent`].
/// The [`emit_rt_event`](TracingProvider::emit_rt_event) method has strict
/// requirements:
///
/// # RT-Safety Requirements
///
/// - Must not allocate memory
/// - Must not block
/// - Must not acquire locks that could block
/// - Must complete in bounded time (typically < 100ns)
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` as the provider may be called
/// from multiple threads.
pub trait TracingProvider: Send + Sync {
    /// Initialize the tracing provider
    ///
    /// This method is called once at startup and may perform
    /// allocations, I/O, and other blocking operations.
    ///
    /// # Errors
    ///
    /// Returns an error if the provider cannot be initialized.
    fn initialize(&mut self) -> Result<(), TracingError>;

    /// Emit a real-time trace event
    ///
    /// This method must be RT-safe. See trait documentation for requirements.
    ///
    /// # Performance
    ///
    /// Implementations should target < 100ns for this method.
    fn emit_rt_event(&self, event: RTTraceEvent);

    /// Emit an application trace event
    ///
    /// This method is NOT RT-safe. It may allocate and block.
    /// Use for application-level events only.
    fn emit_app_event(&self, event: AppTraceEvent);

    /// Get current tracing metrics
    fn metrics(&self) -> TracingMetrics;

    /// Check if the provider is enabled
    ///
    /// Returns true if the provider can emit events.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Shutdown the provider
    ///
    /// Called during graceful shutdown. May perform blocking operations.
    fn shutdown(&mut self);
}

/// Create a platform-specific tracing provider
///
/// Returns the appropriate provider for the current platform:
/// - Windows: `WindowsETWProvider`
/// - Linux: `LinuxTracepointsProvider`
/// - Other: `FallbackProvider`
///
/// # Errors
///
/// Returns an error if the platform provider cannot be created.
pub fn create_platform_provider() -> Result<Box<dyn TracingProvider>, TracingError> {
    #[cfg(target_os = "windows")]
    {
        crate::platform::WindowsETWProvider::new().map(|p| Box::new(p) as Box<dyn TracingProvider>)
    }

    #[cfg(target_os = "linux")]
    {
        crate::platform::LinuxTracepointsProvider::new()
            .map(|p| Box::new(p) as Box<dyn TracingProvider>)
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Ok(Box::new(crate::platform::FallbackProvider::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_platform_provider() {
        let result = create_platform_provider();
        assert!(result.is_ok());
    }
}
