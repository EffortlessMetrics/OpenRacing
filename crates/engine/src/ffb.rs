//! Force Feedback Mode Matrix and Capability Negotiation
//!
//! This module implements the three FFB modes and device capability negotiation
//! as specified in the design document.

use racing_wheel_schemas::prelude::*;
use std::fmt;

// Frame struct is now exported from rt module to avoid duplication

/// Internal pipeline operating modes (use rt::FFBMode for public API)
/// TODO: Used for future pipeline mode switching implementation
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PipelineMode {
    /// PID pass-through mode - Game emits DirectInput/PID effects
    PidPassthrough,

    /// Raw torque mode - Host synthesizes torque at 1kHz (preferred)
    RawTorque1kHz,

    /// Telemetry synthesis mode - Host computes torque from game telemetry (fallback)
    TelemetrySynth,
}

impl fmt::Display for PipelineMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PipelineMode::PidPassthrough => write!(f, "PID Pass-through"),
            PipelineMode::RawTorque1kHz => write!(f, "Raw Torque @1kHz"),
            PipelineMode::TelemetrySynth => write!(f, "Telemetry Synthesis"),
        }
    }
}

/// Game compatibility information for mode selection
#[derive(Debug, Clone)]
pub struct GameCompatibility {
    pub game_id: String,
    pub supports_robust_ffb: bool,
    pub supports_telemetry: bool,
    pub preferred_mode: crate::rt::FFBMode,
}

/// Mode selection policy based on device capabilities and game compatibility
pub struct ModeSelectionPolicy;

impl ModeSelectionPolicy {
    /// Select the best FFB mode based on device capabilities and game compatibility
    pub fn select_mode(
        device_caps: &DeviceCapabilities,
        game_compat: Option<&GameCompatibility>,
    ) -> crate::rt::FFBMode {
        // Priority order: Raw Torque > PID Pass-through > Telemetry Synthesis

        // Check if device supports raw torque at 1kHz (preferred)
        if device_caps.supports_raw_torque_1khz {
            // If we have game compatibility info, respect its preference
            if let Some(game) = game_compat {
                if game.supports_robust_ffb {
                    return crate::rt::FFBMode::RawTorque;
                }
                // Fall back to telemetry synthesis for arcade/console ports
                if game.supports_telemetry {
                    return crate::rt::FFBMode::TelemetrySynth;
                }
            }
            // Default to raw torque if no game info or game supports robust FFB
            return crate::rt::FFBMode::RawTorque;
        }

        // Fall back to PID pass-through for commodity wheels
        if device_caps.supports_pid {
            return crate::rt::FFBMode::PidPassthrough;
        }

        // Last resort: telemetry synthesis
        crate::rt::FFBMode::TelemetrySynth
    }

    /// Check if the selected mode is compatible with the device
    pub fn is_mode_compatible(mode: crate::rt::FFBMode, device_caps: &DeviceCapabilities) -> bool {
        match mode {
            crate::rt::FFBMode::PidPassthrough => device_caps.supports_pid,
            crate::rt::FFBMode::RawTorque => device_caps.supports_raw_torque_1khz,
            crate::rt::FFBMode::TelemetrySynth => true, // Always supported as fallback
        }
    }

    /// Get the update rate for a given mode
    pub fn get_update_rate_hz(mode: crate::rt::FFBMode) -> f32 {
        match mode {
            crate::rt::FFBMode::PidPassthrough => 60.0, // Typical PID update rate
            crate::rt::FFBMode::RawTorque => 1000.0,    // 1kHz raw torque
            crate::rt::FFBMode::TelemetrySynth => 60.0, // Limited by telemetry rate
        }
    }
}

/// Device capability negotiation on connect
pub struct CapabilityNegotiator;

impl CapabilityNegotiator {
    /// Parse device capabilities from Feature Report 0x01
    pub fn parse_capabilities_report(report: &[u8]) -> Result<DeviceCapabilities, String> {
        if report.len() < 8 {
            return Err("Capabilities report too short".to_string());
        }

        if report[0] != 0x01 {
            return Err(format!(
                "Invalid report ID: expected 0x01, got 0x{:02x}",
                report[0]
            ));
        }

        let supports_pid = (report[1] & 0x01) != 0;
        let supports_raw_torque_1khz = (report[1] & 0x02) != 0;
        let supports_health_stream = (report[1] & 0x04) != 0;
        let supports_led_bus = (report[1] & 0x08) != 0;

        let max_torque_cnm = u16::from_le_bytes([report[2], report[3]]);
        let max_torque =
            TorqueNm::from_cnm(max_torque_cnm).map_err(|e| format!("Invalid max torque: {}", e))?;

        let encoder_cpr = u16::from_le_bytes([report[4], report[5]]);
        let min_report_period_us = u16::from_le_bytes([report[6], report[7]]);

        Ok(DeviceCapabilities::new(
            supports_pid,
            supports_raw_torque_1khz,
            supports_health_stream,
            supports_led_bus,
            max_torque,
            encoder_cpr,
            min_report_period_us,
        ))
    }

    /// Create capabilities report for virtual devices
    pub fn create_capabilities_report(caps: &DeviceCapabilities) -> Vec<u8> {
        let mut report = vec![0u8; 8];

        report[0] = 0x01; // Report ID

        // Pack capability flags
        let mut flags = 0u8;
        if caps.supports_pid {
            flags |= 0x01;
        }
        if caps.supports_raw_torque_1khz {
            flags |= 0x02;
        }
        if caps.supports_health_stream {
            flags |= 0x04;
        }
        if caps.supports_led_bus {
            flags |= 0x08;
        }
        report[1] = flags;

        // Max torque in centi-Newton-meters
        let max_torque_cnm = caps.max_torque.to_cnm();
        report[2..4].copy_from_slice(&max_torque_cnm.to_le_bytes());

        // Encoder CPR
        report[4..6].copy_from_slice(&caps.encoder_cpr.to_le_bytes());

        // Min report period
        report[6..8].copy_from_slice(&caps.min_report_period_us.to_le_bytes());

        report
    }

    /// Negotiate capabilities with a device
    pub fn negotiate_capabilities(
        device_caps: &DeviceCapabilities,
        game_compat: Option<&GameCompatibility>,
    ) -> NegotiationResult {
        let selected_mode = ModeSelectionPolicy::select_mode(device_caps, game_compat);
        let update_rate = ModeSelectionPolicy::get_update_rate_hz(selected_mode);

        // Validate that the device can actually support the selected mode
        if !ModeSelectionPolicy::is_mode_compatible(selected_mode, device_caps) {
            return NegotiationResult {
                mode: crate::rt::FFBMode::TelemetrySynth, // Fallback
                update_rate_hz: 60.0,
                warnings: vec![
                    "Device does not support preferred mode, falling back to telemetry synthesis"
                        .to_string(),
                ],
            };
        }

        let mut warnings = Vec::new();

        // Check if device can actually achieve the target update rate
        let device_max_rate = device_caps.max_update_rate_hz();
        if update_rate > device_max_rate {
            warnings.push(format!(
                "Device max rate {:.0}Hz is lower than mode requirement {:.0}Hz",
                device_max_rate, update_rate
            ));
        }

        // Warn about potential issues
        if selected_mode == crate::rt::FFBMode::TelemetrySynth {
            warnings
                .push("Using telemetry synthesis mode - FFB quality may be reduced".to_string());
        }

        NegotiationResult {
            mode: selected_mode,
            update_rate_hz: update_rate.min(device_max_rate),
            warnings,
        }
    }
}

/// Result of capability negotiation
#[derive(Debug, Clone)]
pub struct NegotiationResult {
    pub mode: crate::rt::FFBMode,
    pub update_rate_hz: f32,
    pub warnings: Vec<String>,
}

impl NegotiationResult {
    /// Check if negotiation was successful without warnings
    pub fn is_optimal(&self) -> bool {
        self.warnings.is_empty() && self.mode == crate::rt::FFBMode::RawTorque
    }

    /// Get a human-readable summary of the negotiation
    pub fn summary(&self) -> String {
        let mut summary = format!("Mode: {} @ {:.0}Hz", self.mode, self.update_rate_hz);

        if !self.warnings.is_empty() {
            summary.push_str("\nWarnings:");
            for warning in &self.warnings {
                summary.push_str(&format!("\n  - {}", warning));
            }
        }

        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_capabilities(
        supports_pid: bool,
        supports_raw_torque: bool,
        max_torque_nm: f32,
    ) -> DeviceCapabilities {
        DeviceCapabilities::new(
            supports_pid,
            supports_raw_torque,
            true, // supports_health_stream
            true, // supports_led_bus
            TorqueNm::from_raw(max_torque_nm),
            10000, // encoder_cpr
            1000,  // min_report_period_us (1kHz)
        )
    }

    #[test]
    fn test_mode_selection_raw_torque_preferred() {
        let caps = create_test_capabilities(true, true, 25.0);
        let mode = ModeSelectionPolicy::select_mode(&caps, None);
        assert_eq!(mode, crate::rt::FFBMode::RawTorque);
    }

    #[test]
    fn test_mode_selection_pid_fallback() {
        let caps = create_test_capabilities(true, false, 15.0);
        let mode = ModeSelectionPolicy::select_mode(&caps, None);
        assert_eq!(mode, crate::rt::FFBMode::PidPassthrough);
    }

    #[test]
    fn test_mode_selection_telemetry_fallback() {
        let caps = create_test_capabilities(false, false, 10.0);
        let mode = ModeSelectionPolicy::select_mode(&caps, None);
        assert_eq!(mode, crate::rt::FFBMode::TelemetrySynth);
    }

    #[test]
    fn test_mode_selection_with_game_compatibility() {
        let caps = create_test_capabilities(true, true, 25.0);
        let game_compat = GameCompatibility {
            game_id: "arcade-racer".to_string(),
            supports_robust_ffb: false,
            supports_telemetry: true,
            preferred_mode: crate::rt::FFBMode::TelemetrySynth,
        };

        let mode = ModeSelectionPolicy::select_mode(&caps, Some(&game_compat));
        assert_eq!(mode, crate::rt::FFBMode::TelemetrySynth);
    }

    #[test]
    fn test_mode_compatibility_check() {
        let caps = create_test_capabilities(true, true, 25.0);

        assert!(ModeSelectionPolicy::is_mode_compatible(
            crate::rt::FFBMode::PidPassthrough,
            &caps
        ));
        assert!(ModeSelectionPolicy::is_mode_compatible(
            crate::rt::FFBMode::RawTorque,
            &caps
        ));
        assert!(ModeSelectionPolicy::is_mode_compatible(
            crate::rt::FFBMode::TelemetrySynth,
            &caps
        ));

        let limited_caps = create_test_capabilities(false, true, 25.0);
        assert!(!ModeSelectionPolicy::is_mode_compatible(
            crate::rt::FFBMode::PidPassthrough,
            &limited_caps
        ));
        assert!(ModeSelectionPolicy::is_mode_compatible(
            crate::rt::FFBMode::RawTorque,
            &limited_caps
        ));
    }

    #[test]
    fn test_update_rates() {
        assert_eq!(
            ModeSelectionPolicy::get_update_rate_hz(crate::rt::FFBMode::PidPassthrough),
            60.0
        );
        assert_eq!(
            ModeSelectionPolicy::get_update_rate_hz(crate::rt::FFBMode::RawTorque),
            1000.0
        );
        assert_eq!(
            ModeSelectionPolicy::get_update_rate_hz(crate::rt::FFBMode::TelemetrySynth),
            60.0
        );
    }

    #[test]
    fn test_capabilities_report_parsing() {
        let report = vec![
            0x01, // Report ID
            0x0F, // All flags set
            0xC4, 0x09, // 2500 cNm (25.0 Nm)
            0x10, 0x27, // 10000 CPR
            0xE8, 0x03, // 1000 us (1kHz)
        ];

        let caps = CapabilityNegotiator::parse_capabilities_report(&report).unwrap();

        assert!(caps.supports_pid);
        assert!(caps.supports_raw_torque_1khz);
        assert!(caps.supports_health_stream);
        assert!(caps.supports_led_bus);
        assert_eq!(caps.max_torque.value(), 25.0);
        assert_eq!(caps.encoder_cpr, 10000);
        assert_eq!(caps.min_report_period_us, 1000);
    }

    #[test]
    fn test_capabilities_report_creation() {
        let caps = create_test_capabilities(true, true, 25.0);
        let report = CapabilityNegotiator::create_capabilities_report(&caps);

        assert_eq!(report[0], 0x01); // Report ID
        assert_eq!(report[1] & 0x0F, 0x0F); // All flags set

        let max_torque_cnm = u16::from_le_bytes([report[2], report[3]]);
        assert_eq!(max_torque_cnm, 2500); // 25.0 Nm
    }

    #[test]
    fn test_capability_negotiation() {
        let caps = create_test_capabilities(true, true, 25.0);
        let result = CapabilityNegotiator::negotiate_capabilities(&caps, None);

        assert_eq!(result.mode, crate::rt::FFBMode::RawTorque);
        assert_eq!(result.update_rate_hz, 1000.0);
        assert!(result.is_optimal());
    }

    #[test]
    fn test_capability_negotiation_with_warnings() {
        let caps = create_test_capabilities(false, false, 10.0);
        let result = CapabilityNegotiator::negotiate_capabilities(&caps, None);

        assert_eq!(result.mode, crate::rt::FFBMode::TelemetrySynth);
        assert!(!result.is_optimal());
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_negotiation_result_summary() {
        let result = NegotiationResult {
            mode: crate::rt::FFBMode::RawTorque,
            update_rate_hz: 1000.0,
            warnings: vec!["Test warning".to_string()],
        };

        let summary = result.summary();
        assert!(summary.contains("RawTorque"));
        assert!(summary.contains("1000Hz"));
        assert!(summary.contains("Test warning"));
    }
}
