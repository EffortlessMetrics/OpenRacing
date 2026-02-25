# openracing-pipeline

FFB pipeline compilation and execution for OpenRacing.

## Overview

This crate provides pipeline compilation and execution for force feedback processing.
It transforms filter configurations into RT-safe executable pipelines.

## Features

- **RT-Safe Execution**: Zero allocations in the hot path
- **Atomic Pipeline Swap**: Seamless pipeline replacement at tick boundaries
- **Deterministic Hashing**: Configuration change detection
- **Comprehensive Validation**: Pre-compilation validation of all parameters

## Architecture

```text
FilterConfig → PipelineCompiler → CompiledPipeline → Pipeline
                    ↓                                      ↓
              PipelineValidator                        process()
                                                          (RT-safe)
```

## RT Safety Guarantees

- **No heap allocations** in `Pipeline::process()` hot path
- **O(n) time complexity** where n = filter node count
- **Bounded execution time** for all filters
- **Atomic pipeline swap** at tick boundaries

## Example

```rust
use openracing_pipeline::prelude::*;
use openracing_filters::Frame;

// Create a pipeline
let mut pipeline = Pipeline::new();

// Create a frame to process
let mut frame = Frame {
    ffb_in: 0.5,
    torque_out: 0.5,
    wheel_speed: 0.0,
    hands_off: false,
    ts_mono_ns: 0,
    seq: 1,
};

// RT-safe processing (no allocations)
let result = pipeline.process(&mut frame);
assert!(result.is_ok());
```

## Compilation Example

```rust
use openracing_pipeline::prelude::*;
use racing_wheel_schemas::entities::FilterConfig;

#[tokio::main]
async fn main() -> Result<(), PipelineError> {
    let compiler = PipelineCompiler::new();
    let config = FilterConfig::default();

    let compiled = compiler.compile_pipeline(config).await?;
    println!("Compiled pipeline with {} nodes", compiled.pipeline.node_count());

    Ok(())
}
```

## License

MIT OR Apache-2.0
