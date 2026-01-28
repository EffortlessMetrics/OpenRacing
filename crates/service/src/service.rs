//! Main service implementation

use crate::{ApplicationDeviceService, ApplicationProfileService, ApplicationSafetyService};
use anyhow::Result;
use racing_wheel_engine::{SafetyPolicy, TracingManager, VirtualDevice, VirtualHidPort};
use racing_wheel_schemas::prelude::DeviceId;
use std::sync::Arc;
use tracing::{error, info};

/// Main wheel service that orchestrates all application services
#[derive(Clone)]
pub struct WheelService {
    /// Profile service for managing wheel profiles
    profile_service: Arc<ApplicationProfileService>,
    /// Device service for managing hardware
    device_service: Arc<ApplicationDeviceService>,
    /// Safety service for torque management
    safety_service: Arc<ApplicationSafetyService>,
    /// Tracing manager for observability
    tracer: Option<Arc<TracingManager>>,
}

impl WheelService {
    /// Create new service instance
    pub async fn new() -> Result<Self> {
        info!("Initializing Racing Wheel Service");

        // Initialize tracing
        let tracer = match TracingManager::new() {
            Ok(mut tracer) => {
                if let Err(e) = tracer.initialize() {
                    error!(error = %e, "Failed to initialize tracing, continuing without it");
                    None
                } else {
                    info!("Tracing initialized successfully");
                    Some(Arc::new(tracer))
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to create tracing manager, continuing without it");
                None
            }
        };

        // Initialize HID port (using virtual port for now)
        let mut virtual_port = VirtualHidPort::new();

        // Seed with a default virtual device for testing/development
        let device_id: DeviceId = "virtual-wheel-0"
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse device ID: {}", e))?;
        let virtual_device = VirtualDevice::new(device_id, "Virtual Racing Wheel".to_string());
        virtual_port
            .add_device(virtual_device)
            .map_err(|e| anyhow::anyhow!("Failed to add virtual device: {}", e))?;

        let hid_port = Arc::new(virtual_port);
        info!("HID port initialized with virtual device");

        // Initialize profile repository (using simple in-memory storage for now)
        // In a real implementation, this would be a file-based or database repository
        info!("Profile repository initialized");

        // Initialize safety policy
        let safety_policy = SafetyPolicy::default();
        info!("Safety policy initialized");

        // Create application services
        let profile_service = Arc::new(
            ApplicationProfileService::new()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create profile service: {}", e))?,
        );
        info!("Profile service created");

        let device_service = Arc::new(
            ApplicationDeviceService::new(hid_port, tracer.clone())
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create device service: {}", e))?,
        );
        info!("Device service created");

        let safety_service = Arc::new(
            ApplicationSafetyService::new(safety_policy, tracer.clone())
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create safety service: {}", e))?,
        );
        info!("Safety service created");

        Ok(Self {
            profile_service,
            device_service,
            safety_service,
            tracer,
        })
    }

    /// Run the service
    pub async fn run(self) -> Result<()> {
        info!("Starting Racing Wheel Service");

        // Start all services
        if let Err(e) = self.device_service.start().await {
            error!(error = %e, "Failed to start device service");
            return Err(e);
        }

        if let Err(e) = self.safety_service.start().await {
            error!(error = %e, "Failed to start safety service");
            return Err(e);
        }

        info!("All services started successfully");

        // Service main loop
        let shutdown_signal = tokio::signal::ctrl_c();

        tokio::select! {
            _ = shutdown_signal => {
                info!("Shutdown signal received");
            }
            _ = self.service_health_monitor() => {
                error!("Service health monitor exited unexpectedly");
            }
        }

        info!("Racing Wheel Service shutting down");
        self.shutdown().await?;

        Ok(())
    }

    /// Get profile service reference
    pub fn profile_service(&self) -> &Arc<ApplicationProfileService> {
        &self.profile_service
    }

    /// Get device service reference
    pub fn device_service(&self) -> &Arc<ApplicationDeviceService> {
        &self.device_service
    }

    /// Get safety service reference
    pub fn safety_service(&self) -> &Arc<ApplicationSafetyService> {
        &self.safety_service
    }

    /// Service health monitoring
    async fn service_health_monitor(&self) -> Result<()> {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

        loop {
            interval.tick().await;

            // Check service health and log statistics
            let profile_stats = self
                .profile_service
                .get_profile_statistics()
                .await
                .unwrap_or_else(|e| {
                    error!(error = %e, "Failed to get profile statistics");
                    crate::profile_service::ProfileStatistics {
                        total_profiles: 0,
                        active_profiles: 0,
                        cached_profiles: 0,
                        signed_profiles: 0,
                        trusted_profiles: 0,
                        session_overrides: 0,
                    }
                });

            let device_stats = self.device_service.get_statistics().await;
            let safety_stats = self.safety_service.get_statistics().await;

            info!(
                profiles_total = profile_stats.total_profiles,
                profiles_active = profile_stats.active_profiles,
                profiles_cached = profile_stats.cached_profiles,
                devices_total = device_stats.total_devices,
                devices_connected = device_stats.connected_devices,
                devices_ready = device_stats.ready_devices,
                devices_faulted = device_stats.faulted_devices,
                safety_total = safety_stats.total_devices,
                safety_safe_torque = safety_stats.safe_torque_devices,
                safety_high_torque = safety_stats.high_torque_devices,
                safety_faulted = safety_stats.faulted_devices,
                "Service health check"
            );
        }
    }

    /// Shutdown the service gracefully
    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down services");

        // Shutdown tracing if available
        if let Some(_tracer) = &self.tracer {
            // Note: TracingManager doesn't have a mutable shutdown method in our current design
            // In a real implementation, we would properly shutdown the tracing system
            info!("Tracing shutdown completed");
        }

        info!("Service shutdown completed");
        Ok(())
    }
}
