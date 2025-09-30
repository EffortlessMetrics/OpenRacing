//! Safety service with state machine for torque gate management

use anyhow::Result;
use racing_wheel_engine::{
    safety::{SafetyState, FaultType}, SafetyPolicy, SafetyViolation, TracingManager, AppTraceEvent
};
use racing_wheel_schemas::prelude::{DeviceId, TorqueNm};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc};
use tokio::time::interval;
use tracing::{info, warn, error, debug};

/// Safety interlock state for high-torque operations
#[derive(Debug, Clone, PartialEq)]
pub enum InterlockState {
    /// Safe torque mode (default)
    SafeTorque,
    /// Challenge issued, waiting for user response
    Challenge {
        challenge_token: u32,
        expires_at: Instant,
    },
    /// High torque unlocked and active
    HighTorqueActive {
        unlocked_at: Instant,
        device_token: u32,
    },
    /// Faulted state - torque disabled
    Faulted {
        fault_type: FaultType,
        occurred_at: Instant,
    },
}

/// Per-device safety context
#[derive(Debug, Clone)]
pub struct DeviceSafetyContext {
    pub device_id: DeviceId,
    pub interlock_state: InterlockState,
    pub max_safe_torque: TorqueNm,
    pub current_torque_limit: TorqueNm,
    pub hands_on_detected: bool,
    pub last_hands_on_time: Option<Instant>,
    pub temperature_c: Option<f32>,
    pub fault_count: u32,
    pub last_fault_time: Option<Instant>,
}

/// Safety event types
#[derive(Debug, Clone)]
pub enum SafetyEvent {
    /// High torque requested
    HighTorqueRequested {
        device_id: DeviceId,
        requested_by: String,
    },
    /// Challenge response received
    ChallengeResponse {
        device_id: DeviceId,
        token: u32,
        success: bool,
    },
    /// Fault detected
    FaultDetected {
        device_id: DeviceId,
        fault_type: FaultType,
        severity: FaultSeverity,
    },
    /// Fault cleared
    FaultCleared {
        device_id: DeviceId,
        fault_type: FaultType,
    },
    /// Emergency stop triggered
    EmergencyStop {
        device_id: DeviceId,
        reason: String,
    },
}

/// Fault severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultSeverity {
    /// Warning - continue operation with monitoring
    Warning,
    /// Critical - reduce torque but continue
    Critical,
    /// Fatal - immediate torque cutoff
    Fatal,
}

/// Application-level safety service
pub struct ApplicationSafetyService {
    /// Per-device safety contexts
    device_contexts: Arc<RwLock<HashMap<DeviceId, DeviceSafetyContext>>>,
    /// Safety policy engine
    safety_policy: SafetyPolicy,
    /// Event sender for safety events
    event_sender: mpsc::UnboundedSender<SafetyEvent>,
    /// Event receiver
    event_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<SafetyEvent>>>>,
    /// Tracing manager for observability
    tracer: Option<Arc<TracingManager>>,
    /// Safety monitoring interval
    monitoring_interval: Duration,
    /// Challenge timeout duration
    challenge_timeout: Duration,
    /// Hands-off timeout duration
    hands_off_timeout: Duration,
}

impl ApplicationSafetyService {
    /// Create new safety service
    pub async fn new(
        safety_policy: SafetyPolicy,
        tracer: Option<Arc<TracingManager>>,
    ) -> Result<Self> {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        Ok(Self {
            device_contexts: Arc::new(RwLock::new(HashMap::new())),
            safety_policy,
            event_sender,
            event_receiver: Arc::new(RwLock::new(Some(event_receiver))),
            tracer,
            monitoring_interval: Duration::from_millis(100), // 10Hz monitoring
            challenge_timeout: Duration::from_secs(30),
            hands_off_timeout: Duration::from_secs(5),
        })
    }

    /// Start the safety service
    pub async fn start(&self) -> Result<()> {
        info!("Starting safety service");

        // Start safety monitoring
        self.start_safety_monitoring().await?;

        // Start event processing
        self.start_event_processing().await?;

        info!("Safety service started successfully");
        Ok(())
    }

    /// Register a device with the safety service
    pub async fn register_device(
        &self,
        device_id: DeviceId,
        max_safe_torque: TorqueNm,
    ) -> Result<()> {
        info!(device_id = %device_id, max_safe_torque = %max_safe_torque, "Registering device with safety service");

        let context = DeviceSafetyContext {
            device_id: device_id.clone(),
            interlock_state: InterlockState::SafeTorque,
            max_safe_torque,
            current_torque_limit: max_safe_torque * 0.3, // Start with 30% of max
            hands_on_detected: false,
            last_hands_on_time: None,
            temperature_c: None,
            fault_count: 0,
            last_fault_time: None,
        };

        {
            let mut contexts = self.device_contexts.write().await;
            contexts.insert(device_id.clone(), context);
        }

        self.emit_safety_state_changed(&device_id, "unregistered", "safe_torque", "device_registered").await;

        info!(device_id = %device_id, "Device registered with safety service");
        Ok(())
    }

    /// Unregister a device from the safety service
    pub async fn unregister_device(&self, device_id: &DeviceId) -> Result<()> {
        info!(device_id = %device_id, "Unregistering device from safety service");

        {
            let mut contexts = self.device_contexts.write().await;
            contexts.remove(device_id);
        }

        info!(device_id = %device_id, "Device unregistered from safety service");
        Ok(())
    }

    /// Request high torque mode for a device
    pub async fn request_high_torque(
        &self,
        device_id: &DeviceId,
        requested_by: String,
    ) -> Result<InterlockState> {
        info!(device_id = %device_id, requested_by = %requested_by, "High torque mode requested");

        let mut contexts = self.device_contexts.write().await;
        let context = contexts.get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("Device not registered: {}", device_id))?;

        match context.interlock_state {
            InterlockState::SafeTorque => {
                // Check preconditions
                if let Err(violation) = self.check_high_torque_preconditions(context).await {
                    warn!(
                        device_id = %device_id,
                        violation = ?violation,
                        "High torque request denied due to safety violation"
                    );
                    return Err(anyhow::anyhow!("High torque request denied: {:?}", violation));
                }

                // Issue challenge
                let challenge_token = self.generate_challenge_token();
                let expires_at = Instant::now() + self.challenge_timeout;

                context.interlock_state = InterlockState::Challenge {
                    challenge_token,
                    expires_at,
                };

                self.emit_safety_state_changed(device_id, "safe_torque", "challenge", "high_torque_requested").await;

                // Send event
                let _ = self.event_sender.send(SafetyEvent::HighTorqueRequested {
                    device_id: device_id.clone(),
                    requested_by,
                });

                info!(
                    device_id = %device_id,
                    challenge_token = challenge_token,
                    "High torque challenge issued"
                );

                Ok(context.interlock_state.clone())
            }
            InterlockState::Challenge { .. } => {
                warn!(device_id = %device_id, "High torque challenge already active");
                Ok(context.interlock_state.clone())
            }
            InterlockState::HighTorqueActive { .. } => {
                info!(device_id = %device_id, "High torque already active");
                Ok(context.interlock_state.clone())
            }
            InterlockState::Faulted { .. } => {
                warn!(device_id = %device_id, "Cannot enable high torque while faulted");
                Err(anyhow::anyhow!("Device is in faulted state"))
            }
        }
    }

    /// Respond to high torque challenge
    pub async fn respond_to_challenge(
        &self,
        device_id: &DeviceId,
        response_token: u32,
    ) -> Result<InterlockState> {
        info!(device_id = %device_id, response_token = response_token, "Challenge response received");

        let mut contexts = self.device_contexts.write().await;
        let context = contexts.get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("Device not registered: {}", device_id))?;

        match &context.interlock_state {
            InterlockState::Challenge { challenge_token, expires_at } => {
                let success = *challenge_token == response_token && Instant::now() < *expires_at;

                // Send event
                let _ = self.event_sender.send(SafetyEvent::ChallengeResponse {
                    device_id: device_id.clone(),
                    token: response_token,
                    success,
                });

                if success {
                    // Activate high torque
                    let device_token = self.generate_device_token();
                    context.interlock_state = InterlockState::HighTorqueActive {
                        unlocked_at: Instant::now(),
                        device_token,
                    };
                    context.current_torque_limit = context.max_safe_torque;

                    self.emit_safety_state_changed(device_id, "challenge", "high_torque_active", "challenge_success").await;

                    info!(
                        device_id = %device_id,
                        device_token = device_token,
                        "High torque activated"
                    );
                } else {
                    // Challenge failed, return to safe torque
                    context.interlock_state = InterlockState::SafeTorque;
                    context.current_torque_limit = context.max_safe_torque * 0.3;

                    self.emit_safety_state_changed(device_id, "challenge", "safe_torque", "challenge_failed").await;

                    warn!(device_id = %device_id, "Challenge response failed");
                }

                Ok(context.interlock_state.clone())
            }
            _ => {
                warn!(device_id = %device_id, "No active challenge to respond to");
                Err(anyhow::anyhow!("No active challenge"))
            }
        }
    }

    /// Trigger emergency stop for a device
    pub async fn emergency_stop(&self, device_id: &DeviceId, reason: String) -> Result<()> {
        error!(device_id = %device_id, reason = %reason, "Emergency stop triggered");

        let mut contexts = self.device_contexts.write().await;
        let context = contexts.get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("Device not registered: {}", device_id))?;

        let old_state = format!("{:?}", context.interlock_state);
        context.interlock_state = InterlockState::Faulted {
            fault_type: FaultType::EmergencyStop,
            occurred_at: Instant::now(),
        };
        context.current_torque_limit = TorqueNm::from(0.0);
        context.fault_count += 1;
        context.last_fault_time = Some(Instant::now());

        self.emit_safety_state_changed(device_id, &old_state, "faulted", &reason).await;

        // Send event
        let _ = self.event_sender.send(SafetyEvent::EmergencyStop {
            device_id: device_id.clone(),
            reason,
        });

        error!(device_id = %device_id, "Emergency stop activated");
        Ok(())
    }

    /// Report fault for a device
    pub async fn report_fault(
        &self,
        device_id: &DeviceId,
        fault_type: FaultType,
        severity: FaultSeverity,
    ) -> Result<()> {
        warn!(
            device_id = %device_id,
            fault_type = ?fault_type,
            severity = ?severity,
            "Fault reported"
        );

        let mut contexts = self.device_contexts.write().await;
        let context = contexts.get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("Device not registered: {}", device_id))?;

        context.fault_count += 1;
        context.last_fault_time = Some(Instant::now());

        // Handle fault based on severity
        match severity {
            FaultSeverity::Warning => {
                // Continue operation but log the warning
                warn!(device_id = %device_id, fault_type = ?fault_type, "Warning fault detected");
            }
            FaultSeverity::Critical => {
                // Reduce torque but continue operation
                context.current_torque_limit = context.current_torque_limit * 0.5;
                warn!(
                    device_id = %device_id,
                    fault_type = ?fault_type,
                    new_limit = %context.current_torque_limit,
                    "Critical fault detected, torque reduced"
                );
            }
            FaultSeverity::Fatal => {
                // Immediate torque cutoff
                let old_state = format!("{:?}", context.interlock_state);
                context.interlock_state = InterlockState::Faulted {
                    fault_type,
                    occurred_at: Instant::now(),
                };
                context.current_torque_limit = TorqueNm::from(0.0);

                self.emit_safety_state_changed(device_id, &old_state, "faulted", &format!("fatal_fault_{:?}", fault_type)).await;

                error!(
                    device_id = %device_id,
                    fault_type = ?fault_type,
                    "Fatal fault detected, torque disabled"
                );
            }
        }

        // Send event
        let _ = self.event_sender.send(SafetyEvent::FaultDetected {
            device_id: device_id.clone(),
            fault_type,
            severity,
        });

        Ok(())
    }

    /// Clear fault for a device
    pub async fn clear_fault(&self, device_id: &DeviceId, fault_type: FaultType) -> Result<()> {
        info!(device_id = %device_id, fault_type = ?fault_type, "Clearing fault");

        let mut contexts = self.device_contexts.write().await;
        let context = contexts.get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("Device not registered: {}", device_id))?;

        // Only clear if currently faulted with the same fault type
        if let InterlockState::Faulted { fault_type: current_fault, .. } = &context.interlock_state {
            if *current_fault == fault_type {
                context.interlock_state = InterlockState::SafeTorque;
                context.current_torque_limit = context.max_safe_torque * 0.3;

                self.emit_safety_state_changed(device_id, "faulted", "safe_torque", "fault_cleared").await;

                // Send event
                let _ = self.event_sender.send(SafetyEvent::FaultCleared {
                    device_id: device_id.clone(),
                    fault_type,
                });

                info!(device_id = %device_id, fault_type = ?fault_type, "Fault cleared");
            } else {
                warn!(
                    device_id = %device_id,
                    requested_fault = ?fault_type,
                    current_fault = ?current_fault,
                    "Cannot clear fault - different fault type active"
                );
                return Err(anyhow::anyhow!("Different fault type active"));
            }
        } else {
            warn!(device_id = %device_id, "Device is not in faulted state");
            return Err(anyhow::anyhow!("Device is not faulted"));
        }

        Ok(())
    }

    /// Update hands-on detection for a device
    pub async fn update_hands_on_detection(&self, device_id: &DeviceId, hands_on: bool) -> Result<()> {
        let mut contexts = self.device_contexts.write().await;
        let context = contexts.get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("Device not registered: {}", device_id))?;

        let was_hands_on = context.hands_on_detected;
        context.hands_on_detected = hands_on;

        if hands_on {
            context.last_hands_on_time = Some(Instant::now());
        }

        // Log state changes
        if was_hands_on != hands_on {
            if hands_on {
                debug!(device_id = %device_id, "Hands-on detected");
            } else {
                debug!(device_id = %device_id, "Hands-off detected");
            }
        }

        Ok(())
    }

    /// Get current safety state for a device
    pub async fn get_safety_state(&self, device_id: &DeviceId) -> Result<DeviceSafetyContext> {
        let contexts = self.device_contexts.read().await;
        contexts.get(device_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Device not registered: {}", device_id))
    }

    /// Get current torque limit for a device
    pub async fn get_torque_limit(&self, device_id: &DeviceId) -> Result<TorqueNm> {
        let contexts = self.device_contexts.read().await;
        contexts.get(device_id)
            .map(|ctx| ctx.current_torque_limit)
            .ok_or_else(|| anyhow::anyhow!("Device not registered: {}", device_id))
    }

    /// Check preconditions for high torque mode
    async fn check_high_torque_preconditions(
        &self,
        context: &DeviceSafetyContext,
    ) -> Result<(), SafetyViolation> {
        // Check hands-on detection
        if !context.hands_on_detected {
            return Err(SafetyViolation::HandsOff);
        }

        // Check recent hands-on activity
        if let Some(last_hands_on) = context.last_hands_on_time {
            if last_hands_on.elapsed() > self.hands_off_timeout {
                return Err(SafetyViolation::HandsOff);
            }
        } else {
            return Err(SafetyViolation::HandsOff);
        }

        // Check temperature
        if let Some(temp) = context.temperature_c {
            if temp > 80.0 {
                return Err(SafetyViolation::OverTemperature);
            }
        }

        // Check fault history
        if context.fault_count > 5 {
            if let Some(last_fault) = context.last_fault_time {
                if last_fault.elapsed() < Duration::from_minutes(5) {
                    return Err(SafetyViolation::RecentFaults);
                }
            }
        }

        Ok(())
    }

    /// Generate challenge token
    fn generate_challenge_token(&self) -> u32 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        Instant::now().hash(&mut hasher);
        (hasher.finish() as u32) & 0x7FFFFFFF // Ensure positive
    }

    /// Generate device token
    fn generate_device_token(&self) -> u32 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        (Instant::now(), "device_token").hash(&mut hasher);
        (hasher.finish() as u32) & 0x7FFFFFFF // Ensure positive
    }

    /// Start safety monitoring
    async fn start_safety_monitoring(&self) -> Result<()> {
        let contexts = Arc::clone(&self.device_contexts);
        let _event_sender = self.event_sender.clone();
        let hands_off_timeout = self.hands_off_timeout;
        let _challenge_timeout = self.challenge_timeout;

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(100)); // 10Hz monitoring
            
            loop {
                interval.tick().await;
                
                let mut contexts_guard = contexts.write().await;
                let now = Instant::now();

                for (device_id, context) in contexts_guard.iter_mut() {
                    // Check for expired challenges
                    if let InterlockState::Challenge { expires_at, .. } = &context.interlock_state {
                        if now > *expires_at {
                            warn!(device_id = %device_id, "Challenge expired, returning to safe torque");
                            context.interlock_state = InterlockState::SafeTorque;
                            context.current_torque_limit = context.max_safe_torque * 0.3;
                        }
                    }

                    // Check for hands-off timeout in high torque mode
                    if let InterlockState::HighTorqueActive { .. } = &context.interlock_state {
                        if let Some(last_hands_on) = context.last_hands_on_time {
                            if !context.hands_on_detected || last_hands_on.elapsed() > hands_off_timeout {
                                warn!(device_id = %device_id, "Hands-off timeout in high torque mode");
                                context.interlock_state = InterlockState::SafeTorque;
                                context.current_torque_limit = context.max_safe_torque * 0.3;
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Start event processing
    async fn start_event_processing(&self) -> Result<()> {
        let mut receiver = {
            let mut receiver_guard = self.event_receiver.write().await;
            receiver_guard.take()
                .ok_or_else(|| anyhow::anyhow!("Event receiver already taken"))?
        };

        tokio::spawn(async move {
            while let Some(event) = receiver.recv().await {
                debug!(event = ?event, "Processing safety event");
                // Additional event processing logic would go here
            }
        });

        Ok(())
    }

    /// Emit safety state changed event
    async fn emit_safety_state_changed(
        &self,
        device_id: &DeviceId,
        old_state: &str,
        new_state: &str,
        reason: &str,
    ) {
        if let Some(tracer) = &self.tracer {
            tracer.emit_app_event(AppTraceEvent::SafetyStateChanged {
                device_id: device_id.to_string(),
                old_state: old_state.to_string(),
                new_state: new_state.to_string(),
                reason: reason.to_string(),
            });
        }
    }

    /// Get safety service statistics
    pub async fn get_statistics(&self) -> SafetyServiceStatistics {
        let contexts = self.device_contexts.read().await;
        
        let mut safe_torque_count = 0;
        let mut high_torque_count = 0;
        let mut faulted_count = 0;
        let mut challenge_count = 0;

        for context in contexts.values() {
            match context.interlock_state {
                InterlockState::SafeTorque => safe_torque_count += 1,
                InterlockState::Challenge { .. } => challenge_count += 1,
                InterlockState::HighTorqueActive { .. } => high_torque_count += 1,
                InterlockState::Faulted { .. } => faulted_count += 1,
            }
        }

        SafetyServiceStatistics {
            total_devices: contexts.len(),
            safe_torque_devices: safe_torque_count,
            high_torque_devices: high_torque_count,
            faulted_devices: faulted_count,
            challenge_devices: challenge_count,
        }
    }
}

/// Safety service statistics
#[derive(Debug, Clone)]
pub struct SafetyServiceStatistics {
    pub total_devices: usize,
    pub safe_torque_devices: usize,
    pub high_torque_devices: usize,
    pub faulted_devices: usize,
    pub challenge_devices: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use racing_wheel_schemas::TorqueNm;

    #[tokio::test]
    async fn test_safety_service_creation() {
        let safety_policy = SafetyPolicy::default();
        let service = ApplicationSafetyService::new(safety_policy, None).await;
        assert!(service.is_ok());
    }

    #[tokio::test]
    async fn test_device_registration() {
        let safety_policy = SafetyPolicy::default();
        let service = ApplicationSafetyService::new(safety_policy, None).await.unwrap();

        let device_id = DeviceId::from("test-device");
        let max_torque = TorqueNm::from(10.0);

        // Test registration
        let result = service.register_device(device_id.clone(), max_torque).await;
        assert!(result.is_ok());

        // Test getting safety state
        let state = service.get_safety_state(&device_id).await.unwrap();
        assert_eq!(state.interlock_state, InterlockState::SafeTorque);
        assert_eq!(state.max_safe_torque, max_torque);

        // Test unregistration
        let result = service.unregister_device(&device_id).await;
        assert!(result.is_ok());

        // Should fail to get state after unregistration
        let result = service.get_safety_state(&device_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_high_torque_workflow() {
        let safety_policy = SafetyPolicy::default();
        let service = ApplicationSafetyService::new(safety_policy, None).await.unwrap();

        let device_id = DeviceId::from("test-device");
        let max_torque = TorqueNm::from(10.0);

        // Register device
        service.register_device(device_id.clone(), max_torque).await.unwrap();

        // Enable hands-on detection
        service.update_hands_on_detection(&device_id, true).await.unwrap();

        // Request high torque
        let state = service.request_high_torque(&device_id, "test_user".to_string()).await.unwrap();
        
        if let InterlockState::Challenge { challenge_token, .. } = state {
            // Respond to challenge
            let result = service.respond_to_challenge(&device_id, challenge_token).await.unwrap();
            assert!(matches!(result, InterlockState::HighTorqueActive { .. }));

            // Check torque limit
            let torque_limit = service.get_torque_limit(&device_id).await.unwrap();
            assert_eq!(torque_limit, max_torque);
        } else {
            panic!("Expected challenge state");
        }
    }

    #[tokio::test]
    async fn test_fault_handling() {
        let safety_policy = SafetyPolicy::default();
        let service = ApplicationSafetyService::new(safety_policy, None).await.unwrap();

        let device_id = DeviceId::from("test-device");
        let max_torque = TorqueNm::from(10.0);

        // Register device
        service.register_device(device_id.clone(), max_torque).await.unwrap();

        // Report a fatal fault
        service.report_fault(&device_id, FaultType::OverTemperature, FaultSeverity::Fatal).await.unwrap();

        // Check that device is now faulted
        let state = service.get_safety_state(&device_id).await.unwrap();
        assert!(matches!(state.interlock_state, InterlockState::Faulted { .. }));

        // Check that torque is disabled
        let torque_limit = service.get_torque_limit(&device_id).await.unwrap();
        assert_eq!(torque_limit, TorqueNm::from(0.0));

        // Clear the fault
        service.clear_fault(&device_id, FaultType::OverTemperature).await.unwrap();

        // Check that device is back to safe torque
        let state = service.get_safety_state(&device_id).await.unwrap();
        assert_eq!(state.interlock_state, InterlockState::SafeTorque);
    }

    #[tokio::test]
    async fn test_emergency_stop() {
        let safety_policy = SafetyPolicy::default();
        let service = ApplicationSafetyService::new(safety_policy, None).await.unwrap();

        let device_id = DeviceId::from("test-device");
        let max_torque = TorqueNm::from(10.0);

        // Register device
        service.register_device(device_id.clone(), max_torque).await.unwrap();

        // Trigger emergency stop
        service.emergency_stop(&device_id, "User requested".to_string()).await.unwrap();

        // Check that device is faulted
        let state = service.get_safety_state(&device_id).await.unwrap();
        assert!(matches!(state.interlock_state, InterlockState::Faulted { .. }));

        // Check that torque is disabled
        let torque_limit = service.get_torque_limit(&device_id).await.unwrap();
        assert_eq!(torque_limit, TorqueNm::from(0.0));
    }

    #[tokio::test]
    async fn test_safety_statistics() {
        let safety_policy = SafetyPolicy::default();
        let service = ApplicationSafetyService::new(safety_policy, None).await.unwrap();

        // Initially no devices
        let stats = service.get_statistics().await;
        assert_eq!(stats.total_devices, 0);

        // Register a device
        let device_id = DeviceId::from("test-device");
        service.register_device(device_id, TorqueNm::from(10.0)).await.unwrap();

        let stats = service.get_statistics().await;
        assert_eq!(stats.total_devices, 1);
        assert_eq!(stats.safe_torque_devices, 1);
        assert_eq!(stats.high_torque_devices, 0);
        assert_eq!(stats.faulted_devices, 0);
    }
}