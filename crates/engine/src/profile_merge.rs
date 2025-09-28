//! Deterministic profile merging engine
//!
//! This module implements the deterministic profile hierarchy resolution
//! according to the domain policy: Global → Game → Car → Session overrides.

use racing_wheel_schemas::{Profile, BaseSettings, FilterConfig, CurvePoint, Gain};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tracing::{debug, trace};

/// Profile merge engine that implements deterministic hierarchy resolution
#[derive(Clone)]
pub struct ProfileMergeEngine;

/// Result of profile merge operation
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// The merged profile
    pub profile: Profile,
    /// Hash of the merge inputs for change detection
    pub merge_hash: u64,
    /// Merge statistics for debugging
    pub stats: MergeStats,
}

/// Statistics about the merge operation
#[derive(Debug, Clone)]
pub struct MergeStats {
    /// Number of profiles merged
    pub profiles_merged: usize,
    /// Whether session overrides were applied
    pub session_overrides_applied: bool,
    /// Number of filter parameters overridden
    pub filter_overrides: usize,
    /// Number of LED settings overridden
    pub led_overrides: usize,
    /// Number of haptics settings overridden
    pub haptics_overrides: usize,
}

impl ProfileMergeEngine {
    /// Create a new profile merge engine
    pub fn new() -> Self {
        Self
    }

    /// Merge profiles according to hierarchy: Global → Game → Car → Session
    /// 
    /// This method implements deterministic merging where:
    /// - Later profiles in the hierarchy override earlier ones
    /// - Only non-default values are considered for override
    /// - The merge result is deterministic (same inputs → same output)
    pub fn merge_profiles(
        &self,
        global_profile: &Profile,
        game_profile: Option<&Profile>,
        car_profile: Option<&Profile>,
        session_overrides: Option<&BaseSettings>,
    ) -> MergeResult {
        debug!("Starting profile merge operation");
        
        let mut stats = MergeStats {
            profiles_merged: 1, // Always have global
            session_overrides_applied: session_overrides.is_some(),
            filter_overrides: 0,
            led_overrides: 0,
            haptics_overrides: 0,
        };

        // Start with global profile as base
        let mut merged_profile = global_profile.clone();
        trace!("Base profile: Global ({})", global_profile.id);

        // Apply game profile if present
        if let Some(game_profile) = game_profile {
            self.merge_profile_into(&mut merged_profile, game_profile, &mut stats);
            stats.profiles_merged += 1;
            trace!("Applied game profile: {}", game_profile.id);
        }

        // Apply car profile if present
        if let Some(car_profile) = car_profile {
            self.merge_profile_into(&mut merged_profile, car_profile, &mut stats);
            stats.profiles_merged += 1;
            trace!("Applied car profile: {}", car_profile.id);
        }

        // Apply session overrides if present
        if let Some(session_overrides) = session_overrides {
            self.apply_session_overrides(&mut merged_profile, session_overrides, &mut stats);
            trace!("Applied session overrides");
        }

        // Calculate deterministic hash of merge inputs
        let merge_hash = self.calculate_merge_hash(
            global_profile,
            game_profile,
            car_profile,
            session_overrides,
        );

        // Update merged profile metadata
        merged_profile.metadata.modified_at = chrono::Utc::now().to_rfc3339();

        debug!("Profile merge completed: {} profiles merged, hash: {:x}", 
               stats.profiles_merged, merge_hash);

        MergeResult {
            profile: merged_profile,
            merge_hash,
            stats,
        }
    }

    /// Merge one profile into another (target is modified)
    fn merge_profile_into(&self, target: &mut Profile, source: &Profile, stats: &mut MergeStats) {
        // Merge base settings
        self.merge_base_settings(&mut target.base_settings, &source.base_settings, stats);

        // Merge LED config if source has one
        if source.led_config.is_some() {
            target.led_config = source.led_config.clone();
            stats.led_overrides += 1;
        }

        // Merge haptics config if source has one
        if source.haptics_config.is_some() {
            target.haptics_config = source.haptics_config.clone();
            stats.haptics_overrides += 1;
        }
    }

    /// Merge base settings with deterministic override logic
    fn merge_base_settings(&self, target: &mut BaseSettings, source: &BaseSettings, stats: &mut MergeStats) {
        // Create a default BaseSettings to compare against
        let defaults = BaseSettings::default();
        
        // Only override if the source value is different from the default
        if source.ffb_gain.value() != defaults.ffb_gain.value() {
            target.ffb_gain = source.ffb_gain;
            stats.filter_overrides += 1;
        }

        if source.degrees_of_rotation.value() != defaults.degrees_of_rotation.value() {
            target.degrees_of_rotation = source.degrees_of_rotation;
            stats.filter_overrides += 1;
        }

        if source.torque_cap.value() != defaults.torque_cap.value() {
            target.torque_cap = source.torque_cap;
            stats.filter_overrides += 1;
        }

        // Merge filter configuration
        self.merge_filter_config(&mut target.filters, &source.filters, stats);
    }

    /// Merge filter configurations with granular override logic
    fn merge_filter_config(&self, target: &mut FilterConfig, source: &FilterConfig, stats: &mut MergeStats) {
        // Always override with source values for hierarchy precedence
        target.reconstruction = source.reconstruction;
        target.friction = source.friction;
        target.damper = source.damper;
        target.inertia = source.inertia;
        target.slew_rate = source.slew_rate;
        target.notch_filters = source.notch_filters.clone();
        target.curve_points = source.curve_points.clone();
        
        stats.filter_overrides += 7;
    }

    /// Apply session overrides to merged profile
    fn apply_session_overrides(&self, target: &mut Profile, overrides: &BaseSettings, stats: &mut MergeStats) {
        // Session overrides always take precedence
        target.base_settings = overrides.clone();
        stats.filter_overrides += 10; // Indicate session override applied
    }

    /// Calculate deterministic hash of merge inputs
    fn calculate_merge_hash(
        &self,
        global_profile: &Profile,
        game_profile: Option<&Profile>,
        car_profile: Option<&Profile>,
        session_overrides: Option<&BaseSettings>,
    ) -> u64 {
        let mut hasher = DefaultHasher::new();

        // Hash global profile
        global_profile.calculate_hash().hash(&mut hasher);

        // Hash game profile if present
        if let Some(profile) = game_profile {
            profile.calculate_hash().hash(&mut hasher);
        } else {
            0u64.hash(&mut hasher);
        }

        // Hash car profile if present
        if let Some(profile) = car_profile {
            profile.calculate_hash().hash(&mut hasher);
        } else {
            0u64.hash(&mut hasher);
        }

        // Hash session overrides if present
        if let Some(overrides) = session_overrides {
            self.hash_base_settings(overrides).hash(&mut hasher);
        } else {
            0u64.hash(&mut hasher);
        }

        hasher.finish()
    }

    /// Calculate hash for base settings
    fn hash_base_settings(&self, settings: &BaseSettings) -> u64 {
        let mut hasher = DefaultHasher::new();
        
        settings.ffb_gain.value().to_bits().hash(&mut hasher);
        settings.degrees_of_rotation.value().to_bits().hash(&mut hasher);
        settings.torque_cap.value().to_bits().hash(&mut hasher);
        
        // Hash filter config
        settings.filters.reconstruction.hash(&mut hasher);
        settings.filters.friction.value().to_bits().hash(&mut hasher);
        settings.filters.damper.value().to_bits().hash(&mut hasher);
        settings.filters.inertia.value().to_bits().hash(&mut hasher);
        settings.filters.slew_rate.value().to_bits().hash(&mut hasher);
        
        // Hash curve points
        for point in &settings.filters.curve_points {
            point.input.to_bits().hash(&mut hasher);
            point.output.to_bits().hash(&mut hasher);
        }
        
        // Hash notch filters
        for filter in &settings.filters.notch_filters {
            filter.frequency.value().to_bits().hash(&mut hasher);
            filter.q_factor.to_bits().hash(&mut hasher);
            filter.gain_db.to_bits().hash(&mut hasher);
        }
        
        hasher.finish()
    }

    // Default value detection methods for deterministic merging

    fn is_default_gain(&self, gain: Gain) -> bool {
        (gain.value() - 0.7).abs() < f32::EPSILON
    }

    fn is_default_dor(&self, dor: racing_wheel_schemas::Degrees) -> bool {
        (dor.value() - 900.0).abs() < f32::EPSILON
    }

    fn is_default_torque_cap(&self, torque: racing_wheel_schemas::TorqueNm) -> bool {
        (torque.value() - 15.0).abs() < f32::EPSILON
    }

    fn is_default_friction(&self, friction: Gain) -> bool {
        (friction.value() - 0.1).abs() < f32::EPSILON
    }

    fn is_default_damper(&self, damper: Gain) -> bool {
        (damper.value() - 0.15).abs() < f32::EPSILON
    }

    fn is_default_inertia(&self, inertia: Gain) -> bool {
        (inertia.value() - 0.05).abs() < f32::EPSILON
    }

    fn is_default_slew_rate(&self, slew_rate: Gain) -> bool {
        (slew_rate.value() - 0.8).abs() < f32::EPSILON
    }

    fn is_linear_curve(&self, curve_points: &[CurvePoint]) -> bool {
        curve_points.len() == 2
            && (curve_points[0].input - 0.0).abs() < f32::EPSILON
            && (curve_points[0].output - 0.0).abs() < f32::EPSILON
            && (curve_points[1].input - 1.0).abs() < f32::EPSILON
            && (curve_points[1].output - 1.0).abs() < f32::EPSILON
    }
}

impl Default for ProfileMergeEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use racing_wheel_schemas::{
        ProfileId, ProfileScope, TorqueNm, Degrees
    };

    fn create_test_profile(id: &str, scope: ProfileScope) -> Profile {
        Profile::new(
            ProfileId::new(id.to_string()).unwrap(),
            scope,
            BaseSettings::default(),
            format!("Test Profile {}", id),
        )
    }

    fn create_custom_base_settings() -> BaseSettings {
        BaseSettings::new(
            Gain::new(0.8).unwrap(),
            Degrees::new_dor(540.0).unwrap(),
            TorqueNm::new(20.0).unwrap(),
            FilterConfig::default(),
        )
    }

    #[test]
    fn test_merge_engine_global_only() {
        let engine = ProfileMergeEngine::new();
        let global_profile = create_test_profile("global", ProfileScope::global());

        let result = engine.merge_profiles(&global_profile, None, None, None);

        assert_eq!(result.stats.profiles_merged, 1);
        assert!(!result.stats.session_overrides_applied);
        assert_eq!(result.profile.id, global_profile.id);
    }

    #[test]
    fn test_merge_engine_with_game_profile() {
        let engine = ProfileMergeEngine::new();
        let global_profile = create_test_profile("global", ProfileScope::global());
        let mut game_profile = create_test_profile("iracing", ProfileScope::for_game("iracing".to_string()));
        
        // Modify game profile to have different settings
        game_profile.base_settings.ffb_gain = Gain::new(0.8).unwrap();

        let result = engine.merge_profiles(&global_profile, Some(&game_profile), None, None);

        assert_eq!(result.stats.profiles_merged, 2);
        assert_eq!(result.profile.base_settings.ffb_gain.value(), 0.8);
        assert!(result.stats.filter_overrides > 0);
    }

    #[test]
    fn test_merge_engine_full_hierarchy() {
        let engine = ProfileMergeEngine::new();
        let global_profile = create_test_profile("global", ProfileScope::global());
        let mut game_profile = create_test_profile("iracing", ProfileScope::for_game("iracing".to_string()));
        let mut car_profile = create_test_profile("gt3", ProfileScope::for_car("iracing".to_string(), "gt3".to_string()));

        // Set different values at each level
        game_profile.base_settings.ffb_gain = Gain::new(0.8).unwrap();
        car_profile.base_settings.degrees_of_rotation = Degrees::new_dor(540.0).unwrap();

        let result = engine.merge_profiles(
            &global_profile,
            Some(&game_profile),
            Some(&car_profile),
            None,
        );

        assert_eq!(result.stats.profiles_merged, 3);
        assert_eq!(result.profile.base_settings.ffb_gain.value(), 0.8); // From game
        assert_eq!(result.profile.base_settings.degrees_of_rotation.value(), 540.0); // From car
    }

    #[test]
    fn test_merge_engine_with_session_overrides() {
        let engine = ProfileMergeEngine::new();
        let global_profile = create_test_profile("global", ProfileScope::global());
        let session_overrides = create_custom_base_settings();

        let result = engine.merge_profiles(
            &global_profile,
            None,
            None,
            Some(&session_overrides),
        );

        assert!(result.stats.session_overrides_applied);
        assert_eq!(result.profile.base_settings.ffb_gain.value(), 0.8);
        assert_eq!(result.profile.base_settings.degrees_of_rotation.value(), 540.0);
        assert_eq!(result.profile.base_settings.torque_cap.value(), 20.0);
    }

    #[test]
    fn test_merge_engine_deterministic_hash() {
        let engine = ProfileMergeEngine::new();
        let global_profile = create_test_profile("global", ProfileScope::global());
        let game_profile = create_test_profile("iracing", ProfileScope::for_game("iracing".to_string()));

        // Merge the same profiles multiple times
        let result1 = engine.merge_profiles(&global_profile, Some(&game_profile), None, None);
        let result2 = engine.merge_profiles(&global_profile, Some(&game_profile), None, None);

        // Hash should be identical
        assert_eq!(result1.merge_hash, result2.merge_hash);
        
        // Profile hashes should also be identical
        assert_eq!(result1.profile.calculate_hash(), result2.profile.calculate_hash());
    }

    #[test]
    fn test_merge_engine_different_inputs_different_hash() {
        let engine = ProfileMergeEngine::new();
        let global_profile = create_test_profile("global", ProfileScope::global());
        let game_profile1 = create_test_profile("iracing", ProfileScope::for_game("iracing".to_string()));
        let mut game_profile2 = create_test_profile("acc", ProfileScope::for_game("acc".to_string()));
        
        // Make game_profile2 different
        game_profile2.base_settings.ffb_gain = Gain::new(0.9).unwrap();

        let result1 = engine.merge_profiles(&global_profile, Some(&game_profile1), None, None);
        let result2 = engine.merge_profiles(&global_profile, Some(&game_profile2), None, None);

        // Hashes should be different
        assert_ne!(result1.merge_hash, result2.merge_hash);
    }

    #[test]
    fn test_merge_engine_default_value_detection() {
        let engine = ProfileMergeEngine::new();
        
        // Test default value detection
        assert!(engine.is_default_gain(Gain::new(0.7).unwrap()));
        assert!(!engine.is_default_gain(Gain::new(0.8).unwrap()));
        
        assert!(engine.is_default_dor(Degrees::new_dor(900.0).unwrap()));
        assert!(!engine.is_default_dor(Degrees::new_dor(540.0).unwrap()));
        
        assert!(engine.is_default_torque_cap(TorqueNm::new(15.0).unwrap()));
        assert!(!engine.is_default_torque_cap(TorqueNm::new(20.0).unwrap()));
    }

    #[test]
    fn test_merge_engine_linear_curve_detection() {
        let engine = ProfileMergeEngine::new();
        
        let linear_curve = vec![
            CurvePoint::new(0.0, 0.0).unwrap(),
            CurvePoint::new(1.0, 1.0).unwrap(),
        ];
        
        let non_linear_curve = vec![
            CurvePoint::new(0.0, 0.0).unwrap(),
            CurvePoint::new(0.5, 0.7).unwrap(),
            CurvePoint::new(1.0, 1.0).unwrap(),
        ];
        
        assert!(engine.is_linear_curve(&linear_curve));
        assert!(!engine.is_linear_curve(&non_linear_curve));
    }
}