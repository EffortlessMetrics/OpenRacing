//! Bincode compatibility helpers for .wbb serialization.

use serde::{Serialize, de::DeserializeOwned};
use std::io::Read;

fn config() -> impl bincode::config::Config {
    // Legacy config preserves bincode 1.x wire format semantics.
    bincode::config::legacy()
}

pub fn encode_to_vec<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    bincode::serde::encode_to_vec(value, config()).map_err(|e| e.to_string())
}

pub fn decode_from_slice<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let (value, used) =
        bincode::serde::decode_from_slice(bytes, config()).map_err(|e| e.to_string())?;
    if used != bytes.len() {
        return Err(format!("trailing bytes: used {used} of {}", bytes.len()));
    }
    Ok(value)
}

pub fn decode_from_std_read<T: DeserializeOwned, R: Read>(reader: &mut R) -> Result<T, String> {
    bincode::serde::decode_from_std_read(reader, config()).map_err(|e| e.to_string())
}
