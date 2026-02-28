//! Core pipeline types for FFB processing
//!
//! This module provides the fundamental types used in pipeline compilation
//! and execution.

use openracing_curves::CurveLut;
use openracing_filters::Frame;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{Mutex, oneshot};

/// Function pointer type for filter nodes
///
/// Each filter node is a function that takes a mutable frame and a pointer
/// to its state data. The state pointer is guaranteed to be properly aligned
/// and points to the correct state type for the filter.
///
/// # Safety
///
/// The caller must ensure that:
/// - The state pointer points to the correct state type for this filter
/// - The state pointer is properly aligned for the state type
/// - The state memory is valid for the duration of the call
pub type FilterNodeFn = fn(&mut Frame, *mut u8);

/// Compiled filter pipeline with zero-allocation execution
///
/// A pipeline contains all the filter nodes and their state for RT-safe
/// processing. Once compiled, the pipeline can be swapped atomically
/// in the RT loop.
///
/// # RT Safety
///
/// - `process()` is RT-safe: no allocations, no syscalls, O(n) where n = node count
/// - State is stored in a pre-allocated buffer with proper alignment
/// - Pipeline swap is atomic from the RT thread's perspective
///
/// # Example
///
/// ```
/// use openracing_pipeline::{Pipeline, Frame};
///
/// let mut pipeline = Pipeline::new();
/// let mut frame = Frame::default();
/// frame.torque_out = 0.5;
///
/// // RT-safe processing
/// let result = pipeline.process(&mut frame);
/// assert!(result.is_ok());
/// ```
#[derive(Debug)]
pub struct Pipeline {
    /// Function pointers for each filter node
    pub(crate) nodes: Vec<FilterNodeFn>,
    /// State storage for all nodes (Structure of Arrays)
    pub(crate) state: Vec<u8>,
    /// Offsets into state storage for each node
    pub(crate) state_offsets: Vec<usize>,
    /// Configuration hash for deterministic comparison
    pub(crate) config_hash: u64,
    /// Optional response curve for torque transformation (pre-computed LUT)
    /// Boxed to reduce Pipeline size in enum variants
    pub(crate) response_curve: Option<Box<CurveLut>>,
}

/// Pipeline compilation result
///
/// Contains the compiled pipeline and its configuration hash
/// for change detection.
#[derive(Debug)]
pub struct CompiledPipeline {
    /// The compiled pipeline ready for RT execution
    pub pipeline: Pipeline,
    /// Configuration hash for change detection
    pub config_hash: u64,
}

/// Pipeline compilation and execution errors
#[derive(Debug, Error)]
pub enum PipelineError {
    /// Invalid filter configuration
    #[error("Invalid filter configuration: {0}")]
    InvalidConfig(String),

    /// Compilation failed
    #[error("Compilation failed: {0}")]
    CompilationFailed(String),

    /// Pipeline swap failed
    #[error("Pipeline swap failed: {0}")]
    SwapFailed(String),

    /// Non-monotonic curve points
    #[error("Non-monotonic curve points")]
    NonMonotonicCurve,

    /// Invalid filter parameters
    #[error("Invalid filter parameters: {0}")]
    InvalidParameters(String),
}

/// Internal compilation task for async compilation
#[derive(Debug)]
pub(crate) struct CompilationTask {
    /// Filter configuration to compile
    pub config: racing_wheel_schemas::entities::FilterConfig,
    /// Response channel for compilation result
    pub response_tx: oneshot::Sender<Result<CompiledPipeline, PipelineError>>,
}

/// Shared compilation task queue
pub(crate) type SharedTaskQueue = Arc<Mutex<Vec<CompilationTask>>>;

impl Pipeline {
    /// Create empty pipeline
    ///
    /// An empty pipeline passes frames through unchanged (identity transform).
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            state: Vec::new(),
            state_offsets: Vec::new(),
            config_hash: 0,
            response_curve: None,
        }
    }

    /// Create pipeline with specific configuration hash
    ///
    /// Used internally during compilation to set the deterministic hash.
    #[must_use]
    pub fn with_hash(config_hash: u64) -> Self {
        Self {
            nodes: Vec::new(),
            state: Vec::new(),
            state_offsets: Vec::new(),
            config_hash,
            response_curve: None,
        }
    }

    /// Set the response curve for this pipeline
    ///
    /// The curve is pre-computed as a LUT at profile load time (not in RT path).
    /// This ensures zero allocations during RT processing.
    pub fn set_response_curve(&mut self, curve: CurveLut) {
        self.response_curve = Some(Box::new(curve));
    }

    /// Get the response curve if set
    #[must_use]
    pub fn response_curve(&self) -> Option<&CurveLut> {
        self.response_curve.as_deref()
    }

    /// Get the configuration hash for this pipeline
    #[must_use]
    pub fn config_hash(&self) -> u64 {
        self.config_hash
    }

    /// Check if pipeline is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get the number of filter nodes
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Add a filter node to the pipeline (used during compilation)
    ///
    /// This method is used internally by the compiler to build the pipeline.
    /// It ensures proper alignment of state data.
    pub(crate) fn add_node(&mut self, node_fn: FilterNodeFn, state_size: usize) {
        let align = std::mem::align_of::<f64>();
        let current_len = self.state.len();
        let aligned_offset = (current_len + align - 1) & !(align - 1);

        self.state.resize(aligned_offset, 0);
        self.state_offsets.push(aligned_offset);
        self.state.resize(aligned_offset + state_size, 0);
        self.nodes.push(node_fn);
    }

    /// Initialize state for a specific node (used during compilation)
    ///
    /// # Safety
    ///
    /// The caller must ensure that:
    /// - `node_index` is valid
    /// - The type `T` matches the state type for this node
    /// - The state offset is properly aligned for type `T`
    pub(crate) unsafe fn init_node_state<T>(&mut self, node_index: usize, initial_state: T)
    where
        T: Copy,
    {
        debug_assert!(node_index < self.state_offsets.len(), "Invalid node index");
        debug_assert!(node_index < self.state_offsets.len(), "Invalid node index");
        debug_assert!(
            self.state_offsets[node_index].is_multiple_of(std::mem::align_of::<T>()),
            "State offset not aligned for type"
        );

        if node_index < self.state_offsets.len() {
            let offset = self.state_offsets[node_index];
            // SAFETY: The caller ensures the offset is valid and aligned
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

impl Clone for Pipeline {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            state: self.state.clone(),
            state_offsets: self.state_offsets.clone(),
            config_hash: self.config_hash,
            response_curve: self.response_curve.clone(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_new() {
        let pipeline = Pipeline::new();
        assert!(pipeline.is_empty());
        assert_eq!(pipeline.node_count(), 0);
        assert_eq!(pipeline.config_hash(), 0);
    }

    #[test]
    fn test_pipeline_with_hash() {
        let hash = 0xDEADBEEF_u64;
        let pipeline = Pipeline::with_hash(hash);
        assert_eq!(pipeline.config_hash(), hash);
        assert!(pipeline.is_empty());
    }

    #[test]
    fn test_pipeline_response_curve() {
        let mut pipeline = Pipeline::new();
        assert!(pipeline.response_curve().is_none());

        let lut = CurveLut::linear();
        pipeline.set_response_curve(lut);
        assert!(pipeline.response_curve().is_some());
    }

    #[test]
    fn test_pipeline_clone() {
        let pipeline = Pipeline::with_hash(0x12345678);
        let cloned = pipeline.clone();
        assert_eq!(pipeline.config_hash(), cloned.config_hash());
    }
}
