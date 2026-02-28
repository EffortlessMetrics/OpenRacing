//! FFB Pipeline Compilation and Execution for OpenRacing
//!
//! This crate provides pipeline compilation and execution for force feedback processing.
//! It transforms filter configurations into RT-safe executable pipelines.
//!
//! # Overview
//!
//! The pipeline system includes:
//! - **Pipeline**: Compiled pipeline ready for RT execution
//! - **PipelineCompiler**: Compiles FilterConfig to executable pipeline
//! - **PipelineValidator**: Validates configurations before compilation
//! - **Hash calculation**: Deterministic hashing for change detection
//!
//! # RT Safety Guarantees
//!
//! - **No heap allocations** in `Pipeline::process()` hot path
//! - **O(n) time complexity** where n = filter node count
//! - **Bounded execution time** for all filters
//! - **Atomic pipeline swap** at tick boundaries
//!
//! # Architecture
//!
//! ```text
//! FilterConfig → PipelineCompiler → CompiledPipeline → Pipeline
//!                     ↓                                      ↓
//!               PipelineValidator                        process()
//!                                                          (RT-safe)
//! ```
//!
//! # Example
//!
//! ```
//! use openracing_pipeline::prelude::*;
//! use openracing_filters::Frame;
//!
//! // Create a pipeline
//! let mut pipeline = Pipeline::new();
//!
//! // Create a frame to process
//! let mut frame = Frame {
//!     ffb_in: 0.5,
//!     torque_out: 0.5,
//!     wheel_speed: 0.0,
//!     hands_off: false,
//!     ts_mono_ns: 0,
//!     seq: 1,
//! };
//!
//! // RT-safe processing (no allocations)
//! let result = pipeline.process(&mut frame);
//! assert!(result.is_ok());
//! ```
//!
//! # Compilation Example
//!
//! ```ignore
//! use openracing_pipeline::prelude::*;
//! use racing_wheel_schemas::entities::FilterConfig;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), PipelineError> {
//!     let compiler = PipelineCompiler::new();
//!     let config = FilterConfig::default();
//!
//!     let compiled = compiler.compile_pipeline(config).await?;
//!     println!("Compiled pipeline with {} nodes", compiled.pipeline.node_count());
//!
//!     Ok(())
//! }
//! ```

#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod compiler;
pub mod executor;
pub mod hash;
pub mod prelude;
pub mod state;
pub mod types;
pub mod validation;

pub use compiler::PipelineCompiler;
pub use hash::{calculate_config_hash, calculate_config_hash_with_curve};
pub use state::PipelineStateSnapshot;
pub use types::{CompiledPipeline, FilterNodeFn, Pipeline, PipelineError};
pub use validation::PipelineValidator;

pub use openracing_filters::Frame;
