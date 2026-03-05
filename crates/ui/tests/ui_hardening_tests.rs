//! UI hardening tests for racing-wheel-ui.
//!
//! Covers: safety display rendering, status display rendering, diagnostic
//! display, theme/styling, data formatting, error sanitisation, consent
//! dialog state machine, and safety banner behaviour.

use racing_wheel_engine::safety::{ButtonCombo, ConsentRequirements, InterlockChallenge};
use racing_wheel_ui::commands::*;
use racing_wheel_ui::error::*;
use racing_wheel_ui::safety::*;
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Helper to build an InterlockChallenge with the actual struct fields.
fn make_challenge(token: u32, secs: u64) -> InterlockChallenge {
    InterlockChallenge {
        challenge_token: token,
        combo_required: ButtonCombo::BothClutchPaddles,
        expires: Instant::now() + Duration::from_secs(secs),
        ui_consent_given: false,
        combo_start: None,
    }
}

// ===========================================================================
// 1. ConsentDialog — full state machine
// ===========================================================================

mod consent_dialog_state_machine {
    use super::*;

    fn test_requirements() -> ConsentRequirements {
        ConsentRequirements {
            max_torque_nm: 25.0,
            warnings: vec![
                "High torque can cause injury".to_string(),
                "Ensure emergency stop is accessible".to_string(),
            ],
            disclaimers: vec!["User assumes all risk".to_string()],
            requires_explicit_consent: true,
        }
    }

    #[test]
    fn initial_state_is_show_warnings() -> TestResult {
        let dialog = ConsentDialog::new(test_requirements());
        assert_eq!(dialog.state().step, ConsentStep::ShowWarnings);
        assert!(!dialog.is_complete());
        assert!(!dialog.is_consent_complete());
        Ok(())
    }

    #[test]
    fn acknowledge_warnings_only_stays_in_show_warnings() -> TestResult {
        let mut dialog = ConsentDialog::new(test_requirements());
        dialog.acknowledge_warnings().map_err(|e| e.to_string())?;
        assert_eq!(dialog.state().step, ConsentStep::ShowWarnings);
        Ok(())
    }

    #[test]
    fn acknowledge_disclaimers_only_stays_in_show_warnings() -> TestResult {
        let mut dialog = ConsentDialog::new(test_requirements());
        dialog
            .acknowledge_disclaimers()
            .map_err(|e| e.to_string())?;
        assert_eq!(dialog.state().step, ConsentStep::ShowWarnings);
        Ok(())
    }

    #[test]
    fn both_ack_advances_to_require_consent() -> TestResult {
        let mut dialog = ConsentDialog::new(test_requirements());
        dialog.acknowledge_warnings().map_err(|e| e.to_string())?;
        dialog
            .acknowledge_disclaimers()
            .map_err(|e| e.to_string())?;
        assert_eq!(dialog.state().step, ConsentStep::RequireConsent);
        Ok(())
    }

    #[test]
    fn ack_order_does_not_matter() -> TestResult {
        let mut dialog = ConsentDialog::new(test_requirements());
        // Disclaimers first, then warnings
        dialog
            .acknowledge_disclaimers()
            .map_err(|e| e.to_string())?;
        dialog.acknowledge_warnings().map_err(|e| e.to_string())?;
        assert_eq!(dialog.state().step, ConsentStep::RequireConsent);
        Ok(())
    }

    #[test]
    fn provide_consent_succeeds_after_both_ack() -> TestResult {
        let mut dialog = ConsentDialog::new(test_requirements());
        dialog.acknowledge_warnings().map_err(|e| e.to_string())?;
        dialog
            .acknowledge_disclaimers()
            .map_err(|e| e.to_string())?;
        dialog.provide_consent().map_err(|e| e.to_string())?;
        assert!(dialog.is_consent_complete());
        Ok(())
    }

    #[test]
    fn provide_consent_fails_in_wrong_step() {
        let mut dialog = ConsentDialog::new(test_requirements());
        let result = dialog.provide_consent();
        assert!(result.is_err());
    }

    #[test]
    fn acknowledge_warnings_fails_after_advancing() {
        let mut dialog = ConsentDialog::new(test_requirements());
        let _ = dialog.acknowledge_warnings();
        let _ = dialog.acknowledge_disclaimers();
        // Now in RequireConsent step — cannot re-ack warnings
        let result = dialog.acknowledge_warnings();
        assert!(result.is_err());
    }

    #[test]
    fn physical_ack_requires_consent_first() {
        let mut dialog = ConsentDialog::new(test_requirements());
        let challenge = make_challenge(42, 30);
        let result = dialog.start_physical_ack(challenge);
        assert!(result.is_err());
    }

    #[test]
    fn physical_ack_transitions_to_awaiting() -> TestResult {
        let mut dialog = ConsentDialog::new(test_requirements());
        dialog.acknowledge_warnings().map_err(|e| e.to_string())?;
        dialog
            .acknowledge_disclaimers()
            .map_err(|e| e.to_string())?;
        dialog.provide_consent().map_err(|e| e.to_string())?;

        let challenge = make_challenge(42, 30);
        dialog
            .start_physical_ack(challenge)
            .map_err(|e| e.to_string())?;
        assert_eq!(dialog.state().step, ConsentStep::AwaitingPhysicalAck);
        assert!(dialog.state().challenge.is_some());
        assert!(dialog.state().time_remaining.is_some());
        Ok(())
    }

    #[test]
    fn mark_activated_sets_final_state() -> TestResult {
        let mut dialog = ConsentDialog::new(test_requirements());
        dialog.acknowledge_warnings().map_err(|e| e.to_string())?;
        dialog
            .acknowledge_disclaimers()
            .map_err(|e| e.to_string())?;
        dialog.provide_consent().map_err(|e| e.to_string())?;

        let challenge = make_challenge(99, 30);
        dialog
            .start_physical_ack(challenge)
            .map_err(|e| e.to_string())?;
        dialog.mark_activated();

        assert_eq!(dialog.state().step, ConsentStep::Activated);
        assert!(dialog.is_complete());
        assert!(dialog.state().challenge.is_none());
        assert!(dialog.state().time_remaining.is_none());
        assert!(dialog.state().error_message.is_none());
        Ok(())
    }

    #[test]
    fn mark_failed_sets_error() {
        let mut dialog = ConsentDialog::new(test_requirements());
        dialog.mark_failed("Hardware fault".to_string());
        assert!(dialog.is_complete());
        assert!(matches!(dialog.state().step, ConsentStep::Failed { .. }));
        assert_eq!(
            dialog.state().error_message.as_deref(),
            Some("Hardware fault")
        );
    }

    #[test]
    fn cancel_sets_cancelled_state() {
        let mut dialog = ConsentDialog::new(test_requirements());
        dialog.cancel();
        assert!(dialog.is_complete());
        assert!(matches!(dialog.state().step, ConsentStep::Failed { .. }));
        assert!(
            dialog
                .state()
                .error_message
                .as_ref()
                .map(|m| m.contains("cancelled"))
                .unwrap_or(false)
        );
    }

    #[test]
    fn expired_challenge_transitions_to_failed() -> TestResult {
        let mut dialog = ConsentDialog::new(test_requirements());
        dialog.acknowledge_warnings().map_err(|e| e.to_string())?;
        dialog
            .acknowledge_disclaimers()
            .map_err(|e| e.to_string())?;
        dialog.provide_consent().map_err(|e| e.to_string())?;

        let challenge = make_challenge(1, 30);
        dialog
            .start_physical_ack(challenge)
            .map_err(|e| e.to_string())?;

        // Simulate expiry
        dialog.update_time_remaining(Duration::ZERO);
        assert!(matches!(dialog.state().step, ConsentStep::Failed { .. }));
        assert!(
            dialog
                .state()
                .error_message
                .as_ref()
                .map(|m| m.contains("expired"))
                .unwrap_or(false)
        );
        Ok(())
    }

    #[test]
    fn update_time_remaining_positive_keeps_awaiting() -> TestResult {
        let mut dialog = ConsentDialog::new(test_requirements());
        dialog.acknowledge_warnings().map_err(|e| e.to_string())?;
        dialog
            .acknowledge_disclaimers()
            .map_err(|e| e.to_string())?;
        dialog.provide_consent().map_err(|e| e.to_string())?;

        let challenge = make_challenge(1, 30);
        dialog
            .start_physical_ack(challenge)
            .map_err(|e| e.to_string())?;

        dialog.update_time_remaining(Duration::from_secs(15));
        assert_eq!(dialog.state().step, ConsentStep::AwaitingPhysicalAck);
        assert_eq!(dialog.state().time_remaining, Some(Duration::from_secs(15)));
        Ok(())
    }
}

// ===========================================================================
// 2. ComboInstructions rendering
// ===========================================================================

mod combo_instructions {
    use super::*;

    #[test]
    fn both_clutch_paddles_has_instructions() {
        let instr =
            ComboInstructions::for_combo(ButtonCombo::BothClutchPaddles, Duration::from_secs(3));
        assert!(!instr.instructions.is_empty());
        assert!((instr.hold_duration_secs - 3.0).abs() < f32::EPSILON);
        assert!(instr.visual_aid.is_some());
    }

    #[test]
    fn custom_sequence_has_instructions() {
        let instr =
            ComboInstructions::for_combo(ButtonCombo::CustomSequence(42), Duration::from_secs(5));
        assert!(!instr.instructions.is_empty());
        assert!((instr.hold_duration_secs - 5.0).abs() < f32::EPSILON);
        // Custom sequences may have no visual aid
        assert!(instr.visual_aid.is_none());
    }

    #[test]
    fn clutch_paddles_instructions_mention_hold() {
        let instr = ComboInstructions::for_combo(
            ButtonCombo::BothClutchPaddles,
            Duration::from_millis(1500),
        );
        let all_text = instr.instructions.join(" ");
        assert!(
            all_text.to_lowercase().contains("hold"),
            "Clutch paddle instructions should mention 'hold'"
        );
    }

    #[test]
    fn clutch_paddles_instructions_mention_duration() {
        let instr =
            ComboInstructions::for_combo(ButtonCombo::BothClutchPaddles, Duration::from_secs(2));
        let all_text = instr.instructions.join(" ");
        assert!(
            all_text.contains("2.0"),
            "Instructions should mention the hold duration"
        );
    }
}

// ===========================================================================
// 3. SafetyBanner behaviour
// ===========================================================================

mod safety_banner_tests {
    use super::*;

    fn make_banner() -> SafetyBanner {
        SafetyBanner::new(20.0, ButtonCombo::BothClutchPaddles)
    }

    #[test]
    fn new_banner_is_inactive() {
        let banner = make_banner();
        assert!(!banner.active);
        assert!((banner.current_torque_nm - 0.0).abs() < f32::EPSILON);
        assert!((banner.max_torque_nm - 20.0).abs() < f32::EPSILON);
    }

    #[test]
    fn activate_sets_active() {
        let mut banner = make_banner();
        banner.activate();
        assert!(banner.active);
        assert_eq!(banner.time_active, Duration::ZERO);
    }

    #[test]
    fn deactivate_resets_state() {
        let mut banner = make_banner();
        banner.activate();
        banner.update_torque(10.0);
        banner.update_time_active(Duration::from_secs(60));
        banner.deactivate();
        assert!(!banner.active);
        assert!((banner.current_torque_nm - 0.0).abs() < f32::EPSILON);
        assert_eq!(banner.time_active, Duration::ZERO);
    }

    // --- Warning levels ---

    #[test]
    fn warning_level_low_below_50_pct() {
        let mut banner = make_banner();
        banner.update_torque(9.9); // 49.5% of 20
        assert_eq!(banner.get_warning_level(), WarningLevel::Low);
    }

    #[test]
    fn warning_level_medium_above_50_pct() {
        let mut banner = make_banner();
        banner.update_torque(11.0); // 55% of 20
        assert_eq!(banner.get_warning_level(), WarningLevel::Medium);
    }

    #[test]
    fn warning_level_high_above_70_pct() {
        let mut banner = make_banner();
        banner.update_torque(15.0); // 75% of 20
        assert_eq!(banner.get_warning_level(), WarningLevel::High);
    }

    #[test]
    fn warning_level_critical_above_90_pct() {
        let mut banner = make_banner();
        banner.update_torque(19.0); // 95% of 20
        assert_eq!(banner.get_warning_level(), WarningLevel::Critical);
    }

    #[test]
    fn warning_level_at_exact_boundaries() {
        let mut banner = SafetyBanner::new(100.0, ButtonCombo::BothClutchPaddles);

        // At exactly 50% (not > 0.5, so Low)
        banner.update_torque(50.0);
        assert_eq!(banner.get_warning_level(), WarningLevel::Low);

        // At exactly 70% (> 0.5 but not > 0.7, so Medium)
        banner.update_torque(70.0);
        assert_eq!(banner.get_warning_level(), WarningLevel::Medium);

        // Just above 70%
        banner.update_torque(70.1);
        assert_eq!(banner.get_warning_level(), WarningLevel::High);

        // At exactly 90% (> 0.7 but not > 0.9, so High)
        banner.update_torque(90.0);
        assert_eq!(banner.get_warning_level(), WarningLevel::High);

        // Just above 90%
        banner.update_torque(90.1);
        assert_eq!(banner.get_warning_level(), WarningLevel::Critical);
    }

    #[test]
    fn warning_level_zero_torque() {
        let mut banner = make_banner();
        banner.update_torque(0.0);
        assert_eq!(banner.get_warning_level(), WarningLevel::Low);
    }

    // --- Hands-on detection ---

    #[test]
    fn hands_off_warning_when_false() {
        let mut banner = make_banner();
        banner.update_hands_on(Some(false));
        assert!(banner.should_show_hands_off_warning());
    }

    #[test]
    fn no_hands_off_warning_when_true() {
        let mut banner = make_banner();
        banner.update_hands_on(Some(true));
        assert!(!banner.should_show_hands_off_warning());
    }

    #[test]
    fn no_hands_off_warning_when_none() {
        let mut banner = make_banner();
        banner.update_hands_on(None);
        assert!(!banner.should_show_hands_off_warning());
    }

    #[test]
    fn update_time_active() {
        let mut banner = make_banner();
        banner.activate();
        banner.update_time_active(Duration::from_secs(120));
        assert_eq!(banner.time_active, Duration::from_secs(120));
    }
}

// ===========================================================================
// 4. WarningLevel theme / styling
// ===========================================================================

mod warning_level_styling {
    use super::*;

    #[test]
    fn each_level_has_unique_color() {
        let levels = [
            WarningLevel::Low,
            WarningLevel::Medium,
            WarningLevel::High,
            WarningLevel::Critical,
        ];
        let colors: Vec<&str> = levels.iter().map(|l| l.color()).collect();
        // All colors should be distinct
        for (i, c1) in colors.iter().enumerate() {
            for (j, c2) in colors.iter().enumerate() {
                if i != j {
                    assert_ne!(c1, c2, "Warning levels {i} and {j} share color {c1}");
                }
            }
        }
    }

    #[test]
    fn each_level_has_non_empty_text() {
        let levels = [
            WarningLevel::Low,
            WarningLevel::Medium,
            WarningLevel::High,
            WarningLevel::Critical,
        ];
        for level in &levels {
            assert!(!level.text().is_empty(), "{:?} has empty text", level);
        }
    }

    #[test]
    fn critical_text_is_uppercase() {
        let text = WarningLevel::Critical.text();
        assert_eq!(
            text,
            text.to_uppercase(),
            "Critical text should be all-caps"
        );
    }

    #[test]
    fn colors_are_valid_hex() {
        let levels = [
            WarningLevel::Low,
            WarningLevel::Medium,
            WarningLevel::High,
            WarningLevel::Critical,
        ];
        for level in &levels {
            let color = level.color();
            assert!(color.starts_with('#'), "Color should start with #: {color}");
            assert_eq!(color.len(), 7, "Hex color should be 7 chars: {color}");
            assert!(
                color[1..].chars().all(|c| c.is_ascii_hexdigit()),
                "Color should be valid hex: {color}"
            );
        }
    }

    #[test]
    fn low_is_green_family() {
        let color = WarningLevel::Low.color();
        // Green channel should be dominant (G > R in hex)
        let r = u8::from_str_radix(&color[1..3], 16).ok();
        let g = u8::from_str_radix(&color[3..5], 16).ok();
        match (r, g) {
            (Some(r_val), Some(g_val)) => {
                assert!(g_val > r_val, "Low (green) should have G > R: {color}");
            }
            _ => panic!("Failed to parse hex color: {color}"),
        }
    }

    #[test]
    fn critical_is_red_family() {
        let color = WarningLevel::Critical.color();
        let r = u8::from_str_radix(&color[1..3], 16).ok();
        let g = u8::from_str_radix(&color[3..5], 16).ok();
        match (r, g) {
            (Some(r_val), Some(g_val)) => {
                assert!(r_val > g_val, "Critical (red) should have R > G: {color}");
            }
            _ => panic!("Failed to parse hex color: {color}"),
        }
    }
}

// ===========================================================================
// 5. Error sanitisation
// ===========================================================================

mod error_sanitisation {
    use super::*;

    #[test]
    fn clean_message_passes_validation() -> TestResult {
        validate_user_error_message("Failed to connect to the wheeld service").map_err(|e| e.into())
    }

    #[test]
    fn stack_trace_detected() {
        assert!(contains_internal_details(
            "panicked at 'oh no', src/main.rs:42"
        ));
    }

    #[test]
    fn unix_path_detected() {
        assert!(contains_internal_details(
            "Error loading /home/user/.config/file"
        ));
    }

    #[test]
    fn windows_path_detected() {
        assert!(contains_internal_details(
            "Error loading C:\\Users\\Admin\\file"
        ));
    }

    #[test]
    fn hex_address_detected() {
        assert!(contains_internal_details("Segfault at 0x7fff12345678"));
    }

    #[test]
    fn rust_debug_artifacts_detected() {
        assert!(contains_internal_details("Got Some(42)"));
        assert!(contains_internal_details("Value is Ok(data)"));
        assert!(contains_internal_details("Result: Err(problem)"));
    }

    #[test]
    fn short_message_rejected() {
        let result = validate_user_error_message("Err");
        assert!(result.is_err());
    }

    #[test]
    fn very_long_message_rejected() {
        let long_msg = "A".repeat(501);
        let result = validate_user_error_message(&long_msg);
        assert!(result.is_err());
    }

    #[test]
    fn control_chars_rejected() {
        let result = validate_user_error_message("Bad message\x07with bell");
        assert!(result.is_err());
    }

    #[test]
    fn newlines_and_tabs_allowed() -> TestResult {
        validate_user_error_message("Line one\nLine two\tindented").map_err(|e| e.into())
    }

    #[test]
    fn sanitize_removes_paths() {
        let sanitized = sanitize_error_message(
            "IO error at /home/user/.cargo/registry/src/tokio/net.rs:123",
            Some("Connection failed"),
        );
        assert!(!sanitized.contains("/home/"));
        assert!(sanitized.starts_with("Connection failed"));
    }

    #[test]
    fn sanitize_removes_stack_traces() {
        let sanitized = sanitize_error_message(
            "Error occurred\nstack backtrace:\n  0: 0x7fff12345678",
            Some("Operation failed"),
        );
        assert!(!sanitized.contains("stack backtrace"));
        assert!(!sanitized.contains("0x7fff"));
    }

    #[test]
    fn sanitize_empty_error_gives_default() {
        let sanitized = sanitize_error_message("", Some("Context message"));
        assert!(!sanitized.trim().is_empty());
        assert!(sanitized.len() >= 5);
    }

    #[test]
    fn sanitize_truncates_long_messages() {
        let long_error = "A".repeat(1000);
        let sanitized = sanitize_error_message(&long_error, Some("Err"));
        assert!(sanitized.len() <= 503); // 500 + "..."
    }

    #[test]
    fn format_ipc_error_includes_operation() {
        let formatted = format_ipc_error("list devices", "connection refused");
        assert!(formatted.contains("list devices"));
    }

    #[test]
    fn format_ipc_error_is_non_empty() {
        let formatted = format_ipc_error("test", "");
        assert!(!formatted.trim().is_empty());
    }
}

// ===========================================================================
// 6. AppState and commands data types
// ===========================================================================

mod commands_data_types {
    use super::*;

    #[test]
    fn app_state_new_does_not_panic() {
        let _state = AppState::new();
        // AppState::new() should create a valid default state without panicking
    }

    #[test]
    fn app_state_default_trait() {
        let _state = AppState::default();
        // AppState implements Default without panicking
    }

    #[test]
    fn device_info_roundtrip_json() -> TestResult {
        let device = DeviceInfo {
            id: "wheel-001".to_string(),
            name: "Fanatec DD Pro".to_string(),
            device_type: "WheelBase".to_string(),
            state: "Connected".to_string(),
            capabilities: DeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: true,
                supports_health_stream: true,
                supports_led_bus: false,
                max_torque_cnm: 2500,
                encoder_cpr: 65536,
            },
        };

        let json_str = serde_json::to_string(&device)?;
        let parsed: DeviceInfo = serde_json::from_str(&json_str)?;
        assert_eq!(parsed.id, "wheel-001");
        assert_eq!(parsed.name, "Fanatec DD Pro");
        assert_eq!(parsed.device_type, "WheelBase");
        assert!(parsed.capabilities.supports_pid);
        assert!(!parsed.capabilities.supports_led_bus);
        Ok(())
    }

    #[test]
    fn device_status_roundtrip_json() -> TestResult {
        let status = DeviceStatus {
            device: DeviceInfo {
                id: "p-001".to_string(),
                name: "Pedals".to_string(),
                device_type: "Pedals".to_string(),
                state: "Connected".to_string(),
                capabilities: DeviceCapabilities {
                    supports_pid: false,
                    supports_raw_torque_1khz: false,
                    supports_health_stream: true,
                    supports_led_bus: false,
                    max_torque_cnm: 0,
                    encoder_cpr: 1024,
                },
            },
            last_seen: "2024-01-01T00:00:00Z".to_string(),
            active_faults: vec!["thermal".to_string()],
            telemetry: Some(TelemetryData {
                wheel_angle_deg: 90.0,
                wheel_speed_rad_s: 1.5,
                temperature_c: 55,
                fault_flags: 0,
                hands_on: true,
            }),
        };

        let json_str = serde_json::to_string(&status)?;
        let parsed: DeviceStatus = serde_json::from_str(&json_str)?;
        assert_eq!(parsed.device.id, "p-001");
        assert_eq!(parsed.active_faults.len(), 1);
        assert!(parsed.telemetry.is_some());
        let tel = parsed.telemetry.as_ref().ok_or("no telemetry")?;
        assert!((tel.wheel_angle_deg - 90.0).abs() < f32::EPSILON);
        assert!(tel.hands_on);
        Ok(())
    }

    #[test]
    fn telemetry_data_roundtrip_json() -> TestResult {
        let tel = TelemetryData {
            wheel_angle_deg: -45.5,
            wheel_speed_rad_s: 0.0,
            temperature_c: 72,
            fault_flags: 3,
            hands_on: false,
        };
        let json_str = serde_json::to_string(&tel)?;
        let parsed: TelemetryData = serde_json::from_str(&json_str)?;
        assert!((parsed.wheel_angle_deg - (-45.5)).abs() < f32::EPSILON);
        assert_eq!(parsed.temperature_c, 72);
        assert_eq!(parsed.fault_flags, 3);
        assert!(!parsed.hands_on);
        Ok(())
    }

    #[test]
    fn profile_info_roundtrip_json() -> TestResult {
        let profile = ProfileInfo {
            schema_version: "wheel.profile/1".to_string(),
            game: Some("iracing".to_string()),
            car: Some("gt3".to_string()),
            track: None,
            ffb_gain: 0.85,
            dor_deg: 900,
            torque_cap_nm: 12.0,
        };
        let json_str = serde_json::to_string(&profile)?;
        let parsed: ProfileInfo = serde_json::from_str(&json_str)?;
        assert_eq!(parsed.game.as_deref(), Some("iracing"));
        assert!(parsed.track.is_none());
        assert!((parsed.ffb_gain - 0.85).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn service_status_roundtrip_json() -> TestResult {
        let status = ServiceStatus {
            connected: true,
            version: "0.2.0".to_string(),
            features: vec!["device_management".to_string(), "telemetry".to_string()],
            error: None,
        };
        let json_str = serde_json::to_string(&status)?;
        let parsed: ServiceStatus = serde_json::from_str(&json_str)?;
        assert!(parsed.connected);
        assert_eq!(parsed.features.len(), 2);
        assert!(parsed.error.is_none());
        Ok(())
    }

    #[test]
    fn service_status_with_error() -> TestResult {
        let status = ServiceStatus {
            connected: false,
            version: String::new(),
            features: vec![],
            error: Some("Connection refused".to_string()),
        };
        let json_str = serde_json::to_string(&status)?;
        let parsed: ServiceStatus = serde_json::from_str(&json_str)?;
        assert!(!parsed.connected);
        assert_eq!(parsed.error.as_deref(), Some("Connection refused"));
        Ok(())
    }

    #[test]
    fn op_result_success_roundtrip() -> TestResult {
        let result = OpResult {
            success: true,
            message: "Profile applied".to_string(),
        };
        let json_str = serde_json::to_string(&result)?;
        let parsed: OpResult = serde_json::from_str(&json_str)?;
        assert!(parsed.success);
        assert_eq!(parsed.message, "Profile applied");
        Ok(())
    }

    #[test]
    fn op_result_failure_roundtrip() -> TestResult {
        let result = OpResult {
            success: false,
            message: "Device not found".to_string(),
        };
        let json_str = serde_json::to_string(&result)?;
        let parsed: OpResult = serde_json::from_str(&json_str)?;
        assert!(!parsed.success);
        Ok(())
    }
}

// ===========================================================================
// 7. ConsentFlowState serialisation
// ===========================================================================

mod consent_flow_serialisation {
    use super::*;

    #[test]
    fn consent_flow_state_serialises() -> TestResult {
        let state = ConsentFlowState {
            step: ConsentStep::ShowWarnings,
            requirements: ConsentRequirements {
                max_torque_nm: 15.0,
                warnings: vec!["Warning 1".to_string()],
                disclaimers: vec!["Disclaimer 1".to_string()],
                requires_explicit_consent: true,
            },
            challenge: None,
            time_remaining: None,
            error_message: None,
        };
        let json = serde_json::to_string(&state)?;
        assert!(json.contains("ShowWarnings"));
        Ok(())
    }

    #[test]
    fn consent_step_variants_serialise() -> TestResult {
        let steps = vec![
            ConsentStep::ShowWarnings,
            ConsentStep::RequireConsent,
            ConsentStep::AwaitingPhysicalAck,
            ConsentStep::Activated,
            ConsentStep::Failed {
                reason: "timeout".to_string(),
            },
        ];
        for step in &steps {
            let json = serde_json::to_string(step)?;
            assert!(!json.is_empty(), "ConsentStep should serialise: {:?}", step);
        }
        Ok(())
    }

    #[test]
    fn warning_level_serialises() -> TestResult {
        let levels = [
            WarningLevel::Low,
            WarningLevel::Medium,
            WarningLevel::High,
            WarningLevel::Critical,
        ];
        for level in &levels {
            let json = serde_json::to_string(level)?;
            let parsed: WarningLevel = serde_json::from_str(&json)?;
            assert_eq!(*level, parsed);
        }
        Ok(())
    }

    #[test]
    fn safety_banner_serialises() -> TestResult {
        let banner = SafetyBanner::new(25.0, ButtonCombo::BothClutchPaddles);
        let json = serde_json::to_string(&banner)?;
        assert!(json.contains("25.0") || json.contains("25"));
        let parsed: SafetyBanner = serde_json::from_str(&json)?;
        assert!((parsed.max_torque_nm - 25.0).abs() < f32::EPSILON);
        assert!(!parsed.active);
        Ok(())
    }

    #[test]
    fn combo_instructions_serialises() -> TestResult {
        let instr =
            ComboInstructions::for_combo(ButtonCombo::BothClutchPaddles, Duration::from_secs(2));
        let json = serde_json::to_string(&instr)?;
        assert!(json.contains("instructions"));
        Ok(())
    }
}

// ===========================================================================
// 8. Edge cases and data formatting
// ===========================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn device_info_with_empty_fields() -> TestResult {
        let device = DeviceInfo {
            id: String::new(),
            name: String::new(),
            device_type: String::new(),
            state: String::new(),
            capabilities: DeviceCapabilities {
                supports_pid: false,
                supports_raw_torque_1khz: false,
                supports_health_stream: false,
                supports_led_bus: false,
                max_torque_cnm: 0,
                encoder_cpr: 0,
            },
        };
        let json_str = serde_json::to_string(&device)?;
        let parsed: DeviceInfo = serde_json::from_str(&json_str)?;
        assert!(parsed.id.is_empty());
        Ok(())
    }

    #[test]
    fn device_status_with_no_telemetry() -> TestResult {
        let status = DeviceStatus {
            device: DeviceInfo {
                id: "x".to_string(),
                name: "X".to_string(),
                device_type: "Unknown".to_string(),
                state: "Disconnected".to_string(),
                capabilities: DeviceCapabilities {
                    supports_pid: false,
                    supports_raw_torque_1khz: false,
                    supports_health_stream: false,
                    supports_led_bus: false,
                    max_torque_cnm: 0,
                    encoder_cpr: 0,
                },
            },
            last_seen: String::new(),
            active_faults: vec![],
            telemetry: None,
        };
        let json_str = serde_json::to_string(&status)?;
        let parsed: DeviceStatus = serde_json::from_str(&json_str)?;
        assert!(parsed.telemetry.is_none());
        assert!(parsed.active_faults.is_empty());
        Ok(())
    }

    #[test]
    fn device_status_many_faults() -> TestResult {
        let faults: Vec<String> = (0..100).map(|i| format!("fault_{i}")).collect();
        let status = DeviceStatus {
            device: DeviceInfo {
                id: "d".to_string(),
                name: "D".to_string(),
                device_type: "WheelBase".to_string(),
                state: "Error".to_string(),
                capabilities: DeviceCapabilities {
                    supports_pid: false,
                    supports_raw_torque_1khz: false,
                    supports_health_stream: false,
                    supports_led_bus: false,
                    max_torque_cnm: 0,
                    encoder_cpr: 0,
                },
            },
            last_seen: "now".to_string(),
            active_faults: faults,
            telemetry: None,
        };
        let json_str = serde_json::to_string(&status)?;
        let parsed: DeviceStatus = serde_json::from_str(&json_str)?;
        assert_eq!(parsed.active_faults.len(), 100);
        Ok(())
    }

    #[test]
    fn telemetry_extreme_values() -> TestResult {
        let tel = TelemetryData {
            wheel_angle_deg: f32::MAX,
            wheel_speed_rad_s: f32::MIN,
            temperature_c: u32::MAX,
            fault_flags: u32::MAX,
            hands_on: true,
        };
        let json_str = serde_json::to_string(&tel)?;
        let parsed: TelemetryData = serde_json::from_str(&json_str)?;
        assert_eq!(parsed.temperature_c, u32::MAX);
        assert_eq!(parsed.fault_flags, u32::MAX);
        Ok(())
    }

    #[test]
    fn banner_with_very_high_torque_ratio() {
        let mut banner = SafetyBanner::new(1.0, ButtonCombo::BothClutchPaddles);
        banner.update_torque(1000.0); // Way beyond max
        assert_eq!(banner.get_warning_level(), WarningLevel::Critical);
    }
}
