//! SPSC (Single Producer Single Consumer) shared memory for RT communication.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use shared_memory::{Shmem, ShmemConf};

use crate::error::NativePluginError;

/// Maximum shared memory size (4MB).
const MAX_SHARED_MEMORY_SIZE: usize = 4 * 1024 * 1024;

/// Default ring buffer capacity (frames).
const DEFAULT_FRAME_CAPACITY: u32 = 1024;

/// Shared memory header for SPSC communication.
#[repr(C)]
#[derive(Debug)]
pub struct SharedMemoryHeader {
    /// Protocol version.
    pub version: u32,
    /// Producer sequence number.
    pub producer_seq: AtomicU32,
    /// Consumer sequence number.
    pub consumer_seq: AtomicU32,
    /// Size of each frame in bytes.
    pub frame_size: u32,
    /// Maximum number of frames in the ring buffer.
    pub max_frames: u32,
    /// Shutdown flag.
    pub shutdown_flag: AtomicBool,
}

/// SPSC channel for single-producer single-consumer communication.
pub struct SpscChannel {
    shmem: Shmem,
    header: *mut SharedMemoryHeader,
    frame_size: usize,
    max_frames: u32,
}

unsafe impl Send for SpscChannel {}
unsafe impl Sync for SpscChannel {}

impl SpscChannel {
    /// Create a new SPSC channel with default capacity.
    pub fn new(frame_size: usize) -> Result<Self, NativePluginError> {
        Self::with_capacity(frame_size, DEFAULT_FRAME_CAPACITY)
    }

    /// Create a new SPSC channel with specified capacity.
    pub fn with_capacity(frame_size: usize, capacity: u32) -> Result<Self, NativePluginError> {
        let shmem_size =
            std::mem::size_of::<SharedMemoryHeader>() + (frame_size * capacity as usize);

        if shmem_size > MAX_SHARED_MEMORY_SIZE {
            return Err(NativePluginError::SharedMemoryError(format!(
                "Shared memory size {} exceeds maximum {}",
                shmem_size, MAX_SHARED_MEMORY_SIZE
            )));
        }

        let shmem = ShmemConf::new()
            .size(shmem_size)
            .create()
            .map_err(|e| NativePluginError::SharedMemoryError(e.to_string()))?;

        let header = unsafe {
            let ptr = shmem.as_ptr() as *mut SharedMemoryHeader;
            (*ptr).version = 1;
            (*ptr).producer_seq.store(0, Ordering::Relaxed);
            (*ptr).consumer_seq.store(0, Ordering::Relaxed);
            (*ptr).frame_size = frame_size as u32;
            (*ptr).max_frames = capacity;
            (*ptr).shutdown_flag.store(false, Ordering::Relaxed);
            ptr
        };

        Ok(Self {
            shmem,
            header,
            frame_size,
            max_frames: capacity,
        })
    }

    /// Open an existing SPSC channel by OS ID.
    pub fn open(shmem_id: &str) -> Result<Self, NativePluginError> {
        let shmem = ShmemConf::new()
            .os_id(shmem_id)
            .open()
            .map_err(|e| NativePluginError::SharedMemoryError(e.to_string()))?;

        let header = shmem.as_ptr() as *mut SharedMemoryHeader;

        let frame_size = unsafe { (*header).frame_size as usize };
        let max_frames = unsafe { (*header).max_frames };

        Ok(Self {
            shmem,
            header,
            frame_size,
            max_frames,
        })
    }

    /// Get the OS ID for this shared memory region.
    pub fn os_id(&self) -> &str {
        self.shmem.get_os_id()
    }

    /// Get a writer for this channel.
    pub fn writer(&self) -> SpscWriter {
        SpscWriter {
            header: self.header,
            frame_size: self.frame_size,
            max_frames: self.max_frames,
        }
    }

    /// Get a reader for this channel.
    pub fn reader(&self) -> SpscReader {
        SpscReader {
            header: self.header,
            frame_size: self.frame_size,
            max_frames: self.max_frames,
        }
    }

    /// Signal shutdown.
    pub fn shutdown(&self) {
        unsafe {
            (*self.header).shutdown_flag.store(true, Ordering::Release);
        }
    }

    /// Check if shutdown has been signaled.
    pub fn is_shutdown(&self) -> bool {
        unsafe { (*self.header).shutdown_flag.load(Ordering::Acquire) }
    }

    /// Get the frame size.
    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    /// Get the maximum frames.
    pub fn max_frames(&self) -> u32 {
        self.max_frames
    }
}

/// Writer for SPSC channel.
pub struct SpscWriter {
    header: *mut SharedMemoryHeader,
    frame_size: usize,
    max_frames: u32,
}

unsafe impl Send for SpscWriter {}
unsafe impl Sync for SpscWriter {}

impl SpscWriter {
    /// Write a frame to the channel.
    ///
    /// Returns `Ok(())` on success, or an error if the buffer is full.
    pub fn write(&self, frame: &[u8]) -> Result<(), NativePluginError> {
        if frame.len() != self.frame_size {
            return Err(NativePluginError::SharedMemoryError(format!(
                "Frame size mismatch: expected {}, got {}",
                self.frame_size,
                frame.len()
            )));
        }

        unsafe {
            let frames_ptr =
                (self.header as *mut u8).add(std::mem::size_of::<SharedMemoryHeader>());

            let producer_seq = (*self.header).producer_seq.load(Ordering::Acquire);
            let consumer_seq = (*self.header).consumer_seq.load(Ordering::Acquire);

            if producer_seq.wrapping_sub(consumer_seq) >= self.max_frames {
                return Err(NativePluginError::SharedMemoryError(
                    "Ring buffer full".to_string(),
                ));
            }

            let index = (producer_seq % self.max_frames) as usize;
            let offset = index * self.frame_size;
            std::ptr::copy_nonoverlapping(frame.as_ptr(), frames_ptr.add(offset), self.frame_size);

            (*self.header)
                .producer_seq
                .store(producer_seq.wrapping_add(1), Ordering::Release);
        }

        Ok(())
    }

    /// Try to write a frame without blocking.
    ///
    /// Returns `Ok(true)` if written, `Ok(false)` if buffer full.
    pub fn try_write(&self, frame: &[u8]) -> Result<bool, NativePluginError> {
        match self.write(frame) {
            Ok(()) => Ok(true),
            Err(NativePluginError::SharedMemoryError(msg)) if msg.contains("full") => Ok(false),
            Err(e) => Err(e),
        }
    }
}

/// Reader for SPSC channel.
pub struct SpscReader {
    header: *mut SharedMemoryHeader,
    frame_size: usize,
    max_frames: u32,
}

unsafe impl Send for SpscReader {}
unsafe impl Sync for SpscReader {}

impl SpscReader {
    /// Read a frame from the channel.
    ///
    /// Returns `Ok(frame)` on success, or an error if no data is available.
    pub fn read(&self, buffer: &mut [u8]) -> Result<(), NativePluginError> {
        if buffer.len() != self.frame_size {
            return Err(NativePluginError::SharedMemoryError(format!(
                "Buffer size mismatch: expected {}, got {}",
                self.frame_size,
                buffer.len()
            )));
        }

        unsafe {
            let frames_ptr =
                (self.header as *const u8).add(std::mem::size_of::<SharedMemoryHeader>());

            let producer_seq = (*self.header).producer_seq.load(Ordering::Acquire);
            let consumer_seq = (*self.header).consumer_seq.load(Ordering::Acquire);

            if consumer_seq >= producer_seq {
                return Err(NativePluginError::SharedMemoryError(
                    "No data available".to_string(),
                ));
            }

            let index = (consumer_seq % self.max_frames) as usize;
            let offset = index * self.frame_size;
            std::ptr::copy_nonoverlapping(
                frames_ptr.add(offset),
                buffer.as_mut_ptr(),
                self.frame_size,
            );

            (*self.header)
                .consumer_seq
                .store(consumer_seq.wrapping_add(1), Ordering::Release);
        }

        Ok(())
    }

    /// Try to read a frame without blocking.
    ///
    /// Returns `Ok(true)` if read, `Ok(false)` if no data.
    pub fn try_read(&self, buffer: &mut [u8]) -> Result<bool, NativePluginError> {
        match self.read(buffer) {
            Ok(()) => Ok(true),
            Err(NativePluginError::SharedMemoryError(msg)) if msg.contains("No data") => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Check if data is available.
    pub fn has_data(&self) -> bool {
        unsafe {
            let producer_seq = (*self.header).producer_seq.load(Ordering::Acquire);
            let consumer_seq = (*self.header).consumer_seq.load(Ordering::Acquire);
            consumer_seq < producer_seq
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spsc_channel_creation() {
        let frame_size = 64;
        let channel = SpscChannel::new(frame_size).expect("Failed to create channel");
        assert!(channel.frame_size() == frame_size);
        assert!(!channel.is_shutdown());
    }

    #[test]
    fn test_spsc_write_read() {
        let frame_size = 64;
        let channel = SpscChannel::new(frame_size).expect("Failed to create channel");

        let writer = channel.writer();
        let reader = channel.reader();

        let frame = vec![0x42u8; frame_size];
        writer.write(&frame).expect("Failed to write");

        let mut buffer = vec![0u8; frame_size];
        reader.read(&mut buffer).expect("Failed to read");

        assert_eq!(buffer, frame);
    }

    #[test]
    fn test_spsc_ring_buffer() {
        let frame_size = 16;
        let capacity = 4u32;
        let channel =
            SpscChannel::with_capacity(frame_size, capacity).expect("Failed to create channel");

        let writer = channel.writer();
        let reader = channel.reader();

        for i in 0u8..4 {
            let frame = vec![i; frame_size];
            writer.write(&frame).expect("Failed to write");
        }

        assert!(writer.write(&vec![0xFF; frame_size]).is_err());

        for i in 0u8..4 {
            let mut buffer = vec![0u8; frame_size];
            reader.read(&mut buffer).expect("Failed to read");
            assert_eq!(buffer, vec![i; frame_size]);
        }
    }

    #[test]
    fn test_shutdown_flag() {
        let channel = SpscChannel::new(64).expect("Failed to create channel");
        assert!(!channel.is_shutdown());

        channel.shutdown();
        assert!(channel.is_shutdown());
    }
}
