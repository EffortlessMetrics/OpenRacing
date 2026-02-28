//! DiRT Rally 2.0 telemetry adapter for Codemasters Mode 1 (legacy) UDP format.
//!
//! Enable in-game: Settings → Accessibility → UDP Telemetry, port 20777, mode 1.
//!
//! The Mode 1 packet is a fixed-layout binary stream of 264 bytes where every
//! field is a little-endian `f32` at a known byte offset.  Parsing is delegated
//! to [`crate::codemasters_shared`].

use crate::codemasters_shared;
use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver,
    telemetry_now_ns,
};
use anyhow::Result;
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_PORT: u16 = 20777;
const MAX_PACKET_SIZE: usize = 2048;
const DEFAULT_HEARTBEAT_TIMEOUT_MS: u64 = 1_500;

const ENV_PORT: &str = "OPENRACING_DIRT_RALLY_2_UDP_PORT";
const ENV_HEARTBEAT_TIMEOUT_MS: &str = "OPENRACING_DIRT_RALLY_2_HEARTBEAT_TIMEOUT_MS";

const GAME_LABEL: &str = "DiRT Rally 2.0";

/// DiRT Rally 2.0 adapter for Codemasters Mode 1 UDP telemetry.
#[derive(Clone)]
pub struct DirtRally2Adapter {
    bind_port: u16,
    update_rate: Duration,
    heartbeat_timeout: Duration,
    last_packet_ns: Arc<AtomicU64>,
}

impl Default for DirtRally2Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl DirtRally2Adapter {
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
        let now = u128::from(telemetry_now_ns());
        let elapsed_ns = now.saturating_sub(u128::from(last));
        elapsed_ns <= self.heartbeat_timeout.as_nanos()
    }
}

fn parse_packet(data: &[u8]) -> Result<NormalizedTelemetry> {
    codemasters_shared::parse_codemasters_mode1_common(data, GAME_LABEL)
}

#[async_trait]
impl TelemetryAdapter for DirtRally2Adapter {
    fn game_id(&self) -> &str {
        "dirt_rally_2"
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
                    warn!(
                        error = %error,
                        port = bind_port,
                        "DiRT Rally 2.0 UDP socket bind failed"
                    );
                    return;
                }
            };

            info!(port = bind_port, "DiRT Rally 2.0 UDP adapter bound");

            let mut frame_seq = 0u64;
            let mut buf = vec![0u8; MAX_PACKET_SIZE];
            let timeout = (update_rate * 4).max(Duration::from_millis(25));

            loop {
                let recv = tokio::time::timeout(timeout, socket.recv(&mut buf)).await;
                let len = match recv {
                    Ok(Ok(len)) => len,
                    Ok(Err(error)) => {
                        warn!(error = %error, "DiRT Rally 2.0 UDP receive error");
                        continue;
                    }
                    Err(_) => {
                        debug!("DiRT Rally 2.0 UDP receive timeout");
                        continue;
                    }
                };

                let data = &buf[..len];
                let normalized = match parse_packet(data) {
                    Ok(n) => n,
                    Err(error) => {
                        warn!(error = %error, "Failed to parse DiRT Rally 2.0 packet");
                        continue;
                    }
                };

                last_packet_ns.store(telemetry_now_ns(), Ordering::Relaxed);

                let frame = TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                if tx.send(frame).await.is_err() {
                    break;
                }

                frame_seq = frame_seq.saturating_add(1);
            }
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

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::codemasters_shared::*;
    use proptest::prelude::*;

    fn make_min_packet() -> Vec<u8> {
        vec![0u8; MIN_PACKET_SIZE]
    }

    fn write_f32_le(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Any packet shorter than MIN_PACKET_SIZE must return Err, never panic.
        #[test]
        fn prop_short_packet_returns_err(len in 0usize..MIN_PACKET_SIZE) {
            let data = vec![0u8; len];
            prop_assert!(parse_packet(&data).is_err());
        }

        /// Arbitrary bytes at or above MIN_PACKET_SIZE must never panic.
        #[test]
        fn prop_arbitrary_packet_no_panic(
            data in proptest::collection::vec(any::<u8>(), MIN_PACKET_SIZE..=MIN_PACKET_SIZE * 2)
        ) {
            let _ = parse_packet(&data);
        }

        /// FFB scalar is always clamped to [-1, 1] regardless of lateral G input.
        #[test]
        fn prop_ffb_scalar_in_range(lat_g in -10.0f32..=10.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_GFORCE_LAT, lat_g);
            let t = parse_packet(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                t.ffb_scalar >= -1.0 && t.ffb_scalar <= 1.0,
                "ffb_scalar {} must be in [-1, 1]",
                t.ffb_scalar
            );
        }

        /// Throttle is always clamped to [0, 1].
        #[test]
        fn prop_throttle_clamped(throttle in -5.0f32..=5.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_THROTTLE, throttle);
            let t = parse_packet(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                t.throttle >= 0.0 && t.throttle <= 1.0,
                "throttle {} must be in [0, 1]",
                t.throttle
            );
        }

        /// Brake is always clamped to [0, 1].
        #[test]
        fn prop_brake_clamped(brake in -5.0f32..=5.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_BRAKE, brake);
            let t = parse_packet(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                t.brake >= 0.0 && t.brake <= 1.0,
                "brake {} must be in [0, 1]",
                t.brake
            );
        }

        /// Steering angle is always clamped to [-1, 1].
        #[test]
        fn prop_steering_clamped(steer in -5.0f32..=5.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_STEER, steer);
            let t = parse_packet(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                t.steering_angle >= -1.0 && t.steering_angle <= 1.0,
                "steering_angle {} must be in [-1, 1]",
                t.steering_angle
            );
        }

        /// Gear field is always in the valid range [-1, 8].
        #[test]
        fn prop_gear_in_valid_range(gear_val in 0.0f32..=10.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_GEAR, gear_val);
            let t = parse_packet(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                t.gear >= -1 && t.gear <= 8,
                "gear {} must be in [-1, 8]",
                t.gear
            );
        }

        /// Speed from normal wheel-speed values is finite and non-negative.
        #[test]
        fn prop_speed_non_negative_from_wheel_speeds(ws in 0.0f32..=100.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_WHEEL_SPEED_FL, ws);
            write_f32_le(&mut buf, OFF_WHEEL_SPEED_FR, ws);
            write_f32_le(&mut buf, OFF_WHEEL_SPEED_RL, ws);
            write_f32_le(&mut buf, OFF_WHEEL_SPEED_RR, ws);
            let t = parse_packet(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                t.speed_ms >= 0.0 && t.speed_ms.is_finite(),
                "speed_ms {} must be finite and non-negative",
                t.speed_ms
            );
        }

        /// RPM is always non-negative (negative inputs are clamped to 0).
        #[test]
        fn prop_rpm_non_negative(rpm in -1000.0f32..=20000.0f32) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_RPM, rpm);
            let t = parse_packet(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(t.rpm >= 0.0, "rpm {} must be non-negative", t.rpm);
        }

        /// Fuel percent is always in [0, 1] when capacity is positive.
        #[test]
        fn prop_fuel_percent_in_range(
            fuel_in in 0.0f32..=200.0f32,
            fuel_cap in 1.0f32..=300.0f32,
        ) {
            let mut buf = make_min_packet();
            write_f32_le(&mut buf, OFF_FUEL_IN_TANK, fuel_in);
            write_f32_le(&mut buf, OFF_FUEL_CAPACITY, fuel_cap);
            let t = parse_packet(&buf).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
            prop_assert!(
                t.fuel_percent >= 0.0 && t.fuel_percent <= 1.0,
                "fuel_percent {} must be in [0, 1]",
                t.fuel_percent
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codemasters_shared::*;
    use crate::TelemetryValue;

    fn make_packet(size: usize) -> Vec<u8> {
        vec![0u8; size]
    }

    fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn rejects_short_packet() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let result = adapter.normalize(&[0u8; MIN_PACKET_SIZE - 1]);
        assert!(result.is_err(), "expected error for short packet");
        Ok(())
    }

    #[test]
    fn zero_packet_returns_zero_speed_and_rpm() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn zero_gear_maps_to_reverse() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        // 0.0 in packet → reverse = -1
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn speed_extracted_from_wheel_speeds() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FL, 20.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_FR, 20.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RL, 20.0);
        write_f32(&mut raw, OFF_WHEEL_SPEED_RR, 20.0);
        let t = adapter.normalize(&raw)?;
        assert!(
            (t.speed_ms - 20.0).abs() < 0.001,
            "speed_ms should be 20.0, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn speed_falls_back_to_velocity_magnitude() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        // All wheel speeds remain 0; set velocity components for a 3-4-5 right triangle.
        write_f32(&mut raw, OFF_VEL_X, 3.0);
        write_f32(&mut raw, OFF_VEL_Y, 0.0);
        write_f32(&mut raw, OFF_VEL_Z, 4.0);
        let t = adapter.normalize(&raw)?;
        assert!(
            (t.speed_ms - 5.0).abs() < 0.001,
            "speed_ms should be 5.0, got {}",
            t.speed_ms
        );
        Ok(())
    }

    #[test]
    fn gear_extraction_forward_gears() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        for g in 1i8..=6i8 {
            let mut raw = make_packet(MIN_PACKET_SIZE);
            write_f32(&mut raw, OFF_GEAR, f32::from(g));
            let t = adapter.normalize(&raw)?;
            assert_eq!(t.gear, g, "expected gear {g}");
        }
        Ok(())
    }

    #[test]
    fn gear_extraction_reverse() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_GEAR, 0.0);
        let t = adapter.normalize(&raw)?;
        assert_eq!(t.gear, -1);
        Ok(())
    }

    #[test]
    fn in_pit_flag_set_when_one() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_IN_PIT, 1.0);
        let t = adapter.normalize(&raw)?;
        assert!(t.flags.in_pits, "in_pits should be true");
        Ok(())
    }

    #[test]
    fn in_pit_flag_clear_when_zero() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let raw = make_packet(MIN_PACKET_SIZE);
        let t = adapter.normalize(&raw)?;
        assert!(!t.flags.in_pits, "in_pits should be false");
        Ok(())
    }

    #[test]
    fn rpm_and_rpm_fraction_extracted() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = DirtRally2Adapter::new();
        let mut raw = make_packet(MIN_PACKET_SIZE);
        write_f32(&mut raw, OFF_RPM, 5000.0);
        write_f32(&mut raw, OFF_MAX_RPM, 8000.0);
        let t = adapter.normalize(&raw)?;
        assert!((t.rpm - 5000.0).abs() < 0.001, "rpm should be 5000.0");
        assert!(
            (t.max_rpm - 8000.0).abs() < 0.001,
            "max_rpm should be 8000.0"
        );
        if let Some(TelemetryValue::Float(fraction)) = t.extended.get("rpm_fraction") {
            assert!(
                (fraction - 0.625).abs() < 0.001,
                "rpm_fraction should be 0.625, got {fraction}"
            );
        } else {
            return Err("rpm_fraction not found in extended telemetry".into());
        }
        Ok(())
    }
}
