//! Filter pipeline for real-time force feedback processing

use crate::rt::Frame;
use crate::rt::RTResult;
use racing_wheel_schemas::prelude::*;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};
use tracing::debug;

/// Function pointer type for filter nodes
pub type FilterNodeFn = fn(&mut Frame, *mut u8);

/// Compiled filter pipeline with zero-allocation execution
#[derive(Debug)]
pub struct Pipeline {
    /// Function pointers for each filter node
    nodes: Vec<FilterNodeFn>,
    /// State storage for all nodes (Structure of Arrays)
    state: Vec<u8>,
    /// Offsets into state storage for each node
    state_offsets: Vec<usize>,
    /// Configuration hash for deterministic comparison
    config_hash: u64,
}

/// Pipeline compilation result
#[derive(Debug)]
pub struct CompiledPipeline {
    /// The compiled pipeline ready for RT execution
    pub pipeline: Pipeline,
    /// Configuration hash for change detection
    pub config_hash: u64,
}

/// Pipeline compiler for converting FilterConfig to executable pipeline
pub struct PipelineCompiler {
    /// Pending compilation tasks
    pending_compilations: Arc<Mutex<Vec<CompilationTask>>>,
}

/// Internal compilation task
struct CompilationTask {
    config: FilterConfig,
    response_tx: oneshot::Sender<Result<CompiledPipeline, PipelineError>>,
}

/// Pipeline compilation and execution errors
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("Invalid filter configuration: {0}")]
    InvalidConfig(String),

    #[error("Compilation failed: {0}")]
    CompilationFailed(String),

    #[error("Pipeline swap failed: {0}")]
    SwapFailed(String),

    #[error("Non-monotonic curve points")]
    NonMonotonicCurve,

    #[error("Invalid filter parameters: {0}")]
    InvalidParameters(String),
}

impl Pipeline {
    /// Create empty pipeline
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            state: Vec::new(),
            state_offsets: Vec::new(),
            config_hash: 0,
        }
    }

    /// Create pipeline with specific configuration hash
    pub fn with_hash(config_hash: u64) -> Self {
        Self {
            nodes: Vec::new(),
            state: Vec::new(),
            state_offsets: Vec::new(),
            config_hash,
        }
    }

    /// Process frame through pipeline (RT-safe, no allocations)
    #[inline]
    pub fn process(&mut self, frame: &mut Frame) -> RTResult {
        // Ensure we don't allocate on the hot path
        #[cfg(debug_assertions)]
        {
            let _alloc_guard = crate::allocation_tracker::track();
            // Process the pipeline
            self.process_internal(frame)?;
            // Assert no allocations occurred
            crate::assert_zero_alloc!(_alloc_guard, "Pipeline hot path allocated memory");
            Ok(())
        }

        #[cfg(not(debug_assertions))]
        {
            self.process_internal(frame)
        }
    }

    /// Internal processing method (separated for allocation tracking)
    #[inline]
    fn process_internal(&mut self, frame: &mut Frame) -> RTResult {
        for (i, &node_fn) in self.nodes.iter().enumerate() {
            let state_ptr = unsafe { self.state.as_mut_ptr().add(self.state_offsets[i]) };

            // Call filter node function
            node_fn(frame, state_ptr);

            // Validate output is within bounds
            if !frame.torque_out.is_finite() || frame.torque_out.abs() > 1.0 {
                return Err(crate::RTError::PipelineFault);
            }
        }

        Ok(())
    }

    /// Swap pipeline at tick boundary (RT-safe, atomic)
    pub fn swap_at_tick_boundary(&mut self, new_pipeline: Pipeline) {
        // This is atomic from the RT thread's perspective
        *self = new_pipeline;
    }

    /// Get the configuration hash for this pipeline
    pub fn config_hash(&self) -> u64 {
        self.config_hash
    }

    /// Check if pipeline is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get the number of filter nodes
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Add a filter node to the pipeline (used during compilation)
    fn add_node(&mut self, node_fn: FilterNodeFn, state_size: usize) {
        // Ensure proper alignment for the state data
        let align = std::mem::align_of::<f64>(); // Use f64 alignment for safety
        let current_len = self.state.len();
        let aligned_offset = (current_len + align - 1) & !(align - 1);

        // Pad to alignment boundary
        self.state.resize(aligned_offset, 0);
        self.state_offsets.push(aligned_offset);

        // Add the actual state data
        self.state.resize(aligned_offset + state_size, 0);
        self.nodes.push(node_fn);
    }

    /// Initialize state for a specific node (used during compilation)
    fn init_node_state<T>(&mut self, node_index: usize, initial_state: T)
    where
        T: Copy,
    {
        if node_index < self.state_offsets.len() {
            let offset = self.state_offsets[node_index];

            // Verify alignment
            assert_eq!(
                offset % std::mem::align_of::<T>(),
                0,
                "State offset {} is not aligned for type {}",
                offset,
                std::any::type_name::<T>()
            );

            let state_ptr = unsafe { self.state.as_mut_ptr().add(offset) as *mut T };
            unsafe {
                *state_ptr = initial_state;
            }
        }
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineCompiler {
    /// Create a new pipeline compiler
    pub fn new() -> Self {
        Self {
            pending_compilations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Compile a FilterConfig into an executable pipeline (off-thread)
    pub async fn compile_pipeline(
        &self,
        config: FilterConfig,
    ) -> Result<CompiledPipeline, PipelineError> {
        debug!("Compiling pipeline from FilterConfig");

        // Validate configuration first
        self.validate_config(&config)?;

        // Calculate deterministic hash of the configuration
        let config_hash = self.calculate_config_hash(&config);

        // Create new pipeline
        let mut pipeline = Pipeline::with_hash(config_hash);

        // Add filter nodes in the correct order
        self.add_reconstruction_filter(&mut pipeline, config.reconstruction)?;
        self.add_friction_filter(&mut pipeline, config.friction)?;
        self.add_damper_filter(&mut pipeline, config.damper)?;
        self.add_inertia_filter(&mut pipeline, config.inertia)?;
        self.add_notch_filters(&mut pipeline, &config.notch_filters)?;
        self.add_slew_rate_filter(&mut pipeline, config.slew_rate)?;
        self.add_curve_filter(&mut pipeline, &config.curve_points)?;

        // Add safety and model filters
        self.add_torque_cap_filter(&mut pipeline, config.torque_cap.value())?;
        self.add_bumpstop_filter(&mut pipeline, &config.bumpstop)?;
        self.add_hands_off_detector(&mut pipeline, &config.hands_off)?;

        debug!(
            "Pipeline compiled successfully with {} nodes, hash: {:x}",
            pipeline.node_count(),
            config_hash
        );

        Ok(CompiledPipeline {
            pipeline,
            config_hash,
        })
    }

    /// Compile pipeline asynchronously and return immediately
    pub async fn compile_pipeline_async(
        &self,
        config: FilterConfig,
    ) -> Result<oneshot::Receiver<Result<CompiledPipeline, PipelineError>>, PipelineError> {
        let (tx, rx) = oneshot::channel();

        let task = CompilationTask {
            config,
            response_tx: tx,
        };

        {
            let mut pending = self.pending_compilations.lock().await;
            pending.push(task);
        }

        // Spawn compilation task
        let compiler = self.clone();
        tokio::spawn(async move {
            compiler.process_pending_compilations().await;
        });

        Ok(rx)
    }

    /// Process all pending compilation tasks
    async fn process_pending_compilations(&self) {
        let tasks = {
            let mut pending = self.pending_compilations.lock().await;
            std::mem::take(&mut *pending)
        };

        for task in tasks {
            let result = self.compile_pipeline(task.config).await;
            let _ = task.response_tx.send(result);
        }
    }

    /// Validate filter configuration
    fn validate_config(&self, config: &FilterConfig) -> Result<(), PipelineError> {
        // Validate reconstruction level
        if config.reconstruction > 8 {
            return Err(PipelineError::InvalidConfig(format!(
                "Reconstruction level must be 0-8, got {}",
                config.reconstruction
            )));
        }

        // Validate gain values are in valid range
        if !(0.0..=1.0).contains(&config.friction.value()) {
            return Err(PipelineError::InvalidParameters(format!(
                "Friction must be 0.0-1.0, got {}",
                config.friction.value()
            )));
        }

        if !(0.0..=1.0).contains(&config.damper.value()) {
            return Err(PipelineError::InvalidParameters(format!(
                "Damper must be 0.0-1.0, got {}",
                config.damper.value()
            )));
        }

        if !(0.0..=1.0).contains(&config.inertia.value()) {
            return Err(PipelineError::InvalidParameters(format!(
                "Inertia must be 0.0-1.0, got {}",
                config.inertia.value()
            )));
        }

        if !(0.0..=1.0).contains(&config.slew_rate.value()) {
            return Err(PipelineError::InvalidParameters(format!(
                "Slew rate must be 0.0-1.0, got {}",
                config.slew_rate.value()
            )));
        }

        // Validate curve points are monotonic
        self.validate_curve_monotonic(&config.curve_points)?;

        // Validate notch filters
        for (i, filter) in config.notch_filters.iter().enumerate() {
            if !((0.0..=500.0).contains(&filter.frequency.value())
                && filter.frequency.value() > 0.0)
            {
                return Err(PipelineError::InvalidParameters(format!(
                    "Notch filter {} frequency must be 0-500 Hz, got {}",
                    i,
                    filter.frequency.value()
                )));
            }

            if !((0.0..=20.0).contains(&filter.q_factor) && filter.q_factor > 0.0) {
                return Err(PipelineError::InvalidParameters(format!(
                    "Notch filter {} Q factor must be 0-20, got {}",
                    i, filter.q_factor
                )));
            }
        }

        Ok(())
    }

    /// Validate that curve points are monotonic
    fn validate_curve_monotonic(&self, curve_points: &[CurvePoint]) -> Result<(), PipelineError> {
        if curve_points.len() < 2 {
            return Err(PipelineError::InvalidConfig(
                "Curve must have at least 2 points".to_string(),
            ));
        }

        for window in curve_points.windows(2) {
            if window[1].input <= window[0].input {
                return Err(PipelineError::NonMonotonicCurve);
            }
        }

        // Ensure curve starts at 0 and ends at 1
        // We already checked curve_points is non-empty above
        let first = &curve_points[0];
        let last = &curve_points[curve_points.len() - 1];

        if first.input != 0.0 {
            return Err(PipelineError::InvalidConfig(
                "Curve must start at input 0.0".to_string(),
            ));
        }

        if last.input != 1.0 {
            return Err(PipelineError::InvalidConfig(
                "Curve must end at input 1.0".to_string(),
            ));
        }

        Ok(())
    }

    /// Calculate deterministic hash of filter configuration
    fn calculate_config_hash(&self, config: &FilterConfig) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash all configuration parameters that affect pipeline behavior
        config.reconstruction.hash(&mut hasher);
        config.friction.value().to_bits().hash(&mut hasher);
        config.damper.value().to_bits().hash(&mut hasher);
        config.inertia.value().to_bits().hash(&mut hasher);
        config.slew_rate.value().to_bits().hash(&mut hasher);

        // Hash curve points
        for point in &config.curve_points {
            point.input.to_bits().hash(&mut hasher);
            point.output.to_bits().hash(&mut hasher);
        }

        // Hash notch filters
        for filter in &config.notch_filters {
            filter.frequency.value().to_bits().hash(&mut hasher);
            filter.q_factor.to_bits().hash(&mut hasher);
            filter.gain_db.to_bits().hash(&mut hasher);
        }

        hasher.finish()
    }

    /// Add reconstruction filter to pipeline
    fn add_reconstruction_filter(
        &self,
        pipeline: &mut Pipeline,
        level: u8,
    ) -> Result<(), PipelineError> {
        if level == 0 {
            return Ok(()); // No reconstruction filter
        }

        // Add reconstruction filter node with appropriate state
        pipeline.add_node(
            crate::filters::reconstruction_filter,
            std::mem::size_of::<crate::filters::ReconstructionState>(),
        );
        let node_index = pipeline.nodes.len() - 1;

        let state = crate::filters::ReconstructionState::new(level);
        pipeline.init_node_state(node_index, state);
        Ok(())
    }

    /// Add friction filter to pipeline
    fn add_friction_filter(
        &self,
        pipeline: &mut Pipeline,
        friction: Gain,
    ) -> Result<(), PipelineError> {
        if friction.value() == 0.0 {
            return Ok(()); // No friction
        }

        pipeline.add_node(
            crate::filters::friction_filter,
            std::mem::size_of::<crate::filters::FrictionState>(),
        );
        let node_index = pipeline.nodes.len() - 1;

        let state = crate::filters::FrictionState::new(friction.value(), true); // Enable speed adaptation
        pipeline.init_node_state(node_index, state);
        Ok(())
    }

    /// Add damper filter to pipeline
    fn add_damper_filter(
        &self,
        pipeline: &mut Pipeline,
        damper: Gain,
    ) -> Result<(), PipelineError> {
        if damper.value() == 0.0 {
            return Ok(()); // No damping
        }

        pipeline.add_node(
            crate::filters::damper_filter,
            std::mem::size_of::<crate::filters::DamperState>(),
        );
        let node_index = pipeline.nodes.len() - 1;

        let state = crate::filters::DamperState::new(damper.value(), true); // Enable speed adaptation
        pipeline.init_node_state(node_index, state);
        Ok(())
    }

    /// Add inertia filter to pipeline
    fn add_inertia_filter(
        &self,
        pipeline: &mut Pipeline,
        inertia: Gain,
    ) -> Result<(), PipelineError> {
        if inertia.value() == 0.0 {
            return Ok(()); // No inertia
        }

        pipeline.add_node(
            crate::filters::inertia_filter,
            std::mem::size_of::<crate::filters::InertiaState>(),
        );
        let node_index = pipeline.nodes.len() - 1;

        let state = crate::filters::InertiaState::new(inertia.value());
        pipeline.init_node_state(node_index, state);
        Ok(())
    }

    /// Add notch filters to pipeline
    fn add_notch_filters(
        &self,
        pipeline: &mut Pipeline,
        filters: &[NotchFilter],
    ) -> Result<(), PipelineError> {
        for filter in filters {
            pipeline.add_node(
                crate::filters::notch_filter,
                std::mem::size_of::<crate::filters::NotchState>(),
            );
            let node_index = pipeline.nodes.len() - 1;

            let state = crate::filters::NotchState::new(
                filter.frequency.value(),
                filter.q_factor,
                filter.gain_db,
                1000.0, // 1kHz sample rate
            );

            pipeline.init_node_state(node_index, state);
        }
        Ok(())
    }

    /// Add slew rate limiter to pipeline
    fn add_slew_rate_filter(
        &self,
        pipeline: &mut Pipeline,
        slew_rate: Gain,
    ) -> Result<(), PipelineError> {
        if slew_rate.value() >= 1.0 {
            return Ok(()); // No slew rate limiting
        }

        pipeline.add_node(
            crate::filters::slew_rate_filter,
            std::mem::size_of::<crate::filters::SlewRateState>(),
        );
        let node_index = pipeline.nodes.len() - 1;

        let state = crate::filters::SlewRateState::new(slew_rate.value());
        pipeline.init_node_state(node_index, state);
        Ok(())
    }

    /// Add curve mapping filter to pipeline
    fn add_curve_filter(
        &self,
        pipeline: &mut Pipeline,
        curve_points: &[CurvePoint],
    ) -> Result<(), PipelineError> {
        if curve_points.len() == 2
            && curve_points[0].input == 0.0
            && curve_points[0].output == 0.0
            && curve_points[1].input == 1.0
            && curve_points[1].output == 1.0
        {
            return Ok(()); // Linear curve, no filtering needed
        }

        pipeline.add_node(
            crate::filters::curve_filter,
            std::mem::size_of::<crate::filters::CurveState>(),
        );
        let node_index = pipeline.nodes.len() - 1;

        // Convert CurvePoint to tuple format for the filter
        let curve_tuples: Vec<(f32, f32)> =
            curve_points.iter().map(|p| (p.input, p.output)).collect();

        let state = crate::filters::CurveState::new(&curve_tuples);
        pipeline.init_node_state(node_index, state);
        Ok(())
    }

    /// Add torque cap filter to pipeline
    fn add_torque_cap_filter(
        &self,
        pipeline: &mut Pipeline,
        torque_cap: f32,
    ) -> Result<(), PipelineError> {
        if torque_cap >= 1.0 {
            return Ok(()); // No torque limiting needed
        }

        pipeline.add_node(
            crate::filters::torque_cap_filter,
            std::mem::size_of::<f32>(),
        );
        let node_index = pipeline.nodes.len() - 1;
        pipeline.init_node_state(node_index, torque_cap);
        Ok(())
    }

    /// Add bumpstop model filter to pipeline
    fn add_bumpstop_filter(
        &self,
        pipeline: &mut Pipeline,
        bumpstop_config: &BumpstopConfig,
    ) -> Result<(), PipelineError> {
        if !bumpstop_config.enabled {
            return Ok(()); // Bumpstop disabled
        }

        pipeline.add_node(
            crate::filters::bumpstop_filter,
            std::mem::size_of::<crate::filters::BumpstopState>(),
        );
        let node_index = pipeline.nodes.len() - 1;

        let state = crate::filters::BumpstopState::new(
            bumpstop_config.enabled,
            bumpstop_config.start_angle,
            bumpstop_config.max_angle,
            bumpstop_config.stiffness,
            bumpstop_config.damping,
        );

        pipeline.init_node_state(node_index, state);
        Ok(())
    }

    /// Add hands-off detector to pipeline
    fn add_hands_off_detector(
        &self,
        pipeline: &mut Pipeline,
        config: &HandsOffConfig,
    ) -> Result<(), PipelineError> {
        if !config.enabled {
            return Ok(()); // Hands-off detection disabled
        }

        pipeline.add_node(
            crate::filters::hands_off_detector,
            std::mem::size_of::<crate::filters::HandsOffState>(),
        );
        let node_index = pipeline.nodes.len() - 1;

        let state = crate::filters::HandsOffState::new(
            config.enabled,
            config.threshold,
            config.timeout_seconds,
        );

        pipeline.init_node_state(node_index, state);
        Ok(())
    }

    /// Interpolate curve value for a given input
    /// TODO: Used for future curve-based FFB effects implementation
    #[allow(dead_code)]
    fn interpolate_curve(&self, input: f32, curve_points: &[CurvePoint]) -> f32 {
        let clamped_input = input.clamp(0.0, 1.0);

        // Find the two points to interpolate between
        for window in curve_points.windows(2) {
            if clamped_input >= window[0].input && clamped_input <= window[1].input {
                let t = (clamped_input - window[0].input) / (window[1].input - window[0].input);
                return window[0].output + t * (window[1].output - window[0].output);
            }
        }

        // Fallback (shouldn't happen with valid curve)
        clamped_input
    }
}

impl Clone for PipelineCompiler {
    fn clone(&self) -> Self {
        Self {
            pending_compilations: Arc::clone(&self.pending_compilations),
        }
    }
}

impl Default for PipelineCompiler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use crate::filters::*;

    #[track_caller]
    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("unexpected Err: {e:?}"),
        }
    }

    fn create_test_filter_config() -> FilterConfig {
        FilterConfig::new_complete(
            4,                     // reconstruction
            must(Gain::new(0.1)),  // friction
            must(Gain::new(0.15)), // damper
            must(Gain::new(0.05)), // inertia
            vec![must(NotchFilter::new(
                must(FrequencyHz::new(60.0)),
                2.0,
                -12.0,
            ))],
            must(Gain::new(0.8)), // slew_rate
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(0.5, 0.6)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(0.9)), // torque_cap
            BumpstopConfig::default(),
            HandsOffConfig::default(),
        )
        .unwrap()
    }

    fn create_linear_filter_config() -> FilterConfig {
        FilterConfig::new_complete(
            0,                    // no reconstruction
            must(Gain::new(0.0)), // no friction
            must(Gain::new(0.0)), // no damper
            must(Gain::new(0.0)), // no inertia
            vec![],               // no notch filters
            must(Gain::new(1.0)), // no slew rate limiting
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(1.0)), // no torque cap
            BumpstopConfig::default(),
            HandsOffConfig::default(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn test_pipeline_compilation_basic() {
        let compiler = PipelineCompiler::new();
        let config = create_test_filter_config();

        let result = compiler.compile_pipeline(config).await;
        assert!(result.is_ok());

        let compiled = result.unwrap();
        assert!(compiled.pipeline.node_count() > 0);
        assert!(compiled.config_hash != 0);
    }

    #[tokio::test]
    async fn test_pipeline_compilation_deterministic() {
        let compiler = PipelineCompiler::new();
        let config = create_test_filter_config();

        // Compile the same config twice
        let result1 = compiler.compile_pipeline(config.clone()).await.unwrap();
        let result2 = compiler.compile_pipeline(config).await.unwrap();

        // Should produce identical hashes
        assert_eq!(result1.config_hash, result2.config_hash);
        assert_eq!(result1.pipeline.node_count(), result2.pipeline.node_count());
    }

    #[tokio::test]
    async fn test_pipeline_compilation_different_configs() {
        let compiler = PipelineCompiler::new();
        let config1 = create_test_filter_config();
        let config2 = create_linear_filter_config();

        let result1 = compiler.compile_pipeline(config1).await.unwrap();
        let result2 = compiler.compile_pipeline(config2).await.unwrap();

        // Should produce different hashes
        assert_ne!(result1.config_hash, result2.config_hash);
    }

    #[test]
    fn test_pipeline_processing_zero_alloc() {
        let mut pipeline = Pipeline::new();
        let mut frame = crate::rt::Frame {
            ffb_in: 0.5,
            torque_out: 0.0,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 1,
        };

        // Track allocations during processing
        #[cfg(debug_assertions)]
        {
            let alloc_guard = crate::allocation_tracker::track();
            let result = pipeline.process(&mut frame);
            assert!(result.is_ok());

            // Assert no allocations occurred
            crate::assert_zero_alloc!(alloc_guard, "Pipeline processing allocated memory");
        }

        #[cfg(not(debug_assertions))]
        {
            let result = pipeline.process(&mut frame);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_pipeline_swap_atomicity() {
        let mut pipeline1 = Pipeline::new();
        let pipeline2 = Pipeline::with_hash(0x12345678);

        // Verify initial state
        assert_eq!(pipeline1.config_hash(), 0);
        assert_eq!(pipeline1.node_count(), 0);

        // Perform atomic swap
        pipeline1.swap_at_tick_boundary(pipeline2);

        // Verify swap completed atomically
        assert_eq!(pipeline1.config_hash(), 0x12345678);
    }

    #[tokio::test]
    async fn test_pipeline_validation_invalid_config() {
        let compiler = PipelineCompiler::new();

        // Create invalid config with reconstruction level too high
        let invalid_config_result = FilterConfig::new_complete(
            10, // Invalid: > 8
            must(Gain::new(0.1)),
            must(Gain::new(0.15)),
            must(Gain::new(0.05)),
            vec![],
            must(Gain::new(0.8)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(1.0)),
            BumpstopConfig::default(),
            HandsOffConfig::default(),
        );

        // Should fail validation
        assert!(invalid_config_result.is_err());

        // For the compiler test, use a valid config that will fail compilation validation
        let mut invalid_config = create_test_filter_config();
        invalid_config.reconstruction = 10;

        let result = compiler.compile_pipeline(invalid_config).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            PipelineError::InvalidConfig(_) => {} // Expected
            _ => panic!("Expected InvalidConfig error"),
        }
    }

    #[tokio::test]
    async fn test_pipeline_validation_non_monotonic_curve() {
        let compiler = PipelineCompiler::new();

        // Create config with non-monotonic curve - this should fail at construction
        let invalid_config_result = FilterConfig::new_complete(
            4,
            must(Gain::new(0.1)),
            must(Gain::new(0.15)),
            must(Gain::new(0.05)),
            vec![],
            must(Gain::new(0.8)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(0.7, 0.6)),
                must(CurvePoint::new(0.5, 0.8)), // Non-monotonic!
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(1.0)),
            BumpstopConfig::default(),
            HandsOffConfig::default(),
        );

        // Should fail at construction due to non-monotonic curve
        assert!(invalid_config_result.is_err());

        // For the compiler test, create a valid config
        let valid_config = create_test_filter_config();

        let result = compiler.compile_pipeline(valid_config).await;
        assert!(result.is_ok()); // Should succeed with valid config
    }

    #[tokio::test]
    async fn test_pipeline_validation_invalid_parameters() {
        let compiler = PipelineCompiler::new();

        // Create config with invalid parameters - high frequency notch filter
        let invalid_config = FilterConfig::new_complete(
            4,
            must(Gain::new(0.1)),
            must(Gain::new(0.15)),
            must(Gain::new(0.05)),
            vec![must(NotchFilter::new(
                must(FrequencyHz::new(600.0)), // Too high frequency
                2.0,
                -12.0,
            ))],
            must(Gain::new(0.8)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(1.0)),
            BumpstopConfig::default(),
            HandsOffConfig::default(),
        )
        .unwrap();

        let result = compiler.compile_pipeline(invalid_config).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            PipelineError::InvalidParameters(_) => {} // Expected
            _ => panic!("Expected InvalidParameters error"),
        }
    }

    #[test]
    fn test_filter_nodes_bounds_checking() {
        let mut frame = crate::rt::Frame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 10.0, // rad/s
            hands_off: false,
            ts_mono_ns: 0,
            seq: 1,
        };

        // Test friction filter
        let friction_coeff = 0.2f32;
        let state_ptr = &friction_coeff as *const f32 as *mut u8;
        friction_filter(&mut frame, state_ptr);

        // Output should be bounded
        assert!(frame.torque_out.is_finite());
        assert!(frame.torque_out.abs() <= 2.0); // Reasonable bound

        // Test with extreme wheel speed
        frame.wheel_speed = 1000.0;
        friction_filter(&mut frame, state_ptr);
        assert!(frame.torque_out.is_finite());
    }

    #[test]
    fn test_curve_filter_lookup_table() {
        let mut frame = crate::rt::Frame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 1,
        };

        // Create a curve state with a quadratic curve
        let curve_points = vec![(0.0, 0.0), (0.5, 0.25), (1.0, 1.0)];
        let mut curve_state = CurveState::new(&curve_points);

        let state_ptr = &mut curve_state as *mut CurveState as *mut u8;
        curve_filter(&mut frame, state_ptr);

        // Should apply quadratic mapping: 0.5^2 = 0.25
        assert!((frame.torque_out.abs() - 0.25).abs() < 0.1);
    }

    #[test]
    fn test_slew_rate_limiter() {
        let mut slew_state = SlewRateState::new(100.0); // 100% slew rate = 0.1 per tick

        let mut frame = crate::rt::Frame {
            ffb_in: 0.5,
            torque_out: 1.0, // Large jump
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 1,
        };

        let state_ptr = &mut slew_state as *mut SlewRateState as *mut u8;
        slew_rate_filter(&mut frame, state_ptr);

        // Should be limited to max_change_per_tick
        assert!((frame.torque_out - 0.1).abs() < 0.01);

        // Apply again - should continue ramping
        frame.torque_out = 1.0;
        slew_rate_filter(&mut frame, state_ptr);
        assert!((frame.torque_out - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_notch_filter_stability() {
        let mut notch_state = NotchState::new(60.0, 2.0, -12.0, 1000.0);

        let mut frame = crate::rt::Frame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 1,
        };

        let state_ptr = &mut notch_state as *mut NotchState as *mut u8;

        // Apply filter multiple times to check stability
        for _ in 0..100 {
            notch_filter(&mut frame, state_ptr);
            assert!(frame.torque_out.is_finite());
            assert!(frame.torque_out.abs() < 10.0); // Reasonable bound
        }
    }

    #[tokio::test]
    async fn test_pipeline_async_compilation() {
        let compiler = PipelineCompiler::new();
        let config = create_test_filter_config();

        // Test async compilation
        let rx = compiler.compile_pipeline_async(config).await.unwrap();
        let result = rx.await.unwrap();

        assert!(result.is_ok());
        let compiled = result.unwrap();
        assert!(compiled.pipeline.node_count() > 0);
    }

    #[test]
    fn test_pipeline_empty_state() {
        let pipeline = Pipeline::new();
        assert!(pipeline.is_empty());
        assert_eq!(pipeline.node_count(), 0);
        assert_eq!(pipeline.config_hash(), 0);
    }

    #[test]
    fn test_pipeline_with_hash() {
        let hash = 0xDEADBEEF;
        let pipeline = Pipeline::with_hash(hash);
        assert_eq!(pipeline.config_hash(), hash);
        assert!(pipeline.is_empty());
    }

    // Performance test to ensure compilation is reasonably fast
    #[tokio::test]
    async fn test_pipeline_compilation_performance() {
        let compiler = PipelineCompiler::new();
        let config = create_test_filter_config();

        let start = std::time::Instant::now();

        // Compile multiple pipelines
        for _ in 0..10 {
            let result = compiler.compile_pipeline(config.clone()).await;
            assert!(result.is_ok());
        }

        let duration = start.elapsed();

        // Should complete within reasonable time (adjust as needed)
        assert!(
            duration.as_millis() < 100,
            "Compilation took too long: {:?}",
            duration
        );
    }
}
