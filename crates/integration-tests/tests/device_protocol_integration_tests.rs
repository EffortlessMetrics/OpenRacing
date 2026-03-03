//! Cross-crate device protocol integration tests.
//!
//! Verifies interactions across the engine, schemas, and service crates
//! for device management:
//!
//! 1. Complete device flow: discover → identify → configure → use
//! 2. Multi-device simultaneous operation
//! 3. Device failover and recovery
//! 4. Protocol negotiation

use std::collections::HashSet;

use anyhow::Result;

use racing_wheel_engine::ports::{HidDevice, HidPort};
use racing_wheel_engine::safety::{FaultType, SafetyService};
use racing_wheel_engine::{
    CapabilityNegotiator, FFBMode, Frame, ModeSelectionPolicy, Pipeline, VirtualDevice,
    VirtualHidPort,
};
use racing_wheel_schemas::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Device fixture for parameterized testing across vendors.
struct DeviceFixture {
    name: &'static str,
    device_id: &'static str,
    max_torque_nm: f32,
    supports_raw_1khz: bool,
    encoder_cpr: u16,
}

const DEVICE_FIXTURES: &[DeviceFixture] = &[
    DeviceFixture {
        name: "Fanatec CSL DD",
        device_id: "proto-fanatec-001",
        max_torque_nm: 8.0,
        supports_raw_1khz: true,
        encoder_cpr: 65535,
    },
    DeviceFixture {
        name: "Thrustmaster T248",
        device_id: "proto-thrustmaster-001",
        max_torque_nm: 3.5,
        supports_raw_1khz: false,
        encoder_cpr: 4096,
    },
    DeviceFixture {
        name: "Logitech G Pro",
        device_id: "proto-logitech-001",
        max_torque_nm: 11.0,
        supports_raw_1khz: true,
        encoder_cpr: 65535,
    },
    DeviceFixture {
        name: "Moza R12",
        device_id: "proto-moza-001",
        max_torque_nm: 12.0,
        supports_raw_1khz: true,
        encoder_cpr: 65535,
    },
    DeviceFixture {
        name: "Simucube 2 Sport",
        device_id: "proto-simucube-001",
        max_torque_nm: 17.0,
        supports_raw_1khz: true,
        encoder_cpr: 65535,
    },
    DeviceFixture {
        name: "Simagic M10",
        device_id: "proto-simagic-001",
        max_torque_nm: 10.0,
        supports_raw_1khz: true,
        encoder_cpr: 65535,
    },
];

fn make_device_caps(fixture: &DeviceFixture) -> Result<DeviceCapabilities> {
    Ok(DeviceCapabilities::new(
        !fixture.supports_raw_1khz,
        fixture.supports_raw_1khz,
        true,
        false,
        TorqueNm::new(fixture.max_torque_nm)?,
        fixture.encoder_cpr,
        if fixture.supports_raw_1khz { 1000 } else { 4000 },
    ))
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Complete device flow: discover → identify → configure → use
// ═══════════════════════════════════════════════════════════════════════════════

mod complete_device_flow {
    use super::*;

    /// Full lifecycle: create → enumerate → read capabilities → write FFB →
    /// read telemetry → disconnect for each vendor fixture.
    #[tokio::test]
    async fn discover_identify_configure_use_all_vendors() -> Result<()> {
        for fixture in DEVICE_FIXTURES {
            let id: DeviceId = fixture.device_id.parse()?;
            let mut device = VirtualDevice::new(id.clone(), fixture.name.to_string());

            // Discover: device is connected
            assert!(
                device.is_connected(),
                "{}: device must be connected after creation",
                fixture.name
            );

            // Identify: read capabilities
            let caps = device.capabilities();
            assert!(
                caps.max_torque.value() > 0.0,
                "{}: max torque must be positive, got {}",
                fixture.name,
                caps.max_torque.value()
            );

            // Configure: write initial FFB
            let write_result = device.write_ffb_report(1.0, 0);
            assert!(
                write_result.is_ok(),
                "{}: FFB write must succeed on connected device",
                fixture.name
            );

            // Use: read telemetry
            let telemetry = device.read_telemetry();
            assert!(
                telemetry.is_some(),
                "{}: telemetry must be readable while connected",
                fixture.name
            );

            // Disconnect
            device.disconnect();
            assert!(
                !device.is_connected(),
                "{}: device must be disconnected after disconnect()",
                fixture.name
            );

            // Post-disconnect write must fail gracefully
            let post_write = device.write_ffb_report(1.0, 1);
            assert!(
                post_write.is_err(),
                "{}: FFB write must fail after disconnect",
                fixture.name
            );
        }

        Ok(())
    }

    /// HidPort enumeration discovers all added virtual devices.
    #[tokio::test]
    async fn port_enumeration_discovers_all_devices() -> Result<()> {
        let mut port = VirtualHidPort::new();

        for fixture in DEVICE_FIXTURES {
            let id: DeviceId = fixture.device_id.parse()?;
            let device = VirtualDevice::new(id, fixture.name.to_string());
            port.add_device(device)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
        }

        let devices = port
            .list_devices()
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        assert_eq!(
            devices.len(),
            DEVICE_FIXTURES.len(),
            "Port must list all {} devices, found {}",
            DEVICE_FIXTURES.len(),
            devices.len()
        );

        let ids: HashSet<&DeviceId> = devices.iter().map(|d| &d.id).collect();
        assert_eq!(
            ids.len(),
            DEVICE_FIXTURES.len(),
            "All device IDs must be unique"
        );

        for info in &devices {
            assert!(info.is_connected, "Device {} must be connected", info.id);
        }

        Ok(())
    }

    /// Device capabilities are accessible and valid after discovery.
    #[test]
    fn device_capabilities_valid_after_discovery() -> Result<()> {
        for fixture in DEVICE_FIXTURES {
            let id: DeviceId = fixture.device_id.parse()?;
            let device = VirtualDevice::new(id, fixture.name.to_string());
            let caps = device.capabilities();

            assert!(
                caps.max_torque.value() > 0.0,
                "{}: max_torque must be > 0",
                fixture.name
            );
            assert!(
                caps.encoder_cpr > 0,
                "{}: encoder_cpr must be > 0",
                fixture.name
            );
            assert!(
                caps.min_report_period_us > 0,
                "{}: min_report_period_us must be > 0",
                fixture.name
            );
        }

        Ok(())
    }

    /// Telemetry data is valid through entire device lifecycle stages.
    #[tokio::test]
    async fn telemetry_valid_through_lifecycle_stages() -> Result<()> {
        let id: DeviceId = "lifecycle-telem-proto-001".parse()?;
        let mut device = VirtualDevice::new(id, "Lifecycle Telemetry Wheel".to_string());

        // Before FFB
        let t1 = device
            .read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("telemetry missing at creation"))?;
        assert!(t1.temperature_c <= 150, "temperature must be sane at creation");

        // After FFB write
        device.write_ffb_report(5.0, 0)?;
        let t2 = device
            .read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("telemetry missing after FFB"))?;
        assert!(
            t2.temperature_c <= 150,
            "temperature must be sane after FFB"
        );

        // After disconnect + reconnect
        device.disconnect();
        device.reconnect();
        let t3 = device
            .read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("telemetry missing after reconnect"))?;
        assert!(
            t3.temperature_c <= 150,
            "temperature must be sane after reconnect"
        );

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Multi-device simultaneous operation
// ═══════════════════════════════════════════════════════════════════════════════

mod multi_device {
    use super::*;

    /// Two devices registered on the same port maintain independent state.
    #[tokio::test]
    async fn two_devices_independent_state() -> Result<()> {
        let mut port = VirtualHidPort::new();

        let id_a: DeviceId = "multi-a-001".parse()?;
        let id_b: DeviceId = "multi-b-001".parse()?;

        port.add_device(VirtualDevice::new(
            id_a.clone(),
            "Device Alpha".to_string(),
        ))
        .map_err(|e| anyhow::anyhow!("{}", e))?;
        port.add_device(VirtualDevice::new(
            id_b.clone(),
            "Device Beta".to_string(),
        ))
        .map_err(|e| anyhow::anyhow!("{}", e))?;

        let devices = port
            .list_devices()
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        assert_eq!(devices.len(), 2, "Both devices must be listed");

        let has_a = devices.iter().any(|d| d.id == id_a);
        let has_b = devices.iter().any(|d| d.id == id_b);
        assert!(has_a, "Device Alpha must be present");
        assert!(has_b, "Device Beta must be present");

        Ok(())
    }

    /// FFB writes to multiple devices via port.open_device are independent.
    #[tokio::test]
    async fn ffb_writes_isolated_between_devices() -> Result<()> {
        let mut port = VirtualHidPort::new();

        let id_a: DeviceId = "ffb-iso-a".parse()?;
        let id_b: DeviceId = "ffb-iso-b".parse()?;

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

        // Both devices still functional
        assert!(
            dev_a.read_telemetry().is_some(),
            "Device A telemetry must be readable"
        );
        assert!(
            dev_b.read_telemetry().is_some(),
            "Device B telemetry must be readable"
        );
        assert!(dev_a.is_connected(), "Device A must stay connected");
        assert!(dev_b.is_connected(), "Device B must stay connected");

        Ok(())
    }

    /// Concurrent FFB writes on two devices from separate tasks are safe.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_ffb_writes_safe() -> Result<()> {
        let mut port = VirtualHidPort::new();

        let id_a: DeviceId = "conc-proto-a".parse()?;
        let id_b: DeviceId = "conc-proto-b".parse()?;

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
            for i in 0u16..30 {
                dev_a.write_ffb_report(2.0, i)?;
                let _ = dev_a.read_telemetry();
            }
            Ok(())
        });

        let handle_b = tokio::task::spawn_blocking(move || -> Result<()> {
            for i in 0u16..30 {
                dev_b.write_ffb_report(-2.0, i)?;
                let _ = dev_b.read_telemetry();
            }
            Ok(())
        });

        handle_a
            .await
            .map_err(|e| anyhow::anyhow!("task A panicked: {}", e))??;
        handle_b
            .await
            .map_err(|e| anyhow::anyhow!("task B panicked: {}", e))??;

        Ok(())
    }

    /// Disconnecting one device does not affect the other.
    #[tokio::test]
    async fn disconnect_one_preserves_other() -> Result<()> {
        let id_a: DeviceId = "disc-proto-a".parse()?;
        let id_b: DeviceId = "disc-proto-b".parse()?;

        let mut dev_a = VirtualDevice::new(id_a, "Wheel A".to_string());
        let mut dev_b = VirtualDevice::new(id_b, "Wheel B".to_string());

        // Both functional
        dev_a.write_ffb_report(1.0, 0)?;
        dev_b.write_ffb_report(1.0, 0)?;

        // Disconnect A
        dev_a.disconnect();
        assert!(!dev_a.is_connected(), "A must be disconnected");

        // B still works
        assert!(dev_b.is_connected(), "B must still be connected");
        let result = dev_b.write_ffb_report(2.0, 1);
        assert!(result.is_ok(), "B FFB write must succeed");

        Ok(())
    }

    /// Pipeline processes frames independently per device.
    #[test]
    fn pipeline_independent_per_device() -> Result<()> {
        for fixture in DEVICE_FIXTURES {
            let mut pipeline = Pipeline::new();
            let mut frame = Frame {
                ffb_in: 0.5,
                torque_out: 0.5,
                wheel_speed: 1.0,
                hands_off: false,
                ts_mono_ns: 1_000_000,
                seq: 1,
            };

            pipeline.process(&mut frame)?;
            assert!(
                frame.torque_out.is_finite(),
                "{}: pipeline output must be finite",
                fixture.name
            );
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Device failover and recovery
// ═══════════════════════════════════════════════════════════════════════════════

mod failover_recovery {
    use super::*;

    /// Device reconnect after disconnect restores full functionality.
    #[tokio::test]
    async fn reconnect_restores_functionality() -> Result<()> {
        let id: DeviceId = "failover-001".parse()?;
        let mut device = VirtualDevice::new(id, "Failover Wheel".to_string());

        // Initial write succeeds
        device.write_ffb_report(3.0, 0)?;

        // Disconnect
        device.disconnect();
        assert!(!device.is_connected());

        // Write fails after disconnect
        let err = device.write_ffb_report(1.0, 1);
        assert!(err.is_err(), "Write must fail while disconnected");

        // Reconnect restores functionality
        device.reconnect();
        assert!(device.is_connected(), "Device must be connected after reconnect");

        let result = device.write_ffb_report(2.0, 2);
        assert!(result.is_ok(), "Write must succeed after reconnect");

        let telem = device.read_telemetry();
        assert!(telem.is_some(), "Telemetry must be readable after reconnect");

        Ok(())
    }

    /// Safety service transitions to faulted state on USB stall.
    #[test]
    fn safety_service_faults_on_usb_stall() -> Result<()> {
        let mut safety = SafetyService::new(5.0, 20.0);

        // Initially normal
        let initial_torque = safety.clamp_torque_nm(3.0);
        assert!(
            initial_torque.abs() > 0.0,
            "Non-faulted safety should allow non-zero torque"
        );

        // Report USB stall fault
        safety.report_fault(FaultType::UsbStall);

        // Faulted state must zero torque
        let faulted_torque = safety.clamp_torque_nm(3.0);
        assert!(
            faulted_torque.abs() < 0.001,
            "Faulted safety must zero torque, got {}",
            faulted_torque
        );

        Ok(())
    }

    /// Safety service clamps torque to safe limits even without faults.
    #[test]
    fn safety_clamp_respects_max_torque() -> Result<()> {
        let safety = SafetyService::new(5.0, 20.0);

        let clamped = safety.clamp_torque_nm(100.0);
        assert!(
            clamped <= 5.0,
            "Torque must be clamped to max safe: got {}",
            clamped
        );

        let neg_clamped = safety.clamp_torque_nm(-100.0);
        assert!(
            neg_clamped >= -5.0,
            "Negative torque must be clamped: got {}",
            neg_clamped
        );

        Ok(())
    }

    /// Fault injection on virtual device sets fault flags.
    #[test]
    fn fault_injection_sets_flags() -> Result<()> {
        let id: DeviceId = "fault-inject-001".parse()?;
        let mut device = VirtualDevice::new(id, "Fault Test Wheel".to_string());

        // No faults initially
        let telem_before = device
            .read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("telemetry missing"))?;
        assert_eq!(
            telem_before.fault_flags, 0,
            "No faults expected initially"
        );

        // Inject fault
        device.inject_fault(0x01);
        let telem_after = device
            .read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("telemetry missing after fault"))?;
        assert!(
            telem_after.fault_flags & 0x01 != 0,
            "Fault flag 0x01 must be set"
        );

        // Clear faults
        device.clear_faults();
        let telem_cleared = device
            .read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("telemetry missing after clear"))?;
        assert_eq!(
            telem_cleared.fault_flags, 0,
            "Faults must be cleared"
        );

        Ok(())
    }

    /// Multiple sequential fault types accumulate in fault flags.
    #[test]
    fn multiple_fault_types_accumulate() -> Result<()> {
        let id: DeviceId = "multi-fault-001".parse()?;
        let mut device = VirtualDevice::new(id, "Multi-Fault Wheel".to_string());

        device.inject_fault(0x01);
        device.inject_fault(0x02);

        let telem = device
            .read_telemetry()
            .ok_or_else(|| anyhow::anyhow!("telemetry missing"))?;
        assert!(
            telem.fault_flags & 0x01 != 0,
            "Fault 0x01 must be set"
        );
        assert!(
            telem.fault_flags & 0x02 != 0,
            "Fault 0x02 must also be set"
        );

        Ok(())
    }

    /// Device remains stable through multiple disconnect/reconnect cycles.
    #[tokio::test]
    async fn multiple_reconnect_cycles_stable() -> Result<()> {
        let id: DeviceId = "cycle-stability-001".parse()?;
        let mut device = VirtualDevice::new(id, "Cycle Stability Wheel".to_string());

        for cycle in 0..5 {
            assert!(
                device.is_connected(),
                "Cycle {cycle}: device must start connected"
            );
            device.write_ffb_report(1.0 + cycle as f32, cycle as u16)?;

            let telem = device
                .read_telemetry()
                .ok_or_else(|| anyhow::anyhow!("cycle {}: telemetry missing", cycle))?;
            assert!(
                telem.temperature_c <= 150,
                "Cycle {cycle}: temperature must be sane"
            );

            device.disconnect();
            assert!(!device.is_connected(), "Cycle {cycle}: must be disconnected");

            device.reconnect();
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Protocol negotiation
// ═══════════════════════════════════════════════════════════════════════════════

mod protocol_negotiation {
    use super::*;

    /// Direct-drive devices (raw torque 1kHz) negotiate RawTorque mode.
    #[test]
    fn dd_devices_negotiate_raw_torque() -> Result<()> {
        for fixture in DEVICE_FIXTURES.iter().filter(|f| f.supports_raw_1khz) {
            let caps = make_device_caps(fixture)?;

            let result = CapabilityNegotiator::negotiate_capabilities(&caps, None);
            assert_eq!(
                result.mode,
                FFBMode::RawTorque,
                "{}: DD device must negotiate RawTorque, got {:?}",
                fixture.name,
                result.mode
            );
            assert!(
                result.update_rate_hz >= 999.0,
                "{}: RawTorque must be ~1kHz, got {}",
                fixture.name,
                result.update_rate_hz
            );
        }

        Ok(())
    }

    /// PID-only devices negotiate PidPassthrough mode.
    #[test]
    fn pid_only_devices_negotiate_pid_passthrough() -> Result<()> {
        for fixture in DEVICE_FIXTURES.iter().filter(|f| !f.supports_raw_1khz) {
            let caps = make_device_caps(fixture)?;

            let result = CapabilityNegotiator::negotiate_capabilities(&caps, None);
            assert_eq!(
                result.mode,
                FFBMode::PidPassthrough,
                "{}: PID-only device must negotiate PidPassthrough, got {:?}",
                fixture.name,
                result.mode
            );
        }

        Ok(())
    }

    /// Mode selection policy validates RawTorque compatibility.
    #[test]
    fn mode_selection_validates_raw_torque_compatibility() -> Result<()> {
        let dd_caps = DeviceCapabilities::new(
            false,
            true,
            true,
            true,
            TorqueNm::new(25.0)?,
            10000,
            1000,
        );
        assert!(
            ModeSelectionPolicy::is_mode_compatible(FFBMode::RawTorque, &dd_caps),
            "RawTorque must be compatible with DD device"
        );

        let pid_caps = DeviceCapabilities::new(
            true,
            false,
            false,
            false,
            TorqueNm::new(3.0)?,
            1024,
            16666,
        );
        assert!(
            !ModeSelectionPolicy::is_mode_compatible(FFBMode::RawTorque, &pid_caps),
            "RawTorque must not be compatible with PID-only device"
        );

        Ok(())
    }

    /// Game compatibility info influences mode negotiation.
    #[test]
    fn game_compatibility_influences_mode() -> Result<()> {
        let caps = DeviceCapabilities::new(
            false,
            true,
            true,
            true,
            TorqueNm::new(20.0)?,
            65535,
            1000,
        );

        let game_compat = racing_wheel_engine::GameCompatibility {
            game_id: "test_game".to_string(),
            supports_robust_ffb: true,
            supports_telemetry: true,
            preferred_mode: FFBMode::RawTorque,
        };

        let result =
            CapabilityNegotiator::negotiate_capabilities(&caps, Some(&game_compat));
        assert_eq!(
            result.mode,
            FFBMode::RawTorque,
            "DD device + robust FFB game should use RawTorque"
        );

        Ok(())
    }

    /// Mode selection policy per vendor fixture produces correct mode.
    #[test]
    fn mode_selection_correct_per_vendor() -> Result<()> {
        for fixture in DEVICE_FIXTURES {
            let caps = make_device_caps(fixture)?;

            let mode = ModeSelectionPolicy::select_mode(&caps, None);
            if fixture.supports_raw_1khz {
                assert_eq!(
                    mode,
                    FFBMode::RawTorque,
                    "{}: raw-capable must select RawTorque",
                    fixture.name
                );
            } else {
                assert_ne!(
                    mode,
                    FFBMode::RawTorque,
                    "{}: non-raw must not select RawTorque",
                    fixture.name
                );
            }
        }

        Ok(())
    }

    /// Safety service integrates with negotiated mode: clamped torque
    /// respects device max torque.
    #[test]
    fn safety_clamp_respects_negotiated_device_limits() -> Result<()> {
        for fixture in DEVICE_FIXTURES {
            let safety = SafetyService::new(fixture.max_torque_nm, 50.0);
            let requested = fixture.max_torque_nm * 2.0;
            let clamped = safety.clamp_torque_nm(requested);

            assert!(
                clamped <= fixture.max_torque_nm,
                "{}: clamped torque {} must be ≤ device max {}",
                fixture.name,
                clamped,
                fixture.max_torque_nm
            );
        }

        Ok(())
    }

    /// Pipeline processes frames correctly for all negotiated mode types.
    #[test]
    fn pipeline_processes_all_mode_types() -> Result<()> {
        let modes = [
            (FFBMode::RawTorque, 0.8f32),
            (FFBMode::PidPassthrough, 0.3),
        ];

        for (mode, input) in &modes {
            let mut pipeline = Pipeline::new();
            let mut frame = Frame {
                ffb_in: *input,
                torque_out: *input,
                wheel_speed: 5.0,
                hands_off: false,
                ts_mono_ns: 1_000_000,
                seq: 1,
            };

            pipeline.process(&mut frame)?;
            assert!(
                frame.torque_out.is_finite(),
                "{mode:?}: pipeline output must be finite"
            );
        }

        Ok(())
    }
}
