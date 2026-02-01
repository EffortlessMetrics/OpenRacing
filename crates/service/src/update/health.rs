//! Health check implementation for update system

use crate::update::{HealthCheck, HealthCheckType};
use anyhow::{Context, Result};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

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
    tracing::debug!("Checking service start");

    // Try to start the service
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

    // Wait a moment for service to fully start
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Check if service is actually running
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

    tracing::debug!("Service start check passed");
    Ok(())
}

/// Check if the racing wheel service responds to ping
async fn check_service_ping() -> Result<()> {
    tracing::debug!("Checking service ping");

    // Use wheelctl to ping the service
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

    tracing::debug!("Service ping check passed");
    Ok(())
}

/// Check if device enumeration works
async fn check_device_enumeration() -> Result<()> {
    tracing::debug!("Checking device enumeration");

    // Use wheelctl to list devices
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

    tracing::debug!("Device enumeration check passed");
    Ok(())
}

/// Check if a command runs successfully with expected exit code
async fn check_command(command: &str, args: &[String], expected_exit_code: i32) -> Result<()> {
    tracing::debug!("Checking command: {} {:?}", command, args);

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

    tracing::debug!("Command check passed");
    Ok(())
}

/// Check if a file exists and has expected properties
async fn check_file(
    path: &std::path::Path,
    expected_hash: Option<&str>,
    expected_size: Option<u64>,
) -> Result<()> {
    tracing::debug!("Checking file: {}", path.display());

    // Check if file exists
    if !path.exists() {
        return Err(anyhow::anyhow!("File does not exist: {}", path.display()));
    }

    // Check file size if specified
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

    // Check file hash if specified
    if let Some(expected_hash) = expected_hash {
        let actual_hash = crate::update::delta::compute_file_hash(path)
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

    tracing::debug!("File check passed");
    Ok(())
}

/// Health check runner for batch execution
pub struct HealthCheckRunner {
    /// Maximum number of concurrent health checks
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

        // Start initial batch of checks
        for _ in 0..self.max_concurrent.min(checks.len()) {
            if let Some((index, check)) = check_iter.next() {
                let check_future = run_health_check_with_result(index, check);
                futures.push(check_future);
            }
        }

        // Process results and start new checks
        while let Some(result) = futures.next().await {
            results.push(result);

            // Start next check if available
            if let Some((index, check)) = check_iter.next() {
                let check_future = run_health_check_with_result(index, check);
                futures.push(check_future);
            }
        }

        // Sort results by original index
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

/// Result of a single health check
#[derive(Debug)]
pub struct HealthCheckResult {
    pub index: usize,
    pub check_id: String,
    pub success: bool,
    pub error: Option<String>,
    pub duration: Duration,
}

/// Summary of health check execution
#[derive(Debug)]
pub struct HealthCheckSummary {
    pub total_checks: usize,
    pub passed_checks: usize,
    pub failed_checks: usize,
    pub critical_failures: usize,
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
        // Test with a simple command that should succeed
        let result = check_command("echo", &["hello".to_string()], 0).await;
        assert!(result.is_ok());

        // Test with a command that should fail
        let result = check_command("false", &[], 1).await;
        assert!(result.is_ok());

        // Test with wrong expected exit code
        let result = check_command("true", &[], 1).await;
        assert!(result.is_err());
        Ok(())
    }
}
