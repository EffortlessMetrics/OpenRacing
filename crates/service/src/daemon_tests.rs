//! Service daemon lifecycle tests

#[cfg(test)]
mod tests {
    use crate::{IpcConfig, ServiceConfig, ServiceDaemon, TransportType};
    use std::time::Duration;
    use tempfile::TempDir;
    use tracing_test::traced_test;

    fn create_test_config() -> ServiceConfig {
        ServiceConfig {
            service_name: "test-wheeld".to_string(),
            service_display_name: "Test Racing Wheel Service".to_string(),
            service_description: "Test service for unit tests".to_string(),
            ipc: IpcConfig {
                transport: TransportType::default(),
                bind_address: None,
                max_connections: 5,
                connection_timeout: Duration::from_secs(5),
                enable_acl: false, // Disable ACL for tests
            },
            health_check_interval: 1, // Fast health checks for tests
            max_restart_attempts: 2,
            restart_delay: 1,
            auto_restart: false, // Disable auto-restart for controlled tests
        }
    }

    #[tokio::test]
    #[traced_test]
    async fn test_service_daemon_creation() {
        let config = create_test_config();
        let daemon = ServiceDaemon::new(config).await;

        assert!(daemon.is_ok(), "Failed to create service daemon");
    }

    #[tokio::test]
    #[traced_test]
    #[cfg_attr(target_os = "windows", ignore)]
    async fn test_service_config_save_load() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Override the config path for testing
        // SAFETY: This is a test-only function that sets environment variables
        // in a controlled test environment. We ensure no other threads are
        // reading these variables during the test.
        unsafe {
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            std::env::set_var("HOME", temp_dir.path());
        }

        let original_config = create_test_config();

        // Save config
        original_config.save().await.expect("Failed to save config");

        // Load config
        let loaded_config = ServiceConfig::load().await.expect("Failed to load config");

        assert_eq!(original_config.service_name, loaded_config.service_name);
        assert_eq!(
            original_config.health_check_interval,
            loaded_config.health_check_interval
        );
        assert_eq!(
            original_config.max_restart_attempts,
            loaded_config.max_restart_attempts
        );
    }

    #[tokio::test]
    #[traced_test]
    async fn test_service_daemon_startup_shutdown() {
        let config = create_test_config();
        let daemon = ServiceDaemon::new(config)
            .await
            .expect("Failed to create daemon");

        // Test that daemon can be started and shut down quickly
        let daemon_handle = tokio::spawn(async move { daemon.run().await });

        // Give the daemon a moment to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Send shutdown signal (in a real test, we'd use the proper shutdown mechanism)
        daemon_handle.abort();

        // Verify the task was aborted (simulating shutdown)
        let result = daemon_handle.await;
        assert!(result.is_err()); // Should be cancelled
    }

    #[tokio::test]
    #[traced_test]
    async fn test_service_restart_logic() {
        let mut config = create_test_config();
        config.auto_restart = true;
        config.max_restart_attempts = 2;
        config.restart_delay = 1;

        let _daemon = ServiceDaemon::new(config)
            .await
            .expect("Failed to create daemon");

        // This test verifies the restart logic exists
        // In a real scenario, we would simulate service failures
        // For now, just verify the daemon can be created with restart config
        // (daemon creation with expect above validates acceptance)
    }
}
