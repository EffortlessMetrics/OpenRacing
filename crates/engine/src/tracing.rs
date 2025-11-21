//! Real-time observability and tracing for the FFB engine
//!
//! This module provides platform-specific tracing capabilities:
//! - Windows: ETW (Event Tracing for Windows) provider
//! - Linux: Tracepoints or Perfetto integration
//! - Cross-platform: Structured logging with device/game context

use std::time::Instant;

/// Real-time trace events that can be emitted from the hot path
#[derive(Debug, Clone, Copy)]
pub enum RTTraceEvent {
    /// RT tick started
    TickStart {
        tick_count: u64,
        timestamp_ns: u64,
    },
    /// RT tick completed
    TickEnd {
        tick_count: u64,
        timestamp_ns: u64,
        processing_time_ns: u64,
    },
    /// HID write operation
    HidWrite {
        tick_count: u64,
        timestamp_ns: u64,
        torque_nm: f32,
        seq: u16,
    },
    /// Deadline miss detected
    DeadlineMiss {
        tick_count: u64,
        timestamp_ns: u64,
        jitter_ns: u64,
    },
    /// Pipeline fault occurred
    PipelineFault {
        tick_count: u64,
        timestamp_ns: u64,
        error_code: u8,
    },
}

/// Non-RT trace events for application-level logging
#[derive(Debug, Clone)]
pub enum AppTraceEvent {
    /// Device connected
    DeviceConnected {
        device_id: String,
        device_name: String,
        capabilities: String,
    },
    /// Device disconnected
    DeviceDisconnected {
        device_id: String,
        reason: String,
    },
    /// Game telemetry started
    TelemetryStarted {
        game_id: String,
        telemetry_rate_hz: f32,
    },
    /// Profile applied
    ProfileApplied {
        device_id: String,
        profile_name: String,
        profile_hash: String,
    },
    /// Safety state changed
    SafetyStateChanged {
        device_id: String,
        old_state: String,
        new_state: String,
        reason: String,
    },
}

/// Metrics collected for observability
#[derive(Debug, Clone)]
pub struct TracingMetrics {
    /// Total number of RT events emitted
    pub rt_events_emitted: u64,
    /// Total number of app events emitted
    pub app_events_emitted: u64,
    /// Number of events dropped due to buffer full
    pub events_dropped: u64,
    /// Last update timestamp
    pub last_update: Instant,
}

impl Default for TracingMetrics {
    fn default() -> Self {
        Self {
            rt_events_emitted: 0,
            app_events_emitted: 0,
            events_dropped: 0,
            last_update: Instant::now(),
        }
    }
}

/// Platform-specific tracing provider trait
pub trait TracingProvider: Send + Sync {
    /// Initialize the tracing provider
    fn initialize(&mut self) -> Result<(), TracingError>;
    
    /// Emit a real-time trace event (must be RT-safe)
    fn emit_rt_event(&self, event: RTTraceEvent);
    
    /// Emit an application trace event
    fn emit_app_event(&self, event: AppTraceEvent);
    
    /// Get current metrics
    fn metrics(&self) -> TracingMetrics;
    
    /// Shutdown the provider
    fn shutdown(&mut self);
}

/// Tracing errors
#[derive(Debug, thiserror::Error)]
pub enum TracingError {
    #[error("Platform not supported")]
    PlatformNotSupported,
    #[error("Provider initialization failed: {0}")]
    InitializationFailed(String),
    #[error("Event emission failed: {0}")]
    EmissionFailed(String),
}

/// Global tracing manager
pub struct TracingManager {
    provider: Box<dyn TracingProvider>,
    enabled: bool,
}

impl TracingManager {
    /// Create new tracing manager with platform-specific provider
    pub fn new() -> Result<Self, TracingError> {
        let provider = create_platform_provider()?;
        Ok(Self {
            provider,
            enabled: true,
        })
    }
    
    /// Initialize tracing
    pub fn initialize(&mut self) -> Result<(), TracingError> {
        self.provider.initialize()
    }
    
    /// Enable/disable tracing
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    
    /// Emit RT event (RT-safe if provider is RT-safe)
    #[inline]
    pub fn emit_rt_event(&self, event: RTTraceEvent) {
        if self.enabled {
            self.provider.emit_rt_event(event);
        }
    }
    
    /// Emit app event
    pub fn emit_app_event(&self, event: AppTraceEvent) {
        if self.enabled {
            self.provider.emit_app_event(event);
        }
    }
    
    /// Get metrics
    pub fn metrics(&self) -> TracingMetrics {
        self.provider.metrics()
    }
    
    /// Shutdown tracing
    pub fn shutdown(&mut self) {
        self.provider.shutdown();
    }
}

/// Create platform-specific tracing provider
fn create_platform_provider() -> Result<Box<dyn TracingProvider>, TracingError> {
    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(WindowsETWProvider::new()?))
    }
    
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(LinuxTracepointsProvider::new()?))
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Ok(Box::new(FallbackProvider::new()))
    }
}

/// Windows ETW provider implementation
#[cfg(target_os = "windows")]
pub struct WindowsETWProvider {
    provider_handle: Option<u64>,
    metrics: TracingMetrics,
}

#[cfg(target_os = "windows")]
impl WindowsETWProvider {
    pub fn new() -> Result<Self, TracingError> {
        Ok(Self {
            provider_handle: None,
            metrics: TracingMetrics::default(),
        })
    }
}

#[cfg(target_os = "windows")]
impl TracingProvider for WindowsETWProvider {
    fn initialize(&mut self) -> Result<(), TracingError> {
        use windows::Win32::System::Diagnostics::Etw::EventRegister;
        use windows::core::GUID;
        
        // Define our ETW provider GUID: {12345678-1234-5678-9ABC-123456789ABC}
        let provider_guid = GUID::from_u128(0x12345678_1234_5678_9ABC_123456789ABC);
        
        let mut handle: u64 = 0;
        
        unsafe {
            let result = EventRegister(
                &provider_guid,
                None, // No enable callback
                None, // No callback context
                &mut handle,
            );
            
            if result != 0 {
                return Err(TracingError::InitializationFailed(
                    format!("EventRegister failed with code: {}", result)
                ));
            }
        }
        
        self.provider_handle = Some(handle);
        tracing::info!("ETW provider initialized with handle: {}", handle);
        Ok(())
    }
    
    fn emit_rt_event(&self, event: RTTraceEvent) {
        if let Some(handle) = self.provider_handle {
            self.emit_etw_event(handle, event);
        }
    }
    
    fn emit_app_event(&self, event: AppTraceEvent) {
        // For app events, use structured logging as well as ETW
        match &event {
            AppTraceEvent::DeviceConnected { device_id, device_name, capabilities } => {
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
            AppTraceEvent::TelemetryStarted { game_id, telemetry_rate_hz } => {
                tracing::info!(
                    game_id = %game_id,
                    telemetry_rate_hz = %telemetry_rate_hz,
                    "Telemetry started"
                );
            }
            AppTraceEvent::ProfileApplied { device_id, profile_name, profile_hash } => {
                tracing::info!(
                    device_id = %device_id,
                    profile_name = %profile_name,
                    profile_hash = %profile_hash,
                    "Profile applied"
                );
            }
            AppTraceEvent::SafetyStateChanged { device_id, old_state, new_state, reason } => {
                tracing::warn!(
                    device_id = %device_id,
                    old_state = %old_state,
                    new_state = %new_state,
                    reason = %reason,
                    "Safety state changed"
                );
            }
        }
        
        if let Some(handle) = self.provider_handle {
            self.emit_etw_app_event(handle, event);
        }
    }
    
    fn metrics(&self) -> TracingMetrics {
        self.metrics.clone()
    }
    
    fn shutdown(&mut self) {
        if let Some(handle) = self.provider_handle.take() {
            use windows::Win32::System::Diagnostics::Etw::EventUnregister;
            
            unsafe {
                let _ = EventUnregister(handle);
            }
            
            tracing::info!("ETW provider shutdown");
        }
    }
}

#[cfg(target_os = "windows")]
impl WindowsETWProvider {
    fn emit_etw_event(&self, handle: u64, event: RTTraceEvent) {
        use windows::Win32::System::Diagnostics::Etw::{
            EventWrite, EVENT_DESCRIPTOR,
        };
        
        // Create event descriptor based on event type
        let event_descriptor = match event {
            RTTraceEvent::TickStart { .. } => EVENT_DESCRIPTOR {
                Id: 1,
                Version: 1,
                Channel: 0,
                Level: 4, // Informational
                Opcode: 1, // Start
                Task: 1,   // RT Task
                Keyword: 0x1, // RT keyword
            },
            RTTraceEvent::TickEnd { .. } => EVENT_DESCRIPTOR {
                Id: 2,
                Version: 1,
                Channel: 0,
                Level: 4, // Informational
                Opcode: 2, // Stop
                Task: 1,   // RT Task
                Keyword: 0x1, // RT keyword
            },
            RTTraceEvent::HidWrite { .. } => EVENT_DESCRIPTOR {
                Id: 3,
                Version: 1,
                Channel: 0,
                Level: 4, // Informational
                Opcode: 0, // Info
                Task: 2,   // HID Task
                Keyword: 0x2, // HID keyword
            },
            RTTraceEvent::DeadlineMiss { .. } => EVENT_DESCRIPTOR {
                Id: 4,
                Version: 1,
                Channel: 0,
                Level: 2, // Warning
                Opcode: 0, // Info
                Task: 1,   // RT Task
                Keyword: 0x4, // Error keyword
            },
            RTTraceEvent::PipelineFault { .. } => EVENT_DESCRIPTOR {
                Id: 5,
                Version: 1,
                Channel: 0,
                Level: 1, // Error
                Opcode: 0, // Info
                Task: 3,   // Pipeline Task
                Keyword: 0x4, // Error keyword
            },
        };
        
        // For simplicity, emit events without detailed data for now
        // In a production implementation, we would properly format the event data
        unsafe {
            let _ = EventWrite(handle, &event_descriptor, None);
        }
    }
    
    fn emit_etw_app_event(&self, handle: u64, _event: AppTraceEvent) {
        use windows::Win32::System::Diagnostics::Etw::{
            EventWrite, EVENT_DESCRIPTOR,
        };
        
        // For app events, we'll emit simpler ETW events and rely on structured logging for details
        let event_descriptor = EVENT_DESCRIPTOR {
            Id: 100, // App events start at 100
            Version: 1,
            Channel: 0,
            Level: 4, // Informational
            Opcode: 0, // Info
            Task: 10,  // App Task
            Keyword: 0x10, // App keyword
        };
        
        // For simplicity, emit without detailed data
        unsafe {
            let _ = EventWrite(handle, &event_descriptor, None);
        }
    }
}

/// Linux tracepoints provider implementation
#[cfg(target_os = "linux")]
pub struct LinuxTracepointsProvider {
    trace_fd: Option<std::fs::File>,
    metrics: TracingMetrics,
}

#[cfg(target_os = "linux")]
impl LinuxTracepointsProvider {
    pub fn new() -> Result<Self, TracingError> {
        Ok(Self {
            trace_fd: None,
            metrics: TracingMetrics::default(),
        })
    }
}

#[cfg(target_os = "linux")]
impl TracingProvider for LinuxTracepointsProvider {
    fn initialize(&mut self) -> Result<(), TracingError> {
        use std::fs::OpenOptions;
        use std::io::Write;
        
        // Try to open the trace marker file
        match OpenOptions::new()
            .write(true)
            .open("/sys/kernel/debug/tracing/trace_marker")
        {
            Ok(mut file) => {
                // Test write to ensure we have permissions
                if let Err(e) = writeln!(file, "wheel: tracing initialized") {
                    tracing::warn!("Failed to write to trace_marker: {}", e);
                    // Continue without tracepoints, fall back to structured logging only
                } else {
                    self.trace_fd = Some(file);
                    tracing::info!("Linux tracepoints initialized");
                }
            }
            Err(e) => {
                tracing::warn!("Failed to open trace_marker: {}, falling back to structured logging", e);
                // Continue without tracepoints
            }
        }
        
        Ok(())
    }
    
    fn emit_rt_event(&self, event: RTTraceEvent) {
        // For RT events, we need to be very careful about performance
        // Only emit to tracepoints if we have the file descriptor
        if let Some(_trace_fd) = &self.trace_fd {
            // In a real implementation, we'd write to the trace_marker file
            // For now, we'll just use a fast path that doesn't allocate
            self.emit_tracepoint_event(event);
        }
    }
    
    fn emit_app_event(&self, event: AppTraceEvent) {
        // For app events, use structured logging
        match &event {
            AppTraceEvent::DeviceConnected { device_id, device_name, capabilities } => {
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
            AppTraceEvent::TelemetryStarted { game_id, telemetry_rate_hz } => {
                tracing::info!(
                    game_id = %game_id,
                    telemetry_rate_hz = %telemetry_rate_hz,
                    "Telemetry started"
                );
            }
            AppTraceEvent::ProfileApplied { device_id, profile_name, profile_hash } => {
                tracing::info!(
                    device_id = %device_id,
                    profile_name = %profile_name,
                    profile_hash = %profile_hash,
                    "Profile applied"
                );
            }
            AppTraceEvent::SafetyStateChanged { device_id, old_state, new_state, reason } => {
                tracing::warn!(
                    device_id = %device_id,
                    old_state = %old_state,
                    new_state = %new_state,
                    reason = %reason,
                    "Safety state changed"
                );
            }
        }
        
        // Also emit to tracepoints if available
        if let Some(_trace_fd) = &self.trace_fd {
            self.emit_tracepoint_app_event(event);
        }
    }
    
    fn metrics(&self) -> TracingMetrics {
        self.metrics.clone()
    }
    
    fn shutdown(&mut self) {
        if let Some(_trace_fd) = self.trace_fd.take() {
            tracing::info!("Linux tracepoints provider shutdown");
        }
    }
}

#[cfg(target_os = "linux")]
impl LinuxTracepointsProvider {
    fn emit_tracepoint_event(&self, event: RTTraceEvent) {
        // In a real implementation, this would write to the trace_marker file
        // We need to be very careful about performance here
        match event {
            RTTraceEvent::TickStart { tick_count, timestamp_ns } => {
                let _ = (tick_count, timestamp_ns);
                // Format: "wheel_tick_start: tick=123 ts=456789"
                // This would be written to trace_marker
            }
            RTTraceEvent::TickEnd { tick_count, timestamp_ns, processing_time_ns } => {
                let _ = (tick_count, timestamp_ns, processing_time_ns);
                // Format: "wheel_tick_end: tick=123 ts=456789 proc_time=1000"
            }
            RTTraceEvent::HidWrite { tick_count, timestamp_ns, torque_nm, seq } => {
                let _ = (tick_count, timestamp_ns, torque_nm, seq);
                // Format: "wheel_hid_write: tick=123 ts=456789 torque=5.5 seq=42"
            }
            RTTraceEvent::DeadlineMiss { tick_count, timestamp_ns, jitter_ns } => {
                let _ = (tick_count, timestamp_ns, jitter_ns);
                // Format: "wheel_deadline_miss: tick=123 ts=456789 jitter=250000"
            }
            RTTraceEvent::PipelineFault { tick_count, timestamp_ns, error_code } => {
                let _ = (tick_count, timestamp_ns, error_code);
                // Format: "wheel_pipeline_fault: tick=123 ts=456789 error=3"
            }
        }
    }
    
    fn emit_tracepoint_app_event(&self, event: AppTraceEvent) {
        // Similar to RT events but for app-level events
        match event {
            AppTraceEvent::DeviceConnected { .. } => {
                // Format: "wheel_device_connected: ..."
            }
            AppTraceEvent::DeviceDisconnected { .. } => {
                // Format: "wheel_device_disconnected: ..."
            }
            AppTraceEvent::TelemetryStarted { .. } => {
                // Format: "wheel_telemetry_started: ..."
            }
            AppTraceEvent::ProfileApplied { .. } => {
                // Format: "wheel_profile_applied: ..."
            }
            AppTraceEvent::SafetyStateChanged { .. } => {
                // Format: "wheel_safety_state_changed: ..."
            }
        }
    }
}

/// Fallback provider for unsupported platforms
pub struct FallbackProvider {
    metrics: TracingMetrics,
}

impl FallbackProvider {
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
        // For RT events, we need to be very careful about performance
        // Only emit critical events to avoid impacting RT performance
        match event {
            RTTraceEvent::DeadlineMiss { tick_count, jitter_ns, .. } => {
                tracing::warn!(
                    tick_count = tick_count,
                    jitter_ns = jitter_ns,
                    "RT deadline miss"
                );
            }
            RTTraceEvent::PipelineFault { tick_count, error_code, .. } => {
                tracing::error!(
                    tick_count = tick_count,
                    error_code = error_code,
                    "RT pipeline fault"
                );
            }
            _ => {
                // Skip other RT events to avoid performance impact
            }
        }
    }
    
    fn emit_app_event(&self, event: AppTraceEvent) {
        // Use structured logging for all app events
        match &event {
            AppTraceEvent::DeviceConnected { device_id, device_name, capabilities } => {
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
            AppTraceEvent::TelemetryStarted { game_id, telemetry_rate_hz } => {
                tracing::info!(
                    game_id = %game_id,
                    telemetry_rate_hz = %telemetry_rate_hz,
                    "Telemetry started"
                );
            }
            AppTraceEvent::ProfileApplied { device_id, profile_name, profile_hash } => {
                tracing::info!(
                    device_id = %device_id,
                    profile_name = %profile_name,
                    profile_hash = %profile_hash,
                    "Profile applied"
                );
            }
            AppTraceEvent::SafetyStateChanged { device_id, old_state, new_state, reason } => {
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

/// Convenience macros for emitting trace events
#[macro_export]
macro_rules! trace_rt_tick_start {
    ($tracer:expr, $tick_count:expr, $timestamp_ns:expr) => {
        $tracer.emit_rt_event($crate::tracing::RTTraceEvent::TickStart {
            tick_count: $tick_count,
            timestamp_ns: $timestamp_ns,
        });
    };
}

#[macro_export]
macro_rules! trace_rt_tick_end {
    ($tracer:expr, $tick_count:expr, $timestamp_ns:expr, $processing_time_ns:expr) => {
        $tracer.emit_rt_event($crate::tracing::RTTraceEvent::TickEnd {
            tick_count: $tick_count,
            timestamp_ns: $timestamp_ns,
            processing_time_ns: $processing_time_ns,
        });
    };
}

#[macro_export]
macro_rules! trace_rt_hid_write {
    ($tracer:expr, $tick_count:expr, $timestamp_ns:expr, $torque_nm:expr, $seq:expr) => {
        $tracer.emit_rt_event($crate::tracing::RTTraceEvent::HidWrite {
            tick_count: $tick_count,
            timestamp_ns: $timestamp_ns,
            torque_nm: $torque_nm,
            seq: $seq,
        });
    };
}

#[macro_export]
macro_rules! trace_rt_deadline_miss {
    ($tracer:expr, $tick_count:expr, $timestamp_ns:expr, $jitter_ns:expr) => {
        $tracer.emit_rt_event($crate::tracing::RTTraceEvent::DeadlineMiss {
            tick_count: $tick_count,
            timestamp_ns: $timestamp_ns,
            jitter_ns: $jitter_ns,
        });
    };
}

#[macro_export]
macro_rules! trace_rt_pipeline_fault {
    ($tracer:expr, $tick_count:expr, $timestamp_ns:expr, $error_code:expr) => {
        $tracer.emit_rt_event($crate::tracing::RTTraceEvent::PipelineFault {
            tick_count: $tick_count,
            timestamp_ns: $timestamp_ns,
            error_code: $error_code as u8,
        });
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_manager_creation() {
        let manager = TracingManager::new();
        assert!(manager.is_ok());
    }

    #[test]
    fn test_fallback_provider() {
        let mut provider = FallbackProvider::new();
        assert!(provider.initialize().is_ok());
        
        // Test RT event emission
        provider.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 1000000,
        });
        
        // Test app event emission
        provider.emit_app_event(AppTraceEvent::DeviceConnected {
            device_id: "test-device".to_string(),
            device_name: "Test Device".to_string(),
            capabilities: "test-caps".to_string(),
        });
        
        let metrics = provider.metrics();
        assert_eq!(metrics.rt_events_emitted, 0); // Fallback doesn't count events
        
        provider.shutdown();
    }

    #[test]
    fn test_rt_trace_events() {
        let events = [
            RTTraceEvent::TickStart { tick_count: 1, timestamp_ns: 1000000 },
            RTTraceEvent::TickEnd { tick_count: 1, timestamp_ns: 1001000, processing_time_ns: 500 },
            RTTraceEvent::HidWrite { tick_count: 1, timestamp_ns: 1000500, torque_nm: 5.5, seq: 42 },
            RTTraceEvent::DeadlineMiss { tick_count: 2, timestamp_ns: 2000000, jitter_ns: 250000 },
            RTTraceEvent::PipelineFault { tick_count: 3, timestamp_ns: 3000000, error_code: 3 },
        ];
        
        let mut provider = FallbackProvider::new();
        provider.initialize().unwrap();
        
        for event in events {
            provider.emit_rt_event(event);
        }
    }

    #[test]
    fn test_app_trace_events() {
        let events = [
            AppTraceEvent::DeviceConnected {
                device_id: "dev1".to_string(),
                device_name: "Device 1".to_string(),
                capabilities: "caps1".to_string(),
            },
            AppTraceEvent::DeviceDisconnected {
                device_id: "dev1".to_string(),
                reason: "unplugged".to_string(),
            },
            AppTraceEvent::TelemetryStarted {
                game_id: "iracing".to_string(),
                telemetry_rate_hz: 60.0,
            },
            AppTraceEvent::ProfileApplied {
                device_id: "dev1".to_string(),
                profile_name: "gt3".to_string(),
                profile_hash: "abc123".to_string(),
            },
            AppTraceEvent::SafetyStateChanged {
                device_id: "dev1".to_string(),
                old_state: "safe".to_string(),
                new_state: "high_torque".to_string(),
                reason: "user_consent".to_string(),
            },
        ];
        
        let mut provider = FallbackProvider::new();
        provider.initialize().unwrap();
        
        for event in events {
            provider.emit_app_event(event);
        }
    }

    #[tokio::test]
    async fn test_tracing_manager_lifecycle() {
        let mut manager = TracingManager::new().unwrap();
        
        // Initialize
        manager.initialize().unwrap();
        
        // Test enabling/disabling
        manager.set_enabled(false);
        manager.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 1000000,
        });
        
        manager.set_enabled(true);
        manager.emit_rt_event(RTTraceEvent::TickEnd {
            tick_count: 1,
            timestamp_ns: 1001000,
            processing_time_ns: 500,
        });
        
        // Test app events
        manager.emit_app_event(AppTraceEvent::DeviceConnected {
            device_id: "test".to_string(),
            device_name: "Test".to_string(),
            capabilities: "test".to_string(),
        });
        
        // Get metrics
        let _metrics = manager.metrics();
        
        // Shutdown
        manager.shutdown();
    }
}