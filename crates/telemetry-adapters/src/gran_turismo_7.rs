//! Gran Turismo 7 UDP telemetry adapter.
//!
//! Receives Salsa20-encrypted UDP packets on port 33740.
//! Protocol reverse-engineered by the community (Nenkai/gt7dashboard et al.).
//!
//! # Protocol overview
//! GT7 broadcasts 296-byte encrypted UDP packets from the PlayStation to any
//! host that has recently sent a heartbeat. Decryption uses Salsa20 with:
//! - Key: first 32 bytes of `"Simulator Interface Packet GT7 ver 0.0"`
//! - Nonce: bytes `[0x40..0x48]` of the **raw** (encrypted) packet
//!
//! A single-byte heartbeat (`b"A"`) must be sent to the PlayStation on port
//! 33739 every ~100 ms to keep the stream active.

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFlags, TelemetryFrame, TelemetryReceiver,
    telemetry_now_ns,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// UDP port on which GT7 broadcasts telemetry.
pub const GT7_RECV_PORT: u16 = 33740;
/// UDP port to which heartbeat packets must be sent to keep the stream alive.
pub const GT7_SEND_PORT: u16 = 33739;
/// Expected size of every GT7 telemetry packet (bytes).
pub const PACKET_SIZE: usize = 296;
/// Magic number present in bytes 0–3 of a correctly decrypted packet.
pub const MAGIC: u32 = 0x4737_5330; // "0S7G" little-endian

/// Salsa20 decryption key: first 32 bytes of the GT7 protocol string.
const SALSA_KEY: &[u8; 32] = b"Simulator Interface Packet GT7 v";

// ---------------------------------------------------------------------------
// Packet field offsets (all values are little-endian)
// ---------------------------------------------------------------------------
pub const OFF_MAGIC: usize = 0;
const OFF_ENGINE_RPM: usize = 60;
const OFF_FUEL_LEVEL: usize = 68;
const OFF_FUEL_CAPACITY: usize = 72;
const OFF_SPEED_MS: usize = 76;
const OFF_WATER_TEMP: usize = 88;
const OFF_TIRE_TEMP_FL: usize = 96;
const OFF_TIRE_TEMP_FR: usize = 100;
const OFF_TIRE_TEMP_RL: usize = 104;
const OFF_TIRE_TEMP_RR: usize = 108;
const OFF_LAP_COUNT: usize = 114;
const OFF_BEST_LAP_MS: usize = 118;
const OFF_LAST_LAP_MS: usize = 122;
const OFF_THROTTLE: usize = 141;
const OFF_BRAKE: usize = 142;
const OFF_RPM_ALERT_END: usize = 148;
const OFF_FLAGS: usize = 156;
const OFF_GEAR_BYTE: usize = 160;
const OFF_CAR_CODE: usize = 280;

// GT7 flags bitmask (offset 156, u32 little-endian)
const FLAG_PAUSED: u32 = 1 << 1;
const FLAG_REV_LIMIT: u32 = 1 << 5;
const FLAG_ASM_ACTIVE: u32 = 1 << 9;
const FLAG_TCS_ACTIVE: u32 = 1 << 10;

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Gran Turismo 7 telemetry adapter.
///
/// Listens for UDP packets on [`GT7_RECV_PORT`] and sends heartbeats back to
/// the source host on [`GT7_SEND_PORT`] to keep the stream alive.
pub struct GranTurismo7Adapter {
    recv_port: u16,
    update_rate: Duration,
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
        }
    }

    /// Override the receive port (useful for testing with ephemeral ports).
    pub fn with_port(mut self, port: u16) -> Self {
        self.recv_port = port;
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

            let heartbeat_payload = b"A";
            let mut buf = [0u8; PACKET_SIZE + 16];
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
pub(crate) fn decrypt_and_parse(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < PACKET_SIZE {
        return Err(anyhow!(
            "GT7 packet too short: expected {PACKET_SIZE}, got {}",
            data.len()
        ));
    }

    let mut buf = [0u8; PACKET_SIZE];
    buf.copy_from_slice(&data[..PACKET_SIZE]);
    salsa20_xor(&mut buf);

    let magic = read_u32_le(&buf, OFF_MAGIC);
    if magic != MAGIC {
        return Err(anyhow!(
            "GT7 magic mismatch: expected 0x{MAGIC:08X}, got 0x{magic:08X}"
        ));
    }

    parse_decrypted(&buf)
}

/// XOR the buffer in-place with the Salsa20 keystream.
///
/// The 8-byte nonce is read from bytes `[0x40..0x48]` of the **raw**
/// (pre-decryption) packet, as specified by the GT7 protocol.
pub(crate) fn salsa20_xor(buf: &mut [u8; PACKET_SIZE]) {
    let nonce: [u8; 8] = [
        buf[0x40], buf[0x41], buf[0x42], buf[0x43], buf[0x44], buf[0x45], buf[0x46], buf[0x47],
    ];

    let blocks_needed = PACKET_SIZE.div_ceil(64); // 5 full 64-byte blocks
    for block_idx in 0..blocks_needed {
        let ks = salsa20_block(SALSA_KEY, &nonce, block_idx as u64);
        let start = block_idx * 64;
        let end = (start + 64).min(PACKET_SIZE);
        for (b, k) in buf[start..end].iter_mut().zip(ks.iter()) {
            *b ^= k;
        }
    }
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
pub fn parse_decrypted(buf: &[u8; PACKET_SIZE]) -> Result<NormalizedTelemetry> {
    let rpm = read_f32_le(buf, OFF_ENGINE_RPM);
    let max_rpm = read_f32_le(buf, OFF_RPM_ALERT_END).max(0.0);
    let speed_ms = read_f32_le(buf, OFF_SPEED_MS).max(0.0);
    let fuel_level = read_f32_le(buf, OFF_FUEL_LEVEL);
    let fuel_capacity = read_f32_le(buf, OFF_FUEL_CAPACITY);
    let water_temp = read_f32_le(buf, OFF_WATER_TEMP);
    let flags_raw = read_u32_le(buf, OFF_FLAGS);
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

    Ok(builder.build())
}

// ---------------------------------------------------------------------------
// Low-level read helpers
// ---------------------------------------------------------------------------

fn read_f32_le(data: &[u8], offset: usize) -> f32 {
    let bytes: [u8; 4] = data[offset..offset + 4].try_into().unwrap_or([0; 4]);
    f32::from_le_bytes(bytes)
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    let bytes: [u8; 4] = data[offset..offset + 4].try_into().unwrap_or([0; 4]);
    u32::from_le_bytes(bytes)
}

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    let bytes: [u8; 2] = data[offset..offset + 2].try_into().unwrap_or([0; 2]);
    u16::from_le_bytes(bytes)
}

fn read_i32_le(data: &[u8], offset: usize) -> i32 {
    let bytes: [u8; 4] = data[offset..offset + 4].try_into().unwrap_or([0; 4]);
    i32::from_le_bytes(bytes)
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
        let flags: u32 = FLAG_TCS_ACTIVE;
        buf[OFF_FLAGS..OFF_FLAGS + 4].copy_from_slice(&flags.to_le_bytes());

        let telemetry = parse_decrypted(&buf)?;
        assert!(telemetry.flags.traction_control, "TCS flag should be set");
        Ok(())
    }

    #[test]
    fn test_asm_flag_mapped() -> TestResult {
        let mut buf = make_decrypted_buf();
        let flags: u32 = FLAG_ASM_ACTIVE;
        buf[OFF_FLAGS..OFF_FLAGS + 4].copy_from_slice(&flags.to_le_bytes());

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
}
