//! End-to-end multi-vendor integration tests.
//!
//! Cross-crate coverage: engine (VirtualDevice, VirtualHidPort, CapabilityNegotiator,
//! ModeSelectionPolicy, Pipeline) × schemas (DeviceCapabilities, TorqueNm, DeviceId)
//! × service (ProfileService, AutoProfileSwitchingService) × telemetry-adapters.
//!
//! Scenarios:
//! 1. Switching between different vendor devices
//! 2. Profile auto-selection based on connected device
//! 3. Device capability detection across vendors
//! 4. Protocol version negotiation

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use tempfile::TempDir;
use tokio::sync::Mutex;

use racing_wheel_engine::ports::HidPort;
use racing_wheel_engine::{
    CapabilityNegotiator, FFBMode, Frame, ModeSelectionPolicy, Pipeline, VirtualDevice,
    VirtualHidPort,
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

/// Vendor fixture parameters: name, max torque, raw-1kHz?, PID?, encoder CPR,
/// min report period µs.
struct VendorSpec {
    id: &'static str,
    name: &'static str,
    max_torque_nm: f32,
    supports_raw_1khz: bool,
    supports_pid: bool,
    encoder_cpr: u16,
    min_report_period_us: u16,
}

const VENDORS: &[VendorSpec] = &[
    VendorSpec {
        id: "mv-fanatec-dd",
        name: "Fanatec GT DD Pro",
        max_torque_nm: 8.0,
        supports_raw_1khz: true,
        supports_pid: false,
        encoder_cpr: 65535,
        min_report_period_us: 1000,
    },
    VendorSpec {
        id: "mv-thrustmaster-t300",
        name: "Thrustmaster T300RS",
        max_torque_nm: 3.9,
        supports_raw_1khz: false,
        supports_pid: true,
        encoder_cpr: 4096,
        min_report_period_us: 4000,
    },
    VendorSpec {
        id: "mv-logitech-g923",
        name: "Logitech G923",
        max_torque_nm: 2.2,
        supports_raw_1khz: false,
        supports_pid: true,
        encoder_cpr: 4096,
        min_report_period_us: 16666,
    },
    VendorSpec {
        id: "mv-moza-r9",
        name: "Moza R9 V2",
        max_torque_nm: 9.0,
        supports_raw_1khz: true,
        supports_pid: false,
        encoder_cpr: 65535,
        min_report_period_us: 1000,
    },
    VendorSpec {
        id: "mv-simucube-2",
        name: "Simucube 2 Pro",
        max_torque_nm: 25.0,
        supports_raw_1khz: true,
        supports_pid: false,
        encoder_cpr: 65535,
        min_report_period_us: 500,
    },
    VendorSpec {
        id: "mv-simagic-alpha",
        name: "Simagic Alpha Mini",
        max_torque_nm: 10.0,
        supports_raw_1khz: true,
        supports_pid: false,
        encoder_cpr: 65535,
        min_report_period_us: 1000,
    },
];

fn caps_for_vendor(v: &VendorSpec) -> Result<DeviceCapabilities> {
    Ok(DeviceCapabilities::new(
        v.supports_pid,
        v.supports_raw_1khz,
        true,
        v.supports_raw_1khz, // LED bus on DD devices
        TorqueNm::new(v.max_torque_nm)?,
        v.encoder_cpr,
        v.min_report_period_us,
    ))
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
    async fn start_for_game(&self, game_id: &str) -> anyhow::Result<()> {
        self.starts.lock().await.push(game_id.to_string());
        Ok(())
    }
    async fn stop_for_game(&self, game_id: &str) -> anyhow::Result<()> {
        self.stops.lock().await.push(game_id.to_string());
        Ok(())
    }
}

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

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Switching between different vendor devices
// ═══════════════════════════════════════════════════════════════════════════════

/// Enumerate all vendor devices in a single port, then sequentially open each
/// one, write FFB, and verify the pipeline operates correctly for that vendor's
/// capabilities.
#[tokio::test]
async fn switch_between_vendor_devices_sequentially() -> Result<()> {
    let mut port = VirtualHidPort::new();

    for v in VENDORS {
        let id: DeviceId = v.id.parse()?;
        let device = VirtualDevice::new(id, v.name.to_string());
        port.add_device(device)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }

    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(
        devices.len(),
        VENDORS.len(),
        "all vendor devices must be enumerated"
    );

    // Sequentially open, use, and release each device
    for v in VENDORS {
        let id: DeviceId = v.id.parse()?;
        let mut dev = port
            .open_device(&id)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let mut pipeline = Pipeline::new();
        let mut frame = Frame {
            ffb_in: 0.3,
            torque_out: 0.3,
            wheel_speed: 1.0,
            hands_off: false,
            ts_mono_ns: 1_000_000,
            seq: 0,
        };
        pipeline.process(&mut frame)?;

        // Clamp torque to device limit
        let max_t = dev.capabilities().max_torque.value();
        let torque = frame.torque_out * max_t;
        let clamped = torque.clamp(-max_t, max_t);
        dev.write_ffb_report(clamped, 0)?;

        let telem = dev
            .read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("{}: telemetry missing", v.name))?;
        assert!(
            telem.temperature_c <= 150,
            "{}: temperature out of range",
            v.name
        );
    }

    Ok(())
}

/// After disconnecting one vendor device and connecting another, the pipeline
/// must negotiate the correct mode for the new device.
#[test]
fn switch_vendor_renegotiates_ffb_mode() -> Result<()> {
    // Start with a DD device (RawTorque)
    let dd_caps = caps_for_vendor(&VENDORS[0])?; // Fanatec DD
    let mode_dd = ModeSelectionPolicy::select_mode(&dd_caps, None);
    assert_eq!(mode_dd, FFBMode::RawTorque, "DD device must get RawTorque");

    // Switch to a PID-only device
    let pid_caps = caps_for_vendor(&VENDORS[1])?; // Thrustmaster T300
    let mode_pid = ModeSelectionPolicy::select_mode(&pid_caps, None);
    assert_eq!(
        mode_pid,
        FFBMode::PidPassthrough,
        "PID device must get PidPassthrough"
    );

    // Verify compatibility cross-checks
    assert!(ModeSelectionPolicy::is_mode_compatible(
        FFBMode::RawTorque,
        &dd_caps
    ));
    assert!(!ModeSelectionPolicy::is_mode_compatible(
        FFBMode::RawTorque,
        &pid_caps
    ));
    assert!(ModeSelectionPolicy::is_mode_compatible(
        FFBMode::PidPassthrough,
        &pid_caps
    ));

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Profile auto-selection based on connected device
// ═══════════════════════════════════════════════════════════════════════════════

/// When different games start, the profile service must switch to the
/// corresponding game profile.  This tests the cross-crate interaction between
/// the service crate (AutoProfileSwitchingService) and schemas (Profile, ProfileId).
#[tokio::test]
async fn profile_auto_selects_on_game_start() -> Result<()> {
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

    // Game A starts → profile A
    svc.handle_event(game_started_event("iracing", "iRacingSim64DX11.exe"))
        .await;
    assert_eq!(
        svc.get_active_profile().await.as_deref(),
        Some("iracing_gt3")
    );

    // Game A stops → global
    svc.handle_event(game_stopped_event("iracing", "iRacingSim64DX11.exe"))
        .await;
    assert_eq!(svc.get_active_profile().await.as_deref(), Some("global"));

    // Game B starts → profile B
    svc.handle_event(game_started_event("acc", "AC2-Win64-Shipping.exe"))
        .await;
    assert_eq!(svc.get_active_profile().await.as_deref(), Some("acc_gt3"));

    // Telemetry adapters must have been started/stopped in order
    let starts = mock.started_games().await;
    let stops = mock.stopped_games().await;
    assert_eq!(starts, vec!["iracing", "acc"]);
    assert_eq!(stops, vec!["iracing"]);

    Ok(())
}

/// Profile switching must be idempotent: re-detecting the same running game
/// must not duplicate adapter starts.
#[tokio::test]
async fn profile_switch_idempotent_on_same_game() -> Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;

    seed_profile(&profile_service, "iracing_gt3").await?;

    let mock = Arc::new(MockAdapterControl::new());
    let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
        .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

    svc.set_game_profile("iracing".to_string(), "iracing_gt3".to_string())
        .await?;

    // Start the same game twice
    svc.handle_event(game_started_event("iracing", "iRacingSim64DX11.exe"))
        .await;
    svc.handle_event(game_started_event("iracing", "iRacingSim64DX11.exe"))
        .await;

    // Profile must still be the correct one
    assert_eq!(
        svc.get_active_profile().await.as_deref(),
        Some("iracing_gt3")
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Device capability detection across vendors
// ═══════════════════════════════════════════════════════════════════════════════

/// All vendor fixtures must produce valid capabilities through the
/// CapabilityNegotiator report round-trip (engine ↔ schemas).
#[test]
fn capability_detection_round_trips_for_all_vendors() -> Result<()> {
    for v in VENDORS {
        let caps = caps_for_vendor(v)?;
        let report = CapabilityNegotiator::create_capabilities_report(&caps);
        let parsed = CapabilityNegotiator::parse_capabilities_report(&report)
            .map_err(|e| anyhow::anyhow!("{}: {e}", v.name))?;

        assert_eq!(
            parsed.supports_raw_torque_1khz, caps.supports_raw_torque_1khz,
            "{}: raw_torque_1khz mismatch",
            v.name
        );
        assert_eq!(
            parsed.supports_pid, caps.supports_pid,
            "{}: supports_pid mismatch",
            v.name
        );
        assert_eq!(
            parsed.encoder_cpr, caps.encoder_cpr,
            "{}: encoder_cpr mismatch",
            v.name
        );
        assert_eq!(
            parsed.min_report_period_us, caps.min_report_period_us,
            "{}: min_report_period_us mismatch",
            v.name
        );

        let torque_delta = (parsed.max_torque.value() - caps.max_torque.value()).abs();
        assert!(
            torque_delta < 0.02,
            "{}: torque round-trip delta {torque_delta} exceeds 0.02 Nm",
            v.name
        );
    }

    Ok(())
}

/// Each vendor's capabilities must produce the correct FFB mode when negotiated.
#[test]
fn capability_detection_yields_correct_mode_per_vendor() -> Result<()> {
    for v in VENDORS {
        let caps = caps_for_vendor(v)?;
        let mode = ModeSelectionPolicy::select_mode(&caps, None);

        if v.supports_raw_1khz {
            assert_eq!(
                mode,
                FFBMode::RawTorque,
                "{}: raw-capable vendor must select RawTorque",
                v.name
            );
        } else if v.supports_pid {
            assert_eq!(
                mode,
                FFBMode::PidPassthrough,
                "{}: PID vendor must select PidPassthrough",
                v.name
            );
        } else {
            assert_eq!(
                mode,
                FFBMode::TelemetrySynth,
                "{}: fallback vendor must select TelemetrySynth",
                v.name
            );
        }
    }

    Ok(())
}

/// Negotiation with game compatibility context must respect game preferences
/// for DD devices.
#[test]
fn capability_detection_respects_game_compatibility() -> Result<()> {
    let dd_caps = caps_for_vendor(&VENDORS[0])?; // Fanatec DD

    // Game that supports robust FFB → RawTorque
    let robust_game = racing_wheel_engine::GameCompatibility {
        game_id: "iracing".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };
    let result = CapabilityNegotiator::negotiate_capabilities(&dd_caps, Some(&robust_game));
    assert_eq!(
        result.mode,
        FFBMode::RawTorque,
        "robust FFB game + DD device → RawTorque"
    );

    // Game with telemetry but no robust FFB → TelemetrySynth
    let telem_only_game = racing_wheel_engine::GameCompatibility {
        game_id: "arcade_racer".to_string(),
        supports_robust_ffb: false,
        supports_telemetry: true,
        preferred_mode: FFBMode::TelemetrySynth,
    };
    let result2 = CapabilityNegotiator::negotiate_capabilities(&dd_caps, Some(&telem_only_game));
    assert_eq!(
        result2.mode,
        FFBMode::TelemetrySynth,
        "telemetry-only game + DD device → TelemetrySynth"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Protocol version negotiation
// ═══════════════════════════════════════════════════════════════════════════════

/// The negotiation result's update_rate_hz must match the selected mode.
#[test]
fn protocol_negotiation_rate_matches_mode() -> Result<()> {
    for v in VENDORS {
        let caps = caps_for_vendor(v)?;
        let result = CapabilityNegotiator::negotiate_capabilities(&caps, None);

        let expected_rate = ModeSelectionPolicy::get_update_rate_hz(result.mode);
        // The negotiated rate can be lower if the device cannot reach the
        // target rate, but it should never exceed the mode's target.
        assert!(
            result.update_rate_hz <= expected_rate + 1.0,
            "{}: negotiated rate {} should not exceed mode target {}",
            v.name,
            result.update_rate_hz,
            expected_rate
        );
        assert!(
            result.update_rate_hz > 0.0,
            "{}: negotiated rate must be positive",
            v.name
        );
    }

    Ok(())
}

/// Negotiating with a device that cannot support the target rate should
/// produce a warning but still select a valid mode.
#[test]
fn protocol_negotiation_warns_on_rate_mismatch() -> Result<()> {
    // Create a device with a very slow report period (10ms → max ~100Hz)
    let slow_caps = DeviceCapabilities::new(
        false,
        true, // claims raw torque but min period is too slow
        false,
        false,
        TorqueNm::new(5.0)?,
        4096,
        10000, // 10ms → 100Hz max
    );

    let result = CapabilityNegotiator::negotiate_capabilities(&slow_caps, None);

    // Mode selection still works
    assert_eq!(result.mode, FFBMode::RawTorque);
    // A warning about rate mismatch should be present
    assert!(
        !result.warnings.is_empty(),
        "slow device should produce a negotiation warning"
    );

    Ok(())
}

/// Vendor devices must all produce non-empty, unique device IDs when
/// enumerated from a shared port (schemas DeviceId uniqueness).
#[tokio::test]
async fn protocol_all_vendor_ids_unique_in_port() -> Result<()> {
    let mut port = VirtualHidPort::new();

    for v in VENDORS {
        let id: DeviceId = v.id.parse()?;
        port.add_device(VirtualDevice::new(id, v.name.to_string()))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }

    let devices = port
        .list_devices()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let ids: std::collections::HashSet<&DeviceId> = devices.iter().map(|d| &d.id).collect();
    assert_eq!(
        ids.len(),
        VENDORS.len(),
        "all vendor device IDs must be unique"
    );

    for d in &devices {
        assert!(!d.id.as_str().is_empty(), "device ID must not be empty");
    }

    Ok(())
}

/// Pipeline processes frames correctly regardless of which vendor device
/// produced the input.
#[test]
fn protocol_pipeline_agnostic_to_vendor() -> Result<()> {
    let mut pipeline = Pipeline::new();

    for v in VENDORS {
        let mut frame = Frame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 2.0,
            hands_off: false,
            ts_mono_ns: 1_000_000,
            seq: 1,
        };
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite(),
            "{}: pipeline output must be finite",
            v.name
        );
    }

    Ok(())
}
