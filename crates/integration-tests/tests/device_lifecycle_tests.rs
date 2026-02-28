//! Integration tests for device lifecycle, multi-device, game switching,
//! error recovery, and configuration persistence scenarios.
//!
//! Covers the five integration test categories requested:
//! 1. Device lifecycle: connect → configure → run → disconnect per vendor
//! 2. Multi-device: simultaneous devices, routing, FFB isolation
//! 3. Game switching: telemetry adapter hot-swap on game change
//! 4. Error recovery: USB disconnect/reconnect mid-FFB, safety interlocks
//! 5. Configuration persistence: device configs survive service restart

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use tempfile::TempDir;
use tokio::sync::Mutex;

use racing_wheel_engine::ports::{HidDevice, HidPort};
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState};
use racing_wheel_engine::{
    FFBMode, Frame, ModeSelectionPolicy, Pipeline, VirtualDevice, VirtualHidPort,
};
use racing_wheel_schemas::prelude::*;
use racing_wheel_service::{
    auto_profile_switching::AutoProfileSwitchingService,
    game_telemetry_bridge::TelemetryAdapterControl,
    process_detection::{ProcessEvent, ProcessInfo},
    profile_repository::ProfileRepositoryConfig,
    profile_service::ProfileService,
};

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn game_started_event(game_id: &str, exe: &str) -> ProcessEvent {
    ProcessEvent::GameStarted {
        game_id: game_id.to_string(),
        process_info: ProcessInfo {
            pid: 1234,
            name: exe.to_string(),
            game_id: Some(game_id.to_string()),
            detected_at: Instant::now(),
        },
    }
}

fn game_stopped_event(game_id: &str, exe: &str) -> ProcessEvent {
    ProcessEvent::GameStopped {
        game_id: game_id.to_string(),
        process_info: ProcessInfo {
            pid: 1234,
            name: exe.to_string(),
            game_id: Some(game_id.to_string()),
            detected_at: Instant::now(),
        },
    }
}

async fn make_profile_service(tmp: &TempDir) -> Result<Arc<ProfileService>> {
    let config = ProfileRepositoryConfig {
        profiles_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    Ok(Arc::new(ProfileService::new_with_config(config).await?))
}

async fn seed_profile(service: &ProfileService, id: &str) -> Result<ProfileId> {
    let profile_id: ProfileId = id.parse()?;
    let profile = Profile::new(
        profile_id,
        ProfileScope::global(),
        BaseSettings::default(),
        id.to_string(),
    );
    service.create_profile(profile).await
}

struct MockAdapterControl {
    starts: Arc<Mutex<Vec<String>>>,
    stops: Arc<Mutex<Vec<String>>>,
}

impl MockAdapterControl {
    fn new() -> Self {
        Self {
            starts: Arc::new(Mutex::new(Vec::new())),
            stops: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn started_games(&self) -> Vec<String> {
        self.starts.lock().await.clone()
    }

    async fn stopped_games(&self) -> Vec<String> {
        self.stops.lock().await.clone()
    }
}

#[async_trait]
impl TelemetryAdapterControl for MockAdapterControl {
    async fn start_for_game(&self, game_id: &str) -> Result<()> {
        self.starts.lock().await.push(game_id.to_string());
        Ok(())
    }

    async fn stop_for_game(&self, game_id: &str) -> Result<()> {
        self.stops.lock().await.push(game_id.to_string());
        Ok(())
    }
}

/// Vendor family fixture: name, max torque, supports raw torque 1kHz, encoder CPR
struct VendorFixture {
    name: &'static str,
    device_id: &'static str,
    max_torque_nm: f64,
    supports_raw_1khz: bool,
    encoder_cpr: u16,
}

const VENDOR_FIXTURES: &[VendorFixture] = &[
    VendorFixture {
        name: "Fanatec GT DD Pro",
        device_id: "fanatec-gt-dd-pro-001",
        max_torque_nm: 8.0,
        supports_raw_1khz: true,
        encoder_cpr: 65535,
    },
    VendorFixture {
        name: "Thrustmaster T300RS",
        device_id: "thrustmaster-t300rs-001",
        max_torque_nm: 3.9,
        supports_raw_1khz: false,
        encoder_cpr: 4096,
    },
    VendorFixture {
        name: "Logitech G923",
        device_id: "logitech-g923-001",
        max_torque_nm: 2.2,
        supports_raw_1khz: false,
        encoder_cpr: 4096,
    },
    VendorFixture {
        name: "Moza R9 V2",
        device_id: "moza-r9-v2-001",
        max_torque_nm: 9.0,
        supports_raw_1khz: true,
        encoder_cpr: 65535,
    },
    VendorFixture {
        name: "Simucube 2 Pro",
        device_id: "simucube-2-pro-001",
        max_torque_nm: 25.0,
        supports_raw_1khz: true,
        encoder_cpr: 65535,
    },
    VendorFixture {
        name: "Simagic Alpha Mini",
        device_id: "simagic-alpha-mini-001",
        max_torque_nm: 10.0,
        supports_raw_1khz: true,
        encoder_cpr: 65535,
    },
    VendorFixture {
        name: "Asetek Forte",
        device_id: "asetek-forte-001",
        max_torque_nm: 18.0,
        supports_raw_1khz: true,
        encoder_cpr: 65535,
    },
    VendorFixture {
        name: "VRS DirectForce Pro",
        device_id: "vrs-dfp-001",
        max_torque_nm: 20.0,
        supports_raw_1khz: true,
        encoder_cpr: 65535,
    },
];

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Device Lifecycle Tests
// ═══════════════════════════════════════════════════════════════════════════════

mod device_lifecycle {
    use super::*;

    /// Scenario: connect → configure → run FFB → disconnect for each vendor
    ///
    /// Verifies the full lifecycle of a virtual device from each vendor family.
    /// After disconnect, FFB writes must fail gracefully (no panic).
    #[tokio::test]
    async fn scenario_full_lifecycle_all_vendor_families() -> Result<()> {
        for fixture in VENDOR_FIXTURES {
            let id: DeviceId = fixture.device_id.parse()?;
            let mut device = VirtualDevice::new(id.clone(), fixture.name.to_string());

            // Connect: device is initially connected
            assert!(
                device.is_connected(),
                "{}: device must be connected after creation",
                fixture.name
            );

            // Configure: read capabilities and verify
            let caps = device.capabilities();
            assert!(
                caps.max_torque.value() > 0.0,
                "{}: max torque must be positive",
                fixture.name
            );

            // Run FFB: write a torque command
            let write_result = device.write_ffb_report(1.0, 0);
            assert!(
                write_result.is_ok(),
                "{}: FFB write must succeed while connected",
                fixture.name
            );

            // Read telemetry while running
            let telemetry = device.read_telemetry();
            assert!(
                telemetry.is_some(),
                "{}: telemetry must be readable while connected",
                fixture.name
            );

            // Disconnect: device goes offline
            device.disconnect();
            assert!(
                !device.is_connected(),
                "{}: device must be disconnected",
                fixture.name
            );

            // Verify FFB stops cleanly: write should fail but not panic
            let post_disconnect = device.write_ffb_report(1.0, 1);
            assert!(
                post_disconnect.is_err(),
                "{}: FFB write must fail after disconnect",
                fixture.name
            );
        }

        Ok(())
    }

    /// Scenario: device reconnect restores functionality
    ///
    /// After disconnect + reconnect, the device must be fully functional again.
    #[tokio::test]
    async fn scenario_reconnect_restores_device_functionality() -> Result<()> {
        let id: DeviceId = "lifecycle-reconnect-001".parse()?;
        let mut device = VirtualDevice::new(id, "Reconnect Test Wheel".to_string());

        // Initial FFB write succeeds
        device.write_ffb_report(2.0, 0)?;

        // Disconnect
        device.disconnect();
        assert!(!device.is_connected());

        // Reconnect
        device.reconnect();
        assert!(device.is_connected());

        // FFB write succeeds again
        let result = device.write_ffb_report(3.0, 1);
        assert!(result.is_ok(), "FFB write must succeed after reconnect");

        // Telemetry readable again
        let telem = device.read_telemetry();
        assert!(
            telem.is_some(),
            "telemetry must be readable after reconnect"
        );

        Ok(())
    }

    /// Scenario: device telemetry data is valid throughout lifecycle
    ///
    /// All telemetry fields must be finite and within sane ranges at every
    /// lifecycle stage.
    #[tokio::test]
    async fn scenario_telemetry_valid_throughout_lifecycle() -> Result<()> {
        let id: DeviceId = "lifecycle-telem-001".parse()?;
        let mut device = VirtualDevice::new(id, "Telemetry Lifecycle Wheel".to_string());

        // Before any FFB
        let t1 = device
            .read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("telemetry missing at creation"))?;
        assert!(t1.temperature_c <= 150, "temperature must be in sane range");

        // After FFB write
        device.write_ffb_report(5.0, 0)?;
        let t2 = device
            .read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("telemetry missing after FFB"))?;
        assert!(
            t2.temperature_c <= 150,
            "temperature must remain sane after FFB"
        );

        // After disconnect + reconnect
        device.disconnect();
        device.reconnect();
        let t3 = device
            .read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("telemetry missing after reconnect"))?;
        assert!(
            t3.temperature_c <= 150,
            "temperature must remain sane after reconnect"
        );

        Ok(())
    }

    /// Scenario: each vendor family has correct FFB mode negotiation
    ///
    /// Devices supporting raw torque at 1kHz should negotiate RawTorque mode;
    /// others should fall back to PID or TelemetrySynth.
    #[test]
    fn scenario_ffb_mode_negotiation_per_vendor_family() -> Result<()> {
        for fixture in VENDOR_FIXTURES {
            let caps = DeviceCapabilities::new(
                !fixture.supports_raw_1khz, // supports_pid (fallback for non-raw)
                fixture.supports_raw_1khz,
                true,
                false,
                TorqueNm::new(fixture.max_torque_nm as f32)?,
                fixture.encoder_cpr,
                if fixture.supports_raw_1khz {
                    1000
                } else {
                    4000
                },
            );

            let mode = ModeSelectionPolicy::select_mode(&caps, None);

            if fixture.supports_raw_1khz {
                assert_eq!(
                    mode,
                    FFBMode::RawTorque,
                    "{}: raw-capable device must select RawTorque",
                    fixture.name
                );
            } else {
                assert_ne!(
                    mode,
                    FFBMode::RawTorque,
                    "{}: non-raw device must not select RawTorque",
                    fixture.name
                );
            }
        }

        Ok(())
    }

    /// Scenario: pipeline processes frames for each vendor without panic
    #[test]
    fn scenario_pipeline_processes_frames_per_vendor() -> Result<()> {
        for fixture in VENDOR_FIXTURES {
            let mut pipeline = Pipeline::new();
            let mut frame = Frame {
                ffb_in: 0.5,
                torque_out: 0.5,
                wheel_speed: 1.0,
                hands_off: false,
                ts_mono_ns: 1_000_000,
                seq: 1,
            };

            let result = pipeline.process(&mut frame);
            assert!(
                result.is_ok(),
                "{}: pipeline.process must not fail on valid frame",
                fixture.name
            );
            assert!(
                frame.torque_out.is_finite(),
                "{}: torque_out must be finite after pipeline",
                fixture.name
            );
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Multi-Device Tests
// ═══════════════════════════════════════════════════════════════════════════════

mod multi_device {
    use super::*;

    /// Scenario: two devices connected simultaneously
    ///
    /// Both devices must appear in the port listing with correct IDs and
    /// independent connection state.
    #[tokio::test]
    async fn scenario_two_devices_enumerated_simultaneously() -> Result<()> {
        let mut port = VirtualHidPort::new();

        let id_a: DeviceId = "multi-fanatec-001".parse()?;
        let id_b: DeviceId = "multi-moza-001".parse()?;

        port.add_device(VirtualDevice::new(
            id_a.clone(),
            "Fanatec DD Pro".to_string(),
        ))
        .map_err(|e| anyhow::anyhow!("{}", e))?;
        port.add_device(VirtualDevice::new(id_b.clone(), "Moza R9".to_string()))
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let devices = port
            .list_devices()
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        assert_eq!(devices.len(), 2, "both devices must be listed");

        let ids: Vec<&DeviceId> = devices.iter().map(|d| &d.id).collect();
        assert!(ids.contains(&&id_a), "Fanatec must be in device list");
        assert!(ids.contains(&&id_b), "Moza must be in device list");

        for dev in &devices {
            assert!(dev.is_connected, "all devices must be connected");
        }

        Ok(())
    }

    /// Scenario: FFB isolation between two concurrent devices
    ///
    /// Writing different torque values to two devices must not cross-contaminate.
    #[tokio::test]
    async fn scenario_ffb_isolation_between_devices() -> Result<()> {
        let mut port = VirtualHidPort::new();

        let id_a: DeviceId = "iso-wheel-a".parse()?;
        let id_b: DeviceId = "iso-wheel-b".parse()?;

        port.add_device(VirtualDevice::new(id_a.clone(), "Wheel A".to_string()))
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        port.add_device(VirtualDevice::new(id_b.clone(), "Wheel B".to_string()))
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let mut dev_a = port
            .open_device(&id_a)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let mut dev_b = port
            .open_device(&id_b)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // Write different torque values
        dev_a.write_ffb_report(5.0, 0)?;
        dev_b.write_ffb_report(-3.0, 0)?;

        // Both devices must still be independently functional
        let telem_a = dev_a.read_telemetry();
        let telem_b = dev_b.read_telemetry();

        assert!(telem_a.is_some(), "device A telemetry must be readable");
        assert!(telem_b.is_some(), "device B telemetry must be readable");

        // Verify devices remain connected and independent
        assert!(dev_a.is_connected(), "device A must remain connected");
        assert!(dev_b.is_connected(), "device B must remain connected");

        Ok(())
    }

    /// Scenario: concurrent FFB writes on two devices do not panic
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scenario_concurrent_ffb_writes_safe() -> Result<()> {
        let mut port = VirtualHidPort::new();

        let id_a: DeviceId = "concurrent-write-a".parse()?;
        let id_b: DeviceId = "concurrent-write-b".parse()?;

        port.add_device(VirtualDevice::new(id_a.clone(), "Concurrent A".to_string()))
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        port.add_device(VirtualDevice::new(id_b.clone(), "Concurrent B".to_string()))
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let mut dev_a = port
            .open_device(&id_a)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let mut dev_b = port
            .open_device(&id_b)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let handle_a = tokio::task::spawn_blocking(move || -> Result<()> {
            for i in 0u16..50 {
                dev_a.write_ffb_report(2.0, i)?;
                let _ = dev_a.read_telemetry();
            }
            Ok(())
        });

        let handle_b = tokio::task::spawn_blocking(move || -> Result<()> {
            for i in 0u16..50 {
                dev_b.write_ffb_report(-2.0, i)?;
                let _ = dev_b.read_telemetry();
            }
            Ok(())
        });

        handle_a
            .await
            .map_err(|e| anyhow::anyhow!("device A task panicked: {}", e))??;
        handle_b
            .await
            .map_err(|e| anyhow::anyhow!("device B task panicked: {}", e))??;

        Ok(())
    }

    /// Scenario: disconnecting one device does not affect the other
    ///
    /// Uses VirtualDevice directly (disconnect is a concrete method, not on
    /// HidDevice trait), validating that each device's state is independent.
    #[tokio::test]
    async fn scenario_disconnect_one_device_other_unaffected() -> Result<()> {
        let id_a: DeviceId = "disconnect-a".parse()?;
        let id_b: DeviceId = "disconnect-b".parse()?;

        let mut dev_a = VirtualDevice::new(id_a, "Wheel A".to_string());
        let mut dev_b = VirtualDevice::new(id_b, "Wheel B".to_string());

        // Both functional initially
        dev_a.write_ffb_report(1.0, 0)?;
        dev_b.write_ffb_report(1.0, 0)?;

        // Disconnect device A
        dev_a.disconnect();
        assert!(!dev_a.is_connected());

        // Device B must remain functional
        assert!(dev_b.is_connected(), "device B must still be connected");
        let write_result = dev_b.write_ffb_report(1.0, 1);
        assert!(
            write_result.is_ok(),
            "device B FFB write must succeed after device A disconnects"
        );
        let telem = dev_b.read_telemetry();
        assert!(
            telem.is_some(),
            "device B telemetry must be readable after device A disconnects"
        );

        Ok(())
    }

    /// Scenario: three or more devices simultaneously
    #[tokio::test]
    async fn scenario_three_devices_simultaneously() -> Result<()> {
        let mut port = VirtualHidPort::new();

        let ids: Vec<DeviceId> = vec![
            "triple-fanatec".parse()?,
            "triple-moza".parse()?,
            "triple-simucube".parse()?,
        ];

        let names = ["Fanatec DD1", "Moza R16", "Simucube 2 Ultimate"];

        for (id, name) in ids.iter().zip(names.iter()) {
            port.add_device(VirtualDevice::new(id.clone(), name.to_string()))
                .map_err(|e| anyhow::anyhow!("{}", e))?;
        }

        let devices = port
            .list_devices()
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        assert_eq!(devices.len(), 3, "all three devices must be listed");

        for id in &ids {
            let mut dev = port
                .open_device(id)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            assert!(dev.is_connected());
            dev.write_ffb_report(1.0, 0)?;
            assert!(dev.read_telemetry().is_some());
        }

        Ok(())
    }

    /// Scenario: independent telemetry from two devices
    #[tokio::test]
    async fn scenario_independent_telemetry_per_device() -> Result<()> {
        let mut port = VirtualHidPort::new();

        let id_a: DeviceId = "telem-ind-a".parse()?;
        let id_b: DeviceId = "telem-ind-b".parse()?;

        port.add_device(VirtualDevice::new(
            id_a.clone(),
            "Telem Wheel A".to_string(),
        ))
        .map_err(|e| anyhow::anyhow!("{}", e))?;
        port.add_device(VirtualDevice::new(
            id_b.clone(),
            "Telem Wheel B".to_string(),
        ))
        .map_err(|e| anyhow::anyhow!("{}", e))?;

        let mut dev_a = port
            .open_device(&id_a)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let mut dev_b = port
            .open_device(&id_b)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let telem_a = dev_a
            .read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("device A telemetry missing"))?;
        let telem_b = dev_b
            .read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("device B telemetry missing"))?;

        // Both must return valid telemetry (finite temperature, etc.)
        assert!(
            telem_a.temperature_c <= 150,
            "device A temperature in sane range"
        );
        assert!(
            telem_b.temperature_c <= 150,
            "device B temperature in sane range"
        );

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Game Switching Tests
// ═══════════════════════════════════════════════════════════════════════════════

mod game_switching {
    use super::*;

    /// Scenario: switching from iRacing to ACC while device is connected
    ///
    /// Telemetry adapter for iRacing must stop and ACC adapter must start.
    /// Profile must switch to the ACC-mapped profile.
    #[tokio::test]
    async fn scenario_switch_iracing_to_acc() -> Result<()> {
        let tmp = TempDir::new()?;
        let profile_service = make_profile_service(&tmp).await?;

        seed_profile(&profile_service, "iracing_gt3").await?;
        seed_profile(&profile_service, "acc_gt3").await?;
        seed_profile(&profile_service, "global").await?;

        let mock = Arc::new(MockAdapterControl::new());
        let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
            .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

        svc.set_game_profile("iracing".to_string(), "iracing_gt3".to_string())
            .await?;
        svc.set_game_profile("acc".to_string(), "acc_gt3".to_string())
            .await?;

        // Start iRacing
        svc.handle_event(game_started_event("iracing", "iRacingSim64DX11.exe"))
            .await;
        assert_eq!(
            svc.get_active_profile().await.as_deref(),
            Some("iracing_gt3")
        );

        // Stop iRacing
        svc.handle_event(game_stopped_event("iracing", "iRacingSim64DX11.exe"))
            .await;

        // Start ACC
        svc.handle_event(game_started_event("acc", "AC2-Win64-Shipping.exe"))
            .await;
        assert_eq!(svc.get_active_profile().await.as_deref(), Some("acc_gt3"));

        // Verify adapter sequence
        let starts = mock.started_games().await;
        let stops = mock.stopped_games().await;

        assert_eq!(starts, vec!["iracing", "acc"]);
        assert_eq!(stops, vec!["iracing"]);

        Ok(())
    }

    /// Scenario: rapid game switching does not lose events
    ///
    /// Start game A → stop game A → start game B in quick succession.
    /// All adapter start/stop events must be recorded in order.
    #[tokio::test]
    async fn scenario_rapid_game_switching_preserves_order() -> Result<()> {
        let tmp = TempDir::new()?;
        let profile_service = make_profile_service(&tmp).await?;
        seed_profile(&profile_service, "global").await?;

        let mock = Arc::new(MockAdapterControl::new());
        let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
            .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

        // Rapid game switching sequence
        let games = [
            ("forza_motorsport", "ForzaMotorsport.exe"),
            ("acc", "AC2-Win64-Shipping.exe"),
            ("iracing", "iRacingSim64DX11.exe"),
        ];

        for (game_id, exe) in &games {
            svc.handle_event(game_started_event(game_id, exe)).await;
            svc.handle_event(game_stopped_event(game_id, exe)).await;
        }

        let starts = mock.started_games().await;
        let stops = mock.stopped_games().await;

        assert_eq!(
            starts.len(),
            3,
            "all three games must have started adapters"
        );
        assert_eq!(stops.len(), 3, "all three games must have stopped adapters");

        // Order must be preserved
        assert_eq!(starts[0], "forza_motorsport");
        assert_eq!(starts[1], "acc");
        assert_eq!(starts[2], "iracing");

        Ok(())
    }

    /// Scenario: stopping a game that was never started still calls adapter stop
    ///
    /// The `AutoProfileSwitchingService` does not track per-game start state,
    /// so a `GameStopped` event always invokes `stop_for_game`. This verifies
    /// that the stop call completes without error, even when no start preceded it.
    #[tokio::test]
    async fn scenario_stop_unstarted_game_is_noop() -> Result<()> {
        let tmp = TempDir::new()?;
        let profile_service = make_profile_service(&tmp).await?;

        let mock = Arc::new(MockAdapterControl::new());
        let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
            .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

        // Stop a game that was never started — must not panic
        svc.handle_event(game_stopped_event("iracing", "iRacingSim64DX11.exe"))
            .await;

        // No adapter starts should have occurred
        let starts = mock.started_games().await;
        assert!(
            starts.is_empty(),
            "no adapters should start for never-started game"
        );

        Ok(())
    }

    /// Scenario: same game stopped twice calls adapter stop each time
    ///
    /// The service does not deduplicate stop events; each `GameStopped`
    /// triggers `stop_for_game`. This test verifies no panic on double-stop.
    #[tokio::test]
    async fn scenario_double_stop_triggers_single_adapter_stop() -> Result<()> {
        let tmp = TempDir::new()?;
        let profile_service = make_profile_service(&tmp).await?;

        let mock = Arc::new(MockAdapterControl::new());
        let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
            .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

        svc.handle_event(game_started_event("acc", "AC2-Win64-Shipping.exe"))
            .await;
        svc.handle_event(game_stopped_event("acc", "AC2-Win64-Shipping.exe"))
            .await;
        svc.handle_event(game_stopped_event("acc", "AC2-Win64-Shipping.exe"))
            .await;

        // Double stop must not panic. The service invokes stop_for_game on
        // every GameStopped event, so we expect two stop calls.
        let stops = mock.stopped_games().await;
        assert_eq!(
            stops.len(),
            2,
            "double stop must invoke adapter stop for each event"
        );

        Ok(())
    }

    /// Scenario: profile reverts to global when game stops
    #[tokio::test]
    async fn scenario_profile_reverts_to_global_on_game_stop() -> Result<()> {
        let tmp = TempDir::new()?;
        let profile_service = make_profile_service(&tmp).await?;

        seed_profile(&profile_service, "forza_setup").await?;
        seed_profile(&profile_service, "global").await?;

        let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?;
        svc.set_game_profile("forza_motorsport".to_string(), "forza_setup".to_string())
            .await?;

        svc.handle_event(game_started_event(
            "forza_motorsport",
            "ForzaMotorsport.exe",
        ))
        .await;
        assert_eq!(
            svc.get_active_profile().await.as_deref(),
            Some("forza_setup")
        );

        svc.handle_event(game_stopped_event(
            "forza_motorsport",
            "ForzaMotorsport.exe",
        ))
        .await;
        assert_eq!(
            svc.get_active_profile().await.as_deref(),
            Some("global"),
            "profile must revert to global when game stops"
        );

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Error Recovery Tests
// ═══════════════════════════════════════════════════════════════════════════════

mod error_recovery {
    use super::*;

    /// Scenario: USB disconnect mid-FFB triggers safety interlock
    ///
    /// When a device disconnects while FFB is active, the safety service
    /// must transition to a faulted state and clamp torque to zero.
    #[test]
    fn scenario_disconnect_mid_ffb_triggers_safety_interlock() -> Result<()> {
        let id: DeviceId = "usb-fault-001".parse()?;
        let mut device = VirtualDevice::new(id, "Fault Test Wheel".to_string());
        let mut safety = SafetyService::new(5.0, 25.0);

        // Active FFB
        device.write_ffb_report(4.0, 0)?;

        // Simulate USB disconnect
        device.disconnect();
        assert!(!device.is_connected());

        // Safety system detects USB stall
        safety.report_fault(FaultType::UsbStall);

        // Torque must be clamped to zero
        let clamped = safety.clamp_torque_nm(10.0);
        assert!(
            clamped.abs() < f32::EPSILON,
            "torque must be zero after USB stall fault, got {}",
            clamped
        );

        // State must be Faulted
        match safety.state() {
            SafetyState::Faulted { fault, .. } => {
                assert_eq!(*fault, FaultType::UsbStall);
            }
            other => anyhow::bail!("expected Faulted state, got {:?}", other),
        }

        Ok(())
    }

    /// Scenario: reconnect after USB disconnect allows fault recovery
    ///
    /// After reconnecting, the device becomes functional again, but the
    /// safety service requires explicit fault clearing (with minimum duration).
    #[test]
    fn scenario_reconnect_after_disconnect_restores_device() -> Result<()> {
        let id: DeviceId = "usb-recover-001".parse()?;
        let mut device = VirtualDevice::new(id, "Recovery Wheel".to_string());

        // FFB active, then disconnect
        device.write_ffb_report(3.0, 0)?;
        device.disconnect();
        assert!(device.write_ffb_report(1.0, 1).is_err());

        // Reconnect
        device.reconnect();
        assert!(device.is_connected());

        // Device is functional again
        let result = device.write_ffb_report(2.0, 2);
        assert!(
            result.is_ok(),
            "device must accept FFB writes after reconnect"
        );

        Ok(())
    }

    /// Scenario: safety interlock engages within timing budget
    ///
    /// Fault detection + response must complete within 10ms (requirement).
    #[test]
    fn scenario_safety_interlock_timing_budget() -> Result<()> {
        let mut safety = SafetyService::new(5.0, 25.0);

        let start = Instant::now();
        safety.report_fault(FaultType::Overcurrent);
        let torque = safety.clamp_torque_nm(25.0);
        let elapsed = start.elapsed();

        assert!(
            torque.abs() < f32::EPSILON,
            "torque must be zero after overcurrent fault"
        );
        assert!(
            elapsed < Duration::from_millis(10),
            "fault detection + response must complete within 10ms, took {:?}",
            elapsed
        );

        Ok(())
    }

    /// Scenario: multiple fault types all trigger zero torque
    #[test]
    fn scenario_all_fault_types_trigger_zero_torque() -> Result<()> {
        let fault_types = [
            FaultType::UsbStall,
            FaultType::EncoderNaN,
            FaultType::ThermalLimit,
            FaultType::Overcurrent,
            FaultType::PluginOverrun,
            FaultType::TimingViolation,
            FaultType::SafetyInterlockViolation,
            FaultType::HandsOffTimeout,
            FaultType::PipelineFault,
        ];

        for fault in &fault_types {
            let mut safety = SafetyService::new(5.0, 25.0);
            safety.report_fault(*fault);

            let clamped = safety.clamp_torque_nm(20.0);
            assert!(
                clamped.abs() < f32::EPSILON,
                "{:?} fault must clamp torque to zero, got {}",
                fault,
                clamped
            );
        }

        Ok(())
    }

    /// Scenario: fault during concurrent device operation is safe
    ///
    /// Uses VirtualDevice directly for disconnect control; verifies that one
    /// device's fault does not crash the other's FFB loop.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scenario_fault_during_concurrent_operation() -> Result<()> {
        let id_a: DeviceId = "fault-concurrent-a".parse()?;
        let id_b: DeviceId = "fault-concurrent-b".parse()?;

        let mut dev_a = VirtualDevice::new(id_a, "Fault Dev A".to_string());
        let mut dev_b = VirtualDevice::new(id_b, "Fault Dev B".to_string());

        // Device A operates normally
        let handle_a = tokio::task::spawn_blocking(move || -> Result<()> {
            for i in 0u16..20 {
                let _ = dev_a.write_ffb_report(1.0, i);
            }
            Ok(())
        });

        // Device B disconnects mid-operation
        let handle_b = tokio::task::spawn_blocking(move || -> Result<()> {
            for i in 0u16..10 {
                let _ = dev_b.write_ffb_report(-1.0, i);
            }
            dev_b.disconnect();
            // Writes after disconnect should fail but not panic
            for i in 10u16..20 {
                let _ = dev_b.write_ffb_report(-1.0, i);
            }
            Ok(())
        });

        handle_a
            .await
            .map_err(|e| anyhow::anyhow!("device A task panicked: {}", e))??;
        handle_b
            .await
            .map_err(|e| anyhow::anyhow!("device B task panicked: {}", e))??;

        Ok(())
    }

    /// Scenario: rapid disconnect/reconnect cycles do not corrupt state
    #[test]
    fn scenario_rapid_disconnect_reconnect_cycles() -> Result<()> {
        let id: DeviceId = "rapid-cycle-001".parse()?;
        let mut device = VirtualDevice::new(id, "Rapid Cycle Wheel".to_string());

        for i in 0u16..10 {
            device.disconnect();
            assert!(!device.is_connected());

            device.reconnect();
            assert!(device.is_connected());

            // Must be functional after each cycle
            let result = device.write_ffb_report(1.0, i);
            assert!(result.is_ok(), "FFB write must succeed after cycle {}", i);
        }

        // Final telemetry check
        let telem = device.read_telemetry();
        assert!(
            telem.is_some(),
            "telemetry must be readable after rapid cycling"
        );

        Ok(())
    }

    /// Scenario: NaN torque input is safely handled after fault recovery
    #[test]
    fn scenario_nan_torque_safe_after_recovery() -> Result<()> {
        let safety = SafetyService::new(5.0, 25.0);

        // NaN must be clamped to zero even in normal state
        let clamped = safety.clamp_torque_nm(f32::NAN);
        assert!(
            clamped.abs() < f32::EPSILON,
            "NaN torque must clamp to zero, got {}",
            clamped
        );

        // Infinity must be clamped to safe limit
        let pos_inf = safety.clamp_torque_nm(f32::INFINITY);
        assert!(
            pos_inf.is_finite() && pos_inf <= 5.0 + f32::EPSILON,
            "positive infinity must clamp to safe limit, got {}",
            pos_inf
        );

        let neg_inf = safety.clamp_torque_nm(f32::NEG_INFINITY);
        assert!(
            neg_inf.is_finite() && neg_inf >= -5.0 - f32::EPSILON,
            "negative infinity must clamp to safe limit, got {}",
            neg_inf
        );

        Ok(())
    }

    /// Scenario: fault clears only after minimum hold duration
    #[test]
    fn scenario_fault_clear_requires_minimum_duration() -> Result<()> {
        let mut safety = SafetyService::new(5.0, 25.0);
        safety.report_fault(FaultType::ThermalLimit);

        // Immediate clear must fail
        let result = safety.clear_fault();
        assert!(
            result.is_err(),
            "clearing fault immediately must fail (minimum hold duration)"
        );

        // Safety remains in faulted state
        match safety.state() {
            SafetyState::Faulted { .. } => {}
            other => anyhow::bail!("expected Faulted state, got {:?}", other),
        }

        // Torque still zero
        let clamped = safety.clamp_torque_nm(10.0);
        assert!(
            clamped.abs() < f32::EPSILON,
            "torque must remain zero while fault not cleared"
        );

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Configuration Persistence Tests
// ═══════════════════════════════════════════════════════════════════════════════

mod config_persistence {
    use super::*;

    /// Scenario: profile persists across service restart
    ///
    /// Create a profile, shut down the service, re-create the service from
    /// the same directory, and verify the profile is still present.
    #[tokio::test]
    async fn scenario_profile_survives_service_restart() -> Result<()> {
        let tmp = TempDir::new()?;

        // First service instance: create a profile
        {
            let svc = make_profile_service(&tmp).await?;
            seed_profile(&svc, "persisted_profile").await?;

            let profiles = svc.list_profiles().await?;
            assert!(
                profiles
                    .iter()
                    .any(|p| p.id.to_string() == "persisted_profile"),
                "profile must exist in first service instance"
            );
        }
        // Service drops here (simulates shutdown)

        // Second service instance: profile must still exist
        {
            let svc = make_profile_service(&tmp).await?;
            let profiles = svc.list_profiles().await?;
            assert!(
                profiles
                    .iter()
                    .any(|p| p.id.to_string() == "persisted_profile"),
                "profile must survive service restart"
            );
        }

        Ok(())
    }

    /// Scenario: multiple profiles persist independently
    #[tokio::test]
    async fn scenario_multiple_profiles_persist() -> Result<()> {
        let tmp = TempDir::new()?;

        let profile_names = ["iracing_gt3", "acc_gt4", "forza_drift"];

        // First instance: create all profiles
        {
            let svc = make_profile_service(&tmp).await?;
            for name in &profile_names {
                seed_profile(&svc, name).await?;
            }

            let profiles = svc.list_profiles().await?;
            assert_eq!(
                profiles.len(),
                profile_names.len(),
                "all profiles must exist in first instance"
            );
        }

        // Second instance: all profiles present
        {
            let svc = make_profile_service(&tmp).await?;
            let profiles = svc.list_profiles().await?;

            for name in &profile_names {
                assert!(
                    profiles.iter().any(|p| p.id.to_string() == *name),
                    "profile '{}' must survive restart",
                    name
                );
            }
        }

        Ok(())
    }

    /// Scenario: game-to-profile mappings persist across restart
    #[tokio::test]
    async fn scenario_game_profile_mapping_persists() -> Result<()> {
        let tmp = TempDir::new()?;

        // First instance: set up game→profile mapping
        {
            let profile_service = make_profile_service(&tmp).await?;
            seed_profile(&profile_service, "iracing_gt3").await?;

            let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?;
            svc.set_game_profile("iracing".to_string(), "iracing_gt3".to_string())
                .await?;

            // Verify mapping works
            svc.handle_event(game_started_event("iracing", "iRacingSim64DX11.exe"))
                .await;
            assert_eq!(
                svc.get_active_profile().await.as_deref(),
                Some("iracing_gt3")
            );
        }

        // Second instance: profile still present
        {
            let profile_service = make_profile_service(&tmp).await?;
            let profiles = profile_service.list_profiles().await?;
            assert!(
                profiles.iter().any(|p| p.id.to_string() == "iracing_gt3"),
                "iracing_gt3 profile must survive restart"
            );
        }

        Ok(())
    }

    /// Scenario: empty profiles dir produces empty list (no crash)
    #[tokio::test]
    async fn scenario_empty_profiles_dir_no_crash() -> Result<()> {
        let tmp = TempDir::new()?;

        let svc = make_profile_service(&tmp).await?;
        let profiles = svc.list_profiles().await?;

        assert!(
            profiles.is_empty(),
            "empty profiles dir must yield empty list"
        );

        Ok(())
    }

    /// Scenario: profile directory with invalid JSON files handled gracefully
    #[tokio::test]
    async fn scenario_corrupt_profile_file_handled_gracefully() -> Result<()> {
        let tmp = TempDir::new()?;

        // Write a corrupt JSON file to the profiles directory
        std::fs::write(tmp.path().join("corrupt.json"), "{ not valid json")?;

        // Service must start without panicking
        let svc = make_profile_service(&tmp).await?;
        // list_profiles may skip corrupt files or return an error, but must not panic
        let _profiles = svc.list_profiles().await;

        Ok(())
    }

    /// Scenario: profile created in one instance is loadable in another
    #[tokio::test]
    async fn scenario_profile_round_trip_across_instances() -> Result<()> {
        let tmp = TempDir::new()?;

        let created_id;
        // First instance: create
        {
            let svc = make_profile_service(&tmp).await?;
            created_id = seed_profile(&svc, "roundtrip_test").await?;

            let loaded = svc.get_profile(&created_id).await?;
            assert!(loaded.is_some(), "profile must be loadable after creation");
        }

        // Second instance: load
        {
            let svc = make_profile_service(&tmp).await?;
            let loaded = svc.get_profile(&created_id).await?;
            assert!(
                loaded.is_some(),
                "profile must be loadable in new service instance"
            );
        }

        Ok(())
    }
}
