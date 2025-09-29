//! Platform-specific service daemon implementations

use anyhow::{Result, Context};
use std::path::PathBuf;
use tracing::{info, warn};

use crate::daemon::ServiceDaemon;

// Windows-specific implementations
#[cfg(windows)]
impl ServiceDaemon {
    pub(crate) async fn install_windows_service() -> Result<()> {
        use std::process::Command;
        
        let exe_path = std::env::current_exe()
            .context("Failed to get current executable path")?;
        
        // Create service using sc.exe (no admin rights required for user services)
        let output = Command::new("sc")
            .args(&[
                "create",
                "wheeld",
                &format!("binPath= \"{}\"", exe_path.display()),
                "start= auto",
                "obj= LocalSystem",
                "DisplayName= Racing Wheel Service",
            ])
            .output()
            .context("Failed to execute sc command")?;
        
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to create service: {}", error));
        }
        
        info!("Windows service installed successfully");
        Ok(())
    }
    
    pub(crate) async fn uninstall_windows_service() -> Result<()> {
        use std::process::Command;
        
        // Stop service first
        let _ = Command::new("sc")
            .args(&["stop", "wheeld"])
            .output();
        
        // Delete service
        let output = Command::new("sc")
            .args(&["delete", "wheeld"])
            .output()
            .context("Failed to execute sc command")?;
        
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to delete service: {}", error));
        }
        
        info!("Windows service uninstalled successfully");
        Ok(())
    }
    
    pub(crate) async fn status_windows_service() -> Result<String> {
        use std::process::Command;
        
        let output = Command::new("sc")
            .args(&["query", "wheeld"])
            .output()
            .context("Failed to execute sc command")?;
        
        if output.status.success() {
            let status = String::from_utf8_lossy(&output.stdout);
            Ok(status.to_string())
        } else {
            Ok("Service not installed".to_string())
        }
    }
}

// Unix-specific implementations
#[cfg(unix)]
impl ServiceDaemon {
    pub(crate) async fn install_unix_service() -> Result<()> {
        let exe_path = std::env::current_exe()
            .context("Failed to get current executable path")?;
        
        let home_dir = std::env::var("HOME")
            .context("HOME environment variable not set")?;
        
        let systemd_dir = PathBuf::from(&home_dir)
            .join(".config/systemd/user");
        
        tokio::fs::create_dir_all(&systemd_dir).await
            .context("Failed to create systemd user directory")?;
        
        let service_file = systemd_dir.join("wheeld.service");
        
        let service_content = format!(
            r#"[Unit]
Description=Racing Wheel Service
After=graphical-session.target

[Service]
Type=simple
ExecStart={}
Restart=always
RestartSec=5
Environment=HOME={}

[Install]
WantedBy=default.target
"#,
            exe_path.display(),
            home_dir
        );
        
        tokio::fs::write(&service_file, service_content).await
            .context("Failed to write service file")?;
        
        // Enable and start the service
        let output = std::process::Command::new("systemctl")
            .args(&["--user", "daemon-reload"])
            .output()
            .context("Failed to reload systemd")?;
        
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to reload systemd: {}", error);
        }
        
        let output = std::process::Command::new("systemctl")
            .args(&["--user", "enable", "wheeld.service"])
            .output()
            .context("Failed to enable service")?;
        
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to enable service: {}", error));
        }
        
        info!("Unix service installed successfully at {:?}", service_file);
        Ok(())
    }
    
    pub(crate) async fn uninstall_unix_service() -> Result<()> {
        // Stop and disable service
        let _ = std::process::Command::new("systemctl")
            .args(&["--user", "stop", "wheeld.service"])
            .output();
        
        let _ = std::process::Command::new("systemctl")
            .args(&["--user", "disable", "wheeld.service"])
            .output();
        
        // Remove service file
        let home_dir = std::env::var("HOME")
            .context("HOME environment variable not set")?;
        
        let service_file = PathBuf::from(&home_dir)
            .join(".config/systemd/user/wheeld.service");
        
        if service_file.exists() {
            tokio::fs::remove_file(&service_file).await
                .context("Failed to remove service file")?;
        }
        
        // Reload systemd
        let _ = std::process::Command::new("systemctl")
            .args(&["--user", "daemon-reload"])
            .output();
        
        info!("Unix service uninstalled successfully");
        Ok(())
    }
    
    pub(crate) async fn status_unix_service() -> Result<String> {
        let output = std::process::Command::new("systemctl")
            .args(&["--user", "status", "wheeld.service"])
            .output()
            .context("Failed to execute systemctl command")?;
        
        let status = String::from_utf8_lossy(&output.stdout);
        Ok(status.to_string())
    }
}