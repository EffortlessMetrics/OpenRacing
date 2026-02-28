//! F1 2023/2024 native UDP telemetry adapter.
//!
//! Parses the EA F1 2023 (packet format `2023`) and F1 2024 (packet format `2024`)
//! binary UDP protocols directly.  No bridge layer or XML spec file is required.
//!
//! ## Supported packet types
//!
//! | Packet ID | Name           | Fields used                                |
//! |-----------|----------------|--------------------------------------------|
//! | 1         | Session        | track ID, session type, temperatures       |
//! | 6         | Car Telemetry  | speed, gear, RPM, DRS, tyre temps/pressure |
//! | 7         | Car Status     | fuel, ERS, pit limiter, tyre compound      |
//!
//! All other packet IDs are silently discarded.
//!
//! ## Packet format differences
//!
//! - F1 23 (format `2023`): CarStatusData is **47 bytes** per car; no engine-power fields.
//! - F1 24 (format `2024`): CarStatusData is **55 bytes** per car; adds `enginePowerICE`
//!   and `enginePowerMGUK` before the ERS block.
//! - CarTelemetryData is 60 bytes per car in both versions, identical to F1 25.
//!
//! ## Default UDP port
//! `20777` (override with `OPENRACING_F1_NATIVE_UDP_PORT`).
//!
//! ## Unit conventions
//! - Speed: km/h → m/s (÷ 3.6)
//! - Tyre pressure: PSI (as reported by the game)
//! - ERS store energy: Joules
//! - Fuel remaining: kg
//! - Temperatures: °C

use crate::f1_25::{
    ByteReader, CAR_TELEMETRY_ENTRY_SIZE, ERS_MAX_STORE_ENERGY_J, SessionData, parse_car_telemetry,
    parse_header, parse_session_data, track_name_from_id, tyre_compound_name,
};
use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFlags, TelemetryFrame, TelemetryReceiver,
    TelemetryValue, telemetry_now_ns,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

// ── Constants ─────────────────────────────────────────────────────────────────

const DEFAULT_PORT: u16 = 20777;
const DEFAULT_HEARTBEAT_TIMEOUT_MS: u64 = 2_000;
const MAX_PACKET_BYTES: usize = 2048;

// These match the f1_25 constants (same protocol family); redeclared here
// because the originals are module-private in f1_25.
const HEADER_SIZE: usize = 29;
const NUM_CARS: usize = 22;
const PACKET_ID_SESSION: u8 = 1;
const PACKET_ID_CAR_TELEMETRY: u8 = 6;
const PACKET_ID_CAR_STATUS: u8 = 7;

/// F1 23 packet format discriminator value.
pub const PACKET_FORMAT_2023: u16 = 2023;
/// F1 24 packet format discriminator value.
pub const PACKET_FORMAT_2024: u16 = 2024;

/// F1 23 CarStatusData entry size (47 bytes per car, no engine-power fields).
pub const CAR_STATUS_2023_ENTRY_SIZE: usize = 47;
/// F1 24 CarStatusData entry size (55 bytes per car, adds enginePowerICE/MGUK).
pub const CAR_STATUS_2024_ENTRY_SIZE: usize = 55;

/// Minimum valid Car Status packet size for F1 23 (all 22 cars).
pub const MIN_CAR_STATUS_2023_PACKET_SIZE: usize =
    HEADER_SIZE + NUM_CARS * CAR_STATUS_2023_ENTRY_SIZE;
/// Minimum valid Car Status packet size for F1 24 (all 22 cars).
pub const MIN_CAR_STATUS_2024_PACKET_SIZE: usize =
    HEADER_SIZE + NUM_CARS * CAR_STATUS_2024_ENTRY_SIZE;

const ENV_PORT: &str = "OPENRACING_F1_NATIVE_UDP_PORT";
const ENV_HEARTBEAT_MS: &str = "OPENRACING_F1_NATIVE_HEARTBEAT_TIMEOUT_MS";

// ── Parsed packet structs ─────────────────────────────────────────────────────

/// Car status data, unified across F1 23 and F1 24.
///
/// For F1 23, `engine_power_ice` and `engine_power_mguk` are always `0.0`
/// because those fields were not present in the F1 23 spec.
#[derive(Debug, Clone, Default)]
pub struct F1NativeCarStatusData {
    pub traction_control: u8,
    pub anti_lock_brakes: u8,
    pub pit_limiter_status: u8,
    pub fuel_in_tank: f32,
    pub fuel_remaining_laps: f32,
    pub max_rpm: u16,
    pub drs_allowed: u8,
    pub actual_tyre_compound: u8,
    pub tyre_age_laps: u8,
    /// ICE power in Watts.  `0.0` for F1 23.
    pub engine_power_ice: f32,
    /// MGU-K power in Watts.  `0.0` for F1 23.
    pub engine_power_mguk: f32,
    pub ers_store_energy: f32,
    pub ers_deploy_mode: u8,
    pub ers_harvested_mguk: f32,
    pub ers_harvested_mguh: f32,
    pub ers_deployed: f32,
}

/// Combined mutable state accumulated across successive UDP packets.
#[derive(Debug, Default)]
pub struct F1NativeState {
    pub latest_telemetry: Option<crate::f1_25::CarTelemetryData>,
    pub latest_status: Option<F1NativeCarStatusData>,
    pub session: SessionData,
}

// ── Adapter struct ────────────────────────────────────────────────────────────

/// Native F1 2023/2024 UDP telemetry adapter.
///
/// Listens on a UDP socket for EA F1 23 or F1 24 binary packets and emits
/// [`NormalizedTelemetry`] frames once both a Car Telemetry (ID 6) and a Car
/// Status (ID 7) packet have been received for the player's car.
///
/// Packet format (`2023` vs `2024`) is detected automatically from each
/// packet header; mixed-format sessions are not expected in practice but are
/// handled gracefully.
#[derive(Clone)]
pub struct F1NativeAdapter {
    bind_port: u16,
    update_rate: Duration,
    heartbeat_timeout: Duration,
    last_packet_ns: Arc<AtomicU64>,
}

impl Default for F1NativeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl F1NativeAdapter {
    /// Create a new adapter, reading configuration from environment variables.
    pub fn new() -> Self {
        let bind_port = env_u16(ENV_PORT, DEFAULT_PORT);
        let heartbeat_ms = env_u64(ENV_HEARTBEAT_MS, DEFAULT_HEARTBEAT_TIMEOUT_MS);
        Self {
            bind_port,
            update_rate: Duration::from_millis(16),
            heartbeat_timeout: Duration::from_millis(heartbeat_ms),
            last_packet_ns: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Override the UDP bind port (useful in tests).
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

    /// Decode one raw UDP payload, updating `state`.
    ///
    /// Returns `Ok(Some(NormalizedTelemetry))` when both Car Telemetry and Car
    /// Status data are available after this packet.  Returns `Ok(None)` for
    /// Session updates or incomplete state.  Returns `Err` only for malformed
    /// packets or unsupported packet formats.
    pub fn process_packet(
        state: &mut F1NativeState,
        raw: &[u8],
    ) -> Result<Option<NormalizedTelemetry>> {
        let header = parse_header(raw)?;
        match header.packet_format {
            PACKET_FORMAT_2023 | PACKET_FORMAT_2024 => {}
            other => {
                return Err(anyhow!(
                    "F1 native: unexpected packet format {} (expected 2023 or 2024)",
                    other
                ));
            }
        }

        let player = usize::from(header.player_car_index);
        match header.packet_id {
            PACKET_ID_SESSION => {
                state.session = parse_session_data(raw)?;
                Ok(None)
            }
            PACKET_ID_CAR_TELEMETRY => {
                let telem = parse_car_telemetry(raw, player)?;
                state.latest_telemetry = Some(telem);
                Ok(Self::maybe_emit(state))
            }
            PACKET_ID_CAR_STATUS => {
                let status = match header.packet_format {
                    PACKET_FORMAT_2023 => parse_car_status_2023(raw, player)?,
                    PACKET_FORMAT_2024 => parse_car_status_2024(raw, player)?,
                    _ => unreachable!("packet_format already validated above"),
                };
                state.latest_status = Some(status);
                Ok(Self::maybe_emit(state))
            }
            other => {
                debug!(
                    packet_id = other,
                    "F1 native: ignoring unrecognised packet id"
                );
                Ok(None)
            }
        }
    }

    fn maybe_emit(state: &F1NativeState) -> Option<NormalizedTelemetry> {
        match (&state.latest_telemetry, &state.latest_status) {
            (Some(t), Some(s)) => Some(normalize(t, s, &state.session)),
            _ => None,
        }
    }
}

// ── TelemetryAdapter impl ─────────────────────────────────────────────────────

#[async_trait]
impl TelemetryAdapter for F1NativeAdapter {
    fn game_id(&self) -> &str {
        "f1_native"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let bind_port = self.bind_port;
        let update_rate = self.update_rate;
        let last_packet_ns = Arc::clone(&self.last_packet_ns);
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port));
            let socket = match UdpSocket::bind(addr).await {
                Ok(s) => s,
                Err(err) => {
                    warn!(error = %err, port = bind_port, "F1 native UDP socket bind failed");
                    return;
                }
            };
            info!(
                port = bind_port,
                "F1 native UDP adapter bound (formats 2023/2024)"
            );

            let mut state = F1NativeState::default();
            let mut frame_seq = 0u64;
            let mut buf = vec![0u8; MAX_PACKET_BYTES];
            let timeout = update_rate * 4;

            loop {
                let recv_result = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
                let len = match recv_result {
                    Ok(Ok(n)) => n,
                    Ok(Err(err)) => {
                        warn!(error = %err, "F1 native UDP receive error");
                        continue;
                    }
                    Err(_) => {
                        debug!("F1 native UDP receive timeout");
                        continue;
                    }
                };

                last_packet_ns.store(telemetry_now_ns(), Ordering::Relaxed);

                match Self::process_packet(&mut state, &buf[..len]) {
                    Ok(Some(normalized)) => {
                        let ts = telemetry_now_ns();
                        let frame = TelemetryFrame::new(normalized, ts, frame_seq, len);
                        if tx.send(frame).await.is_err() {
                            break;
                        }
                        frame_seq = frame_seq.saturating_add(1);
                    }
                    Ok(None) => {}
                    Err(err) => {
                        warn!(error = %err, len, "F1 native packet decode failed");
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    /// Normalise a single raw Car Telemetry (ID 6) packet.
    ///
    /// Accepts formats `2023` or `2024` and produces normalised output using
    /// default CarStatus values.  Returns an error for non-telemetry packets or
    /// unsupported formats.
    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        let header = parse_header(raw)?;
        match header.packet_format {
            PACKET_FORMAT_2023 | PACKET_FORMAT_2024 => {}
            other => {
                return Err(anyhow!(
                    "F1 native normalize(): unexpected packet format {} (expected 2023 or 2024)",
                    other
                ));
            }
        }

        let player = usize::from(header.player_car_index);
        match header.packet_id {
            PACKET_ID_CAR_TELEMETRY => {
                let telem = parse_car_telemetry(raw, player)?;
                let status = F1NativeCarStatusData::default();
                Ok(normalize(&telem, &status, &SessionData::default()))
            }
            PACKET_ID_CAR_STATUS => {
                // Validate only; cannot produce speed/gear without telemetry.
                match header.packet_format {
                    PACKET_FORMAT_2023 => {
                        let _ = parse_car_status_2023(raw, player)?;
                    }
                    PACKET_FORMAT_2024 => {
                        let _ = parse_car_status_2024(raw, player)?;
                    }
                    _ => unreachable!(),
                }
                Err(anyhow!(
                    "F1 native normalize() received CarStatus (ID 7) without preceding \
                     CarTelemetry; use process_packet() with F1NativeState for multi-packet \
                     normalisation"
                ))
            }
            PACKET_ID_SESSION => {
                let _ = parse_session_data(raw)?;
                Err(anyhow!(
                    "F1 native normalize() received Session (ID 1); not a complete telemetry packet"
                ))
            }
            other => Err(anyhow!(
                "F1 native normalize(): unsupported packet id {}",
                other
            )),
        }
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.is_recent_packet())
    }
}

// ── Binary parsing ────────────────────────────────────────────────────────────

/// Parse the CarStatusData entry for `player_index` from an F1 23 Car Status packet.
///
/// F1 23 CarStatusData layout (47 bytes per car):
/// - bytes 0-28: traction/ABS/fuelMix/bias/pitLimiter/fuel/RPM/DRS/tyres/FIA flags (shared)
/// - bytes 29-32: ersStoreEnergy (f32)
/// - byte 33: ersDeployMode
/// - bytes 34-37: ersHarvestedMGUK (f32)
/// - bytes 38-41: ersHarvestedMGUH (f32)
/// - bytes 42-45: ersDeployedThisLap (f32)
/// - byte 46: networkPaused (ignored)
pub fn parse_car_status_2023(raw: &[u8], player_index: usize) -> Result<F1NativeCarStatusData> {
    if raw.len() < MIN_CAR_STATUS_2023_PACKET_SIZE {
        return Err(anyhow!(
            "F1 23 CarStatus packet too short: {} bytes (need {})",
            raw.len(),
            MIN_CAR_STATUS_2023_PACKET_SIZE
        ));
    }
    if player_index >= NUM_CARS {
        return Err(anyhow!(
            "F1 23 player car index {} out of range (max {})",
            player_index,
            NUM_CARS - 1
        ));
    }

    let car_offset = HEADER_SIZE + player_index * CAR_STATUS_2023_ENTRY_SIZE;
    let mut r = ByteReader::at(raw, car_offset);

    let traction_control = r.u8()?; // 0
    let anti_lock_brakes = r.u8()?; // 1
    r.skip(2)?; // fuelMix (2), frontBrakeBias (3)
    let pit_limiter_status = r.u8()?; // 4
    let fuel_in_tank = r.f32_le()?; // 5-8
    r.skip(4)?; // fuelCapacity (9-12)
    let fuel_remaining_laps = r.f32_le()?; // 13-16
    let max_rpm = r.u16_le()?; // 17-18
    r.skip(2)?; // idleRPM (19-20)
    r.skip(1)?; // maxGears (21)
    let drs_allowed = r.u8()?; // 22
    r.skip(2)?; // drsActivationDistance (23-24)
    let actual_tyre_compound = r.u8()?; // 25
    r.skip(1)?; // visualTyreCompound (26)
    let tyre_age_laps = r.u8()?; // 27
    r.skip(1)?; // vehicleFiaFlags (28)
    let ers_store_energy = r.f32_le()?; // 29-32
    let ers_deploy_mode = r.u8()?; // 33
    let ers_harvested_mguk = r.f32_le()?; // 34-37
    let ers_harvested_mguh = r.f32_le()?; // 38-41
    let ers_deployed = r.f32_le()?; // 42-45
    // networkPaused (46) ignored

    Ok(F1NativeCarStatusData {
        traction_control,
        anti_lock_brakes,
        pit_limiter_status,
        fuel_in_tank,
        fuel_remaining_laps,
        max_rpm,
        drs_allowed,
        actual_tyre_compound,
        tyre_age_laps,
        engine_power_ice: 0.0,  // not present in F1 23
        engine_power_mguk: 0.0, // not present in F1 23
        ers_store_energy,
        ers_deploy_mode,
        ers_harvested_mguk,
        ers_harvested_mguh,
        ers_deployed,
    })
}

/// Parse the CarStatusData entry for `player_index` from an F1 24 Car Status packet.
///
/// F1 24 CarStatusData layout (55 bytes per car):
/// - bytes 0-28: shared fields (identical to F1 23)
/// - bytes 29-32: enginePowerICE (f32) — new in F1 24
/// - bytes 33-36: enginePowerMGUK (f32) — new in F1 24
/// - bytes 37-40: ersStoreEnergy (f32)
/// - byte 41: ersDeployMode
/// - bytes 42-45: ersHarvestedMGUK (f32)
/// - bytes 46-49: ersHarvestedMGUH (f32)
/// - bytes 50-53: ersDeployedThisLap (f32)
/// - byte 54: networkPaused (ignored)
pub fn parse_car_status_2024(raw: &[u8], player_index: usize) -> Result<F1NativeCarStatusData> {
    if raw.len() < MIN_CAR_STATUS_2024_PACKET_SIZE {
        return Err(anyhow!(
            "F1 24 CarStatus packet too short: {} bytes (need {})",
            raw.len(),
            MIN_CAR_STATUS_2024_PACKET_SIZE
        ));
    }
    if player_index >= NUM_CARS {
        return Err(anyhow!(
            "F1 24 player car index {} out of range (max {})",
            player_index,
            NUM_CARS - 1
        ));
    }

    let car_offset = HEADER_SIZE + player_index * CAR_STATUS_2024_ENTRY_SIZE;
    let mut r = ByteReader::at(raw, car_offset);

    let traction_control = r.u8()?; // 0
    let anti_lock_brakes = r.u8()?; // 1
    r.skip(2)?; // fuelMix (2), frontBrakeBias (3)
    let pit_limiter_status = r.u8()?; // 4
    let fuel_in_tank = r.f32_le()?; // 5-8
    r.skip(4)?; // fuelCapacity (9-12)
    let fuel_remaining_laps = r.f32_le()?; // 13-16
    let max_rpm = r.u16_le()?; // 17-18
    r.skip(2)?; // idleRPM (19-20)
    r.skip(1)?; // maxGears (21)
    let drs_allowed = r.u8()?; // 22
    r.skip(2)?; // drsActivationDistance (23-24)
    let actual_tyre_compound = r.u8()?; // 25
    r.skip(1)?; // visualTyreCompound (26)
    let tyre_age_laps = r.u8()?; // 27
    r.skip(1)?; // vehicleFiaFlags (28)
    let engine_power_ice = r.f32_le()?; // 29-32
    let engine_power_mguk = r.f32_le()?; // 33-36
    let ers_store_energy = r.f32_le()?; // 37-40
    let ers_deploy_mode = r.u8()?; // 41
    let ers_harvested_mguk = r.f32_le()?; // 42-45
    let ers_harvested_mguh = r.f32_le()?; // 46-49
    let ers_deployed = r.f32_le()?; // 50-53
    // networkPaused (54) ignored

    Ok(F1NativeCarStatusData {
        traction_control,
        anti_lock_brakes,
        pit_limiter_status,
        fuel_in_tank,
        fuel_remaining_laps,
        max_rpm,
        drs_allowed,
        actual_tyre_compound,
        tyre_age_laps,
        engine_power_ice,
        engine_power_mguk,
        ers_store_energy,
        ers_deploy_mode,
        ers_harvested_mguk,
        ers_harvested_mguh,
        ers_deployed,
    })
}

// ── Normalization ─────────────────────────────────────────────────────────────

/// Combine parsed car telemetry, status, and session into [`NormalizedTelemetry`].
pub fn normalize(
    telem: &crate::f1_25::CarTelemetryData,
    status: &F1NativeCarStatusData,
    session: &SessionData,
) -> NormalizedTelemetry {
    let speed_ms = f32::from(telem.speed_kmh) / 3.6;
    let rpm = f32::from(telem.engine_rpm);

    let drs_active = telem.drs != 0;
    let drs_available = status.drs_allowed != 0;
    let pit_limiter = status.pit_limiter_status != 0;

    let flags = TelemetryFlags {
        pit_limiter,
        in_pits: pit_limiter,
        drs_active,
        drs_available,
        traction_control: status.traction_control != 0,
        abs_active: status.anti_lock_brakes != 0,
        ers_available: status.ers_store_energy > 0.0,
        ..TelemetryFlags::default()
    };

    let track_id = track_name_from_id(session.track_id);
    let tyre_name = tyre_compound_name(status.actual_tyre_compound);
    let ers_fraction = if ERS_MAX_STORE_ENERGY_J > 0.0 {
        (status.ers_store_energy / ERS_MAX_STORE_ENERGY_J).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let rpm_fraction = if status.max_rpm > 0 {
        (f32::from(telem.engine_rpm) / f32::from(status.max_rpm)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    NormalizedTelemetry::builder()
        .speed_ms(speed_ms)
        .rpm(rpm)
        .gear(telem.gear)
        .flags(flags)
        .track_id(track_id)
        .extended(
            "throttle".to_string(),
            TelemetryValue::Float(telem.throttle),
        )
        .extended("brake".to_string(), TelemetryValue::Float(telem.brake))
        .extended("steer".to_string(), TelemetryValue::Float(telem.steer))
        .extended(
            "drs_active".to_string(),
            TelemetryValue::Boolean(drs_active),
        )
        .extended(
            "drs_available".to_string(),
            TelemetryValue::Boolean(drs_available),
        )
        .extended(
            "ers_store_energy_j".to_string(),
            TelemetryValue::Float(status.ers_store_energy),
        )
        .extended(
            "ers_store_fraction".to_string(),
            TelemetryValue::Float(ers_fraction),
        )
        .extended(
            "ers_deploy_mode".to_string(),
            TelemetryValue::Integer(i32::from(status.ers_deploy_mode)),
        )
        .extended(
            "ers_harvested_mguk_j".to_string(),
            TelemetryValue::Float(status.ers_harvested_mguk),
        )
        .extended(
            "ers_harvested_mguh_j".to_string(),
            TelemetryValue::Float(status.ers_harvested_mguh),
        )
        .extended(
            "ers_deployed_j".to_string(),
            TelemetryValue::Float(status.ers_deployed),
        )
        .extended(
            "engine_power_ice_w".to_string(),
            TelemetryValue::Float(status.engine_power_ice),
        )
        .extended(
            "engine_power_mguk_w".to_string(),
            TelemetryValue::Float(status.engine_power_mguk),
        )
        .extended(
            "engine_temperature_c".to_string(),
            TelemetryValue::Integer(i32::from(telem.engine_temperature)),
        )
        .extended(
            "rpm_fraction".to_string(),
            TelemetryValue::Float(rpm_fraction),
        )
        .extended(
            "fuel_remaining_kg".to_string(),
            TelemetryValue::Float(status.fuel_in_tank),
        )
        .extended(
            "fuel_remaining_laps".to_string(),
            TelemetryValue::Float(status.fuel_remaining_laps),
        )
        .extended(
            "tyre_compound".to_string(),
            TelemetryValue::Integer(i32::from(status.actual_tyre_compound)),
        )
        .extended(
            "tyre_compound_name".to_string(),
            TelemetryValue::String(tyre_name.to_string()),
        )
        .extended(
            "tyre_age_laps".to_string(),
            TelemetryValue::Integer(i32::from(status.tyre_age_laps)),
        )
        .extended(
            "tyre_pressure_rl_psi".to_string(),
            TelemetryValue::Float(telem.tyres_pressure[0]),
        )
        .extended(
            "tyre_pressure_rr_psi".to_string(),
            TelemetryValue::Float(telem.tyres_pressure[1]),
        )
        .extended(
            "tyre_pressure_fl_psi".to_string(),
            TelemetryValue::Float(telem.tyres_pressure[2]),
        )
        .extended(
            "tyre_pressure_fr_psi".to_string(),
            TelemetryValue::Float(telem.tyres_pressure[3]),
        )
        .extended(
            "tyre_surface_temp_rl_c".to_string(),
            TelemetryValue::Integer(i32::from(telem.tyres_surface_temperature[0])),
        )
        .extended(
            "tyre_surface_temp_rr_c".to_string(),
            TelemetryValue::Integer(i32::from(telem.tyres_surface_temperature[1])),
        )
        .extended(
            "tyre_surface_temp_fl_c".to_string(),
            TelemetryValue::Integer(i32::from(telem.tyres_surface_temperature[2])),
        )
        .extended(
            "tyre_surface_temp_fr_c".to_string(),
            TelemetryValue::Integer(i32::from(telem.tyres_surface_temperature[3])),
        )
        .extended(
            "tyre_inner_temp_rl_c".to_string(),
            TelemetryValue::Integer(i32::from(telem.tyres_inner_temperature[0])),
        )
        .extended(
            "tyre_inner_temp_rr_c".to_string(),
            TelemetryValue::Integer(i32::from(telem.tyres_inner_temperature[1])),
        )
        .extended(
            "tyre_inner_temp_fl_c".to_string(),
            TelemetryValue::Integer(i32::from(telem.tyres_inner_temperature[2])),
        )
        .extended(
            "tyre_inner_temp_fr_c".to_string(),
            TelemetryValue::Integer(i32::from(telem.tyres_inner_temperature[3])),
        )
        .extended(
            "brake_temp_rl_c".to_string(),
            TelemetryValue::Integer(i32::from(telem.brakes_temperature[0])),
        )
        .extended(
            "brake_temp_rr_c".to_string(),
            TelemetryValue::Integer(i32::from(telem.brakes_temperature[1])),
        )
        .extended(
            "brake_temp_fl_c".to_string(),
            TelemetryValue::Integer(i32::from(telem.brakes_temperature[2])),
        )
        .extended(
            "brake_temp_fr_c".to_string(),
            TelemetryValue::Integer(i32::from(telem.brakes_temperature[3])),
        )
        .extended(
            "session_type".to_string(),
            TelemetryValue::Integer(i32::from(session.session_type)),
        )
        .extended(
            "track_temperature_c".to_string(),
            TelemetryValue::Integer(i32::from(session.track_temperature)),
        )
        .extended(
            "air_temperature_c".to_string(),
            TelemetryValue::Integer(i32::from(session.air_temperature)),
        )
        .extended(
            "decoder_type".to_string(),
            TelemetryValue::String("f1_native_udp".to_string()),
        )
        .build()
}

// ── Env helpers ───────────────────────────────────────────────────────────────

fn env_u16(name: &str, fallback: u16) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(fallback)
}

fn env_u64(name: &str, fallback: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(fallback)
}

// ── Test packet builders (pub for integration tests) ─────────────────────────

/// Build a minimal valid F1 23 packet header (29 bytes).
pub fn build_f1_native_header_bytes(
    packet_format: u16,
    packet_id: u8,
    player_index: u8,
) -> Vec<u8> {
    let game_year = match packet_format {
        2023 => 23u8,
        2024 => 24u8,
        other => (other % 100) as u8,
    };
    let mut buf = Vec::with_capacity(HEADER_SIZE);
    buf.extend_from_slice(&packet_format.to_le_bytes()); // 0-1
    buf.push(game_year); // 2
    buf.push(1); // gameMajorVersion  (3)
    buf.push(0); // gameMinorVersion  (4)
    buf.push(1); // packetVersion  (5)
    buf.push(packet_id); // 6
    buf.extend_from_slice(&0u64.to_le_bytes()); // sessionUID  (7-14)
    buf.extend_from_slice(&0.0f32.to_le_bytes()); // sessionTime  (15-18)
    buf.extend_from_slice(&0u32.to_le_bytes()); // frameIdentifier  (19-22)
    buf.extend_from_slice(&0u32.to_le_bytes()); // overallFrameIdentifier  (23-26)
    buf.push(player_index); // 27
    buf.push(255); // secondaryPlayerCarIndex  (28)
    buf
}

/// Build a minimal valid F1 23 Car Status packet for car at `player_index`.
pub fn build_car_status_packet_f23(
    player_index: u8,
    fuel_in_tank: f32,
    ers_store_energy: f32,
    drs_allowed: u8,
    pit_limiter: u8,
    actual_tyre_compound: u8,
    max_rpm: u16,
) -> Vec<u8> {
    let mut buf =
        build_f1_native_header_bytes(PACKET_FORMAT_2023, PACKET_ID_CAR_STATUS, player_index);

    let total_car_bytes = NUM_CARS * CAR_STATUS_2023_ENTRY_SIZE;
    buf.extend(std::iter::repeat_n(0u8, total_car_bytes));
    let offset = HEADER_SIZE + usize::from(player_index) * CAR_STATUS_2023_ENTRY_SIZE;

    // 0: tractionControl, 1: antiLockBrakes stay zero
    // 2: fuelMix, 3: frontBrakeBias stay zero
    buf[offset + 4] = pit_limiter; // pitLimiterStatus
    buf[offset + 5..offset + 9].copy_from_slice(&fuel_in_tank.to_le_bytes());
    // fuelCapacity (9-12) stays zero
    // fuelRemainingLaps (13-16) stays zero
    buf[offset + 17..offset + 19].copy_from_slice(&max_rpm.to_le_bytes());
    // idleRPM (19-20) stays zero
    // maxGears (21) stays zero
    buf[offset + 22] = drs_allowed;
    // drsActivationDistance (23-24) stays zero
    buf[offset + 25] = actual_tyre_compound;
    // visualTyreCompound (26), tyresAgeLaps (27), vehicleFiaFlags (28) stay zero
    buf[offset + 29..offset + 33].copy_from_slice(&ers_store_energy.to_le_bytes());
    // ersDeployMode (33) stays zero
    // ersHarvested/Deployed (34-45) stay zero
    // networkPaused (46) stays zero

    buf
}

/// Build a minimal valid F1 24 Car Status packet for car at `player_index`.
pub fn build_car_status_packet_f24(
    player_index: u8,
    fuel_in_tank: f32,
    ers_store_energy: f32,
    drs_allowed: u8,
    pit_limiter: u8,
    actual_tyre_compound: u8,
    max_rpm: u16,
) -> Vec<u8> {
    let mut buf =
        build_f1_native_header_bytes(PACKET_FORMAT_2024, PACKET_ID_CAR_STATUS, player_index);

    let total_car_bytes = NUM_CARS * CAR_STATUS_2024_ENTRY_SIZE;
    buf.extend(std::iter::repeat_n(0u8, total_car_bytes));
    let offset = HEADER_SIZE + usize::from(player_index) * CAR_STATUS_2024_ENTRY_SIZE;

    buf[offset + 4] = pit_limiter;
    buf[offset + 5..offset + 9].copy_from_slice(&fuel_in_tank.to_le_bytes());
    buf[offset + 17..offset + 19].copy_from_slice(&max_rpm.to_le_bytes());
    buf[offset + 22] = drs_allowed;
    buf[offset + 25] = actual_tyre_compound;
    // enginePowerICE (29-32), enginePowerMGUK (33-36) stay zero
    buf[offset + 37..offset + 41].copy_from_slice(&ers_store_energy.to_le_bytes());
    // networkPaused (54) stays zero

    buf
}

/// Build a minimal valid Car Telemetry packet for the given format (2023 or 2024).
///
/// Car Telemetry layout is identical between F1 23 and F1 24; the format
/// only affects the header's `packetFormat` field.
#[allow(clippy::too_many_arguments)]
pub fn build_car_telemetry_packet_native(
    packet_format: u16,
    player_index: u8,
    speed_kmh: u16,
    gear: i8,
    engine_rpm: u16,
    throttle: f32,
    brake: f32,
    steer: f32,
    drs: u8,
    tyres_pressure: [f32; 4],
) -> Vec<u8> {
    let mut buf =
        build_f1_native_header_bytes(packet_format, PACKET_ID_CAR_TELEMETRY, player_index);

    let total_car_bytes = NUM_CARS * CAR_TELEMETRY_ENTRY_SIZE;
    buf.extend(std::iter::repeat_n(0u8, total_car_bytes));
    let offset = HEADER_SIZE + usize::from(player_index) * CAR_TELEMETRY_ENTRY_SIZE;

    buf[offset..offset + 2].copy_from_slice(&speed_kmh.to_le_bytes());
    buf[offset + 2..offset + 6].copy_from_slice(&throttle.to_le_bytes());
    buf[offset + 6..offset + 10].copy_from_slice(&steer.to_le_bytes());
    buf[offset + 10..offset + 14].copy_from_slice(&brake.to_le_bytes());
    buf[offset + 14] = 0; // clutch
    buf[offset + 15] = gear as u8;
    buf[offset + 16..offset + 18].copy_from_slice(&engine_rpm.to_le_bytes());
    buf[offset + 18] = drs;
    // revLightsPercent (19), revLightsBitValue (20-21) stay zero
    // brakes_temperature (22-29) stay zero
    // tyres_surface_temperature (30-33) stay zero
    // tyres_inner_temperature (34-37) stay zero
    // engine_temperature (38-39) stays zero
    buf[offset + 40..offset + 44].copy_from_slice(&tyres_pressure[0].to_le_bytes()); // RL
    buf[offset + 44..offset + 48].copy_from_slice(&tyres_pressure[1].to_le_bytes()); // RR
    buf[offset + 48..offset + 52].copy_from_slice(&tyres_pressure[2].to_le_bytes()); // FL
    buf[offset + 52..offset + 56].copy_from_slice(&tyres_pressure[3].to_le_bytes()); // FR
    // surfaceType (56-59) stays zero

    // Trailer: mfdPanelIndex (1), mfdPanelIndexSecondary (1), suggestedGear (1)
    buf.extend_from_slice(&[0u8; 3]);
    buf
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    // ── Header parsing ──────────────────────────────────────────────────────

    #[test]
    fn parse_header_extracts_format_2023() -> TestResult {
        let raw = build_f1_native_header_bytes(2023, 6, 0);
        let header = parse_header(&raw)?;
        assert_eq!(header.packet_format, 2023);
        assert_eq!(header.packet_id, 6);
        Ok(())
    }

    #[test]
    fn parse_header_extracts_format_2024() -> TestResult {
        let raw = build_f1_native_header_bytes(2024, 7, 2);
        let header = parse_header(&raw)?;
        assert_eq!(header.packet_format, 2024);
        assert_eq!(header.packet_id, 7);
        assert_eq!(header.player_car_index, 2);
        Ok(())
    }

    // ── process_packet rejects wrong format ─────────────────────────────────

    #[test]
    fn process_packet_rejects_format_2025() {
        let raw = build_f1_native_header_bytes(2025, PACKET_ID_CAR_TELEMETRY, 0);
        let mut state = F1NativeState::default();
        let result = F1NativeAdapter::process_packet(&mut state, &raw);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("2025"),
            "error should mention the received format"
        );
    }

    #[test]
    fn process_packet_rejects_format_2022() {
        let raw = build_f1_native_header_bytes(2022, PACKET_ID_CAR_TELEMETRY, 0);
        let mut state = F1NativeState::default();
        let result = F1NativeAdapter::process_packet(&mut state, &raw);
        assert!(result.is_err());
    }

    // ── F1 23 CarStatus parsing ─────────────────────────────────────────────

    #[test]
    fn car_status_2023_round_trip_fuel_ers_drs() -> TestResult {
        let raw = build_car_status_packet_f23(
            0,           // player_index
            30.0,        // fuel_in_tank kg
            2_500_000.0, // ers_store_energy J
            1,           // drs_allowed
            0,           // pit_limiter
            12,          // actual_tyre_compound (Soft)
            15_000,      // max_rpm
        );
        let status = parse_car_status_2023(&raw, 0)?;
        assert!(
            (status.fuel_in_tank - 30.0).abs() < 1e-5,
            "fuel_in_tank mismatch"
        );
        assert!(
            (status.ers_store_energy - 2_500_000.0).abs() < 1.0,
            "ers_store_energy mismatch"
        );
        assert_eq!(status.drs_allowed, 1);
        assert_eq!(status.pit_limiter_status, 0);
        assert_eq!(status.actual_tyre_compound, 12);
        assert_eq!(status.max_rpm, 15_000);
        // engine power not present in F1 23
        assert_eq!(status.engine_power_ice, 0.0);
        assert_eq!(status.engine_power_mguk, 0.0);
        Ok(())
    }

    #[test]
    fn car_status_2023_rejects_short_packet() {
        let result = parse_car_status_2023(&[0u8; 100], 0);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("too short"));
    }

    #[test]
    fn car_status_2023_rejects_out_of_range_player_index() -> TestResult {
        let raw = build_car_status_packet_f23(0, 10.0, 1_000_000.0, 0, 0, 13, 12000);
        let result = parse_car_status_2023(&raw, 22);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn car_status_2023_pit_limiter_active() -> TestResult {
        let raw = build_car_status_packet_f23(1, 5.0, 0.0, 0, 1, 14, 10000);
        let status = parse_car_status_2023(&raw, 1)?;
        assert_eq!(status.pit_limiter_status, 1);
        Ok(())
    }

    // ── F1 24 CarStatus parsing ─────────────────────────────────────────────

    #[test]
    fn car_status_2024_round_trip_fuel_ers_engine_power() -> TestResult {
        let raw = build_car_status_packet_f24(
            0,           // player_index
            28.5,        // fuel_in_tank kg
            3_000_000.0, // ers_store_energy J
            1,           // drs_allowed
            0,           // pit_limiter
            13,          // actual_tyre_compound (Medium)
            14_500,      // max_rpm
        );
        let status = parse_car_status_2024(&raw, 0)?;
        assert!(
            (status.fuel_in_tank - 28.5).abs() < 1e-5,
            "fuel_in_tank mismatch"
        );
        assert!(
            (status.ers_store_energy - 3_000_000.0).abs() < 1.0,
            "ers_store_energy mismatch"
        );
        assert_eq!(status.drs_allowed, 1);
        assert_eq!(status.actual_tyre_compound, 13);
        assert_eq!(status.max_rpm, 14_500);
        // engine_power_ice and engine_power_mguk are zero in the test builder
        assert_eq!(status.engine_power_ice, 0.0);
        Ok(())
    }

    #[test]
    fn car_status_2024_rejects_short_packet() {
        let result = parse_car_status_2024(&[0u8; 200], 0);
        assert!(result.is_err());
    }

    #[test]
    fn car_status_2024_rejects_out_of_range_player_index() -> TestResult {
        let raw = build_car_status_packet_f24(0, 10.0, 0.0, 0, 0, 14, 12000);
        let result = parse_car_status_2024(&raw, 22);
        assert!(result.is_err());
        Ok(())
    }

    // ── CarTelemetry parsing (shared format) ────────────────────────────────

    #[test]
    fn car_telemetry_2023_round_trip_speed_gear_rpm() -> TestResult {
        let raw = build_car_telemetry_packet_native(
            PACKET_FORMAT_2023,
            0,                        // player_index
            180,                      // speed_kmh
            5,                        // gear
            12000,                    // engine_rpm
            0.75,                     // throttle
            0.0,                      // brake
            -0.1,                     // steer
            0,                        // drs off
            [23.0, 23.0, 22.5, 22.5], // tyre pressures PSI
        );
        let header = parse_header(&raw)?;
        assert_eq!(header.packet_format, 2023);
        let telem = parse_car_telemetry(&raw, 0)?;
        assert_eq!(telem.speed_kmh, 180);
        assert_eq!(telem.gear, 5);
        assert_eq!(telem.engine_rpm, 12000);
        assert!((telem.throttle - 0.75).abs() < 1e-5, "throttle mismatch");
        assert_eq!(telem.drs, 0);
        assert!(
            (telem.tyres_pressure[2] - 22.5).abs() < 1e-4,
            "FL pressure mismatch"
        );
        Ok(())
    }

    #[test]
    fn car_telemetry_2024_drs_active() -> TestResult {
        let raw = build_car_telemetry_packet_native(
            PACKET_FORMAT_2024,
            0,
            250,
            8,
            14500,
            0.95,
            0.0,
            0.05,
            1, // drs active
            [24.0; 4],
        );
        let telem = parse_car_telemetry(&raw, 0)?;
        assert_eq!(telem.drs, 1);
        assert_eq!(telem.speed_kmh, 250);
        Ok(())
    }

    #[test]
    fn car_telemetry_reverse_gear() -> TestResult {
        let raw = build_car_telemetry_packet_native(
            PACKET_FORMAT_2023,
            0,
            5,
            -1,
            2000,
            0.0,
            0.0,
            0.0,
            0,
            [20.0; 4],
        );
        let telem = parse_car_telemetry(&raw, 0)?;
        assert_eq!(telem.gear, -1);
        Ok(())
    }

    // ── Full process_packet state machine ───────────────────────────────────

    #[test]
    fn process_packet_emits_normalized_after_telem_and_status_f23() -> TestResult {
        let mut state = F1NativeState::default();

        // First: send Car Telemetry — should not yet emit (no status)
        let telem_raw = build_car_telemetry_packet_native(
            PACKET_FORMAT_2023,
            0,
            180,
            5,
            12000,
            0.7,
            0.0,
            0.0,
            0,
            [23.0; 4],
        );
        let result = F1NativeAdapter::process_packet(&mut state, &telem_raw)?;
        assert!(
            result.is_none(),
            "should not emit before status is received"
        );

        // Then: send Car Status — should now emit
        let status_raw = build_car_status_packet_f23(0, 25.0, 2_000_000.0, 1, 0, 12, 13000);
        let normalized = F1NativeAdapter::process_packet(&mut state, &status_raw)?;
        assert!(
            normalized.is_some(),
            "should emit after both telem and status"
        );

        let norm = normalized.unwrap();
        let expected_speed = 180.0 / 3.6;
        assert!(
            (norm.speed_ms - expected_speed).abs() < 0.01,
            "speed_ms mismatch"
        );
        assert_eq!(norm.gear, 5);
        assert!((norm.rpm - 12000.0).abs() < 0.1, "rpm mismatch");
        assert!(norm.flags.drs_available, "drs_available should be true");
        assert!(!norm.flags.drs_active, "drs should not be active");
        assert!(!norm.flags.pit_limiter, "pit_limiter should be off");
        Ok(())
    }

    #[test]
    fn process_packet_emits_normalized_after_telem_and_status_f24() -> TestResult {
        let mut state = F1NativeState::default();

        let telem_raw = build_car_telemetry_packet_native(
            PACKET_FORMAT_2024,
            0,
            300,
            8,
            14000,
            1.0,
            0.0,
            0.0,
            1, // drs active
            [24.0; 4],
        );
        F1NativeAdapter::process_packet(&mut state, &telem_raw)?;

        let status_raw = build_car_status_packet_f24(0, 10.0, 3_500_000.0, 1, 0, 14, 15000);
        let normalized = F1NativeAdapter::process_packet(&mut state, &status_raw)?;
        assert!(
            normalized.is_some(),
            "should emit after both telem and status"
        );

        let norm = normalized.unwrap();
        assert!(
            (norm.speed_ms - 300.0 / 3.6).abs() < 0.01,
            "speed_ms mismatch"
        );
        assert_eq!(norm.gear, 8);
        assert!(norm.flags.drs_active, "drs should be active");
        assert!(norm.flags.ers_available, "ers_available should be true");
        Ok(())
    }

    #[test]
    fn process_packet_updates_session_data() -> TestResult {
        let mut state = F1NativeState::default();

        // Build a minimal Session packet (format 2023)
        let mut session_raw =
            build_f1_native_header_bytes(PACKET_FORMAT_2023, PACKET_ID_SESSION, 0);
        session_raw.push(0); // weather
        session_raw.push(32i8 as u8); // trackTemperature 32°C
        session_raw.push(26i8 as u8); // airTemperature 26°C
        session_raw.push(50); // totalLaps
        session_raw.extend_from_slice(&5326u16.to_le_bytes()); // trackLength
        session_raw.push(6); // sessionType (Race)
        session_raw.push(11i8 as u8); // trackId (Monza)

        let result = F1NativeAdapter::process_packet(&mut state, &session_raw)?;
        assert!(result.is_none(), "Session packet should not emit telemetry");
        assert_eq!(state.session.track_temperature, 32);
        assert_eq!(state.session.air_temperature, 26);
        assert_eq!(state.session.session_type, 6);
        assert_eq!(state.session.track_id, 11);
        Ok(())
    }

    #[test]
    fn process_packet_ignores_unknown_packet_id() -> TestResult {
        let raw = build_f1_native_header_bytes(PACKET_FORMAT_2023, 99, 0);
        let mut state = F1NativeState::default();
        let result = F1NativeAdapter::process_packet(&mut state, &raw)?;
        assert!(result.is_none());
        Ok(())
    }

    // ── normalize() single-packet API ───────────────────────────────────────

    #[test]
    fn adapter_normalize_single_car_telemetry_packet() -> TestResult {
        let adapter = F1NativeAdapter::new();
        let raw = build_car_telemetry_packet_native(
            PACKET_FORMAT_2023,
            0,
            144,
            4,
            9500,
            0.5,
            0.2,
            0.0,
            0,
            [21.0; 4],
        );
        let norm = adapter.normalize(&raw)?;
        assert!(
            (norm.speed_ms - 144.0 / 3.6).abs() < 0.01,
            "speed_ms mismatch"
        );
        assert_eq!(norm.gear, 4);
        assert!((norm.rpm - 9500.0).abs() < 0.1, "rpm mismatch");
        assert_eq!(
            norm.extended.get("decoder_type"),
            Some(&TelemetryValue::String("f1_native_udp".to_string()))
        );
        Ok(())
    }

    #[test]
    fn adapter_normalize_rejects_car_status_packet_alone() -> TestResult {
        let adapter = F1NativeAdapter::new();
        let raw = build_car_status_packet_f23(0, 20.0, 1_000_000.0, 0, 0, 13, 12000);
        let result = adapter.normalize(&raw);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn adapter_normalize_rejects_wrong_format() {
        let adapter = F1NativeAdapter::new();
        let raw = build_f1_native_header_bytes(2025, PACKET_ID_CAR_TELEMETRY, 0);
        let result = adapter.normalize(&raw);
        assert!(result.is_err());
    }

    // ── game_id ─────────────────────────────────────────────────────────────

    #[test]
    fn adapter_game_id_is_f1_native() {
        let adapter = F1NativeAdapter::new();
        assert_eq!(adapter.game_id(), "f1_native");
    }

    // ── Normalization correctness ────────────────────────────────────────────

    #[test]
    fn normalize_speed_conversion_from_kmh_to_ms() -> TestResult {
        let telem = crate::f1_25::CarTelemetryData {
            speed_kmh: 360,
            throttle: 1.0,
            steer: 0.0,
            brake: 0.0,
            gear: 8,
            engine_rpm: 14000,
            drs: 0,
            brakes_temperature: [500, 500, 500, 500],
            tyres_surface_temperature: [90, 90, 90, 90],
            tyres_inner_temperature: [100, 100, 100, 100],
            engine_temperature: 110,
            tyres_pressure: [24.0; 4],
        };
        let status = F1NativeCarStatusData {
            max_rpm: 15000,
            drs_allowed: 0,
            actual_tyre_compound: 14,
            ers_store_energy: 1_000_000.0,
            ..F1NativeCarStatusData::default()
        };
        let norm = normalize(&telem, &status, &SessionData::default());
        let expected_ms = 360.0 / 3.6;
        assert!(
            (norm.speed_ms - expected_ms).abs() < 0.01,
            "speed_ms conversion failed"
        );
        assert_eq!(norm.gear, 8);
        assert!((norm.rpm - 14000.0).abs() < 0.1, "rpm mismatch");
        assert!(norm.flags.ers_available, "ers_available should be true");
        assert_eq!(
            norm.extended.get("throttle"),
            Some(&TelemetryValue::Float(1.0))
        );
        Ok(())
    }

    #[test]
    fn normalize_ers_fraction_is_clamped_to_0_1() -> TestResult {
        let telem = crate::f1_25::CarTelemetryData {
            speed_kmh: 0,
            throttle: 0.0,
            steer: 0.0,
            brake: 0.0,
            gear: 0,
            engine_rpm: 0,
            drs: 0,
            brakes_temperature: [0; 4],
            tyres_surface_temperature: [0; 4],
            tyres_inner_temperature: [0; 4],
            engine_temperature: 0,
            tyres_pressure: [0.0; 4],
        };
        let status = F1NativeCarStatusData {
            ers_store_energy: ERS_MAX_STORE_ENERGY_J * 2.0, // overflow
            ..F1NativeCarStatusData::default()
        };
        let norm = normalize(&telem, &status, &SessionData::default());
        if let Some(TelemetryValue::Float(fraction)) = norm.extended.get("ers_store_fraction") {
            assert!(*fraction <= 1.0, "ers_store_fraction must not exceed 1.0");
            assert!(*fraction >= 0.0, "ers_store_fraction must not be negative");
        } else {
            return Err("ers_store_fraction not found in extended fields".into());
        }
        Ok(())
    }

    #[test]
    fn normalize_f23_has_zero_engine_power_fields() -> TestResult {
        let telem = crate::f1_25::CarTelemetryData {
            speed_kmh: 100,
            throttle: 0.5,
            steer: 0.0,
            brake: 0.0,
            gear: 3,
            engine_rpm: 8000,
            drs: 0,
            brakes_temperature: [0; 4],
            tyres_surface_temperature: [0; 4],
            tyres_inner_temperature: [0; 4],
            engine_temperature: 0,
            tyres_pressure: [0.0; 4],
        };
        // Status parsed from F1 23 packet always has zero engine power
        let status = F1NativeCarStatusData {
            engine_power_ice: 0.0,
            engine_power_mguk: 0.0,
            ..F1NativeCarStatusData::default()
        };
        let norm = normalize(&telem, &status, &SessionData::default());
        assert_eq!(
            norm.extended.get("engine_power_ice_w"),
            Some(&TelemetryValue::Float(0.0))
        );
        assert_eq!(
            norm.extended.get("engine_power_mguk_w"),
            Some(&TelemetryValue::Float(0.0))
        );
        Ok(())
    }
}
