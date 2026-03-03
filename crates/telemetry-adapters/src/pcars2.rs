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
const OFF_CAR_FLAGS: usize = 17; // u8:  sCarFlags — verified SDK offset 17
const OFF_OIL_TEMP: usize = 18; // i16: sOilTempCelsius — verified SDK offset 18
const OFF_OIL_PRESSURE: usize = 20; // u16: sOilPressureKPa — verified SDK offset 20
const OFF_WATER_TEMP: usize = 22; // i16: sWaterTempCelsius — verified SDK offset 22
const OFF_WATER_PRESSURE: usize = 24; // u16: sWaterPressureKpa — verified SDK offset 24
const OFF_FUEL_PRESSURE: usize = 26; // u16: sFuelPressureKpa — verified SDK offset 26
const OFF_FUEL_CAPACITY: usize = 28; // u8:  sFuelCapacity — verified SDK offset 28
const OFF_BRAKE: usize = 29; // u8:  sBrake (filtered) — verified SDK offset 29
const OFF_THROTTLE: usize = 30; // u8:  sThrottle (filtered) — verified SDK offset 30
const OFF_CLUTCH: usize = 31; // u8:  sClutch (filtered) — verified SDK offset 31
const OFF_FUEL_LEVEL: usize = 32; // f32: sFuelLevel [0.0-1.0] — verified SDK offset 32
const OFF_SPEED: usize = 36; // f32: sSpeed (m/s) — verified SDK offset 36
const OFF_RPM: usize = 40; // u16: sRpm — verified SDK offset 40
const OFF_MAX_RPM: usize = 42; // u16: sMaxRpm — verified SDK offset 42
const OFF_STEERING: usize = 44; // i8:  sSteering (filtered) — verified SDK offset 44
const OFF_GEAR_NUM_GEARS: usize = 45; // u8:  sGearNumGears — verified SDK offset 45
const OFF_BOOST: usize = 46; // u8:  sBoostAmount — verified SDK offset 46
const OFF_CRASH_STATE: usize = 47; // u8:  sCrashState — verified SDK offset 47
const OFF_ODOMETER: usize = 48; // f32: sOdometerKM — verified SDK offset 48

// Motion vectors (3 × f32 arrays, 12 bytes each, starting after sOdometerKM)
const OFF_LOCAL_ACCEL_X: usize = 100; // f32: sLocalAcceleration[0] (lateral, m/s²)
const OFF_LOCAL_ACCEL_Y: usize = 104; // f32: sLocalAcceleration[1] (vertical, m/s²)
const OFF_LOCAL_ACCEL_Z: usize = 108; // f32: sLocalAcceleration[2] (longitudinal, m/s²)

// Tyre data
const OFF_TYRE_TEMP: usize = 176; // u8[4]: sTyreTemp (°C, FL/FR/RL/RR)
const OFF_AIR_PRESSURE: usize = 352; // u16[4]: sAirPressure (kPa)

/// Minimum packet size to read acceleration data (through sLocalAcceleration[2]).
const PCARS2_ACCEL_MIN_SIZE: usize = 112;
/// Minimum packet size to read tyre temperatures (through sTyreTemp[3]).
const PCARS2_TYRE_TEMP_MIN_SIZE: usize = 180;
/// Minimum packet size to read tyre air pressures (through sAirPressure[3]).
const PCARS2_AIR_PRESSURE_MIN_SIZE: usize = 360;

// ──────────────────────────────────────────────────────────────────────────────
// Packet header fields shared by all SMS UDP packet types.
// ──────────────────────────────────────────────────────────────────────────────
const OFF_PACKET_TYPE: usize = 10; // u8: mPacketType (0=telemetry, 3=timings)
/// Packet type for sTimingsData (position, lap, lap times).
pub const PACKET_TYPE_TIMINGS: u8 = 3;

/// Viewed participant index (telemetry packet only, offset 12, i8).
const OFF_VIEWED_PARTICIPANT: usize = 12;

// ──────────────────────────────────────────────────────────────────────────────
// sTimingsData packet (type 3) offsets.
//
// Verified against CrewChiefV4 PCars2/PCars2SharedMemoryStruct.cs and
// PCars2/PCars2UDPTelemetryDataStruct.cs (sTimingsData, sParticipantInfo).
//
// Body (after 12-byte header):
//  12: i8   sNumParticipants
//  13: u32  sParticipantsChangedTimestamp
//  17: f32  sEventTimeRemaining
//  21: f32  sSplitTimeAhead
//  25: f32  sSplitTimeBehind
//  29: sParticipantInfo[32]
//
// sParticipantInfo (28 bytes per entry):
//   0: i16[3] sWorldPosition (6 bytes)
//   6: u16    sCurrentLapDistance
//   8: u8     sRacePosition (lower 7 bits = 1-based position, top bit = active)
//   9: u8     sLapsCompleted
//  10: u8     sCurrentLap (1-based current lap)
//  11: u8     sSector
//  12: f32    sLastSectorTime
//  16: f32    sBestLapTime   (-1.0 = no valid time)
//  20: f32    sLastLapTime   (-1.0 = no valid time)
//  24: f32    sCurrentTime   (current lap elapsed time, seconds)
// ──────────────────────────────────────────────────────────────────────────────
const TIMINGS_OFF_NUM_PARTICIPANTS: usize = 12;
const TIMINGS_OFF_PARTICIPANTS: usize = 29;
const PARTICIPANT_ENTRY_SIZE: usize = 28;
const PART_OFF_RACE_POSITION: usize = 8;
const PART_OFF_CURRENT_LAP: usize = 10;
const PART_OFF_BEST_LAP_TIME: usize = 16;
const PART_OFF_LAST_LAP_TIME: usize = 20;
const PART_OFF_CURRENT_TIME: usize = 24;

/// Minimum size for a timing packet with at least one participant entry.
const PCARS2_TIMINGS_MIN_SIZE: usize = TIMINGS_OFF_PARTICIPANTS + PARTICIPANT_ENTRY_SIZE;

/// Standard gravitational acceleration (m/s²) for converting local acceleration to G-forces.
const G_ACCEL: f32 = 9.80665;
/// Conversion factor: 1 kPa ≈ 0.145038 PSI.
const KPA_TO_PSI: f32 = 0.145_038;

/// Parse a pCars2/pCars3 UDP sTelemetryData packet into normalized telemetry.
///
/// All byte offsets verified against CrewChiefV4 `PCars2UDPTelemetryDataStruct.cs`:
/// - Filtered inputs at offsets 29–31 (u8 brake/throttle/clutch → /255)
/// - Filtered steering at offset 44 (i8 → /127)
/// - Speed at offset 36 (f32, m/s), RPM at offset 40 (u16), MaxRPM at offset 42 (u16)
/// - Gear+NumGears at offset 45 (u8: low nibble=gear 0–14/15=reverse, high nibble=numGears)
/// - Water temperature at offset 22 (i16, °C)
/// - Fuel level at offset 32 (f32, 0.0–1.0)
/// - Car flags at offset 17 (u8, bit flags for speed limiter, ABS, handbrake, etc.)
/// - Local acceleration at offsets 100–111 (f32[3], m/s² → G-forces)
/// - Tyre temperatures at offset 176 (u8[4], °C)
/// - Tyre air pressures at offset 352 (u16[4], kPa → PSI)
///
/// Fields beyond offset 45 are extracted only when the packet is large enough.
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

    // sCarFlags: bit 3 = speed limiter (pit limiter), bit 4 = ABS active.
    let car_flags = read_u8(data, OFF_CAR_FLAGS).unwrap_or(0);
    let flags = TelemetryFlags {
        pit_limiter: car_flags & 0x08 != 0,
        abs_active: car_flags & 0x10 != 0,
        ..TelemetryFlags::default()
    };

    let mut builder = NormalizedTelemetry::builder()
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
        .flags(flags);

    // G-forces from local acceleration (m/s² → G).
    if data.len() >= PCARS2_ACCEL_MIN_SIZE {
        if let Some(ax) = read_f32_le(data, OFF_LOCAL_ACCEL_X) {
            builder = builder.lateral_g(ax / G_ACCEL);
        }
        if let Some(ay) = read_f32_le(data, OFF_LOCAL_ACCEL_Y) {
            builder = builder.vertical_g(ay / G_ACCEL);
        }
        if let Some(az) = read_f32_le(data, OFF_LOCAL_ACCEL_Z) {
            builder = builder.longitudinal_g(az / G_ACCEL);
        }
    }

    // Tyre temperatures (u8 °C, FL/FR/RL/RR).
    if data.len() >= PCARS2_TYRE_TEMP_MIN_SIZE {
        let temps = [
            read_u8(data, OFF_TYRE_TEMP).unwrap_or(0),
            read_u8(data, OFF_TYRE_TEMP + 1).unwrap_or(0),
            read_u8(data, OFF_TYRE_TEMP + 2).unwrap_or(0),
            read_u8(data, OFF_TYRE_TEMP + 3).unwrap_or(0),
        ];
        builder = builder.tire_temps_c(temps);
    }

    // Tyre air pressures (u16 kPa → PSI, FL/FR/RL/RR).
    if data.len() >= PCARS2_AIR_PRESSURE_MIN_SIZE {
        let pressures = [
            read_u16_le(data, OFF_AIR_PRESSURE).unwrap_or(0) as f32 * KPA_TO_PSI,
            read_u16_le(data, OFF_AIR_PRESSURE + 2).unwrap_or(0) as f32 * KPA_TO_PSI,
            read_u16_le(data, OFF_AIR_PRESSURE + 4).unwrap_or(0) as f32 * KPA_TO_PSI,
            read_u16_le(data, OFF_AIR_PRESSURE + 6).unwrap_or(0) as f32 * KPA_TO_PSI,
        ];
        builder = builder.tire_pressures_psi(pressures);
    }

    // Extended fields: oil temp, pressures, boost, crash state, odometer.
    let oil_temp = read_i16_le(data, OFF_OIL_TEMP).unwrap_or(0);
    builder = builder.extended("oil_temp_c", TelemetryValue::Integer(oil_temp as i32));

    let oil_pressure = read_u16_le(data, OFF_OIL_PRESSURE).unwrap_or(0);
    builder = builder.extended(
        "oil_pressure_kpa",
        TelemetryValue::Integer(oil_pressure as i32),
    );

    let water_pressure = read_u16_le(data, OFF_WATER_PRESSURE).unwrap_or(0);
    builder = builder.extended(
        "water_pressure_kpa",
        TelemetryValue::Integer(water_pressure as i32),
    );

    let fuel_pressure = read_u16_le(data, OFF_FUEL_PRESSURE).unwrap_or(0);
    builder = builder.extended(
        "fuel_pressure_kpa",
        TelemetryValue::Integer(fuel_pressure as i32),
    );

    let fuel_capacity = read_u8(data, OFF_FUEL_CAPACITY).unwrap_or(0);
    builder = builder.extended(
        "fuel_capacity",
        TelemetryValue::Integer(fuel_capacity as i32),
    );

    let boost = read_u8(data, OFF_BOOST).unwrap_or(0);
    builder = builder.extended("boost_amount", TelemetryValue::Integer(boost as i32));

    let crash_state = read_u8(data, OFF_CRASH_STATE).unwrap_or(0);
    builder = builder.extended("crash_state", TelemetryValue::Integer(crash_state as i32));

    if let Some(odometer) = read_f32_le(data, OFF_ODOMETER) {
        builder = builder.extended("odometer_km", TelemetryValue::Float(odometer));
    }

    Ok(builder.build())
}

/// Returns the SMS packet type from the header, or `None` if the packet is too short.
pub fn pcars2_packet_type(data: &[u8]) -> Option<u8> {
    read_u8(data, OFF_PACKET_TYPE)
}

/// Parse timing data from a pCars2/pCars3 sTimingsData packet (type 3).
///
/// Extracts position, lap, and lap time fields for the specified `participant_idx`.
/// Returns a [`NormalizedTelemetry`] containing only timing-related fields; callers
/// should merge these into a full telemetry frame via [`merge_timing_fields`].
///
/// The SMS protocol uses `-1.0` for invalid / no-data lap times; such values are
/// left at the `NormalizedTelemetry` default of `0.0`.
pub fn parse_pcars2_timings_packet(
    data: &[u8],
    participant_idx: u8,
) -> Result<NormalizedTelemetry> {
    if data.len() < PCARS2_TIMINGS_MIN_SIZE {
        return Err(anyhow!(
            "PCARS2 timings packet too short: expected at least {PCARS2_TIMINGS_MIN_SIZE}, got {}",
            data.len()
        ));
    }

    let num_participants = read_i8(data, TIMINGS_OFF_NUM_PARTICIPANTS).unwrap_or(0);
    let idx = if (participant_idx as i8) >= num_participants || num_participants <= 0 {
        0usize
    } else {
        participant_idx as usize
    };

    let base = TIMINGS_OFF_PARTICIPANTS + idx * PARTICIPANT_ENTRY_SIZE;
    if data.len() < base + PARTICIPANT_ENTRY_SIZE {
        return Err(anyhow!(
            "PCARS2 timings packet too short for participant {idx}"
        ));
    }

    let race_position_raw = read_u8(data, base + PART_OFF_RACE_POSITION).unwrap_or(0);
    let position = race_position_raw & 0x7F; // lower 7 bits

    let current_lap = read_u8(data, base + PART_OFF_CURRENT_LAP).unwrap_or(0);

    let best_lap_time = read_f32_le(data, base + PART_OFF_BEST_LAP_TIME).unwrap_or(-1.0);
    let last_lap_time = read_f32_le(data, base + PART_OFF_LAST_LAP_TIME).unwrap_or(-1.0);
    let current_time = read_f32_le(data, base + PART_OFF_CURRENT_TIME).unwrap_or(-1.0);

    let mut builder = NormalizedTelemetry::builder()
        .position(position)
        .lap(current_lap as u16);

    if current_time >= 0.0 {
        builder = builder.current_lap_time_s(current_time);
    }
    if best_lap_time >= 0.0 {
        builder = builder.best_lap_time_s(best_lap_time);
    }
    if last_lap_time >= 0.0 {
        builder = builder.last_lap_time_s(last_lap_time);
    }

    Ok(builder.build())
}

/// Merge timing fields from a previously parsed sTimingsData packet into a
/// telemetry frame. Non-default values in `timing` overwrite `telemetry`.
pub fn merge_timing_fields(telemetry: &mut NormalizedTelemetry, timing: &NormalizedTelemetry) {
    if timing.position > 0 {
        telemetry.position = timing.position;
    }
    if timing.lap > 0 {
        telemetry.lap = timing.lap;
    }
    if timing.current_lap_time_s > 0.0 {
        telemetry.current_lap_time_s = timing.current_lap_time_s;
    }
    if timing.best_lap_time_s > 0.0 {
        telemetry.best_lap_time_s = timing.best_lap_time_s;
    }
    if timing.last_lap_time_s > 0.0 {
        telemetry.last_lap_time_s = timing.last_lap_time_s;
    }
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
            let mut last_timing = NormalizedTelemetry::default();
            let mut viewed_participant: u8 = 0;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => {
                        let pkt = &buf[..len];
                        match pcars2_packet_type(pkt) {
                            Some(PACKET_TYPE_TIMINGS) => {
                                match parse_pcars2_timings_packet(pkt, viewed_participant) {
                                    Ok(timing) => last_timing = timing,
                                    Err(e) => {
                                        debug!("Failed to parse PCARS2 timing packet: {e}")
                                    }
                                }
                            }
                            _ => {
                                if let Some(vp) = read_i8(pkt, OFF_VIEWED_PARTICIPANT)
                                    && vp >= 0
                                {
                                    viewed_participant = vp as u8;
                                }
                                match parse_pcars2_packet(pkt) {
                                    Ok(mut normalized) => {
                                        merge_timing_fields(&mut normalized, &last_timing);
                                        let frame = TelemetryFrame::new(
                                            normalized,
                                            telemetry_now_ns(),
                                            frame_idx,
                                            len,
                                        );
                                        if tx.send(frame).await.is_err() {
                                            debug!(
                                                "Receiver dropped, stopping PCARS2 UDP monitoring"
                                            );
                                            break;
                                        }
                                        frame_idx = frame_idx.saturating_add(1);
                                    }
                                    Err(e) => debug!("Failed to parse PCARS2 UDP packet: {e}"),
                                }
                            }
                        }
                    }
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
        match pcars2_packet_type(raw) {
            Some(PACKET_TYPE_TIMINGS) => parse_pcars2_timings_packet(raw, 0),
            _ => parse_pcars2_packet(raw),
        }
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

    /// Build a full-size (538-byte) packet with acceleration, tyre, and pressure data.
    fn make_full_pcars2_packet(
        steering: f32,
        throttle: f32,
        brake: f32,
        speed: f32,
        rpm: f32,
        max_rpm: f32,
        gear: u32,
    ) -> Vec<u8> {
        let mut data = vec![0u8; MAX_PACKET_SIZE.min(538)];
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
    fn test_parse_gforces() -> TestResult {
        let mut data = make_full_pcars2_packet(0.0, 0.5, 0.0, 30.0, 3000.0, 7000.0, 2);
        // Write lateral accel = 9.80665 m/s² (1G), longitudinal = 4.903325 m/s² (0.5G)
        let lat_accel: f32 = 9.80665;
        let vert_accel: f32 = -9.80665;
        let long_accel: f32 = 4.903_325;
        data[OFF_LOCAL_ACCEL_X..OFF_LOCAL_ACCEL_X + 4].copy_from_slice(&lat_accel.to_le_bytes());
        data[OFF_LOCAL_ACCEL_Y..OFF_LOCAL_ACCEL_Y + 4].copy_from_slice(&vert_accel.to_le_bytes());
        data[OFF_LOCAL_ACCEL_Z..OFF_LOCAL_ACCEL_Z + 4].copy_from_slice(&long_accel.to_le_bytes());

        let result = parse_pcars2_packet(&data)?;
        assert!((result.lateral_g - 1.0).abs() < 0.001);
        assert!((result.vertical_g - (-1.0)).abs() < 0.001);
        assert!((result.longitudinal_g - 0.5).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_parse_tyre_temps() -> TestResult {
        let mut data = make_full_pcars2_packet(0.0, 0.5, 0.0, 30.0, 3000.0, 7000.0, 2);
        data[OFF_TYRE_TEMP] = 85;
        data[OFF_TYRE_TEMP + 1] = 90;
        data[OFF_TYRE_TEMP + 2] = 80;
        data[OFF_TYRE_TEMP + 3] = 88;

        let result = parse_pcars2_packet(&data)?;
        assert_eq!(result.tire_temps_c, [85, 90, 80, 88]);
        Ok(())
    }

    #[test]
    fn test_parse_tyre_pressures() -> TestResult {
        let mut data = make_full_pcars2_packet(0.0, 0.5, 0.0, 30.0, 3000.0, 7000.0, 2);
        // 200 kPa ≈ 29.0 PSI
        let pressure_kpa: u16 = 200;
        for i in 0..4 {
            data[OFF_AIR_PRESSURE + i * 2..OFF_AIR_PRESSURE + i * 2 + 2]
                .copy_from_slice(&pressure_kpa.to_le_bytes());
        }

        let result = parse_pcars2_packet(&data)?;
        let expected_psi = 200.0 * KPA_TO_PSI;
        for &p in &result.tire_pressures_psi {
            assert!((p - expected_psi).abs() < 0.01);
        }
        Ok(())
    }

    #[test]
    fn test_parse_car_flags() -> TestResult {
        let mut data = make_pcars2_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0);
        // Bit 3 = speed limiter (pit_limiter), bit 4 = ABS
        data[OFF_CAR_FLAGS] = 0x18; // both pit_limiter and ABS
        let result = parse_pcars2_packet(&data)?;
        assert!(result.flags.pit_limiter);
        assert!(result.flags.abs_active);
        Ok(())
    }

    #[test]
    fn test_parse_car_flags_none_set() -> TestResult {
        let mut data = make_pcars2_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0);
        data[OFF_CAR_FLAGS] = 0x00;
        let result = parse_pcars2_packet(&data)?;
        assert!(!result.flags.pit_limiter);
        assert!(!result.flags.abs_active);
        Ok(())
    }

    #[test]
    fn test_parse_extended_fields() -> TestResult {
        let mut data = make_full_pcars2_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0);
        // Oil temp = 110°C
        data[OFF_OIL_TEMP..OFF_OIL_TEMP + 2].copy_from_slice(&110i16.to_le_bytes());
        // Boost = 50
        data[OFF_BOOST] = 50;
        // Crash state = 2
        data[OFF_CRASH_STATE] = 2;

        let result = parse_pcars2_packet(&data)?;
        assert_eq!(
            result.get_extended("oil_temp_c"),
            Some(&TelemetryValue::Integer(110))
        );
        assert_eq!(
            result.get_extended("boost_amount"),
            Some(&TelemetryValue::Integer(50))
        );
        assert_eq!(
            result.get_extended("crash_state"),
            Some(&TelemetryValue::Integer(2))
        );
        Ok(())
    }

    #[test]
    fn test_min_packet_skips_extended_tyre_data() -> TestResult {
        // Minimum-size packet should parse successfully but have no tyre temps or pressures.
        let data = make_pcars2_packet(0.0, 0.5, 0.0, 30.0, 3000.0, 7000.0, 2);
        assert_eq!(data.len(), PCARS2_UDP_MIN_SIZE);
        let result = parse_pcars2_packet(&data)?;
        assert_eq!(result.tire_temps_c, [0, 0, 0, 0]);
        assert_eq!(result.tire_pressures_psi, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(result.lateral_g, 0.0);
        assert_eq!(result.longitudinal_g, 0.0);
        assert_eq!(result.vertical_g, 0.0);
        Ok(())
    }

    // ── Timing packet tests ─────────────────────────────────────────────────

    /// Build a minimal sTimingsData packet (type 3) with one participant entry.
    fn make_timings_packet(
        position: u8,
        current_lap: u8,
        best_lap_time: f32,
        last_lap_time: f32,
        current_time: f32,
    ) -> Vec<u8> {
        let mut data = vec![0u8; PCARS2_TIMINGS_MIN_SIZE];
        // Header: packet type = 3 (timings)
        data[OFF_PACKET_TYPE] = PACKET_TYPE_TIMINGS;
        // Body: 1 participant
        data[TIMINGS_OFF_NUM_PARTICIPANTS] = 1u8;
        // Participant 0
        let base = TIMINGS_OFF_PARTICIPANTS;
        data[base + PART_OFF_RACE_POSITION] = position;
        data[base + PART_OFF_CURRENT_LAP] = current_lap;
        data[base + PART_OFF_BEST_LAP_TIME..base + PART_OFF_BEST_LAP_TIME + 4]
            .copy_from_slice(&best_lap_time.to_le_bytes());
        data[base + PART_OFF_LAST_LAP_TIME..base + PART_OFF_LAST_LAP_TIME + 4]
            .copy_from_slice(&last_lap_time.to_le_bytes());
        data[base + PART_OFF_CURRENT_TIME..base + PART_OFF_CURRENT_TIME + 4]
            .copy_from_slice(&current_time.to_le_bytes());
        data
    }

    #[test]
    fn test_parse_timings_valid() -> TestResult {
        let data = make_timings_packet(3, 5, 62.5, 63.1, 30.0);
        let result = parse_pcars2_timings_packet(&data, 0)?;
        assert_eq!(result.position, 3);
        assert_eq!(result.lap, 5);
        assert!((result.best_lap_time_s - 62.5).abs() < 0.01);
        assert!((result.last_lap_time_s - 63.1).abs() < 0.01);
        assert!((result.current_lap_time_s - 30.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_parse_timings_invalid_times() -> TestResult {
        let data = make_timings_packet(1, 1, -1.0, -1.0, -1.0);
        let result = parse_pcars2_timings_packet(&data, 0)?;
        assert_eq!(result.position, 1);
        assert_eq!(result.lap, 1);
        // -1.0 means no data; builder leaves at default 0.0
        assert_eq!(result.best_lap_time_s, 0.0);
        assert_eq!(result.last_lap_time_s, 0.0);
        assert_eq!(result.current_lap_time_s, 0.0);
        Ok(())
    }

    #[test]
    fn test_parse_timings_too_short() {
        let data = vec![0u8; 30];
        assert!(parse_pcars2_timings_packet(&data, 0).is_err());
    }

    #[test]
    fn test_parse_timings_position_top_bit_masked() -> TestResult {
        // Top bit set (0x80 | 5 = 0x85) → position should be 5
        let mut data = make_timings_packet(5, 2, 60.0, 61.0, 10.0);
        let base = TIMINGS_OFF_PARTICIPANTS;
        data[base + PART_OFF_RACE_POSITION] = 0x85;
        let result = parse_pcars2_timings_packet(&data, 0)?;
        assert_eq!(result.position, 5);
        Ok(())
    }

    #[test]
    fn test_merge_timing_fields() -> TestResult {
        let mut telemetry =
            parse_pcars2_packet(&make_pcars2_packet(0.0, 0.5, 0.0, 30.0, 3000.0, 7000.0, 2))?;
        let timing = parse_pcars2_timings_packet(&make_timings_packet(2, 4, 65.0, 66.0, 20.0), 0)?;
        merge_timing_fields(&mut telemetry, &timing);
        assert_eq!(telemetry.position, 2);
        assert_eq!(telemetry.lap, 4);
        assert!((telemetry.current_lap_time_s - 20.0).abs() < 0.01);
        assert!((telemetry.best_lap_time_s - 65.0).abs() < 0.01);
        assert!((telemetry.last_lap_time_s - 66.0).abs() < 0.01);
        // Original telemetry fields preserved
        assert!((telemetry.speed_ms - 30.0).abs() < 0.01);
        assert!((telemetry.rpm - 3000.0).abs() < 1.0);
        Ok(())
    }

    #[test]
    fn test_packet_type_detection() {
        let mut telem_pkt = make_pcars2_packet(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0);
        telem_pkt[OFF_PACKET_TYPE] = 0; // telemetry
        assert_eq!(pcars2_packet_type(&telem_pkt), Some(0));

        let timing_pkt = make_timings_packet(1, 1, 60.0, 61.0, 10.0);
        assert_eq!(pcars2_packet_type(&timing_pkt), Some(PACKET_TYPE_TIMINGS));

        assert_eq!(pcars2_packet_type(&[]), None);
    }

    #[test]
    fn test_normalize_dispatches_timing_packet() -> TestResult {
        let adapter = PCars2Adapter::new();
        let data = make_timings_packet(3, 5, 62.5, 63.1, 30.0);
        let result = adapter.normalize(&data)?;
        assert_eq!(result.position, 3);
        assert_eq!(result.lap, 5);
        Ok(())
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
