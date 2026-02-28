//! Project CARS 3 telemetry adapter.
//!
//! PCARS3 uses the same UDP telemetry format and packet structure as PCARS2.
//! This adapter delegates parsing to the shared [`crate::pcars2`] implementation
//! while exposing a distinct game identity.

use crate::{NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver, telemetry_now_ns};
use anyhow::Result;
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_PCARS3_PORT: u16 = 5606;
const MAX_PACKET_SIZE: usize = 512;

/// Project CARS 3 telemetry adapter.
///
/// Reuses the PCARS2 UDP packet format; only the game identity differs.
pub struct PCars3Adapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for PCars3Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PCars3Adapter {
    pub fn new() -> Self {
        Self {
            bind_port: DEFAULT_PCARS3_PORT,
            update_rate: Duration::from_millis(10),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for PCars3Adapter {
    fn game_id(&self) -> &str {
        "project_cars_3"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = mpsc::channel(100);
        let bind_port = self.bind_port;
        let update_rate = self.update_rate;

        tokio::spawn(async move {
            let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bind_port));
            let socket = match TokioUdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to bind PCARS3 UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("PCARS3 adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_idx = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => {
                        match crate::pcars2::parse_pcars2_packet(&buf[..len]) {
                            Ok(normalized) => {
                                let frame = TelemetryFrame::new(
                                    normalized,
                                    telemetry_now_ns(),
                                    frame_idx,
                                    len,
                                );
                                if tx.send(frame).await.is_err() {
                                    debug!("Receiver dropped, stopping PCARS3 UDP monitoring");
                                    break;
                                }
                                frame_idx = frame_idx.saturating_add(1);
                            }
                            Err(e) => debug!("Failed to parse PCARS3 UDP packet: {e}"),
                        }
                    }
                    Ok(Err(e)) => warn!("PCARS3 UDP receive error: {e}"),
                    Err(_) => debug!("No PCARS3 telemetry data received (timeout)"),
                }
            }
            info!("Stopped PCARS3 telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        crate::pcars2::parse_pcars2_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_pcars3_process_running())
    }
}

#[cfg(windows)]
fn is_pcars3_process_running() -> bool {
    use std::ffi::CStr;
    use std::mem;
    use winapi::um::{
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        tlhelp32::{
            CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
            TH32CS_SNAPPROCESS,
        },
    };

    const PCARS3_PROCESS_NAMES: &[&str] = &["pcars3.exe", "projectcars3.exe"];

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
                if PCARS3_PROCESS_NAMES.iter().any(|p| name.contains(p)) {
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
fn is_pcars3_process_running() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Minimum packet size matching the PCARS2 format.
    const PCARS2_UDP_MIN_SIZE: usize = 84;

    // Field offsets (same as PCARS2)
    const OFF_STEERING: usize = 40;
    const OFF_THROTTLE: usize = 44;
    const OFF_BRAKE: usize = 48;
    const OFF_SPEED: usize = 52;
    const OFF_RPM: usize = 56;
    const OFF_MAX_RPM: usize = 60;
    const OFF_GEAR: usize = 80;

    fn make_pcars3_packet(
        steering: f32,
        throttle: f32,
        brake: f32,
        speed: f32,
        rpm: f32,
        max_rpm: f32,
        gear: u32,
    ) -> Vec<u8> {
        let mut data = vec![0u8; PCARS2_UDP_MIN_SIZE];
        data[OFF_STEERING..OFF_STEERING + 4].copy_from_slice(&steering.to_le_bytes());
        data[OFF_THROTTLE..OFF_THROTTLE + 4].copy_from_slice(&throttle.to_le_bytes());
        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&brake.to_le_bytes());
        data[OFF_SPEED..OFF_SPEED + 4].copy_from_slice(&speed.to_le_bytes());
        data[OFF_RPM..OFF_RPM + 4].copy_from_slice(&rpm.to_le_bytes());
        data[OFF_MAX_RPM..OFF_MAX_RPM + 4].copy_from_slice(&max_rpm.to_le_bytes());
        data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&gear.to_le_bytes());
        data
    }

    #[test]
    fn test_pcars3_parse_empty_input() {
        let adapter = PCars3Adapter::new();
        assert!(adapter.normalize(&[]).is_err());
    }

    #[test]
    fn test_pcars3_parse_valid_packet() -> TestResult {
        let adapter = PCars3Adapter::new();
        let data = make_pcars3_packet(0.25, 0.9, 0.1, 45.0, 6000.0, 9000.0, 4);
        let result = adapter.normalize(&data)?;
        assert!((result.steering_angle - 0.25).abs() < 0.001);
        assert!((result.throttle - 0.9).abs() < 0.001);
        assert!((result.brake - 0.1).abs() < 0.001);
        assert!((result.speed_ms - 45.0).abs() < 0.01);
        assert!((result.rpm - 6000.0).abs() < 0.01);
        assert!((result.max_rpm - 9000.0).abs() < 0.01);
        assert_eq!(result.gear, 4);
        Ok(())
    }

    #[test]
    fn test_pcars3_game_id() {
        let adapter = PCars3Adapter::new();
        assert_eq!(adapter.game_id(), "project_cars_3");
    }

    #[test]
    fn test_pcars3_update_rate() {
        let adapter = PCars3Adapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(10));
    }

    #[test]
    fn test_pcars3_truncated_packet() {
        let adapter = PCars3Adapter::new();
        let data = vec![0u8; 50];
        assert!(adapter.normalize(&data).is_err());
    }

    #[test]
    fn test_pcars3_with_port() {
        let adapter = PCars3Adapter::new().with_port(9999);
        assert_eq!(adapter.bind_port, 9999);
    }

    #[cfg(test)]
    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(500))]

            #[test]
            fn pcars3_no_panic_on_arbitrary_bytes(
                data in proptest::collection::vec(any::<u8>(), 0..512)
            ) {
                let adapter = PCars3Adapter::new();
                // Must never panic on arbitrary input.
                let _ = adapter.normalize(&data);
            }

            #[test]
            fn pcars3_short_packet_always_errors(
                data in proptest::collection::vec(any::<u8>(), 0..PCARS2_UDP_MIN_SIZE)
            ) {
                let adapter = PCars3Adapter::new();
                prop_assert!(adapter.normalize(&data).is_err());
            }

            #[test]
            fn pcars3_valid_packet_fields_in_range(
                steering in -1.0f32..=1.0f32,
                throttle in 0.0f32..1.0f32,
                brake in 0.0f32..1.0f32,
                speed in 0.0f32..200.0f32,
                rpm in 0.0f32..12000.0f32,
                max_rpm in 5000.0f32..12000.0f32,
                gear in 0u32..8u32,
            ) {
                let data = make_pcars3_packet(steering, throttle, brake, speed, rpm, max_rpm, gear);
                let adapter = PCars3Adapter::new();
                let result = adapter.normalize(&data).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
                prop_assert!(result.steering_angle >= -1.0 && result.steering_angle <= 1.0);
                prop_assert!(result.throttle >= 0.0 && result.throttle <= 1.0);
                prop_assert!(result.brake >= 0.0 && result.brake <= 1.0);
                prop_assert!(result.speed_ms >= 0.0);
                prop_assert!(result.rpm >= 0.0);
            }
        }
    }
}
