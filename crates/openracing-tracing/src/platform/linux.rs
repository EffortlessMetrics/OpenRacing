//! Linux tracepoints provider

use crate::{AppTraceEvent, RTTraceEvent, TracingError, TracingMetrics, TracingProvider};
use std::fs::File;
use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Linux tracepoints provider implementation
///
/// Uses the trace_marker interface for RT-safe tracing.
///
/// # RT Safety
///
/// Tracepoints are RT-safe when using trace_marker:
/// - No allocations in the hot path
/// - Bounded execution time
/// - Can be enabled/disabled dynamically
///
/// # Permissions
///
/// Requires write access to `/sys/kernel/debug/tracing/trace_marker`.
/// This typically requires root or appropriate group membership.
pub struct LinuxTracepointsProvider {
    trace_file: Option<Mutex<File>>,
    rt_events_count: AtomicU64,
    app_events_count: AtomicU64,
    events_dropped: AtomicU64,
}

impl LinuxTracepointsProvider {
    /// Create a new tracepoints provider
    pub fn new() -> Result<Self, TracingError> {
        Ok(Self {
            trace_file: None,
            rt_events_count: AtomicU64::new(0),
            app_events_count: AtomicU64::new(0),
            events_dropped: AtomicU64::new(0),
        })
    }

    fn format_rt_event(&self, event: RTTraceEvent) -> [u8; 128] {
        let mut buf = [0u8; 128];
        let s = match event {
            RTTraceEvent::TickStart {
                tick_count,
                timestamp_ns,
            } => format!(
                "openracing_tick_start tick={} ts={}",
                tick_count, timestamp_ns
            ),
            RTTraceEvent::TickEnd {
                tick_count,
                timestamp_ns,
                processing_time_ns,
            } => format!(
                "openracing_tick_end tick={} ts={} proc={}",
                tick_count, timestamp_ns, processing_time_ns
            ),
            RTTraceEvent::HidWrite {
                tick_count,
                timestamp_ns,
                torque_nm,
                seq,
            } => format!(
                "openracing_hid_write tick={} ts={} torque={} seq={}",
                tick_count, timestamp_ns, torque_nm, seq
            ),
            RTTraceEvent::DeadlineMiss {
                tick_count,
                timestamp_ns,
                jitter_ns,
            } => format!(
                "openracing_deadline_miss tick={} ts={} jitter={}",
                tick_count, timestamp_ns, jitter_ns
            ),
            RTTraceEvent::PipelineFault {
                tick_count,
                timestamp_ns,
                error_code,
            } => format!(
                "openracing_pipeline_fault tick={} ts={} error={}",
                tick_count, timestamp_ns, error_code
            ),
        };

        let bytes = s.as_bytes();
        let len = bytes.len().min(buf.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        buf
    }
}

impl TracingProvider for LinuxTracepointsProvider {
    fn initialize(&mut self) -> Result<(), TracingError> {
        match File::options()
            .write(true)
            .open("/sys/kernel/debug/tracing/trace_marker")
        {
            Ok(mut file) => {
                if writeln!(file, "openracing: tracing initialized").is_ok() {
                    self.trace_file = Some(Mutex::new(file));
                    tracing::info!("Linux tracepoints initialized");
                } else {
                    tracing::warn!("Failed to write to trace_marker, falling back to logging");
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to open trace_marker: {}, falling back to structured logging",
                    e
                );
            }
        }

        Ok(())
    }

    fn emit_rt_event(&self, event: RTTraceEvent) {
        if let Some(ref mutex) = self.trace_file {
            // try_lock avoids blocking the RT thread; dropped events are counted
            if let Ok(mut file) = mutex.try_lock() {
                let buf = self.format_rt_event(event);
                let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
                file.write_all(&buf[..len]).ok();
                file.write_all(b"\n").ok();
                self.rt_events_count.fetch_add(1, Ordering::Relaxed);
            } else {
                self.events_dropped.fetch_add(1, Ordering::Relaxed);
            }
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

        self.app_events_count.fetch_add(1, Ordering::Relaxed);
    }

    fn metrics(&self) -> TracingMetrics {
        TracingMetrics {
            rt_events_emitted: self.rt_events_count.load(Ordering::Relaxed),
            app_events_emitted: self.app_events_count.load(Ordering::Relaxed),
            events_dropped: self.events_dropped.load(Ordering::Relaxed),
            ..Default::default()
        }
    }

    fn is_enabled(&self) -> bool {
        self.trace_file.is_some()
    }

    fn shutdown(&mut self) {
        if let Some(file) = self.trace_file.take() {
            if let Ok(mut f) = file.into_inner() {
                let _ = writeln!(f, "openracing: tracing shutdown");
            }
        }
        tracing::info!("Linux tracepoints provider shutdown");
    }
}

impl core::fmt::Debug for LinuxTracepointsProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LinuxTracepointsProvider")
            .field("trace_file", &self.trace_file.is_some())
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

impl Default for LinuxTracepointsProvider {
    fn default() -> Self {
        Self::new().expect("failed to create LinuxTracepointsProvider")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracepoints_provider_creation() {
        let result = LinuxTracepointsProvider::new();
        assert!(result.is_ok());
    }

    #[test]
    fn test_tracepoints_provider_lifecycle() {
        let mut provider = LinuxTracepointsProvider::new().expect("creation failed");

        let _ = provider.initialize();

        provider.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 1000,
        });

        provider.shutdown();
    }
}
