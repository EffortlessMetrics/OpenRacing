//! Tests for pipeline swap atomicity and deterministic profile resolution
//!
//! These tests verify that:
//! 1. Pipeline swaps are atomic from the RT thread perspective
//! 2. Profile resolution is deterministic (same inputs → same outputs)
//! 3. No heap allocations occur on the hot path after pipeline compile
//! 4. Two-phase apply works correctly under concurrent load

use racing_wheel_engine::{
    Pipeline, PipelineCompiler, TwoPhaseApplyCoordinator, ProfileMergeEngine
};
use racing_wheel_schemas::{
    Profile, ProfileId, ProfileScope, BaseSettings, FilterConfig, 
    Gain, Degrees, TorqueNm, CurvePoint
};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::Duration;

/// Test that pipeline swaps are atomic from RT thread perspective
#[tokio::test]
async fn test_pipeline_swap_atomicity() {
    let initial_pipeline = Pipeline::new();
    let coordinator = TwoPhaseApplyCoordinator::new(initial_pipeline);
    let active_pipeline = coordinator.get_active_pipeline();

    // Create test profiles with different configurations
    let global_profile = create_test_profile("global", ProfileScope::global());
    let mut game_profile = create_test_profile("iracing", ProfileScope::for_game("iracing".to_string()));
    game_profile.base_settings.ffb_gain = Gain::new(0.8).unwrap();

    // Counter to track RT thread executions
    let rt_executions = Arc::new(AtomicUsize::new(0));
    let rt_executions_clone = Arc::clone(&rt_executions);

    // Simulate RT thread processing frames
    let rt_handle = tokio::spawn(async move {
        let mut frame = racing_wheel_engine::ffb::Frame {
            ffb_in: 0.5,
            torque_out: 0.0,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };

        for _ in 0..1000 {
            {
                let mut pipeline = active_pipeline.write().await;
                let result = pipeline.process(&mut frame);
                
                // Pipeline should never fail due to concurrent modification
                assert!(result.is_ok(), "Pipeline processing failed: {:?}", result);
                
                // Torque output should always be finite
                assert!(frame.torque_out.is_finite(), "Non-finite torque output: {}", frame.torque_out);
            }
            
            rt_executions_clone.fetch_add(1, Ordering::Relaxed);
            
            // Simulate 1kHz tick rate
            tokio::time::sleep(Duration::from_micros(1000)).await;
        }
    });

    // Start multiple concurrent apply operations
    let mut apply_handles = Vec::new();
    for i in 0..10 {
        let coordinator_clone = coordinator.clone();
        let global_clone = global_profile.clone();
        let game_clone = game_profile.clone();
        
        let handle = tokio::spawn(async move {
            let result_rx = coordinator_clone.apply_profile_async(
                &global_clone,
                Some(&game_clone),
                None,
                None,
            ).await;
            
            assert!(result_rx.is_ok(), "Apply {} failed to start", i);
            
            // Process applies periodically
            for _ in 0..10 {
                coordinator_clone.process_pending_applies_at_tick_boundary().await;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            
            let result = result_rx.unwrap().await;
            assert!(result.is_ok(), "Apply {} failed: {:?}", i, result);
            
            let apply_result = result.unwrap();
            assert!(apply_result.success, "Apply {} was not successful", i);
        });
        
        apply_handles.push(handle);
    }

    // Wait for all applies to complete
    for handle in apply_handles {
        handle.await.unwrap();
    }

    // Wait for RT thread to complete
    rt_handle.await.unwrap();

    // Verify RT thread executed without interruption
    let final_executions = rt_executions.load(Ordering::Relaxed);
    assert_eq!(final_executions, 1000, "RT thread was interrupted");

    // Verify final statistics
    let stats = coordinator.get_stats().await;
    assert_eq!(stats.total_applies, 10);
    assert_eq!(stats.successful_applies, 10);
    assert_eq!(stats.failed_applies, 0);
}

/// Test deterministic profile resolution
#[tokio::test]
async fn test_deterministic_profile_resolution() {
    let merge_engine = ProfileMergeEngine::default();

    // Create test profiles
    let global_profile = create_test_profile("global", ProfileScope::global());
    let mut game_profile = create_test_profile("iracing", ProfileScope::for_game("iracing".to_string()));
    let mut car_profile = create_test_profile("gt3", ProfileScope::for_car("iracing".to_string(), "gt3".to_string()));

    // Set specific values
    game_profile.base_settings.ffb_gain = Gain::new(0.8).unwrap();
    car_profile.base_settings.degrees_of_rotation = Degrees::new_dor(540.0).unwrap();

    let session_overrides = BaseSettings::new(
        Gain::new(0.9).unwrap(),
        Degrees::new_dor(720.0).unwrap(),
        TorqueNm::new(18.0).unwrap(),
        FilterConfig::default(),
    );

    // Perform the same merge multiple times
    let mut results = Vec::new();
    for _ in 0..10 {
        let result = merge_engine.merge_profiles(
            &global_profile,
            Some(&game_profile),
            Some(&car_profile),
            Some(&session_overrides),
        );
        results.push(result);
    }

    // All results should be identical
    let first_hash = results[0].merge_hash;
    let first_profile_hash = results[0].profile.calculate_hash();

    for (i, result) in results.iter().enumerate() {
        assert_eq!(result.merge_hash, first_hash, 
                  "Merge hash differs at iteration {}: expected {:x}, got {:x}", 
                  i, first_hash, result.merge_hash);
        
        assert_eq!(result.profile.calculate_hash(), first_profile_hash,
                  "Profile hash differs at iteration {}", i);
        
        // Verify session overrides took precedence
        assert_eq!(result.profile.base_settings.ffb_gain.value(), 0.9);
        assert_eq!(result.profile.base_settings.degrees_of_rotation.value(), 720.0);
        assert_eq!(result.profile.base_settings.torque_cap.value(), 18.0);
    }
}

/// Test that different inputs produce different hashes
#[tokio::test]
async fn test_different_inputs_different_hashes() {
    let merge_engine = ProfileMergeEngine::default();

    let global_profile = create_test_profile("global", ProfileScope::global());
    let mut game_profile1 = create_test_profile("iracing", ProfileScope::for_game("iracing".to_string()));
    let mut game_profile2 = create_test_profile("acc", ProfileScope::for_game("acc".to_string()));

    // Make profiles different
    game_profile1.base_settings.ffb_gain = Gain::new(0.7).unwrap();
    game_profile2.base_settings.ffb_gain = Gain::new(0.8).unwrap();

    let result1 = merge_engine.merge_profiles(&global_profile, Some(&game_profile1), None, None);
    let result2 = merge_engine.merge_profiles(&global_profile, Some(&game_profile2), None, None);

    // Hashes should be different
    assert_ne!(result1.merge_hash, result2.merge_hash,
              "Different inputs produced same hash: {:x}", result1.merge_hash);
    
    assert_ne!(result1.profile.calculate_hash(), result2.profile.calculate_hash(),
              "Different profiles produced same hash");
}

/// Test pipeline compilation determinism
#[tokio::test]
async fn test_pipeline_compilation_determinism() {
    let compiler = PipelineCompiler::new();

    // Create filter config with various settings
    let mut filter_config = FilterConfig::default();
    filter_config.reconstruction = 6;
    filter_config.friction = Gain::new(0.2).unwrap();
    filter_config.damper = Gain::new(0.25).unwrap();
    filter_config.curve_points = vec![
        CurvePoint::new(0.0, 0.0).unwrap(),
        CurvePoint::new(0.5, 0.7).unwrap(),
        CurvePoint::new(1.0, 1.0).unwrap(),
    ];

    // Compile the same config multiple times
    let mut compiled_pipelines = Vec::new();
    for _ in 0..5 {
        let compiled = compiler.compile_pipeline(filter_config.clone()).await;
        assert!(compiled.is_ok(), "Pipeline compilation failed: {:?}", compiled);
        compiled_pipelines.push(compiled.unwrap());
    }

    // All compilations should produce identical hashes
    let first_hash = compiled_pipelines[0].config_hash;
    for (i, compiled) in compiled_pipelines.iter().enumerate() {
        assert_eq!(compiled.config_hash, first_hash,
                  "Compilation {} produced different hash: expected {:x}, got {:x}",
                  i, first_hash, compiled.config_hash);
    }
}

/// Test no allocations on hot path (debug builds only)
#[cfg(debug_assertions)]
#[tokio::test]
async fn test_no_allocations_on_hot_path() {
    let compiler = PipelineCompiler::new();
    
    // Create a simple filter config
    let filter_config = FilterConfig::default();
    
    // Compile pipeline
    let compiled = compiler.compile_pipeline(filter_config).await.unwrap();
    let mut pipeline = compiled.pipeline;

    // Create test frame
    let mut frame = racing_wheel_engine::ffb::Frame {
        ffb_in: 0.5,
        torque_out: 0.0,
        wheel_speed: 1.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };

    // Process frame multiple times - should not allocate
    for _ in 0..1000 {
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok(), "Pipeline processing failed: {:?}", result);
    }

    // If we reach here without panicking, no allocations occurred
    // (The panic would be triggered by the allocation tracking in pipeline.rs)
}

/// Test concurrent pipeline compilation
#[tokio::test]
async fn test_concurrent_pipeline_compilation() {
    let compiler = PipelineCompiler::new();

    // Create different filter configs
    let configs = vec![
        create_filter_config_with_friction(0.1),
        create_filter_config_with_friction(0.2),
        create_filter_config_with_friction(0.3),
        create_filter_config_with_damper(0.1),
        create_filter_config_with_damper(0.2),
    ];

    // Compile all configs concurrently
    let mut handles = Vec::new();
    for (i, config) in configs.into_iter().enumerate() {
        let compiler_clone = compiler.clone();
        let handle = tokio::spawn(async move {
            let result = compiler_clone.compile_pipeline(config).await;
            assert!(result.is_ok(), "Compilation {} failed: {:?}", i, result);
            result.unwrap()
        });
        handles.push(handle);
    }

    // Wait for all compilations to complete
    let mut compiled_pipelines = Vec::new();
    for handle in handles {
        let compiled = handle.await.unwrap();
        compiled_pipelines.push(compiled);
    }

    // Verify all compilations succeeded and produced different hashes
    assert_eq!(compiled_pipelines.len(), 5);
    
    let mut hashes = std::collections::HashSet::new();
    for compiled in &compiled_pipelines {
        assert!(hashes.insert(compiled.config_hash), 
               "Duplicate hash found: {:x}", compiled.config_hash);
    }
}

/// Test profile hierarchy precedence
#[tokio::test]
async fn test_profile_hierarchy_precedence() {
    let merge_engine = ProfileMergeEngine::default();

    // Create profiles with different values at each level
    let mut global_profile = create_test_profile("global", ProfileScope::global());
    global_profile.base_settings.ffb_gain = Gain::new(0.5).unwrap();
    global_profile.base_settings.degrees_of_rotation = Degrees::new_dor(900.0).unwrap();
    global_profile.base_settings.torque_cap = TorqueNm::new(10.0).unwrap();

    let mut game_profile = create_test_profile("iracing", ProfileScope::for_game("iracing".to_string()));
    game_profile.base_settings.ffb_gain = Gain::new(0.75).unwrap(); // Non-default value
    game_profile.base_settings.degrees_of_rotation = Degrees::new_dor(720.0).unwrap();
    // torque_cap left as default (should not override)

    let mut car_profile = create_test_profile("gt3", ProfileScope::for_car("iracing".to_string(), "gt3".to_string()));
    car_profile.base_settings.ffb_gain = Gain::new(0.8).unwrap();
    // Other settings left as default

    let session_overrides = BaseSettings::new(
        Gain::new(0.9).unwrap(), // Should override everything
        Degrees::new_dor(540.0).unwrap(), // Should override everything
        TorqueNm::new(25.0).unwrap(), // Should override everything
        FilterConfig::default(),
    );

    // Test different hierarchy levels
    
    // Global only
    let result_global = merge_engine.merge_profiles(&global_profile, None, None, None);
    assert_eq!(result_global.profile.base_settings.ffb_gain.value(), 0.5);
    assert_eq!(result_global.profile.base_settings.degrees_of_rotation.value(), 900.0);
    assert_eq!(result_global.profile.base_settings.torque_cap.value(), 10.0);

    // Global + Game
    let result_game = merge_engine.merge_profiles(&global_profile, Some(&game_profile), None, None);
    assert_eq!(result_game.profile.base_settings.ffb_gain.value(), 0.75); // Game overrides
    assert_eq!(result_game.profile.base_settings.degrees_of_rotation.value(), 720.0); // Game overrides
    assert_eq!(result_game.profile.base_settings.torque_cap.value(), 10.0); // Global remains

    // Global + Game + Car
    let result_car = merge_engine.merge_profiles(&global_profile, Some(&game_profile), Some(&car_profile), None);
    assert_eq!(result_car.profile.base_settings.ffb_gain.value(), 0.8); // Car overrides
    assert_eq!(result_car.profile.base_settings.degrees_of_rotation.value(), 720.0); // Game remains
    assert_eq!(result_car.profile.base_settings.torque_cap.value(), 10.0); // Global remains

    // Full hierarchy with session overrides
    let result_session = merge_engine.merge_profiles(
        &global_profile, 
        Some(&game_profile), 
        Some(&car_profile), 
        Some(&session_overrides)
    );
    assert_eq!(result_session.profile.base_settings.ffb_gain.value(), 0.9); // Session overrides all
    assert_eq!(result_session.profile.base_settings.degrees_of_rotation.value(), 540.0); // Session overrides all
    assert_eq!(result_session.profile.base_settings.torque_cap.value(), 25.0); // Session overrides all
}

/// Test curve monotonicity validation
#[tokio::test]
async fn test_curve_monotonicity_validation() {
    let compiler = PipelineCompiler::new();

    // Valid monotonic curve
    let valid_config = FilterConfig::new(
        4,
        Gain::new(0.1).unwrap(),
        Gain::new(0.15).unwrap(),
        Gain::new(0.05).unwrap(),
        vec![],
        Gain::new(0.8).unwrap(),
        vec![
            CurvePoint::new(0.0, 0.0).unwrap(),
            CurvePoint::new(0.5, 0.6).unwrap(),
            CurvePoint::new(1.0, 1.0).unwrap(),
        ],
    ).unwrap();

    let result = compiler.compile_pipeline(valid_config).await;
    assert!(result.is_ok(), "Valid monotonic curve should compile");

    // Invalid non-monotonic curve
    let invalid_config = FilterConfig::new(
        4,
        Gain::new(0.1).unwrap(),
        Gain::new(0.15).unwrap(),
        Gain::new(0.05).unwrap(),
        vec![],
        Gain::new(0.8).unwrap(),
        vec![
            CurvePoint::new(0.0, 0.0).unwrap(),
            CurvePoint::new(0.7, 0.6).unwrap(), // Non-monotonic
            CurvePoint::new(0.5, 0.8).unwrap(),
            CurvePoint::new(1.0, 1.0).unwrap(),
        ],
    );

    // This should fail during FilterConfig creation
    assert!(invalid_config.is_err(), "Non-monotonic curve should be rejected");
}

// Helper functions

fn create_test_profile(id: &str, scope: ProfileScope) -> Profile {
    Profile::new(
        ProfileId::new(id.to_string()).unwrap(),
        scope,
        BaseSettings::default(),
        format!("Test Profile {}", id),
    )
}

fn create_filter_config_with_friction(friction: f32) -> FilterConfig {
    let mut config = FilterConfig::default();
    config.friction = Gain::new(friction).unwrap();
    config
}

fn create_filter_config_with_damper(damper: f32) -> FilterConfig {
    let mut config = FilterConfig::default();
    config.damper = Gain::new(damper).unwrap();
    config
}

/// Stress test for two-phase apply under high load
#[tokio::test]
async fn test_two_phase_apply_stress() {
    let initial_pipeline = Pipeline::new();
    let coordinator = TwoPhaseApplyCoordinator::new(initial_pipeline);

    let global_profile = create_test_profile("global", ProfileScope::global());
    
    // Create many different profiles
    let mut profiles = Vec::new();
    for i in 0..50 {
        let mut profile = create_test_profile(&format!("profile_{}", i), ProfileScope::global());
        profile.base_settings.ffb_gain = Gain::new(0.5 + (i as f32 * 0.01)).unwrap();
        profiles.push(profile);
    }

    // Start many concurrent applies
    let mut handles = Vec::new();
    for (i, profile) in profiles.iter().enumerate() {
        let coordinator_clone = coordinator.clone();
        let global_clone = global_profile.clone();
        let profile_clone = profile.clone();
        
        let handle = tokio::spawn(async move {
            let result_rx = coordinator_clone.apply_profile_async(
                &global_clone,
                Some(&profile_clone),
                None,
                None,
            ).await;
            
            assert!(result_rx.is_ok(), "Apply {} failed to start", i);
            result_rx.unwrap()
        });
        
        handles.push(handle);
    }

    // Collect all result receivers
    let mut result_receivers = Vec::new();
    for handle in handles {
        let rx = handle.await.unwrap();
        result_receivers.push(rx);
    }

    // Process applies in batches
    for _ in 0..20 {
        coordinator.process_pending_applies_at_tick_boundary().await;
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    // Wait for all results
    let mut successful_applies = 0;
    for (i, rx) in result_receivers.into_iter().enumerate() {
        let result = rx.await;
        assert!(result.is_ok(), "Apply {} result channel failed", i);
        
        let apply_result = result.unwrap();
        if apply_result.success {
            successful_applies += 1;
        }
    }

    // Verify statistics
    let stats = coordinator.get_stats().await;
    assert_eq!(stats.total_applies, 50);
    assert_eq!(stats.successful_applies, successful_applies as u64);
    assert_eq!(stats.pending_applies, 0);
    
    // Most applies should succeed (allow for some failures under stress)
    assert!(successful_applies >= 45, "Too many failed applies: {}/50", successful_applies);
}