//! Filter pipeline for real-time force feedback processing

use crate::ffb::Frame;
use crate::rt::RTResult;
use racing_wheel_schemas::{FilterConfig, NotchFilter, CurvePoint, Gain};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use tracing::{debug, error};

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
            return Ok(());
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
            let state_ptr = unsafe {
                self.state.as_mut_ptr().add(self.state_offsets[i])
            };
            
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
        T: Copy 
    {
        if node_index < self.state_offsets.len() {
            let offset = self.state_offsets[node_index];
            
            // Verify alignment
            assert_eq!(offset % std::mem::align_of::<T>(), 0, 
                      "State offset {} is not aligned for type {}", 
                      offset, std::any::type_name::<T>());
            
            let state_ptr = unsafe {
                self.state.as_mut_ptr().add(offset) as *mut T
            };
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
    pub async fn compile_pipeline(&self, config: FilterConfig) -> Result<CompiledPipeline, PipelineError> {
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

        debug!("Pipeline compiled successfully with {} nodes, hash: {:x}", 
               pipeline.node_count(), config_hash);

        Ok(CompiledPipeline {
            pipeline,
            config_hash,
        })
    }

    /// Compile pipeline asynchronously and return immediately
    pub async fn compile_pipeline_async(&self, config: FilterConfig) -> Result<oneshot::Receiver<Result<CompiledPipeline, PipelineError>>, PipelineError> {
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
            return Err(PipelineError::InvalidConfig(
                format!("Reconstruction level must be 0-8, got {}", config.reconstruction)
            ));
        }

        // Validate gain values are in valid range
        if config.friction.value() < 0.0 || config.friction.value() > 1.0 {
            return Err(PipelineError::InvalidParameters(
                format!("Friction must be 0.0-1.0, got {}", config.friction.value())
            ));
        }

        if config.damper.value() < 0.0 || config.damper.value() > 1.0 {
            return Err(PipelineError::InvalidParameters(
                format!("Damper must be 0.0-1.0, got {}", config.damper.value())
            ));
        }

        if config.inertia.value() < 0.0 || config.inertia.value() > 1.0 {
            return Err(PipelineError::InvalidParameters(
                format!("Inertia must be 0.0-1.0, got {}", config.inertia.value())
            ));
        }

        if config.slew_rate.value() < 0.0 || config.slew_rate.value() > 1.0 {
            return Err(PipelineError::InvalidParameters(
                format!("Slew rate must be 0.0-1.0, got {}", config.slew_rate.value())
            ));
        }

        // Validate curve points are monotonic
        self.validate_curve_monotonic(&config.curve_points)?;

        // Validate notch filters
        for (i, filter) in config.notch_filters.iter().enumerate() {
            if filter.frequency.value() <= 0.0 || filter.frequency.value() > 500.0 {
                return Err(PipelineError::InvalidParameters(
                    format!("Notch filter {} frequency must be 0-500 Hz, got {}", i, filter.frequency.value())
                ));
            }
            
            if filter.q_factor <= 0.0 || filter.q_factor > 20.0 {
                return Err(PipelineError::InvalidParameters(
                    format!("Notch filter {} Q factor must be 0-20, got {}", i, filter.q_factor)
                ));
            }
        }

        Ok(())
    }

    /// Validate that curve points are monotonic
    fn validate_curve_monotonic(&self, curve_points: &[CurvePoint]) -> Result<(), PipelineError> {
        if curve_points.len() < 2 {
            return Err(PipelineError::InvalidConfig(
                "Curve must have at least 2 points".to_string()
            ));
        }

        for window in curve_points.windows(2) {
            if window[1].input <= window[0].input {
                return Err(PipelineError::NonMonotonicCurve);
            }
        }

        // Ensure curve starts at 0 and ends at 1
        if curve_points[0].input != 0.0 {
            return Err(PipelineError::InvalidConfig(
                "Curve must start at input 0.0".to_string()
            ));
        }

        if curve_points.last().unwrap().input != 1.0 {
            return Err(PipelineError::InvalidConfig(
                "Curve must end at input 1.0".to_string()
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
    fn add_reconstruction_filter(&self, pipeline: &mut Pipeline, level: u8) -> Result<(), PipelineError> {
        if level == 0 {
            return Ok(()); // No reconstruction filter
        }

        // Add reconstruction filter node with appropriate state
        pipeline.add_node(reconstruction_filter, std::mem::size_of::<ReconstructionState>());
        let node_index = pipeline.nodes.len() - 1;
        
        let state = ReconstructionState {
            level,
            prev_output: 0.0,
            alpha: 0.1f32.powi(level as i32), // More aggressive filtering for higher levels
        };
        
        pipeline.init_node_state(node_index, state);
        Ok(())
    }

    /// Add friction filter to pipeline
    fn add_friction_filter(&self, pipeline: &mut Pipeline, friction: Gain) -> Result<(), PipelineError> {
        if friction.value() == 0.0 {
            return Ok(()); // No friction
        }

        pipeline.add_node(friction_filter, std::mem::size_of::<f32>());
        let node_index = pipeline.nodes.len() - 1;
        pipeline.init_node_state(node_index, friction.value());
        Ok(())
    }

    /// Add damper filter to pipeline
    fn add_damper_filter(&self, pipeline: &mut Pipeline, damper: Gain) -> Result<(), PipelineError> {
        if damper.value() == 0.0 {
            return Ok(()); // No damping
        }

        pipeline.add_node(damper_filter, std::mem::size_of::<f32>());
        let node_index = pipeline.nodes.len() - 1;
        pipeline.init_node_state(node_index, damper.value());
        Ok(())
    }

    /// Add inertia filter to pipeline
    fn add_inertia_filter(&self, pipeline: &mut Pipeline, inertia: Gain) -> Result<(), PipelineError> {
        if inertia.value() == 0.0 {
            return Ok(()); // No inertia
        }

        pipeline.add_node(inertia_filter, std::mem::size_of::<InertiaState>());
        let node_index = pipeline.nodes.len() - 1;
        
        let state = InertiaState {
            coefficient: inertia.value(),
            prev_acceleration: 0.0,
        };
        
        pipeline.init_node_state(node_index, state);
        Ok(())
    }

    /// Add notch filters to pipeline
    fn add_notch_filters(&self, pipeline: &mut Pipeline, filters: &[NotchFilter]) -> Result<(), PipelineError> {
        for filter in filters {
            pipeline.add_node(notch_filter, std::mem::size_of::<NotchState>());
            let node_index = pipeline.nodes.len() - 1;
            
            let state = NotchState::new(
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
    fn add_slew_rate_filter(&self, pipeline: &mut Pipeline, slew_rate: Gain) -> Result<(), PipelineError> {
        if slew_rate.value() >= 1.0 {
            return Ok(()); // No slew rate limiting
        }

        pipeline.add_node(slew_rate_filter, std::mem::size_of::<SlewRateState>());
        let node_index = pipeline.nodes.len() - 1;
        
        let state = SlewRateState {
            max_change_per_tick: slew_rate.value() / 1000.0, // Per 1ms tick
            prev_output: 0.0,
        };
        
        pipeline.init_node_state(node_index, state);
        Ok(())
    }

    /// Add curve mapping filter to pipeline
    fn add_curve_filter(&self, pipeline: &mut Pipeline, curve_points: &[CurvePoint]) -> Result<(), PipelineError> {
        if curve_points.len() == 2 
            && curve_points[0].input == 0.0 && curve_points[0].output == 0.0
            && curve_points[1].input == 1.0 && curve_points[1].output == 1.0 {
            return Ok(()); // Linear curve, no filtering needed
        }

        // Pre-compute curve lookup table for fast RT execution
        const CURVE_LUT_SIZE: usize = 1024;
        let mut lut = [0.0f32; CURVE_LUT_SIZE];
        
        for i in 0..CURVE_LUT_SIZE {
            let input = i as f32 / (CURVE_LUT_SIZE - 1) as f32;
            lut[i] = self.interpolate_curve(input, curve_points);
        }

        pipeline.add_node(curve_filter, std::mem::size_of::<CurveState>());
        let node_index = pipeline.nodes.len() - 1;
        
        let state = CurveState {
            lut,
            lut_size: CURVE_LUT_SIZE,
        };
        
        pipeline.init_node_state(node_index, state);
        Ok(())
    }

    /// Interpolate curve value for a given input
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

// Filter node state structures

/// State for reconstruction filter
#[repr(C)]
#[derive(Copy, Clone)]
struct ReconstructionState {
    level: u8,
    prev_output: f32,
    alpha: f32,
}

/// State for inertia filter
#[repr(C)]
#[derive(Copy, Clone)]
struct InertiaState {
    coefficient: f32,
    prev_acceleration: f32,
}

/// State for notch filter (biquad implementation)
#[repr(C)]
#[derive(Copy, Clone)]
struct NotchState {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl NotchState {
    fn new(frequency: f32, q: f32, gain_db: f32, sample_rate: f32) -> Self {
        let omega = 2.0 * std::f32::consts::PI * frequency / sample_rate;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / (2.0 * q);
        let a = 10.0f32.powf(gain_db / 40.0);

        // Notch filter coefficients
        let b0 = 1.0;
        let b1 = -2.0 * cos_omega;
        let b2 = 1.0;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_omega;
        let a2 = 1.0 - alpha / a;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }
}

/// State for slew rate limiter
#[repr(C)]
#[derive(Copy, Clone)]
struct SlewRateState {
    max_change_per_tick: f32,
    prev_output: f32,
}

/// State for curve mapping (lookup table)
#[repr(C)]
#[derive(Copy, Clone)]
struct CurveState {
    lut: [f32; 1024],
    lut_size: usize,
}

// Filter node implementations (RT-safe, no allocations)

/// Reconstruction filter (anti-aliasing)
pub fn reconstruction_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &mut *(state as *mut ReconstructionState);
        let filtered = state.prev_output + state.alpha * (frame.ffb_in - state.prev_output);
        frame.torque_out = filtered;
        state.prev_output = filtered;
    }
}

/// Friction filter with speed adaptation
pub fn friction_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let friction_coeff = *(state as *mut f32);
        let speed_factor = 1.0 - (frame.wheel_speed.abs() * 0.1).min(0.8);
        let friction_torque = -frame.wheel_speed.signum() * friction_coeff * speed_factor;
        frame.torque_out += friction_torque;
    }
}

/// Damper filter
pub fn damper_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let damper_coeff = *(state as *mut f32);
        let damper_torque = -frame.wheel_speed * damper_coeff;
        frame.torque_out += damper_torque;
    }
}

/// Inertia filter
pub fn inertia_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &mut *(state as *mut InertiaState);
        
        // Calculate acceleration (change in wheel speed)
        let acceleration = frame.wheel_speed - state.prev_acceleration;
        let inertia_torque = -acceleration * state.coefficient;
        
        frame.torque_out += inertia_torque;
        state.prev_acceleration = frame.wheel_speed;
    }
}

/// Notch filter (biquad implementation)
pub fn notch_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &mut *(state as *mut NotchState);
        
        let input = frame.torque_out;
        let output = state.b0 * input + state.b1 * state.x1 + state.b2 * state.x2
                   - state.a1 * state.y1 - state.a2 * state.y2;
        
        // Update delay line
        state.x2 = state.x1;
        state.x1 = input;
        state.y2 = state.y1;
        state.y1 = output;
        
        frame.torque_out = output;
    }
}

/// Slew rate limiter
pub fn slew_rate_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &mut *(state as *mut SlewRateState);
        
        let desired_output = frame.torque_out;
        let max_change = state.max_change_per_tick;
        let change = desired_output - state.prev_output;
        
        let limited_change = change.clamp(-max_change, max_change);
        let limited_output = state.prev_output + limited_change;
        
        frame.torque_out = limited_output;
        state.prev_output = limited_output;
    }
}

/// Curve mapping filter using lookup table
pub fn curve_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let state = &*(state as *mut CurveState);
        
        let input = frame.torque_out.abs().clamp(0.0, 1.0);
        let index = (input * (state.lut_size - 1) as f32) as usize;
        let index = index.min(state.lut_size - 1);
        
        let mapped_output = state.lut[index];
        frame.torque_out = frame.torque_out.signum() * mapped_output;
    }
}

/// Torque limiting filter (safety)
pub fn torque_limit_filter(frame: &mut Frame, state: *mut u8) {
    unsafe {
        let max_torque = *(state as *mut f32);
        frame.torque_out = frame.torque_out.clamp(-max_torque, max_torque);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use racing_wheel_schemas::{
        Gain, FrequencyHz, CurvePoint, NotchFilter, FilterConfig
    };


    fn create_test_filter_config() -> FilterConfig {
        FilterConfig::new(
            4, // reconstruction
            Gain::new(0.1).unwrap(), // friction
            Gain::new(0.15).unwrap(), // damper
            Gain::new(0.05).unwrap(), // inertia
            vec![
                NotchFilter::new(
                    FrequencyHz::new(60.0).unwrap(),
                    2.0,
                    -12.0
                ).unwrap()
            ],
            Gain::new(0.8).unwrap(), // slew_rate
            vec![
                CurvePoint::new(0.0, 0.0).unwrap(),
                CurvePoint::new(0.5, 0.6).unwrap(),
                CurvePoint::new(1.0, 1.0).unwrap(),
            ]
        ).unwrap()
    }

    fn create_linear_filter_config() -> FilterConfig {
        FilterConfig::new(
            0, // no reconstruction
            Gain::new(0.0).unwrap(), // no friction
            Gain::new(0.0).unwrap(), // no damper
            Gain::new(0.0).unwrap(), // no inertia
            vec![], // no notch filters
            Gain::new(1.0).unwrap(), // no slew rate limiting
            vec![
                CurvePoint::new(0.0, 0.0).unwrap(),
                CurvePoint::new(1.0, 1.0).unwrap(),
            ]
        ).unwrap()
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
        let mut frame = crate::ffb::Frame {
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
        let invalid_config = FilterConfig {
            reconstruction: 10, // Invalid: > 8
            friction: Gain::new(0.1).unwrap(),
            damper: Gain::new(0.15).unwrap(),
            inertia: Gain::new(0.05).unwrap(),
            notch_filters: vec![],
            slew_rate: Gain::new(0.8).unwrap(),
            curve_points: vec![
                CurvePoint::new(0.0, 0.0).unwrap(),
                CurvePoint::new(1.0, 1.0).unwrap(),
            ],
        };

        let result = compiler.compile_pipeline(invalid_config).await;
        assert!(result.is_err());
        
        match result.unwrap_err() {
            PipelineError::InvalidConfig(_) => {}, // Expected
            _ => panic!("Expected InvalidConfig error"),
        }
    }

    #[tokio::test]
    async fn test_pipeline_validation_non_monotonic_curve() {
        let compiler = PipelineCompiler::new();
        
        // Create config with non-monotonic curve
        let invalid_config = FilterConfig {
            reconstruction: 4,
            friction: Gain::new(0.1).unwrap(),
            damper: Gain::new(0.15).unwrap(),
            inertia: Gain::new(0.05).unwrap(),
            notch_filters: vec![],
            slew_rate: Gain::new(0.8).unwrap(),
            curve_points: vec![
                CurvePoint::new(0.0, 0.0).unwrap(),
                CurvePoint::new(0.7, 0.6).unwrap(),
                CurvePoint::new(0.5, 0.8).unwrap(), // Non-monotonic!
                CurvePoint::new(1.0, 1.0).unwrap(),
            ],
        };

        let result = compiler.compile_pipeline(invalid_config).await;
        assert!(result.is_err());
        
        match result.unwrap_err() {
            PipelineError::NonMonotonicCurve => {}, // Expected
            _ => panic!("Expected NonMonotonicCurve error"),
        }
    }

    #[tokio::test]
    async fn test_pipeline_validation_invalid_parameters() {
        let compiler = PipelineCompiler::new();
        
        // Create config with invalid gain values
        let invalid_config = FilterConfig {
            reconstruction: 4,
            friction: Gain::new(1.5).unwrap_or(Gain::FULL), // This would be invalid if not clamped
            damper: Gain::new(0.15).unwrap(),
            inertia: Gain::new(0.05).unwrap(),
            notch_filters: vec![
                NotchFilter::new(
                    FrequencyHz::new(600.0).unwrap(), // Too high frequency
                    2.0,
                    -12.0
                ).unwrap()
            ],
            slew_rate: Gain::new(0.8).unwrap(),
            curve_points: vec![
                CurvePoint::new(0.0, 0.0).unwrap(),
                CurvePoint::new(1.0, 1.0).unwrap(),
            ],
        };

        let result = compiler.compile_pipeline(invalid_config).await;
        assert!(result.is_err());
        
        match result.unwrap_err() {
            PipelineError::InvalidParameters(_) => {}, // Expected
            _ => panic!("Expected InvalidParameters error"),
        }
    }

    #[test]
    fn test_filter_nodes_bounds_checking() {
        let mut frame = crate::ffb::Frame {
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
        let mut frame = crate::ffb::Frame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 1,
        };

        // Create a curve state with a simple mapping
        let mut curve_state = CurveState {
            lut: [0.0; 1024],
            lut_size: 1024,
        };
        
        // Fill LUT with a simple quadratic curve
        for i in 0..1024 {
            let input = i as f32 / 1023.0;
            curve_state.lut[i] = input * input; // Quadratic curve
        }

        let state_ptr = &mut curve_state as *mut CurveState as *mut u8;
        curve_filter(&mut frame, state_ptr);

        // Should apply quadratic mapping: 0.5^2 = 0.25
        assert!((frame.torque_out.abs() - 0.25).abs() < 0.1);
    }

    #[test]
    fn test_slew_rate_limiter() {
        let mut slew_state = SlewRateState {
            max_change_per_tick: 0.1, // 10% change per tick
            prev_output: 0.0,
        };

        let mut frame = crate::ffb::Frame {
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
        slew_rate_filter(&mut frame, state_ptr);
        assert!((frame.torque_out - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_notch_filter_stability() {
        let mut notch_state = NotchState::new(60.0, 2.0, -12.0, 1000.0);
        
        let mut frame = crate::ffb::Frame {
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
        assert!(duration.as_millis() < 100, "Compilation took too long: {:?}", duration);
    }
}