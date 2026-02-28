//! Windows ETW (Event Tracing for Windows) provider

use crate::{AppTraceEvent, RTTraceEvent, TracingError, TracingMetrics, TracingProvider};
use std::sync::atomic::{AtomicU64, Ordering};

use windows::Win32::System::Diagnostics::Etw::{
    EVENT_DESCRIPTOR, EventRegister, EventUnregister, EventWrite, REGHANDLE,
};
use windows::core::GUID;

/// ETW Provider GUID for OpenRacing
const PROVIDER_GUID: u128 = 0x12345678_1234_5678_9ABC_123456789ABC;

/// Windows ETW provider implementation
///
/// Uses Event Tracing for Windows for high-performance, RT-safe tracing.
///
/// # RT Safety
///
/// ETW is RT-safe:
/// - EventWrite is designed for kernel-mode use
/// - No allocations in the hot path
/// - Bounded execution time
/// - Can be enabled/disabled dynamically via ETW sessions
pub struct WindowsETWProvider {
    provider_handle: Option<REGHANDLE>,
    rt_events_count: AtomicU64,
    app_events_count: AtomicU64,
}

impl WindowsETWProvider {
    /// Create a new ETW provider
    pub fn new() -> Result<Self, TracingError> {
        Ok(Self {
            provider_handle: None,
            rt_events_count: AtomicU64::new(0),
            app_events_count: AtomicU64::new(0),
        })
    }

    fn emit_etw_event(&self, handle: REGHANDLE, event: RTTraceEvent) {
        let event_descriptor = match event {
            RTTraceEvent::TickStart { .. } => EVENT_DESCRIPTOR {
                Id: 1,
                Version: 1,
                Channel: 0,
                Level: 4,
                Opcode: 1,
                Task: 1,
                Keyword: 0x1,
            },
            RTTraceEvent::TickEnd { .. } => EVENT_DESCRIPTOR {
                Id: 2,
                Version: 1,
                Channel: 0,
                Level: 4,
                Opcode: 2,
                Task: 1,
                Keyword: 0x1,
            },
            RTTraceEvent::HidWrite { .. } => EVENT_DESCRIPTOR {
                Id: 3,
                Version: 1,
                Channel: 0,
                Level: 4,
                Opcode: 0,
                Task: 2,
                Keyword: 0x2,
            },
            RTTraceEvent::DeadlineMiss { .. } => EVENT_DESCRIPTOR {
                Id: 4,
                Version: 1,
                Channel: 0,
                Level: 2,
                Opcode: 0,
                Task: 1,
                Keyword: 0x4,
            },
            RTTraceEvent::PipelineFault { .. } => EVENT_DESCRIPTOR {
                Id: 5,
                Version: 1,
                Channel: 0,
                Level: 1,
                Opcode: 0,
                Task: 3,
                Keyword: 0x4,
            },
        };

        unsafe {
            let _ = EventWrite(handle, &event_descriptor, None);
        }

        self.rt_events_count.fetch_add(1, Ordering::Relaxed);
    }

    fn emit_etw_app_event(&self, handle: REGHANDLE, _event: &AppTraceEvent) {
        let event_descriptor = EVENT_DESCRIPTOR {
            Id: 100,
            Version: 1,
            Channel: 0,
            Level: 4,
            Opcode: 0,
            Task: 10,
            Keyword: 0x10,
        };

        unsafe {
            let _ = EventWrite(handle, &event_descriptor, None);
        }

        self.app_events_count.fetch_add(1, Ordering::Relaxed);
    }
}

impl TracingProvider for WindowsETWProvider {
    fn initialize(&mut self) -> Result<(), TracingError> {
        let provider_guid = GUID::from_u128(PROVIDER_GUID);
        let mut handle = REGHANDLE(0);

        unsafe {
            let result = EventRegister(&provider_guid, None, None, &mut handle);

            if result != 0 {
                return Err(TracingError::InitializationFailed(format!(
                    "EventRegister failed with code: {}",
                    result
                )));
            }
        }

        self.provider_handle = Some(handle);
        tracing::info!("ETW provider initialized with handle: {}", handle.0);
        Ok(())
    }

    fn emit_rt_event(&self, event: RTTraceEvent) {
        if let Some(handle) = self.provider_handle {
            self.emit_etw_event(handle, event);
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

        if let Some(handle) = self.provider_handle {
            self.emit_etw_app_event(handle, &event);
        }
    }

    fn metrics(&self) -> TracingMetrics {
        TracingMetrics {
            rt_events_emitted: self.rt_events_count.load(Ordering::Relaxed),
            app_events_emitted: self.app_events_count.load(Ordering::Relaxed),
            ..Default::default()
        }
    }

    fn is_enabled(&self) -> bool {
        self.provider_handle.is_some()
    }

    fn shutdown(&mut self) {
        if let Some(handle) = self.provider_handle.take() {
            unsafe {
                let _ = EventUnregister(handle);
            }
            tracing::info!("ETW provider shutdown");
        }
    }
}

impl core::fmt::Debug for WindowsETWProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WindowsETWProvider")
            .field("provider_handle", &self.provider_handle.map(|h| h.0))
            .field(
                "rt_events_count",
                &self.rt_events_count.load(Ordering::Relaxed),
            )
            .field(
                "app_events_count",
                &self.app_events_count.load(Ordering::Relaxed),
            )
            .finish()
    }
}

impl Default for WindowsETWProvider {
    fn default() -> Self {
        // WindowsETWProvider::new() is infallible in practice
        Self::new().unwrap_or_else(|_| Self {
            provider_handle: None,
            rt_events_count: AtomicU64::new(0),
            app_events_count: AtomicU64::new(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_etw_provider_creation() {
        let result = WindowsETWProvider::new();
        assert!(result.is_ok());
    }

    #[test]
    fn test_etw_provider_lifecycle() {
        let mut provider = WindowsETWProvider::new().expect("creation failed");

        let init_result = provider.initialize();
        if init_result.is_ok() {
            provider.emit_rt_event(RTTraceEvent::TickStart {
                tick_count: 1,
                timestamp_ns: 1000,
            });

            let metrics = provider.metrics();
            assert_eq!(metrics.rt_events_emitted, 1);

            provider.shutdown();
            assert!(!provider.is_enabled());
        }
    }
}
