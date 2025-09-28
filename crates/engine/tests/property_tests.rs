//! Property-based tests for domain policies
//!
//! These tests use property-based testing to verify that the domain policies
//! behave correctly across a wide range of inputs and edge cases.

use racing_wheel_engine::{SafetyPolicy, ProfileHierarchyPolicy, SafetyViolation};
use racing_wheel_schemas::{
    DeviceId, ProfileId, DeviceCapabilities, DeviceType, Device, DeviceState,
    Profile, ProfileScope, BaseSettings, FilterConfig, Gain, Degrees, TorqueNm,
    CurvePoint, NotchFilter, FrequencyHz, LedConfig, HapticsConfig
};
use std::time::Duration;
use std::collections::HashMap;

// Property-based testing using quickcheck
use quickcheck::{TestResult, Arbitrary, Gen};
use quickcheck_macros::quickcheck;

/// Arbitrary implementation for TorqueNm for property testing
#[derive(Debug, Clone)]
struct ArbitraryTorqueNm(TorqueNm);

impl Arbitrary for ArbitraryTorqueNm {
    fn arbitrary(g: &mut Gen) -> Self {
        let value = f32::arbitrary(g).abs() % 50.0; // Limit to reasonable range
        ArbitraryTorqueNm(TorqueNm::new(value).unwrap_or(TorqueNm::ZERO))
    }
}

/// Arbitrary implementation for Degrees for property testing
#[derive(Debug, Clone)]
struct ArbitraryDegrees(Degrees);

impl Arbitrary for ArbitraryDegrees {
    fn arbitrary(g: &mut Gen) -> Self {
        let value = (f32::arbitrary(g).abs() % 1980.0) + 180.0; // 180-2160 range
        ArbitraryDegrees(Degrees::new_dor(value).unwrap_or(Degrees::from_raw(900.0)))
    }
}

/// Arbitrary implementation for Gain for property testing
#[derive(Debug, Clone)]
struct ArbitraryGain(Gain);

impl Arbitrary for ArbitraryGain {
    fn arbitrary(g: &mut Gen) -> Self {
        let value = f32::arbitrary(g).abs() % 1.0; // 0.0-1.0 range
        ArbitraryGain(Gain::new(value).unwrap_or(Gain::ZERO))
    }
}

/// Create a test device with arbitrary capabilities
fn create_arbitrary_device(max_torque: TorqueNm) -> Device {
    let id = DeviceId::new("test-device".to_string()).unwrap();
    let capabilities = DeviceCapabilities::new(
        false,
        true,
        true,
        true,
        max_torque,
        10000,
        1000,
    );
    
    Device::new(id, "Test Wheel".to_string(), DeviceType::WheelBase, capabilities)
}

/// Create a test profile with arbitrary settings
fn create_arbitrary_profile(
    id: &str,
    scope: ProfileScope,
    ffb_gain: Gain,
    dor: Degrees,
    torque_cap: TorqueNm,
) -> Profile {
    let profile_id = ProfileId::new(id.to_string()).unwrap();
    let base_settings = BaseSettings::new(
        ffb_gain,
        dor,
        torque_cap,
        FilterConfig::default(),
    );
    
    Profile::new(profile_id, scope, base_settings, format!("Test Profile {}", id))
}

#[quickcheck]
fn prop_safety_policy_torque_validation_never_exceeds_device_limit(
    requested_torque: ArbitraryTorqueNm,
    device_max_torque: ArbitraryTorqueNm,
    is_high_torque: bool,
) -> TestResult {
    let policy = SafetyPolicy::new();
    let capabilities = DeviceCapabilities::new(
        false, true, true, true,
        device_max_torque.0,
        10000, 1000,
    );
    
    match policy.validate_torque_limits(requested_torque.0, is_high_torque, &capabilities) {
        Ok(validated_torque) => {
            // Validated torque should never exceed device capability
            TestResult::from_bool(validated_torque <= device_max_torque.0)
        }
        Err(_) => {
            // If validation fails, the requested torque should exceed some limit
            let policy_limit = policy.get_max_torque(is_high_torque);
            let effective_limit = policy_limit.min(device_max_torque.0);
            TestResult::from_bool(requested_torque.0 > effective_limit)
        }
    }
}

#[quickcheck]
fn prop_safety_policy_high_torque_requires_operational_device(
    temperature: u8,
    hands_off_seconds: u8,
) -> TestResult {
    let mut policy = SafetyPolicy::new();
    
    // Test with faulted device
    let mut device = create_arbitrary_device(TorqueNm::new(25.0).unwrap());
    device.set_fault_flags(0x04); // Set thermal fault
    
    let result = policy.can_enable_high_torque(
        &device,
        Duration::from_secs(hands_off_seconds as u64),
        temperature,
    );
    
    // Should always fail for faulted device
    TestResult::from_bool(matches!(result, Err(SafetyViolation::ActiveFaults(_))))
}

#[quickcheck]
fn prop_safety_policy_temperature_limit_enforced(temperature: u8) -> TestResult {
    let mut policy = SafetyPolicy::new();
    let device = create_arbitrary_device(TorqueNm::new(25.0).unwrap());
    
    let result = policy.can_enable_high_torque(
        &device,
        Duration::from_secs(1), // Hands on
        temperature,
    );
    
    let max_temp = policy.max_temperature();
    
    if temperature >= max_temp {
        // Should fail for high temperature
        TestResult::from_bool(matches!(result, Err(SafetyViolation::TemperatureTooHigh { .. })))
    } else {
        // May succeed or fail for other reasons, but not temperature
        match result {
            Err(SafetyViolation::TemperatureTooHigh { .. }) => TestResult::from_bool(false),
            _ => TestResult::passed(),
        }
    }
}

#[quickcheck]
fn prop_safety_policy_hands_off_limit_enforced(hands_off_seconds: u16) -> TestResult {
    let mut policy = SafetyPolicy::new();
    let device = create_arbitrary_device(TorqueNm::new(25.0).unwrap());
    
    let result = policy.can_enable_high_torque(
        &device,
        Duration::from_secs(hands_off_seconds as u64),
        50, // Normal temperature
    );
    
    let max_hands_off = policy.max_hands_off_duration();
    let hands_off_duration = Duration::from_secs(hands_off_seconds as u64);
    
    if hands_off_duration > max_hands_off {
        // Should fail for long hands-off duration
        TestResult::from_bool(matches!(result, Err(SafetyViolation::HandsOffTooLong { .. })))
    } else {
        // May succeed or fail for other reasons, but not hands-off
        match result {
            Err(SafetyViolation::HandsOffTooLong { .. }) => TestResult::from_bool(false),
            _ => TestResult::passed(),
        }
    }
}

#[quickcheck]
fn prop_profile_hierarchy_resolution_is_deterministic(
    global_gain: ArbitraryGain,
    game_gain: ArbitraryGain,
    car_gain: ArbitraryGain,
) -> bool {
    let global_profile = create_arbitrary_profile(
        "global",
        ProfileScope::global(),
        global_gain.0,
        Degrees::from_raw(900.0),
        TorqueNm::from_raw(15.0),
    );
    
    let game_profile = create_arbitrary_profile(
        "game",
        ProfileScope::for_game("iracing".to_string()),
        game_gain.0,
        Degrees::from_raw(540.0),
        TorqueNm::from_raw(20.0),
    );
    
    let car_profile = create_arbitrary_profile(
        "car",
        ProfileScope::for_car("iracing".to_string(), "gt3".to_string()),
        car_gain.0,
        Degrees::from_raw(720.0),
        TorqueNm::from_raw(25.0),
    );
    
    // Resolve the same hierarchy twice
    let resolved1 = ProfileHierarchyPolicy::resolve_profile_hierarchy(
        &global_profile,
        Some(&game_profile),
        Some(&car_profile),
        None,
    );
    
    let resolved2 = ProfileHierarchyPolicy::resolve_profile_hierarchy(
        &global_profile,
        Some(&game_profile),
        Some(&car_profile),
        None,
    );
    
    // Results should be identical (deterministic)
    resolved1.base_settings.ffb_gain.value() == resolved2.base_settings.ffb_gain.value()
        && resolved1.base_settings.degrees_of_rotation.value() == resolved2.base_settings.degrees_of_rotation.value()
        && resolved1.base_settings.torque_cap.value() == resolved2.base_settings.torque_cap.value()
}

#[quickcheck]
fn prop_profile_hierarchy_more_specific_wins(
    global_gain: ArbitraryGain,
    car_gain: ArbitraryGain,
) -> bool {
    let global_profile = create_arbitrary_profile(
        "global",
        ProfileScope::global(),
        global_gain.0,
        Degrees::from_raw(900.0),
        TorqueNm::from_raw(15.0),
    );
    
    let car_profile = create_arbitrary_profile(
        "car",
        ProfileScope::for_car("iracing".to_string(), "gt3".to_string()),
        car_gain.0,
        Degrees::from_raw(720.0),
        TorqueNm::from_raw(25.0),
    );
    
    let resolved = ProfileHierarchyPolicy::resolve_profile_hierarchy(
        &global_profile,
        None,
        Some(&car_profile),
        None,
    );
    
    // Car profile (more specific) should win
    resolved.base_settings.ffb_gain.value() == car_gain.0.value()
        && resolved.base_settings.degrees_of_rotation.value() == 720.0
        && resolved.base_settings.torque_cap.value() == 25.0
}

#[quickcheck]
fn prop_profile_hierarchy_hash_consistency(
    global_gain: ArbitraryGain,
    game_gain: ArbitraryGain,
) -> bool {
    let global_profile = create_arbitrary_profile(
        "global",
        ProfileScope::global(),
        global_gain.0,
        Degrees::from_raw(900.0),
        TorqueNm::from_raw(15.0),
    );
    
    let game_profile = create_arbitrary_profile(
        "game",
        ProfileScope::for_game("iracing".to_string()),
        game_gain.0,
        Degrees::from_raw(540.0),
        TorqueNm::from_raw(20.0),
    );
    
    // Same inputs should produce same hash
    let hash1 = ProfileHierarchyPolicy::calculate_hierarchy_hash(
        &global_profile,
        Some(&game_profile),
        None,
        None,
    );
    
    let hash2 = ProfileHierarchyPolicy::calculate_hierarchy_hash(
        &global_profile,
        Some(&game_profile),
        None,
        None,
    );
    
    hash1 == hash2
}

#[quickcheck]
fn prop_profile_scope_matching_is_consistent(game_name: String, car_name: String) -> TestResult {
    // Skip empty strings as they're not valid for our use case
    if game_name.is_empty() || car_name.is_empty() {
        return TestResult::discard();
    }
    
    let global_scope = ProfileScope::global();
    let game_scope = ProfileScope::for_game(game_name.clone());
    let car_scope = ProfileScope::for_car(game_name.clone(), car_name.clone());
    
    // Global scope should match everything
    let global_matches_all = global_scope.matches(Some(&game_name), Some(&car_name), None)
        && global_scope.matches(Some(&game_name), None, None)
        && global_scope.matches(None, None, None);
    
    // Game scope should match its game
    let game_matches_correctly = game_scope.matches(Some(&game_name), Some(&car_name), None)
        && game_scope.matches(Some(&game_name), None, None)
        && !game_scope.matches(Some("other_game"), None, None);
    
    // Car scope should match its specific game+car combination
    let car_matches_correctly = car_scope.matches(Some(&game_name), Some(&car_name), None)
        && !car_scope.matches(Some(&game_name), Some("other_car"), None)
        && !car_scope.matches(Some("other_game"), Some(&car_name), None);
    
    TestResult::from_bool(global_matches_all && game_matches_correctly && car_matches_correctly)
}

#[quickcheck]
fn prop_profile_specificity_ordering_is_correct() -> bool {
    let global_scope = ProfileScope::global();
    let game_scope = ProfileScope::for_game("iracing".to_string());
    let car_scope = ProfileScope::for_car("iracing".to_string(), "gt3".to_string());
    let track_scope = ProfileScope::for_track(
        "iracing".to_string(),
        "gt3".to_string(),
        "spa".to_string(),
    );
    
    // Specificity levels should be in ascending order
    global_scope.specificity_level() < game_scope.specificity_level()
        && game_scope.specificity_level() < car_scope.specificity_level()
        && car_scope.specificity_level() < track_scope.specificity_level()
        && global_scope.specificity_level() == 0
        && track_scope.specificity_level() == 3
}

// Integration property tests
#[quickcheck]
fn prop_safety_and_profile_integration(
    profile_torque_cap: ArbitraryTorqueNm,
    device_max_torque: ArbitraryTorqueNm,
    is_high_torque: bool,
) -> TestResult {
    // Skip cases where profile torque cap is higher than device capability
    // (this should be caught by validation)
    if profile_torque_cap.0 > device_max_torque.0 {
        return TestResult::discard();
    }
    
    let policy = SafetyPolicy::new();
    let capabilities = DeviceCapabilities::new(
        false, true, true, true,
        device_max_torque.0,
        10000, 1000,
    );
    
    // The effective torque limit should be the minimum of:
    // 1. Safety policy limit (safe/high torque)
    // 2. Device capability
    // 3. Profile torque cap
    let safety_limit = policy.get_max_torque(is_high_torque);
    let expected_limit = safety_limit.min(device_max_torque.0).min(profile_torque_cap.0);
    
    match policy.validate_torque_limits(expected_limit, is_high_torque, &capabilities) {
        Ok(validated_torque) => {
            TestResult::from_bool(validated_torque <= expected_limit)
        }
        Err(_) => {
            // Should not fail for a torque value at or below the expected limit
            TestResult::from_bool(false)
        }
    }
}

// Edge case tests
#[test]
fn test_safety_policy_edge_cases() {
    let mut policy = SafetyPolicy::new();
    let device = create_arbitrary_device(TorqueNm::new(25.0).unwrap());
    
    // Test exactly at temperature limit
    let result = policy.can_enable_high_torque(
        &device,
        Duration::from_secs(1),
        policy.max_temperature(),
    );
    assert!(matches!(result, Err(SafetyViolation::TemperatureTooHigh { .. })));
    
    // Test exactly at hands-off limit
    let result = policy.can_enable_high_torque(
        &device,
        policy.max_hands_off_duration(),
        50,
    );
    assert!(result.is_ok()); // Should be OK at the limit
    
    // Test just over hands-off limit
    let result = policy.can_enable_high_torque(
        &device,
        policy.max_hands_off_duration() + Duration::from_millis(1),
        50,
    );
    assert!(matches!(result, Err(SafetyViolation::HandsOffTooLong { .. })));
}

#[test]
fn test_profile_hierarchy_edge_cases() {
    // Test with empty profile list
    let profiles: Vec<Profile> = vec![];
    let result = ProfileHierarchyPolicy::find_most_specific_profile(
        &profiles,
        Some("iracing"),
        Some("gt3"),
        None,
    );
    assert!(result.is_none());
    
    // Test with only global profile
    let global_profile = create_arbitrary_profile(
        "global",
        ProfileScope::global(),
        Gain::from_raw(0.7),
        Degrees::from_raw(900.0),
        TorqueNm::from_raw(15.0),
    );
    
    let profiles = vec![global_profile];
    let result = ProfileHierarchyPolicy::find_most_specific_profile(
        &profiles,
        Some("any_game"),
        Some("any_car"),
        None,
    );
    assert!(result.is_some());
    assert_eq!(result.unwrap().id.as_str(), "global");
}

// Run the property tests
#[cfg(test)]
mod quickcheck_tests {
    use super::*;
    
    use quickcheck::quickcheck;
    
    quickcheck! {
        fn prop_torque_validation_bounds(
            requested: ArbitraryTorqueNm,
            device_max: ArbitraryTorqueNm,
            is_high: bool
        ) -> TestResult {
            prop_safety_policy_torque_validation_never_exceeds_device_limit(requested, device_max, is_high)
        }
        
        fn prop_device_operational_check(temp: u8, hands_off: u8) -> TestResult {
            prop_safety_policy_high_torque_requires_operational_device(temp, hands_off)
        }
        
        fn prop_temperature_enforcement(temp: u8) -> TestResult {
            prop_safety_policy_temperature_limit_enforced(temp)
        }
        
        fn prop_hands_off_enforcement(hands_off: u16) -> TestResult {
            prop_safety_policy_hands_off_limit_enforced(hands_off)
        }
        
        fn prop_hierarchy_deterministic(
            global: ArbitraryGain,
            game: ArbitraryGain,
            car: ArbitraryGain
        ) -> bool {
            prop_profile_hierarchy_resolution_is_deterministic(global, game, car)
        }
        
        fn prop_specific_wins(global: ArbitraryGain, car: ArbitraryGain) -> bool {
            prop_profile_hierarchy_more_specific_wins(global, car)
        }
        
        fn prop_hash_consistent(global: ArbitraryGain, game: ArbitraryGain) -> bool {
            prop_profile_hierarchy_hash_consistency(global, game)
        }
        
        fn prop_scope_matching(game: String, car: String) -> TestResult {
            prop_profile_scope_matching_is_consistent(game, car)
        }
        
        fn prop_specificity_order() -> bool {
            prop_profile_specificity_ordering_is_correct()
        }
        
        fn prop_integration_test(
            profile_cap: ArbitraryTorqueNm,
            device_max: ArbitraryTorqueNm,
            is_high: bool
        ) -> TestResult {
            prop_safety_and_profile_integration(profile_cap, device_max, is_high)
        }
    }
}