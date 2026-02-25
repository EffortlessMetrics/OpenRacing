//! Prelude for openracing-pipeline
//!
//! This module provides convenient re-exports of commonly used types.
//!
//! # Example
//!
//! ```
//! use openracing_pipeline::prelude::*;
//!
//! let pipeline = Pipeline::new();
//! assert!(pipeline.is_empty());
//! ```

pub use crate::compiler::PipelineCompiler;
pub use crate::hash::{calculate_config_hash, calculate_config_hash_with_curve};
pub use crate::types::{CompiledPipeline, FilterNodeFn, Pipeline, PipelineError};
pub use crate::validation::PipelineValidator;
