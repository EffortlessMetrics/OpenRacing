//! Snapshot tests for engine safety types — ButtonCombo, ConsentRequirements,
//! and safety state diagnostic output.
//!
//! Note: SafetyState, InterlockChallenge, and InterlockAck contain `Instant`
//! fields which are non-deterministic, so we test only the deterministic types.

use racing_wheel_engine::safety::{ButtonCombo, ConsentRequirements};

// --- ButtonCombo Debug and serialized ---

#[test]
fn snapshot_button_combo_both_clutch() {
    insta::assert_debug_snapshot!("button_combo_both_clutch", ButtonCombo::BothClutchPaddles);
}

#[test]
fn snapshot_button_combo_custom_sequence() {
    insta::assert_debug_snapshot!("button_combo_custom_42", ButtonCombo::CustomSequence(42));
}

#[test]
fn snapshot_button_combo_both_clutch_json() {
    insta::assert_json_snapshot!(
        "button_combo_both_clutch_json",
        ButtonCombo::BothClutchPaddles
    );
}

#[test]
fn snapshot_button_combo_custom_json() {
    insta::assert_json_snapshot!("button_combo_custom_json", ButtonCombo::CustomSequence(99));
}

// --- ConsentRequirements ---

#[test]
fn snapshot_consent_requirements_basic() {
    let consent = ConsentRequirements {
        max_torque_nm: 25.0,
        warnings: vec![
            "High torque mode can cause injury".to_string(),
            "Ensure wheel is firmly mounted".to_string(),
        ],
        disclaimers: vec!["Use at your own risk".to_string()],
        requires_explicit_consent: true,
    };
    insta::assert_json_snapshot!("consent_requirements_basic", consent);
}

#[test]
fn snapshot_consent_requirements_minimal() {
    let consent = ConsentRequirements {
        max_torque_nm: 8.0,
        warnings: vec![],
        disclaimers: vec![],
        requires_explicit_consent: false,
    };
    insta::assert_json_snapshot!("consent_requirements_minimal", consent);
}

#[test]
fn snapshot_consent_requirements_debug() {
    let consent = ConsentRequirements {
        max_torque_nm: 20.0,
        warnings: vec!["Keep hands on wheel".to_string()],
        disclaimers: vec![],
        requires_explicit_consent: true,
    };
    insta::assert_debug_snapshot!("consent_requirements_debug", consent);
}

// --- Re-exported FMEA types through safety module ---

use racing_wheel_engine::safety::{FaultAction, FaultType};

#[test]
fn snapshot_engine_fault_type_all_display() {
    let fault_types = [
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::ThermalLimit,
        FaultType::Overcurrent,
        FaultType::PluginOverrun,
        FaultType::TimingViolation,
        FaultType::SafetyInterlockViolation,
        FaultType::HandsOffTimeout,
        FaultType::PipelineFault,
    ];
    let output: Vec<String> = fault_types.iter().map(|ft| format!("{}", ft)).collect();
    insta::assert_debug_snapshot!("engine_fault_types_all_display", output);
}

#[test]
fn snapshot_engine_fault_action_all_debug() {
    let actions = [
        FaultAction::SoftStop,
        FaultAction::Quarantine,
        FaultAction::LogAndContinue,
        FaultAction::Restart,
        FaultAction::SafeMode,
    ];
    insta::assert_debug_snapshot!("engine_fault_actions_all", actions);
}
