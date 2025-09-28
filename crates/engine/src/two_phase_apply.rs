//! Two-phase apply system for atomic pipeline updates
//!
//! This module implements the two-phase apply pattern:
//! 1. Compile off-thread → 2. Swap at tick boundary → 3. Ack to UI

use crate::pipeline::{Pipeline, PipelineCompiler, CompiledPipeline, PipelineError};
use crate::profile_merge::{ProfileMergeEngine, MergeResult};
use racing_wheel_schemas::{Profile, BaseSettings};
use std::sync::Arc;
use tokio::sync::{oneshot, RwLock, Mutex};
use tracing::{debug, info, error};
use std::time::Instant;

/// Two-phase apply coordinator
#[derive(Clone)]
pub struct TwoPhaseApplyCoordinator {
    /// Pipeline compiler for off-thread compilation
    compiler: PipelineCompiler,
    
    /// Profile merge engine
    merge_engine: ProfileMergeEngine,
    
    /// Currently active pipeline (shared with RT thread)
    active_pipeline: Arc<RwLock<Pipeline>>,
    
    /// Pending apply operations
    pending_applies: Arc<Mutex<Vec<PendingApply>>>,
    
    /// Apply statistics
    stats: Arc<Mutex<ApplyStats>>,
}

/// Pending apply operation
struct PendingApply {
    /// Compiled pipeline ready for swap
    compiled_pipeline: CompiledPipeline,
    
    /// Merge result that generated this pipeline
    merge_result: MergeResult,
    
    /// Response channel to notify UI
    response_tx: oneshot::Sender<ApplyResult>,
    
    /// Timestamp when apply was requested
    requested_at: Instant,
}

/// Result of apply operation
#[derive(Debug, Clone)]
pub struct ApplyResult {
    /// Whether the apply was successful
    pub success: bool,
    
    /// Configuration hash of applied pipeline
    pub config_hash: u64,
    
    /// Merge hash for change detection
    pub merge_hash: u64,
    
    /// Time taken for the entire operation
    pub duration_ms: u64,
    
    /// Error message if apply failed
    pub error: Option<String>,
    
    /// Apply statistics
    pub stats: ApplyOperationStats,
}

/// Statistics for a single apply operation
#[derive(Debug, Clone)]
pub struct ApplyOperationStats {
    /// Time spent compiling pipeline (ms)
    pub compilation_time_ms: u64,
    
    /// Time spent waiting for tick boundary (ms)
    pub wait_time_ms: u64,
    
    /// Time spent swapping pipeline (μs)
    pub swap_time_us: u64,
    
    /// Number of filter nodes in pipeline
    pub node_count: usize,
    
    /// Size of pipeline state in bytes
    pub state_size_bytes: usize,
}

/// Overall apply statistics
#[derive(Debug, Clone)]
pub struct ApplyStats {
    /// Total number of applies attempted
    pub total_applies: u64,
    
    /// Number of successful applies
    pub successful_applies: u64,
    
    /// Number of failed applies
    pub failed_applies: u64,
    
    /// Average compilation time (ms)
    pub avg_compilation_time_ms: f64,
    
    /// Average swap time (μs)
    pub avg_swap_time_us: f64,
    
    /// Maximum swap time observed (μs)
    pub max_swap_time_us: u64,
    
    /// Number of applies currently pending
    pub pending_applies: usize,
}

impl TwoPhaseApplyCoordinator {
    /// Create a new two-phase apply coordinator
    pub fn new(initial_pipeline: Pipeline) -> Self {
        Self {
            compiler: PipelineCompiler::new(),
            merge_engine: ProfileMergeEngine::new(),
            active_pipeline: Arc::new(RwLock::new(initial_pipeline)),
            pending_applies: Arc::new(Mutex::new(Vec::new())),
            stats: Arc::new(Mutex::new(ApplyStats::default())),
        }
    }

    /// Apply a new profile configuration (async, returns immediately)
    /// 
    /// This method implements the two-phase apply pattern:
    /// 1. Merge profiles according to hierarchy
    /// 2. Compile pipeline off-thread
    /// 3. Queue for atomic swap at tick boundary
    /// 4. Send acknowledgment to UI when complete
    pub async fn apply_profile_async(
        &self,
        global_profile: &Profile,
        game_profile: Option<&Profile>,
        car_profile: Option<&Profile>,
        session_overrides: Option<&BaseSettings>,
    ) -> Result<oneshot::Receiver<ApplyResult>, PipelineError> {
        let start_time = Instant::now();
        
        debug!("Starting two-phase apply operation");

        // Phase 1: Merge profiles (deterministic)
        let merge_result = self.merge_engine.merge_profiles(
            global_profile,
            game_profile,
            car_profile,
            session_overrides,
        );

        info!("Profile merge completed: hash {:x}, {} profiles merged", 
              merge_result.merge_hash, merge_result.stats.profiles_merged);

        // Phase 2: Compile pipeline off-thread
        let compilation_start = Instant::now();
        let compiled_pipeline = self.compiler.compile_pipeline(
            merge_result.profile.base_settings.filters.clone()
        ).await?;
        let compilation_time = compilation_start.elapsed();

        info!("Pipeline compilation completed: hash {:x}, {} nodes, {}ms", 
              compiled_pipeline.config_hash, 
              compiled_pipeline.pipeline.node_count(),
              compilation_time.as_millis());

        // Create response channel
        let (response_tx, response_rx) = oneshot::channel();

        // Create pending apply
        let pending_apply = PendingApply {
            compiled_pipeline,
            merge_result,
            response_tx,
            requested_at: start_time,
        };

        // Queue for atomic swap
        {
            let mut pending = self.pending_applies.lock().await;
            pending.push(pending_apply);
        }

        // Update statistics
        {
            let mut stats = self.stats.lock().await;
            stats.total_applies += 1;
            stats.pending_applies += 1;
        }

        debug!("Apply operation queued for tick boundary swap");
        Ok(response_rx)
    }

    /// Process pending applies at tick boundary (called from RT thread)
    /// 
    /// This method must be called from the RT thread at tick boundaries
    /// to ensure atomic pipeline swaps without disrupting the 1kHz loop.
    pub async fn process_pending_applies_at_tick_boundary(&self) {
        let pending_applies = {
            let mut pending = self.pending_applies.lock().await;
            std::mem::take(&mut *pending)
        };

        if pending_applies.is_empty() {
            return;
        }

        debug!("Processing {} pending applies at tick boundary", pending_applies.len());

        for pending_apply in pending_applies {
            let swap_start = Instant::now();
            
            // Extract values before moving
            let config_hash = pending_apply.compiled_pipeline.config_hash;
            let merge_hash = pending_apply.merge_result.merge_hash;
            let node_count = pending_apply.compiled_pipeline.pipeline.node_count();

            // Phase 3: Atomic swap at tick boundary
            let swap_result = self.swap_pipeline_atomic(pending_apply.compiled_pipeline.pipeline).await;
            
            let swap_time = swap_start.elapsed();
            let total_time = pending_apply.requested_at.elapsed();

            // Create apply result
            let apply_result = match swap_result {
                Ok(()) => {
                    info!("Pipeline swap successful: total time {}ms, swap time {}μs", 
                          total_time.as_millis(), swap_time.as_micros());

                    ApplyResult {
                        success: true,
                        config_hash,
                        merge_hash,
                        duration_ms: total_time.as_millis() as u64,
                        error: None,
                        stats: ApplyOperationStats {
                            compilation_time_ms: 0, // TODO: Track this properly
                            wait_time_ms: total_time.as_millis() as u64,
                            swap_time_us: swap_time.as_micros() as u64,
                            node_count,
                            state_size_bytes: 0, // TODO: Calculate this
                        },
                    }
                }
                Err(e) => {
                    error!("Pipeline swap failed: {}", e);

                    ApplyResult {
                        success: false,
                        config_hash: 0,
                        merge_hash: 0,
                        duration_ms: total_time.as_millis() as u64,
                        error: Some(e.to_string()),
                        stats: ApplyOperationStats {
                            compilation_time_ms: 0,
                            wait_time_ms: total_time.as_millis() as u64,
                            swap_time_us: swap_time.as_micros() as u64,
                            node_count: 0,
                            state_size_bytes: 0,
                        },
                    }
                }
            };

            // Update statistics
            {
                let mut stats = self.stats.lock().await;
                stats.pending_applies -= 1;
                
                if apply_result.success {
                    stats.successful_applies += 1;
                    stats.avg_swap_time_us = (stats.avg_swap_time_us + apply_result.stats.swap_time_us as f64) / 2.0;
                    stats.max_swap_time_us = stats.max_swap_time_us.max(apply_result.stats.swap_time_us);
                } else {
                    stats.failed_applies += 1;
                }
            }

            // Phase 4: Send acknowledgment to UI
            let _ = pending_apply.response_tx.send(apply_result);
        }
    }

    /// Swap pipeline atomically (RT-safe)
    async fn swap_pipeline_atomic(&self, new_pipeline: Pipeline) -> Result<(), PipelineError> {
        // This operation must be atomic from the RT thread's perspective
        let mut active = self.active_pipeline.write().await;
        active.swap_at_tick_boundary(new_pipeline);
        Ok(())
    }

    /// Get the currently active pipeline (for RT thread)
    pub fn get_active_pipeline(&self) -> Arc<RwLock<Pipeline>> {
        Arc::clone(&self.active_pipeline)
    }

    /// Get apply statistics
    pub async fn get_stats(&self) -> ApplyStats {
        let stats = self.stats.lock().await;
        stats.clone()
    }

    /// Clear apply statistics
    pub async fn clear_stats(&self) {
        let mut stats = self.stats.lock().await;
        *stats = ApplyStats::default();
    }

    /// Check if there are pending applies
    pub async fn has_pending_applies(&self) -> bool {
        let pending = self.pending_applies.lock().await;
        !pending.is_empty()
    }

    /// Get the number of pending applies
    pub async fn pending_apply_count(&self) -> usize {
        let pending = self.pending_applies.lock().await;
        pending.len()
    }
}

impl Default for ApplyStats {
    fn default() -> Self {
        Self {
            total_applies: 0,
            successful_applies: 0,
            failed_applies: 0,
            avg_compilation_time_ms: 0.0,
            avg_swap_time_us: 0.0,
            max_swap_time_us: 0,
            pending_applies: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use racing_wheel_schemas::{
        ProfileId, ProfileScope, TorqueNm, Degrees, Gain, FilterConfig
    };

    fn create_test_profile(id: &str, scope: ProfileScope) -> Profile {
        Profile::new(
            ProfileId::new(id.to_string()).unwrap(),
            scope,
            BaseSettings::default(),
            format!("Test Profile {}", id),
        )
    }

    #[tokio::test]
    async fn test_two_phase_apply_basic() {
        let initial_pipeline = Pipeline::new();
        let coordinator = TwoPhaseApplyCoordinator::new(initial_pipeline);

        let global_profile = create_test_profile("global", ProfileScope::global());

        // Start apply operation
        let result_rx = coordinator.apply_profile_async(&global_profile, None, None, None).await;
        assert!(result_rx.is_ok());

        // Process pending applies
        coordinator.process_pending_applies_at_tick_boundary().await;

        // Check result
        let result = result_rx.unwrap().await;
        assert!(result.is_ok());
        
        let apply_result = result.unwrap();
        assert!(apply_result.success);
        assert!(apply_result.config_hash != 0);
    }

    #[tokio::test]
    async fn test_two_phase_apply_with_hierarchy() {
        let initial_pipeline = Pipeline::new();
        let coordinator = TwoPhaseApplyCoordinator::new(initial_pipeline);

        let global_profile = create_test_profile("global", ProfileScope::global());
        let mut game_profile = create_test_profile("iracing", ProfileScope::for_game("iracing".to_string()));
        
        // Modify game profile
        game_profile.base_settings.ffb_gain = Gain::new(0.8).unwrap();

        // Start apply operation
        let result_rx = coordinator.apply_profile_async(
            &global_profile, 
            Some(&game_profile), 
            None, 
            None
        ).await;
        assert!(result_rx.is_ok());

        // Process pending applies
        coordinator.process_pending_applies_at_tick_boundary().await;

        // Check result
        let result = result_rx.unwrap().await;
        assert!(result.is_ok());
        
        let apply_result = result.unwrap();
        assert!(apply_result.success);
        assert!(apply_result.merge_hash != 0);
    }

    #[tokio::test]
    async fn test_two_phase_apply_with_session_overrides() {
        let initial_pipeline = Pipeline::new();
        let coordinator = TwoPhaseApplyCoordinator::new(initial_pipeline);

        let global_profile = create_test_profile("global", ProfileScope::global());
        let session_overrides = BaseSettings::new(
            Gain::new(0.9).unwrap(),
            Degrees::new_dor(540.0).unwrap(),
            TorqueNm::new(20.0).unwrap(),
            FilterConfig::default(),
        );

        // Start apply operation
        let result_rx = coordinator.apply_profile_async(
            &global_profile, 
            None, 
            None, 
            Some(&session_overrides)
        ).await;
        assert!(result_rx.is_ok());

        // Process pending applies
        coordinator.process_pending_applies_at_tick_boundary().await;

        // Check result
        let result = result_rx.unwrap().await;
        assert!(result.is_ok());
        
        let apply_result = result.unwrap();
        assert!(apply_result.success);
    }

    #[tokio::test]
    async fn test_two_phase_apply_statistics() {
        let initial_pipeline = Pipeline::new();
        let coordinator = TwoPhaseApplyCoordinator::new(initial_pipeline);

        let global_profile = create_test_profile("global", ProfileScope::global());

        // Check initial stats
        let initial_stats = coordinator.get_stats().await;
        assert_eq!(initial_stats.total_applies, 0);
        assert_eq!(initial_stats.successful_applies, 0);

        // Perform apply
        let result_rx = coordinator.apply_profile_async(&global_profile, None, None, None).await.unwrap();
        
        // Check pending stats
        let pending_stats = coordinator.get_stats().await;
        assert_eq!(pending_stats.total_applies, 1);
        assert_eq!(pending_stats.pending_applies, 1);

        // Process applies
        coordinator.process_pending_applies_at_tick_boundary().await;
        let _ = result_rx.await;

        // Check final stats
        let final_stats = coordinator.get_stats().await;
        assert_eq!(final_stats.total_applies, 1);
        assert_eq!(final_stats.successful_applies, 1);
        assert_eq!(final_stats.pending_applies, 0);
    }

    #[tokio::test]
    async fn test_two_phase_apply_multiple_pending() {
        let initial_pipeline = Pipeline::new();
        let coordinator = TwoPhaseApplyCoordinator::new(initial_pipeline);

        let global_profile = create_test_profile("global", ProfileScope::global());

        // Start multiple apply operations
        let result_rx1 = coordinator.apply_profile_async(&global_profile, None, None, None).await.unwrap();
        let result_rx2 = coordinator.apply_profile_async(&global_profile, None, None, None).await.unwrap();

        // Check pending count
        assert_eq!(coordinator.pending_apply_count().await, 2);

        // Process all pending applies
        coordinator.process_pending_applies_at_tick_boundary().await;

        // Check results
        let result1 = result_rx1.await.unwrap();
        let result2 = result_rx2.await.unwrap();
        
        assert!(result1.success);
        assert!(result2.success);
        
        // Check no more pending
        assert_eq!(coordinator.pending_apply_count().await, 0);
    }

    #[tokio::test]
    async fn test_two_phase_apply_deterministic() {
        let initial_pipeline = Pipeline::new();
        let coordinator = TwoPhaseApplyCoordinator::new(initial_pipeline);

        let global_profile = create_test_profile("global", ProfileScope::global());

        // Perform same apply twice
        let result_rx1 = coordinator.apply_profile_async(&global_profile, None, None, None).await.unwrap();
        coordinator.process_pending_applies_at_tick_boundary().await;
        let result1 = result_rx1.await.unwrap();

        let result_rx2 = coordinator.apply_profile_async(&global_profile, None, None, None).await.unwrap();
        coordinator.process_pending_applies_at_tick_boundary().await;
        let result2 = result_rx2.await.unwrap();

        // Results should have same hashes (deterministic)
        assert_eq!(result1.config_hash, result2.config_hash);
        assert_eq!(result1.merge_hash, result2.merge_hash);
    }
}