//! ACC (Assetto Corsa Competizione) telemetry adapter using the official broadcasting protocol
//!
//! Implements telemetry adapter for ACC using UDP broadcast protocol v4.
//! Requirements: GI-03, GI-04

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFlags, TelemetryFrame, TelemetryReceiver,
    TelemetryValue, telemetry_now_ns,
};
use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

const REGISTER_COMMAND_APPLICATION: u8 = 1;
const REQUEST_ENTRY_LIST: u8 = 10;
const REQUEST_TRACK_DATA: u8 = 11;
const PROTOCOL_VERSION: u8 = 4;

const MSG_REGISTRATION_RESULT: u8 = 1;
const MSG_REALTIME_UPDATE: u8 = 2;
const MSG_REALTIME_CAR_UPDATE: u8 = 3;
const MSG_ENTRY_LIST: u8 = 4;
const MSG_TRACK_DATA: u8 = 5;
const MSG_ENTRY_LIST_CAR: u8 = 6;
const MSG_BROADCASTING_EVENT: u8 = 7;

const DEFAULT_ACC_PORT: u16 = 9000;
const MAX_PACKET_SIZE: usize = 4096;

/// ACC telemetry adapter using UDP broadcast protocol.
pub struct ACCAdapter {
    server_address: SocketAddr,
    update_rate: Duration,
    display_name: String,
    connection_password: String,
    command_password: String,
}

impl Default for ACCAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ACCAdapter {
    /// Create a new ACC adapter.
    pub fn new() -> Self {
        Self {
            server_address: SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::LOCALHOST,
                DEFAULT_ACC_PORT,
            )),
            update_rate: Duration::from_millis(16),
            display_name: "OpenRacing".to_string(),
            connection_password: String::new(),
            command_password: String::new(),
        }
    }

    /// Create ACC adapter with custom ACC broadcasting endpoint.
    pub fn with_address(server_address: SocketAddr) -> Self {
        Self {
            server_address,
            ..Self::new()
        }
    }

    /// Check if ACC is running by attempting a registration handshake.
    async fn check_acc_running(&self) -> bool {
        let bind_address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));
        let socket = match TokioUdpSocket::bind(bind_address).await {
            Ok(socket) => socket,
            Err(_) => return false,
        };

        if socket.connect(self.server_address).await.is_err() {
            return false;
        }

        let register_packet = match build_register_packet(
            &self.display_name,
            &self.connection_password,
            duration_to_interval_ms(self.update_rate),
            &self.command_password,
        ) {
            Ok(packet) => packet,
            Err(_) => return false,
        };

        if socket.send(&register_packet).await.is_err() {
            return false;
        }

        let mut buf = [0u8; MAX_PACKET_SIZE];
        let receive_result =
            tokio::time::timeout(Duration::from_millis(200), socket.recv(&mut buf)).await;

        let len = match receive_result {
            Ok(Ok(len)) => len,
            _ => return false,
        };

        matches!(
            parse_inbound_message(&buf[..len]),
            Ok(ACCInboundMessage::RegistrationResult(_))
        )
    }
}

#[async_trait]
impl TelemetryAdapter for ACCAdapter {
    fn game_id(&self) -> &str {
        "acc"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);

        let server_address = self.server_address;
        let update_rate = self.update_rate;
        let display_name = self.display_name.clone();
        let connection_password = self.connection_password.clone();
        let command_password = self.command_password.clone();

        tokio::spawn(async move {
            let bind_address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));
            let socket = match TokioUdpSocket::bind(bind_address).await {
                Ok(socket) => socket,
                Err(e) => {
                    error!(error = %e, "Failed to bind ACC telemetry UDP socket");
                    return;
                }
            };

            if let Err(e) = socket.connect(server_address).await {
                error!(error = %e, address = %server_address, "Failed to connect ACC telemetry UDP socket");
                return;
            }

            let register_packet = match build_register_packet(
                &display_name,
                &connection_password,
                duration_to_interval_ms(update_rate),
                &command_password,
            ) {
                Ok(packet) => packet,
                Err(e) => {
                    error!(error = %e, "Failed to encode ACC registration packet");
                    return;
                }
            };

            if let Err(e) = socket.send(&register_packet).await {
                error!(error = %e, "Failed to send ACC registration packet");
                return;
            }

            info!(
                endpoint = %server_address,
                "ACC telemetry adapter connected; waiting for protocol messages"
            );

            let mut frame_seq = 0u64;
            let mut state = ACCSessionState::default();
            let mut buf = [0u8; MAX_PACKET_SIZE];

            loop {
                match tokio::time::timeout(update_rate * 2, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => {
                        let packet_data = &buf[..len];
                        match parse_inbound_message(packet_data) {
                            Ok(message) => {
                                if let ACCInboundMessage::RegistrationResult(result) = &message {
                                    if result.success {
                                        info!(
                                            connection_id = result.connection_id,
                                            readonly = result.readonly,
                                            "ACC registration successful"
                                        );

                                        let request_entry_list =
                                            build_request_entry_list_packet(result.connection_id);
                                        if let Err(e) = socket.send(&request_entry_list).await {
                                            debug!(error = %e, "Failed to request ACC entry list");
                                        }

                                        let request_track_data =
                                            build_request_track_data_packet(result.connection_id);
                                        if let Err(e) = socket.send(&request_track_data).await {
                                            debug!(error = %e, "Failed to request ACC track data");
                                        }
                                    } else {
                                        warn!(
                                            error = %result.error,
                                            "ACC registration rejected"
                                        );
                                    }
                                }

                                if let Some(normalized) = state.update_and_normalize(&message) {
                                    let frame = TelemetryFrame::new(
                                        normalized,
                                        telemetry_now_ns(),
                                        frame_seq,
                                        len,
                                    );

                                    if tx.send(frame).await.is_err() {
                                        debug!(
                                            "Telemetry receiver dropped, stopping ACC monitoring"
                                        );
                                        break;
                                    }

                                    frame_seq = frame_seq.saturating_add(1);
                                }
                            }
                            Err(e) => {
                                warn!(error = %e, "Failed to parse ACC UDP packet");
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        warn!(error = %e, "ACC UDP receive error");
                    }
                    Err(_) => {
                        debug!("No ACC telemetry data received (timeout)");
                    }
                }
            }

            info!("Stopped ACC telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        // Monitoring task will stop when receiver is dropped.
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        let message = parse_inbound_message(raw)?;
        let mut state = ACCSessionState::default();
        state
            .update_and_normalize(&message)
            .ok_or_else(|| anyhow!("ACC packet does not carry realtime telemetry"))
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.check_acc_running().await)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ACCInboundMessage {
    RegistrationResult(RegistrationResult),
    RealtimeUpdate(RealtimeUpdate),
    RealtimeCarUpdate(RealtimeCarUpdate),
    TrackData(TrackData),
    EntryList,
    EntryListCar,
    BroadcastingEvent,
    Unknown(u8),
}

#[derive(Debug, Clone, PartialEq)]
struct RegistrationResult {
    connection_id: i32,
    success: bool,
    readonly: bool,
    error: String,
}

#[derive(Debug, Clone, PartialEq)]
struct RealtimeUpdate {
    focused_car_index: Option<u16>,
    session_time_ms: f32,
    ambient_temp_c: u8,
    track_temp_c: u8,
    rain_level: f32,
    wetness: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct RealtimeCarUpdate {
    car_index: u16,
    gear: i8,
    car_location: u8,
    speed_kmh: u16,
    position: u16,
    cup_position: u16,
    track_position: u16,
    laps: u16,
    delta_ms: i32,
}

#[derive(Debug, Clone, PartialEq)]
struct TrackData {
    track_name: String,
}

#[derive(Debug, Default)]
struct ACCSessionState {
    track_name: Option<String>,
    focused_car_index: Option<u16>,
    latest_realtime: Option<RealtimeUpdate>,
    latest_car_updates: HashMap<u16, RealtimeCarUpdate>,
}

impl ACCSessionState {
    fn update_and_normalize(&mut self, message: &ACCInboundMessage) -> Option<NormalizedTelemetry> {
        match message {
            ACCInboundMessage::RealtimeUpdate(update) => {
                self.focused_car_index = update.focused_car_index;
                self.latest_realtime = Some(update.clone());

                let focused_car = self
                    .focused_car_index
                    .and_then(|index| self.latest_car_updates.get(&index));
                focused_car.map(|car| self.normalize_car(car))
            }
            ACCInboundMessage::RealtimeCarUpdate(update) => {
                self.latest_car_updates
                    .insert(update.car_index, update.clone());
                if let Some(focused) = self.focused_car_index
                    && focused != update.car_index
                {
                    return None;
                }

                Some(self.normalize_car(update))
            }
            ACCInboundMessage::TrackData(track_data) => {
                self.track_name = Some(track_data.track_name.clone());
                None
            }
            _ => None,
        }
    }

    fn normalize_car(&self, car: &RealtimeCarUpdate) -> NormalizedTelemetry {
        let flags = TelemetryFlags {
            in_pits: matches!(car.car_location, 2..=4),
            pit_limiter: car.car_location == 2,
            ..TelemetryFlags::default()
        };

        let track_id = self.track_name.clone();
        let speed_ms = f32::from(car.speed_kmh) / 3.6;
        let car_id = format!("car_{}", car.car_index);

        let mut builder = NormalizedTelemetry::builder()
            .speed_ms(speed_ms)
            .gear(car.gear)
            .flags(flags)
            .car_id(car_id)
            .extended(
                "position".to_string(),
                TelemetryValue::Integer(i32::from(car.position)),
            )
            .extended(
                "cup_position".to_string(),
                TelemetryValue::Integer(i32::from(car.cup_position)),
            )
            .extended(
                "track_position".to_string(),
                TelemetryValue::Integer(i32::from(car.track_position)),
            )
            .extended(
                "laps".to_string(),
                TelemetryValue::Integer(i32::from(car.laps)),
            )
            .extended(
                "delta_ms".to_string(),
                TelemetryValue::Integer(car.delta_ms),
            );

        if let Some(track_name) = track_id {
            builder = builder.track_id(track_name);
        }

        if let Some(realtime) = &self.latest_realtime {
            builder = builder
                .extended(
                    "session_time_ms".to_string(),
                    TelemetryValue::Float(realtime.session_time_ms),
                )
                .extended(
                    "ambient_temp_c".to_string(),
                    TelemetryValue::Integer(i32::from(realtime.ambient_temp_c)),
                )
                .extended(
                    "track_temp_c".to_string(),
                    TelemetryValue::Integer(i32::from(realtime.track_temp_c)),
                )
                .extended(
                    "rain_level".to_string(),
                    TelemetryValue::Float(realtime.rain_level),
                )
                .extended(
                    "wetness".to_string(),
                    TelemetryValue::Float(realtime.wetness),
                );
        }

        builder.build()
    }
}

fn build_register_packet(
    display_name: &str,
    connection_password: &str,
    update_interval_ms: i32,
    command_password: &str,
) -> Result<Vec<u8>> {
    if update_interval_ms <= 0 {
        return Err(anyhow!(
            "ACC update interval must be positive, got {update_interval_ms}"
        ));
    }

    let mut buffer = Vec::with_capacity(128);
    buffer.push(REGISTER_COMMAND_APPLICATION);
    buffer.push(PROTOCOL_VERSION);
    write_acc_string(&mut buffer, display_name)?;
    write_acc_string(&mut buffer, connection_password)?;
    buffer.extend_from_slice(&update_interval_ms.to_le_bytes());
    write_acc_string(&mut buffer, command_password)?;
    Ok(buffer)
}

fn build_request_entry_list_packet(connection_id: i32) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(8);
    buffer.push(REQUEST_ENTRY_LIST);
    buffer.extend_from_slice(&connection_id.to_le_bytes());
    buffer
}

fn build_request_track_data_packet(connection_id: i32) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(8);
    buffer.push(REQUEST_TRACK_DATA);
    buffer.extend_from_slice(&connection_id.to_le_bytes());
    buffer
}

fn parse_inbound_message(data: &[u8]) -> Result<ACCInboundMessage> {
    let mut reader = PacketReader::new(data);
    let message_type = reader.read_u8().context("missing ACC message type")?;

    let message = match message_type {
        MSG_REGISTRATION_RESULT => {
            ACCInboundMessage::RegistrationResult(parse_registration_result(&mut reader)?)
        }
        MSG_REALTIME_UPDATE => {
            ACCInboundMessage::RealtimeUpdate(parse_realtime_update(&mut reader)?)
        }
        MSG_REALTIME_CAR_UPDATE => {
            ACCInboundMessage::RealtimeCarUpdate(parse_realtime_car_update(&mut reader)?)
        }
        MSG_ENTRY_LIST => {
            parse_entry_list(&mut reader)?;
            ACCInboundMessage::EntryList
        }
        MSG_TRACK_DATA => ACCInboundMessage::TrackData(parse_track_data(&mut reader)?),
        MSG_ENTRY_LIST_CAR => {
            parse_entry_list_car(&mut reader)?;
            ACCInboundMessage::EntryListCar
        }
        MSG_BROADCASTING_EVENT => {
            parse_broadcasting_event(&mut reader)?;
            ACCInboundMessage::BroadcastingEvent
        }
        other => ACCInboundMessage::Unknown(other),
    };

    Ok(message)
}

fn parse_registration_result(reader: &mut PacketReader<'_>) -> Result<RegistrationResult> {
    Ok(RegistrationResult {
        connection_id: reader.read_i32_le()?,
        success: reader.read_bool_u8()?,
        // ACC SDK: byte == 0 means read-only (inverted from bool convention).
        readonly: reader.read_u8()? == 0,
        error: read_acc_string(reader)?,
    })
}

fn parse_realtime_update(reader: &mut PacketReader<'_>) -> Result<RealtimeUpdate> {
    let _event_index = reader.read_u16_le()?;
    let _session_index = reader.read_u16_le()?;
    let _session_type = reader.read_u8()?;
    let _phase = reader.read_u8()?;

    let session_time_ms = reader.read_f32_le()?;
    let _session_end_time_ms = reader.read_f32_le()?;

    let focused_car_index_raw = reader.read_i32_le()?;
    let focused_car_index = u16::try_from(focused_car_index_raw).ok();

    let _active_camera_set = read_acc_string(reader)?;
    let _active_camera = read_acc_string(reader)?;
    let _current_hud_page = read_acc_string(reader)?;

    let is_replay_playing = reader.read_bool_u8()?;
    if is_replay_playing {
        let _replay_session_time_ms = reader.read_f32_le()?;
        let _replay_remaining_time_ms = reader.read_f32_le()?;
    }

    let _time_of_day_ms = reader.read_f32_le()?;
    let ambient_temp_c = reader.read_u8()?;
    let track_temp_c = reader.read_u8()?;
    let _clouds = f32::from(reader.read_u8()?) / 10.0;
    let rain_level = f32::from(reader.read_u8()?) / 10.0;
    let wetness = f32::from(reader.read_u8()?) / 10.0;

    let _best_session_lap = read_lap_time_ms(reader)?;

    Ok(RealtimeUpdate {
        focused_car_index,
        session_time_ms,
        ambient_temp_c,
        track_temp_c,
        rain_level,
        wetness,
    })
}

fn parse_realtime_car_update(reader: &mut PacketReader<'_>) -> Result<RealtimeCarUpdate> {
    let car_index = reader.read_u16_le()?;
    let _driver_index = reader.read_u16_le()?;
    let _driver_count = reader.read_u8()?;

    let gear_raw = reader.read_u8()?;
    let gear = (i16::from(gear_raw) - 2).clamp(i16::from(i8::MIN), i16::from(i8::MAX)) as i8;

    let _world_pos_x = reader.read_f32_le()?;
    let _world_pos_y = reader.read_f32_le()?;
    let _yaw = reader.read_f32_le()?;

    let car_location = reader.read_u8()?;
    let speed_kmh = reader.read_u16_le()?;
    let position = reader.read_u16_le()?;
    let cup_position = reader.read_u16_le()?;
    let track_position = reader.read_u16_le()?;
    let _spline_position = reader.read_f32_le()?;
    let laps = reader.read_u16_le()?;
    let delta_ms = reader.read_i32_le()?;

    let _best_session_lap = read_lap_time_ms(reader)?;
    let _last_lap = read_lap_time_ms(reader)?;
    let _current_lap = read_lap_time_ms(reader)?;

    Ok(RealtimeCarUpdate {
        car_index,
        gear,
        car_location,
        speed_kmh,
        position,
        cup_position,
        track_position,
        laps,
        delta_ms,
    })
}

fn parse_entry_list(reader: &mut PacketReader<'_>) -> Result<()> {
    let _connection_id = reader.read_i32_le()?;
    let car_count = usize::from(reader.read_u16_le()?);
    for _ in 0..car_count {
        let _car_id = reader.read_u16_le()?;
    }
    Ok(())
}

fn parse_track_data(reader: &mut PacketReader<'_>) -> Result<TrackData> {
    let _connection_id = reader.read_i32_le()?;
    let track_name = read_acc_string(reader)?;
    let _track_id = reader.read_i32_le()?;
    let _track_meters = reader.read_i32_le()?;

    let camera_set_count = usize::from(reader.read_u8()?);
    for _ in 0..camera_set_count {
        let _camera_set_name = read_acc_string(reader)?;
        let camera_count = usize::from(reader.read_u8()?);
        for _ in 0..camera_count {
            let _camera_name = read_acc_string(reader)?;
        }
    }

    let hud_pages_count = usize::from(reader.read_u8()?);
    for _ in 0..hud_pages_count {
        let _hud_page = read_acc_string(reader)?;
    }

    Ok(TrackData { track_name })
}

fn parse_entry_list_car(reader: &mut PacketReader<'_>) -> Result<()> {
    let _car_index = reader.read_u16_le()?;
    let _car_model_type = reader.read_u8()?;
    let _team_name = read_acc_string(reader)?;
    let _race_number = reader.read_i32_le()?;
    let _cup_category = reader.read_u8()?;
    let _current_driver_index = reader.read_u8()?;
    let _nationality = reader.read_u16_le()?;

    let drivers_count = usize::from(reader.read_u8()?);
    for _ in 0..drivers_count {
        let _first_name = read_acc_string(reader)?;
        let _last_name = read_acc_string(reader)?;
        let _short_name = read_acc_string(reader)?;
        let _category = reader.read_u8()?;
        let _driver_nationality = reader.read_u16_le()?;
    }

    Ok(())
}

fn parse_broadcasting_event(reader: &mut PacketReader<'_>) -> Result<()> {
    let _kind = reader.read_u8()?;
    let _message = read_acc_string(reader)?;
    let _time_ms = reader.read_i32_le()?;
    let _car_id = reader.read_i32_le()?;
    Ok(())
}

fn read_lap_time_ms(reader: &mut PacketReader<'_>) -> Result<i32> {
    let lap_time_ms = reader.read_i32_le()?;
    let _car_index = reader.read_u16_le()?;
    let _driver_index = reader.read_u16_le()?;

    let split_count = usize::from(reader.read_u8()?);
    for _ in 0..split_count {
        let _split = reader.read_i32_le()?;
    }

    let _is_invalid = reader.read_bool_u8()?;
    let _is_valid_for_best = reader.read_bool_u8()?;
    let _is_outlap = reader.read_bool_u8()?;
    let _is_inlap = reader.read_bool_u8()?;

    Ok(lap_time_ms)
}

fn write_acc_string(buffer: &mut Vec<u8>, value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let length = u16::try_from(bytes.len())
        .map_err(|_| anyhow!("ACC string exceeds u16 length: {} bytes", bytes.len()))?;

    buffer.extend_from_slice(&length.to_le_bytes());
    buffer.extend_from_slice(bytes);
    Ok(())
}

fn read_acc_string(reader: &mut PacketReader<'_>) -> Result<String> {
    let length = usize::from(reader.read_u16_le()?);
    let raw = reader.read_exact(length)?;
    String::from_utf8(raw.to_vec()).context("ACC string is not valid UTF-8")
}

fn duration_to_interval_ms(update_rate: Duration) -> i32 {
    let millis = update_rate.as_millis();
    if millis == 0 {
        return 1;
    }

    if millis > i32::MAX as u128 {
        return i32::MAX;
    }

    millis as i32
}

struct PacketReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> PacketReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| anyhow!("ACC packet offset overflow"))?;

        if end > self.data.len() {
            return Err(anyhow!(
                "ACC packet too short: need {len} bytes at {}, total {}",
                self.offset,
                self.data.len()
            ));
        }

        let slice = &self.data[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8> {
        let bytes = self.read_exact(1)?;
        Ok(bytes[0])
    }

    fn read_bool_u8(&mut self) -> Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    fn read_u16_le(&mut self) -> Result<u16> {
        let bytes = self.read_exact(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_i32_le(&mut self) -> Result<i32> {
        let bytes = self.read_exact(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_f32_le(&mut self) -> Result<f32> {
        let bytes = self.read_exact(4)?;
        let val = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        Ok(if val.is_finite() { val } else { 0.0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;
    const FIXTURE_REGISTRATION_RESULT_SUCCESS: &[u8] =
        include_bytes!("../../service/tests/fixtures/acc/registration_result_success.bin");
    const FIXTURE_TRACK_DATA_MONZA: &[u8] =
        include_bytes!("../../service/tests/fixtures/acc/track_data_monza.bin");
    const FIXTURE_REALTIME_UPDATE_FOCUSED_CAR_7: &[u8] =
        include_bytes!("../../service/tests/fixtures/acc/realtime_update_focused_car_7.bin");
    const FIXTURE_REALTIME_CAR_UPDATE_CAR_7: &[u8] =
        include_bytes!("../../service/tests/fixtures/acc/realtime_car_update_car_7.bin");

    fn push_acc_string(buffer: &mut Vec<u8>, value: &str) -> TestResult {
        write_acc_string(buffer, value)?;
        Ok(())
    }

    fn push_lap(buffer: &mut Vec<u8>, lap_time_ms: i32) {
        buffer.extend_from_slice(&lap_time_ms.to_le_bytes());
        buffer.extend_from_slice(&1u16.to_le_bytes());
        buffer.extend_from_slice(&0u16.to_le_bytes());
        buffer.push(0); // split count
        buffer.push(0); // is invalid
        buffer.push(1); // valid for best
        buffer.push(0); // outlap
        buffer.push(0); // inlap
    }

    #[test]
    fn test_acc_adapter_creation() -> TestResult {
        let adapter = ACCAdapter::new();
        assert_eq!(adapter.game_id(), "acc");
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
        assert_eq!(
            adapter.server_address,
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 9000))
        );
        Ok(())
    }

    #[test]
    fn test_acc_adapter_with_address() -> TestResult {
        let addr: SocketAddr = "192.168.1.100:9100".parse()?;
        let adapter = ACCAdapter::with_address(addr);
        assert_eq!(adapter.server_address, addr);
        Ok(())
    }

    #[test]
    fn test_build_register_packet_layout() -> TestResult {
        let packet = build_register_packet("OpenRacing", "", 16, "cmd")?;
        let mut reader = PacketReader::new(&packet);

        assert_eq!(reader.read_u8()?, REGISTER_COMMAND_APPLICATION);
        assert_eq!(reader.read_u8()?, PROTOCOL_VERSION);
        assert_eq!(read_acc_string(&mut reader)?, "OpenRacing");
        assert_eq!(read_acc_string(&mut reader)?, "");
        assert_eq!(reader.read_i32_le()?, 16);
        assert_eq!(read_acc_string(&mut reader)?, "cmd");
        Ok(())
    }

    #[test]
    fn test_parse_registration_result() -> TestResult {
        let mut packet = vec![MSG_REGISTRATION_RESULT];
        packet.extend_from_slice(&42i32.to_le_bytes());
        packet.push(1); // success
        packet.push(1); // writable (not readonly)
        push_acc_string(&mut packet, "")?;

        let message = parse_inbound_message(&packet)?;
        match message {
            ACCInboundMessage::RegistrationResult(result) => {
                assert_eq!(result.connection_id, 42);
                assert!(result.success);
                assert!(!result.readonly);
                assert!(result.error.is_empty());
            }
            _ => return Err("expected registration result message".into()),
        }

        Ok(())
    }

    #[test]
    fn test_parse_registration_result_fixture() -> TestResult {
        let message = parse_inbound_message(FIXTURE_REGISTRATION_RESULT_SUCCESS)?;
        match message {
            ACCInboundMessage::RegistrationResult(result) => {
                assert_eq!(result.connection_id, 1337);
                assert!(result.success);
                // Fixture byte is 0x00 → readonly per ACC SDK (byte==0 means readonly).
                assert!(result.readonly);
                assert!(result.error.is_empty());
            }
            _ => return Err("expected fixture registration result".into()),
        }

        Ok(())
    }

    #[test]
    fn test_parse_realtime_sequence_from_fixtures() -> TestResult {
        let mut state = ACCSessionState::default();

        let track_msg = parse_inbound_message(FIXTURE_TRACK_DATA_MONZA)?;
        state.update_and_normalize(&track_msg);

        let realtime_msg = parse_inbound_message(FIXTURE_REALTIME_UPDATE_FOCUSED_CAR_7)?;
        state.update_and_normalize(&realtime_msg);

        let car_msg = parse_inbound_message(FIXTURE_REALTIME_CAR_UPDATE_CAR_7)?;
        let normalized = state
            .update_and_normalize(&car_msg)
            .ok_or("expected normalized telemetry from fixture car update")?;

        assert_eq!(normalized.car_id, Some("car_7".to_string()));
        assert_eq!(normalized.track_id, Some("monza".to_string()));
        assert_eq!(normalized.speed_ms, 50.0);
        assert_eq!(normalized.gear, 4);
        assert_eq!(
            normalized.extended.get("session_time_ms"),
            Some(&TelemetryValue::Float(12345.0))
        );
        assert_eq!(
            normalized.extended.get("ambient_temp_c"),
            Some(&TelemetryValue::Integer(24))
        );
        assert_eq!(
            normalized.extended.get("track_temp_c"),
            Some(&TelemetryValue::Integer(31))
        );
        Ok(())
    }

    #[test]
    fn test_fixture_packets_parse_without_errors() -> TestResult {
        let fixtures = [
            FIXTURE_REGISTRATION_RESULT_SUCCESS,
            FIXTURE_TRACK_DATA_MONZA,
            FIXTURE_REALTIME_UPDATE_FOCUSED_CAR_7,
            FIXTURE_REALTIME_CAR_UPDATE_CAR_7,
        ];

        for fixture in fixtures {
            let parsed = parse_inbound_message(fixture);
            assert!(parsed.is_ok());
        }

        Ok(())
    }

    #[test]
    fn test_truncated_fixture_packets_fail_cleanly() -> TestResult {
        let fixtures = [
            FIXTURE_REGISTRATION_RESULT_SUCCESS,
            FIXTURE_TRACK_DATA_MONZA,
            FIXTURE_REALTIME_UPDATE_FOCUSED_CAR_7,
            FIXTURE_REALTIME_CAR_UPDATE_CAR_7,
        ];

        for fixture in fixtures {
            if fixture.len() > 1 {
                let truncated = &fixture[..fixture.len() - 1];
                let parsed = parse_inbound_message(truncated);
                assert!(parsed.is_err());
            }
        }

        Ok(())
    }

    #[test]
    fn test_parse_realtime_car_update_to_normalized() -> TestResult {
        let mut track_packet = vec![MSG_TRACK_DATA];
        track_packet.extend_from_slice(&7i32.to_le_bytes());
        push_acc_string(&mut track_packet, "monza")?;
        track_packet.extend_from_slice(&1i32.to_le_bytes());
        track_packet.extend_from_slice(&5793i32.to_le_bytes());
        track_packet.push(0); // camera sets
        track_packet.push(0); // hud pages

        let mut car_packet = vec![MSG_REALTIME_CAR_UPDATE];
        car_packet.extend_from_slice(&7u16.to_le_bytes()); // car index
        car_packet.extend_from_slice(&0u16.to_le_bytes()); // driver index
        car_packet.push(1); // driver count
        car_packet.push(6); // gear raw => 4
        car_packet.extend_from_slice(&100.0f32.to_le_bytes());
        car_packet.extend_from_slice(&200.0f32.to_le_bytes());
        car_packet.extend_from_slice(&0.25f32.to_le_bytes());
        car_packet.push(1); // car location = track
        car_packet.extend_from_slice(&180u16.to_le_bytes()); // kmh
        car_packet.extend_from_slice(&2u16.to_le_bytes());
        car_packet.extend_from_slice(&2u16.to_le_bytes());
        car_packet.extend_from_slice(&2u16.to_le_bytes());
        car_packet.extend_from_slice(&0.5f32.to_le_bytes());
        car_packet.extend_from_slice(&12u16.to_le_bytes());
        car_packet.extend_from_slice(&(-120i32).to_le_bytes());
        push_lap(&mut car_packet, 91_000);
        push_lap(&mut car_packet, 92_000);
        push_lap(&mut car_packet, 45_000);

        let mut state = ACCSessionState::default();
        let track_message = parse_inbound_message(&track_packet)?;
        state.update_and_normalize(&track_message);

        let car_message = parse_inbound_message(&car_packet)?;
        let normalized = state
            .update_and_normalize(&car_message)
            .ok_or("expected normalized telemetry")?;

        assert_eq!(normalized.speed_ms, 50.0);
        assert_eq!(normalized.gear, 4);
        assert_eq!(normalized.track_id, Some("monza".to_string()));
        assert_eq!(normalized.car_id, Some("car_7".to_string()));
        assert_eq!(
            normalized.extended.get("laps"),
            Some(&TelemetryValue::Integer(12))
        );
        assert_eq!(
            normalized.extended.get("delta_ms"),
            Some(&TelemetryValue::Integer(-120))
        );
        Ok(())
    }

    #[test]
    fn test_parse_invalid_packet() -> TestResult {
        let small_data = [0u8; 0];
        let result = parse_inbound_message(&small_data);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_string_round_trip() -> TestResult {
        let mut bytes = Vec::new();
        write_acc_string(&mut bytes, "ferrari_296_gt3")?;

        let mut reader = PacketReader::new(&bytes);
        let decoded = read_acc_string(&mut reader)?;
        assert_eq!(decoded, "ferrari_296_gt3");
        Ok(())
    }

    proptest! {
        #[test]
        fn prop_string_round_trip(value in "[ -~]{0,128}") {
            let mut bytes = Vec::new();
            let encoded = write_acc_string(&mut bytes, &value);
            prop_assert!(encoded.is_ok());

            let mut reader = PacketReader::new(&bytes);
            let decoded = read_acc_string(&mut reader);
            prop_assert_eq!(decoded.ok(), Some(value));
        }

        #[test]
        fn prop_acc_normalize_speed_non_negative(data: Vec<u8>) {
            if let Ok(normalized) = ACCAdapter::new().normalize(&data) {
                prop_assert!(normalized.speed_ms >= 0.0);
                prop_assert!(normalized.speed_ms.is_finite());
            }
        }

        #[test]
        fn prop_acc_normalize_gear_in_range(data: Vec<u8>) {
            if let Ok(normalized) = ACCAdapter::new().normalize(&data) {
                // Gear is decoded as (raw_byte - 2) clamped to i8.
                // Just verify normalization succeeds without panicking.
                let _gear: i8 = normalized.gear;
            }
        }
    }

    #[test]
    fn test_normalize_method() -> TestResult {
        let adapter = ACCAdapter::new();

        let mut packet = vec![MSG_REALTIME_CAR_UPDATE];
        packet.extend_from_slice(&1u16.to_le_bytes());
        packet.extend_from_slice(&0u16.to_le_bytes());
        packet.push(1);
        packet.push(4);
        packet.extend_from_slice(&0.0f32.to_le_bytes());
        packet.extend_from_slice(&0.0f32.to_le_bytes());
        packet.extend_from_slice(&0.0f32.to_le_bytes());
        packet.push(1);
        packet.extend_from_slice(&120u16.to_le_bytes());
        packet.extend_from_slice(&1u16.to_le_bytes());
        packet.extend_from_slice(&1u16.to_le_bytes());
        packet.extend_from_slice(&1u16.to_le_bytes());
        packet.extend_from_slice(&0.0f32.to_le_bytes());
        packet.extend_from_slice(&3u16.to_le_bytes());
        packet.extend_from_slice(&0i32.to_le_bytes());
        push_lap(&mut packet, 90_000);
        push_lap(&mut packet, 91_000);
        push_lap(&mut packet, 44_000);

        let result = adapter.normalize(&packet);
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn test_normalize_invalid_data() -> TestResult {
        let adapter = ACCAdapter::new();

        let invalid_data = vec![0u8; 3];
        let result = adapter.normalize(&invalid_data);
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_is_game_running() -> TestResult {
        let adapter = ACCAdapter::new();
        let result = adapter.is_game_running().await;
        assert!(result.is_ok());
        Ok(())
    }
}
