//! Real-time torque streaming primitives for HID backends.
//!
//! This module isolates lock-free mailbox reads, deterministic clamping, and
//! safe-zero behavior from backend-specific transport details.

use std::sync::atomic::{AtomicBool, AtomicI16, AtomicU8, AtomicU16, Ordering};

pub use racing_wheel_hid_moza_protocol::{TorqueEncoder, TorqueQ8_8};

/// RT I/O error classification for streaming writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtIoError {
    WouldBlock,
    Disconnected,
    WatchdogTimeout,
    Other,
}

/// RT writer abstraction.
///
/// Implementations must avoid allocation and blocking in `write`.
pub trait RtWriter {
    fn write(&mut self, bytes: &[u8]) -> Result<(), RtIoError>;
}

/// Lock-free command mailbox shared between non-RT producer and RT writer.
#[derive(Debug)]
pub struct TorqueMailbox {
    pub armed: AtomicBool,
    pub torque: AtomicI16,
    pub seq: AtomicU16,
    pub flags: AtomicU8,
}

impl TorqueMailbox {
    pub const fn new() -> Self {
        Self {
            armed: AtomicBool::new(false),
            torque: AtomicI16::new(0),
            seq: AtomicU16::new(0),
            flags: AtomicU8::new(0),
        }
    }
}

impl Default for TorqueMailbox {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for `RtTorqueStream`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamConfig {
    /// User/vehicle absolute torque limit in Q8.8 Nm.
    pub user_abs_limit: TorqueQ8_8,
    /// Number of consecutive ticks allowed with unchanged sequence value.
    pub watchdog_max_stale_ticks: u32,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            user_abs_limit: i16::MAX,
            watchdog_max_stale_ticks: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct RtWatchdog {
    seen_sequence: bool,
    last_sequence: u16,
    stale_ticks: u32,
    max_stale_ticks: u32,
}

impl RtWatchdog {
    fn new(max_stale_ticks: u32) -> Self {
        Self {
            seen_sequence: false,
            last_sequence: 0,
            stale_ticks: 0,
            max_stale_ticks,
        }
    }

    fn reset(&mut self) {
        self.seen_sequence = false;
        self.last_sequence = 0;
        self.stale_ticks = 0;
    }

    fn timed_out(&mut self, current_sequence: u16) -> bool {
        if !self.seen_sequence {
            self.seen_sequence = true;
            self.last_sequence = current_sequence;
            self.stale_ticks = 0;
            return false;
        }

        if current_sequence != self.last_sequence {
            self.last_sequence = current_sequence;
            self.stale_ticks = 0;
            return false;
        }

        self.stale_ticks = self.stale_ticks.saturating_add(1);
        self.stale_ticks >= self.max_stale_ticks
    }
}

/// RT torque streamer with deterministic per-tick behavior.
pub struct RtTorqueStream<'a, W, E, const N: usize> {
    writer: W,
    encoder: E,
    mailbox: &'a TorqueMailbox,
    buffer: [u8; N],
    user_abs_limit: TorqueQ8_8,
    watchdog: RtWatchdog,
}

impl<'a, W, E, const N: usize> RtTorqueStream<'a, W, E, N>
where
    W: RtWriter,
    E: TorqueEncoder<N>,
{
    pub fn new(writer: W, encoder: E, mailbox: &'a TorqueMailbox, config: StreamConfig) -> Self {
        let abs_limit = i32::from(config.user_abs_limit)
            .abs()
            .min(i32::from(i16::MAX)) as i16;

        Self {
            writer,
            encoder,
            mailbox,
            buffer: [0u8; N],
            user_abs_limit: abs_limit,
            watchdog: RtWatchdog::new(config.watchdog_max_stale_ticks),
        }
    }

    /// Update user absolute limit (Q8.8 Nm).
    pub fn set_user_abs_limit(&mut self, limit: TorqueQ8_8) {
        self.user_abs_limit = i32::from(limit).abs().min(i32::from(i16::MAX)) as i16;
    }

    /// Run one RT tick.
    pub fn tick(&mut self) -> Result<(), RtIoError> {
        if !self.mailbox.armed.load(Ordering::Relaxed) {
            self.emit_zero();
            self.watchdog.reset();
            return Ok(());
        }

        let requested_torque = self.mailbox.torque.load(Ordering::Relaxed);
        let sequence = self.mailbox.seq.load(Ordering::Relaxed);
        let flags = self.mailbox.flags.load(Ordering::Relaxed);

        if self.watchdog.timed_out(sequence) {
            self.emit_zero();
            self.mailbox.armed.store(false, Ordering::Relaxed);
            return Err(RtIoError::WatchdogTimeout);
        }

        let clamped_torque = self.clamp_torque(requested_torque);
        let len = self
            .encoder
            .encode(clamped_torque, sequence, flags, &mut self.buffer);

        match self.writer.write(&self.buffer[..len]) {
            Ok(()) | Err(RtIoError::WouldBlock) => Ok(()),
            Err(err) => {
                self.emit_zero();
                self.mailbox.armed.store(false, Ordering::Relaxed);
                self.watchdog.reset();
                Err(err)
            }
        }
    }

    fn clamp_torque(&self, requested: TorqueQ8_8) -> TorqueQ8_8 {
        let device_clamped = requested.clamp(self.encoder.clamp_min(), self.encoder.clamp_max());
        let user_limit = i32::from(self.user_abs_limit);
        i32::from(device_clamped)
            .clamp(-user_limit, user_limit)
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
    }

    fn emit_zero(&mut self) {
        let len = self.encoder.encode_zero(&mut self.buffer);
        let _ = self.writer.write(&self.buffer[..len]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestEncoder;

    impl TorqueEncoder<8> for TestEncoder {
        fn encode(&self, torque: TorqueQ8_8, seq: u16, flags: u8, out: &mut [u8; 8]) -> usize {
            out[0] = 0x20;
            let torque_bytes = torque.to_le_bytes();
            out[1] = torque_bytes[0];
            out[2] = torque_bytes[1];
            out[3] = flags;
            let seq_bytes = seq.to_le_bytes();
            out[4] = seq_bytes[0];
            out[5] = seq_bytes[1];
            6
        }

        fn encode_zero(&self, out: &mut [u8; 8]) -> usize {
            out[0] = 0x20;
            out[1] = 0;
            out[2] = 0;
            out[3] = 0;
            out[4] = 0;
            out[5] = 0;
            6
        }

        fn clamp_min(&self) -> TorqueQ8_8 {
            -512
        }

        fn clamp_max(&self) -> TorqueQ8_8 {
            512
        }

        fn positive_is_clockwise(&self) -> bool {
            true
        }
    }

    #[derive(Default)]
    struct ScriptedWriter {
        scripted_results: Vec<Result<(), RtIoError>>,
        writes: Vec<Vec<u8>>,
        call_index: usize,
    }

    impl RtWriter for ScriptedWriter {
        fn write(&mut self, bytes: &[u8]) -> Result<(), RtIoError> {
            self.writes.push(bytes.to_vec());
            let result = self
                .scripted_results
                .get(self.call_index)
                .copied()
                .unwrap_or(Ok(()));
            self.call_index = self.call_index.saturating_add(1);
            result
        }
    }

    #[test]
    fn test_disarmed_stream_writes_zero() {
        let mailbox = TorqueMailbox::new();
        let writer = ScriptedWriter::default();
        let mut stream =
            RtTorqueStream::new(writer, TestEncoder, &mailbox, StreamConfig::default());

        let result = stream.tick();
        assert!(result.is_ok());
        assert_eq!(stream.writer.writes.len(), 1);
        assert_eq!(stream.writer.writes[0][1], 0);
        assert_eq!(stream.writer.writes[0][2], 0);
    }

    #[test]
    fn test_armed_stream_applies_clamps() {
        let mailbox = TorqueMailbox::new();
        mailbox.armed.store(true, Ordering::Relaxed);
        mailbox.torque.store(800, Ordering::Relaxed);
        mailbox.seq.store(1, Ordering::Relaxed);
        mailbox.flags.store(0x05, Ordering::Relaxed);

        let config = StreamConfig {
            user_abs_limit: 300,
            watchdog_max_stale_ticks: 10,
        };
        let writer = ScriptedWriter::default();
        let mut stream = RtTorqueStream::new(writer, TestEncoder, &mailbox, config);

        let result = stream.tick();
        assert!(result.is_ok());
        assert_eq!(stream.writer.writes.len(), 1);

        let write = &stream.writer.writes[0];
        let torque = i16::from_le_bytes([write[1], write[2]]);
        assert_eq!(torque, 300);
        assert_eq!(write[3], 0x05);
        assert_eq!(u16::from_le_bytes([write[4], write[5]]), 1);
    }

    #[test]
    fn test_would_block_keeps_stream_armed() {
        let mailbox = TorqueMailbox::new();
        mailbox.armed.store(true, Ordering::Relaxed);
        mailbox.torque.store(100, Ordering::Relaxed);
        mailbox.seq.store(10, Ordering::Relaxed);

        let writer = ScriptedWriter {
            scripted_results: vec![Err(RtIoError::WouldBlock)],
            writes: Vec::new(),
            call_index: 0,
        };
        let mut stream =
            RtTorqueStream::new(writer, TestEncoder, &mailbox, StreamConfig::default());

        let result = stream.tick();
        assert!(result.is_ok());
        assert!(mailbox.armed.load(Ordering::Relaxed));
        assert_eq!(stream.writer.writes.len(), 1);
    }

    #[test]
    fn test_write_error_disarms_and_emits_zero() {
        let mailbox = TorqueMailbox::new();
        mailbox.armed.store(true, Ordering::Relaxed);
        mailbox.torque.store(120, Ordering::Relaxed);
        mailbox.seq.store(2, Ordering::Relaxed);

        let writer = ScriptedWriter {
            scripted_results: vec![Err(RtIoError::Disconnected), Ok(())],
            writes: Vec::new(),
            call_index: 0,
        };
        let mut stream =
            RtTorqueStream::new(writer, TestEncoder, &mailbox, StreamConfig::default());

        let result = stream.tick();
        assert_eq!(result, Err(RtIoError::Disconnected));
        assert!(!mailbox.armed.load(Ordering::Relaxed));
        assert_eq!(stream.writer.writes.len(), 2);

        let zero = &stream.writer.writes[1];
        assert_eq!(zero[1], 0);
        assert_eq!(zero[2], 0);
    }

    #[test]
    fn test_watchdog_timeout_disarms_on_stale_sequence() {
        let mailbox = TorqueMailbox::new();
        mailbox.armed.store(true, Ordering::Relaxed);
        mailbox.torque.store(100, Ordering::Relaxed);
        mailbox.seq.store(7, Ordering::Relaxed);

        let config = StreamConfig {
            user_abs_limit: 400,
            watchdog_max_stale_ticks: 1,
        };
        let writer = ScriptedWriter::default();
        let mut stream = RtTorqueStream::new(writer, TestEncoder, &mailbox, config);

        let first = stream.tick();
        assert!(first.is_ok());

        let second = stream.tick();
        assert_eq!(second, Err(RtIoError::WatchdogTimeout));
        assert!(!mailbox.armed.load(Ordering::Relaxed));
        assert_eq!(stream.writer.writes.len(), 2);
        let zero = &stream.writer.writes[1];
        assert_eq!(zero[1], 0);
        assert_eq!(zero[2], 0);
    }
}
