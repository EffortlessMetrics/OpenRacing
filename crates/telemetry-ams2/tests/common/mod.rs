use racing_wheel_telemetry_adapters::ams2::AMS2SharedMemory;
use racing_wheel_telemetry_ams2::AMS2Adapter;

pub fn adapter() -> AMS2Adapter {
    AMS2Adapter::new()
}

/// Serialize an `AMS2SharedMemory` to its raw byte representation for the normalize API.
pub fn shared_memory_to_bytes(data: &AMS2SharedMemory) -> Vec<u8> {
    let size = std::mem::size_of::<AMS2SharedMemory>();
    let ptr = data as *const AMS2SharedMemory as *const u8;
    // SAFETY: AMS2SharedMemory is repr(C) and fully initialized via Default in tests.
    unsafe { std::slice::from_raw_parts(ptr, size) }.to_vec()
}

/// Create a default `AMS2SharedMemory` without repeating private-field workarounds.
pub fn default_shared_memory() -> AMS2SharedMemory {
    AMS2SharedMemory::default()
}

/// Write a nul-terminated UTF-8 string into one of AMS2's fixed 64-byte fields.
pub fn write_fixed_str(buf: &mut [u8; 64], s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(buf.len() - 1);
    buf[..len].copy_from_slice(&bytes[..len]);
    buf[len] = 0;
}
