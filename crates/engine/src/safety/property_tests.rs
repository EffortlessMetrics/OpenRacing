//! Property-based tests for safety-critical torque management

use super::*;
use proptest::prelude::*;

/// Strategy for safe torque limits (positive, realistic Nm range)
fn safe_torque_strategy() -> impl Strategy<Value = f32> {
    1.0f32..=20.0
}

/// Strategy for high torque limits (above safe, realistic Nm range)
fn high_torque_strategy() -> impl Strategy<Value = f32> {
    20.0f32..=100.0
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // --- SafetyService::clamp_torque_nm ---

    #[test]
    fn prop_clamp_torque_nan_yields_zero(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
    ) {
        let service = SafetyService::new(safe, high);
        let result = service.clamp_torque_nm(f32::NAN);
        prop_assert_eq!(result, 0.0, "NaN must clamp to 0.0 (safe state)");
    }

    #[test]
    fn prop_clamp_torque_pos_inf_yields_zero(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
    ) {
        let service = SafetyService::new(safe, high);
        let result = service.clamp_torque_nm(f32::INFINITY);
        prop_assert_eq!(result, 0.0, "positive infinity must clamp to 0.0 (safe state)");
    }

    #[test]
    fn prop_clamp_torque_neg_inf_yields_zero(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
    ) {
        let service = SafetyService::new(safe, high);
        let result = service.clamp_torque_nm(f32::NEG_INFINITY);
        prop_assert_eq!(result, 0.0, "negative infinity must clamp to 0.0 (safe state)");
    }

    #[test]
    fn prop_clamp_torque_bounded_in_safe_state(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
        requested in -200.0f32..=200.0,
    ) {
        let service = SafetyService::new(safe, high);
        let result = service.clamp_torque_nm(requested);
        prop_assert!(
            result >= -safe && result <= safe,
            "safe-state clamp {} not in [-{}, {}]", result, safe, safe
        );
    }

    #[test]
    fn prop_clamp_torque_preserves_sign(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
        requested in -200.0f32..=200.0,
    ) {
        let service = SafetyService::new(safe, high);
        let result = service.clamp_torque_nm(requested);
        if requested > 0.0 {
            prop_assert!(result >= 0.0, "positive request {} clamped to negative {}", requested, result);
        } else if requested < 0.0 {
            prop_assert!(result <= 0.0, "negative request {} clamped to positive {}", requested, result);
        }
    }

    // --- Faulted state always yields zero torque ---

    #[test]
    fn prop_faulted_state_clamps_to_zero(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
        requested in -200.0f32..=200.0,
    ) {
        let mut service = SafetyService::new(safe, high);
        service.report_fault(FaultType::UsbStall);
        let result = service.clamp_torque_nm(requested);
        prop_assert_eq!(result, 0.0, "faulted state must always clamp to 0.0");
    }

    #[test]
    fn prop_faulted_max_torque_zero(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
    ) {
        let mut service = SafetyService::new(safe, high);
        service.report_fault(FaultType::ThermalLimit);
        prop_assert_eq!(service.max_torque_nm(), 0.0, "faulted max torque must be 0.0");
    }

    // --- get_max_torque: safe vs high torque ---

    #[test]
    fn prop_safe_state_returns_safe_torque(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
    ) {
        let service = SafetyService::new(safe, high);
        let max = service.max_torque_nm();
        let diff = (max - safe).abs();
        prop_assert!(diff < 0.001, "safe state max torque {} != safe limit {}", max, safe);
    }

    // --- Fault reporting always transitions to Faulted ---

    #[test]
    fn prop_report_fault_transitions_to_faulted(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
        fault_idx in 0usize..9,
    ) {
        let faults = [
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
        let fault = faults[fault_idx];
        let mut service = SafetyService::new(safe, high);
        service.report_fault(fault);
        match service.state() {
            SafetyState::Faulted { fault: f, .. } => {
                prop_assert_eq!(*f, fault, "fault type mismatch");
            }
            other => {
                return Err(TestCaseError::fail(
                    format!("expected Faulted, got {:?}", other)
                ));
            }
        }
    }

    // --- SafetyService initial state is always SafeTorque ---

    #[test]
    fn prop_initial_state_is_safe_torque(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
    ) {
        let service = SafetyService::new(safe, high);
        prop_assert_eq!(service.state(), &SafetyState::SafeTorque);
    }

    // --- Cannot request high torque when faulted ---

    #[test]
    fn prop_cannot_request_high_torque_when_faulted(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
    ) {
        let mut service = SafetyService::new(safe, high);
        service.report_fault(FaultType::Overcurrent);
        let result = service.request_high_torque("device");
        prop_assert!(result.is_err(), "should not be able to request high torque when faulted");
    }

    // --- Challenge / cancel round-trip ---

    #[test]
    fn prop_cancel_challenge_returns_to_safe(
        safe in safe_torque_strategy(),
        high in high_torque_strategy(),
    ) {
        let mut service = SafetyService::new(safe, high);
        let challenge_result = service.request_high_torque("device");
        if let Ok(challenge) = challenge_result {
            let _ = service.provide_ui_consent(challenge.challenge_token);
            let cancel_result = service.cancel_challenge();
            prop_assert!(cancel_result.is_ok(), "cancel should succeed");
            prop_assert_eq!(service.state(), &SafetyState::SafeTorque);
        }
    }
}
