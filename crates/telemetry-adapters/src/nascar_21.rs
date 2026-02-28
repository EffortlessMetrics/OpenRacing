//! NASCAR 21: Ignition telemetry adapter.
//!
//! NASCAR 21: Ignition (704Games/Motorsport Games, 2021) uses the same Papyrus
//! UDP telemetry format as the NASCAR Heat series, broadcasting on port 5606.
//!
//! Packet parsing is delegated to [`crate::nascar::parse_nascar_packet`].

use crate::{
    NormalizedTelemetry, TelemetryAdapter, TelemetryFrame, TelemetryReceiver,
    nascar::parse_nascar_packet,
    telemetry_now_ns,
};
use anyhow::Result;
use async_trait::async_trait;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const DEFAULT_PORT: u16 = 5606;
const MAX_PACKET_SIZE: usize = 512;

const ENV_PORT: &str = "OPENRACING_NASCAR21_UDP_PORT";

/// NASCAR 21: Ignition UDP telemetry adapter.
///
/// Uses the Papyrus UDP protocol (identical to NASCAR Heat 5) on port 5606.
pub struct Nascar21Adapter {
    bind_port: u16,
    update_rate: Duration,
}

impl Default for Nascar21Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl Nascar21Adapter {
    pub fn new() -> Self {
        let bind_port = std::env::var(ENV_PORT)
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .filter(|&p| p > 0)
            .unwrap_or(DEFAULT_PORT);
        Self {
            bind_port,
            update_rate: Duration::from_millis(16),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.bind_port = port;
        self
    }
}

#[async_trait]
impl TelemetryAdapter for Nascar21Adapter {
    fn game_id(&self) -> &str {
        "nascar_21"
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
                    warn!("Failed to bind NASCAR 21 UDP socket on port {bind_port}: {e}");
                    return;
                }
            };
            info!("NASCAR 21 adapter listening on UDP port {bind_port}");
            let mut buf = [0u8; MAX_PACKET_SIZE];
            let mut frame_seq = 0u64;

            loop {
                match tokio::time::timeout(update_rate * 10, socket.recv(&mut buf)).await {
                    Ok(Ok(len)) => match parse_nascar_packet(&buf[..len]) {
                        Ok(normalized) => {
                            let frame =
                                TelemetryFrame::new(normalized, telemetry_now_ns(), frame_seq, len);
                            if tx.send(frame).await.is_err() {
                                debug!("Receiver dropped, stopping NASCAR 21 monitoring");
                                break;
                            }
                            frame_seq = frame_seq.saturating_add(1);
                        }
                        Err(e) => debug!("Failed to parse NASCAR 21 packet: {e}"),
                    },
                    Ok(Err(e)) => warn!("NASCAR 21 UDP receive error: {e}"),
                    Err(_) => debug!("No NASCAR 21 telemetry received (timeout)"),
                }
            }
            info!("Stopped NASCAR 21 telemetry monitoring");
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry> {
        parse_nascar_packet(raw)
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_nascar21_process_running())
    }
}

#[cfg(windows)]
fn is_nascar21_process_running() -> bool {
    use std::ffi::CStr;
    use std::mem;
    use winapi::um::{
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        tlhelp32::{
            CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
            TH32CS_SNAPPROCESS,
        },
    };
    const PROCESS_NAMES: &[&str] = &["nascar21ignition.exe", "nascar2021.exe"];
    // SAFETY: Windows snapshot API with proper initialisation.
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
                if PROCESS_NAMES.iter().any(|p| name.contains(p)) {
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
fn is_nascar21_process_running() -> bool {
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Mirror of nascar.rs MIN_PACKET_SIZE / OFF_RPM constants.
    const MIN_PACKET: usize = 92;
    const OFF_RPM: usize = 72;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn make_packet() -> Vec<u8> {
        vec![0u8; MIN_PACKET]
    }

    fn write_f32(buf: &mut [u8], offset: usize, value: f32) {
        buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn test_adapter_game_id() {
        assert_eq!(Nascar21Adapter::new().game_id(), "nascar_21");
    }

    #[test]
    fn test_normalize_valid_packet() -> TestResult {
        let mut data = make_packet();
        write_f32(&mut data, OFF_RPM, 7200.0);
        let adapter = Nascar21Adapter::new();
        let t = adapter.normalize(&data)?;
        assert!((t.rpm - 7200.0).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn test_normalize_short_packet_returns_err() {
        let adapter = Nascar21Adapter::new();
        assert!(adapter.normalize(&[0u8; 10]).is_err());
    }

    #[test]
    fn test_update_rate() {
        let adapter = Nascar21Adapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_with_port_override() {
        let adapter = Nascar21Adapter::new().with_port(9999);
        assert_eq!(adapter.bind_port, 9999);
    }

    #[test]
    fn test_default_port() {
        // Clear env var if set, then check default
        std::env::remove_var("OPENRACING_NASCAR21_UDP_PORT");
        let adapter = Nascar21Adapter::new();
        assert_eq!(adapter.bind_port, DEFAULT_PORT);
    }
}
