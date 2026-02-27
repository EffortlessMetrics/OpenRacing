//! KartKraft telemetry adapter.
//!
//! KartKraft outputs FlatBuffers-encoded UDP packets (default port 5000).
//! The schema is publicly available at:
//!   <https://github.com/motorsportgames/kartkraft-telemetry/blob/master/Schema/Frame.fbs>
//!
//! Enable via the in-game preferences menu, or by adding these lines to
//! `%localappdata%\project_k\Saved\Config\WindowsNoEditor\Game.ini`:
//!
//! ```ini
//! [/Script/project_k.UDPManager]
//! bConfigOverride=True
//! OutputEndpoints="127.0.0.1:5000"
//! bEnableOutputStandard=True
//! ```

use crate::{NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, telemetry_now_ns};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_PORT: u16 = 5000;
const MAX_PACKET_SIZE: usize = 1024;
const DEFAULT_HEARTBEAT_TIMEOUT_MS: u64 = 1_500;

const ENV_PORT: &str = "OPENRACING_KARTKRAFT_UDP_PORT";
const ENV_HEARTBEAT_TIMEOUT_MS: &str = "OPENRACING_KARTKRAFT_HEARTBEAT_TIMEOUT_MS";

/// KartKraft FlatBuffers file identifier ("KKFB" at bytes [4..8]).
const KKFB_IDENTIFIER: &[u8; 4] = b"KKFB";

/// Maximum steering angle for karts (degrees), used to normalise to [-1, 1].
const KART_MAX_STEER_DEG: f32 = 90.0;

// Frame table field indices (0-indexed, matching Frame.fbs)
const FRAME_FIELD_MOTION: usize = 1;
const FRAME_FIELD_DASH: usize = 2;
const FRAME_FIELD_VEHICLE_CONFIG: usize = 4;
const FRAME_FIELD_TRACK_CONFIG: usize = 5;

// Dashboard field indices
const DASH_FIELD_SPEED: usize = 0;
const DASH_FIELD_RPM: usize = 1;
const DASH_FIELD_STEER: usize = 2;
const DASH_FIELD_THROTTLE: usize = 3;
const DASH_FIELD_BRAKE: usize = 4;
const DASH_FIELD_GEAR: usize = 5;

// VehicleConfig field indices
const VCFG_FIELD_RPM_MAX: usize = 1;

// TrackConfig field indices
const TRKFG_FIELD_NAME: usize = 0;

// Motion field indices
const MOTION_FIELD_TRACTION_LOSS: usize = 6;

// ── Minimal FlatBuffers reader ───────────────────────────────────────────────

fn read_u16_le(buf: &[u8], pos: usize) -> Option<u16> {
    buf.get(pos..pos + 2)
        .and_then(|b| b.try_into().ok())
        .map(u16::from_le_bytes)
}

fn read_i32_le(buf: &[u8], pos: usize) -> Option<i32> {
    buf.get(pos..pos + 4)
        .and_then(|b| b.try_into().ok())
        .map(i32::from_le_bytes)
}

fn read_u32_le(buf: &[u8], pos: usize) -> Option<u32> {
    buf.get(pos..pos + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
}

fn read_f32_le(buf: &[u8], pos: usize) -> Option<f32> {
    buf.get(pos..pos + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
}

/// Return the buffer position of a field's data inside a FlatBuffers table.
///
/// In FlatBuffers:
/// - `buf[table_pos..table_pos+4]` is an i32 soffset to the vtable:
///   `vtable_pos = table_pos − soffset`
/// - The vtable header is two u16s: `[vtable_size, object_size]`.
/// - Field N occupies vtable slot `N + 2` (byte offset `vtable_pos + 4 + N*2`).
/// - A slot value of `0` means the field is absent; otherwise it is the byte
///   offset from `table_pos` to the field's data.
fn fb_field_pos(buf: &[u8], table_pos: usize, field_n: usize) -> Option<usize> {
    let soffset = read_i32_le(buf, table_pos)?;
    let vtable_pos = (table_pos as i64 - soffset as i64) as usize;

    let vtable_size = read_u16_le(buf, vtable_pos)? as usize;
    let voffset_slot = vtable_pos + 4 + field_n * 2;
    if voffset_slot + 2 > vtable_pos + vtable_size {
        return None;
    }

    let field_offset = read_u16_le(buf, voffset_slot)?;
    if field_offset == 0 {
        return None;
    }
    Some(table_pos + field_offset as usize)
}

/// Resolve a table-reference field to the position of its sub-table.
///
/// At the field's data position there is a forward u32 UOffset; the sub-table
/// begins at `ref_pos + uoffset`.
fn fb_subtable_pos(buf: &[u8], table_pos: usize, field_n: usize) -> Option<usize> {
    let ref_pos = fb_field_pos(buf, table_pos, field_n)?;
    let offset = read_u32_le(buf, ref_pos)? as usize;
    Some(ref_pos + offset)
}

fn fb_f32(buf: &[u8], table_pos: usize, field_n: usize) -> Option<f32> {
    read_f32_le(buf, fb_field_pos(buf, table_pos, field_n)?)
}

fn fb_i8(buf: &[u8], table_pos: usize, field_n: usize) -> Option<i8> {
    let pos = fb_field_pos(buf, table_pos, field_n)?;
    buf.get(pos).copied().map(|b| b as i8)
}

/// Read a FlatBuffers UTF-8 string field.
///
/// String layout: `[u32 length][bytes…]`, reached via a forward UOffset at the
/// field's data position.
fn fb_str<'a>(buf: &'a [u8], table_pos: usize, field_n: usize) -> Option<&'a str> {
    let ref_pos = fb_field_pos(buf, table_pos, field_n)?;
    let str_offset = read_u32_le(buf, ref_pos)? as usize;
    let str_start = ref_pos + str_offset;
    let str_len = read_u32_le(buf, str_start)? as usize;
    let str_bytes = buf.get(str_start + 4..str_start + 4 + str_len)?;
    std::str::from_utf8(str_bytes).ok()
}

// ── Packet parser ────────────────────────────────────────────────────────────

fn parse_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < 8 {
        return Err(anyhow!(
            "KartKraft packet too short ({} bytes, need ≥ 8)",
            data.len()
        ));
    }

    // Verify "KKFB" file identifier at bytes [4..8].
    if data.get(4..8) != Some(KKFB_IDENTIFIER.as_slice()) {
        return Err(anyhow!("KartKraft: missing KKFB file identifier"));
    }

    // Root table offset is a u32 LE at bytes [0..4].
    let root_offset = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    if root_offset >= data.len() {
        return Err(anyhow!("KartKraft: root offset {root_offset} out of bounds"));
    }
    let frame_pos = root_offset;

    // Dashboard is required for basic telemetry.
    let dash_pos = fb_subtable_pos(data, frame_pos, FRAME_FIELD_DASH)
        .ok_or_else(|| anyhow!("KartKraft: missing Dashboard data in packet"))?;

    let speed = fb_f32(data, dash_pos, DASH_FIELD_SPEED).unwrap_or(0.0).max(0.0);
    let rpm = fb_f32(data, dash_pos, DASH_FIELD_RPM).unwrap_or(0.0).max(0.0);
    let steer_deg = fb_f32(data, dash_pos, DASH_FIELD_STEER).unwrap_or(0.0);
    let throttle = fb_f32(data, dash_pos, DASH_FIELD_THROTTLE).unwrap_or(0.0).clamp(0.0, 1.0);
    let brake = fb_f32(data, dash_pos, DASH_FIELD_BRAKE).unwrap_or(0.0).clamp(0.0, 1.0);
    // Gear: 0 = neutral, −1 = reverse, 1..N = forward gears.
    let gear = fb_i8(data, dash_pos, DASH_FIELD_GEAR).unwrap_or(0);

    // Normalise steer degrees to [-1, 1].
    let steering_angle = (steer_deg / KART_MAX_STEER_DEG).clamp(-1.0, 1.0);

    // Optional VehicleConfig: max RPM for display.
    let max_rpm = fb_subtable_pos(data, frame_pos, FRAME_FIELD_VEHICLE_CONFIG)
        .and_then(|vc| fb_f32(data, vc, VCFG_FIELD_RPM_MAX))
        .unwrap_or(0.0);

    // Optional TrackConfig: track name.
    let track_id = fb_subtable_pos(data, frame_pos, FRAME_FIELD_TRACK_CONFIG)
        .and_then(|tc| fb_str(data, tc, TRKFG_FIELD_NAME))
        .map(|s| s.to_string());

    // Optional Motion: traction loss used as slip_ratio proxy.
    let slip_ratio = fb_subtable_pos(data, frame_pos, FRAME_FIELD_MOTION)
        .and_then(|m| fb_f32(data, m, MOTION_FIELD_TRACTION_LOSS))
        .map(|tl| tl.abs().clamp(0.0, 1.0))
        .unwrap_or(0.0);

    let mut builder = NormalizedTelemetry::builder()
        .speed_ms(speed)
        .rpm(rpm)
        .gear(gear)
        .throttle(throttle)
        .brake(brake)
        .steering_angle(steering_angle)
        .slip_ratio(slip_ratio);

    if max_rpm > 0.0 {
        builder = builder.max_rpm(max_rpm);
    }
    if let Some(track) = track_id {
        builder = builder.track_id(track);
    }

    Ok(builder.build())
}

// ── Adapter ──────────────────────────────────────────────────────────────────

/// KartKraft telemetry adapter (FlatBuffers UDP).
#[derive(Clone)]
pub struct KartKraftAdapter {
    bind_port: u16,
    update_rate: Duration,
    heartbeat_timeout: Duration,
    last_packet_ns: Arc<AtomicU64>,
}

impl Default for KartKraftAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl KartKraftAdapter {
    pub fn new() -> Self {
        let bind_port = std::env::var(ENV_PORT)
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .filter(|&p| p > 0)
            .unwrap_or(DEFAULT_PORT);

        let heartbeat_ms = std::env::var(ENV_HEARTBEAT_TIMEOUT_MS)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&t| t > 0)
            .unwrap_or(DEFAULT_HEARTBEAT_TIMEOUT_MS);

        Self {
            bind_port,
            update_rate: Duration::from_millis(16),
            heartbeat_timeout: Duration::from_millis(heartbeat_ms),
            last_packet_ns: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }

    fn is_recent_packet(&self) -> bool {
        let last = self.last_packet_ns.load(Ordering::Relaxed);
        if last == 0 {
            return false;
        }
        let elapsed_ns = u128::from(telemetry_now_ns()).saturating_sub(u128::from(last));
        elapsed_ns <= self.heartbeat_timeout.as_nanos()
    }
}

#[async_trait]
impl TelemetryAdapter for KartKraftAdapter {
    fn game_id(&self) -> &str {
        "kartkraft"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let bind_port = self.bind_port;
        let update_rate = self.update_rate;
        let last_packet_ns = Arc::clone(&self.last_packet_ns);
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(error) => {
                    warn!(error = %error, port = bind_port, "KartKraft UDP socket bind failed");
                    return;
                }
            };
            info!(port = bind_port, "KartKraft UDP adapter bound");

            let mut buf = vec![0u8; MAX_PACKET_SIZE];
            let mut sequence = 0u64;
            let timeout = (update_rate * 4).max(Duration::from_millis(25));

            loop {
                let recv = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
                let len = match recv {
                    Ok(Ok(len)) => len,
                    Ok(Err(error)) => {
                        warn!(error = %error, "KartKraft UDP receive error");
                        continue;
                    }
                    Err(_) => {
                        debug!("KartKraft UDP receive timeout");
                        continue;
                    }
                };

                let normalized = match parse_packet(&buf[..len]) {
                    Ok(n) => n,
                    Err(error) => {
                        debug!(error = %error, "Failed to parse KartKraft packet");
                        continue;
                    }
                };

                last_packet_ns.store(telemetry_now_ns(), Ordering::Relaxed);

                let frame = TelemetryFrame::new(normalized, telemetry_now_ns(), sequence, len);
                if tx.send(frame).await.is_err() {
                    break;
                }
                sequence = sequence.saturating_add(1);
            }

            info!("KartKraft telemetry monitoring stopped");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.is_recent_packet())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Build a minimal valid KartKraft FlatBuffer containing only a Dashboard
    /// sub-table so that parse_packet can extract the core telemetry fields.
    ///
    /// FlatBuffers binary layout (little-endian, vtables before tables):
    ///
    /// ```text
    /// [00..04] root_offset  (u32)  → position of Frame table
    /// [04..08] "KKFB"       file identifier
    ///
    /// vtable_dash (u16[]):
    ///   vtable_size  = 4 + 6*2 = 16  (header + 6 field slots)
    ///   object_size  = 4 + 6*4 = 28  (soffset + 6 × f32/i8 data)
    ///   field slots  = [4, 8, 12, 16, 20, 24]  (offsets from dash_table_pos)
    ///
    /// dash_table:
    ///   soffset → vtable_dash
    ///   speed, rpm, steer, throttle, brake (all f32)
    ///   gear (i8, padded)
    ///
    /// vtable_frame (u16[]):
    ///   vtable_size  = 4 + 3*2 = 10  (header + fields 0,1,2)
    ///   object_size  = 4 + 2*4 = 12  (soffset + 2 × u32 offsets for motion[absent],dash)
    ///   field slots  = [0, 0, 4]     (motion absent, dash at offset 4)
    ///
    /// frame_table:
    ///   soffset → vtable_frame
    ///   (pad to align)
    ///   dash_uoffset  (u32)  → dash_table (forward UOffset)
    /// ```
    fn make_test_packet(
        speed: f32,
        rpm: f32,
        steer_deg: f32,
        throttle: f32,
        brake: f32,
        gear: i8,
    ) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();

        // Helper closures
        let push_u16 = |buf: &mut Vec<u8>, v: u16| buf.extend_from_slice(&v.to_le_bytes());
        let push_i32 = |buf: &mut Vec<u8>, v: i32| buf.extend_from_slice(&v.to_le_bytes());
        let push_u32 = |buf: &mut Vec<u8>, v: u32| buf.extend_from_slice(&v.to_le_bytes());
        let push_f32 = |buf: &mut Vec<u8>, v: f32| buf.extend_from_slice(&v.to_le_bytes());

        // ── Dashboard vtable (6 scalar fields) ───────────────────────────────
        //    field offsets from dash_table_pos:
        //      speed    @ 4   (after soffset i32)
        //      rpm      @ 8
        //      steer    @ 12
        //      throttle @ 16
        //      brake    @ 20
        //      gear     @ 24  (i8, stored as 1 byte but we'll pad to 4 for simplicity)
        let vtable_dash_start = buf.len(); // byte 0
        push_u16(&mut buf, 16); // vtable_size = 4 + 6*2
        push_u16(&mut buf, 28); // object_size = 4 + 6*4
        push_u16(&mut buf, 4);  // field 0 (speed)
        push_u16(&mut buf, 8);  // field 1 (rpm)
        push_u16(&mut buf, 12); // field 2 (steer)
        push_u16(&mut buf, 16); // field 3 (throttle)
        push_u16(&mut buf, 20); // field 4 (brake)
        push_u16(&mut buf, 24); // field 5 (gear)

        // ── Dashboard table ───────────────────────────────────────────────────
        let dash_table_start = buf.len(); // byte 16
        // soffset = dash_table_start - vtable_dash_start = 16
        push_i32(&mut buf, (dash_table_start - vtable_dash_start) as i32);
        push_f32(&mut buf, speed);       // field 0 @ offset 4
        push_f32(&mut buf, rpm);          // field 1 @ offset 8
        push_f32(&mut buf, steer_deg);    // field 2 @ offset 12
        push_f32(&mut buf, throttle);     // field 3 @ offset 16
        push_f32(&mut buf, brake);        // field 4 @ offset 20
        // gear is i8 but we store it at 4-byte alignment for simplicity
        buf.push(gear as u8);
        buf.push(0);
        buf.push(0);
        buf.push(0);

        // ── Frame vtable (3 slots: timestamp absent, motion absent, dash present) ──
        let vtable_frame_start = buf.len(); // byte 44
        push_u16(&mut buf, 10); // vtable_size = 4 + 3*2
        push_u16(&mut buf, 12); // object_size = 4 + 2*u32 (soffset + dash UOffset placeholder)
        push_u16(&mut buf, 0);  // field 0 (timestamp) absent
        push_u16(&mut buf, 0);  // field 1 (motion) absent
        push_u16(&mut buf, 4);  // field 2 (dash) at byte offset 4 from frame_table_start

        // ── Frame table ───────────────────────────────────────────────────────
        let frame_table_start = buf.len(); // byte 54
        // soffset = frame_table_start - vtable_frame_start = 10
        push_i32(&mut buf, (frame_table_start - vtable_frame_start) as i32);
        // dash UOffset field (offset 4 from frame_table_start = position 58 in buf)
        // The UOffset is relative to its own position.
        // dash_table_start = 16 (computed above)
        // ref_pos = frame_table_start + 4 = 58
        // uoffset = dash_table_start - ref_pos = 16 - 58 = negative → PROBLEM
        //
        // FlatBuffers forward offsets must be positive. We need to place the
        // referenced table *after* the current position. Rebuild the buffer
        // so the Frame table comes first, then the Dash table.
        let _ = vtable_dash_start; // suppress warning; we'll rebuild

        // ──────────────────────────────────────────────────────────────────────
        // Rebuild: Frame must come before the Dash it references so that the
        // forward UOffset is positive.
        // ──────────────────────────────────────────────────────────────────────
        buf.clear();

        // Reserve space for root_offset (u32) and file identifier (4 bytes).
        push_u32(&mut buf, 0);          // placeholder for root_offset
        buf.extend_from_slice(b"KKFB"); // file identifier

        // ── Frame vtable ─────────────────────────────────────────────────────
        let vt_frame_start = buf.len();
        push_u16(&mut buf, 10); // vtable_size
        push_u16(&mut buf, 12); // object_size (soffset[4] + dash_uoffset[4] + align[4])
        push_u16(&mut buf, 0);  // field 0 absent
        push_u16(&mut buf, 0);  // field 1 absent
        push_u16(&mut buf, 4);  // field 2 (dash) offset 4 from frame_table

        // ── Frame table ───────────────────────────────────────────────────────
        let frame_table_pos = buf.len();
        // soffset: frame_table_pos − vt_frame_start
        push_i32(&mut buf, (frame_table_pos - vt_frame_start) as i32);
        // Placeholder for dash UOffset (at frame_table_pos + 4)
        push_u32(&mut buf, 0);          // will be patched
        push_u32(&mut buf, 0);          // padding to match object_size = 12

        // Patch root_offset to point to frame_table_pos
        let root_offset_val = frame_table_pos as u32;
        buf[0..4].copy_from_slice(&root_offset_val.to_le_bytes());

        // ── Dashboard vtable ──────────────────────────────────────────────────
        let vt_dash_start = buf.len();
        push_u16(&mut buf, 16); // vtable_size = 4 + 6*2
        push_u16(&mut buf, 28); // object_size = 4 + 6*4
        push_u16(&mut buf, 4);  // speed
        push_u16(&mut buf, 8);  // rpm
        push_u16(&mut buf, 12); // steer
        push_u16(&mut buf, 16); // throttle
        push_u16(&mut buf, 20); // brake
        push_u16(&mut buf, 24); // gear

        // ── Dashboard table ───────────────────────────────────────────────────
        let dash_table_pos = buf.len();
        push_i32(&mut buf, (dash_table_pos - vt_dash_start) as i32);
        push_f32(&mut buf, speed);
        push_f32(&mut buf, rpm);
        push_f32(&mut buf, steer_deg);
        push_f32(&mut buf, throttle);
        push_f32(&mut buf, brake);
        buf.push(gear as u8);
        buf.push(0);
        buf.push(0);
        buf.push(0);

        // Patch dash UOffset:  ref_pos = frame_table_pos + 4
        //                      uoffset = dash_table_pos − ref_pos
        let ref_pos = frame_table_pos + 4;
        let dash_uoffset = (dash_table_pos - ref_pos) as u32;
        buf[ref_pos..ref_pos + 4].copy_from_slice(&dash_uoffset.to_le_bytes());

        buf
    }

    #[test]
    fn test_parse_valid_packet() -> TestResult {
        let data = make_test_packet(25.0, 8000.0, 45.0, 0.8, 0.1, 3);
        let t = parse_packet(&data)?;

        assert!((t.speed_ms - 25.0).abs() < 0.001, "speed_ms {}", t.speed_ms);
        assert!((t.rpm - 8000.0).abs() < 0.1, "rpm {}", t.rpm);
        assert!((t.throttle - 0.8).abs() < 0.001, "throttle {}", t.throttle);
        assert!((t.brake - 0.1).abs() < 0.001, "brake {}", t.brake);
        assert_eq!(t.gear, 3, "gear");
        // steer: 45° / 90° = 0.5
        assert!((t.steering_angle - 0.5).abs() < 0.001, "steering_angle {}", t.steering_angle);
        Ok(())
    }

    #[test]
    fn test_too_short_rejected() {
        assert!(parse_packet(&[]).is_err());
        assert!(parse_packet(&[0u8; 7]).is_err());
    }

    #[test]
    fn test_wrong_identifier_rejected() {
        let mut data = make_test_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0);
        data[4] = b'X'; // corrupt identifier
        assert!(parse_packet(&data).is_err());
    }

    #[test]
    fn test_adapter_game_id() {
        assert_eq!(KartKraftAdapter::new().game_id(), "kartkraft");
    }

    #[test]
    fn test_adapter_update_rate() {
        let adapter = KartKraftAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_normalize_delegates_to_parse() -> TestResult {
        let adapter = KartKraftAdapter::new();
        let data = make_test_packet(10.0, 5000.0, -30.0, 0.5, 0.0, 2);
        let t = adapter.normalize(&data)?;
        assert!((t.rpm - 5000.0).abs() < 0.1);
        assert_eq!(t.gear, 2);
        Ok(())
    }

    #[test]
    fn test_steering_normalisation() -> TestResult {
        // Full right lock (90°) → 1.0
        let data = make_test_packet(0.0, 0.0, 90.0, 0.0, 0.0, 0);
        let t = parse_packet(&data)?;
        assert!((t.steering_angle - 1.0).abs() < 0.001);

        // Full left lock (−90°) → −1.0
        let data = make_test_packet(0.0, 0.0, -90.0, 0.0, 0.0, 0);
        let t = parse_packet(&data)?;
        assert!((t.steering_angle + 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_reverse_gear() -> TestResult {
        let data = make_test_packet(5.0, 3000.0, 0.0, 0.1, 0.0, -1);
        let t = parse_packet(&data)?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn test_throttle_brake_clamped() -> TestResult {
        let data = make_test_packet(0.0, 0.0, 0.0, 2.0, -1.0, 0);
        let t = parse_packet(&data)?;
        assert!((t.throttle - 1.0).abs() < 0.001, "throttle clamped to 1");
        assert!(t.brake.abs() < 0.001, "brake clamped to 0");
        Ok(())
    }

    #[cfg(test)]
    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #![proptest_config(proptest::test_runner::Config::with_cases(300))]

            #[test]
            fn prop_arbitrary_bytes_never_panic(
                data in proptest::collection::vec(any::<u8>(), 0usize..512)
            ) {
                let _ = parse_packet(&data);
            }

            #[test]
            fn prop_short_packet_returns_err(
                data in proptest::collection::vec(any::<u8>(), 0usize..8)
            ) {
                prop_assert!(parse_packet(&data).is_err());
            }

            #[test]
            fn prop_valid_packet_speed_nonneg(
                speed in 0.0f32..=100.0,
                rpm   in 0.0f32..=20000.0,
                steer in -90.0f32..=90.0,
                thr   in 0.0f32..=1.0,
                brk   in 0.0f32..=1.0,
                gear  in -1i8..=8i8,
            ) {
                let data = make_test_packet(speed, rpm, steer, thr, brk, gear);
                let t = parse_packet(&data).expect("valid packet must parse");
                prop_assert!(t.speed_ms >= 0.0);
                prop_assert!(t.rpm >= 0.0);
                prop_assert!(t.steering_angle >= -1.0 && t.steering_angle <= 1.0);
            }
        }
    }
}
