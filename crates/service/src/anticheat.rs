//! Anti-cheat compatibility documentation and verification
//!
//! Generates comprehensive documentation proving compatibility with
//! anti-cheat systems by documenting all telemetry methods and
//! system interactions.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Anti-cheat compatibility report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiCheatReport {
    /// Report generation timestamp
    pub generated_at: String,
    /// Software version
    pub version: String,
    /// Platform information
    pub platform: PlatformInfo,
    /// Process architecture
    pub process_info: ProcessInfo,
    /// Telemetry methods used
    pub telemetry_methods: Vec<TelemetryMethod>,
    /// File system access patterns
    pub file_access: Vec<FileAccess>,
    /// Network access patterns
    pub network_access: Vec<NetworkAccess>,
    /// System API usage
    pub system_apis: Vec<SystemApi>,
    /// Security measures
    pub security_measures: Vec<SecurityMeasure>,
}

/// Platform information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    /// Operating system
    pub os: String,
    /// OS version
    pub os_version: String,
    /// Architecture
    pub arch: String,
    /// Kernel version (Linux)
    pub kernel_version: Option<String>,
}

/// Process information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    /// Process name
    pub name: String,
    /// Process architecture
    pub arch: String,
    /// Privilege level
    pub privilege_level: String,
    /// Parent process
    pub parent_process: Option<String>,
    /// Child processes
    pub child_processes: Vec<String>,
    /// DLL injection used
    pub dll_injection: bool,
    /// Kernel drivers used
    pub kernel_drivers: Vec<String>,
}

/// Telemetry method documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryMethod {
    /// Game name
    pub game: String,
    /// Method type
    pub method_type: String,
    /// Description
    pub description: String,
    /// Implementation details
    pub implementation: String,
    /// Memory access pattern
    pub memory_access: String,
    /// File access pattern
    pub file_access: Option<String>,
    /// Network protocol
    pub network_protocol: Option<String>,
    /// Anti-cheat compatibility
    pub anticheat_compatible: bool,
    /// Compatibility notes
    pub compatibility_notes: String,
}

/// File system access documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccess {
    /// File path pattern
    pub path_pattern: String,
    /// Access type (read/write/create)
    pub access_type: String,
    /// Purpose
    pub purpose: String,
    /// Frequency
    pub frequency: String,
    /// User consent required
    pub user_consent: bool,
}

/// Network access documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAccess {
    /// Protocol
    pub protocol: String,
    /// Direction (inbound/outbound)
    pub direction: String,
    /// Purpose
    pub purpose: String,
    /// Endpoints
    pub endpoints: Vec<String>,
    /// Data transmitted
    pub data_transmitted: String,
    /// User consent required
    pub user_consent: bool,
}

/// System API usage documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemApi {
    /// API name
    pub api_name: String,
    /// Purpose
    pub purpose: String,
    /// Privilege level required
    pub privilege_level: String,
    /// Frequency of use
    pub frequency: String,
    /// Anti-cheat impact
    pub anticheat_impact: String,
}

/// Security measure documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityMeasure {
    /// Measure name
    pub name: String,
    /// Description
    pub description: String,
    /// Implementation
    pub implementation: String,
    /// Effectiveness
    pub effectiveness: String,
}

impl AntiCheatReport {
    /// Generate comprehensive anti-cheat compatibility report
    pub async fn generate() -> Result<Self> {
        info!("Generating anti-cheat compatibility report");

        let report = Self {
            generated_at: chrono::Utc::now().to_rfc3339(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: Self::collect_platform_info().await?,
            process_info: Self::collect_process_info().await?,
            telemetry_methods: Self::document_telemetry_methods().await?,
            file_access: Self::document_file_access().await?,
            network_access: Self::document_network_access().await?,
            system_apis: Self::document_system_apis().await?,
            security_measures: Self::document_security_measures().await?,
        };

        debug!("Anti-cheat report generated successfully");
        Ok(report)
    }

    /// Convert report to markdown format
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("# Racing Wheel Software - Anti-Cheat Compatibility Report\n\n");
        md.push_str(&format!("**Generated:** {}\n", self.generated_at));
        md.push_str(&format!("**Version:** {}\n\n", self.version));

        md.push_str("## Executive Summary\n\n");
        md.push_str("This software is designed to be fully compatible with anti-cheat systems. ");
        md.push_str("It uses only documented, legitimate methods for game integration and ");
        md.push_str(
            "does not employ any techniques commonly associated with cheating software.\n\n",
        );

        md.push_str("### Key Compatibility Points\n\n");
        md.push_str("- ✅ **No DLL Injection**: Uses only external process communication\n");
        md.push_str("- ✅ **No Kernel Drivers**: Operates entirely in user space\n");
        md.push_str("- ✅ **Documented Methods**: All telemetry methods are publicly documented\n");
        md.push_str("- ✅ **Process Isolation**: Separate processes with clear boundaries\n");
        md.push_str("- ✅ **Signed Binaries**: All executables are digitally signed\n");
        md.push_str("- ✅ **Open Source**: Source code is publicly available for audit\n\n");

        // Platform Information
        md.push_str("## Platform Information\n\n");
        md.push_str(&format!(
            "- **OS:** {} {}\n",
            self.platform.os, self.platform.os_version
        ));
        md.push_str(&format!("- **Architecture:** {}\n", self.platform.arch));
        if let Some(kernel) = &self.platform.kernel_version {
            md.push_str(&format!("- **Kernel:** {}\n", kernel));
        }
        md.push('\n');

        // Process Information
        md.push_str("## Process Architecture\n\n");
        md.push_str(&format!(
            "- **Main Process:** {} ({})\n",
            self.process_info.name, self.process_info.arch
        ));
        md.push_str(&format!(
            "- **Privilege Level:** {}\n",
            self.process_info.privilege_level
        ));
        md.push_str(&format!(
            "- **DLL Injection:** {}\n",
            if self.process_info.dll_injection {
                "❌ Yes"
            } else {
                "✅ No"
            }
        ));
        md.push_str(&format!(
            "- **Kernel Drivers:** {}\n",
            if self.process_info.kernel_drivers.is_empty() {
                "✅ None"
            } else {
                "❌ Present"
            }
        ));

        if !self.process_info.child_processes.is_empty() {
            md.push_str("- **Child Processes:**\n");
            for child in &self.process_info.child_processes {
                md.push_str(&format!("  - {}\n", child));
            }
        }
        md.push('\n');

        // Telemetry Methods
        md.push_str("## Telemetry Methods\n\n");
        md.push_str("All telemetry methods used are documented and legitimate:\n\n");

        for method in &self.telemetry_methods {
            md.push_str(&format!("### {} - {}\n\n", method.game, method.method_type));
            md.push_str(&format!("**Description:** {}\n\n", method.description));
            md.push_str(&format!(
                "**Implementation:** {}\n\n",
                method.implementation
            ));
            md.push_str(&format!("**Memory Access:** {}\n\n", method.memory_access));

            if let Some(file_access) = &method.file_access {
                md.push_str(&format!("**File Access:** {}\n\n", file_access));
            }

            if let Some(network) = &method.network_protocol {
                md.push_str(&format!("**Network Protocol:** {}\n\n", network));
            }

            md.push_str(&format!(
                "**Anti-Cheat Compatible:** {}\n\n",
                if method.anticheat_compatible {
                    "✅ Yes"
                } else {
                    "❌ No"
                }
            ));
            md.push_str(&format!("**Notes:** {}\n\n", method.compatibility_notes));
        }

        // File Access
        md.push_str("## File System Access\n\n");
        md.push_str("| Path Pattern | Access Type | Purpose | Frequency | User Consent |\n");
        md.push_str("|--------------|-------------|---------|-----------|-------------|\n");

        for access in &self.file_access {
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                access.path_pattern,
                access.access_type,
                access.purpose,
                access.frequency,
                if access.user_consent {
                    "✅ Required"
                } else {
                    "❌ Not Required"
                }
            ));
        }
        md.push('\n');

        // Network Access
        if !self.network_access.is_empty() {
            md.push_str("## Network Access\n\n");
            md.push_str("| Protocol | Direction | Purpose | Endpoints | User Consent |\n");
            md.push_str("|----------|-----------|---------|-----------|-------------|\n");

            for access in &self.network_access {
                md.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    access.protocol,
                    access.direction,
                    access.purpose,
                    access.endpoints.join(", "),
                    if access.user_consent {
                        "✅ Required"
                    } else {
                        "❌ Not Required"
                    }
                ));
            }
            md.push('\n');
        }

        // System APIs
        md.push_str("## System API Usage\n\n");
        md.push_str("| API | Purpose | Privilege Level | Frequency | Anti-Cheat Impact |\n");
        md.push_str("|-----|---------|-----------------|-----------|------------------|\n");

        for api in &self.system_apis {
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                api.api_name, api.purpose, api.privilege_level, api.frequency, api.anticheat_impact
            ));
        }
        md.push('\n');

        // Security Measures
        md.push_str("## Security Measures\n\n");
        for measure in &self.security_measures {
            md.push_str(&format!("### {}\n\n", measure.name));
            md.push_str(&format!("**Description:** {}\n\n", measure.description));
            md.push_str(&format!(
                "**Implementation:** {}\n\n",
                measure.implementation
            ));
            md.push_str(&format!("**Effectiveness:** {}\n\n", measure.effectiveness));
        }

        // Conclusion
        md.push_str("## Conclusion\n\n");
        md.push_str(
            "This software is designed with anti-cheat compatibility as a primary concern. ",
        );
        md.push_str("All game interactions use documented, legitimate methods that are commonly ");
        md.push_str("used by hardware manufacturers and racing simulation software. The software ");
        md.push_str("does not employ any techniques that would be flagged by modern anti-cheat systems.\n\n");

        md.push_str(
            "For questions or concerns, please contact the development team with this report.\n",
        );

        md
    }

    async fn collect_platform_info() -> Result<PlatformInfo> {
        let os_info = os_info::get();

        Ok(PlatformInfo {
            os: os_info.os_type().to_string(),
            os_version: os_info.version().to_string(),
            arch: std::env::consts::ARCH.to_string(),
            kernel_version: if cfg!(target_os = "linux") {
                Some(
                    Self::get_kernel_version()
                        .await
                        .unwrap_or_else(|_| "Unknown".to_string()),
                )
            } else {
                Some("Windows".to_string())
            },
        })
    }

    async fn collect_process_info() -> Result<ProcessInfo> {
        Ok(ProcessInfo {
            name: "wheeld".to_string(),
            arch: std::env::consts::ARCH.to_string(),
            privilege_level: "User".to_string(),
            parent_process: Self::get_parent_process().await,
            child_processes: vec![
                "wheel-plugin-helper".to_string(), // Plugin helper process
            ],
            dll_injection: false,   // We explicitly do not use DLL injection
            kernel_drivers: vec![], // We do not use kernel drivers
        })
    }

    async fn document_telemetry_methods() -> Result<Vec<TelemetryMethod>> {
        Ok(vec![
            TelemetryMethod {
                game: "iRacing".to_string(),
                method_type: "Shared Memory".to_string(),
                description: "Reads telemetry data from iRacing's official shared memory interface".to_string(),
                implementation: "Uses iRacing SDK to access shared memory segment 'Local\\IRSDKMemMapFileName'".to_string(),
                memory_access: "Read-only access to documented shared memory structure".to_string(),
                file_access: Some("Reads app.ini configuration file to enable telemetry output".to_string()),
                network_protocol: None,
                anticheat_compatible: true,
                compatibility_notes: "Uses official iRacing SDK methods. No process injection or memory modification.".to_string(),
            },
            TelemetryMethod {
                game: "Assetto Corsa Competizione".to_string(),
                method_type: "UDP Broadcast".to_string(),
                description: "Receives telemetry data via UDP broadcast from ACC's built-in telemetry system".to_string(),
                implementation: "Listens on UDP port for broadcast packets from ACC telemetry system".to_string(),
                memory_access: "No direct memory access to game process".to_string(),
                file_access: Some("Modifies broadcasting.json to enable telemetry output".to_string()),
                network_protocol: Some("UDP broadcast on local network interface".to_string()),
                anticheat_compatible: true,
                compatibility_notes: "Uses ACC's official telemetry API. No process interaction required.".to_string(),
            },
            TelemetryMethod {
                game: "Automobilista 2".to_string(),
                method_type: "Shared Memory".to_string(),
                description: "Reads telemetry from AMS2's shared memory interface".to_string(),
                implementation: "Accesses shared memory segment created by AMS2 telemetry system".to_string(),
                memory_access: "Read-only access to documented shared memory structure".to_string(),
                file_access: None,
                network_protocol: None,
                anticheat_compatible: true,
                compatibility_notes: "Uses documented shared memory interface. No process modification.".to_string(),
            },
        ])
    }

    async fn document_file_access() -> Result<Vec<FileAccess>> {
        Ok(vec![
            FileAccess {
                path_pattern: "%LOCALAPPDATA%/wheel/*".to_string(),
                access_type: "Read/Write".to_string(),
                purpose: "Configuration and profile storage".to_string(),
                frequency: "On startup and configuration changes".to_string(),
                user_consent: false,
            },
            FileAccess {
                path_pattern: "Documents/iRacing/app.ini".to_string(),
                access_type: "Read/Write".to_string(),
                purpose: "Enable iRacing telemetry output".to_string(),
                frequency: "Only when user requests auto-configuration".to_string(),
                user_consent: true,
            },
            FileAccess {
                path_pattern: "Documents/Assetto Corsa Competizione/Config/broadcasting.json"
                    .to_string(),
                access_type: "Read/Write".to_string(),
                purpose: "Enable ACC telemetry broadcast".to_string(),
                frequency: "Only when user requests auto-configuration".to_string(),
                user_consent: true,
            },
            FileAccess {
                path_pattern: "/dev/hidraw*".to_string(),
                access_type: "Read/Write".to_string(),
                purpose: "Racing wheel hardware communication".to_string(),
                frequency: "Continuous during operation".to_string(),
                user_consent: false,
            },
        ])
    }

    async fn document_network_access() -> Result<Vec<NetworkAccess>> {
        Ok(vec![NetworkAccess {
            protocol: "UDP".to_string(),
            direction: "Inbound".to_string(),
            purpose: "Receive game telemetry data".to_string(),
            endpoints: vec![
                "localhost:9000".to_string(),
                "localhost:9996".to_string(),
                "localhost:20777".to_string(),
            ],
            data_transmitted: "Game telemetry data (RPM, speed, etc.)".to_string(),
            user_consent: false,
        }])
    }

    async fn document_system_apis() -> Result<Vec<SystemApi>> {
        let mut apis = vec![
            SystemApi {
                api_name: "HID API".to_string(),
                purpose: "Racing wheel hardware communication".to_string(),
                privilege_level: "User".to_string(),
                frequency: "Continuous".to_string(),
                anticheat_impact: "None - standard hardware interface".to_string(),
            },
            SystemApi {
                api_name: "Shared Memory".to_string(),
                purpose: "Read game telemetry data".to_string(),
                privilege_level: "User".to_string(),
                frequency: "Continuous during gaming".to_string(),
                anticheat_impact: "None - read-only access to documented interfaces".to_string(),
            },
            SystemApi {
                api_name: "File I/O".to_string(),
                purpose: "Configuration and profile management".to_string(),
                privilege_level: "User".to_string(),
                frequency: "Occasional".to_string(),
                anticheat_impact: "None - standard file operations".to_string(),
            },
        ];

        // Platform-specific APIs
        #[cfg(windows)]
        apis.extend(vec![
            SystemApi {
                api_name: "MMCSS (Multimedia Class Scheduler Service)".to_string(),
                purpose: "Real-time thread scheduling for force feedback".to_string(),
                privilege_level: "User".to_string(),
                frequency: "On RT thread creation".to_string(),
                anticheat_impact: "None - standard multimedia API".to_string(),
            },
            SystemApi {
                api_name: "Named Pipes".to_string(),
                purpose: "Inter-process communication".to_string(),
                privilege_level: "User".to_string(),
                frequency: "When UI connects to service".to_string(),
                anticheat_impact: "None - standard IPC mechanism".to_string(),
            },
        ]);

        #[cfg(target_os = "linux")]
        apis.extend(vec![
            SystemApi {
                api_name: "rtkit".to_string(),
                purpose: "Real-time scheduling for force feedback".to_string(),
                privilege_level: "User (via rtkit)".to_string(),
                frequency: "On RT thread creation".to_string(),
                anticheat_impact: "None - standard real-time API".to_string(),
            },
            SystemApi {
                api_name: "Unix Domain Sockets".to_string(),
                purpose: "Inter-process communication".to_string(),
                privilege_level: "User".to_string(),
                frequency: "When UI connects to service".to_string(),
                anticheat_impact: "None - standard IPC mechanism".to_string(),
            },
        ]);

        Ok(apis)
    }

    async fn document_security_measures() -> Result<Vec<SecurityMeasure>> {
        Ok(vec![
            SecurityMeasure {
                name: "Code Signing".to_string(),
                description: "All executables are digitally signed".to_string(),
                implementation: "Authenticode signatures on Windows, GPG signatures on Linux"
                    .to_string(),
                effectiveness: "Prevents tampering and establishes authenticity".to_string(),
            },
            SecurityMeasure {
                name: "Process Isolation".to_string(),
                description: "Separate processes for different components".to_string(),
                implementation: "Service, UI, and plugin helper run as separate processes"
                    .to_string(),
                effectiveness: "Limits blast radius of potential vulnerabilities".to_string(),
            },
            SecurityMeasure {
                name: "Privilege Separation".to_string(),
                description: "Runs with minimal required privileges".to_string(),
                implementation: "User-level service, no admin rights required at runtime"
                    .to_string(),
                effectiveness: "Reduces attack surface and system impact".to_string(),
            },
            SecurityMeasure {
                name: "Input Validation".to_string(),
                description: "All external inputs are validated".to_string(),
                implementation: "Schema validation for configs, bounds checking for telemetry"
                    .to_string(),
                effectiveness: "Prevents injection attacks and data corruption".to_string(),
            },
            SecurityMeasure {
                name: "Memory Safety".to_string(),
                description: "Written in Rust for memory safety".to_string(),
                implementation:
                    "Rust's ownership system prevents buffer overflows and use-after-free"
                        .to_string(),
                effectiveness: "Eliminates entire classes of security vulnerabilities".to_string(),
            },
        ])
    }

    #[cfg(target_os = "linux")]
    async fn get_kernel_version() -> Result<String> {
        let output = tokio::process::Command::new("uname")
            .arg("-r")
            .output()
            .await?;

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    #[cfg(not(target_os = "linux"))]
    async fn get_kernel_version() -> Result<String> {
        Ok("Windows".to_string())
    }

    async fn get_parent_process() -> Option<String> {
        // In a real implementation, we would query the parent process
        // For now, return a typical parent process
        Some("systemd".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    fn sample_report() -> AntiCheatReport {
        AntiCheatReport {
            generated_at: "2025-01-01T00:00:00Z".to_string(),
            version: "0.1.0".to_string(),
            platform: PlatformInfo {
                os: "Windows".to_string(),
                os_version: "10.0".to_string(),
                arch: "x86_64".to_string(),
                kernel_version: None,
            },
            process_info: ProcessInfo {
                name: "wheeld".to_string(),
                arch: "x86_64".to_string(),
                privilege_level: "User".to_string(),
                parent_process: Some("explorer.exe".to_string()),
                child_processes: vec!["wheel-plugin-helper".to_string()],
                dll_injection: false,
                kernel_drivers: vec![],
            },
            telemetry_methods: vec![TelemetryMethod {
                game: "iRacing".to_string(),
                method_type: "Shared Memory".to_string(),
                description: "Reads telemetry".to_string(),
                implementation: "Official SDK".to_string(),
                memory_access: "Read-only".to_string(),
                file_access: Some("app.ini".to_string()),
                network_protocol: None,
                anticheat_compatible: true,
                compatibility_notes: "Official methods".to_string(),
            }],
            file_access: vec![FileAccess {
                path_pattern: "%LOCALAPPDATA%/wheel/*".to_string(),
                access_type: "Read/Write".to_string(),
                purpose: "Config storage".to_string(),
                frequency: "On startup".to_string(),
                user_consent: false,
            }],
            network_access: vec![NetworkAccess {
                protocol: "UDP".to_string(),
                direction: "Inbound".to_string(),
                purpose: "Telemetry".to_string(),
                endpoints: vec!["localhost:9000".to_string()],
                data_transmitted: "Telemetry data".to_string(),
                user_consent: false,
            }],
            system_apis: vec![SystemApi {
                api_name: "HID API".to_string(),
                purpose: "Hardware communication".to_string(),
                privilege_level: "User".to_string(),
                frequency: "Continuous".to_string(),
                anticheat_impact: "None".to_string(),
            }],
            security_measures: vec![SecurityMeasure {
                name: "Code Signing".to_string(),
                description: "Signed binaries".to_string(),
                implementation: "Authenticode".to_string(),
                effectiveness: "Prevents tampering".to_string(),
            }],
        }
    }

    #[test]
    fn test_report_to_markdown_contains_header() -> Result<()> {
        let report = sample_report();
        let md = report.to_markdown();
        assert!(
            md.contains("# Racing Wheel Software - Anti-Cheat Compatibility Report"),
            "Markdown should contain the report header"
        );
        Ok(())
    }

    #[test]
    fn test_report_to_markdown_contains_version() -> Result<()> {
        let report = sample_report();
        let md = report.to_markdown();
        assert!(
            md.contains("**Version:** 0.1.0"),
            "Markdown should contain the version"
        );
        assert!(
            md.contains("**Generated:** 2025-01-01T00:00:00Z"),
            "Markdown should contain the timestamp"
        );
        Ok(())
    }

    #[test]
    fn test_report_no_dll_injection_marker() -> Result<()> {
        let report = sample_report();
        let md = report.to_markdown();
        assert!(md.contains("✅ No"), "Report should show no DLL injection");
        assert!(
            md.contains("✅ None"),
            "Report should show no kernel drivers"
        );
        Ok(())
    }

    #[test]
    fn test_report_dll_injection_true() -> Result<()> {
        let mut report = sample_report();
        report.process_info.dll_injection = true;
        let md = report.to_markdown();
        assert!(md.contains("❌ Yes"), "Report should flag DLL injection");
        Ok(())
    }

    #[test]
    fn test_report_includes_telemetry_methods() -> Result<()> {
        let report = sample_report();
        let md = report.to_markdown();
        assert!(
            md.contains("iRacing - Shared Memory"),
            "Markdown should contain telemetry method heading"
        );
        assert!(
            md.contains("✅ Yes"),
            "Compatible method should show checkmark"
        );
        Ok(())
    }

    #[test]
    fn test_report_includes_network_section() -> Result<()> {
        let report = sample_report();
        let md = report.to_markdown();
        assert!(
            md.contains("## Network Access"),
            "Markdown should include network access section"
        );
        assert!(
            md.contains("localhost:9000"),
            "Markdown should list network endpoints"
        );
        Ok(())
    }

    #[test]
    fn test_report_empty_network_omits_section() -> Result<()> {
        let mut report = sample_report();
        report.network_access.clear();
        let md = report.to_markdown();
        assert!(
            !md.contains("## Network Access"),
            "Empty network access should omit the section"
        );
        Ok(())
    }

    #[test]
    fn test_report_serialization_roundtrip() -> Result<()> {
        let report = sample_report();
        let json = serde_json::to_string(&report)?;
        let deserialized: AntiCheatReport = serde_json::from_str(&json)?;
        assert_eq!(deserialized.version, report.version);
        assert_eq!(deserialized.platform.os, report.platform.os);
        assert_eq!(
            deserialized.process_info.dll_injection,
            report.process_info.dll_injection
        );
        assert_eq!(
            deserialized.telemetry_methods.len(),
            report.telemetry_methods.len()
        );
        Ok(())
    }

    #[test]
    fn test_process_info_defaults_safe() -> Result<()> {
        let report = sample_report();
        assert!(
            !report.process_info.dll_injection,
            "Default process info should not use DLL injection"
        );
        assert!(
            report.process_info.kernel_drivers.is_empty(),
            "Default process info should have no kernel drivers"
        );
        assert_eq!(report.process_info.privilege_level, "User");
        Ok(())
    }

    #[test]
    fn test_report_includes_security_measures() -> Result<()> {
        let report = sample_report();
        let md = report.to_markdown();
        assert!(
            md.contains("## Security Measures"),
            "Markdown should contain security measures section"
        );
        assert!(
            md.contains("### Code Signing"),
            "Should list code signing measure"
        );
        Ok(())
    }
}
