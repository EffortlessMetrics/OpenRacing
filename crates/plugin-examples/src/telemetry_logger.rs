//! Telemetry logging plugin — captures snapshots into a ring buffer.
//!
//! This plugin demonstrates the **read-telemetry** capability. Every `N`
//! ticks it copies the current [`TelemetryFrame`] into a fixed-size ring
//! buffer that can be drained by an external consumer (e.g. a file writer
//! or network sender running on a background thread).
//!
//! # Real-time safety
//!
//! * The ring buffer is pre-allocated at construction time.
//! * [`TelemetryLoggerPlugin::record`] performs only a bounded copy —
//!   no heap allocations, no I/O, no locks.
//! * Suitable for the 1 kHz native-plugin path.

use openracing_plugin_abi::TelemetryFrame;

/// Configuration for the telemetry logger.
#[derive(Debug, Clone, Copy)]
pub struct TelemetryLoggerConfig {
    /// Record one frame every `decimation` ticks (1 = every tick).
    pub decimation: u32,
    /// Ring-buffer capacity (number of frames).
    pub capacity: usize,
}

impl Default for TelemetryLoggerConfig {
    fn default() -> Self {
        Self {
            decimation: 10,
            capacity: 1024,
        }
    }
}

/// A timestamped telemetry snapshot stored in the ring buffer.
#[derive(Debug, Clone, Copy)]
pub struct LogEntry {
    /// Monotonic tick counter at the time of capture.
    pub tick: u64,
    /// The captured telemetry frame.
    pub frame: TelemetryFrame,
}

/// Telemetry logger plugin.
pub struct TelemetryLoggerPlugin {
    config: TelemetryLoggerConfig,
    buffer: Vec<LogEntry>,
    /// Write cursor (wraps around capacity).
    write_idx: usize,
    /// Total entries written (may exceed capacity).
    total_written: u64,
    /// Current tick counter.
    tick: u64,
}

impl TelemetryLoggerPlugin {
    /// Create a new logger with pre-allocated storage.
    #[must_use]
    pub fn new(config: TelemetryLoggerConfig) -> Self {
        let cap = config.capacity.max(1);
        let buffer = vec![
            LogEntry {
                tick: 0,
                frame: TelemetryFrame::default(),
            };
            cap
        ];
        Self {
            config: TelemetryLoggerConfig {
                capacity: cap,
                ..config
            },
            buffer,
            write_idx: 0,
            total_written: 0,
            tick: 0,
        }
    }

    /// Record a telemetry frame (call once per tick).
    ///
    /// The frame is only stored every `decimation` ticks.
    /// Returns `true` if the frame was actually stored.
    pub fn record(&mut self, frame: &TelemetryFrame) -> bool {
        self.tick += 1;

        if self.config.decimation == 0 || !self.tick.is_multiple_of(self.config.decimation as u64) {
            return false;
        }

        self.buffer[self.write_idx] = LogEntry {
            tick: self.tick,
            frame: *frame,
        };
        self.write_idx = (self.write_idx + 1) % self.config.capacity;
        self.total_written += 1;
        true
    }

    /// Number of valid entries currently in the buffer.
    #[must_use]
    pub fn len(&self) -> usize {
        (self.total_written as usize).min(self.config.capacity)
    }

    /// Returns `true` if no entries have been recorded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.total_written == 0
    }

    /// Total number of entries written (including overwritten ones).
    #[must_use]
    pub fn total_written(&self) -> u64 {
        self.total_written
    }

    /// Drain all valid entries in chronological order.
    ///
    /// After draining, the buffer is logically empty and counters are reset.
    pub fn drain(&mut self) -> Vec<LogEntry> {
        let count = self.len();
        if count == 0 {
            return Vec::new();
        }

        let mut entries = Vec::with_capacity(count);
        let start = if self.total_written as usize > self.config.capacity {
            self.write_idx
        } else {
            0
        };

        for i in 0..count {
            let idx = (start + i) % self.config.capacity;
            entries.push(self.buffer[idx]);
        }

        self.write_idx = 0;
        self.total_written = 0;
        entries
    }

    /// Peek at the most recently recorded entry (if any).
    #[must_use]
    pub fn last_entry(&self) -> Option<&LogEntry> {
        if self.total_written == 0 {
            return None;
        }
        let idx = if self.write_idx == 0 {
            self.config.capacity - 1
        } else {
            self.write_idx - 1
        };
        Some(&self.buffer[idx])
    }

    /// Current tick counter.
    #[must_use]
    pub fn current_tick(&self) -> u64 {
        self.tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_at(ts: u64) -> TelemetryFrame {
        TelemetryFrame::new(ts)
    }

    #[test]
    fn empty_on_creation() {
        let plugin = TelemetryLoggerPlugin::new(TelemetryLoggerConfig::default());
        assert!(plugin.is_empty());
        assert_eq!(plugin.len(), 0);
        assert_eq!(plugin.total_written(), 0);
    }

    #[test]
    fn records_at_decimation_interval() {
        let config = TelemetryLoggerConfig {
            decimation: 3,
            capacity: 16,
        };
        let mut plugin = TelemetryLoggerPlugin::new(config);

        let mut stored_count = 0u32;
        for i in 0..9 {
            let frame = frame_at(i);
            if plugin.record(&frame) {
                stored_count += 1;
            }
        }
        // Ticks 3, 6, 9 → stored
        assert_eq!(stored_count, 3);
        assert_eq!(plugin.len(), 3);
    }

    #[test]
    fn ring_buffer_wraps() {
        let config = TelemetryLoggerConfig {
            decimation: 1,
            capacity: 4,
        };
        let mut plugin = TelemetryLoggerPlugin::new(config);

        for i in 0..10 {
            let frame = frame_at(i * 1000);
            plugin.record(&frame);
        }

        // Buffer holds last 4 entries, but total written is 10.
        assert_eq!(plugin.len(), 4);
        assert_eq!(plugin.total_written(), 10);
    }

    #[test]
    fn drain_returns_chronological_order() {
        let config = TelemetryLoggerConfig {
            decimation: 1,
            capacity: 4,
        };
        let mut plugin = TelemetryLoggerPlugin::new(config);

        for i in 0..6 {
            let frame = frame_at((i + 1) * 100);
            plugin.record(&frame);
        }

        let entries = plugin.drain();
        assert_eq!(entries.len(), 4);
        // Should be frames for ticks 3,4,5,6 (timestamps 300,400,500,600)
        for pair in entries.windows(2) {
            assert!(pair[0].tick < pair[1].tick);
        }

        // After drain, buffer is empty.
        assert!(plugin.is_empty());
    }

    #[test]
    fn last_entry_returns_most_recent() {
        let config = TelemetryLoggerConfig {
            decimation: 1,
            capacity: 8,
        };
        let mut plugin = TelemetryLoggerPlugin::new(config);
        assert!(plugin.last_entry().is_none());

        let frame = frame_at(999);
        plugin.record(&frame);
        let last = plugin.last_entry();
        assert!(last.is_some());
        assert_eq!(last.map(|e| e.frame.timestamp_us), Some(999));
    }

    #[test]
    fn decimation_zero_never_stores() {
        let config = TelemetryLoggerConfig {
            decimation: 0,
            capacity: 8,
        };
        let mut plugin = TelemetryLoggerPlugin::new(config);
        for _ in 0..10 {
            let stored = plugin.record(&TelemetryFrame::default());
            assert!(!stored);
        }
        assert!(plugin.is_empty());
    }

    #[test]
    fn capacity_clamped_to_at_least_one() {
        let config = TelemetryLoggerConfig {
            decimation: 1,
            capacity: 0,
        };
        let mut plugin = TelemetryLoggerPlugin::new(config);
        plugin.record(&frame_at(1));
        assert_eq!(plugin.len(), 1);
    }

    #[test]
    fn drain_empty_returns_empty_vec() {
        let mut plugin = TelemetryLoggerPlugin::new(TelemetryLoggerConfig::default());
        let entries = plugin.drain();
        assert!(entries.is_empty());
    }
}
