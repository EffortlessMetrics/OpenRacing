//! F1 25 native UDP telemetry adapter.
//!
//! Parses the EA F1 25 (packet format `2025`) binary UDP protocol directly.
//! No bridge layer or XML spec file is required.
//!
//! ## Supported packet types
//!
//! | Packet ID | Name          | Fields used                                |
//! |-----------|---------------|--------------------------------------------|
//! | 1         | Session        | track ID, session type, temperatures       |
//! | 6         | Car Telemetry  | speed, gear, RPM, DRS, tyre temps/pressure |
//! | 7         | Car Status     | fuel, ERS, pit limiter, tyre compound      |
//!
//! All other packet IDs are silently discarded.
//!
//! ## Default UDP port
//! 20777  (override with `OPENRACING_F1_25_UDP_PORT`).
//!
//! ## Unit conventions
//! - Speed: km/h → m/s (÷ 3.6)
//! - Tyre pressure: PSI (as reported by the game)
//! - ERS store energy: Joules (as reported by the game)
//! - Fuel remaining: kg (as reported by the game)
//! - Temperatures: °C (integers)
//!
//! ## Verification against EA F1 UDP specification (2025-07)
//!
//! Verified against the EA Sports F1 25 UDP specification (packet format 2025)
//! and community implementations.
//!
//! - **Default port**: 20777 — standard Codemasters/EA F1 UDP port since F1 2019. ✓
//! - **Header size**: 29 bytes (consistent across F1 2023/2024/2025 formats). ✓
//! - **Packet format field**: u16 = 2025 (identifies the year/version). ✓
//! - **Packet IDs**: 1=Session, 6=CarTelemetry, 7=CarStatus — standard EA IDs. ✓
//! - **NUM_CARS**: 22 (F1 grid size). ✓
//! - **CarTelemetryData entry**: 60 bytes per car. ✓
//! - **CarStatusData entry**: 55 bytes per car. ✓
//! - **ERS max store**: 4 MJ (4,000,000 J) — per F1 regulations and EA spec. ✓

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

/// Verified: EA Sports F1 25 UDP spec, standard Codemasters/EA port since F1 2019.
const DEFAULT_PORT: u16 = 20777;
const DEFAULT_HEARTBEAT_TIMEOUT_MS: u64 = 2_000;
const MAX_PACKET_BYTES: usize = 2048;
const NUM_CARS: usize = 22;

const PACKET_FORMAT_2025: u16 = 2025;
const HEADER_SIZE: usize = 29;
const PACKET_ID_SESSION: u8 = 1;
const PACKET_ID_CAR_TELEMETRY: u8 = 6;
const PACKET_ID_CAR_STATUS: u8 = 7;

/// EA F1 25 spec: battery stores up to 4 MJ.
pub const ERS_MAX_STORE_ENERGY_J: f32 = 4_000_000.0;

const ENV_PORT: &str = "OPENRACING_F1_25_UDP_PORT";
const ENV_HEARTBEAT_MS: &str = "OPENRACING_F1_25_HEARTBEAT_TIMEOUT_MS";

// ── Packet entry byte sizes ───────────────────────────────────────────────────

/// Size of one CarTelemetryData entry (60 bytes).
pub const CAR_TELEMETRY_ENTRY_SIZE: usize = 60;
/// Size of one CarStatusData entry (55 bytes).
pub const CAR_STATUS_ENTRY_SIZE: usize = 55;

/// Minimum size for a full Car Telemetry packet (all 22 cars + trailer).
pub const MIN_CAR_TELEMETRY_PACKET_SIZE: usize =
    HEADER_SIZE + NUM_CARS * CAR_TELEMETRY_ENTRY_SIZE + 3;
/// Minimum size for a full Car Status packet (all 22 cars).
pub const MIN_CAR_STATUS_PACKET_SIZE: usize = HEADER_SIZE + NUM_CARS * CAR_STATUS_ENTRY_SIZE;

// ── Track name lookup ─────────────────────────────────────────────────────────

/// Maps `m_trackId` (0-based) to a display name.  Unknown IDs return `"Unknown"`.
pub fn track_name_from_id(track_id: i8) -> String {
    const NAMES: &[&str] = &[
        "Melbourne",         // 0
        "Paul Ricard",       // 1
        "Shanghai",          // 2
        "Sakhir (Bahrain)",  // 3
        "Catalunya",         // 4
        "Monaco",            // 5
        "Montreal",          // 6
        "Silverstone",       // 7
        "Hockenheim",        // 8
        "Hungaroring",       // 9
        "Spa",               // 10
        "Monza",             // 11
        "Singapore",         // 12
        "Suzuka",            // 13
        "Abu Dhabi",         // 14
        "Texas",             // 15
        "Brazil",            // 16
        "Austria",           // 17
        "Sochi",             // 18
        "Mexico",            // 19
        "Baku (Azerbaijan)", // 20
        "Sakhir Short",      // 21
        "Silverstone Short", // 22
        "Texas Short",       // 23
        "Suzuka Short",      // 24
        "Hanoi",             // 25
        "Zandvoort",         // 26
        "Imola",             // 27
        "Portimao",          // 28
        "Jeddah",            // 29
        "Miami",             // 30
        "Las Vegas",         // 31
        "Losail",            // 32
    ];
    let idx = usize::try_from(track_id.max(0)).unwrap_or(0);
    NAMES.get(idx).copied().unwrap_or("Unknown").to_string()
}

/// Returns the human-readable tyre compound name for a compound code.
pub fn tyre_compound_name(compound: u8) -> &'static str {
    match compound {
        7 => "Intermediate",
        8 => "Wet",
        9 => "Dry (dev)",
        10 => "Wet (dev)",
        11 => "Super Soft",
        12 => "Soft",
        13 => "Medium",
        14 => "Hard",
        15 => "Wet",
        16 => "C5",
        17 => "C4",
        18 => "C3",
        19 => "C2",
        20 => "C1",
        21 => "C0",
        _ => "Unknown",
    }
}

// ── Parsed packet structs ─────────────────────────────────────────────────────

/// Parsed fields from the 29-byte PacketHeader.
#[derive(Debug, Clone)]
pub struct PacketHeader {
    pub packet_format: u16,
    pub packet_id: u8,
    pub player_car_index: u8,
}

/// Telemetry data for a single car (from packet ID 6).
#[derive(Debug, Clone)]
pub struct CarTelemetryData {
    /// Speed in km/h.
    pub speed_kmh: u16,
    /// Throttle position 0.0–1.0.
    pub throttle: f32,
    /// Steering input −1.0–1.0.
    pub steer: f32,
    /// Brake position 0.0–1.0.
    pub brake: f32,
    /// Gear: −1 = reverse, 0 = neutral, 1–8 = forward.
    pub gear: i8,
    /// Engine revolutions per minute.
    pub engine_rpm: u16,
    /// DRS deployed flag (0 = off, 1 = on).
    pub drs: u8,
    /// Brake temperatures [RL, RR, FL, FR] in °C.
    pub brakes_temperature: [u16; 4],
    /// Tyre surface temperatures [RL, RR, FL, FR] in °C.
    pub tyres_surface_temperature: [u8; 4],
    /// Tyre inner temperatures [RL, RR, FL, FR] in °C.
    pub tyres_inner_temperature: [u8; 4],
    /// Engine temperature in °C.
    pub engine_temperature: u16,
    /// Tyre pressures [RL, RR, FL, FR] in PSI.
    pub tyres_pressure: [f32; 4],
}

/// Status data for a single car (from packet ID 7).
#[derive(Debug, Clone)]
pub struct CarStatusData {
    /// Traction control setting.
    pub traction_control: u8,
    /// ABS setting.
    pub anti_lock_brakes: u8,
    /// Pit-limiter active (1 = on).
    pub pit_limiter_status: u8,
    /// Fuel remaining in kg.
    pub fuel_in_tank: f32,
    /// Estimated laps of fuel remaining.
    pub fuel_remaining_laps: f32,
    /// Maximum engine RPM.
    pub max_rpm: u16,
    /// DRS allowed this lap (1 = yes).
    pub drs_allowed: u8,
    /// Actual tyre compound code.
    pub actual_tyre_compound: u8,
    /// Laps since tyres were fitted.
    pub tyre_age_laps: u8,
    /// ICE power in Watts.
    pub engine_power_ice: f32,
    /// MGU-K power in Watts.
    pub engine_power_mguk: f32,
    /// ERS store energy in Joules.
    pub ers_store_energy: f32,
    /// ERS deployment mode.
    pub ers_deploy_mode: u8,
    /// ERS harvested by MGU-K this lap in Joules.
    pub ers_harvested_mguk: f32,
    /// ERS harvested by MGU-H this lap in Joules.
    pub ers_harvested_mguh: f32,
    /// ERS energy deployed this lap in Joules.
    pub ers_deployed: f32,
}

/// Session-level data (from packet ID 1, limited fields only).
#[derive(Debug, Clone, Default)]
pub struct SessionData {
    pub track_id: i8,
    pub session_type: u8,
    pub track_temperature: i8,
    pub air_temperature: i8,
}

/// Combined mutable state stored between UDP packets in `start_monitoring`.
#[derive(Debug, Default)]
pub struct F125State {
    pub latest_telemetry: Option<CarTelemetryData>,
    pub latest_status: Option<CarStatusData>,
    pub session: SessionData,
}

// ── Adapter struct ────────────────────────────────────────────────────────────

/// Native F1 25 UDP telemetry adapter.
///
/// Listens for EA F1 25 binary UDP packets and emits [`NormalizedTelemetry`]
/// frames once both a Car Telemetry packet (ID 6) and a Car Status packet
/// (ID 7) have been received for the player's car.
#[derive(Clone)]
pub struct F1_25Adapter {
    bind_port: u16,
    update_rate: Duration,
    heartbeat_timeout: Duration,
    last_packet_ns: Arc<AtomicU64>,
}

impl Default for F1_25Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl F1_25Adapter {
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

    /// Process a raw UDP packet, updating `state`.
    ///
    /// Returns `Some(NormalizedTelemetry)` when both telemetry and status data
    /// are available after processing the packet.  Returns `Ok(None)` for
    /// session updates or packets that don't yet complete the picture.
    pub fn process_packet(
        state: &mut F125State,
        raw: &[u8],
    ) -> Result<Option<NormalizedTelemetry>> {
        let header = parse_header(raw)?;
        if header.packet_format != PACKET_FORMAT_2025 {
            return Err(anyhow!(
                "F1 25: unexpected packet format {} (expected {})",
                header.packet_format,
                PACKET_FORMAT_2025
            ));
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
                let status = parse_car_status(raw, player)?;
                state.latest_status = Some(status);
                Ok(Self::maybe_emit(state))
            }
            other => {
                debug!(packet_id = other, "F1 25 ignoring unrecognised packet id");
                Ok(None)
            }
        }
    }

    fn maybe_emit(state: &F125State) -> Option<NormalizedTelemetry> {
        match (&state.latest_telemetry, &state.latest_status) {
            (Some(t), Some(s)) => Some(normalize(t, s, &state.session)),
            _ => None,
        }
    }
}

// ── TelemetryAdapter impl ─────────────────────────────────────────────────────

#[async_trait]
impl TelemetryAdapter for F1_25Adapter {
    fn game_id(&self) -> &str {
        "f1_25"
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
                    warn!(error = %err, port = bind_port, "F1 25 UDP socket bind failed");
                    return;
                }
            };
            info!(port = bind_port, "F1 25 UDP adapter bound");

            let mut state = F125State::default();
            let mut frame_seq = 0u64;
            let mut buf = vec![0u8; MAX_PACKET_BYTES];
            let timeout = update_rate * 4;

            loop {
                let recv_result = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
                let len = match recv_result {
                    Ok(Ok(n)) => n,
                    Ok(Err(err)) => {
                        warn!(error = %err, "F1 25 UDP receive error");
                        continue;
                    }
                    Err(_) => {
                        debug!("F1 25 UDP receive timeout");
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
                        warn!(error = %err, len, "F1 25 packet decode failed");
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    /// Normalise a single raw packet.
    ///
    /// Accepts a Car Telemetry (ID 6) packet and produces normalised output
    /// using default CarStatus values.  Returns an error for invalid or
    /// non-telemetry packets.
    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        let header = parse_header(raw)?;
        if header.packet_format != PACKET_FORMAT_2025 {
            return Err(anyhow!(
                "F1 25: unexpected packet format {} in normalize()",
                header.packet_format
            ));
        }
        let player = usize::from(header.player_car_index);
        match header.packet_id {
            PACKET_ID_CAR_TELEMETRY => {
                let telem = parse_car_telemetry(raw, player)?;
                let status = CarStatusData::default_for_normalize();
                Ok(normalize(&telem, &status, &SessionData::default()))
            }
            PACKET_ID_CAR_STATUS => {
                // Cannot produce speed/gear without telemetry.
                let _ = parse_car_status(raw, player)?; // validate only
                Err(anyhow!(
                    "F1 25 normalize() received CarStatus (ID 7) without preceding CarTelemetry; \
                     use process_packet() with persistent F125State for multi-packet normalisation"
                ))
            }
            PACKET_ID_SESSION => {
                let _ = parse_session_data(raw)?; // validate only
                Err(anyhow!(
                    "F1 25 normalize() received Session (ID 1); not a complete telemetry packet"
                ))
            }
            other => Err(anyhow!(
                "F1 25 normalize(): unsupported packet id {}",
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

// ── Low-level binary parsing ──────────────────────────────────────────────────

/// Cursor-style byte reader for little-endian binary data.
pub struct ByteReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn at(data: &'a [u8], offset: usize) -> Self {
        Self { data, pos: offset }
    }

    #[inline]
    pub fn u8(&mut self) -> Result<u8> {
        if self.pos >= self.data.len() {
            return Err(anyhow!("out of bounds: u8 at offset {}", self.pos));
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    #[inline]
    pub fn i8(&mut self) -> Result<i8> {
        self.u8().map(|v| v as i8)
    }

    #[inline]
    pub fn u16_le(&mut self) -> Result<u16> {
        let end = self.pos + 2;
        if end > self.data.len() {
            return Err(anyhow!("out of bounds: u16 at offset {}", self.pos));
        }
        let v = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos = end;
        Ok(v)
    }

    #[inline]
    pub fn u32_le(&mut self) -> Result<u32> {
        self.read_n4().map(u32::from_le_bytes)
    }

    #[inline]
    pub fn u64_le(&mut self) -> Result<u64> {
        let end = self.pos + 8;
        if end > self.data.len() {
            return Err(anyhow!("out of bounds: u64 at offset {}", self.pos));
        }
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.data[self.pos..end]);
        self.pos = end;
        Ok(u64::from_le_bytes(bytes))
    }

    #[inline]
    pub fn f32_le(&mut self) -> Result<f32> {
        self.read_n4()
            .map(f32::from_le_bytes)
            .map(|v| if v.is_finite() { v } else { 0.0 })
    }

    fn read_n4(&mut self) -> Result<[u8; 4]> {
        let end = self.pos + 4;
        if end > self.data.len() {
            return Err(anyhow!("out of bounds: 4-byte read at offset {}", self.pos));
        }
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.data[self.pos..end]);
        self.pos = end;
        Ok(b)
    }

    pub fn u8_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let end = self.pos + N;
        if end > self.data.len() {
            return Err(anyhow!("out of bounds: [u8; {}] at offset {}", N, self.pos));
        }
        let mut arr = [0u8; N];
        arr.copy_from_slice(&self.data[self.pos..end]);
        self.pos = end;
        Ok(arr)
    }

    pub fn u16_le_array<const N: usize>(&mut self) -> Result<[u16; N]> {
        let mut arr = [0u16; N];
        for item in arr.iter_mut() {
            *item = self.u16_le()?;
        }
        Ok(arr)
    }

    pub fn f32_le_array<const N: usize>(&mut self) -> Result<[f32; N]> {
        let mut arr = [0.0f32; N];
        for item in arr.iter_mut() {
            *item = self.f32_le()?;
        }
        Ok(arr)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        let end = self.pos + n;
        if end > self.data.len() {
            return Err(anyhow!("out of bounds: skip {} at offset {}", n, self.pos));
        }
        self.pos = end;
        Ok(())
    }
}

// ── Parse individual packet types ────────────────────────────────────────────

/// Parse the 29-byte packet header.
pub fn parse_header(raw: &[u8]) -> Result<PacketHeader> {
    if raw.len() < HEADER_SIZE {
        return Err(anyhow!(
            "F1 25 packet too short for header: {} bytes (need {})",
            raw.len(),
            HEADER_SIZE
        ));
    }
    let mut r = ByteReader::new(raw);
    let packet_format = r.u16_le()?; // 0-1
    r.skip(4)?; // gameYear, majorVersion, minorVersion, packetVersion  (2-5)
    let packet_id = r.u8()?; // 6
    r.skip(8)?; // sessionUID  (7-14)
    r.skip(4)?; // sessionTime  (15-18)
    r.skip(4)?; // frameIdentifier  (19-22)
    r.skip(4)?; // overallFrameIdentifier  (23-26)
    let player_car_index = r.u8()?; // 27
    // byte 28: secondaryPlayerCarIndex (skip)
    Ok(PacketHeader {
        packet_format,
        packet_id,
        player_car_index,
    })
}

/// Parse the telemetry entry for `player_index` from a Car Telemetry packet.
pub fn parse_car_telemetry(raw: &[u8], player_index: usize) -> Result<CarTelemetryData> {
    if raw.len() < MIN_CAR_TELEMETRY_PACKET_SIZE {
        return Err(anyhow!(
            "F1 25 CarTelemetry packet too short: {} bytes (need {})",
            raw.len(),
            MIN_CAR_TELEMETRY_PACKET_SIZE
        ));
    }
    if player_index >= NUM_CARS {
        return Err(anyhow!(
            "F1 25 player car index {} out of range (max {})",
            player_index,
            NUM_CARS - 1
        ));
    }

    let car_offset = HEADER_SIZE + player_index * CAR_TELEMETRY_ENTRY_SIZE;
    let mut r = ByteReader::at(raw, car_offset);

    let speed_kmh = r.u16_le()?; // 0-1
    let throttle = r.f32_le()?; // 2-5
    let steer = r.f32_le()?; // 6-9
    let brake = r.f32_le()?; // 10-13
    r.skip(1)?; // clutch (14)
    let gear = r.i8()?; // 15
    let engine_rpm = r.u16_le()?; // 16-17
    let drs = r.u8()?; // 18
    r.skip(1)?; // revLightsPercent (19)
    r.skip(2)?; // revLightsBitValue (20-21)
    let brakes_temperature = r.u16_le_array::<4>()?; // 22-29
    let tyres_surface_temperature = r.u8_array::<4>()?; // 30-33
    let tyres_inner_temperature = r.u8_array::<4>()?; // 34-37
    let engine_temperature = r.u16_le()?; // 38-39
    let tyres_pressure = r.f32_le_array::<4>()?; // 40-55
    // surfaceType[4] bytes 56-59 ignored

    Ok(CarTelemetryData {
        speed_kmh,
        throttle,
        steer,
        brake,
        gear,
        engine_rpm,
        drs,
        brakes_temperature,
        tyres_surface_temperature,
        tyres_inner_temperature,
        engine_temperature,
        tyres_pressure,
    })
}

/// Parse the status entry for `player_index` from a Car Status packet.
pub fn parse_car_status(raw: &[u8], player_index: usize) -> Result<CarStatusData> {
    if raw.len() < MIN_CAR_STATUS_PACKET_SIZE {
        return Err(anyhow!(
            "F1 25 CarStatus packet too short: {} bytes (need {})",
            raw.len(),
            MIN_CAR_STATUS_PACKET_SIZE
        ));
    }
    if player_index >= NUM_CARS {
        return Err(anyhow!(
            "F1 25 player car index {} out of range (max {})",
            player_index,
            NUM_CARS - 1
        ));
    }

    let car_offset = HEADER_SIZE + player_index * CAR_STATUS_ENTRY_SIZE;
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

    Ok(CarStatusData {
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

/// Parse the limited session fields used by this adapter from a Session packet.
pub fn parse_session_data(raw: &[u8]) -> Result<SessionData> {
    // Minimum needed: header (29) + weather (1) + trackTemp (1) + airTemp (1) +
    // totalLaps (1) + trackLength (2) + sessionType (1) + trackId (1) = 37 bytes
    const MIN_SESSION_SIZE: usize = HEADER_SIZE + 8;
    if raw.len() < MIN_SESSION_SIZE {
        return Err(anyhow!(
            "F1 25 Session packet too short: {} bytes (need {})",
            raw.len(),
            MIN_SESSION_SIZE
        ));
    }

    let mut r = ByteReader::at(raw, HEADER_SIZE);
    r.skip(1)?; // weather
    let track_temperature = r.i8()?; // 1
    let air_temperature = r.i8()?; // 2
    r.skip(1)?; // totalLaps
    r.skip(2)?; // trackLength
    let session_type = r.u8()?; // 6
    let track_id = r.i8()?; // 7

    Ok(SessionData {
        track_id,
        session_type,
        track_temperature,
        air_temperature,
    })
}

// ── Normalization ─────────────────────────────────────────────────────────────

/// Combine parsed car telemetry, status, and session into [`NormalizedTelemetry`].
pub fn normalize(
    telem: &CarTelemetryData,
    status: &CarStatusData,
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
            TelemetryValue::String("f1_25_native_udp".to_string()),
        )
        .build()
}

// ── Defaults for single-packet normalize() ────────────────────────────────────

impl CarStatusData {
    /// Safe all-zero defaults used when only a CarTelemetry packet is available.
    fn default_for_normalize() -> Self {
        Self {
            traction_control: 0,
            anti_lock_brakes: 0,
            pit_limiter_status: 0,
            fuel_in_tank: 0.0,
            fuel_remaining_laps: 0.0,
            max_rpm: 0,
            drs_allowed: 0,
            actual_tyre_compound: 0,
            tyre_age_laps: 0,
            engine_power_ice: 0.0,
            engine_power_mguk: 0.0,
            ers_store_energy: 0.0,
            ers_deploy_mode: 0,
            ers_harvested_mguk: 0.0,
            ers_harvested_mguh: 0.0,
            ers_deployed: 0.0,
        }
    }
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

/// Build a minimal valid Car Telemetry packet for car at `player_index`.
///
/// All cars other than `player_index` are zero-filled.
pub fn build_car_telemetry_packet(
    player_index: u8,
    speed_kmh: u16,
    gear: i8,
    engine_rpm: u16,
    throttle: f32,
    brake: f32,
    drs: u8,
    tyres_pressure: [f32; 4],
) -> Vec<u8> {
    let mut buf = build_header_bytes(PACKET_FORMAT_2025, PACKET_ID_CAR_TELEMETRY, player_index);

    // Zero-pad all 22 cars, then overwrite the player's entry.
    let total_car_bytes = NUM_CARS * CAR_TELEMETRY_ENTRY_SIZE;
    buf.extend(std::iter::repeat_n(0u8, total_car_bytes));
    let offset = HEADER_SIZE + usize::from(player_index) * CAR_TELEMETRY_ENTRY_SIZE;

    buf[offset..offset + 2].copy_from_slice(&speed_kmh.to_le_bytes());
    buf[offset + 2..offset + 6].copy_from_slice(&throttle.to_le_bytes());
    buf[offset + 6..offset + 10].copy_from_slice(&0.0f32.to_le_bytes()); // steer
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
    // tyres_pressure (40-55)
    buf[offset + 40..offset + 44].copy_from_slice(&tyres_pressure[0].to_le_bytes()); // RL
    buf[offset + 44..offset + 48].copy_from_slice(&tyres_pressure[1].to_le_bytes()); // RR
    buf[offset + 48..offset + 52].copy_from_slice(&tyres_pressure[2].to_le_bytes()); // FL
    buf[offset + 52..offset + 56].copy_from_slice(&tyres_pressure[3].to_le_bytes()); // FR

    // Trailer: mfdPanelIndex (1), mfdPanelIndexSecondary (1), suggestedGear (1)
    buf.extend_from_slice(&[0u8; 3]);
    buf
}

/// Build a minimal valid Car Status packet for car at `player_index`.
pub fn build_car_status_packet(
    player_index: u8,
    fuel_in_tank: f32,
    ers_store_energy: f32,
    drs_allowed: u8,
    pit_limiter: u8,
    actual_tyre_compound: u8,
    max_rpm: u16,
) -> Vec<u8> {
    let mut buf = build_header_bytes(PACKET_FORMAT_2025, PACKET_ID_CAR_STATUS, player_index);

    // Zero-pad all 22 cars, then overwrite the player's entry.
    let total_car_bytes = NUM_CARS * CAR_STATUS_ENTRY_SIZE;
    buf.extend(std::iter::repeat_n(0u8, total_car_bytes));
    let offset = HEADER_SIZE + usize::from(player_index) * CAR_STATUS_ENTRY_SIZE;

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
    // engine_power_ice (29-32), engine_power_mguk (33-36) stay zero
    buf[offset + 37..offset + 41].copy_from_slice(&ers_store_energy.to_le_bytes());
    // ersDeployMode (41) stays zero
    // ers fields (42-53) stay zero
    // networkPaused (54) stays zero

    buf
}

/// Build a minimal valid Session packet.
pub fn build_session_packet(
    track_id: i8,
    session_type: u8,
    track_temperature: i8,
    air_temperature: i8,
) -> Vec<u8> {
    let player_index = 0u8;
    let mut buf = build_header_bytes(PACKET_FORMAT_2025, PACKET_ID_SESSION, player_index);
    buf.push(0); // weather
    buf.push(track_temperature as u8);
    buf.push(air_temperature as u8);
    buf.push(0); // totalLaps
    buf.extend_from_slice(&0u16.to_le_bytes()); // trackLength
    buf.push(session_type);
    buf.push(track_id as u8);
    buf
}

fn build_header_bytes(packet_format: u16, packet_id: u8, player_index: u8) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SIZE);
    buf.extend_from_slice(&packet_format.to_le_bytes()); // 0-1
    buf.push(25); // gameYear  (2)
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    // ── Header parsing ──────────────────────────────────────────────────────

    #[test]
    fn parse_header_extracts_packet_id_and_player_index() -> TestResult {
        let raw = build_header_bytes(2025, 6, 3);
        let header = parse_header(&raw)?;
        assert_eq!(header.packet_format, 2025);
        assert_eq!(header.packet_id, 6);
        assert_eq!(header.player_car_index, 3);
        Ok(())
    }

    #[test]
    fn parse_header_rejects_short_buffer() {
        let result = parse_header(&[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn process_packet_rejects_wrong_format() {
        let raw = build_header_bytes(2024, PACKET_ID_CAR_TELEMETRY, 0);
        let mut state = F125State::default();
        let result = F1_25Adapter::process_packet(&mut state, &raw);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("2024"),
            "error should mention the actual format"
        );
    }

    // ── CarTelemetry packet ─────────────────────────────────────────────────

    #[test]
    fn car_telemetry_round_trip_speed_gear_rpm() -> TestResult {
        let raw = build_car_telemetry_packet(
            0,                        // player_index
            200,                      // speed_kmh
            6,                        // gear
            14000,                    // engine_rpm
            0.85,                     // throttle
            0.0,                      // brake
            1,                        // drs active
            [24.5, 24.5, 24.0, 24.0], // tyre pressures
        );
        let telem = parse_car_telemetry(&raw, 0)?;
        assert_eq!(telem.speed_kmh, 200);
        assert_eq!(telem.gear, 6);
        assert_eq!(telem.engine_rpm, 14000);
        assert!((telem.throttle - 0.85).abs() < 1e-5, "throttle mismatch");
        assert_eq!(telem.drs, 1);
        assert!(
            (telem.tyres_pressure[2] - 24.0).abs() < 1e-4,
            "FL pressure mismatch"
        );
        Ok(())
    }

    #[test]
    fn car_telemetry_reverse_gear_is_negative_one() -> TestResult {
        let raw = build_car_telemetry_packet(0, 0, -1, 0, 0.0, 0.0, 0, [20.0; 4]);
        let telem = parse_car_telemetry(&raw, 0)?;
        assert_eq!(telem.gear, -1);
        Ok(())
    }

    #[test]
    fn car_telemetry_rejects_short_packet() {
        let result = parse_car_telemetry(&[0u8; 100], 0);
        assert!(result.is_err());
    }

    #[test]
    fn car_telemetry_rejects_out_of_range_player_index() -> TestResult {
        let raw = build_car_telemetry_packet(0, 100, 3, 8000, 0.5, 0.0, 0, [20.0; 4]);
        let result = parse_car_telemetry(&raw, 22); // 22 is out of range
        assert!(result.is_err());
        Ok(())
    }

    // ── CarStatus packet ────────────────────────────────────────────────────

    #[test]
    fn car_status_round_trip_fuel_ers_drs() -> TestResult {
        let raw = build_car_status_packet(
            0,           // player_index
            25.0,        // fuel_in_tank kg
            2_500_000.0, // ers_store_energy J
            1,           // drs_allowed
            0,           // pit_limiter
            17,          // tyre compound (C4)
            15000,       // max_rpm
        );
        let status = parse_car_status(&raw, 0)?;
        assert!((status.fuel_in_tank - 25.0).abs() < 1e-4);
        assert!((status.ers_store_energy - 2_500_000.0).abs() < 1.0);
        assert_eq!(status.drs_allowed, 1);
        assert_eq!(status.actual_tyre_compound, 17);
        assert_eq!(status.max_rpm, 15000);
        Ok(())
    }

    #[test]
    fn car_status_rejects_short_packet() {
        let result = parse_car_status(&[0u8; 100], 0);
        assert!(result.is_err());
    }

    // ── Session packet ──────────────────────────────────────────────────────

    #[test]
    fn session_packet_round_trip_track_and_temperatures() -> TestResult {
        let raw = build_session_packet(10 /* Spa */, 3 /* Race */, 35, 28);
        let session = parse_session_data(&raw)?;
        assert_eq!(session.track_id, 10);
        assert_eq!(session.session_type, 3);
        assert_eq!(session.track_temperature, 35);
        assert_eq!(session.air_temperature, 28);
        Ok(())
    }

    #[test]
    fn session_packet_rejects_short_buffer() {
        let result = parse_session_data(&[0u8; 10]);
        assert!(result.is_err());
    }

    // ── Normalization ───────────────────────────────────────────────────────

    #[test]
    fn normalize_speed_converted_from_kmh_to_ms() -> TestResult {
        let telem = CarTelemetryData {
            speed_kmh: 180,
            throttle: 0.9,
            steer: 0.0,
            brake: 0.0,
            gear: 7,
            engine_rpm: 13000,
            drs: 0,
            brakes_temperature: [300, 300, 300, 300],
            tyres_surface_temperature: [90, 90, 90, 90],
            tyres_inner_temperature: [110, 110, 110, 110],
            engine_temperature: 110,
            tyres_pressure: [22.0, 22.0, 23.0, 23.0],
        };
        let status = CarStatusData::default_for_normalize();
        let session = SessionData::default();

        let nt = normalize(&telem, &status, &session);

        let expected_ms = 180.0 / 3.6;
        let actual_ms = nt.speed_ms;
        assert!(
            (actual_ms - expected_ms).abs() < 0.01,
            "speed mismatch: {actual_ms} ≠ {expected_ms}"
        );
        assert_eq!(nt.gear, 7);
        assert_eq!(nt.rpm, 13000.0);
        Ok(())
    }

    #[test]
    fn normalize_drs_flag_propagates_to_extended_and_flags() -> TestResult {
        let mut telem = CarTelemetryData {
            speed_kmh: 100,
            throttle: 1.0,
            steer: 0.0,
            brake: 0.0,
            gear: 5,
            engine_rpm: 12000,
            drs: 1,
            brakes_temperature: [200; 4],
            tyres_surface_temperature: [80; 4],
            tyres_inner_temperature: [100; 4],
            engine_temperature: 100,
            tyres_pressure: [21.0; 4],
        };
        let mut status = CarStatusData::default_for_normalize();
        status.drs_allowed = 1;
        let session = SessionData::default();

        let nt = normalize(&telem, &status, &session);

        assert!(nt.flags.drs_active, "flags.drs_active should be set");
        assert!(nt.flags.drs_available, "flags.drs_available should be set");
        assert_eq!(
            nt.extended.get("drs_active"),
            Some(&TelemetryValue::Boolean(true))
        );

        // DRS off
        telem.drs = 0;
        let nt2 = normalize(&telem, &status, &session);
        assert!(!nt2.flags.drs_active);
        assert_eq!(
            nt2.extended.get("drs_active"),
            Some(&TelemetryValue::Boolean(false))
        );
        Ok(())
    }

    #[test]
    fn normalize_pit_limiter_sets_both_flags() -> TestResult {
        let telem = CarTelemetryData {
            speed_kmh: 80,
            throttle: 0.3,
            steer: 0.0,
            brake: 0.0,
            gear: 3,
            engine_rpm: 6000,
            drs: 0,
            brakes_temperature: [100; 4],
            tyres_surface_temperature: [70; 4],
            tyres_inner_temperature: [80; 4],
            engine_temperature: 95,
            tyres_pressure: [21.0; 4],
        };
        let mut status = CarStatusData::default_for_normalize();
        status.pit_limiter_status = 1;
        let nt = normalize(&telem, &status, &SessionData::default());

        assert!(nt.flags.pit_limiter);
        assert!(nt.flags.in_pits);
        Ok(())
    }

    #[test]
    fn normalize_ers_fraction_clamped_zero_to_one() -> TestResult {
        let telem = CarTelemetryData {
            speed_kmh: 0,
            throttle: 0.0,
            steer: 0.0,
            brake: 0.0,
            gear: 1,
            engine_rpm: 0,
            drs: 0,
            brakes_temperature: [0; 4],
            tyres_surface_temperature: [0; 4],
            tyres_inner_temperature: [0; 4],
            engine_temperature: 0,
            tyres_pressure: [20.0; 4],
        };
        let mut status = CarStatusData::default_for_normalize();
        // Over-full ERS (should clamp to 1.0)
        status.ers_store_energy = 5_000_000.0;
        let nt = normalize(&telem, &status, &SessionData::default());
        match nt.extended.get("ers_store_fraction") {
            Some(TelemetryValue::Float(f)) => {
                assert!(
                    *f <= 1.0 && *f >= 0.0,
                    "ers_store_fraction out of [0,1]: {f}"
                );
            }
            other => {
                return Err(format!("unexpected value for ers_store_fraction: {other:?}").into());
            }
        }
        Ok(())
    }

    #[test]
    fn normalize_tyre_compound_name_c4() -> TestResult {
        let mut status = CarStatusData::default_for_normalize();
        status.actual_tyre_compound = 17; // C4
        let telem = CarTelemetryData {
            speed_kmh: 0,
            throttle: 0.0,
            steer: 0.0,
            brake: 0.0,
            gear: 1,
            engine_rpm: 0,
            drs: 0,
            brakes_temperature: [0; 4],
            tyres_surface_temperature: [0; 4],
            tyres_inner_temperature: [0; 4],
            engine_temperature: 0,
            tyres_pressure: [22.5; 4],
        };
        let nt = normalize(&telem, &status, &SessionData::default());
        assert_eq!(
            nt.extended.get("tyre_compound_name"),
            Some(&TelemetryValue::String("C4".to_string()))
        );
        Ok(())
    }

    #[test]
    fn normalize_track_id_maps_to_spa() -> TestResult {
        let telem = CarTelemetryData {
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
            tyres_pressure: [20.0; 4],
        };
        let session = SessionData {
            track_id: 10,
            ..Default::default()
        };
        let nt = normalize(&telem, &CarStatusData::default_for_normalize(), &session);
        assert_eq!(nt.track_id, Some("Spa".to_string()));
        Ok(())
    }

    // ── process_packet integration ──────────────────────────────────────────

    #[test]
    fn process_packet_emits_after_both_telemetry_and_status() -> TestResult {
        let mut state = F125State::default();

        // First: CarTelemetry — no emission yet
        let telem_pkt = build_car_telemetry_packet(0, 150, 5, 11500, 0.7, 0.0, 0, [22.0; 4]);
        let result1 = F1_25Adapter::process_packet(&mut state, &telem_pkt)?;
        assert!(result1.is_none(), "should not emit without CarStatus");

        // Second: CarStatus — should now emit
        let status_pkt = build_car_status_packet(0, 20.0, 3_000_000.0, 1, 0, 18, 15000);
        let result2 = F1_25Adapter::process_packet(&mut state, &status_pkt)?;
        let nt = result2.ok_or("should emit after both packets")?;

        let speed_ms = nt.speed_ms;
        assert!((speed_ms - 150.0 / 3.6).abs() < 0.01);
        assert_eq!(nt.gear, 5);
        Ok(())
    }

    #[test]
    fn process_packet_session_updates_state_but_no_emit() -> TestResult {
        let mut state = F125State::default();
        let session_pkt = build_session_packet(11 /* Monza */, 3, 32, 25);
        let result = F1_25Adapter::process_packet(&mut state, &session_pkt)?;
        assert!(result.is_none());
        assert_eq!(state.session.track_id, 11);
        Ok(())
    }

    #[test]
    fn process_packet_session_track_reflected_in_normalized() -> TestResult {
        let mut state = F125State::default();

        // Session packet → Monza
        let session_pkt = build_session_packet(11, 3, 32, 25);
        F1_25Adapter::process_packet(&mut state, &session_pkt)?;

        // Car telemetry
        let telem_pkt = build_car_telemetry_packet(0, 100, 4, 10000, 0.5, 0.0, 0, [21.0; 4]);
        F1_25Adapter::process_packet(&mut state, &telem_pkt)?;

        // Car status → triggers emission
        let status_pkt = build_car_status_packet(0, 15.0, 2_000_000.0, 0, 0, 13, 14000);
        let nt = F1_25Adapter::process_packet(&mut state, &status_pkt)?.ok_or("should emit")?;

        assert_eq!(nt.track_id, Some("Monza".to_string()));
        Ok(())
    }

    // ── normalize() on the adapter (single-packet) ──────────────────────────

    #[test]
    fn adapter_normalize_accepts_car_telemetry_packet() -> TestResult {
        let adapter = F1_25Adapter::new();
        let raw = build_car_telemetry_packet(0, 200, 7, 14500, 1.0, 0.0, 1, [23.0; 4]);
        let nt = adapter.normalize(&raw)?;
        let speed_ms = nt.speed_ms;
        assert!((speed_ms - 200.0 / 3.6).abs() < 0.01);
        assert_eq!(nt.gear, 7);
        assert!(nt.flags.drs_active);
        Ok(())
    }

    #[test]
    fn adapter_normalize_rejects_car_status_only_packet() -> TestResult {
        let adapter = F1_25Adapter::new();
        let raw = build_car_status_packet(0, 20.0, 1_000_000.0, 1, 0, 17, 15000);
        let result = adapter.normalize(&raw);
        assert!(
            result.is_err(),
            "status-only packet should fail normalize()"
        );
        Ok(())
    }

    #[test]
    fn adapter_normalize_rejects_short_packet() {
        let adapter = F1_25Adapter::new();
        let result = adapter.normalize(&[0u8; 10]);
        assert!(result.is_err());
    }

    // ── Adapter metadata ────────────────────────────────────────────────────

    #[test]
    fn adapter_game_id_is_f1_25() {
        let adapter = F1_25Adapter::new();
        assert_eq!(adapter.game_id(), "f1_25");
    }

    #[test]
    fn adapter_update_rate_is_16ms() {
        let adapter = F1_25Adapter::new();
        assert_eq!(
            adapter.expected_update_rate(),
            std::time::Duration::from_millis(16)
        );
    }

    // ── Track / compound helpers ────────────────────────────────────────────

    #[test]
    fn track_name_known_values() {
        assert_eq!(track_name_from_id(0), "Melbourne");
        assert_eq!(track_name_from_id(5), "Monaco");
        assert_eq!(track_name_from_id(10), "Spa");
        assert_eq!(track_name_from_id(11), "Monza");
        assert_eq!(track_name_from_id(29), "Jeddah");
        assert_eq!(track_name_from_id(32), "Losail");
    }

    #[test]
    fn track_name_negative_returns_melbourne() {
        // Negative track_id is treated as index 0 (Melbourne).
        assert_eq!(track_name_from_id(-1), "Melbourne");
    }

    #[test]
    fn track_name_out_of_range_returns_unknown() {
        assert_eq!(track_name_from_id(100), "Unknown");
    }

    #[test]
    fn tyre_compound_codes() {
        assert_eq!(tyre_compound_name(16), "C5");
        assert_eq!(tyre_compound_name(17), "C4");
        assert_eq!(tyre_compound_name(18), "C3");
        assert_eq!(tyre_compound_name(7), "Intermediate");
        assert_eq!(tyre_compound_name(8), "Wet");
        assert_eq!(tyre_compound_name(0), "Unknown");
    }

    // ── ByteReader edge cases ───────────────────────────────────────────────

    #[test]
    fn byte_reader_u16_le() -> TestResult {
        let data = [0x01u8, 0x00]; // 1 in LE
        let mut r = ByteReader::new(&data);
        assert_eq!(r.u16_le()?, 1);
        Ok(())
    }

    #[test]
    fn byte_reader_out_of_bounds_errors() {
        let data = [0x01u8];
        let mut r = ByteReader::new(&data);
        assert!(r.u16_le().is_err());
    }

    #[test]
    fn byte_reader_f32_le_round_trip() -> TestResult {
        let val = 1.234_567_f32;
        let bytes = val.to_le_bytes();
        let mut r = ByteReader::new(&bytes);
        let recovered = r.f32_le()?;
        assert!((recovered - val).abs() < 1e-6);
        Ok(())
    }

    // ── Snapshot-style golden assertions ────────────────────────────────────

    /// Verifies that a realistic lap snapshot produces a stable, fully-populated
    /// NormalizedTelemetry.  Update the expected values intentionally if the
    /// normalisation logic changes.
    #[test]
    fn golden_normalize_spa_race_snapshot() -> TestResult {
        let car_telem = CarTelemetryData {
            speed_kmh: 310,
            throttle: 0.98,
            steer: 0.05,
            brake: 0.0,
            gear: 8,
            engine_rpm: 15000,
            drs: 1,
            brakes_temperature: [450, 450, 480, 480],
            tyres_surface_temperature: [95, 95, 98, 98],
            tyres_inner_temperature: [115, 115, 118, 118],
            engine_temperature: 115,
            tyres_pressure: [23.5, 23.5, 24.2, 24.2],
        };
        let car_status = CarStatusData {
            traction_control: 0,
            anti_lock_brakes: 0,
            pit_limiter_status: 0,
            fuel_in_tank: 28.0,
            fuel_remaining_laps: 14.5,
            max_rpm: 15_100,
            drs_allowed: 1,
            actual_tyre_compound: 17, // C4
            tyre_age_laps: 12,
            engine_power_ice: 600_000.0,
            engine_power_mguk: 120_000.0,
            ers_store_energy: 3_200_000.0,
            ers_deploy_mode: 2,
            ers_harvested_mguk: 800_000.0,
            ers_harvested_mguh: 400_000.0,
            ers_deployed: 1_200_000.0,
        };
        let session = SessionData {
            track_id: 10,    // Spa
            session_type: 3, // Race
            track_temperature: 38,
            air_temperature: 26,
        };

        let nt = normalize(&car_telem, &car_status, &session);

        // Speed: 310 km/h → 86.11 m/s
        let speed = nt.speed_ms;
        assert!((speed - 310.0 / 3.6).abs() < 0.01, "speed={speed}");

        assert_eq!(nt.gear, 8);
        assert_eq!(nt.rpm, 15000.0);
        assert!(nt.flags.drs_active);
        assert!(nt.flags.drs_available);
        assert!(!nt.flags.pit_limiter);
        assert!(nt.flags.ers_available);
        assert_eq!(nt.track_id, Some("Spa".to_string()));

        // Extended fields
        assert_eq!(
            nt.extended.get("tyre_compound_name"),
            Some(&TelemetryValue::String("C4".to_string()))
        );
        assert_eq!(
            nt.extended.get("fuel_remaining_kg"),
            Some(&TelemetryValue::Float(28.0))
        );
        match nt.extended.get("ers_store_fraction") {
            Some(TelemetryValue::Float(f)) => {
                let expected = 3_200_000.0_f32 / 4_000_000.0;
                assert!(
                    (f - expected).abs() < 1e-4,
                    "ers_fraction={f} expected={expected}"
                );
            }
            other => return Err(format!("unexpected ers_store_fraction: {other:?}").into()),
        }
        assert_eq!(
            nt.extended.get("session_type"),
            Some(&TelemetryValue::Integer(3))
        );
        assert_eq!(
            nt.extended.get("track_temperature_c"),
            Some(&TelemetryValue::Integer(38))
        );
        assert_eq!(
            nt.extended.get("decoder_type"),
            Some(&TelemetryValue::String("f1_25_native_udp".to_string()))
        );

        Ok(())
    }

    // ── Property-style tests (deterministic multi-case checks) ──────────────

    #[test]
    fn speed_conversion_is_monotone() {
        for kmh in [0u16, 50, 100, 150, 200, 250, 300, 350] {
            let telem = CarTelemetryData {
                speed_kmh: kmh,
                throttle: 0.0,
                steer: 0.0,
                brake: 0.0,
                gear: 1,
                engine_rpm: 5000,
                drs: 0,
                brakes_temperature: [100; 4],
                tyres_surface_temperature: [80; 4],
                tyres_inner_temperature: [90; 4],
                engine_temperature: 100,
                tyres_pressure: [22.0; 4],
            };
            let nt = normalize(
                &telem,
                &CarStatusData::default_for_normalize(),
                &SessionData::default(),
            );
            let ms = nt.speed_ms;
            let expected = f32::from(kmh) / 3.6;
            assert!(
                (ms - expected).abs() < 0.01,
                "kmh={kmh} expected={expected} got={ms}"
            );
        }
    }

    #[test]
    fn all_gear_values_are_normalised() {
        for gear in [-1i8, 0, 1, 2, 3, 4, 5, 6, 7, 8] {
            let telem = CarTelemetryData {
                speed_kmh: 100,
                throttle: 0.5,
                steer: 0.0,
                brake: 0.0,
                gear,
                engine_rpm: 8000,
                drs: 0,
                brakes_temperature: [200; 4],
                tyres_surface_temperature: [85; 4],
                tyres_inner_temperature: [95; 4],
                engine_temperature: 105,
                tyres_pressure: [22.0; 4],
            };
            let nt = normalize(
                &telem,
                &CarStatusData::default_for_normalize(),
                &SessionData::default(),
            );
            assert_eq!(nt.gear, gear, "gear={gear} not preserved");
        }
    }

    #[test]
    fn fuel_remaining_present_and_non_negative() -> TestResult {
        let telem = CarTelemetryData {
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
            tyres_pressure: [20.0; 4],
        };
        for fuel in [0.0f32, 5.0, 15.0, 50.0, 110.0] {
            let mut status = CarStatusData::default_for_normalize();
            status.fuel_in_tank = fuel;
            let nt = normalize(&telem, &status, &SessionData::default());
            match nt.extended.get("fuel_remaining_kg") {
                Some(TelemetryValue::Float(f)) => assert!(*f >= 0.0 && (*f - fuel).abs() < 1e-4),
                other => return Err(format!("unexpected fuel value: {other:?}").into()),
            }
        }
        Ok(())
    }

    // ── Parsing performance (< 1ms) ─────────────────────────────────────────

    #[test]
    fn parse_car_telemetry_completes_within_1ms() -> TestResult {
        use std::time::Instant;
        let raw = build_car_telemetry_packet(0, 200, 6, 14000, 0.9, 0.0, 1, [23.0; 4]);
        let start = Instant::now();
        let result = parse_car_telemetry(&raw, 0);
        let elapsed = start.elapsed();
        assert!(result.is_ok());
        assert!(elapsed.as_millis() < 1, "parse took {:?} > 1ms", elapsed);
        Ok(())
    }

    #[test]
    fn normalize_completes_within_1ms() -> TestResult {
        use std::time::Instant;
        let telem = CarTelemetryData {
            speed_kmh: 250,
            throttle: 0.8,
            steer: -0.1,
            brake: 0.0,
            gear: 7,
            engine_rpm: 13000,
            drs: 1,
            brakes_temperature: [350; 4],
            tyres_surface_temperature: [90; 4],
            tyres_inner_temperature: [110; 4],
            engine_temperature: 110,
            tyres_pressure: [23.0; 4],
        };
        let status = CarStatusData::default_for_normalize();
        let session = SessionData::default();
        let start = Instant::now();
        let _ = normalize(&telem, &status, &session);
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 1,
            "normalize took {:?} > 1ms",
            elapsed
        );
        Ok(())
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        #[test]
        fn parse_no_panic_on_arbitrary(
            data in proptest::collection::vec(any::<u8>(), 0..1024)
        ) {
            let adapter = F1_25Adapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}
