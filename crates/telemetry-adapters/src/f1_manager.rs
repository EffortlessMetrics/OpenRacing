//! F1 Manager series telemetry adapter (stub).
//!
//! F1 Manager (2022/2023/2024) is a management/strategy game published by EA
//! Sports and developed by Frontier Developments. Unlike the F1 racing
//! simulation titles it does **not** expose UDP telemetry suitable for force
//! feedback or real-time driving data.
//!
//! This adapter registers the game family in the support matrix so users can
//! see it as a known (non-applicable) title. `normalize` always returns
//! [`NormalizedTelemetry::default()`] and `start_monitoring` emits no frames.

use crate::{NormalizedTelemetry, TelemetryAdapter, TelemetryReceiver};
use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::mpsc;

/// Stub adapter for the F1 Manager series (2022/2023/2024).
///
/// F1 Manager is a strategy/management title with no driving simulation or
/// UDP telemetry output applicable to force feedback. The adapter is
/// registered so the game appears in auto-detection results.
pub struct F1ManagerAdapter;

impl Default for F1ManagerAdapter {
    fn default() -> Self {
        Self
    }
}

impl F1ManagerAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TelemetryAdapter for F1ManagerAdapter {
    fn game_id(&self) -> &str {
        "f1_manager"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        // Drop the sender immediately; the receiver will yield no frames.
        let (_tx, rx) = mpsc::channel(1);
        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    /// F1 Manager has no UDP telemetry; returns empty/default telemetry.
    fn normalize(&self, _raw: &[u8]) -> Result<NormalizedTelemetry> {
        Ok(NormalizedTelemetry::default())
    }

    fn expected_update_rate(&self) -> Duration {
        // No real data is expected; use a low rate to avoid unnecessary wakeups.
        Duration::from_secs(1)
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(is_f1_manager_running())
    }
}

#[cfg(windows)]
fn is_f1_manager_running() -> bool {
    use std::ffi::CStr;
    use std::mem;
    use winapi::um::{
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        tlhelp32::{
            CreateToolhelp32Snapshot, PROCESSENTRY32, Process32First, Process32Next,
            TH32CS_SNAPPROCESS,
        },
    };
    const PROCESS_NAMES: &[&str] = &[
        "f1manager2022.exe",
        "f1manager2023.exe",
        "f1manager2024.exe",
        "f1manager22.exe",
        "f1manager23.exe",
        "f1manager24.exe",
    ];
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
fn is_f1_manager_running() -> bool {
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_adapter_game_id() {
        assert_eq!(F1ManagerAdapter::new().game_id(), "f1_manager");
    }

    #[test]
    fn test_normalize_returns_default() -> TestResult {
        let adapter = F1ManagerAdapter::new();
        let result = adapter.normalize(&[])?;
        let expected = NormalizedTelemetry::default();
        assert_eq!(result.rpm, expected.rpm);
        assert_eq!(result.speed_ms, expected.speed_ms);
        Ok(())
    }

    #[test]
    fn test_normalize_any_data_returns_default() -> TestResult {
        let adapter = F1ManagerAdapter::new();
        let result = adapter.normalize(&[0xFF; 512])?;
        let expected = NormalizedTelemetry::default();
        assert_eq!(result.rpm, expected.rpm);
        Ok(())
    }

    #[test]
    fn test_update_rate() {
        let adapter = F1ManagerAdapter::new();
        assert_eq!(adapter.expected_update_rate(), Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_start_monitoring_yields_no_frames() -> TestResult {
        let adapter = F1ManagerAdapter::new();
        let mut rx = adapter.start_monitoring().await?;
        // Sender is immediately dropped; recv should return None promptly.
        let frame = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await;
        // Either a timeout or None is acceptable — no frames expected.
        if let Ok(Some(_)) = frame {
            return Err("F1Manager stub must not emit telemetry frames".into());
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_stop_monitoring() -> TestResult {
        let adapter = F1ManagerAdapter::new();
        adapter.stop_monitoring().await?;
        Ok(())
    }

    #[test]
    fn test_default_impl() {
        let adapter = F1ManagerAdapter;
        assert_eq!(adapter.game_id(), "f1_manager");
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
            let adapter = F1ManagerAdapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}
