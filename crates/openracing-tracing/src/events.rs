//! Trace event definitions for RT and application-level tracing

use core::fmt;

/// Real-time trace events that can be emitted from the hot path
///
/// # RT-Safety
///
/// All variants are RT-safe for emission:
/// - All data is stack-allocated (Copy trait)
/// - No heap allocations required
/// - Fixed-size data structures
/// - Bounded serialization time
///
/// # Performance
///
/// Each variant is designed to emit in under 100ns on modern hardware
/// when using platform-native tracing (ETW/tracepoints).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RTTraceEvent {
    /// RT tick started
    ///
    /// Emitted at the beginning of each RT cycle.
    /// Use for timing analysis and jitter detection.
    TickStart {
        /// Monotonic tick counter
        tick_count: u64,
        /// Timestamp in nanoseconds (monotonic clock)
        timestamp_ns: u64,
    },

    /// RT tick completed
    ///
    /// Emitted at the end of each RT cycle.
    /// Use for latency analysis and throughput measurement.
    TickEnd {
        /// Monotonic tick counter
        tick_count: u64,
        /// Timestamp in nanoseconds (monotonic clock)
        timestamp_ns: u64,
        /// Time spent processing this tick in nanoseconds
        processing_time_ns: u64,
    },

    /// HID write operation
    ///
    /// Emitted when torque data is written to the HID device.
    /// Use for correlating HID output with RT cycles.
    HidWrite {
        /// Monotonic tick counter
        tick_count: u64,
        /// Timestamp in nanoseconds (monotonic clock)
        timestamp_ns: u64,
        /// Torque value in Newton-meters
        torque_nm: f32,
        /// HID sequence number
        seq: u16,
    },

    /// Deadline miss detected
    ///
    /// Emitted when RT cycle exceeds its deadline.
    /// Critical for safety monitoring.
    DeadlineMiss {
        /// Monotonic tick counter
        tick_count: u64,
        /// Timestamp in nanoseconds (monotonic clock)
        timestamp_ns: u64,
        /// Amount of jitter in nanoseconds
        jitter_ns: u64,
    },

    /// Pipeline fault occurred
    ///
    /// Emitted when an error occurs in the RT pipeline.
    /// Critical for safety monitoring and diagnostics.
    PipelineFault {
        /// Monotonic tick counter
        tick_count: u64,
        /// Timestamp in nanoseconds (monotonic clock)
        timestamp_ns: u64,
        /// Error code identifying the fault type
        error_code: u8,
    },
}

impl RTTraceEvent {
    /// Returns the event type as a string for logging/tracing
    #[inline]
    pub const fn event_type(&self) -> &'static str {
        match self {
            RTTraceEvent::TickStart { .. } => "tick_start",
            RTTraceEvent::TickEnd { .. } => "tick_end",
            RTTraceEvent::HidWrite { .. } => "hid_write",
            RTTraceEvent::DeadlineMiss { .. } => "deadline_miss",
            RTTraceEvent::PipelineFault { .. } => "pipeline_fault",
        }
    }

    /// Returns the event category for filtering
    #[inline]
    pub const fn category(&self) -> RTEventCategory {
        match self {
            RTTraceEvent::TickStart { .. } | RTTraceEvent::TickEnd { .. } => {
                RTEventCategory::Timing
            }
            RTTraceEvent::HidWrite { .. } => RTEventCategory::Hid,
            RTTraceEvent::DeadlineMiss { .. } | RTTraceEvent::PipelineFault { .. } => {
                RTEventCategory::Error
            }
        }
    }

    /// Returns the tick count associated with this event
    #[inline]
    pub const fn tick_count(&self) -> u64 {
        match self {
            RTTraceEvent::TickStart { tick_count, .. }
            | RTTraceEvent::TickEnd { tick_count, .. }
            | RTTraceEvent::HidWrite { tick_count, .. }
            | RTTraceEvent::DeadlineMiss { tick_count, .. }
            | RTTraceEvent::PipelineFault { tick_count, .. } => *tick_count,
        }
    }

    /// Returns the timestamp in nanoseconds
    #[inline]
    pub const fn timestamp_ns(&self) -> u64 {
        match self {
            RTTraceEvent::TickStart { timestamp_ns, .. }
            | RTTraceEvent::TickEnd { timestamp_ns, .. }
            | RTTraceEvent::HidWrite { timestamp_ns, .. }
            | RTTraceEvent::DeadlineMiss { timestamp_ns, .. }
            | RTTraceEvent::PipelineFault { timestamp_ns, .. } => *timestamp_ns,
        }
    }

    /// Returns true if this is an error event
    #[inline]
    pub const fn is_error(&self) -> bool {
        matches!(
            self,
            RTTraceEvent::DeadlineMiss { .. } | RTTraceEvent::PipelineFault { .. }
        )
    }
}

/// Category for RT trace events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RTEventCategory {
    /// Timing events (tick start/end)
    Timing,
    /// HID events
    Hid,
    /// Error events
    Error,
}

/// Non-RT trace events for application-level logging
///
/// These events may allocate and are not suitable for the RT hot path.
/// Use for device lifecycle, configuration changes, and safety state transitions.
#[derive(Debug, Clone, PartialEq)]
pub enum AppTraceEvent {
    /// Device connected
    DeviceConnected {
        /// Unique device identifier
        device_id: String,
        /// Human-readable device name
        device_name: String,
        /// Device capabilities description
        capabilities: String,
    },

    /// Device disconnected
    DeviceDisconnected {
        /// Unique device identifier
        device_id: String,
        /// Reason for disconnection
        reason: String,
    },

    /// Game telemetry started
    TelemetryStarted {
        /// Game identifier
        game_id: String,
        /// Telemetry update rate in Hz
        telemetry_rate_hz: f32,
    },

    /// Profile applied
    ProfileApplied {
        /// Target device identifier
        device_id: String,
        /// Profile name
        profile_name: String,
        /// Hash of profile content for verification
        profile_hash: String,
    },

    /// Safety state changed
    SafetyStateChanged {
        /// Device identifier
        device_id: String,
        /// Previous safety state
        old_state: String,
        /// New safety state
        new_state: String,
        /// Reason for state change
        reason: String,
    },
}

impl AppTraceEvent {
    /// Returns the event category for filtering
    pub fn category(&self) -> AppEventCategory {
        match self {
            AppTraceEvent::DeviceConnected { .. } | AppTraceEvent::DeviceDisconnected { .. } => {
                AppEventCategory::Device
            }
            AppTraceEvent::TelemetryStarted { .. } => AppEventCategory::Telemetry,
            AppTraceEvent::ProfileApplied { .. } => AppEventCategory::Profile,
            AppTraceEvent::SafetyStateChanged { .. } => AppEventCategory::Safety,
        }
    }

    /// Returns the device ID if this event is device-related
    pub fn device_id(&self) -> Option<&str> {
        match self {
            AppTraceEvent::DeviceConnected { device_id, .. }
            | AppTraceEvent::DeviceDisconnected { device_id, .. }
            | AppTraceEvent::ProfileApplied { device_id, .. }
            | AppTraceEvent::SafetyStateChanged { device_id, .. } => Some(device_id),
            AppTraceEvent::TelemetryStarted { .. } => None,
        }
    }
}

/// Category for application trace events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEventCategory {
    /// Device lifecycle events
    Device,
    /// Telemetry events
    Telemetry,
    /// Profile events
    Profile,
    /// Safety events
    Safety,
}

impl fmt::Display for RTTraceEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RTTraceEvent::TickStart {
                tick_count,
                timestamp_ns,
            } => {
                write!(f, "TickStart(tick={}, ts={}ns)", tick_count, timestamp_ns)
            }
            RTTraceEvent::TickEnd {
                tick_count,
                timestamp_ns,
                processing_time_ns,
            } => {
                write!(
                    f,
                    "TickEnd(tick={}, ts={}ns, proc={}ns)",
                    tick_count, timestamp_ns, processing_time_ns
                )
            }
            RTTraceEvent::HidWrite {
                tick_count,
                timestamp_ns,
                torque_nm,
                seq,
            } => {
                write!(
                    f,
                    "HidWrite(tick={}, ts={}ns, torque={}, seq={})",
                    tick_count, timestamp_ns, torque_nm, seq
                )
            }
            RTTraceEvent::DeadlineMiss {
                tick_count,
                timestamp_ns,
                jitter_ns,
            } => {
                write!(
                    f,
                    "DeadlineMiss(tick={}, ts={}ns, jitter={}ns)",
                    tick_count, timestamp_ns, jitter_ns
                )
            }
            RTTraceEvent::PipelineFault {
                tick_count,
                timestamp_ns,
                error_code,
            } => {
                write!(
                    f,
                    "PipelineFault(tick={}, ts={}ns, error={})",
                    tick_count, timestamp_ns, error_code
                )
            }
        }
    }
}

impl fmt::Display for AppTraceEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppTraceEvent::DeviceConnected {
                device_id,
                device_name,
                capabilities,
            } => {
                write!(
                    f,
                    "DeviceConnected(id={}, name={}, caps={})",
                    device_id, device_name, capabilities
                )
            }
            AppTraceEvent::DeviceDisconnected { device_id, reason } => {
                write!(f, "DeviceDisconnected(id={}, reason={})", device_id, reason)
            }
            AppTraceEvent::TelemetryStarted {
                game_id,
                telemetry_rate_hz,
            } => {
                write!(
                    f,
                    "TelemetryStarted(game={}, rate={}Hz)",
                    game_id, telemetry_rate_hz
                )
            }
            AppTraceEvent::ProfileApplied {
                device_id,
                profile_name,
                profile_hash,
            } => {
                write!(
                    f,
                    "ProfileApplied(device={}, profile={}, hash={})",
                    device_id, profile_name, profile_hash
                )
            }
            AppTraceEvent::SafetyStateChanged {
                device_id,
                old_state,
                new_state,
                reason,
            } => {
                write!(
                    f,
                    "SafetyStateChanged(device={}, {}->{}, reason={})",
                    device_id, old_state, new_state, reason
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rt_event_category() {
        let start = RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 1000,
        };
        assert_eq!(start.category(), RTEventCategory::Timing);

        let hid = RTTraceEvent::HidWrite {
            tick_count: 1,
            timestamp_ns: 1000,
            torque_nm: 50.0,
            seq: 1,
        };
        assert_eq!(hid.category(), RTEventCategory::Hid);

        let miss = RTTraceEvent::DeadlineMiss {
            tick_count: 1,
            timestamp_ns: 1000,
            jitter_ns: 100,
        };
        assert_eq!(miss.category(), RTEventCategory::Error);
        assert!(miss.is_error());
    }

    #[test]
    fn test_rt_event_accessors() {
        let event = RTTraceEvent::TickEnd {
            tick_count: 42,
            timestamp_ns: 12345,
            processing_time_ns: 500,
        };

        assert_eq!(event.tick_count(), 42);
        assert_eq!(event.timestamp_ns(), 12345);
        assert!(!event.is_error());
    }

    #[test]
    fn test_app_event_category() {
        let connected = AppTraceEvent::DeviceConnected {
            device_id: "dev1".to_string(),
            device_name: "Test".to_string(),
            capabilities: "caps".to_string(),
        };
        assert_eq!(connected.category(), AppEventCategory::Device);
        assert_eq!(connected.device_id(), Some("dev1"));

        let telemetry = AppTraceEvent::TelemetryStarted {
            game_id: "iracing".to_string(),
            telemetry_rate_hz: 60.0,
        };
        assert_eq!(telemetry.category(), AppEventCategory::Telemetry);
        assert_eq!(telemetry.device_id(), None);
    }

    #[test]
    fn test_event_display() {
        let event = RTTraceEvent::HidWrite {
            tick_count: 1,
            timestamp_ns: 1000,
            torque_nm: 50.0,
            seq: 42,
        };
        let s = format!("{}", event);
        assert!(s.contains("HidWrite"));
        assert!(s.contains("torque=50"));
    }
}
