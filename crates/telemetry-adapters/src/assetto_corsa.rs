//! Assetto Corsa (original) telemetry adapter using Remote Telemetry UDP.
//!
//! Implements telemetry via AC's Remote Telemetry UDP protocol (port 9996).
//! Requires a 3-step handshake: connect → response → subscribe.
//! Update packets use the RTCarInfo struct (328 bytes, little-endian).
//!
//! Reference: <https://github.com/vpicon/acudp/blob/master/UDP.md>
#![cfg_attr(not(windows), allow(unused, dead_code))]

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

/// Verified: AC Remote Telemetry handshake port per official SDK (vpicon/acudp).
const DEFAULT_AC_PORT: u16 = 9996;
/// RTCarInfo struct size (AC Remote Telemetry UDP update packet).
const AC_RTCARINFO_SIZE: usize = 328;
const MAX_PACKET_SIZE: usize = 512;

// Handshake operation IDs for AC Remote Telemetry UDP protocol.
const OP_HANDSHAKE: i32 = 0;
const OP_SUBSCRIBE_UPDATE: i32 = 1;

// Byte offsets in the AC RTCarInfo struct (little-endian, naturally aligned).
// Reference: https://github.com/vpicon/acudp/blob/master/UDP.md
#[cfg(test)]
const OFF_SPEED_KMH: usize = 8; // f32 (used in tests only; parse uses speed_Ms)
const OFF_SPEED_MS: usize = 16; // f32
const OFF_ABS_IN_ACTION: usize = 21; // bool (u8)
const OFF_TC_IN_ACTION: usize = 22; // bool (u8)
const OFF_IN_PIT: usize = 24; // bool (u8)
const OFF_ENGINE_LIMITER: usize = 25; // bool (u8)
const OFF_ACCG_VERTICAL: usize = 28; // f32
const OFF_ACCG_HORIZONTAL: usize = 32; // f32
const OFF_ACCG_FRONTAL: usize = 36; // f32
const OFF_LAP_TIME: usize = 40; // i32 (milliseconds)
const OFF_LAST_LAP: usize = 44; // i32 (milliseconds)
const OFF_BEST_LAP: usize = 48; // i32 (milliseconds)
const OFF_LAP_COUNT: usize = 52; // i32
const OFF_GAS: usize = 56; // f32
const OFF_BRAKE: usize = 60; // f32
const OFF_CLUTCH: usize = 64; // f32
const OFF_RPM: usize = 68; // f32
const OFF_STEER: usize = 72; // f32
const OFF_GEAR: usize = 76; // i32 (0=R, 1=N, 2=1st, ...)
const OFF_SLIP_ANGLE_FL: usize = 100; // f32[4] at 100,104,108,112
const OFF_SLIP_RATIO_FL: usize = 132; // f32[4] at 132,136,140,144

/// Assetto Corsa (original) telemetry adapter using Remote Telemetry UDP.
pub struct AssettoCorsaAdapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for AssettoCorsaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AssettoCorsaAdapter {
    pub fn new() -> Self {
        Self {
            bind_port: DEFAULT_AC_PORT,
            update_rate: Duration::from_millis(16),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

fn parse_ac_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < AC_RTCARINFO_SIZE {
        return Err(anyhow!(
            "AC RTCarInfo packet too short: expected {AC_RTCARINFO_SIZE}, got {}",
            data.len()
        ));
    }

    let speed_ms = read_f32_le(data, OFF_SPEED_MS).unwrap_or(0.0);
    let rpm = read_f32_le(data, OFF_RPM).unwrap_or(0.0);
    let steer = read_f32_le(data, OFF_STEER).unwrap_or(0.0).clamp(-1.0, 1.0);
    let gas = read_f32_le(data, OFF_GAS).unwrap_or(0.0);
    let brake = read_f32_le(data, OFF_BRAKE).unwrap_or(0.0);
    let clutch = read_f32_le(data, OFF_CLUTCH).unwrap_or(0.0);

    let gear_raw = read_i32_le(data, OFF_GEAR).unwrap_or(1); // default neutral
    // AC gear: 0=Reverse, 1=Neutral, 2=1st gear, ...
    // Normalized: -1=Reverse, 0=Neutral, 1=1st gear, ...
    let gear: i8 = match gear_raw {
        0 => -1,
        1 => 0,
        g => (g - 1).clamp(i32::from(i8::MIN), i32::from(i8::MAX)) as i8,
    };

    // G-forces
    let vertical_g = read_f32_le(data, OFF_ACCG_VERTICAL).unwrap_or(0.0);
    let lateral_g = read_f32_le(data, OFF_ACCG_HORIZONTAL).unwrap_or(0.0);
    let longitudinal_g = read_f32_le(data, OFF_ACCG_FRONTAL).unwrap_or(0.0);

    // Flags
    let flags = TelemetryFlags {
        abs_active: read_u8(data, OFF_ABS_IN_ACTION) != 0,
        traction_control: read_u8(data, OFF_TC_IN_ACTION) != 0,
        in_pits: read_u8(data, OFF_IN_PIT) != 0,
        engine_limiter: read_u8(data, OFF_ENGINE_LIMITER) != 0,
        ..TelemetryFlags::default()
    };

    // Lap timing (i32 milliseconds → f32 seconds)
    let current_lap_ms = read_i32_le(data, OFF_LAP_TIME).unwrap_or(0);
    let last_lap_ms = read_i32_le(data, OFF_LAST_LAP).unwrap_or(0);
    let best_lap_ms = read_i32_le(data, OFF_BEST_LAP).unwrap_or(0);
    let lap_count = read_i32_le(data, OFF_LAP_COUNT).unwrap_or(0);

    // Slip angles (per-wheel)
    let slip_angle_fl = read_f32_le(data, OFF_SLIP_ANGLE_FL).unwrap_or(0.0);
    let slip_angle_fr = read_f32_le(data, OFF_SLIP_ANGLE_FL + 4).unwrap_or(0.0);
    let slip_angle_rl = read_f32_le(data, OFF_SLIP_ANGLE_FL + 8).unwrap_or(0.0);
    let slip_angle_rr = read_f32_le(data, OFF_SLIP_ANGLE_FL + 12).unwrap_or(0.0);

    // Slip ratios (per-wheel) — no per-wheel builder methods, use extended map
    let slip_ratio_fl = read_f32_le(data, OFF_SLIP_RATIO_FL).unwrap_or(0.0);
    let slip_ratio_fr = read_f32_le(data, OFF_SLIP_RATIO_FL + 4).unwrap_or(0.0);
    let slip_ratio_rl = read_f32_le(data, OFF_SLIP_RATIO_FL + 8).unwrap_or(0.0);
    let slip_ratio_rr = read_f32_le(data, OFF_SLIP_RATIO_FL + 12).unwrap_or(0.0);

    // Overall slip ratio: average of per-wheel absolute values (guard at low speed).
    let slip_ratio = if speed_ms > 1.0 {
        ((slip_ratio_fl.abs() + slip_ratio_fr.abs() + slip_ratio_rl.abs() + slip_ratio_rr.abs())
            / 4.0)
            .min(1.0)
    } else {
        0.0
    };

    let mut builder = NormalizedTelemetry::builder()
        .steering_angle(steer)
        .throttle(gas)
        .brake(brake)
        .clutch(clutch)
        .speed_ms(speed_ms)
        .rpm(rpm)
        .gear(gear)
        .vertical_g(vertical_g)
        .lateral_g(lateral_g)
        .longitudinal_g(longitudinal_g)
        .slip_ratio(slip_ratio)
        .flags(flags)
        .slip_angle_fl(slip_angle_fl)
        .slip_angle_fr(slip_angle_fr)
        .slip_angle_rl(slip_angle_rl)
        .slip_angle_rr(slip_angle_rr)
        .extended("slip_ratio_fl", TelemetryValue::Float(slip_ratio_fl))
        .extended("slip_ratio_fr", TelemetryValue::Float(slip_ratio_fr))
        .extended("slip_ratio_rl", TelemetryValue::Float(slip_ratio_rl))
        .extended("slip_ratio_rr", TelemetryValue::Float(slip_ratio_rr));

    if current_lap_ms > 0 {
        builder = builder.current_lap_time_s(current_lap_ms as f32 / 1000.0);
    }
    if last_lap_ms > 0 {
        builder = builder.last_lap_time_s(last_lap_ms as f32 / 1000.0);
    }
    if best_lap_ms > 0 {
        builder = builder.best_lap_time_s(best_lap_ms as f32 / 1000.0);
    }
    if lap_count > 0 {
        builder = builder.lap(lap_count.clamp(0, i32::from(u16::MAX)) as u16);
    }

    Ok(builder.build())
}

#[async_trait]
impl TelemetryAdapter for AssettoCorsaAdapter {
    fn game_id(&self) -> &str {
        "assetto_corsa"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let ac_port = self.bind_port;
        let update_rate = self.update_rate;

        tokio::spawn(async move {
            // Bind to any available local port (AC listens on ac_port).
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind AC UDP socket: {e}");
                    return;
                }
            };

            let ac_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, ac_port));
            if let Err(e) = socket.connect(ac_addr).await {
                warn!("Failed to connect to AC at {ac_addr}: {e}");
                return;
            }

            // AC Remote Telemetry handshake: send HANDSHAKE, receive response, send SUBSCRIBE.
            let handshake = build_handshake_packet(OP_HANDSHAKE);
            if let Err(e) = socket.send(&handshake).await {
                warn!("Failed to send AC handshake: {e}");
                return;
            }

            let mut buf = [0u8; MAX_PACKET_SIZE];
            match tokio::time::timeout(Duration::from_secs(2), socket.recv(&mut buf)).await {
                Ok(Ok(_)) => info!("AC handshake response received"),
                Ok(Err(e)) => {
                    warn!("Failed to receive AC handshake response: {e}");
                    return;
                }
                Err(_) => {
                    warn!("AC handshake response timeout — is Assetto Corsa running?");
                    return;
                }
            }

            let subscribe = build_handshake_packet(OP_SUBSCRIBE_UPDATE);
            if let Err(e) = socket.send(&subscribe).await {
                warn!("Failed to send AC subscribe request: {e}");
                return;
            }

            info!("AC adapter connected and subscribed via port {ac_port}");
            let mut frame_seq = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_ac_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping AC monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse AC packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("AC UDP receive error: {e}"),
                    Err(_) => debug!("No AC telemetry data received (timeout)"),
                }
            }
            info!("Stopped AC telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_ac_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_ac_process_running())
    }
}

#[cfg(windows)]
fn is_ac_process_running() -> bool {
    use std::ffi::CStr;
    use std::mem;
    use winapi::um::{
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        tlhelp32::{
            CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
            TH32CS_SNAPPROCESS,
        },
    };

    // SAFETY: Windows snapshot API with proper initialization.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: PROCESSENTRY32 = mem::zeroed();
        entry.dwSize = mem::size_of::<PROCESSENTRY32>() as u32;
        let mut found = false;
        if Process32First(snapshot, &mut entry) != 0 {
            loop {
                let name = CStr::from_ptr(entry.szExeFile.as_ptr())
                    .to_string_lossy()
                    .to_ascii_lowercase();
                if name == "acs.exe" {
                    found = true;
                    break;
                }
                if Process32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
        found
    }
}

#[cfg(not(windows))]
fn is_ac_process_running() -> bool {
    is_process_running_linux("acs")
}

#[cfg(not(windows))]
fn is_process_running_linux(process_name: &str) -> bool {
    use std::fs;
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let comm_path = entry.path().join("comm");
            if let Ok(name) = fs::read_to_string(&comm_path)
                && name.trim() == process_name
            {
                return true;
            }
        }
    }
    false
}

fn read_f32_le(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
        .filter(|v| v.is_finite())
}

fn read_i32_le(data: &[u8], offset: usize) -> Option<i32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(i32::from_le_bytes)
}

fn read_u8(data: &[u8], offset: usize) -> u8 {
    data.get(offset).copied().unwrap_or(0)
}

fn build_handshake_packet(operation_id: i32) -> [u8; 12] {
    let mut packet = [0u8; 12];
    packet[0..4].copy_from_slice(&1i32.to_le_bytes()); // identifier
    packet[4..8].copy_from_slice(&1i32.to_le_bytes()); // version
    packet[8..12].copy_from_slice(&operation_id.to_le_bytes());
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn make_valid_ac_packet() -> Vec<u8> {
        let mut data = vec![0u8; AC_RTCARINFO_SIZE];
        // identifier = 'a'
        data[0..4].copy_from_slice(&(b'a' as i32).to_le_bytes());
        // size
        data[4..8].copy_from_slice(&(AC_RTCARINFO_SIZE as i32).to_le_bytes());
        // speed_Kmh (float) at offset 8
        data[OFF_SPEED_KMH..OFF_SPEED_KMH + 4].copy_from_slice(&120.0f32.to_le_bytes());
        // speed_Ms (float) at offset 16
        let speed_ms = 120.0f32 / 3.6;
        data[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&speed_ms.to_le_bytes());
        // flags (bool u8)
        data[OFF_ABS_IN_ACTION] = 1;
        data[OFF_TC_IN_ACTION] = 0;
        data[OFF_IN_PIT] = 1;
        data[OFF_ENGINE_LIMITER] = 0;
        // G-forces
        data[OFF_ACCG_VERTICAL..OFF_ACCG_VERTICAL + 4].copy_from_slice(&1.02f32.to_le_bytes());
        data[OFF_ACCG_HORIZONTAL..OFF_ACCG_HORIZONTAL + 4]
            .copy_from_slice(&(-0.35f32).to_le_bytes());
        data[OFF_ACCG_FRONTAL..OFF_ACCG_FRONTAL + 4].copy_from_slice(&0.45f32.to_le_bytes());
        // lap timing (i32 milliseconds)
        data[OFF_LAP_TIME..OFF_LAP_TIME + 4].copy_from_slice(&62500i32.to_le_bytes());
        data[OFF_LAST_LAP..OFF_LAST_LAP + 4].copy_from_slice(&61200i32.to_le_bytes());
        data[OFF_BEST_LAP..OFF_BEST_LAP + 4].copy_from_slice(&60800i32.to_le_bytes());
        data[OFF_LAP_COUNT..OFF_LAP_COUNT + 4].copy_from_slice(&3i32.to_le_bytes());
        // gas at offset 56
        data[OFF_GAS..OFF_GAS + 4].copy_from_slice(&0.8f32.to_le_bytes());
        // brake at offset 60
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&0.1f32.to_le_bytes());
        // rpm at offset 68
        data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&6000.0f32.to_le_bytes());
        // steer at offset 72
        data[OFF_STEER..OFF_STEER + 4].copy_from_slice(&0.3f32.to_le_bytes());
        // gear at offset 76 (AC: 3 = 2nd gear; 0=R, 1=N, 2=1st, 3=2nd)
        data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&3i32.to_le_bytes());
        // slip angles (f32[4])
        data[OFF_SLIP_ANGLE_FL..OFF_SLIP_ANGLE_FL + 4].copy_from_slice(&0.5f32.to_le_bytes());
        data[OFF_SLIP_ANGLE_FL + 4..OFF_SLIP_ANGLE_FL + 8].copy_from_slice(&0.6f32.to_le_bytes());
        data[OFF_SLIP_ANGLE_FL + 8..OFF_SLIP_ANGLE_FL + 12].copy_from_slice(&0.7f32.to_le_bytes());
        data[OFF_SLIP_ANGLE_FL + 12..OFF_SLIP_ANGLE_FL + 16].copy_from_slice(&0.8f32.to_le_bytes());
        // slip ratios (f32[4])
        data[OFF_SLIP_RATIO_FL..OFF_SLIP_RATIO_FL + 4].copy_from_slice(&0.02f32.to_le_bytes());
        data[OFF_SLIP_RATIO_FL + 4..OFF_SLIP_RATIO_FL + 8].copy_from_slice(&0.03f32.to_le_bytes());
        data[OFF_SLIP_RATIO_FL + 8..OFF_SLIP_RATIO_FL + 12].copy_from_slice(&0.04f32.to_le_bytes());
        data[OFF_SLIP_RATIO_FL + 12..OFF_SLIP_RATIO_FL + 16]
            .copy_from_slice(&0.05f32.to_le_bytes());
        data
    }

    #[test]
    fn test_parse_valid_packet() -> TestResult {
        let data = make_valid_ac_packet();
        let result = parse_ac_packet(&data)?;
        assert!((result.rpm - 6000.0).abs() < 0.01);
        assert_eq!(result.gear, 2); // AC gear 3 → normalized 2
        assert!((result.speed_ms - 120.0 / 3.6).abs() < 0.1);
        assert!((result.steering_angle - 0.3).abs() < 0.001);
        assert!((result.throttle - 0.8).abs() < 0.001);
        assert!((result.brake - 0.1).abs() < 0.001);
        // G-forces
        assert!((result.vertical_g - 1.02).abs() < 0.001);
        assert!((result.lateral_g - (-0.35)).abs() < 0.001);
        assert!((result.longitudinal_g - 0.45).abs() < 0.001);
        // Flags
        let flags = &result.flags;
        assert!(flags.abs_active);
        assert!(!flags.traction_control);
        assert!(flags.in_pits);
        assert!(!flags.engine_limiter);
        // Lap timing
        assert!((result.current_lap_time_s - 62.5).abs() < 0.01);
        assert!((result.last_lap_time_s - 61.2).abs() < 0.01);
        assert!((result.best_lap_time_s - 60.8).abs() < 0.01);
        assert_eq!(result.lap, 3);
        // Slip angles
        assert!((result.slip_angle_fl - 0.5).abs() < 0.001);
        assert!((result.slip_angle_fr - 0.6).abs() < 0.001);
        assert!((result.slip_angle_rl - 0.7).abs() < 0.001);
        assert!((result.slip_angle_rr - 0.8).abs() < 0.001);
        // Slip ratios (extended map)
        assert_eq!(
            result.extended.get("slip_ratio_fl"),
            Some(&TelemetryValue::Float(0.02))
        );
        assert_eq!(
            result.extended.get("slip_ratio_fr"),
            Some(&TelemetryValue::Float(0.03))
        );
        assert_eq!(
            result.extended.get("slip_ratio_rl"),
            Some(&TelemetryValue::Float(0.04))
        );
        assert_eq!(
            result.extended.get("slip_ratio_rr"),
            Some(&TelemetryValue::Float(0.05))
        );
        Ok(())
    }

    #[test]
    fn test_parse_truncated_packet() -> TestResult {
        let data = vec![0u8; 10];
        let result = parse_ac_packet(&data);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_normalization_bounds() -> TestResult {
        let mut data = make_valid_ac_packet();
        data[OFF_STEER..OFF_STEER + 4].copy_from_slice(&2.5f32.to_le_bytes());
        data[OFF_GAS..OFF_GAS + 4].copy_from_slice(&1.5f32.to_le_bytes());
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&(-0.5f32).to_le_bytes());
        let result = parse_ac_packet(&data)?;
        assert!((result.steering_angle - 1.0).abs() < 0.001);
        // Builder clamps throttle to [0,1]
        assert!((result.throttle - 1.0).abs() < 0.001);
        // Builder clamps brake to [0,1], so -0.5 becomes 0.0
        assert!((result.brake - 0.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        let adapter = AssettoCorsaAdapter::new();
        assert_eq!(adapter.game_id(), "assetto_corsa");
    }

    #[test]
    fn test_adapter_expected_update_rate() {
        let adapter = AssettoCorsaAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_normalize_delegates_to_parse() -> TestResult {
        let adapter = AssettoCorsaAdapter::new();
        let data = make_valid_ac_packet();
        let result = adapter.normalize(&data)?;
        assert!(result.rpm > 0.0);
        Ok(())
    }

    #[test]
    fn test_parse_exact_min_size() -> TestResult {
        let data = vec![0u8; AC_RTCARINFO_SIZE];
        let result = parse_ac_packet(&data)?;
        assert_eq!(result.rpm, 0.0);
        assert_eq!(result.speed_ms, 0.0);
        Ok(())
    }

    // ─── Gear edge cases ─────────────────────────────────────────────────────

    #[test]
    fn test_gear_reverse_maps_to_minus_one() -> TestResult {
        let mut data = make_valid_ac_packet();
        data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&0i32.to_le_bytes());
        let result = parse_ac_packet(&data)?;
        assert_eq!(result.gear, -1, "AC gear 0 (reverse) must normalize to -1");
        Ok(())
    }

    #[test]
    fn test_gear_neutral_maps_to_zero() -> TestResult {
        let mut data = make_valid_ac_packet();
        data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&1i32.to_le_bytes());
        let result = parse_ac_packet(&data)?;
        assert_eq!(result.gear, 0, "AC gear 1 (neutral) must normalize to 0");
        Ok(())
    }

    #[test]
    fn test_gear_high_value_maps_correctly() -> TestResult {
        let mut data = make_valid_ac_packet();
        data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&7i32.to_le_bytes());
        let result = parse_ac_packet(&data)?;
        assert_eq!(result.gear, 6, "AC gear 7 must normalize to 6");
        Ok(())
    }

    #[test]
    fn test_gear_absurd_value_no_panic() -> TestResult {
        let mut data = make_valid_ac_packet();
        data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&255i32.to_le_bytes());
        let result = parse_ac_packet(&data)?;
        // 255 - 1 = 254, clamped to i8::MAX = 127
        assert_eq!(result.gear, 127, "AC gear 255 must clamp to i8::MAX (127)");
        Ok(())
    }

    // ─── Clutch assertion ────────────────────────────────────────────────────

    #[test]
    fn test_clutch_value_parsed() -> TestResult {
        let mut data = make_valid_ac_packet();
        data[OFF_CLUTCH..OFF_CLUTCH + 4].copy_from_slice(&0.5f32.to_le_bytes());
        let result = parse_ac_packet(&data)?;
        assert!(
            (result.clutch - 0.5).abs() < 0.001,
            "clutch must be ~0.5, got {}",
            result.clutch
        );
        Ok(())
    }

    // ─── Handshake packet byte layout ────────────────────────────────────────

    #[test]
    fn test_handshake_packet_byte_layout() -> TestResult {
        let pkt = build_handshake_packet(OP_HANDSHAKE);
        assert_eq!(pkt.len(), 12);
        // identifier field = 1 (little-endian)
        assert_eq!(i32::from_le_bytes([pkt[0], pkt[1], pkt[2], pkt[3]]), 1);
        // version field = 1 (little-endian)
        assert_eq!(i32::from_le_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]), 1);
        // operation_id = OP_HANDSHAKE (0)
        assert_eq!(i32::from_le_bytes([pkt[8], pkt[9], pkt[10], pkt[11]]), 0);
        Ok(())
    }

    #[test]
    fn test_subscribe_packet_byte_layout() -> TestResult {
        let pkt = build_handshake_packet(OP_SUBSCRIBE_UPDATE);
        // operation_id = OP_SUBSCRIBE_UPDATE (1)
        assert_eq!(i32::from_le_bytes([pkt[8], pkt[9], pkt[10], pkt[11]]), 1);
        Ok(())
    }

    // ─── Oversized packet ────────────────────────────────────────────────────

    #[test]
    fn test_oversized_packet_parses_correctly() -> TestResult {
        let mut data = make_valid_ac_packet();
        // Append extra bytes beyond the 328-byte RTCarInfo struct
        data.resize(512, 0xAB);
        let result = parse_ac_packet(&data)?;
        // Original fields must still parse correctly
        assert!((result.rpm - 6000.0).abs() < 0.01);
        assert_eq!(
            result.gear, 2,
            "gear must still parse from oversized packet"
        );
        Ok(())
    }

    // ─── NaN / Infinity values ───────────────────────────────────────────────

    #[test]
    fn test_nan_in_float_fields_defaults_to_zero() -> TestResult {
        let mut data = vec![0u8; AC_RTCARINFO_SIZE];
        let nan_bytes = f32::NAN.to_le_bytes();
        data[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&nan_bytes);
        data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&nan_bytes);
        data[OFF_GAS..OFF_GAS + 4].copy_from_slice(&nan_bytes);
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&nan_bytes);
        data[OFF_CLUTCH..OFF_CLUTCH + 4].copy_from_slice(&nan_bytes);
        data[OFF_STEER..OFF_STEER + 4].copy_from_slice(&nan_bytes);

        let result = parse_ac_packet(&data)?;
        assert_eq!(result.speed_ms, 0.0, "NaN speed must default to 0.0");
        assert_eq!(result.rpm, 0.0, "NaN RPM must default to 0.0");
        assert_eq!(result.throttle, 0.0, "NaN throttle must default to 0.0");
        assert_eq!(result.brake, 0.0, "NaN brake must default to 0.0");
        assert_eq!(result.clutch, 0.0, "NaN clutch must default to 0.0");
        assert_eq!(result.steering_angle, 0.0, "NaN steer must default to 0.0");
        Ok(())
    }

    #[test]
    fn test_infinity_in_float_fields_defaults_to_zero() -> TestResult {
        let mut data = vec![0u8; AC_RTCARINFO_SIZE];
        let inf_bytes = f32::INFINITY.to_le_bytes();
        let neg_inf_bytes = f32::NEG_INFINITY.to_le_bytes();
        data[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&inf_bytes);
        data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&neg_inf_bytes);
        data[OFF_STEER..OFF_STEER + 4].copy_from_slice(&inf_bytes);

        let result = parse_ac_packet(&data)?;
        assert_eq!(result.speed_ms, 0.0, "Infinity speed must default to 0.0");
        assert_eq!(result.rpm, 0.0, "-Infinity RPM must default to 0.0");
        assert_eq!(
            result.steering_angle, 0.0,
            "Infinity steer must default to 0.0"
        );
        Ok(())
    }

    // ─── All-zeros packet ────────────────────────────────────────────────────

    #[test]
    fn test_all_zeros_packet_no_panic() -> TestResult {
        let data = vec![0u8; AC_RTCARINFO_SIZE];
        let result = parse_ac_packet(&data)?;
        assert_eq!(result.speed_ms, 0.0);
        assert_eq!(result.rpm, 0.0);
        assert_eq!(result.throttle, 0.0);
        assert_eq!(result.brake, 0.0);
        assert_eq!(result.clutch, 0.0);
        assert_eq!(result.steering_angle, 0.0);
        // gear: raw i32 0 → reverse (-1)
        assert_eq!(
            result.gear, -1,
            "all-zeros gear (raw 0) must map to reverse (-1)"
        );
        Ok(())
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_ac_packet_no_panic_on_arbitrary_bytes(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            // Must never panic on arbitrary input.
            let _ = parse_ac_packet(&data);
        }

        #[test]
        fn parse_ac_packet_too_short_always_errors(size in 0usize..AC_RTCARINFO_SIZE) {
            let data = vec![0u8; size];
            prop_assert!(parse_ac_packet(&data).is_err());
        }

        #[test]
        fn parse_ac_packet_speed_always_nonneg(speed_ms in 0.0f32..=100.0f32) {
            let mut data = vec![0u8; AC_RTCARINFO_SIZE];
            data[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&speed_ms.to_le_bytes());
            let t = parse_ac_packet(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(t.speed_ms >= 0.0);
        }

        #[test]
        fn parse_ac_packet_steering_clamped(steer in any::<f32>()) {
            let mut data = vec![0u8; AC_RTCARINFO_SIZE];
            data[OFF_STEER..OFF_STEER + 4].copy_from_slice(&steer.to_le_bytes());
            if let Ok(result) = parse_ac_packet(&data) {
                prop_assert!(result.steering_angle >= -1.0);
                prop_assert!(result.steering_angle <= 1.0);
            }
        }

        #[test]
        fn parse_ac_packet_rpm_nonneg_on_valid_input(rpm in 0.0f32..=20000.0f32) {
            let mut data = vec![0u8; AC_RTCARINFO_SIZE];
            data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
            let result = parse_ac_packet(&data);
            prop_assert!(result.is_ok());
            let t = result.map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(t.rpm >= 0.0);
        }
    }
}
