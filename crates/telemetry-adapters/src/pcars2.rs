//! Project CARS 2 / Project CARS 3 telemetry adapter.
//!
//! Primary: UDP port 5606 using the SMS sTelemetryData packet (538 bytes, mixed types).
//! Secondary (Windows): shared memory (`$pcars2$`) using the same `SharedMemory` struct
//! defined in the SMS SDK (CREST2 `SharedMemory_v6.h`, version 6 for pCars2, version 9 for AMS2).
//!
//! # SDK References
//! - CREST2 shared memory header: <https://github.com/viper4gh/CREST2> (`SharedMemory_v6.h`)
//! - CREST2-AMS2 shared memory header: <https://github.com/viper4gh/CREST2-AMS2> (`SharedMemory_v9.h`)
//! - CrewChief UDP struct: <https://github.com/mrbelowski/CrewChiefV4> (`PCars2/PCars2UDPTelemetryDataStruct.cs`)
//!
//! # Shared memory vs UDP layout
//! The SMS shared memory (`SharedMemory` struct) is a large C struct (several KB) containing
//! participant arrays (`sParticipantsData[64]`), unfiltered inputs, vehicle/event info, timings,
//! flags, car state (floats for brake/throttle/clutch/steering, int for gear), tyre data, damage
//! and weather. Field types and offsets differ from the compact UDP telemetry packet
//! (`sTelemetryData`), which uses packed types (u8, i8, u16 for the same fields). The shared
//! memory file is named `$pcars2$` and opened with `OpenFileMappingA`/`OpenFileMappingW`.
//!
//! The AMS2 adapter (`ams2.rs`) handles the full shared memory struct path.
#![cfg_attr(not(windows), allow(unused, dead_code))]

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, telemetry_now_ns,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Verified: SMS sTelemetryData UDP fallback port (CrewChiefV4 PCars2 docs).
const DEFAULT_PCARS2_PORT: u16 = 5606;
/// Minimum packet size to read all key UDP fields (through sGearNumGears at offset 45).
const PCARS2_UDP_MIN_SIZE: usize = 46;
/// CrewChief: `UDPPacketSizes.telemetryPacketSize = 538`.
const MAX_PACKET_SIZE: usize = 1500;

#[cfg(windows)]
#[allow(dead_code)] // Retained for documentation; shared memory reading is disabled (see below).
const PCARS2_SHARED_MEMORY_NAME: &str = "Local\\$pcars2$";
/// NOTE: The actual SMS `SharedMemory` struct is much larger than 4096 bytes (it contains
/// `sParticipantsData[64]` arrays, car/track strings, etc.). We map 4096 bytes here only for
/// the shared-memory-present probe; the full struct is handled by the AMS2 adapter.
#[cfg(windows)]
const PCARS2_SHARED_MEMORY_SIZE: usize = 4096;

const PCARS2_PROCESS_NAMES: &[&str] = &["pcars2.exe", "pcars3.exe", "projectcars2.exe"];

// ──────────────────────────────────────────────────────────────────────────────
// PCars2/PCars3 UDP sTelemetryData packet offsets (538-byte packet, type 0).
//
// Verified against:
//   CrewChiefV4/PCars2/PCars2UDPTelemetryDataStruct.cs (sTelemetryData struct)
//   CREST2 SharedMemory_v6.h (SharedMemory struct for the shared-memory layout)
//
// The UDP telemetry packet uses compact types (u8, i8, u16) for inputs and RPM,
// while the shared memory struct uses wider types (f32 for inputs, int for gear).
// This parser handles the UDP format only.
//
// Packet header (12 bytes):
//   0: u32  mPacketNumber
//   4: u32  mCategoryPacketNumber
//   8: u8   mPartialPacketIndex
//   9: u8   mPartialPacketNumber
//  10: u8   mPacketType           (0 = telemetry)
//  11: u8   mPacketVersion
//
// Body (selected fields parsed below):
//  12: i8   sViewedParticipantIndex
//  13: u8   sUnfilteredThrottle     [0-255]
//  14: u8   sUnfilteredBrake        [0-255]
//  15: i8   sUnfilteredSteering     [-128..127]
//  16: u8   sUnfilteredClutch       [0-255]
//  17: u8   sCarFlags
//  18: i16  sOilTempCelsius
//  20: u16  sOilPressureKPa
//  22: i16  sWaterTempCelsius
//  24: u16  sWaterPressureKpa
//  26: u16  sFuelPressureKpa
//  28: u8   sFuelCapacity
//  29: u8   sBrake                  [0-255] (filtered)
//  30: u8   sThrottle               [0-255] (filtered)
//  31: u8   sClutch                 [0-255] (filtered)
//  32: f32  sFuelLevel              [0.0-1.0]
//  36: f32  sSpeed                  m/s
//  40: u16  sRpm
//  42: u16  sMaxRpm
//  44: i8   sSteering               [-127..+127] (filtered)
//  45: u8   sGearNumGears           low nibble=gear (15=reverse), high nibble=numGears
//  46: u8   sBoostAmount
//  47: u8   sCrashState
//  48: f32  sOdometerKM
//  ...  (tyre data, motion vectors, damage, compounds continue to byte 537)
//
// Note: Lap time and position data are in the sTimingsData packet (type 3),
// not in the telemetry packet.
// ──────────────────────────────────────────────────────────────────────────────
const OFF_WATER_TEMP: usize = 22; // i16: sWaterTempCelsius — verified SDK offset 22
const OFF_BRAKE: usize = 29; // u8:  sBrake (filtered) — verified SDK offset 29
const OFF_THROTTLE: usize = 30; // u8:  sThrottle (filtered) — verified SDK offset 30
const OFF_CLUTCH: usize = 31; // u8:  sClutch (filtered) — verified SDK offset 31
const OFF_FUEL_LEVEL: usize = 32; // f32: sFuelLevel [0.0-1.0] — verified SDK offset 32
const OFF_SPEED: usize = 36; // f32: sSpeed (m/s) — verified SDK offset 36
const OFF_RPM: usize = 40; // u16: sRpm — verified SDK offset 40
const OFF_MAX_RPM: usize = 42; // u16: sMaxRpm — verified SDK offset 42
const OFF_STEERING: usize = 44; // i8:  sSteering (filtered) — verified SDK offset 44
const OFF_GEAR_NUM_GEARS: usize = 45; // u8:  sGearNumGears — verified SDK offset 45

/// Parse a pCars2/pCars3 UDP sTelemetryData packet into normalized telemetry.
///
/// All byte offsets verified against CrewChiefV4 `PCars2UDPTelemetryDataStruct.cs`:
/// - Filtered inputs at offsets 29–31 (u8 brake/throttle/clutch → /255)
/// - Filtered steering at offset 44 (i8 → /127)
/// - Speed at offset 36 (f32, m/s), RPM at offset 40 (u16), MaxRPM at offset 42 (u16)
/// - Gear+NumGears at offset 45 (u8: low nibble=gear 0–14/15=reverse, high nibble=numGears)
/// - Water temperature at offset 22 (i16, °C)
/// - Fuel level at offset 32 (f32, 0.0–1.0)
pub fn parse_pcars2_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    if data.len() < PCARS2_UDP_MIN_SIZE {
        return Err(anyhow!(
            "PCARS2 packet too short: expected at least {PCARS2_UDP_MIN_SIZE}, got {}",
            data.len()
        ));
    }

    let steering_raw = read_i8(data, OFF_STEERING).unwrap_or(0);
    let steering = (steering_raw as f32 / 127.0).clamp(-1.0, 1.0);

    let throttle_raw = read_u8(data, OFF_THROTTLE).unwrap_or(0);
    let throttle = throttle_raw as f32 / 255.0;

    let brake_raw = read_u8(data, OFF_BRAKE).unwrap_or(0);
    let brake = brake_raw as f32 / 255.0;

    let clutch_raw = read_u8(data, OFF_CLUTCH).unwrap_or(0);
    let clutch = clutch_raw as f32 / 255.0;

    let fuel_level = read_f32_le(data, OFF_FUEL_LEVEL).unwrap_or(0.0);
    let speed_mps = read_f32_le(data, OFF_SPEED).unwrap_or(0.0);

    let rpm = read_u16_le(data, OFF_RPM).unwrap_or(0) as f32;
    let max_rpm = read_u16_le(data, OFF_MAX_RPM).unwrap_or(0) as f32;

    let gear_byte = read_u8(data, OFF_GEAR_NUM_GEARS).unwrap_or(0);
    let gear_nibble = gear_byte & 0x0F;
    // Verified: CrewChief does `sGearNumGears & 15` for gear, `sGearNumGears >> 4` for numGears.
    // Low nibble: 0=neutral, 1-14=forward gears, 15=reverse
    let gear: i8 = if gear_nibble == 15 {
        -1
    } else {
        gear_nibble as i8
    };
    let num_gears = gear_byte >> 4;

    let water_temp = read_i16_le(data, OFF_WATER_TEMP).unwrap_or(0) as f32;

    Ok(NormalizedTelemetry::builder()
        .steering_angle(steering)
        .throttle(throttle)
        .brake(brake)
        .clutch(clutch)
        .speed_ms(speed_mps)
        .rpm(rpm)
        .max_rpm(max_rpm)
        .gear(gear)
        .num_gears(num_gears)
        .fuel_percent(fuel_level)
        .engine_temp_c(water_temp)
        .build())
}

/// Project CARS 2 / Project CARS 3 telemetry adapter.
pub struct PCars2Adapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for PCars2Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PCars2Adapter {
    pub fn new() -> Self {
        Self {
            bind_port: DEFAULT_PCARS2_PORT,
            update_rate: Duration::from_millis(10),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for PCars2Adapter {
    fn game_id(&self) -> &str {
        "project_cars_2"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let bind_port = self.bind_port;
        let update_rate = self.update_rate;

        tokio::spawn(async move {
            // On Windows, try shared memory first; shared memory is polled per tick.
            #[cfg(windows)]
            if try_read_pcars2_shared_memory().is_some() {
                info!("PCARS2 adapter using shared memory");
                let mut frame_idx = 0u64;
                loop {
                    match try_read_pcars2_shared_memory() {
                        Some(normalized) => {
                            let frame = TelemetryFrame::new(
                                normalized,
                                telemetry_now_ns(),
                                frame_idx,
                                PCARS2_SHARED_MEMORY_SIZE,
                            );
                            if tx.send(frame).await.is_err() {
                                debug!(
                                    "Receiver dropped, stopping PCARS2 shared memory monitoring"
                                );
                                break;
                            }
                            frame_idx = frame_idx.saturating_add(1);
                        }
                        None => {
                            info!("PCARS2 shared memory no longer available");
                            break;
                        }
                    }
                    tokio::time::sleep(update_rate).await;
                }
                return;
            }

            // UDP fallback (non-Windows or shared memory unavailable).
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind PCARS2 UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("PCARS2 adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_idx = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_pcars2_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_idx, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping PCARS2 UDP monitoring");
                                break;
                            }
                            frame_idx = frame_idx.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse PCARS2 UDP packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("PCARS2 UDP receive error: {e}"),
                    Err(_) => debug!("No PCARS2 telemetry data received (timeout)"),
                }
            }
            info!("Stopped PCARS2 telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_pcars2_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_pcars2_process_running())
    }
}

/// Open PCARS2 shared memory, read the simplified packet, and close. Returns None on any failure.
///
/// BUG(known): The `SharedMemory` struct in the SMS SDK has a completely different binary layout
/// from the UDP `sTelemetryData` packet. Fields in shared memory are wider types (f32 for
/// brake/throttle/clutch, int for gear) at different offsets (preceded by participant arrays,
/// unfiltered inputs, vehicle/event info, and timing data). Parsing shared memory bytes with
/// `parse_pcars2_packet` (which uses UDP offsets) produces incorrect values.
///
/// This function is disabled until a proper shared memory struct is implemented. Callers fall
/// through to the working UDP path. See the AMS2 adapter for a struct-based shared memory reader.
#[cfg(windows)]
fn try_read_pcars2_shared_memory() -> Option<NormalizedTelemetry> {
    // Shared memory reading is disabled — the UDP-offset parser cannot be applied to the
    // SharedMemory struct layout. Always return None so callers fall through to the UDP path.
    None
}

#[cfg(windows)]
fn is_pcars2_process_running() -> bool {
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
                if PCARS2_PROCESS_NAMES.iter().any(|p| name.contains(p)) {
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
fn is_pcars2_process_running() -> bool {
    false
}

fn read_f32_le(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(f32::from_le_bytes)
        .filter(|v| v.is_finite())
}

fn read_u16_le(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2)
        .and_then(|b| b.try_into().ok())
        .map(u16::from_le_bytes)
}

fn read_i16_le(data: &[u8], offset: usize) -> Option<i16> {
    data.get(offset..offset + 2)
        .and_then(|b| b.try_into().ok())
        .map(i16::from_le_bytes)
}

fn read_u8(data: &[u8], offset: usize) -> Option<u8> {
    data.get(offset).copied()
}

fn read_i8(data: &[u8], offset: usize) -> Option<i8> {
    data.get(offset).map(|&b| b as i8)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn make_pcars2_packet(
        steering: f32,
        throttle: f32,
        brake: f32,
        speed: f32,
        rpm: f32,
        max_rpm: f32,
        gear: u32,
    ) -> Vec<u8> {
        let mut data = vec![0u8; PCARS2_UDP_MIN_SIZE];
        data[OFF_STEERING] = (steering.clamp(-1.0, 1.0) * 127.0) as i8 as u8;
        data[OFF_THROTTLE] = (throttle.clamp(0.0, 1.0) * 255.0) as u8;
        data[OFF_BRAKE] = (brake.clamp(0.0, 1.0) * 255.0) as u8;
        data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
        data[OFF_RPM..OFF_RPM + 2].copy_from_slice(&(rpm as u16).to_le_bytes());
        data[OFF_MAX_RPM..OFF_MAX_RPM + 2].copy_from_slice(&(max_rpm as u16).to_le_bytes());
        let gear_val: u8 = if gear > 14 { 15 } else { gear as u8 };
        data[OFF_GEAR_NUM_GEARS] = gear_val;
        data
    }

    #[test]
    fn test_parse_valid_packet() -> TestResult {
        let data = make_pcars2_packet(0.3, 0.8, 0.0, 50.0, 5000.0, 8000.0, 3);
        let result = parse_pcars2_packet(&data)?;
        // i8 round-trip: (0.3 * 127) as i8 = 38, 38/127 ≈ 0.2992
        assert!((result.steering_angle - 38.0 / 127.0).abs() < 0.001);
        // u8 round-trip: (0.8 * 255) as u8 = 204, 204/255 = 0.8
        assert!((result.throttle - 204.0 / 255.0).abs() < 0.001);
        assert!((result.speed_ms - 50.0).abs() < 0.01);
        assert!((result.rpm - 5000.0).abs() < 0.01);
        assert_eq!(result.gear, 3);
        Ok(())
    }

    #[test]
    fn test_parse_truncated_packet() {
        let data = vec![0u8; 30];
        assert!(parse_pcars2_packet(&data).is_err());
    }

    #[test]
    fn test_normalization_clamp() -> TestResult {
        let data = make_pcars2_packet(2.0, 1.5, -0.1, 100.0, 7000.0, 8000.0, 4);
        let result = parse_pcars2_packet(&data)?;
        assert!((result.steering_angle - 1.0).abs() < 0.001);
        // Builder clamps throttle to [0,1]; encoding also clamps
        assert!((result.throttle - 1.0).abs() < 0.001);
        // Brake: -0.1 clamped to 0.0 during encoding → u8 0 → 0.0
        assert!((result.brake).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_adapter_game_id() {
        let adapter = PCars2Adapter::new();
        assert_eq!(adapter.game_id(), "project_cars_2");
    }

    #[test]
    fn test_adapter_update_rate() {
        let adapter = PCars2Adapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(10));
    }

    #[test]
    fn test_normalize_delegates_to_parse() -> TestResult {
        let adapter = PCars2Adapter::new();
        let data = make_pcars2_packet(0.0, 0.5, 0.1, 30.0, 3000.0, 7000.0, 2);
        let result = adapter.normalize(&data)?;
        assert!((result.rpm - 3000.0).abs() < 1.0);
        Ok(())
    }

    #[test]
    fn test_parse_empty_packet() {
        assert!(parse_pcars2_packet(&[]).is_err());
    }

    #[cfg(test)]
    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn parse_pcars2_no_panic_on_arbitrary_bytes(
                data in proptest::collection::vec(any::<u8>(), 0..256)
            ) {
                // Must never panic on arbitrary input.
                let _ = parse_pcars2_packet(&data);
            }

            #[test]
            fn short_packet_always_errors(
                data in proptest::collection::vec(any::<u8>(), 0..PCARS2_UDP_MIN_SIZE)
            ) {
                prop_assert!(parse_pcars2_packet(&data).is_err());
            }

            #[test]
            fn valid_packet_speed_nonnegative(
                steering in -1.0f32..=1.0f32,
                throttle in 0.0f32..1.0f32,
                brake in 0.0f32..1.0f32,
                speed in 0.0f32..200.0f32,
                rpm in 0.0f32..12000.0f32,
                max_rpm in 5000.0f32..12000.0f32,
                gear in 0u32..8u32,
            ) {
                let data = make_pcars2_packet(steering, throttle, brake, speed, rpm, max_rpm, gear);
                let result = parse_pcars2_packet(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
                prop_assert!(result.speed_ms >= 0.0, "speed_ms must be non-negative");
            }

            #[test]
            fn valid_packet_steering_clamped(
                steering in -5.0f32..=5.0f32,
                throttle in 0.0f32..1.0f32,
                brake in 0.0f32..1.0f32,
                speed in 0.0f32..200.0f32,
                rpm in 0.0f32..12000.0f32,
                max_rpm in 5000.0f32..12000.0f32,
                gear in 0u32..8u32,
            ) {
                let data = make_pcars2_packet(steering, throttle, brake, speed, rpm, max_rpm, gear);
                let result = parse_pcars2_packet(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
                prop_assert!(
                    result.steering_angle >= -1.0 && result.steering_angle <= 1.0,
                    "steering_angle {} must be in [-1, 1]",
                    result.steering_angle
                );
            }

            #[test]
            fn valid_packet_rpm_nonnegative(
                steering in -1.0f32..=1.0f32,
                throttle in 0.0f32..1.0f32,
                brake in 0.0f32..1.0f32,
                speed in 0.0f32..200.0f32,
                rpm in 0.0f32..12000.0f32,
                max_rpm in 5000.0f32..12000.0f32,
                gear in 0u32..8u32,
            ) {
                let data = make_pcars2_packet(steering, throttle, brake, speed, rpm, max_rpm, gear);
                let result = parse_pcars2_packet(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
                prop_assert!(result.rpm >= 0.0, "rpm {} must be non-negative", result.rpm);
            }

            #[test]
            fn valid_packet_throttle_in_range(
                steering in -1.0f32..=1.0f32,
                throttle in 0.0f32..1.0f32,
                brake in 0.0f32..1.0f32,
                speed in 0.0f32..200.0f32,
                rpm in 0.0f32..12000.0f32,
                max_rpm in 5000.0f32..12000.0f32,
                gear in 0u32..8u32,
            ) {
                let data = make_pcars2_packet(steering, throttle, brake, speed, rpm, max_rpm, gear);
                let result = parse_pcars2_packet(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
                prop_assert!(
                    result.throttle >= 0.0 && result.throttle <= 1.0,
                    "throttle {} must be in [0, 1]",
                    result.throttle
                );
            }

            #[test]
            fn full_size_packet_no_panic(
                data in proptest::collection::vec(any::<u8>(), PCARS2_UDP_MIN_SIZE..=256)
            ) {
                // Must never panic on any full-size input.
                let _ = parse_pcars2_packet(&data);
            }
        }
    }
}
