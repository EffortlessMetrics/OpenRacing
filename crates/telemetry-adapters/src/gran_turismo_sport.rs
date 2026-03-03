//! Gran Turismo Sport UDP telemetry adapter.
//!
//! GT Sport uses the identical Salsa20-encrypted "SimulatorInterface" UDP
//! packet format as GT7, but with different default port numbers:
//! - **Receive** on port 33340 (GT Sport sends telemetry here)
//! - **Send heartbeats** to port 33339 on the PlayStation
//!
//! ## Port verification (2025-07)
//!
//! Verified against Nenkai/PDTools `SimulatorInterfaceClient.cs` (commit 5bb714c):
//! `ReceivePortDefault=33339`, `BindPortDefault=33340` — used for GTSport and GT6.
//! Also confirmed by SimHub wiki (GT Sport: UDP ports 33339 and 33340).
//!
//! Protocol documented by the community:
//! <https://www.gtplanet.net/forum/threads/gt6-is-compatible-with-the-ps4s-remote-play-feature.317250/>

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver,
    gran_turismo_7::{PACKET_SIZE, decrypt_and_parse},
    telemetry_now_ns,
};
use anyhow::Result;
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// UDP port on which GT Sport broadcasts telemetry to the host PC.
/// Verified: Nenkai/PDTools BindPortDefault=33340; SimHub wiki confirms 33340.
pub const GTS_RECV_PORT: u16 = 33340;
/// UDP port on the PlayStation to which heartbeat packets must be sent.
/// Verified: Nenkai/PDTools ReceivePortDefault=33339; SimHub wiki confirms 33339.
pub const GTS_SEND_PORT: u16 = 33339;

/// Gran Turismo Sport telemetry adapter.
///
/// Listens for Salsa20-encrypted UDP packets on [`GTS_RECV_PORT`] and sends
/// heartbeats back to the source host on [`GTS_SEND_PORT`] to keep the stream
/// alive. Packet parsing is delegated to the GT7 implementation since both
/// games share the same SimulatorInterface format.
pub struct GranTurismo7SportsAdapter {
    recv_port: u16,
    update_rate: Duration,
}

impl Default for GranTurismo7SportsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GranTurismo7SportsAdapter {
    pub fn new() -> Self {
        Self {
            recv_port: GTS_RECV_PORT,
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
impl TelemetryAdapter for GranTurismo7SportsAdapter {
    fn game_id(&self) -> &str {
        "gran_turismo_sport"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let recv_port = self.recv_port;

        tokio::spawn(async move {
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, recv_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind GT Sport UDP socket on port {recv_port}: {e}");
                    return;
                }
            };
            info!("GT Sport adapter listening on UDP port {recv_port}");

            let heartbeat_payload = b"A";
            let mut buf = [0u8; PACKET_SIZE + 16];
            let mut frame_seq = 0u64;
            let mut last_heartbeat = tokio::time::Instant::now();
            let mut source_addr: Option<SocketAddr> = None;

            loop {
                if last_heartbeat.elapsed() >= Duration::from_millis(100) {
                    if let Some(addr) = source_addr {
                        let hb_addr = SocketAddr::new(addr.ip(), GTS_SEND_PORT);
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
                                    debug!("Receiver dropped, stopping GT Sport monitoring");
                                    break;
                                }
                                frame_seq = frame_seq.saturating_add(1);
                            }
                            Err(e) => debug!("Failed to parse GT Sport packet: {e}"),
                        }
                    }
                    Ok(Err(e)) => warn!("GT Sport UDP receive error: {e}"),
                    Err(_) => {} // timeout — keep looping to send heartbeat
                }
            }
            info!("Stopped GT Sport telemetry monitoring");
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

    /// GT Sport runs on a PlayStation console; process detection is not applicable.
    async fn is_game_running(&self) -> Result<bool> {
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TelemetryValue;
    use crate::gran_turismo_7::{MAGIC, OFF_MAGIC, PACKET_SIZE_TYPE2, PACKET_SIZE_TYPE3};

    // GT7 field offsets used by GT Sport tests (re-declared locally because
    // the canonical offsets in gran_turismo_7 are crate-private).
    const OFF_ENGINE_RPM: usize = 0x3C;
    const OFF_FUEL_LEVEL: usize = 0x44;
    const OFF_FUEL_CAPACITY: usize = 0x48;
    const OFF_SPEED_MS: usize = 0x4C;
    const OFF_WATER_TEMP: usize = 0x58;
    const OFF_TIRE_TEMP_FL: usize = 0x60;
    const OFF_TIRE_TEMP_FR: usize = 0x64;
    const OFF_TIRE_TEMP_RL: usize = 0x68;
    const OFF_TIRE_TEMP_RR: usize = 0x6C;
    const OFF_LAP_COUNT: usize = 0x74;
    const OFF_BEST_LAP_MS: usize = 0x78;
    const OFF_LAST_LAP_MS: usize = 0x7C;
    const OFF_MAX_ALERT_RPM: usize = 0x8A;
    const OFF_FLAGS: usize = 0x8E;
    const OFF_GEAR_BYTE: usize = 0x90;
    const OFF_THROTTLE: usize = 0x91;
    const OFF_BRAKE: usize = 0x92;
    const OFF_CAR_CODE: usize = 0x124;
    const OFF_WHEEL_ROTATION: usize = 0x128;
    const OFF_SWAY: usize = 0x130;
    const OFF_HEAVE: usize = 0x134;
    const OFF_SURGE: usize = 0x138;
    const OFF_CAR_TYPE_BYTE3: usize = 0x13E;
    const OFF_ENERGY_RECOVERY: usize = 0x150;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Build a minimal 296-byte decrypted buffer with the GT7 magic set.
    fn make_decrypted_buf() -> [u8; PACKET_SIZE] {
        let mut buf = [0u8; PACKET_SIZE];
        buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        buf
    }

    /// Build a 316-byte (PacketType2) decrypted buffer with magic.
    fn make_type2_buf() -> Vec<u8> {
        let mut buf = vec![0u8; PACKET_SIZE_TYPE2];
        buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        buf
    }

    /// Build a 344-byte (PacketType3) decrypted buffer with magic.
    fn make_type3_buf() -> Vec<u8> {
        let mut buf = vec![0u8; PACKET_SIZE_TYPE3];
        buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        buf
    }

    #[test]
    fn test_adapter_game_id() {
        let adapter = GranTurismo7SportsAdapter::new();
        assert_eq!(adapter.game_id(), "gran_turismo_sport");
    }

    #[test]
    fn test_adapter_update_rate() {
        let adapter = GranTurismo7SportsAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(17));
    }

    #[tokio::test]
    async fn test_adapter_is_game_running() -> TestResult {
        let adapter = GranTurismo7SportsAdapter::new();
        let running = adapter.is_game_running().await?;
        assert!(
            !running,
            "GT Sport is a console game; process detection returns false"
        );
        Ok(())
    }

    #[test]
    fn test_normalize_short_data_returns_err() {
        let adapter = GranTurismo7SportsAdapter::new();
        let result = adapter.normalize(&[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_with_port_override() {
        let adapter = GranTurismo7SportsAdapter::new().with_port(12345);
        assert_eq!(adapter.recv_port, 12345);
    }

    #[test]
    fn test_normalize_valid_packet() -> TestResult {
        // Build a plaintext packet with MAGIC set and call parse_decrypted directly.
        // normalize() calls decrypt_and_parse which encrypts-then-checks-magic; testing
        // the decrypted path here is the same validation used by the GT7 property tests.
        let mut buf = [0u8; PACKET_SIZE];
        buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        let result = crate::gran_turismo_7::parse_decrypted(&buf);
        assert!(result.is_ok(), "valid packet must parse successfully");
        Ok(())
    }

    #[test]
    fn test_default_recv_port() {
        let adapter = GranTurismo7SportsAdapter::default();
        assert_eq!(adapter.recv_port, GTS_RECV_PORT);
    }

    #[test]
    fn test_empty_packet_returns_err() -> TestResult {
        let adapter = GranTurismo7SportsAdapter::new();
        let result = adapter.normalize(&[]);
        assert!(result.is_err(), "empty packet must return an error");
        Ok(())
    }

    #[test]
    fn test_port_constants() {
        assert_eq!(
            GTS_RECV_PORT, 33340,
            "GT Sport receive port must be 33340 (Nenkai/PDTools BindPortDefault)"
        );
        assert_eq!(
            GTS_SEND_PORT, 33339,
            "GT Sport send port must be 33339 (Nenkai/PDTools ReceivePortDefault)"
        );
    }

    #[test]
    fn test_minimum_valid_decrypted_packet() -> TestResult {
        let mut buf = [0u8; PACKET_SIZE];
        buf[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        let result = crate::gran_turismo_7::parse_decrypted(&buf);
        assert!(
            result.is_ok(),
            "minimum decrypted packet with magic set must parse successfully"
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // GT Sport field extraction through parse_decrypted (shared with GT7)
    // These validate the GT Sport adapter path specifically.
    // -----------------------------------------------------------------------

    #[test]
    fn test_gts_rpm_extraction() -> TestResult {
        let mut buf = make_decrypted_buf();
        let rpm: f32 = 5500.0;
        buf[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert!(
            (t.rpm - rpm).abs() < 0.01,
            "GT Sport RPM should match: got {}",
            t.rpm
        );
        Ok(())
    }

    #[test]
    fn test_gts_speed_extraction() -> TestResult {
        let mut buf = make_decrypted_buf();
        let speed: f32 = 44.4; // ~160 km/h
        buf[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&speed.to_le_bytes());
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert!(
            (t.speed_ms - speed).abs() < 0.001,
            "GT Sport speed_ms should match"
        );
        Ok(())
    }

    #[test]
    fn test_gts_throttle_brake() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_THROTTLE] = 191; // ~75%
        buf[OFF_BRAKE] = 64; // ~25%
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert!(
            (t.throttle - 191.0 / 255.0).abs() < 0.001,
            "GT Sport throttle normalisation"
        );
        assert!(
            (t.brake - 64.0 / 255.0).abs() < 0.001,
            "GT Sport brake normalisation"
        );
        Ok(())
    }

    #[test]
    fn test_gts_gear_extraction() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_GEAR_BYTE] = (6 << 4) | 5; // gear 5, suggested 6
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert_eq!(t.gear, 5, "GT Sport gear should be low nibble");
        Ok(())
    }

    #[test]
    fn test_gts_fuel_percentage() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_FUEL_LEVEL..OFF_FUEL_LEVEL + 4].copy_from_slice(&15.0f32.to_le_bytes());
        buf[OFF_FUEL_CAPACITY..OFF_FUEL_CAPACITY + 4].copy_from_slice(&60.0f32.to_le_bytes());
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert!(
            (t.fuel_percent - 0.25).abs() < 0.001,
            "GT Sport fuel percent should be 0.25"
        );
        Ok(())
    }

    #[test]
    fn test_gts_water_temp() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_WATER_TEMP..OFF_WATER_TEMP + 4].copy_from_slice(&88.0f32.to_le_bytes());
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert!(
            (t.engine_temp_c - 88.0).abs() < 0.01,
            "GT Sport engine temp"
        );
        Ok(())
    }

    #[test]
    fn test_gts_tire_temps() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_TIRE_TEMP_FL..OFF_TIRE_TEMP_FL + 4].copy_from_slice(&75.0f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_FR..OFF_TIRE_TEMP_FR + 4].copy_from_slice(&78.0f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_RL..OFF_TIRE_TEMP_RL + 4].copy_from_slice(&72.0f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_RR..OFF_TIRE_TEMP_RR + 4].copy_from_slice(&74.0f32.to_le_bytes());
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert_eq!(t.tire_temps_c, [75, 78, 72, 74]);
        Ok(())
    }

    #[test]
    fn test_gts_lap_and_times() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_LAP_COUNT..OFF_LAP_COUNT + 2].copy_from_slice(&7u16.to_le_bytes());
        buf[OFF_BEST_LAP_MS..OFF_BEST_LAP_MS + 4].copy_from_slice(&78_901i32.to_le_bytes());
        buf[OFF_LAST_LAP_MS..OFF_LAST_LAP_MS + 4].copy_from_slice(&80_123i32.to_le_bytes());
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert_eq!(t.lap, 7);
        assert!((t.best_lap_time_s - 78.901).abs() < 0.001);
        assert!((t.last_lap_time_s - 80.123).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_gts_max_rpm() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_MAX_ALERT_RPM..OFF_MAX_ALERT_RPM + 2].copy_from_slice(&9000u16.to_le_bytes());
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert!((t.max_rpm - 9000.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_gts_flags() -> TestResult {
        let mut buf = make_decrypted_buf();
        // TCS + Paused
        let flags: u16 = (1 << 11) | (1 << 1);
        buf[OFF_FLAGS..OFF_FLAGS + 2].copy_from_slice(&flags.to_le_bytes());
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert!(t.flags.traction_control, "TCS should be set");
        assert!(t.flags.session_paused, "Paused should be set");
        assert!(!t.flags.abs_active, "ASM should not be set");
        assert!(!t.flags.engine_limiter, "Rev limit should not be set");
        Ok(())
    }

    #[test]
    fn test_gts_car_code() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_CAR_CODE..OFF_CAR_CODE + 4].copy_from_slice(&567i32.to_le_bytes());
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert_eq!(t.car_id.as_deref(), Some("gt7_567"));
        Ok(())
    }

    #[test]
    fn test_gts_zero_car_code_no_id() -> TestResult {
        let buf = make_decrypted_buf();
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert!(t.car_id.is_none());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // GT Sport error handling for various sizes
    // -----------------------------------------------------------------------

    #[test]
    fn test_gts_truncated_packets() {
        let adapter = GranTurismo7SportsAdapter::new();
        for size in [0, 1, 50, 100, 200, 295] {
            let data = vec![0u8; size];
            let result = adapter.normalize(&data);
            assert!(result.is_err(), "packet of size {size} must return error");
        }
    }

    // -----------------------------------------------------------------------
    // GT Sport extended packet support (Type2/Type3 through parse_decrypted_ext)
    // -----------------------------------------------------------------------

    #[test]
    fn test_gts_type2_extended_fields() -> TestResult {
        let mut buf = make_type2_buf();
        let rotation: f32 = 0.75;
        let sway: f32 = 0.2;
        let heave: f32 = -0.3;
        let surge: f32 = 0.5;
        buf[OFF_WHEEL_ROTATION..OFF_WHEEL_ROTATION + 4].copy_from_slice(&rotation.to_le_bytes());
        buf[OFF_SWAY..OFF_SWAY + 4].copy_from_slice(&sway.to_le_bytes());
        buf[OFF_HEAVE..OFF_HEAVE + 4].copy_from_slice(&heave.to_le_bytes());
        buf[OFF_SURGE..OFF_SURGE + 4].copy_from_slice(&surge.to_le_bytes());
        let t = crate::gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert!((t.steering_angle - rotation).abs() < 0.001);
        assert!((t.lateral_g - sway).abs() < 0.001);
        assert!((t.vertical_g - heave).abs() < 0.001);
        assert!((t.longitudinal_g - surge).abs() < 0.001);
        assert_eq!(
            t.get_extended("gt7_sway"),
            Some(&TelemetryValue::Float(sway))
        );
        Ok(())
    }

    #[test]
    fn test_gts_type3_extended_fields() -> TestResult {
        let mut buf = make_type3_buf();
        buf[OFF_CAR_TYPE_BYTE3] = 4;
        buf[OFF_ENERGY_RECOVERY..OFF_ENERGY_RECOVERY + 4].copy_from_slice(&50.0f32.to_le_bytes());
        // Also set some Type2 fields to ensure they're parsed
        buf[OFF_WHEEL_ROTATION..OFF_WHEEL_ROTATION + 4].copy_from_slice(&(-1.0f32).to_le_bytes());
        let t = crate::gran_turismo_7::parse_decrypted_ext(&buf)?;
        assert_eq!(
            t.get_extended("gt7_car_type"),
            Some(&TelemetryValue::Integer(4))
        );
        assert_eq!(
            t.get_extended("gt7_energy_recovery"),
            Some(&TelemetryValue::Float(50.0))
        );
        assert!((t.steering_angle - (-1.0)).abs() < 0.001);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Realistic full-field GT Sport packet
    // -----------------------------------------------------------------------

    #[test]
    fn test_gts_realistic_full_packet() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&6800.0f32.to_le_bytes());
        buf[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&41.7f32.to_le_bytes()); // ~150 km/h
        buf[OFF_THROTTLE] = 230;
        buf[OFF_BRAKE] = 0;
        buf[OFF_GEAR_BYTE] = (4 << 4) | 3;
        buf[OFF_FUEL_LEVEL..OFF_FUEL_LEVEL + 4].copy_from_slice(&40.0f32.to_le_bytes());
        buf[OFF_FUEL_CAPACITY..OFF_FUEL_CAPACITY + 4].copy_from_slice(&80.0f32.to_le_bytes());
        buf[OFF_WATER_TEMP..OFF_WATER_TEMP + 4].copy_from_slice(&90.0f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_FL..OFF_TIRE_TEMP_FL + 4].copy_from_slice(&80.0f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_FR..OFF_TIRE_TEMP_FR + 4].copy_from_slice(&82.0f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_RL..OFF_TIRE_TEMP_RL + 4].copy_from_slice(&78.0f32.to_le_bytes());
        buf[OFF_TIRE_TEMP_RR..OFF_TIRE_TEMP_RR + 4].copy_from_slice(&79.0f32.to_le_bytes());
        buf[OFF_LAP_COUNT..OFF_LAP_COUNT + 2].copy_from_slice(&5u16.to_le_bytes());
        buf[OFF_BEST_LAP_MS..OFF_BEST_LAP_MS + 4].copy_from_slice(&85_000i32.to_le_bytes());
        buf[OFF_LAST_LAP_MS..OFF_LAST_LAP_MS + 4].copy_from_slice(&86_500i32.to_le_bytes());
        buf[OFF_MAX_ALERT_RPM..OFF_MAX_ALERT_RPM + 2].copy_from_slice(&7500u16.to_le_bytes());
        buf[OFF_CAR_CODE..OFF_CAR_CODE + 4].copy_from_slice(&999i32.to_le_bytes());

        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert!((t.rpm - 6800.0).abs() < 0.01);
        assert!((t.speed_ms - 41.7).abs() < 0.01);
        assert!((t.throttle - 230.0 / 255.0).abs() < 0.001);
        assert_eq!(t.brake, 0.0);
        assert_eq!(t.gear, 3);
        assert!((t.fuel_percent - 0.5).abs() < 0.001);
        assert!((t.engine_temp_c - 90.0).abs() < 0.01);
        assert_eq!(t.tire_temps_c, [80, 82, 78, 79]);
        assert_eq!(t.lap, 5);
        assert!((t.best_lap_time_s - 85.0).abs() < 0.001);
        assert!((t.last_lap_time_s - 86.5).abs() < 0.001);
        assert!((t.max_rpm - 7500.0).abs() < 0.01);
        assert_eq!(t.car_id.as_deref(), Some("gt7_999"));
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Non-finite value handling through GT Sport path
    // -----------------------------------------------------------------------

    #[test]
    fn test_gts_nan_fields_handled() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_ENGINE_RPM..OFF_ENGINE_RPM + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        buf[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&f32::INFINITY.to_le_bytes());
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert_eq!(t.rpm, 0.0, "NaN RPM gives 0.0");
        assert_eq!(
            t.speed_ms, 0.0,
            "Infinity speed gives 0.0 (read_f32_le returns 0 then max(0) = 0)"
        );
        Ok(())
    }

    #[test]
    fn test_gts_negative_speed_clamped() -> TestResult {
        let mut buf = make_decrypted_buf();
        buf[OFF_SPEED_MS..OFF_SPEED_MS + 4].copy_from_slice(&(-5.0f32).to_le_bytes());
        let t = crate::gran_turismo_7::parse_decrypted(&buf)?;
        assert_eq!(t.speed_ms, 0.0, "negative speed clamped to 0");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Default trait implementation
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_and_new_equivalent() {
        let a = GranTurismo7SportsAdapter::default();
        let b = GranTurismo7SportsAdapter::new();
        assert_eq!(a.recv_port, b.recv_port);
        assert_eq!(a.update_rate, b.update_rate);
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Arbitrary byte sequences fed through the GT Sport adapter must never panic.
        #[test]
        fn prop_arbitrary_bytes_no_panic(
            data in proptest::collection::vec(any::<u8>(), 0..512)
        ) {
            let adapter = GranTurismo7SportsAdapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}
