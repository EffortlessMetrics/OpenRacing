/// Write an `f32` in little-endian at `offset` into `buf`.
pub fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Create a zero-filled packet of the given `size`.
#[allow(dead_code)]
pub fn make_packet(size: usize) -> Vec<u8> {
    vec![0u8; size]
}
