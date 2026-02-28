//! Recovery procedures for fault conditions.
//!
//! Provides strategies for recovering from various fault types,
//! including automatic recovery, user-initiated recovery, and
//! escalation procedures.

use crate::FaultType;
use core::time::Duration;

/// Recovery procedure status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStatus {
    /// Recovery not yet started.
    Pending,
    /// Recovery in progress.
    InProgress,
    /// Recovery completed successfully.
    Completed,
    /// Recovery failed.
    Failed,
    /// Recovery was cancelled.
    Cancelled,
    /// Recovery timed out.
    Timeout,
}

/// Result of a recovery attempt.
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    /// Status of the recovery.
    pub status: RecoveryStatus,
    /// Time taken for recovery.
    pub duration: Duration,
    /// Number of attempts made.
    pub attempts: u32,
    /// Error message if failed.
    pub error: Option<heapless::String<128>>,
}

impl RecoveryResult {
    /// Create a successful recovery result.
    pub fn success(duration: Duration, attempts: u32) -> Self {
        Self {
            status: RecoveryStatus::Completed,
            duration,
            attempts,
            error: None,
        }
    }

    /// Create a failed recovery result.
    pub fn failed(duration: Duration, attempts: u32, error: &str) -> Self {
        let mut err = heapless::String::new();
        let _ = err.push_str(error);
        Self {
            status: RecoveryStatus::Failed,
            duration,
            attempts,
            error: Some(err),
        }
    }

    /// Create a timeout recovery result.
    pub fn timeout(duration: Duration, attempts: u32) -> Self {
        Self {
            status: RecoveryStatus::Timeout,
            duration,
            attempts,
            error: None,
        }
    }

    /// Check if recovery was successful.
    pub fn is_success(&self) -> bool {
        self.status == RecoveryStatus::Completed
    }
}

/// Recovery procedure definition.
#[derive(Debug, Clone)]
pub struct RecoveryProcedure {
    /// Fault type this procedure applies to.
    pub fault_type: FaultType,
    /// Maximum number of retry attempts.
    pub max_attempts: u32,
    /// Delay between retry attempts.
    pub retry_delay: Duration,
    /// Total timeout for recovery.
    pub timeout: Duration,
    /// Whether automatic recovery is allowed.
    pub automatic: bool,
    /// Steps in the recovery procedure.
    pub steps: heapless::Vec<RecoveryStep, 8>,
}

/// A single step in a recovery procedure.
#[derive(Debug, Clone)]
pub struct RecoveryStep {
    /// Step name/identifier.
    pub name: heapless::String<32>,
    /// Step description.
    pub description: heapless::String<128>,
    /// Maximum time for this step.
    pub timeout: Duration,
    /// Whether this step can be skipped on failure.
    pub optional: bool,
}

impl RecoveryProcedure {
    /// Create a new recovery procedure for a fault type.
    pub fn new(fault_type: FaultType) -> Self {
        Self {
            fault_type,
            max_attempts: 3,
            retry_delay: Duration::from_millis(100),
            timeout: Duration::from_secs(5),
            automatic: true,
            steps: heapless::Vec::new(),
        }
    }

    /// Create a recovery procedure with custom settings.
    pub fn with_settings(
        fault_type: FaultType,
        max_attempts: u32,
        retry_delay: Duration,
        timeout: Duration,
    ) -> Self {
        Self {
            fault_type,
            max_attempts,
            retry_delay,
            timeout,
            automatic: true,
            steps: heapless::Vec::new(),
        }
    }

    /// Set whether automatic recovery is allowed.
    pub fn automatic(mut self, automatic: bool) -> Self {
        self.automatic = automatic;
        self
    }

    /// Add a recovery step.
    ///
    /// Returns `true` if added successfully, `false` if steps are full.
    pub fn add_step(&mut self, name: &str, description: &str, timeout: Duration) -> bool {
        let mut n = heapless::String::new();
        if n.push_str(name).is_err() {
            return false;
        }
        let mut d = heapless::String::new();
        if d.push_str(description).is_err() {
            return false;
        }
        self.steps
            .push(RecoveryStep {
                name: n,
                description: d,
                timeout,
                optional: false,
            })
            .is_ok()
    }

    /// Add an optional recovery step.
    ///
    /// Returns `true` if added successfully, `false` if steps are full.
    pub fn add_optional_step(&mut self, name: &str, description: &str, timeout: Duration) -> bool {
        let mut n = heapless::String::new();
        if n.push_str(name).is_err() {
            return false;
        }
        let mut d = heapless::String::new();
        if d.push_str(description).is_err() {
            return false;
        }
        self.steps
            .push(RecoveryStep {
                name: n,
                description: d,
                timeout,
                optional: true,
            })
            .is_ok()
    }

    /// Get the default recovery procedure for a fault type.
    pub fn default_for(fault_type: FaultType) -> Self {
        let mut procedure = Self::new(fault_type);

        match fault_type {
            FaultType::UsbStall => {
                procedure.max_attempts = 3;
                procedure.retry_delay = Duration::from_millis(100);
                procedure.timeout = Duration::from_secs(10);
                let _ = procedure.add_step(
                    "reset_usb",
                    "Reset USB connection",
                    Duration::from_millis(100),
                );
                let _ =
                    procedure.add_step("reconnect", "Reconnect to device", Duration::from_secs(2));
                let _ = procedure.add_step(
                    "verify",
                    "Verify communication",
                    Duration::from_millis(500),
                );
            }
            FaultType::EncoderNaN => {
                procedure.automatic = false;
                procedure.max_attempts = 1;
                procedure.timeout = Duration::from_secs(30);
                let _ =
                    procedure.add_step("calibrate", "Recalibrate encoder", Duration::from_secs(10));
                let _ =
                    procedure.add_step("verify", "Verify encoder readings", Duration::from_secs(5));
            }
            FaultType::ThermalLimit => {
                procedure.automatic = true;
                procedure.max_attempts = 1;
                procedure.timeout = Duration::from_secs(60);
                let _ = procedure.add_step(
                    "reduce_load",
                    "Reduce torque output",
                    Duration::from_millis(50),
                );
                let _ =
                    procedure.add_step("cooldown", "Wait for cooldown", Duration::from_secs(30));
                let _ = procedure.add_step(
                    "verify",
                    "Verify temperature normal",
                    Duration::from_secs(5),
                );
            }
            FaultType::Overcurrent => {
                procedure.automatic = false;
                procedure.max_attempts = 1;
                procedure.timeout = Duration::from_secs(60);
                let _ =
                    procedure.add_step("disconnect", "Disconnect load", Duration::from_millis(100));
                let _ = procedure.add_step("inspect", "Inspect hardware", Duration::from_secs(30));
                let _ =
                    procedure.add_step("verify", "Verify no short circuit", Duration::from_secs(5));
            }
            FaultType::PluginOverrun => {
                procedure.automatic = true;
                procedure.max_attempts = 3;
                procedure.retry_delay = Duration::from_millis(1000);
                procedure.timeout = Duration::from_secs(30);
                let _ = procedure.add_step(
                    "quarantine",
                    "Quarantine plugin",
                    Duration::from_millis(10),
                );
                let _ =
                    procedure.add_step("reset", "Reset plugin state", Duration::from_millis(100));
                let _ = procedure.add_step(
                    "release",
                    "Release from quarantine",
                    Duration::from_millis(10),
                );
            }
            FaultType::TimingViolation => {
                procedure.automatic = true;
                procedure.max_attempts = 1;
                procedure.timeout = Duration::from_millis(100);
                let _ =
                    procedure.add_step("log", "Log violation details", Duration::from_millis(10));
                let _ = procedure.add_optional_step(
                    "adjust_priority",
                    "Adjust RT priority",
                    Duration::from_millis(50),
                );
            }
            FaultType::SafetyInterlockViolation => {
                procedure.automatic = false;
                procedure.max_attempts = 1;
                procedure.timeout = Duration::from_secs(300);
                let _ = procedure.add_step(
                    "reset",
                    "Reset interlock state",
                    Duration::from_millis(100),
                );
                let _ = procedure.add_step(
                    "challenge",
                    "Require new challenge",
                    Duration::from_secs(30),
                );
                let _ = procedure.add_step(
                    "verify",
                    "Verify physical presence",
                    Duration::from_secs(5),
                );
            }
            FaultType::HandsOffTimeout => {
                procedure.automatic = false;
                procedure.max_attempts = 1;
                procedure.timeout = Duration::from_secs(30);
                let _ = procedure.add_step(
                    "reduce_torque",
                    "Reduce to safe torque",
                    Duration::from_millis(50),
                );
                let _ = procedure.add_step(
                    "verify_hands",
                    "Verify hands on wheel",
                    Duration::from_secs(5),
                );
                let _ = procedure.add_step(
                    "rechallenge",
                    "Request new challenge",
                    Duration::from_secs(10),
                );
            }
            FaultType::PipelineFault => {
                procedure.automatic = true;
                procedure.max_attempts = 3;
                procedure.retry_delay = Duration::from_millis(50);
                procedure.timeout = Duration::from_secs(5);
                let _ = procedure.add_step(
                    "reset_pipeline",
                    "Reset filter pipeline",
                    Duration::from_millis(10),
                );
                let _ = procedure.add_step(
                    "verify",
                    "Verify pipeline output",
                    Duration::from_millis(100),
                );
            }
        }

        procedure
    }
}

/// Context for recovery execution.
#[derive(Debug, Clone)]
pub struct RecoveryContext {
    /// Current fault type.
    pub fault_type: FaultType,
    /// Current attempt number.
    pub attempt: u32,
    /// Time when recovery started.
    pub start_time: Duration,
    /// Current step index.
    pub current_step: usize,
    /// Time when current step started.
    pub step_start_time: Duration,
    /// Whether recovery was cancelled.
    pub cancelled: bool,
    /// Recovery procedure being executed.
    pub procedure: RecoveryProcedure,
}

impl RecoveryContext {
    /// Create a new recovery context.
    pub fn new(fault_type: FaultType) -> Self {
        Self {
            fault_type,
            attempt: 1,
            start_time: Duration::ZERO,
            current_step: 0,
            step_start_time: Duration::ZERO,
            cancelled: false,
            procedure: RecoveryProcedure::default_for(fault_type),
        }
    }

    /// Create a recovery context with a specific procedure.
    pub fn with_procedure(procedure: RecoveryProcedure) -> Self {
        Self {
            fault_type: procedure.fault_type,
            attempt: 1,
            start_time: Duration::ZERO,
            current_step: 0,
            step_start_time: Duration::ZERO,
            cancelled: false,
            procedure,
        }
    }

    /// Start the recovery process.
    pub fn start(&mut self, current_time: Duration) {
        self.start_time = current_time;
        self.step_start_time = current_time;
        self.current_step = 0;
        self.attempt = 1;
    }

    /// Advance to the next step.
    pub fn advance_step(&mut self, current_time: Duration) {
        self.current_step = self.current_step.saturating_add(1);
        self.step_start_time = current_time;
    }

    /// Check if recovery has timed out.
    pub fn is_timed_out(&self, current_time: Duration) -> bool {
        current_time.saturating_sub(self.start_time) > self.procedure.timeout
    }

    /// Check if current step has timed out.
    pub fn is_step_timed_out(&self, current_time: Duration) -> bool {
        if let Some(step) = self.procedure.steps.get(self.current_step) {
            return current_time.saturating_sub(self.step_start_time) > step.timeout;
        }
        false
    }

    /// Check if all steps are complete.
    pub fn is_complete(&self) -> bool {
        self.current_step >= self.procedure.steps.len()
    }

    /// Check if more attempts are available.
    pub fn can_retry(&self) -> bool {
        self.attempt < self.procedure.max_attempts
    }

    /// Start a retry attempt.
    pub fn start_retry(&mut self, current_time: Duration) -> bool {
        if !self.can_retry() {
            return false;
        }
        self.attempt = self.attempt.saturating_add(1);
        self.current_step = 0;
        self.start_time = current_time;
        self.step_start_time = current_time;
        true
    }

    /// Cancel the recovery.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Get the current step (if any).
    pub fn current_step(&self) -> Option<&RecoveryStep> {
        self.procedure.steps.get(self.current_step)
    }

    /// Get elapsed time since recovery started.
    pub fn elapsed(&self, current_time: Duration) -> Duration {
        current_time.saturating_sub(self.start_time)
    }

    /// Get elapsed time since current step started.
    pub fn step_elapsed(&self, current_time: Duration) -> Duration {
        current_time.saturating_sub(self.step_start_time)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_result_success() {
        let result = RecoveryResult::success(Duration::from_millis(100), 1);
        assert!(result.is_success());
        assert_eq!(result.status, RecoveryStatus::Completed);
    }

    #[test]
    fn test_recovery_result_failed() {
        let result = RecoveryResult::failed(Duration::from_millis(100), 3, "test error");
        assert!(!result.is_success());
        assert_eq!(result.status, RecoveryStatus::Failed);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_recovery_result_timeout() {
        let result = RecoveryResult::timeout(Duration::from_secs(10), 2);
        assert_eq!(result.status, RecoveryStatus::Timeout);
    }

    #[test]
    fn test_recovery_procedure_creation() {
        let procedure = RecoveryProcedure::new(FaultType::UsbStall);
        assert_eq!(procedure.fault_type, FaultType::UsbStall);
        assert_eq!(procedure.max_attempts, 3);
        assert!(procedure.automatic);
    }

    #[test]
    fn test_recovery_procedure_add_step() {
        let mut procedure = RecoveryProcedure::new(FaultType::UsbStall);
        assert!(procedure.add_step("test", "Test step", Duration::from_millis(100)));
        assert_eq!(procedure.steps.len(), 1);
    }

    #[test]
    fn test_recovery_procedure_default_for_usb() {
        let procedure = RecoveryProcedure::default_for(FaultType::UsbStall);
        assert_eq!(procedure.fault_type, FaultType::UsbStall);
        assert!(procedure.automatic);
        assert!(!procedure.steps.is_empty());
    }

    #[test]
    fn test_recovery_procedure_default_for_encoder() {
        let procedure = RecoveryProcedure::default_for(FaultType::EncoderNaN);
        assert_eq!(procedure.fault_type, FaultType::EncoderNaN);
        assert!(!procedure.automatic); // Requires manual calibration
    }

    #[test]
    fn test_recovery_context_creation() {
        let ctx = RecoveryContext::new(FaultType::UsbStall);
        assert_eq!(ctx.fault_type, FaultType::UsbStall);
        assert_eq!(ctx.attempt, 1);
        assert_eq!(ctx.current_step, 0);
        assert!(!ctx.cancelled);
    }

    #[test]
    fn test_recovery_context_start() {
        let mut ctx = RecoveryContext::new(FaultType::UsbStall);
        ctx.start(Duration::from_millis(100));

        assert_eq!(ctx.start_time, Duration::from_millis(100));
        assert_eq!(ctx.step_start_time, Duration::from_millis(100));
    }

    #[test]
    fn test_recovery_context_advance_step() {
        let mut ctx = RecoveryContext::new(FaultType::UsbStall);
        ctx.start(Duration::from_millis(0));
        ctx.advance_step(Duration::from_millis(50));

        assert_eq!(ctx.current_step, 1);
        assert_eq!(ctx.step_start_time, Duration::from_millis(50));
    }

    #[test]
    fn test_recovery_context_timeout() {
        let mut ctx = RecoveryContext::new(FaultType::UsbStall);
        ctx.procedure.timeout = Duration::from_millis(100);
        ctx.start(Duration::from_millis(0));

        assert!(!ctx.is_timed_out(Duration::from_millis(50)));
        assert!(ctx.is_timed_out(Duration::from_millis(150)));
    }

    #[test]
    fn test_recovery_context_retry() {
        let mut ctx = RecoveryContext::new(FaultType::UsbStall);
        ctx.procedure.max_attempts = 3;
        ctx.start(Duration::from_millis(0));

        assert!(ctx.can_retry());
        assert!(ctx.start_retry(Duration::from_millis(100)));
        assert_eq!(ctx.attempt, 2);

        assert!(ctx.can_retry());
        assert!(ctx.start_retry(Duration::from_millis(200)));
        assert_eq!(ctx.attempt, 3);

        assert!(!ctx.can_retry());
        assert!(!ctx.start_retry(Duration::from_millis(300)));
    }

    #[test]
    fn test_recovery_context_cancel() {
        let mut ctx = RecoveryContext::new(FaultType::UsbStall);
        assert!(!ctx.cancelled);

        ctx.cancel();
        assert!(ctx.cancelled);
    }
}
