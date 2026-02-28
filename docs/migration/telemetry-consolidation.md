# Telemetry Crate Consolidation Migration Guide

## Summary

This guide documents the consolidation of 10 telemetry crates into 4 focused crates.

### Before (10 crates)
1. `telemetry-core` - GameTelemetry, GameTelemetryAdapter trait
2. `telemetry-contracts` - NormalizedTelemetry, TelemetryFlags
3. `telemetry-adapters` - Game-specific implementations
4. `telemetry-orchestrator` - Coordination layer
5. `telemetry-integration` - Integration tests as crate
6. `telemetry-recorder` - Recording/playback
7. `telemetry-rate-limiter` - Rate limiting
8. `telemetry-bdd-metrics` - BDD metrics
9. `telemetry-support` - Utilities
10. `telemetry-config-writers` - Config file writers

### After (4 crates)
1. **`telemetry-core`** - Core types, adapter trait, contracts, rate limiting, BDD metrics, integration utilities, orchestrator
2. **`telemetry-adapters`** - Game-specific implementations
3. **`telemetry-recorder`** - Recording/playback
4. **`telemetry-config`** - Config writers, game support matrix, utilities

## Migration Paths

### From `telemetry-contracts`

**Old:**
```rust
use racing_wheel_telemetry_contracts::{
    NormalizedTelemetry, TelemetryFlags, TelemetryFrame, TelemetryValue,
};
```

**New:**
```rust
use racing_wheel_telemetry_core::{
    NormalizedTelemetry, TelemetryFlags, TelemetryFrame, TelemetryValue,
};
```

### From `telemetry-rate-limiter`

**Old:**
```rust
use racing_wheel_telemetry_rate_limiter::{
    RateLimiter, RateLimiterStats, AdaptiveRateLimiter,
};
```

**New:**
```rust
use racing_wheel_telemetry_core::{
    RateLimiter, RateLimiterStats, AdaptiveRateLimiter,
};
```

### From `telemetry-bdd-metrics`

**Old:**
```rust
use racing_wheel_telemetry_bdd_metrics::{
    BddMatrixMetrics, MatrixParityPolicy, RuntimeBddMatrixMetrics,
};
```

**New:**
```rust
use racing_wheel_telemetry_core::{
    BddMatrixMetrics, MatrixParityPolicy, RuntimeBddMatrixMetrics,
};
```

### From `telemetry-integration`

**Old:**
```rust
use racing_wheel_telemetry_integration::{
    compare_matrix_and_registry, CoveragePolicy, RegistryCoverage,
    RuntimeCoverageReport,
};
```

**New:**
```rust
use racing_wheel_telemetry_core::{
    compare_matrix_and_registry, CoveragePolicy, RegistryCoverage,
    RuntimeCoverageReport,
};
```

### From `telemetry-orchestrator`

**Old:**
```rust
use racing_wheel_telemetry_orchestrator::TelemetryService;
```

**New:**
```rust
use racing_wheel_telemetry_core::TelemetryService;
```

### From `telemetry-support`

**Old:**
```rust
use racing_wheel_telemetry_support::{
    load_default_matrix, matrix_game_ids, normalize_game_id,
    GameSupportMatrix, GameSupport,
};
```

**New:**
```rust
use racing_wheel_telemetry_config::support::{
    load_default_matrix, matrix_game_ids, normalize_game_id,
    GameSupportMatrix, GameSupport,
};
// Or directly:
use racing_wheel_telemetry_config::{
    load_default_matrix, matrix_game_ids, normalize_game_id,
    GameSupportMatrix, GameSupport,
};
```

### From `telemetry-config-writers`

**Old:**
```rust
use racing_wheel_telemetry_config_writers::{
    config_writer_factories, ConfigWriter, TelemetryConfig,
    IRacingConfigWriter, ACCConfigWriter,
};
```

**New:**
```rust
use racing_wheel_telemetry_config::{
    config_writer_factories, ConfigWriter, TelemetryConfig,
    IRacingConfigWriter, ACCConfigWriter,
};
```

## Cargo.toml Updates

### Old Dependencies
```toml
[dependencies]
racing-wheel-telemetry-contracts = { path = "../telemetry-contracts" }
racing-wheel-telemetry-rate-limiter = { path = "../telemetry-rate-limiter" }
racing-wheel-telemetry-bdd-metrics = { path = "../telemetry-bdd-metrics" }
racing-wheel-telemetry-support = { path = "../telemetry-support" }
racing-wheel-telemetry-config-writers = { path = "../telemetry-config-writers" }
racing-wheel-telemetry-integration = { path = "../telemetry-integration" }
racing-wheel-telemetry-orchestrator = { path = "../telemetry-orchestrator" }
```

### New Dependencies
```toml
[dependencies]
racing-wheel-telemetry-core = { path = "../telemetry-core" }
racing-wheel-telemetry-config = { path = "../telemetry-config" }
# These remain the same:
racing-wheel-telemetry-adapters = { path = "../telemetry-adapters" }
racing-wheel-telemetry-recorder = { path = "../telemetry-recorder" }
```

## Deprecated Crates

The following crates are now **deprecated** and will be removed in a future release:

| Old Crate | Replacement |
|-----------|-------------|
| `telemetry-contracts` | `telemetry-core` |
| `telemetry-rate-limiter` | `telemetry-core` |
| `telemetry-bdd-metrics` | `telemetry-core` |
| `telemetry-integration` | `telemetry-core` |
| `telemetry-orchestrator` | `telemetry-core` |
| `telemetry-support` | `telemetry-config` |
| `telemetry-config-writers` | `telemetry-config` |

## Benefits of Consolidation

1. **Simplified Dependencies**: Fewer crates to manage
2. **Clearer Boundaries**: Each crate has a focused purpose
3. **Reduced Build Time**: Fewer crate compilations
4. **Better Organization**: Related functionality grouped together
5. **Easier Maintenance**: Less context switching between crates

## Timeline

- **Phase 1**: New consolidated crates available (current)
- **Phase 2**: Deprecation warnings added to old crates
- **Phase 3**: Old crates removed (future release)
