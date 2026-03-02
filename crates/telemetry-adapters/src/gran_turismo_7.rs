//! Gran Turismo 7 UDP telemetry adapter.
//!
//! Receives Salsa20-encrypted UDP packets on port 33740.
//! Protocol reverse-engineered by the community (Nenkai/gt7dashboard et al.).
//!
//! # Protocol overview
//! GT7 broadcasts 296-byte encrypted UDP packets from the PlayStation to any
//! host that has recently sent a heartbeat. Decryption uses Salsa20 with:
//! - Key: first 32 bytes of `"Simulator Interface Packet GT7 ver 0.0"`
//! - Nonce: derived from `[0x40..0x44]` of the raw packet — `iv1 = LE_u32`,
//!   `iv2 = iv1 ^ 0xDEAD_BEAF`, nonce = `[iv2_le, iv1_le]`
//!
//! A single-byte heartbeat (`b"A"`) must be sent to the PlayStation on port
//! 33739 every ~100 ms to keep the stream active.
//!
//! ## Verification against Nenkai/PDTools (2025-07)
//!
//! Verified against `SimulatorPacket.cs` and `SimulatorInterfaceCryptorGT7.cs`
//! from [Nenkai/PDTools](https://github.com/Nenkai/PDTools) (commit 5bb714c).
//!
//! - **Ports**: recv=33740, send(heartbeat)=33739 — matches `BindPortGT7`/`ReceivePortGT7`. ✓
//! - **Packet size**: 0x128 = 296 bytes (heartbeat type `"A"`, PacketType1). ✓
//! - **Salsa20 key**: `"Simulator Interface Packet GT7 v"` (first 32 bytes of full string). ✓
//! - **XOR key**: `0xDEADBEAF` — matches PDTools default `XorKey` for PacketType1. ✓
//! - **Nonce**: `[iv2_le, iv1_le]` where `iv2 = iv1 ^ XorKey` and iv1 from `[0x40..0x44]`. ✓
//! - **Magic**: `0x47375330` ("0S7G" LE) — matches PDTools `"G7S0"` check. ✓
//! - **Field offsets**: All verified against `SimulatorPacket.Read()` sequential layout:
//!   EngineRPM@0x3C, GasLevel@0x44, GasCapacity@0x48, MetersPerSecond@0x4C,
//!   WaterTemp@0x58, TireFL–RR@0x60–0x6C, LapCount@0x74, BestLap@0x78,
//!   LastLap@0x7C, MaxAlertRPM@0x8A, Flags@0x8E, Gear@0x90, Throttle@0x91,
//!   Brake@0x92, CarCode@0x124. ✓
//! - **Flags bitmask**: Paused(1<<1), RevLimit(1<<5), ASM(1<<10), TCS(1<<11) — matches
//!   PDTools `SimulatorFlags` enum. ✓
//! - **Gear encoding**: low nibble = current gear, high nibble = suggested gear. ✓
//!
//! ### Extended packet support (GT7 ≥ 1.42)
//!
//! PDTools documents two additional heartbeat types added in GT7 v1.42:
//! - PacketType2 (heartbeat `"B"`, XOR `0xDEADBEEF`): 0x13C = **316 bytes** — adds
//!   WheelRotation (rad), FillerFloatFB, Sway, Heave, Surge fields.
//! - PacketType3 (heartbeat `"~"`, XOR `0x55FABB4F`): 0x158 = **344 bytes** — adds
//!   car-type indicator, energy recovery, and unknown fields.
//!
//! Our adapter sends a PacketType3 heartbeat (`"~"`) by default to request the
//! maximum data. Decryption auto-detects the packet type from its length and
//! applies the correct XOR key. Backward compatibility is maintained: 296-byte
//! packets from older GT7 versions are still parsed correctly.

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFlags, TelemetryFrame, TelemetryReceiver,
    TelemetryValue, telemetry_now_ns,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// UDP port on which GT7 broadcasts telemetry.
/// Verified: Nenkai/PDTools BindPortGT7=33740; Bornhall/gt7telemetry ReceivePort=33740.
pub const GT7_RECV_PORT: u16 = 33740;
/// UDP port to which heartbeat packets must be sent to keep the stream alive.
/// Verified: Nenkai/PDTools ReceivePortGT7=33739; Bornhall/gt7telemetry SendPort=33739.
pub const GT7_SEND_PORT: u16 = 33739;

/// PacketType1 size: 0x128 = 296 bytes (heartbeat `"A"`, standard).
pub const PACKET_SIZE: usize = 296;
/// PacketType2 size: 0x13C = 316 bytes (heartbeat `"B"`, GT7 ≥ 1.42).
/// Adds WheelRotation, FillerFloatFB, Sway, Heave, Surge.
pub const PACKET_SIZE_TYPE2: usize = 0x13C; // 316
/// PacketType3 size: 0x158 = 344 bytes (heartbeat `"~"`, GT7 ≥ 1.42).
/// Adds car-type indicator, energy recovery, and additional unknowns.
pub const PACKET_SIZE_TYPE3: usize = 0x158; // 344
/// Maximum packet size across all known types (used for receive buffer).
pub const MAX_PACKET_SIZE: usize = PACKET_SIZE_TYPE3;

/// Magic number present in bytes 0–3 of a correctly decrypted packet.
pub const MAGIC: u32 = 0x4737_5330; // "0S7G" little-endian

/// Salsa20 decryption key: first 32 bytes of the GT7 protocol string.
const SALSA_KEY: &[u8; 32] = b"Simulator Interface Packet GT7 v";

// XOR keys used in Salsa20 nonce derivation, per packet type.
// Ref: Nenkai/PDTools SimulatorInterfaceCryptorGT7.cs + SimulatorInterfaceClient.cs
const XOR_KEY_TYPE1: u32 = 0xDEAD_BEAF;
const XOR_KEY_TYPE2: u32 = 0xDEAD_BEEF;
const XOR_KEY_TYPE3: u32 = 0x55FA_BB4F;

// ---------------------------------------------------------------------------
// Packet field offsets (all values are little-endian)
// Authoritative reference: Nenkai/PDTools SimulatorPacket.cs
// ---------------------------------------------------------------------------
pub const OFF_MAGIC: usize = 0x00;
const OFF_ENGINE_RPM: usize = 0x3C; // 60 — f32
const OFF_FUEL_LEVEL: usize = 0x44; // 68 — f32
const OFF_FUEL_CAPACITY: usize = 0x48; // 72 — f32
const OFF_SPEED_MS: usize = 0x4C; // 76 — f32
const OFF_WATER_TEMP: usize = 0x58; // 88 — f32
const OFF_TIRE_TEMP_FL: usize = 0x60; // 96 — f32
const OFF_TIRE_TEMP_FR: usize = 0x64; // 100 — f32
const OFF_TIRE_TEMP_RL: usize = 0x68; // 104 — f32
const OFF_TIRE_TEMP_RR: usize = 0x6C; // 108 — f32
const OFF_LAP_COUNT: usize = 0x74; // 116 — i16
const OFF_BEST_LAP_MS: usize = 0x78; // 120 — i32
const OFF_LAST_LAP_MS: usize = 0x7C; // 124 — i32
const OFF_MAX_ALERT_RPM: usize = 0x8A; // 138 — i16 (rev-limiter alert ceiling)
const OFF_FLAGS: usize = 0x8E; // 142 — i16 (SimulatorFlags)
const OFF_GEAR_BYTE: usize = 0x90; // 144 — u8 (low nibble = gear, high = suggested)
const OFF_THROTTLE: usize = 0x91; // 145 — u8
const OFF_BRAKE: usize = 0x92; // 146 — u8
const OFF_CAR_CODE: usize = 0x124; // 292 — i32

// --- Extended field offsets (PacketType2: ≥ 316 bytes) ---
// Ref: Nenkai/PDTools SimulatorPacket.cs `if (data.Length >= 0x13C)` block
const OFF_WHEEL_ROTATION: usize = 0x128; // 296 — f32 (radians)
#[allow(dead_code)] // Documented offset from PDTools; not yet consumed.
const OFF_FILLER_FLOAT_FB: usize = 0x12C; // 300 — f32 (purpose unknown)
const OFF_SWAY: usize = 0x130; // 304 — f32 (lateral motion)
const OFF_HEAVE: usize = 0x134; // 308 — f32 (vertical motion)
const OFF_SURGE: usize = 0x138; // 312 — f32 (longitudinal motion)

// --- Extended field offsets (PacketType3: ≥ 344 bytes) ---
// Ref: Nenkai/PDTools SimulatorPacket.cs `if (data.Length >= 0x158)` block
#[allow(dead_code)] // Documented offset from PDTools; unknown purpose.
const OFF_CAR_TYPE_BYTE1: usize = 0x13C; // 316 — u8 (unknown)
#[allow(dead_code)] // Documented offset from PDTools; unknown purpose.
const OFF_CAR_TYPE_BYTE2: usize = 0x13D; // 317 — u8 (unknown)
const OFF_CAR_TYPE_BYTE3: usize = 0x13E; // 318 — u8 (4 = electric)
#[allow(dead_code)] // Documented offset from PDTools; not yet consumed.
const OFF_NO_GAS_CONSUMPTION: usize = 0x13F; // 319 — u8
#[allow(dead_code)] // Documented offset from PDTools; unknown purpose (Vector4).
const OFF_UNK5_VEC4: usize = 0x140; // 320 — 4× f32 (Vector4)
const OFF_ENERGY_RECOVERY: usize = 0x150; // 336 — f32
#[allow(dead_code)] // Documented offset from PDTools; unknown purpose.
const OFF_UNK7: usize = 0x154; // 340 — f32

// GT7 flags bitmask (offset 0x8E, u16 little-endian — SimulatorFlags enum)
const FLAG_PAUSED: u16 = 1 << 1;
const FLAG_REV_LIMIT: u16 = 1 << 5;
const FLAG_ASM_ACTIVE: u16 = 1 << 10;
const FLAG_TCS_ACTIVE: u16 = 1 << 11;

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// GT7 packet type, determining heartbeat byte, XOR key, and packet size.
///
/// Ref: Nenkai/PDTools `SimInterfacePacketType` enum and
/// `SimulatorInterfaceClient.GetExpectedPacketSize()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gt7PacketType {
    /// Standard 296-byte packet. Heartbeat `"A"`, XOR `0xDEADBEAF`.
    Type1,
    /// Extended 316-byte packet (GT7 ≥ 1.42). Heartbeat `"B"`, XOR `0xDEADBEEF`.
    /// Adds WheelRotation, Sway, Heave, Surge.
    Type2,
    /// Full 344-byte packet (GT7 ≥ 1.42). Heartbeat `"~"`, XOR `0x55FABB4F`.
    /// Adds energy recovery and car-type indicator atop Type2 fields.
    Type3,
}

impl Gt7PacketType {
    /// Heartbeat byte to send to the PlayStation to request this packet type.
    pub const fn heartbeat(self) -> &'static [u8] {
        match self {
            Self::Type1 => b"A",
            Self::Type2 => b"B",
            Self::Type3 => b"~",
        }
    }

    /// Expected packet size in bytes.
    pub const fn expected_size(self) -> usize {
        match self {
            Self::Type1 => PACKET_SIZE,
            Self::Type2 => PACKET_SIZE_TYPE2,
            Self::Type3 => PACKET_SIZE_TYPE3,
        }
    }

    /// XOR key used in Salsa20 nonce derivation.
    pub const fn xor_key(self) -> u32 {
        match self {
            Self::Type1 => XOR_KEY_TYPE1,
            Self::Type2 => XOR_KEY_TYPE2,
            Self::Type3 => XOR_KEY_TYPE3,
        }
    }
}

/// Detect the packet type from the received packet length.
/// Returns `None` for unrecognised sizes.
fn detect_packet_type(len: usize) -> Option<Gt7PacketType> {
    match len {
        PACKET_SIZE_TYPE3 => Some(Gt7PacketType::Type3),
        PACKET_SIZE_TYPE2 => Some(Gt7PacketType::Type2),
        PACKET_SIZE => Some(Gt7PacketType::Type1),
        _ => None,
    }
}

/// Gran Turismo 7 telemetry adapter.
///
/// Listens for UDP packets on [`GT7_RECV_PORT`] and sends heartbeats back to
/// the source host on [`GT7_SEND_PORT`] to keep the stream alive.
pub struct GranTurismo7Adapter {
    recv_port: u16,
    update_rate: Duration,
    packet_type: Gt7PacketType,
}

impl Default for GranTurismo7Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GranTurismo7Adapter {
    pub fn new() -> Self {
        Self {
            recv_port: GT7_RECV_PORT,
            update_rate: Duration::from_millis(17), // ~60 Hz
            packet_type: Gt7PacketType::Type3,      // request maximum data by default
        }
    }

    /// Override the receive port (useful for testing with ephemeral ports).
    pub fn with_port(mut self, port: u16) -> Self {
        self.recv_port = port;
        self
    }

    /// Override the packet type (determines heartbeat byte and expected size).
    pub fn with_packet_type(mut self, packet_type: Gt7PacketType) -> Self {
        self.packet_type = packet_type;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for GranTurismo7Adapter {
    fn game_id(&self) -> &str {
        "gran_turismo_7"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let recv_port = self.recv_port;
        let heartbeat_payload: &'static [u8] = self.packet_type.heartbeat();

        tokio::spawn(async move {
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, recv_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind GT7 UDP socket on port {recv_port}: {e}");
                    return;
                }
            };
            info!("GT7 adapter listening on UDP port {recv_port}");

            let mut buf = [0u8; MAX_PACKET_SIZE + 16];
            let mut frame_seq = 0u64;
            let mut last_heartbeat = tokio::time::Instant::now();
            // Track the source address so heartbeats go to the right host.
            let mut source_addr: Option<SocketAddr> = None;

            loop {
                // Send heartbeat every 100 ms to keep the stream alive.
                if last_heartbeat.elapsed() >= Duration::from_millis(100) {
                    if let Some(addr) = source_addr {
                        let hb_addr = SocketAddr::new(addr.ip(), GT7_SEND_PORT);
                        let _ = socket.send_to(heartbeat_payload, hb_addr).await;
                    }
                    last_heartbeat = tokio::time::Instant::now();
                }

                match tokio::time::timeout(Duration::from_millis(50), socket.recv_from(&mut buf))
                    .await
                {
                    Ok(Ok((len, src))) => {
                        source_addr = Some(src);
                        match decrypt_and_parse(&buf[..len]) {
                            Ok(normalized) => {
                                let frame = TelemetryFrame::new(
                                    normalized,
                                    telemetry_now_ns(),
                                    frame_seq,
                                    len,
                                );
                                if tx.send(frame).await.is_err() {
                                    debug!("Receiver dropped, stopping GT7 monitoring");
                                    break;
                                }
                                frame_seq = frame_seq.saturating_add(1);
                            }
                            Err(e) => debug!("Failed to parse GT7 packet: {e}"),
                        }
                    }
                    Ok(Err(e)) => warn!("GT7 UDP receive error: {e}"),
                    Err(_) => {} // timeout — keep looping to send heartbeat
                }
            }
            info!("Stopped GT7 telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        decrypt_and_parse(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    /// GT7 runs on a PlayStation console; process detection is not applicable.
    async fn is_game_running(&self) -> Result<bool> {
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Decryption
// ---------------------------------------------------------------------------

/// Decrypt a raw GT7 packet and parse it into normalised telemetry.
///
/// Accepts packets of any recognised size (296, 316, or 344 bytes).
/// The XOR key for Salsa20 nonce derivation is selected automatically
/// based on the packet length.
pub(crate) fn decrypt_and_parse(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < PACKET_SIZE {
        return Err(anyhow!(
            "GT7 packet too short: expected at least {PACKET_SIZE}, got {}",
            data.len()
        ));
    }

    let pkt_type = detect_packet_type(data.len()).unwrap_or(Gt7PacketType::Type1);
    let pkt_len = pkt_type.expected_size().min(data.len());

    let mut buf = vec![0u8; pkt_len];
    buf.copy_from_slice(&data[..pkt_len]);
    salsa20_decrypt(&mut buf, pkt_type.xor_key());

    let magic = read_u32_le(&buf, OFF_MAGIC);
    if magic != MAGIC {
        return Err(anyhow!(
            "GT7 magic mismatch: expected 0x{MAGIC:08X}, got 0x{magic:08X}"
        ));
    }

    parse_decrypted_ext(&buf)
}

/// XOR the buffer in-place with the Salsa20 keystream.
///
/// The nonce is derived from 4 bytes at `[0x40..0x44]` of the **raw**
/// (pre-decryption) packet: `iv1 = LE_u32(buf[0x40..0x44])`, then
/// `iv2 = iv1 ^ xor_key`, and the 8-byte nonce is `[iv2_le, iv1_le]`.
///
/// `xor_key` differs per packet type:
/// - PacketType1: `0xDEADBEAF`
/// - PacketType2: `0xDEADBEEF`
/// - PacketType3: `0x55FABB4F`
pub(crate) fn salsa20_decrypt(buf: &mut [u8], xor_key: u32) {
    // Read the 4-byte IV seed from the raw packet.
    let iv1 = u32::from_le_bytes([buf[0x40], buf[0x41], buf[0x42], buf[0x43]]);
    let iv2 = iv1 ^ xor_key;

    let mut nonce = [0u8; 8];
    nonce[..4].copy_from_slice(&iv2.to_le_bytes());
    nonce[4..].copy_from_slice(&iv1.to_le_bytes());

    let pkt_len = buf.len();
    let blocks_needed = pkt_len.div_ceil(64);
    for block_idx in 0..blocks_needed {
        let ks = salsa20_block(SALSA_KEY, &nonce, block_idx as u64);
        let start = block_idx * 64;
        let end = (start + 64).min(pkt_len);
        for (b, k) in buf[start..end].iter_mut().zip(ks.iter()) {
            *b ^= k;
        }
    }
}

/// Backward-compatible wrapper using the Type1 XOR key on a fixed-size buffer.
#[cfg(test)]
pub(crate) fn salsa20_xor(buf: &mut [u8; PACKET_SIZE]) {
    salsa20_decrypt(buf, XOR_KEY_TYPE1);
}

/// Generate one 64-byte Salsa20 keystream block for the given counter value.
///
/// Implements Salsa20/20 as specified by D. J. Bernstein.
fn salsa20_block(key: &[u8; 32], nonce: &[u8; 8], counter: u64) -> [u8; 64] {
    // State layout (indices into the 16-word state array):
    //   0: sigma[0]  1–4: key[0..16]  5: sigma[1]  6–7: nonce  8–9: counter
    //  10: sigma[2] 11–14: key[16..32] 15: sigma[3]
    let mut state = [0u32; 16];
    state[0] = 0x6170_7865; // "expa"
    state[1] = u32::from_le_bytes([key[0], key[1], key[2], key[3]]);
    state[2] = u32::from_le_bytes([key[4], key[5], key[6], key[7]]);
    state[3] = u32::from_le_bytes([key[8], key[9], key[10], key[11]]);
    state[4] = u32::from_le_bytes([key[12], key[13], key[14], key[15]]);
    state[5] = 0x3320_646e; // "nd 3"
    state[6] = u32::from_le_bytes([nonce[0], nonce[1], nonce[2], nonce[3]]);
    state[7] = u32::from_le_bytes([nonce[4], nonce[5], nonce[6], nonce[7]]);
    state[8] = counter as u32;
    state[9] = (counter >> 32) as u32;
    state[10] = 0x7962_2d32; // "2-by"
    state[11] = u32::from_le_bytes([key[16], key[17], key[18], key[19]]);
    state[12] = u32::from_le_bytes([key[20], key[21], key[22], key[23]]);
    state[13] = u32::from_le_bytes([key[24], key[25], key[26], key[27]]);
    state[14] = u32::from_le_bytes([key[28], key[29], key[30], key[31]]);
    state[15] = 0x6b20_6574; // "te k"

    let mut working = state;

    // 20 rounds = 10 double rounds (column then row)
    for _ in 0..10 {
        // Column round
        qr(&mut working, 0, 4, 8, 12);
        qr(&mut working, 5, 9, 13, 1);
        qr(&mut working, 10, 14, 2, 6);
        qr(&mut working, 15, 3, 7, 11);
        // Row round
        qr(&mut working, 0, 1, 2, 3);
        qr(&mut working, 5, 6, 7, 4);
        qr(&mut working, 10, 11, 8, 9);
        qr(&mut working, 15, 12, 13, 14);
    }

    // Add initial state (feed-forward)
    for i in 0..16 {
        working[i] = working[i].wrapping_add(state[i]);
    }

    // Serialize to little-endian bytes
    let mut out = [0u8; 64];
    for (i, &word) in working.iter().enumerate() {
        out[i * 4..(i + 1) * 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

/// Salsa20 quarter-round operating on indexed slots of the state array.
#[inline]
fn qr(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[b] ^= s[a].wrapping_add(s[d]).rotate_left(7);
    s[c] ^= s[b].wrapping_add(s[a]).rotate_left(9);
    s[d] ^= s[c].wrapping_add(s[b]).rotate_left(13);
    s[a] ^= s[d].wrapping_add(s[c]).rotate_left(18);
}

// ---------------------------------------------------------------------------
// Packet parsing
// ---------------------------------------------------------------------------

/// Parse fields from an already-decrypted, magic-verified GT7 packet.
///
/// Works with standard (296-byte) and extended (316/344-byte) packets.
/// Extended fields (wheel rotation, motion data, energy recovery) are
/// populated when the buffer is large enough.
pub fn parse_decrypted(buf: &[u8; PACKET_SIZE]) -> Result<NormalizedTelemetry> {
    parse_decrypted_ext(buf)
}

/// Parse fields from an already-decrypted GT7 packet of any supported size.
pub fn parse_decrypted_ext(buf: &[u8]) -> Result<NormalizedTelemetry> {
    if buf.len() < PACKET_SIZE {
        return Err(anyhow!(
            "GT7 decrypted buffer too short: expected at least {PACKET_SIZE}, got {}",
            buf.len()
        ));
    }
    let rpm = read_f32_le(buf, OFF_ENGINE_RPM);
    let max_alert_rpm = read_u16_le(buf, OFF_MAX_ALERT_RPM) as f32;
    let max_rpm = max_alert_rpm.max(0.0);
    let speed_ms = read_f32_le(buf, OFF_SPEED_MS).max(0.0);
    let fuel_level = read_f32_le(buf, OFF_FUEL_LEVEL);
    let fuel_capacity = read_f32_le(buf, OFF_FUEL_CAPACITY);
    let water_temp = read_f32_le(buf, OFF_WATER_TEMP);
    let flags_raw = read_u16_le(buf, OFF_FLAGS);
    let lap_count = read_u16_le(buf, OFF_LAP_COUNT);
    let best_lap_ms = read_i32_le(buf, OFF_BEST_LAP_MS);
    let last_lap_ms = read_i32_le(buf, OFF_LAST_LAP_MS);

    // Throttle/brake are u8 [0..255] → normalised to [0.0, 1.0]
    let throttle = buf[OFF_THROTTLE] as f32 / 255.0;
    let brake = buf[OFF_BRAKE] as f32 / 255.0;

    // Gear: low nibble of gear byte (0 = neutral, 1–8 = forward gears)
    let gear_byte = buf[OFF_GEAR_BYTE];
    let gear: i8 = match gear_byte & 0x0F {
        0 => 0,
        g @ 1..=8 => g as i8,
        _ => 0,
    };

    // Tire temperatures: f32 Celsius clamped to u8 for the normalised field
    let tire_fl = read_f32_le(buf, OFF_TIRE_TEMP_FL).clamp(0.0, 255.0) as u8;
    let tire_fr = read_f32_le(buf, OFF_TIRE_TEMP_FR).clamp(0.0, 255.0) as u8;
    let tire_rl = read_f32_le(buf, OFF_TIRE_TEMP_RL).clamp(0.0, 255.0) as u8;
    let tire_rr = read_f32_le(buf, OFF_TIRE_TEMP_RR).clamp(0.0, 255.0) as u8;

    let fuel_percent = if fuel_capacity > 0.0 {
        (fuel_level / fuel_capacity).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let best_lap_s = if best_lap_ms > 0 {
        best_lap_ms as f32 / 1000.0
    } else {
        0.0
    };
    let last_lap_s = if last_lap_ms > 0 {
        last_lap_ms as f32 / 1000.0
    } else {
        0.0
    };

    let car_code = read_i32_le(buf, OFF_CAR_CODE);

    let telemetry_flags = TelemetryFlags {
        traction_control: (flags_raw & FLAG_TCS_ACTIVE) != 0,
        abs_active: (flags_raw & FLAG_ASM_ACTIVE) != 0,
        engine_limiter: (flags_raw & FLAG_REV_LIMIT) != 0,
        session_paused: (flags_raw & FLAG_PAUSED) != 0,
        ..TelemetryFlags::default()
    };

    let mut builder = NormalizedTelemetry::builder()
        .rpm(rpm)
        .max_rpm(max_rpm)
        .speed_ms(speed_ms)
        .throttle(throttle)
        .brake(brake)
        .gear(gear)
        .fuel_percent(fuel_percent)
        .engine_temp_c(water_temp)
        .tire_temps_c([tire_fl, tire_fr, tire_rl, tire_rr])
        .lap(lap_count)
        .best_lap_time_s(best_lap_s)
        .last_lap_time_s(last_lap_s)
        .flags(telemetry_flags);

    if car_code != 0 {
        builder = builder.car_id(format!("gt7_{car_code}"));
    }

    // --- PacketType2 extended fields (≥ 316 bytes) ---
    // Ref: Nenkai/PDTools SimulatorPacket.cs `if (data.Length >= 0x13C)`
    if buf.len() >= PACKET_SIZE_TYPE2 {
        let wheel_rotation = read_f32_le(buf, OFF_WHEEL_ROTATION);
        builder = builder.steering_angle(wheel_rotation);

        let sway = read_f32_le(buf, OFF_SWAY);
        let heave = read_f32_le(buf, OFF_HEAVE);
        let surge = read_f32_le(buf, OFF_SURGE);

        // Map motion data to g-force fields: sway = lateral, surge = longitudinal, heave = vertical.
        builder = builder
            .lateral_g(sway)
            .longitudinal_g(surge)
            .vertical_g(heave);

        // Store raw motion values in extended data for consumers that need them.
        builder = builder.extended("gt7_sway".to_owned(), TelemetryValue::Float(sway));
        builder = builder.extended("gt7_heave".to_owned(), TelemetryValue::Float(heave));
        builder = builder.extended("gt7_surge".to_owned(), TelemetryValue::Float(surge));
    }

    // --- PacketType3 extended fields (≥ 344 bytes) ---
    // Ref: Nenkai/PDTools SimulatorPacket.cs `if (data.Length >= 0x158)`
    if buf.len() >= PACKET_SIZE_TYPE3 {
        let car_type_byte = buf[OFF_CAR_TYPE_BYTE3]; // 4 = electric
        let energy_recovery = read_f32_le(buf, OFF_ENERGY_RECOVERY);

        builder = builder.extended(
            "gt7_car_type".to_owned(),
            TelemetryValue::Integer(car_type_byte as i32),
        );
        builder = builder.extended(
            "gt7_energy_recovery".to_owned(),
            TelemetryValue::Float(energy_recovery),
        );
    }

    Ok(builder.build())
}

// ---------------------------------------------------------------------------
// Low-level read helpers
// ---------------------------------------------------------------------------

fn read_f32_le(data: &[u8], offset: usize) -> f32 {
    let val = data
        .get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
        .unwrap_or(0.0);
    if val.is_finite() { val } else { 0.0 }
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or(0)
}

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    data.get(offset..offset + 2)
        .and_then(|b| b.try_into().ok())
        .map(u16::from_le_bytes)
        .unwrap_or(0)
}

fn read_i32_le(data: &[u8], offset: usize) -> i32 {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(i32::from_le_bytes)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    fn buf_with_magic() -> [u8; PACKET_SIZE] {
        let mut buf = [0u8; PACKET_SIZE];
        buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        buf
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Any input shorter than PACKET_SIZE must return Err from decrypt_and_parse.
        #[test]
        fn prop_short_input_returns_err(len in 0usize..PACKET_SIZE) {
            let data = vec![0u8; len];
            prop_assert!(decrypt_and_parse(&data).is_err());
        }

        /// Arbitrary bytes at PACKET_SIZE must never panic in decrypt_and_parse.
        #[test]
        fn prop_arbitrary_packet_no_panic(
            data in proptest::collection::vec(any::<u8>(), PACKET_SIZE..=PACKET_SIZE)
        ) {
            let _ = decrypt_and_parse(&data);
        }

        /// parse_decrypted with valid magic and finite speed produces non-negative speed.
        #[test]
        fn prop_speed_non_negative(speed in 0.0f32..=300.0f32) {
            let mut buf = buf_with_magic();
            buf[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&speed.to_le_bytes());
            let t = parse_decrypted(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                t.speed_ms >= 0.0 && t.speed_ms.is_finite(),
                "speed_ms {} must be finite and non-negative",
                t.speed_ms
            );
        }

        /// Throttle is always normalized to [0, 1] (u8 → f32 / 255).
        #[test]
        fn prop_throttle_normalized(throttle_byte in 0u8..=255u8) {
            let mut buf = buf_with_magic();
            buf[OFF_THROTTLE] = throttle_byte;
            let t = parse_decrypted(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                t.throttle >= 0.0 && t.throttle <= 1.0,
                "throttle {} must be in [0, 1]",
                t.throttle
            );
        }

        /// Brake is always normalized to [0, 1] (u8 → f32 / 255).
        #[test]
        fn prop_brake_normalized(brake_byte in 0u8..=255u8) {
            let mut buf = buf_with_magic();
            buf[OFF_BRAKE] = brake_byte;
            let t = parse_decrypted(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                t.brake >= 0.0 && t.brake <= 1.0,
                "brake {} must be in [0, 1]",
                t.brake
            );
        }

        /// Gear (low nibble of gear byte) is always in [0, 8].
        #[test]
        fn prop_gear_in_range(gear_byte in 0u8..=255u8) {
            let mut buf = buf_with_magic();
            buf[OFF_GEAR_BYTE] = gear_byte;
            let t = parse_decrypted(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                t.gear >= 0 && t.gear <= 8,
                "gear {} must be in [0, 8]",
                t.gear
            );
        }

        /// Fuel percent is always in [0, 1] when capacity is positive.
        #[test]
        fn prop_fuel_percent_in_range(
            fuel in 0.0f32..=100.0f32,
            cap in 1.0f32..=200.0f32,
        ) {
            let mut buf = buf_with_magic();
            buf[OFF_FUEL_LEVEL..OFF_FUEL_LEVEL + 4].copy_from_slice(&fuel.to_le_bytes());
            buf[OFF_FUEL_CAPACITY..OFF_FUEL_CAPACITY + 4].copy_from_slice(&cap.to_le_bytes());
            let t = parse_decrypted(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0,
                "fuel_percent {} must be in [0, 1]",
                t.fuel_percent
            );
        }

        /// RPM is finite when a valid RPM is placed at the expected offset.
        #[test]
        fn prop_rpm_finite(rpm in 0.0f32..=20000.0f32) {
            let mut buf = buf_with_magic();
            buf[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
            let t = parse_decrypted(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(t.rpm.is_finite(), "rpm must be finite");
        }

        /// salsa20_block is deterministic: same inputs always give same output.
        #[test]
        fn prop_salsa20_block_is_deterministic(counter in any::<u64>()) {
            let nonce = [0u8; 8];
            let block_a = salsa20_block(SALSA_KEY, &nonce, counter);
            let block_b = salsa20_block(SALSA_KEY, &nonce, counter);
            prop_assert_eq!(block_a, block_b, "salsa20_block must be deterministic");
        }

        // ---------------------------------------------------------------
        // Extended packet (316 / 344 byte) property tests
        // ---------------------------------------------------------------

        /// Arbitrary 316-byte packets must never panic in decrypt_and_parse.
        #[test]
        fn prop_arbitrary_type2_packet_no_panic(
            data in proptest::collection::vec(any::<u8>(), PACKET_SIZE_TYPE2..=PACKET_SIZE_TYPE2)
        ) {
            let _ = decrypt_and_parse(&data);
        }

        /// Arbitrary 344-byte packets must never panic in decrypt_and_parse.
        #[test]
        fn prop_arbitrary_type3_packet_no_panic(
            data in proptest::collection::vec(any::<u8>(), PACKET_SIZE_TYPE3..=PACKET_SIZE_TYPE3)
        ) {
            let _ = decrypt_and_parse(&data);
        }

        /// detect_packet_type returns the correct variant for all three sizes.
        #[test]
        fn prop_detect_packet_type_unknown_sizes(len in 0usize..2048) {
            let result = detect_packet_type(len);
            match len {
                PACKET_SIZE => prop_assert_eq!(result, Some(Gt7PacketType::Type1)),
                PACKET_SIZE_TYPE2 => prop_assert_eq!(result, Some(Gt7PacketType::Type2)),
                PACKET_SIZE_TYPE3 => prop_assert_eq!(result, Some(Gt7PacketType::Type3)),
                _ => prop_assert_eq!(result, None),
            }
        }

        /// 296-byte packets still parse correctly (backward compatibility).
        #[test]
        fn prop_type1_backward_compat(
            rpm in 0.0f32..=20000.0f32,
            throttle_byte in 0u8..=255u8,
            brake_byte in 0u8..=255u8,
            gear_byte in 0u8..=255u8,
        ) {
            let mut buf = buf_with_magic();
            buf[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
            buf[OFF_THROTTLE] = throttle_byte;
            buf[OFF_BRAKE] = brake_byte;
            buf[OFF_GEAR_BYTE] = gear_byte;

            let t = parse_decrypted(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(t.rpm.is_finite(), "rpm must be finite");
            prop_assert!(t.throttle >= 0.0 && t.throttle <= 1.0);
            prop_assert!(t.brake >= 0.0 && t.brake <= 1.0);
            prop_assert!(t.gear >= 0 && t.gear <= 8);
            prop_assert!(t.extended.is_empty(), "Type1 must have no extended data");
        }

        /// Type2 extended fields: wheel rotation is stored as steering_angle.
        #[test]
        fn prop_type2_wheel_rotation(rotation in proptest::num::f32::ANY) {
            let mut buf = vec![0u8; PACKET_SIZE_TYPE2];
            buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
            buf[OFF_WHEEL_ROTATION..OFF_WHEEL_ROTATION + 4]
                .copy_from_slice(&rotation.to_le_bytes());

            let t = parse_decrypted_ext(&buf)
                .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            // Non-finite floats are replaced with 0.0 by read_f32_le.
            if rotation.is_finite() {
                prop_assert!(
                    (t.steering_angle - rotation).abs() < f32::EPSILON,
                    "steering_angle {} should equal rotation {}",
                    t.steering_angle, rotation
                );
            } else {
                prop_assert_eq!(t.steering_angle, 0.0,
                    "non-finite rotation should yield 0.0");
            }
        }

        /// Type2 motion fields: sway/heave/surge map to lateral/vertical/longitudinal g.
        #[test]
        fn prop_type2_motion_fields(
            sway in proptest::num::f32::ANY,
            heave in proptest::num::f32::ANY,
            surge in proptest::num::f32::ANY,
        ) {
            let mut buf = vec![0u8; PACKET_SIZE_TYPE2];
            buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
            buf[OFF_SWAY..OFF_SWAY + 4].copy_from_slice(&sway.to_le_bytes());
            buf[OFF_HEAVE..OFF_HEAVE + 4].copy_from_slice(&heave.to_le_bytes());
            buf[OFF_SURGE..OFF_SURGE + 4].copy_from_slice(&surge.to_le_bytes());

            let t = parse_decrypted_ext(&buf)
                .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;

            let expected_sway = if sway.is_finite() { sway } else { 0.0 };
            let expected_heave = if heave.is_finite() { heave } else { 0.0 };
            let expected_surge = if surge.is_finite() { surge } else { 0.0 };

            prop_assert!(
                (t.lateral_g - expected_sway).abs() < f32::EPSILON,
                "lateral_g {} should equal sway {}", t.lateral_g, expected_sway
            );
            prop_assert!(
                (t.vertical_g - expected_heave).abs() < f32::EPSILON,
                "vertical_g {} should equal heave {}", t.vertical_g, expected_heave
            );
            prop_assert!(
                (t.longitudinal_g - expected_surge).abs() < f32::EPSILON,
                "longitudinal_g {} should equal surge {}", t.longitudinal_g, expected_surge
            );

            // Extended data must also contain the raw motion values.
            prop_assert_eq!(
                t.get_extended("gt7_sway"),
                Some(&TelemetryValue::Float(expected_sway))
            );
            prop_assert_eq!(
                t.get_extended("gt7_heave"),
                Some(&TelemetryValue::Float(expected_heave))
            );
            prop_assert_eq!(
                t.get_extended("gt7_surge"),
                Some(&TelemetryValue::Float(expected_surge))
            );
        }

        /// Type3 car_type byte is stored as an integer in extended data.
        #[test]
        fn prop_type3_car_type_byte(car_type in 0u8..=255u8) {
            let mut buf = vec![0u8; PACKET_SIZE_TYPE3];
            buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
            buf[OFF_CAR_TYPE_BYTE3] = car_type;

            let t = parse_decrypted_ext(&buf)
                .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert_eq!(
                t.get_extended("gt7_car_type"),
                Some(&TelemetryValue::Integer(car_type as i32))
            );
        }

        /// Type3 energy recovery is stored as a float in extended data.
        #[test]
        fn prop_type3_energy_recovery(energy in proptest::num::f32::ANY) {
            let mut buf = vec![0u8; PACKET_SIZE_TYPE3];
            buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
            buf[OFF_ENERGY_RECOVERY..OFF_ENERGY_RECOVERY + 4]
                .copy_from_slice(&energy.to_le_bytes());

            let t = parse_decrypted_ext(&buf)
                .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            let expected = if energy.is_finite() { energy } else { 0.0 };
            prop_assert_eq!(
                t.get_extended("gt7_energy_recovery"),
                Some(&TelemetryValue::Float(expected))
            );
        }

        /// Type3 packets also parse all Type2 extended fields.
        #[test]
        fn prop_type3_includes_type2_fields(
            rotation in -10.0f32..=10.0f32,
            sway in -5.0f32..=5.0f32,
        ) {
            let mut buf = vec![0u8; PACKET_SIZE_TYPE3];
            buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
            buf[OFF_WHEEL_ROTATION..OFF_WHEEL_ROTATION + 4]
                .copy_from_slice(&rotation.to_le_bytes());
            buf[OFF_SWAY..OFF_SWAY + 4].copy_from_slice(&sway.to_le_bytes());

            let t = parse_decrypted_ext(&buf)
                .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                (t.steering_angle - rotation).abs() < f32::EPSILON,
                "Type3 should parse Type2 wheel rotation"
            );
            prop_assert_eq!(
                t.get_extended("gt7_sway"),
                Some(&TelemetryValue::Float(sway)),
                "Type3 should parse Type2 sway"
            );
            // Type3-only fields must also be present.
            prop_assert!(
                t.get_extended("gt7_car_type").is_some(),
                "Type3 must have car_type extended field"
            );
            prop_assert!(
                t.get_extended("gt7_energy_recovery").is_some(),
                "Type3 must have energy_recovery extended field"
            );
        }

        /// Standard fields are parsed identically regardless of packet size.
        #[test]
        fn prop_standard_fields_consistent_across_sizes(
            rpm in 0.0f32..=20000.0f32,
            throttle_byte in 0u8..=255u8,
        ) {
            // Build buffers of each size with identical base fields.
            let mut buf1 = vec![0u8; PACKET_SIZE];
            let mut buf2 = vec![0u8; PACKET_SIZE_TYPE2];
            let mut buf3 = vec![0u8; PACKET_SIZE_TYPE3];
            for b in [&mut buf1, &mut buf2, &mut buf3] {
                b[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
                b[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
                b[OFF_THROTTLE] = throttle_byte;
            }

            let t1 = parse_decrypted_ext(&buf1)
                .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            let t2 = parse_decrypted_ext(&buf2)
                .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            let t3 = parse_decrypted_ext(&buf3)
                .map_err(|e| TestCaseError::fail(format!("{e:?}")))?;

            prop_assert!(
                (t1.rpm - t2.rpm).abs() < f32::EPSILON
                    && (t2.rpm - t3.rpm).abs() < f32::EPSILON,
                "RPM must be identical across all packet sizes"
            );
            prop_assert!(
                (t1.throttle - t2.throttle).abs() < f32::EPSILON
                    && (t2.throttle - t3.throttle).abs() < f32::EPSILON,
                "Throttle must be identical across all packet sizes"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Build a minimal 296-byte decrypted buffer with the GT7 magic set.
    /// Callers may fill in additional fields before calling `parse_decrypted`.
    fn make_decrypted_buf() -> [u8; PACKET_SIZE] {
        let mut buf = [0u8; PACKET_SIZE];
        buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        buf
    }

    // -----------------------------------------------------------------------
    // parse_decrypted tests (operates on pre-decrypted buffers)
    // -----------------------------------------------------------------------

    #[test]
    fn test_short_packet_returns_err() -> TestResult {
        let short = vec![0u8; 100];
        let result = decrypt_and_parse(&short);
        assert!(result.is_err(), "expected error for short packet");
        Ok(())
    }

    #[test]
    fn test_wrong_magic_returns_err() -> TestResult {
        // A zero-filled packet will have the wrong magic after "decryption"
        // (the Salsa20 keystream XORed with zeros gives the keystream itself,
        // which won't match MAGIC).  We test the post-decrypt magic check by
        // constructing a raw buffer that, after XOR with keystream block 0 at
        // offset 0, yields 0x00000000.
        let buf = [0u8; PACKET_SIZE];
        // After salsa20_xor the magic field will be whatever block 0 produces —
        // almost certainly not MAGIC.  Either way, the function must return Err.
        let result = decrypt_and_parse(&buf);
        // The only valid outcome here is an error (wrong magic).
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_rpm_extraction() -> TestResult {
        let mut buf = make_decrypted_buf();
        let expected_rpm = 6500.0f32;
        buf[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&expected_rpm.to_le_bytes());

        let telemetry = parse_decrypted(&buf)?;
        assert!(
            (telemetry.rpm - expected_rpm).abs() < 0.01,
            "RPM mismatch: expected {expected_rpm}, got {}",
            telemetry.rpm
        );
        Ok(())
    }

    #[test]
    fn test_speed_extraction() -> TestResult {
        let mut buf = make_decrypted_buf();
        let expected_ms = 33.33f32; // ~120 km/h
        buf[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&expected_ms.to_le_bytes());

        let telemetry = parse_decrypted(&buf)?;
        assert!(
            (telemetry.speed_ms - expected_ms).abs() < 0.001,
            "speed_ms mismatch: expected {expected_ms}, got {}",
            telemetry.speed_ms
        );
        Ok(())
    }

    #[test]
    fn test_gear_extraction_low_nibble() -> TestResult {
        let mut buf = make_decrypted_buf();
        // Byte 160: high nibble = suggested gear (4), low nibble = current gear (3)
        buf[OFF_GEAR_BYTE] = (4 << 4) | 3;

        let telemetry = parse_decrypted(&buf)?;
        assert_eq!(telemetry.gear, 3, "gear should be low nibble value 3");
        Ok(())
    }

    #[test]
    fn test_neutral_gear() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_GEAR_BYTE] = 0x00; // current gear = 0 = neutral

        let telemetry = parse_decrypted(&buf)?;
        assert_eq!(telemetry.gear, 0);
        Ok(())
    }

    #[test]
    fn test_throttle_brake_normalisation() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_THROTTLE] = 128; // ~50 %
        buf[OFF_BRAKE] = 255; // 100 %

        let telemetry = parse_decrypted(&buf)?;
        assert!(
            (telemetry.throttle - 128.0 / 255.0).abs() < 0.001,
            "throttle normalisation failed"
        );
        assert!(
            (telemetry.brake - 1.0).abs() < 0.001,
            "brake normalisation failed"
        );
        Ok(())
    }

    #[test]
    fn test_fuel_percentage() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_FUEL_LEVEL..OFF_FUEL_LEVEL + 4].copy_from_slice(&25.0f32.to_le_bytes());
        buf[OFF_FUEL_CAPACITY..OFF_FUEL_CAPACITY + 4].copy_from_slice(&50.0f32.to_le_bytes());

        let telemetry = parse_decrypted(&buf)?;
        assert!(
            (telemetry.fuel_percent - 0.5).abs() < 0.001,
            "fuel_percent should be 0.5"
        );
        Ok(())
    }

    #[test]
    fn test_lap_count() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_LAP_COUNT..OFF_LAP_COUNT + 2].copy_from_slice(&5u16.to_le_bytes());

        let telemetry = parse_decrypted(&buf)?;
        assert_eq!(telemetry.lap, 5);
        Ok(())
    }

    #[test]
    fn test_best_lap_time_ms_conversion() -> TestResult {
        let mut buf = make_decrypted_buf();
        // 1 min 23.456 s = 83456 ms
        buf[OFF_BEST_LAP_MS..OFF_BEST_LAP_MS + 4].copy_from_slice(&83_456i32.to_le_bytes());

        let telemetry = parse_decrypted(&buf)?;
        assert!(
            (telemetry.best_lap_time_s - 83.456).abs() < 0.001,
            "best_lap_time_s mismatch"
        );
        Ok(())
    }

    #[test]
    fn test_no_best_lap_when_minus_one() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_BEST_LAP_MS..OFF_BEST_LAP_MS + 4].copy_from_slice(&(-1i32).to_le_bytes());

        let telemetry = parse_decrypted(&buf)?;
        assert_eq!(
            telemetry.best_lap_time_s, 0.0,
            "best_lap_time_s should be 0 when raw value is -1"
        );
        Ok(())
    }

    #[test]
    fn test_tcs_flag_mapped() -> TestResult {
        let mut buf = make_decrypted_buf();
        let flags: u16 = FLAG_TCS_ACTIVE;
        buf[OFF_FLAGS..OFF_FLAGS + 2].copy_from_slice(&flags.to_le_bytes());

        let telemetry = parse_decrypted(&buf)?;
        assert!(telemetry.flags.traction_control, "TCS flag should be set");
        Ok(())
    }

    #[test]
    fn test_asm_flag_mapped() -> TestResult {
        let mut buf = make_decrypted_buf();
        let flags: u16 = FLAG_ASM_ACTIVE;
        buf[OFF_FLAGS..OFF_FLAGS + 2].copy_from_slice(&flags.to_le_bytes());

        let telemetry = parse_decrypted(&buf)?;
        assert!(
            telemetry.flags.abs_active,
            "ASM flag should map to abs_active"
        );
        Ok(())
    }

    #[test]
    fn test_car_code_to_car_id() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_CAR_CODE..OFF_CAR_CODE + 4].copy_from_slice(&42i32.to_le_bytes());

        let telemetry = parse_decrypted(&buf)?;
        assert_eq!(telemetry.car_id.as_deref(), Some("gt7_42"));
        Ok(())
    }

    #[test]
    fn test_zero_car_code_gives_no_car_id() -> TestResult {
        let buf = make_decrypted_buf(); // car_code is 0 by default

        let telemetry = parse_decrypted(&buf)?;
        assert!(telemetry.car_id.is_none());
        Ok(())
    }

    #[test]
    fn test_tire_temps_clamped() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_TIRE_TEMP_FL..OFF_TIRE_TEMP_FL + 4].copy_from_slice(&90.0f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_FR..OFF_TIRE_TEMP_FR + 4].copy_from_slice(&95.5f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_RL..OFF_TIRE_TEMP_RL + 4].copy_from_slice(&88.0f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_RR..OFF_TIRE_TEMP_RR + 4].copy_from_slice(&92.0f32.to_le_bytes());

        let telemetry = parse_decrypted(&buf)?;
        assert_eq!(telemetry.tire_temps_c[0], 90);
        assert_eq!(telemetry.tire_temps_c[1], 95);
        assert_eq!(telemetry.tire_temps_c[2], 88);
        assert_eq!(telemetry.tire_temps_c[3], 92);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Salsa20 smoke tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_salsa20_block_is_deterministic() {
        let nonce = [0u8; 8];
        let block_a = salsa20_block(SALSA_KEY, &nonce, 0);
        let block_b = salsa20_block(SALSA_KEY, &nonce, 0);
        assert_eq!(block_a, block_b, "Salsa20 block must be deterministic");
    }

    #[test]
    fn test_salsa20_different_counters_differ() {
        let nonce = [0u8; 8];
        let block_0 = salsa20_block(SALSA_KEY, &nonce, 0);
        let block_1 = salsa20_block(SALSA_KEY, &nonce, 1);
        assert_ne!(
            block_0, block_1,
            "Different counters must produce different blocks"
        );
    }

    #[test]
    fn test_salsa20_xor_is_deterministic() {
        // Same input must always produce the same output.
        let mut buf_a = [0u8; PACKET_SIZE];
        for (i, b) in buf_a.iter_mut().enumerate() {
            *b = (i & 0xFF) as u8;
        }
        let mut buf_b = buf_a;
        salsa20_xor(&mut buf_a);
        salsa20_xor(&mut buf_b);
        assert_eq!(buf_a, buf_b, "Salsa20 XOR must be deterministic");
    }

    #[test]
    fn test_salsa20_xor_changes_data() {
        // XOR with the Salsa20 keystream must actually change the buffer.
        let mut buf = [0u8; PACKET_SIZE];
        let original = buf;
        salsa20_xor(&mut buf);
        assert_ne!(
            buf, original,
            "Salsa20 XOR should produce non-trivial output"
        );
    }

    // -----------------------------------------------------------------------
    // Adapter trait smoke tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_adapter_game_id() {
        let adapter = GranTurismo7Adapter::new();
        assert_eq!(adapter.game_id(), "gran_turismo_7");
    }

    #[test]
    fn test_adapter_update_rate() {
        let adapter = GranTurismo7Adapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(17));
    }

    #[tokio::test]
    async fn test_adapter_is_game_running() -> TestResult {
        let adapter = GranTurismo7Adapter::new();
        let running = adapter.is_game_running().await?;
        assert!(
            !running,
            "GT7 is a console game; process detection returns false"
        );
        Ok(())
    }

    #[test]
    fn test_normalize_short_data_returns_err() {
        let adapter = GranTurismo7Adapter::new();
        let result = adapter.normalize(&[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_with_port_override() {
        let adapter = GranTurismo7Adapter::new().with_port(12345);
        assert_eq!(adapter.recv_port, 12345);
    }

    // -----------------------------------------------------------------------
    // Extended packet tests (PacketType2 / PacketType3)
    // -----------------------------------------------------------------------

    /// Build a 316-byte (PacketType2) decrypted buffer with the GT7 magic set.
    fn make_type2_buf() -> Vec<u8> {
        let mut buf = vec![0u8; PACKET_SIZE_TYPE2];
        buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        buf
    }

    /// Build a 344-byte (PacketType3) decrypted buffer with the GT7 magic set.
    fn make_type3_buf() -> Vec<u8> {
        let mut buf = vec![0u8; PACKET_SIZE_TYPE3];
        buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        buf
    }

    #[test]
    fn test_type2_wheel_rotation() -> TestResult {
        let mut buf = make_type2_buf();
        let rotation_rad: f32 = 1.5;
        buf[OFF_WHEEL_ROTATION..OFF_WHEEL_ROTATION + 4]
            .copy_from_slice(&rotation_rad.to_le_bytes());

        let t = parse_decrypted_ext(&buf)?;
        assert!(
            (t.steering_angle - rotation_rad).abs() < 0.001,
            "steering_angle should be wheel rotation in radians: got {}",
            t.steering_angle
        );
        Ok(())
    }

    #[test]
    fn test_type2_motion_fields() -> TestResult {
        let mut buf = make_type2_buf();
        let sway: f32 = 0.3;
        let heave: f32 = -0.1;
        let surge: f32 = 0.8;
        buf[OFF_SWAY..OFF_SWAY + 4].copy_from_slice(&sway.to_le_bytes());
        buf[OFF_HEAVE..OFF_HEAVE + 4].copy_from_slice(&heave.to_le_bytes());
        buf[OFF_SURGE..OFF_SURGE + 4].copy_from_slice(&surge.to_le_bytes());

        let t = parse_decrypted_ext(&buf)?;
        assert!(
            (t.lateral_g - sway).abs() < 0.001,
            "lateral_g should map from sway"
        );
        assert!(
            (t.vertical_g - heave).abs() < 0.001,
            "vertical_g should map from heave"
        );
        assert!(
            (t.longitudinal_g - surge).abs() < 0.001,
            "longitudinal_g should map from surge"
        );
        // Also check extended data keys
        assert_eq!(
            t.get_extended("gt7_sway"),
            Some(&TelemetryValue::Float(sway))
        );
        assert_eq!(
            t.get_extended("gt7_heave"),
            Some(&TelemetryValue::Float(heave))
        );
        assert_eq!(
            t.get_extended("gt7_surge"),
            Some(&TelemetryValue::Float(surge))
        );
        Ok(())
    }

    #[test]
    fn test_type2_standard_fields_still_parsed() -> TestResult {
        let mut buf = make_type2_buf();
        let rpm: f32 = 7000.0;
        buf[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&rpm.to_le_bytes());

        let t = parse_decrypted_ext(&buf)?;
        assert!(
            (t.rpm - rpm).abs() < 0.01,
            "Standard RPM field should still work in Type2 packet"
        );
        Ok(())
    }

    #[test]
    fn test_type3_energy_recovery() -> TestResult {
        let mut buf = make_type3_buf();
        let energy: f32 = 42.5;
        buf[OFF_ENERGY_RECOVERY..OFF_ENERGY_RECOVERY + 4].copy_from_slice(&energy.to_le_bytes());

        let t = parse_decrypted_ext(&buf)?;
        assert_eq!(
            t.get_extended("gt7_energy_recovery"),
            Some(&TelemetryValue::Float(energy))
        );
        Ok(())
    }

    #[test]
    fn test_type3_car_type_electric() -> TestResult {
        let mut buf = make_type3_buf();
        buf[OFF_CAR_TYPE_BYTE3] = 4; // 4 = electric (per PDTools)

        let t = parse_decrypted_ext(&buf)?;
        assert_eq!(
            t.get_extended("gt7_car_type"),
            Some(&TelemetryValue::Integer(4))
        );
        Ok(())
    }

    #[test]
    fn test_type3_includes_type2_fields() -> TestResult {
        let mut buf = make_type3_buf();
        let rotation_rad: f32 = -0.5;
        let surge: f32 = 1.2;
        buf[OFF_WHEEL_ROTATION..OFF_WHEEL_ROTATION + 4]
            .copy_from_slice(&rotation_rad.to_le_bytes());
        buf[OFF_SURGE..OFF_SURGE + 4].copy_from_slice(&surge.to_le_bytes());

        let t = parse_decrypted_ext(&buf)?;
        assert!(
            (t.steering_angle - rotation_rad).abs() < 0.001,
            "Type3 should also parse Type2 wheel rotation"
        );
        assert!(
            (t.longitudinal_g - surge).abs() < 0.001,
            "Type3 should also parse Type2 surge"
        );
        Ok(())
    }

    #[test]
    fn test_standard_packet_has_no_extended_fields() -> TestResult {
        let buf = make_decrypted_buf(); // 296-byte standard packet
        let t = parse_decrypted(&buf)?;
        assert_eq!(
            t.steering_angle, 0.0,
            "Standard packet should have default steering_angle"
        );
        assert!(
            t.extended.is_empty(),
            "Standard packet should have no extended data"
        );
        Ok(())
    }

    #[test]
    fn test_detect_packet_type_values() {
        assert_eq!(detect_packet_type(296), Some(Gt7PacketType::Type1));
        assert_eq!(detect_packet_type(316), Some(Gt7PacketType::Type2));
        assert_eq!(detect_packet_type(344), Some(Gt7PacketType::Type3));
        assert_eq!(detect_packet_type(100), None);
        assert_eq!(detect_packet_type(300), None);
    }

    #[test]
    fn test_packet_type_constants() {
        assert_eq!(Gt7PacketType::Type1.heartbeat(), b"A");
        assert_eq!(Gt7PacketType::Type2.heartbeat(), b"B");
        assert_eq!(Gt7PacketType::Type3.heartbeat(), b"~");

        assert_eq!(Gt7PacketType::Type1.expected_size(), 296);
        assert_eq!(Gt7PacketType::Type2.expected_size(), 316);
        assert_eq!(Gt7PacketType::Type3.expected_size(), 344);

        assert_eq!(Gt7PacketType::Type1.xor_key(), 0xDEAD_BEAF);
        assert_eq!(Gt7PacketType::Type2.xor_key(), 0xDEAD_BEEF);
        assert_eq!(Gt7PacketType::Type3.xor_key(), 0x55FA_BB4F);
    }

    #[test]
    fn test_adapter_default_packet_type() {
        let adapter = GranTurismo7Adapter::new();
        assert_eq!(adapter.packet_type, Gt7PacketType::Type3);
    }

    #[test]
    fn test_adapter_with_packet_type_override() {
        let adapter = GranTurismo7Adapter::new().with_packet_type(Gt7PacketType::Type1);
        assert_eq!(adapter.packet_type, Gt7PacketType::Type1);
    }

    #[test]
    fn test_salsa20_decrypt_different_xor_keys_differ() {
        // Same data decrypted with different XOR keys should produce different results.
        let mut buf1 = vec![0u8; PACKET_SIZE_TYPE2];
        for (i, b) in buf1.iter_mut().enumerate() {
            *b = (i & 0xFF) as u8;
        }
        let mut buf2 = buf1.clone();
        salsa20_decrypt(&mut buf1, XOR_KEY_TYPE1);
        salsa20_decrypt(&mut buf2, XOR_KEY_TYPE2);
        assert_ne!(
            buf1, buf2,
            "Different XOR keys must produce different decrypted output"
        );
    }

    // -----------------------------------------------------------------------
    // Salsa20 round-trip and nonce derivation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_salsa20_round_trip() -> TestResult {
        // Salsa20 derives the nonce from buf[0x40..0x44]. After encryption the
        // nonce bytes change, so a naïve double-XOR does NOT round-trip.
        // Instead, verify that preserving the IV seed allows round-trip.
        let mut buf = [0u8; PACKET_SIZE];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (i & 0xFF) as u8;
        }
        let original = buf;
        let iv_bytes: [u8; 4] = [buf[0x40], buf[0x41], buf[0x42], buf[0x43]];
        salsa20_xor(&mut buf);
        assert_ne!(buf, original, "encryption should change the buffer");
        // Restore the IV seed so the second decrypt uses the same nonce.
        buf[0x40..0x44].copy_from_slice(&iv_bytes);
        salsa20_xor(&mut buf);
        // All bytes except the IV offset should match (IV itself was restored).
        for i in 0..PACKET_SIZE {
            if (0x40..0x44).contains(&i) {
                continue; // IV bytes were manually restored
            }
            assert_eq!(
                buf[i], original[i],
                "mismatch at byte {i}: got {}, expected {}",
                buf[i], original[i]
            );
        }
        Ok(())
    }

    #[test]
    fn test_salsa20_nonce_derived_from_iv_offset() {
        // Changing bytes at the IV offset (0x40..0x44) should change the keystream.
        let mut buf_a = [0u8; PACKET_SIZE];
        buf_a[0x40] = 0x01;
        let mut buf_b = [0u8; PACKET_SIZE];
        buf_b[0x40] = 0x02;
        salsa20_xor(&mut buf_a);
        salsa20_xor(&mut buf_b);
        assert_ne!(
            buf_a, buf_b,
            "Different IV seeds must produce different keystreams"
        );
    }

    #[test]
    fn test_salsa20_decrypt_consistency_type2() -> TestResult {
        // Same input encrypted twice must produce the same ciphertext.
        let mut buf_a = vec![0u8; PACKET_SIZE_TYPE2];
        for (i, b) in buf_a.iter_mut().enumerate() {
            *b = ((i * 7) & 0xFF) as u8;
        }
        let mut buf_b = buf_a.clone();
        salsa20_decrypt(&mut buf_a, XOR_KEY_TYPE2);
        salsa20_decrypt(&mut buf_b, XOR_KEY_TYPE2);
        assert_eq!(buf_a, buf_b, "Type2 encryption must be deterministic");
        Ok(())
    }

    #[test]
    fn test_salsa20_decrypt_consistency_type3() -> TestResult {
        // Same input encrypted twice must produce the same ciphertext.
        let mut buf_a = vec![0u8; PACKET_SIZE_TYPE3];
        for (i, b) in buf_a.iter_mut().enumerate() {
            *b = ((i * 13) & 0xFF) as u8;
        }
        let mut buf_b = buf_a.clone();
        salsa20_decrypt(&mut buf_a, XOR_KEY_TYPE3);
        salsa20_decrypt(&mut buf_b, XOR_KEY_TYPE3);
        assert_eq!(buf_a, buf_b, "Type3 encryption must be deterministic");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Additional field extraction tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_water_temp_extraction() -> TestResult {
        let mut buf = make_decrypted_buf();
        let expected_temp: f32 = 85.5;
        buf[OFF_WATER_TEMP..OFF_WATER_TEMP + 4].copy_from_slice(&expected_temp.to_le_bytes());
        let t = parse_decrypted(&buf)?;
        assert!(
            (t.engine_temp_c - expected_temp).abs() < 0.01,
            "engine_temp_c should reflect water_temp: got {}",
            t.engine_temp_c
        );
        Ok(())
    }

    #[test]
    fn test_max_rpm_from_alert_rpm() -> TestResult {
        let mut buf = make_decrypted_buf();
        let alert_rpm: u16 = 8500;
        buf[OFF_MAX_ALERT_RPM..OFF_MAX_ALERT_RPM + 2].copy_from_slice(&alert_rpm.to_le_bytes());
        let t = parse_decrypted(&buf)?;
        assert!(
            (t.max_rpm - 8500.0).abs() < 0.01,
            "max_rpm should be alert RPM value: got {}",
            t.max_rpm
        );
        Ok(())
    }

    #[test]
    fn test_last_lap_time_conversion() -> TestResult {
        let mut buf = make_decrypted_buf();
        // 1 min 45.678 s = 105678 ms
        buf[OFF_LAST_LAP_MS..OFF_LAST_LAP_MS + 4].copy_from_slice(&105_678i32.to_le_bytes());
        let t = parse_decrypted(&buf)?;
        assert!(
            (t.last_lap_time_s - 105.678).abs() < 0.001,
            "last_lap_time_s mismatch: got {}",
            t.last_lap_time_s
        );
        Ok(())
    }

    #[test]
    fn test_last_lap_negative_gives_zero() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_LAST_LAP_MS..OFF_LAST_LAP_MS + 4].copy_from_slice(&(-1i32).to_le_bytes());
        let t = parse_decrypted(&buf)?;
        assert_eq!(
            t.last_lap_time_s, 0.0,
            "last_lap_time_s should be 0 for negative raw value"
        );
        Ok(())
    }

    #[test]
    fn test_fuel_zero_capacity_gives_zero_percent() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_FUEL_LEVEL..OFF_FUEL_LEVEL + 4].copy_from_slice(&50.0f32.to_le_bytes());
        buf[OFF_FUEL_CAPACITY..OFF_FUEL_CAPACITY + 4].copy_from_slice(&0.0f32.to_le_bytes());
        let t = parse_decrypted(&buf)?;
        assert_eq!(
            t.fuel_percent, 0.0,
            "fuel_percent must be 0 when capacity is 0"
        );
        Ok(())
    }

    #[test]
    fn test_fuel_exceeding_capacity_clamps_to_one() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_FUEL_LEVEL..OFF_FUEL_LEVEL + 4].copy_from_slice(&200.0f32.to_le_bytes());
        buf[OFF_FUEL_CAPACITY..OFF_FUEL_CAPACITY + 4].copy_from_slice(&100.0f32.to_le_bytes());
        let t = parse_decrypted(&buf)?;
        assert!(
            (t.fuel_percent - 1.0).abs() < f32::EPSILON,
            "fuel_percent must clamp to 1.0 when level > capacity"
        );
        Ok(())
    }

    #[test]
    fn test_negative_speed_clamped_to_zero() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&(-10.0f32).to_le_bytes());
        let t = parse_decrypted(&buf)?;
        assert_eq!(t.speed_ms, 0.0, "negative speed must be clamped to 0");
        Ok(())
    }

    #[test]
    fn test_nan_speed_gives_zero() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        let t = parse_decrypted(&buf)?;
        assert_eq!(
            t.speed_ms, 0.0,
            "NaN speed should produce 0 (read_f32_le returns 0 for non-finite)"
        );
        Ok(())
    }

    #[test]
    fn test_nan_rpm_gives_zero() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        let t = parse_decrypted(&buf)?;
        assert_eq!(t.rpm, 0.0, "NaN RPM should produce 0.0");
        Ok(())
    }

    #[test]
    fn test_infinity_rpm_gives_zero() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&f32::INFINITY.to_le_bytes());
        let t = parse_decrypted(&buf)?;
        assert_eq!(t.rpm, 0.0, "Infinity RPM should produce 0.0");
        Ok(())
    }

    #[test]
    fn test_neg_infinity_tire_temp_clamped() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_TIRE_TEMP_FL..OFF_TIRE_TEMP_FL + 4]
            .copy_from_slice(&f32::NEG_INFINITY.to_le_bytes());
        let t = parse_decrypted(&buf)?;
        // read_f32_le returns 0.0 for non-finite, then clamped to [0,255] as u8 = 0
        assert_eq!(
            t.tire_temps_c[0], 0,
            "non-finite tire temp should clamp to 0"
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Flag combination tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_rev_limit_flag() -> TestResult {
        let mut buf = make_decrypted_buf();
        let flags: u16 = FLAG_REV_LIMIT;
        buf[OFF_FLAGS..OFF_FLAGS + 2].copy_from_slice(&flags.to_le_bytes());
        let t = parse_decrypted(&buf)?;
        assert!(t.flags.engine_limiter, "REV_LIMIT flag should be set");
        assert!(!t.flags.traction_control, "TCS should not be set");
        assert!(!t.flags.abs_active, "ASM should not be set");
        Ok(())
    }

    #[test]
    fn test_paused_flag() -> TestResult {
        let mut buf = make_decrypted_buf();
        let flags: u16 = FLAG_PAUSED;
        buf[OFF_FLAGS..OFF_FLAGS + 2].copy_from_slice(&flags.to_le_bytes());
        let t = parse_decrypted(&buf)?;
        assert!(t.flags.session_paused, "PAUSED flag should be set");
        Ok(())
    }

    #[test]
    fn test_multiple_flags_combined() -> TestResult {
        let mut buf = make_decrypted_buf();
        let flags: u16 = FLAG_TCS_ACTIVE | FLAG_ASM_ACTIVE | FLAG_REV_LIMIT | FLAG_PAUSED;
        buf[OFF_FLAGS..OFF_FLAGS + 2].copy_from_slice(&flags.to_le_bytes());
        let t = parse_decrypted(&buf)?;
        assert!(t.flags.traction_control, "TCS should be set");
        assert!(t.flags.abs_active, "ASM/ABS should be set");
        assert!(t.flags.engine_limiter, "REV_LIMIT should be set");
        assert!(t.flags.session_paused, "PAUSED should be set");
        Ok(())
    }

    #[test]
    fn test_no_flags_set() -> TestResult {
        let buf = make_decrypted_buf(); // flags are 0 by default
        let t = parse_decrypted(&buf)?;
        assert!(!t.flags.traction_control);
        assert!(!t.flags.abs_active);
        assert!(!t.flags.engine_limiter);
        assert!(!t.flags.session_paused);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Gear edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_gear_max_valid() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_GEAR_BYTE] = 0x08; // 8th gear
        let t = parse_decrypted(&buf)?;
        assert_eq!(t.gear, 8, "8th gear should be valid");
        Ok(())
    }

    #[test]
    fn test_gear_invalid_nibble_maps_to_neutral() -> TestResult {
        let mut buf = make_decrypted_buf();
        // Low nibble 0x0F (15) is out of range [0..8], should map to 0
        buf[OFF_GEAR_BYTE] = 0x0F;
        let t = parse_decrypted(&buf)?;
        assert_eq!(t.gear, 0, "out-of-range gear nibble should map to neutral");
        Ok(())
    }

    #[test]
    fn test_gear_suggested_in_high_nibble_ignored() -> TestResult {
        let mut buf = make_decrypted_buf();
        // High nibble = suggested gear 7, low nibble = current gear 2
        buf[OFF_GEAR_BYTE] = (7 << 4) | 2;
        let t = parse_decrypted(&buf)?;
        assert_eq!(t.gear, 2, "only low nibble should be used for current gear");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Throttle/brake edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_throttle_zero_and_max() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_THROTTLE] = 0;
        buf[OFF_BRAKE] = 0;
        let t = parse_decrypted(&buf)?;
        assert_eq!(t.throttle, 0.0, "zero throttle byte = 0.0");
        assert_eq!(t.brake, 0.0, "zero brake byte = 0.0");

        buf[OFF_THROTTLE] = 255;
        buf[OFF_BRAKE] = 255;
        let t2 = parse_decrypted(&buf)?;
        assert!(
            (t2.throttle - 1.0).abs() < f32::EPSILON,
            "255 throttle = 1.0"
        );
        assert!((t2.brake - 1.0).abs() < f32::EPSILON, "255 brake = 1.0");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Error handling: truncated / odd-sized packets
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_packet_returns_err() {
        let result = decrypt_and_parse(&[]);
        assert!(result.is_err(), "empty packet must fail");
    }

    #[test]
    fn test_packet_one_byte_short_of_type1() {
        let data = vec![0u8; PACKET_SIZE - 1];
        let result = decrypt_and_parse(&data);
        assert!(result.is_err(), "295 bytes must fail");
    }

    #[test]
    fn test_odd_size_between_type1_and_type2() -> TestResult {
        // 300 bytes: larger than Type1 (296), smaller than Type2 (316).
        // detect_packet_type returns None; fallback to Type1 parsing.
        let data = vec![0u8; 300];
        // This will fail on magic check (encrypted data), which is expected.
        let result = decrypt_and_parse(&data);
        // Should not panic; either Ok or Err is fine.
        let _ = result;
        Ok(())
    }

    #[test]
    fn test_parse_decrypted_ext_too_short() {
        let short = vec![0u8; 100];
        let result = parse_decrypted_ext(&short);
        assert!(result.is_err(), "buffer shorter than 296 must fail");
    }

    // -----------------------------------------------------------------------
    // Realistic multi-field packet test
    // -----------------------------------------------------------------------

    #[test]
    fn test_realistic_full_packet() -> TestResult {
        let mut buf = make_decrypted_buf();
        // Set realistic racing values
        buf[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&7200.0f32.to_le_bytes());
        buf[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&55.5f32.to_le_bytes()); // ~200 km/h
        buf[OFF_THROTTLE] = 204; // ~80%
        buf[OFF_BRAKE] = 0;
        buf[OFF_GEAR_BYTE] = (5 << 4) | 4; // gear 4, suggested 5
        buf[OFF_FUEL_LEVEL..OFF_FUEL_LEVEL + 4].copy_from_slice(&30.0f32.to_le_bytes());
        buf[OFF_FUEL_CAPACITY..OFF_FUEL_CAPACITY + 4].copy_from_slice(&60.0f32.to_le_bytes());
        buf[OFF_WATER_TEMP..OFF_WATER_TEMP + 4].copy_from_slice(&92.0f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_FL..OFF_TIRE_TEMP_FL + 4].copy_from_slice(&85.0f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_FR..OFF_TIRE_TEMP_FR + 4].copy_from_slice(&87.0f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_RL..OFF_TIRE_TEMP_RL + 4].copy_from_slice(&82.0f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_RR..OFF_TIRE_TEMP_RR + 4].copy_from_slice(&84.0f32.to_le_bytes());
        buf[OFF_LAP_COUNT..OFF_LAP_COUNT + 2].copy_from_slice(&3u16.to_le_bytes());
        buf[OFF_BEST_LAP_MS..OFF_BEST_LAP_MS + 4].copy_from_slice(&92_345i32.to_le_bytes());
        buf[OFF_LAST_LAP_MS..OFF_LAST_LAP_MS + 4].copy_from_slice(&93_100i32.to_le_bytes());
        buf[OFF_MAX_ALERT_RPM..OFF_MAX_ALERT_RPM + 2].copy_from_slice(&8000u16.to_le_bytes());
        let flags: u16 = FLAG_TCS_ACTIVE;
        buf[OFF_FLAGS..OFF_FLAGS + 2].copy_from_slice(&flags.to_le_bytes());
        buf[OFF_CAR_CODE..OFF_CAR_CODE + 4].copy_from_slice(&1234i32.to_le_bytes());

        let t = parse_decrypted(&buf)?;
        assert!((t.rpm - 7200.0).abs() < 0.01);
        assert!((t.speed_ms - 55.5).abs() < 0.01);
        assert!((t.throttle - 204.0 / 255.0).abs() < 0.001);
        assert_eq!(t.brake, 0.0);
        assert_eq!(t.gear, 4);
        assert!((t.fuel_percent - 0.5).abs() < 0.001);
        assert!((t.engine_temp_c - 92.0).abs() < 0.01);
        assert_eq!(t.tire_temps_c, [85, 87, 82, 84]);
        assert_eq!(t.lap, 3);
        assert!((t.best_lap_time_s - 92.345).abs() < 0.001);
        assert!((t.last_lap_time_s - 93.1).abs() < 0.001);
        assert!((t.max_rpm - 8000.0).abs() < 0.01);
        assert!(t.flags.traction_control);
        assert_eq!(t.car_id.as_deref(), Some("gt7_1234"));
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Realistic extended packet tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_realistic_type3_full_packet() -> TestResult {
        let mut buf = make_type3_buf();
        // Base fields
        buf[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&6000.0f32.to_le_bytes());
        buf[OFF_THROTTLE] = 255;
        buf[OFF_GEAR_BYTE] = 3;
        // Type2 extended fields
        buf[OFF_WHEEL_ROTATION..OFF_WHEEL_ROTATION + 4].copy_from_slice(&(-0.25f32).to_le_bytes());
        buf[OFF_SWAY..OFF_SWAY + 4].copy_from_slice(&0.15f32.to_le_bytes());
        buf[OFF_HEAVE..OFF_HEAVE + 4].copy_from_slice(&(-0.05f32).to_le_bytes());
        buf[OFF_SURGE..OFF_SURGE + 4].copy_from_slice(&0.9f32.to_le_bytes());
        // Type3 extended fields
        buf[OFF_CAR_TYPE_BYTE3] = 4; // electric
        buf[OFF_ENERGY_RECOVERY..OFF_ENERGY_RECOVERY + 4].copy_from_slice(&75.3f32.to_le_bytes());

        let t = parse_decrypted_ext(&buf)?;
        assert!((t.rpm - 6000.0).abs() < 0.01);
        assert!((t.throttle - 1.0).abs() < f32::EPSILON);
        assert_eq!(t.gear, 3);
        assert!((t.steering_angle - (-0.25)).abs() < 0.001);
        assert!((t.lateral_g - 0.15).abs() < 0.001);
        assert!((t.vertical_g - (-0.05)).abs() < 0.001);
        assert!((t.longitudinal_g - 0.9).abs() < 0.001);
        assert_eq!(
            t.get_extended("gt7_car_type"),
            Some(&TelemetryValue::Integer(4))
        );
        assert_eq!(
            t.get_extended("gt7_energy_recovery"),
            Some(&TelemetryValue::Float(75.3))
        );
        Ok(())
    }

    #[test]
    fn test_type2_non_finite_motion_gives_zero() -> TestResult {
        let mut buf = make_type2_buf();
        buf[OFF_SWAY..OFF_SWAY + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        buf[OFF_HEAVE..OFF_HEAVE + 4].copy_from_slice(&f32::INFINITY.to_le_bytes());
        buf[OFF_SURGE..OFF_SURGE + 4].copy_from_slice(&f32::NEG_INFINITY.to_le_bytes());
        let t = parse_decrypted_ext(&buf)?;
        assert_eq!(t.lateral_g, 0.0, "NaN sway should give 0.0");
        assert_eq!(t.vertical_g, 0.0, "Infinity heave should give 0.0");
        assert_eq!(t.longitudinal_g, 0.0, "NEG_INFINITY surge should give 0.0");
        Ok(())
    }

    #[test]
    fn test_type3_zero_energy_recovery() -> TestResult {
        let mut buf = make_type3_buf();
        buf[OFF_ENERGY_RECOVERY..OFF_ENERGY_RECOVERY + 4].copy_from_slice(&0.0f32.to_le_bytes());
        let t = parse_decrypted_ext(&buf)?;
        assert_eq!(
            t.get_extended("gt7_energy_recovery"),
            Some(&TelemetryValue::Float(0.0))
        );
        Ok(())
    }

    #[test]
    fn test_type3_all_car_type_values() -> TestResult {
        for car_type in [0u8, 1, 2, 3, 4, 255] {
            let mut buf = make_type3_buf();
            buf[OFF_CAR_TYPE_BYTE3] = car_type;
            let t = parse_decrypted_ext(&buf)?;
            assert_eq!(
                t.get_extended("gt7_car_type"),
                Some(&TelemetryValue::Integer(car_type as i32)),
                "car_type byte {car_type} should map correctly"
            );
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Port constants
    // -----------------------------------------------------------------------

    #[test]
    fn test_gt7_port_constants() {
        assert_eq!(GT7_RECV_PORT, 33740, "GT7 recv port must be 33740");
        assert_eq!(GT7_SEND_PORT, 33739, "GT7 send port must be 33739");
    }

    #[test]
    fn test_magic_constant() {
        assert_eq!(MAGIC, 0x4737_5330, "MAGIC must be 0x47375330 (\"0S7G\" LE)");
    }

    #[test]
    fn test_packet_size_constants() {
        assert_eq!(PACKET_SIZE, 296);
        assert_eq!(PACKET_SIZE_TYPE2, 316);
        assert_eq!(PACKET_SIZE_TYPE3, 344);
        assert_eq!(MAX_PACKET_SIZE, PACKET_SIZE_TYPE3);
    }

    #[test]
    fn test_adapter_default_impl() {
        let a = GranTurismo7Adapter::default();
        let b = GranTurismo7Adapter::new();
        assert_eq!(a.recv_port, b.recv_port);
        assert_eq!(a.update_rate, b.update_rate);
        assert_eq!(a.packet_type, b.packet_type);
    }
}
