//! Comprehensive system integration tests with virtual hardware simulation
//!
//! Tests the complete system integration including all components working
//! together with virtual devices and mock game telemetry.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FeatureFlags, ServiceDaemon, SystemConfig, WheelService};
    use racing_wheel_engine::{MockTelemetrySource, VirtualHidPort};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;
    use tracing_test::traced_test;

    /// Integration test configuration
    struct IntegrationTestConfig {
        /// Enable virtual devices
        enable_virtual_devices: bool,
        /// Enable mock telemetry
        enable_mock_telemetry: bool,
        /// Disable real-time scheduling
        disable_realtime: bool,
        /// Test duration
        test_duration: Duration,
    }

    impl Default for IntegrationTestConfig {
        fn default() -> Self {
            Self {
                enable_virtual_devices: true,
                enable_mock_telemetry: true,
                disable_realtime: true,
                test_duration: Duration::from_secs(10),
            }
        }
    }

    /// Test complete system startup and shutdown
    #[tokio::test]
    #[traced_test]
    async fn test_complete_system_startup_shutdown() {
        let _config = create_test_system_config().await;
        let flags = create_test_feature_flags();

        // Create service daemon
        let mut service_config = crate::ServiceConfig::default();
        service_config.ipc = crate::IpcConfig::default();

        let daemon = ServiceDaemon::new_with_flags(service_config, flags)
            .await
            .expect("Failed to create service daemon");

        // Start daemon in background
        let daemon_handle = tokio::spawn(async move { daemon.run().await });

        // Let it run for a short time
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Shutdown should be handled by the daemon's signal handling
        // For testing, we'll just verify it started successfully
        assert!(!daemon_handle.is_finished());

        // Cancel the daemon task
        daemon_handle.abort();
    }

    /// Test device enumeration and management
    #[tokio::test]
    #[traced_test]
    async fn test_device_enumeration_and_management() {
        let service = create_test_service().await;

        // Test device enumeration
        let devices = service
            .device_service()
            .enumerate_devices()
            .await
            .expect("Failed to enumerate devices");

        // Should have at least one virtual device
        assert!(!devices.is_empty(), "No devices found");

        // Test device connection
        // TODO: Re-enable when connect_device is implemented
        // if let Some(device) = devices.first() {
        //     let connection_result = service.device_service()
        //         .connect_device(&device.id).await;
        //     assert!(connection_result.is_ok(), "Failed to connect to device");
        //
        //     // Test device status
        //     let status = service.device_service()
        //         .get_device_status(&device.id).await;
        //     assert!(status.is_ok(), "Failed to get device status");
        // }
    }

    /// Test profile management and application
    #[tokio::test]
    #[traced_test]
    async fn test_profile_management() {
        let service = create_test_service().await;

        // Create test profile
        let test_profile = create_test_profile();

        // Save profile
        let save_result = service.profile_service().save_profile(&test_profile).await;
        assert!(save_result.is_ok(), "Failed to save profile");

        // Load profile
        let loaded_profile = service
            .profile_service()
            .load_profile(&test_profile.scope)
            .await;
        assert!(loaded_profile.is_ok(), "Failed to load profile");

        // Apply profile
        let apply_result = service.profile_service().apply_profile(&test_profile).await;
        assert!(apply_result.is_ok(), "Failed to apply profile");

        // Verify profile is active
        let active_profiles = service.profile_service().get_active_profiles().await;
        assert!(active_profiles.is_ok(), "Failed to get active profiles");
    }

    /// Test safety system functionality
    #[tokio::test]
    #[traced_test]
    async fn test_safety_system() {
        let service = create_test_service().await;

        // Test initial safety state (should be safe torque)
        let safety_state = service.safety_service().get_safety_state().await;
        assert!(matches!(
            safety_state.state,
            racing_wheel_engine::SafetyState::SafeTorque
        ));

        // Test torque limit enforcement
        let devices = service
            .device_service()
            .enumerate_devices()
            .await
            .expect("Failed to enumerate devices");

        if let Some(device) = devices.first() {
            // Try to set high torque without unlock (should fail)
            let high_torque_result = service
                .safety_service()
                .request_high_torque(&device.id)
                .await;
            assert!(
                high_torque_result.is_err(),
                "High torque should be denied without unlock"
            );

            // Test fault injection and recovery
            let fault_result = service
                .safety_service()
                .inject_test_fault(&device.id, racing_wheel_engine::FaultType::ThermalLimit)
                .await;
            assert!(fault_result.is_ok(), "Failed to inject test fault");

            // Verify fault state
            let safety_state = service.safety_service().get_safety_state().await;
            assert!(matches!(
                safety_state.state,
                racing_wheel_engine::SafetyState::Faulted { .. }
            ));

            // Test fault recovery
            let recovery_result = service.safety_service().clear_fault(&device.id).await;
            assert!(recovery_result.is_ok(), "Failed to clear fault");
        }
    }

    /// Test game integration and telemetry
    /// TODO: Re-enable when game_service is implemented
    #[tokio::test]
    #[traced_test]
    #[ignore]
    async fn test_game_integration() {
        let service = create_test_service().await;

        // Test game detection
        // let games = service.game_service().detect_games().await
        //     .expect("Failed to detect games");

        // Should detect mock games in test environment
        // assert!(!games.is_empty(), "No games detected");

        // Test telemetry configuration
        // if let Some(game) = games.first() {
        //     let config_result = service.game_service()
        //         .configure_telemetry(&game.id).await;
        //     assert!(config_result.is_ok(), "Failed to configure telemetry");
        //
        //     // Test telemetry reception
        //     let telemetry_stream = service.game_service()
        //         .start_telemetry_monitoring(&game.id).await;
        //     assert!(telemetry_stream.is_ok(), "Failed to start telemetry monitoring");
        //
        //     // Wait for telemetry data
        //     if let Ok(mut stream) = telemetry_stream {
        //         let telemetry_result = timeout(Duration::from_secs(5), stream.recv()).await;
        //         assert!(telemetry_result.is_ok(), "No telemetry data received");
        //     }
        // }

        // Placeholder assertion
        assert!(true, "Test disabled - game_service not yet implemented");
    }

    /// Test force feedback pipeline
    /// TODO: Re-enable when send_ffb_frame and get_device_statistics are implemented
    #[tokio::test]
    #[traced_test]
    #[ignore]
    async fn test_force_feedback_pipeline() {
        let service = create_test_service().await;

        // Get devices
        let devices = service
            .device_service()
            .enumerate_devices()
            .await
            .expect("Failed to enumerate devices");

        if let Some(_device) = devices.first() {
            // Connect device
            // service.device_service().connect_device(&device.id).await
            //     .expect("Failed to connect device");
            //
            // // Create test FFB data
            // let test_ffb_data = racing_wheel_engine::Frame {
            //     ffb_in: 0.5,
            //     torque_out: 0.0,
            //     wheel_speed: 0.0,
            //     hands_off: false,
            //     ts_mono_ns: 0,
            //     seq: 0,
            // };
            //
            // // Send FFB data through pipeline
            // let pipeline_result = service.device_service()
            //     .send_ffb_frame(&device.id, test_ffb_data).await;
            // assert!(pipeline_result.is_ok(), "Failed to send FFB frame");
            //
            // // Verify processing
            // let device_stats = service.device_service().get_device_statistics(&device.id).await;
            // assert!(device_stats.is_ok(), "Failed to get device statistics");
            //
            // if let Ok(stats) = device_stats {
            //     assert!(stats.frames_processed > 0, "No frames processed");
            // }
        }

        // Placeholder assertion
        assert!(
            true,
            "Test disabled - FFB pipeline methods not yet implemented"
        );
    }

    /// Test IPC communication
    #[tokio::test]
    #[traced_test]
    async fn test_ipc_communication() {
        let config = create_test_system_config().await;
        let mut service_config = crate::ServiceConfig::default();
        service_config.ipc = crate::IpcConfig::default();

        // Create IPC server
        let ipc_server = crate::IpcServer::new(service_config.ipc.clone())
            .await
            .expect("Failed to create IPC server");

        // Create service
        let service = create_test_service().await;

        // Start IPC server in background
        let server_handle = tokio::spawn(async move { ipc_server.serve(Arc::new(service)).await });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Create IPC client
        let client = crate::IpcClient::new(service_config.ipc.clone());

        // Test device listing
        let devices_result = client.list_devices().await;
        assert!(devices_result.is_ok(), "Failed to list devices via IPC");

        // Test profile operations
        let test_profile = create_test_profile();
        let save_result = client.save_profile(&test_profile).await;
        assert!(save_result.is_ok(), "Failed to save profile via IPC");

        // Cleanup
        server_handle.abort();
    }

    /// Test plugin system
    /// TODO: Re-enable when plugin_service is implemented
    #[tokio::test]
    #[traced_test]
    #[ignore]
    async fn test_plugin_system() {
        let _service = create_test_service().await;

        // Test plugin enumeration
        // let plugins = service.plugin_service().enumerate_plugins().await
        //     .expect("Failed to enumerate plugins");

        // Should have test plugins available
        // assert!(!plugins.is_empty(), "No plugins found");

        // Test plugin loading
        // if let Some(plugin) = plugins.first() {
        //     let load_result = service.plugin_service()
        //         .load_plugin(&plugin.id).await;
        //     assert!(load_result.is_ok(), "Failed to load plugin");
        //
        //     // Test plugin execution
        //     let test_telemetry = racing_wheel_engine::NormalizedTelemetry {
        //         timestamp: 0,
        //         ffb_scalar: 0.5,
        //         rpm: 5000.0,
        //         speed_ms: 50.0,
        //         slip_ratio: 0.1,
        //         gear: 3,
        //         flags: racing_wheel_engine::TelemetryFlags { ..Default::default() },
        //         car_id: Some("test_car".to_string()),
        //         track_id: Some("test_track".to_string()),
        //     };
        //
        //     let execution_result = service.plugin_service()
        //         .execute_plugin(&plugin.id, &test_telemetry).await;
        //     assert!(execution_result.is_ok(), "Failed to execute plugin");
        // }

        // Placeholder assertion
        assert!(true, "Test disabled - plugin_service not yet implemented");
    }

    /// Test performance under load
    /// TODO: Re-enable when FFB pipeline methods are implemented
    #[tokio::test]
    #[traced_test]
    #[ignore]
    async fn test_performance_under_load() {
        let service = create_test_service().await;

        // Get devices
        let devices = service
            .device_service()
            .enumerate_devices()
            .await
            .expect("Failed to enumerate devices");

        if let Some(_device) = devices.first() {
            // Connect device
            // service.device_service().connect_device(&device.id).await
            //     .expect("Failed to connect device");
            //
            // // Send high-frequency FFB data
            // let start_time = std::time::Instant::now();
            // let target_frames = 1000; // 1 second at 1kHz
            //
            // for i in 0..target_frames {
            //     let test_frame = racing_wheel_engine::Frame {
            //         ffb_in: (i as f32 / target_frames as f32).sin(),
            //         torque_out: 0.0,
            //         wheel_speed: 0.0,
            //         hands_off: false,
            //         ts_mono_ns: i * 1_000_000, // 1ms intervals
            //         seq: i as u16,
            //     };
            //
            //     let result = service.device_service()
            //         .send_ffb_frame(&device.id, test_frame).await;
            //     assert!(result.is_ok(), "Failed to send FFB frame {}", i);
            // }
            //
            // let elapsed = start_time.elapsed();
            // let fps = target_frames as f64 / elapsed.as_secs_f64();
            //
            // // Should maintain reasonable throughput
            // assert!(fps > 500.0, "Throughput too low: {} fps", fps);
            //
            // // Check for missed frames or errors
            // let stats = service.device_service().get_device_statistics(&device.id).await
            //     .expect("Failed to get device statistics");
            //
            // assert_eq!(stats.frames_processed, target_frames, "Frame count mismatch");
            // assert_eq!(stats.frames_dropped, 0, "Frames were dropped");
        }

        // Placeholder assertion
        assert!(
            true,
            "Test disabled - FFB pipeline methods not yet implemented"
        );
    }

    /// Test graceful degradation
    #[tokio::test]
    #[traced_test]
    async fn test_graceful_degradation() {
        let service = create_test_service().await;

        // Test with no devices connected
        let no_device_stats = service.device_service().get_statistics().await;
        assert_eq!(no_device_stats.connected_devices, 0);

        // Service should still be functional
        let profile_stats = service
            .profile_service()
            .get_profile_statistics()
            .await
            .expect("Profile service should work without devices");
        assert!(profile_stats.total_profiles >= 0);

        // Test with telemetry unavailable
        // TODO: Re-enable when game_service is implemented
        // let games = service.game_service().detect_games().await
        //     .expect("Game service should work without active games");
        //
        // // Should handle missing games gracefully
        // for game in games {
        //     let telemetry_result = service.game_service()
        //         .start_telemetry_monitoring(&game.id).await;
        //
        //     // May fail, but should not crash
        //     if telemetry_result.is_err() {
        //         println!("Telemetry unavailable for {}, continuing", game.name);
        //     }
        // }
    }

    /// Test configuration validation and migration
    #[tokio::test]
    #[traced_test]
    async fn test_configuration_validation() {
        // Test valid configuration
        let valid_config = create_test_system_config().await;
        assert!(
            valid_config.validate().is_ok(),
            "Valid config should pass validation"
        );

        // Test invalid configuration
        let mut invalid_config = valid_config.clone();
        invalid_config.engine.tick_rate_hz = 0; // Invalid tick rate
        assert!(
            invalid_config.validate().is_err(),
            "Invalid config should fail validation"
        );

        // Test configuration migration
        let mut old_config = valid_config.clone();
        old_config.schema_version = "wheel.config/0".to_string();

        let migration_result = old_config.migrate();
        assert!(migration_result.is_ok(), "Migration should succeed");

        if let Ok(migrated) = migration_result {
            assert!(migrated, "Migration should have been performed");
            assert_eq!(old_config.schema_version, "wheel.config/1");
        }
    }

    /// Test anti-cheat compatibility
    #[tokio::test]
    #[traced_test]
    async fn test_anticheat_compatibility() {
        // Generate anti-cheat report
        let report = crate::AntiCheatReport::generate()
            .await
            .expect("Failed to generate anti-cheat report");

        // Verify key compatibility points
        assert!(
            !report.process_info.dll_injection,
            "Should not use DLL injection"
        );
        assert!(
            report.process_info.kernel_drivers.is_empty(),
            "Should not use kernel drivers"
        );

        // Verify all telemetry methods are documented
        assert!(
            !report.telemetry_methods.is_empty(),
            "Should document telemetry methods"
        );

        for method in &report.telemetry_methods {
            assert!(
                method.anticheat_compatible,
                "All telemetry methods should be anti-cheat compatible"
            );
            assert!(
                !method.compatibility_notes.is_empty(),
                "Should have compatibility notes"
            );
        }

        // Verify security measures
        assert!(
            !report.security_measures.is_empty(),
            "Should document security measures"
        );

        // Generate markdown report
        let markdown = report.to_markdown();
        assert!(
            markdown.contains("Anti-Cheat Compatibility Report"),
            "Should contain report title"
        );
        assert!(
            markdown.contains("No DLL Injection"),
            "Should document no DLL injection"
        );
        assert!(
            markdown.contains("No Kernel Drivers"),
            "Should document no kernel drivers"
        );
    }

    // Helper functions

    async fn create_test_service() -> WheelService {
        WheelService::new_with_flags(create_test_feature_flags())
            .await
            .expect("Failed to create test service")
    }

    async fn create_test_system_config() -> SystemConfig {
        let mut config = SystemConfig::default();

        // Configure for testing
        config.engine.disable_realtime = true;
        config.development.enable_dev_features = true;
        config.development.enable_virtual_devices = true;
        config.development.mock_telemetry = true;
        config.development.disable_safety_interlocks = true;

        // Use test-specific paths
        config.ipc.transport = crate::system_config::TransportType::Native;
        config.ipc.max_connections = 5;

        config
    }

    fn create_test_feature_flags() -> FeatureFlags {
        FeatureFlags {
            disable_realtime: true,
            force_ffb_mode: Some("raw".to_string()),
            enable_dev_features: true,
            enable_debug_logging: true,
            enable_virtual_devices: true,
            disable_safety_interlocks: true,
            enable_plugin_dev_mode: true,
        }
    }

    fn create_test_profile() -> racing_wheel_engine::Profile {
        racing_wheel_engine::Profile {
            schema_version: "wheel.profile/1".to_string(),
            scope: racing_wheel_engine::ProfileScope {
                game: Some("test_game".to_string()),
                car: Some("test_car".to_string()),
                track: None,
                session: None,
            },
            base_settings: racing_wheel_engine::BaseSettings {
                ffb_gain: 0.8,
                dor_degrees: racing_wheel_engine::Degrees::new(540.0),
                torque_cap: racing_wheel_engine::TorqueNm::new(10.0),
                filters: racing_wheel_engine::FilterConfig::default(),
            },
            led_config: racing_wheel_engine::LedConfig::default(),
            haptics_config: racing_wheel_engine::HapticsConfig::default(),
            signature: None,
        }
    }
}
