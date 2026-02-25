//! Health check implementation for firmware update system
//!
//! Provides health check types and runners for validating firmware updates.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::debug;

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheck {
    /// Check identifier
    pub id: String,

    /// Human-readable description
    pub description: String,

    /// Check type
    pub check_type: HealthCheckType,

    /// Timeout for the check
    pub timeout_seconds: u32,

    /// Whether failure should abort the update
    pub critical: bool,
}

/// Type of health check
#[derive(Debug, Clone)]
pub enum HealthCheckType {
    /// Check if service starts successfully
    ServiceStart,

    /// Check if service responds to ping
    ServicePing,

    /// Check if device enumeration works
    DeviceEnumeration,

    /// Run custom command and check exit code
    Command {
        /// Command to execute
        command: String,
        /// Command arguments
        args: Vec<String>,
        /// Expected exit code
        expected_exit_code: i32,
    },

    /// Check if file exists and has expected properties
    FileCheck {
        /// Path to the file
        path: std::path::PathBuf,
        /// Expected SHA256 hash
        expected_hash: Option<String>,
        /// Expected file size
        expected_size: Option<u64>,
    },
}

/// Result of a single health check
#[derive(Debug)]
pub struct HealthCheckResult {
    /// Index of the check in the original list
    pub index: usize,
    /// Check identifier
    pub check_id: String,
    /// Whether the check passed
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Duration of the check
    pub duration: Duration,
}

/// Summary of health check execution
#[derive(Debug)]
pub struct HealthCheckSummary {
    /// Total number of checks
    pub total_checks: usize,
    /// Number of passed checks
    pub passed_checks: usize,
    /// Number of failed checks
    pub failed_checks: usize,
    /// Number of critical failures
    pub critical_failures: usize,
    /// Individual check results
    pub results: Vec<HealthCheckResult>,
}

impl HealthCheckSummary {
    /// Check if all critical health checks passed
    pub fn all_critical_passed(&self) -> bool {
        self.critical_failures == 0
    }

    /// Get the overall success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_checks == 0 {
            1.0
        } else {
            self.passed_checks as f64 / self.total_checks as f64
        }
    }
}

/// Run a single health check
pub async fn run_health_check(check: &HealthCheck) -> Result<()> {
    let timeout_duration = Duration::from_secs(check.timeout_seconds as u64);

    let result = timeout(timeout_duration, execute_health_check(check))
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "Health check timed out after {} seconds",
                check.timeout_seconds
            )
        })?;

    result.with_context(|| format!("Health check '{}' failed", check.id))
}

/// Execute the actual health check based on its type
async fn execute_health_check(check: &HealthCheck) -> Result<()> {
    match &check.check_type {
        HealthCheckType::ServiceStart => check_service_start().await,

        HealthCheckType::ServicePing => check_service_ping().await,

        HealthCheckType::DeviceEnumeration => check_device_enumeration().await,

        HealthCheckType::Command {
            command,
            args,
            expected_exit_code,
        } => check_command(command, args, *expected_exit_code).await,

        HealthCheckType::FileCheck {
            path,
            expected_hash,
            expected_size,
        } => check_file(path, expected_hash.as_deref(), *expected_size).await,
    }
}

/// Check if the racing wheel service starts successfully
async fn check_service_start() -> Result<()> {
    debug!("Checking service start");

    #[cfg(unix)]
    {
        let output = Command::new("systemctl")
            .args(["--user", "start", "racing-wheel-suite.service"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to execute systemctl start command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Service start failed: {}", stderr));
        }

        tokio::time::sleep(Duration::from_secs(2)).await;

        let status_output = Command::new("systemctl")
            .args(["--user", "is-active", "racing-wheel-suite.service"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to check service status")?;

        if !status_output.status.success() {
            return Err(anyhow::anyhow!("Service is not active after start"));
        }

        let status = String::from_utf8_lossy(&status_output.stdout)
            .trim()
            .to_owned();
        if status != "active" {
            return Err(anyhow::anyhow!(
                "Service status is '{}', expected 'active'",
                status
            ));
        }
    }

    debug!("Service start check passed");
    Ok(())
}

/// Check if the racing wheel service responds to ping
async fn check_service_ping() -> Result<()> {
    debug!("Checking service ping");

    let output = Command::new("wheelctl")
        .args(["ping"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("Failed to execute wheelctl ping command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Service ping failed: {}", stderr));
    }

    debug!("Service ping check passed");
    Ok(())
}

/// Check if device enumeration works
async fn check_device_enumeration() -> Result<()> {
    debug!("Checking device enumeration");

    let output = Command::new("wheelctl")
        .args(["devices", "list"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("Failed to execute wheelctl devices list command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Device enumeration failed: {}", stderr));
    }

    debug!("Device enumeration check passed");
    Ok(())
}

/// Check if a command runs successfully with expected exit code
async fn check_command(command: &str, args: &[String], expected_exit_code: i32) -> Result<()> {
    debug!("Checking command: {} {:?}", command, args);

    let output = Command::new(command)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .with_context(|| format!("Failed to execute command: {}", command))?;

    let actual_exit_code = output.status.code().unwrap_or(-1);

    if actual_exit_code != expected_exit_code {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "Command '{}' exited with code {}, expected {}. stderr: {}",
            command,
            actual_exit_code,
            expected_exit_code,
            stderr
        ));
    }

    debug!("Command check passed");
    Ok(())
}

/// Check if a file exists and has expected properties
async fn check_file(
    path: &Path,
    expected_hash: Option<&str>,
    expected_size: Option<u64>,
) -> Result<()> {
    debug!("Checking file: {}", path.display());

    if !path.exists() {
        return Err(anyhow::anyhow!("File does not exist: {}", path.display()));
    }

    if let Some(expected_size) = expected_size {
        let metadata = tokio::fs::metadata(path)
            .await
            .context("Failed to get file metadata")?;

        let actual_size = metadata.len();
        if actual_size != expected_size {
            return Err(anyhow::anyhow!(
                "File size mismatch: expected {}, got {}",
                expected_size,
                actual_size
            ));
        }
    }

    if let Some(expected_hash) = expected_hash {
        let actual_hash = crate::delta::compute_file_hash(path)
            .await
            .context("Failed to compute file hash")?;

        if actual_hash != expected_hash {
            return Err(anyhow::anyhow!(
                "File hash mismatch: expected {}, got {}",
                expected_hash,
                actual_hash
            ));
        }
    }

    debug!("File check passed");
    Ok(())
}

/// Health check runner for batch execution
pub struct HealthCheckRunner {
    max_concurrent: usize,
}

impl HealthCheckRunner {
    /// Create a new health check runner
    pub fn new(max_concurrent: usize) -> Self {
        Self { max_concurrent }
    }

    /// Run multiple health checks concurrently
    pub async fn run_checks(&self, checks: &[HealthCheck]) -> Result<Vec<HealthCheckResult>> {
        use futures::stream::{FuturesUnordered, StreamExt};

        let mut results = Vec::new();
        let mut futures = FuturesUnordered::new();
        let mut check_iter = checks.iter().enumerate();

        for _ in 0..self.max_concurrent.min(checks.len()) {
            if let Some((index, check)) = check_iter.next() {
                let check_future = run_health_check_with_result(index, check);
                futures.push(check_future);
            }
        }

        while let Some(result) = futures.next().await {
            results.push(result);

            if let Some((index, check)) = check_iter.next() {
                let check_future = run_health_check_with_result(index, check);
                futures.push(check_future);
            }
        }

        results.sort_by_key(|r| r.index);

        Ok(results)
    }

    /// Run health checks and return summary
    pub async fn run_checks_with_summary(
        &self,
        checks: &[HealthCheck],
    ) -> Result<HealthCheckSummary> {
        let results = self.run_checks(checks).await?;

        let mut summary = HealthCheckSummary {
            total_checks: checks.len(),
            passed_checks: 0,
            failed_checks: 0,
            critical_failures: 0,
            results,
        };

        for (i, check) in checks.iter().enumerate() {
            if let Some(result) = summary.results.get(i) {
                if result.success {
                    summary.passed_checks += 1;
                } else {
                    summary.failed_checks += 1;
                    if check.critical {
                        summary.critical_failures += 1;
                    }
                }
            }
        }

        Ok(summary)
    }
}

impl Default for HealthCheckRunner {
    fn default() -> Self {
        Self::new(4)
    }
}

/// Run a health check and return a detailed result
async fn run_health_check_with_result(index: usize, check: &HealthCheck) -> HealthCheckResult {
    let start_time = std::time::Instant::now();

    match run_health_check(check).await {
        Ok(()) => HealthCheckResult {
            index,
            check_id: check.id.clone(),
            success: true,
            error: None,
            duration: start_time.elapsed(),
        },
        Err(e) => HealthCheckResult {
            index,
            check_id: check.id.clone(),
            success: false,
            error: Some(e.to_string()),
            duration: start_time.elapsed(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_file_check_success() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        tokio::fs::write(temp_file.path(), b"test content").await?;

        let result = check_file(temp_file.path(), None, Some(12)).await;
        assert!(result.is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn test_file_check_size_mismatch() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        tokio::fs::write(temp_file.path(), b"test content").await?;

        let result = check_file(temp_file.path(), None, Some(100)).await;
        assert!(result.is_err());
        let err = result.expect_err("Expected size mismatch error");
        assert!(err.to_string().contains("size mismatch"));
        Ok(())
    }

    #[tokio::test]
    async fn test_command_check() -> Result<()> {
        #[cfg(windows)]
        let result = check_command(
            "cmd",
            &["/C".to_string(), "echo".to_string(), "hello".to_string()],
            0,
        )
        .await;
        #[cfg(not(windows))]
        let result = check_command("echo", &["hello".to_string()], 0).await;
        assert!(result.is_ok());

        Ok(())
    }

    #[test]
    fn test_health_check_summary() {
        let summary = HealthCheckSummary {
            total_checks: 10,
            passed_checks: 8,
            failed_checks: 2,
            critical_failures: 1,
            results: Vec::new(),
        };

        assert!(!summary.all_critical_passed());
        assert!((summary.success_rate() - 0.8).abs() < 0.001);
    }
}
