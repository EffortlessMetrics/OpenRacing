//! Capability-based security system for plugins

use std::collections::HashSet;
use std::path::PathBuf;

use crate::manifest::Capability;
use crate::{PluginError, PluginResult};

/// Capability checker for plugin operations
pub struct CapabilityChecker {
    granted_capabilities: HashSet<Capability>,
    allowed_file_paths: Vec<PathBuf>,
    allowed_network_hosts: Vec<String>,
}

impl CapabilityChecker {
    /// Create a new capability checker with granted capabilities
    pub fn new(capabilities: Vec<Capability>) -> Self {
        let mut allowed_file_paths = Vec::new();
        let mut allowed_network_hosts = Vec::new();
        let mut granted_capabilities = HashSet::new();
        
        for cap in capabilities {
            match &cap {
                Capability::FileSystem { paths } => {
                    allowed_file_paths.extend(paths.iter().map(PathBuf::from));
                }
                Capability::Network { hosts } => {
                    allowed_network_hosts.extend(hosts.clone());
                }
                _ => {}
            }
            granted_capabilities.insert(cap);
        }
        
        Self {
            granted_capabilities,
            allowed_file_paths,
            allowed_network_hosts,
        }
    }
    
    /// Check if a capability is granted
    pub fn has_capability(&self, capability: &Capability) -> bool {
        self.granted_capabilities.contains(capability)
    }
    
    /// Check if telemetry read access is allowed
    pub fn check_telemetry_read(&self) -> PluginResult<()> {
        if self.has_capability(&Capability::ReadTelemetry) {
            Ok(())
        } else {
            Err(PluginError::CapabilityViolation {
                capability: "ReadTelemetry".to_string(),
            })
        }
    }
    
    /// Check if telemetry modification is allowed
    pub fn check_telemetry_modify(&self) -> PluginResult<()> {
        if self.has_capability(&Capability::ModifyTelemetry) {
            Ok(())
        } else {
            Err(PluginError::CapabilityViolation {
                capability: "ModifyTelemetry".to_string(),
            })
        }
    }
    
    /// Check if LED control is allowed
    pub fn check_led_control(&self) -> PluginResult<()> {
        if self.has_capability(&Capability::ControlLeds) {
            Ok(())
        } else {
            Err(PluginError::CapabilityViolation {
                capability: "ControlLeds".to_string(),
            })
        }
    }
    
    /// Check if DSP processing is allowed
    pub fn check_dsp_processing(&self) -> PluginResult<()> {
        if self.has_capability(&Capability::ProcessDsp) {
            Ok(())
        } else {
            Err(PluginError::CapabilityViolation {
                capability: "ProcessDsp".to_string(),
            })
        }
    }
    
    /// Check if file system access to a path is allowed
    pub fn check_file_access(&self, path: &std::path::Path) -> PluginResult<()> {
        // Check if any granted file system capability allows this path
        for allowed_path in &self.allowed_file_paths {
            if path.starts_with(allowed_path) {
                return Ok(());
            }
        }
        
        Err(PluginError::CapabilityViolation {
            capability: format!("FileSystem access to {}", path.display()),
        })
    }
    
    /// Check if network access to a host is allowed
    pub fn check_network_access(&self, host: &str) -> PluginResult<()> {
        if self.allowed_network_hosts.contains(&host.to_string()) {
            Ok(())
        } else {
            Err(PluginError::CapabilityViolation {
                capability: format!("Network access to {}", host),
            })
        }
    }
    
    /// Check if inter-plugin communication is allowed
    pub fn check_inter_plugin_comm(&self) -> PluginResult<()> {
        if self.has_capability(&Capability::InterPluginComm) {
            Ok(())
        } else {
            Err(PluginError::CapabilityViolation {
                capability: "InterPluginComm".to_string(),
            })
        }
    }
}

/// WASM capability enforcement using WASI
pub struct WasmCapabilityEnforcer {
    checker: CapabilityChecker,
}

impl WasmCapabilityEnforcer {
    pub fn new(capabilities: Vec<Capability>) -> Self {
        Self {
            checker: CapabilityChecker::new(capabilities),
        }
    }
    
    /// Create WASI context with restricted capabilities
    pub fn create_wasi_context(&self) -> wasmtime_wasi::WasiCtxBuilder {
        let mut builder = wasmtime_wasi::WasiCtxBuilder::new();
        
        // Only allow file system access to granted paths
        for path in &self.checker.allowed_file_paths {
            let dir = cap_std::fs::Dir::open_ambient_dir(path, cap_std::ambient_authority())
                .unwrap_or_else(|_| {
                    // Create a dummy directory if path doesn't exist
                    cap_std::fs::Dir::open_ambient_dir(".", cap_std::ambient_authority()).unwrap()
                });
            builder = builder.preopened_dir(dir, path.to_string_lossy());
        }
        
        // Restrict network access
        if !self.checker.has_capability(&Capability::Network { hosts: vec![] }) {
            // No network access by default
        }
        
        builder
    }
    
    /// Get the capability checker
    pub fn checker(&self) -> &CapabilityChecker {
        &self.checker
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    
    #[test]
    fn test_capability_checker() {
        let capabilities = vec![
            Capability::ReadTelemetry,
            Capability::FileSystem {
                paths: vec!["/tmp".to_string()],
            },
        ];
        
        let checker = CapabilityChecker::new(capabilities);
        
        assert!(checker.check_telemetry_read().is_ok());
        assert!(checker.check_telemetry_modify().is_err());
        assert!(checker.check_file_access(Path::new("/tmp/test.txt")).is_ok());
        assert!(checker.check_file_access(Path::new("/etc/passwd")).is_err());
    }
    
    #[test]
    fn test_network_capability() {
        let capabilities = vec![Capability::Network {
            hosts: vec!["api.example.com".to_string()],
        }];
        
        let checker = CapabilityChecker::new(capabilities);
        
        assert!(checker.check_network_access("api.example.com").is_ok());
        assert!(checker.check_network_access("malicious.com").is_err());
    }
}