# OpenRacing Plugin Examples

Reference implementations demonstrating the OpenRacing plugin system.

## Plugins

| Example | Capability | Description |
|---------|-----------|-------------|
| `road_surface` | DSP / Haptics | Simulates road texture via deterministic FFB roughness |
| `telemetry_logger` | Read Telemetry | Records telemetry snapshots into a pre-allocated ring buffer |
| `dashboard_overlay` | Read Telemetry | Computes gear, RPM bar, speed, shift light, and flag data |

## Building

```bash
# Native (tests & benchmarks)
cargo build -p openracing-plugin-examples

# WASM target (sandboxed execution)
cargo build -p openracing-plugin-examples --target wasm32-unknown-unknown
```

## Testing

```bash
cargo test -p openracing-plugin-examples
```

## Usage

### Road Surface Plugin

```rust
use openracing_plugin_examples::road_surface::{RoadSurfaceConfig, RoadSurfacePlugin};
use openracing_plugin_abi::TelemetryFrame;

let mut plugin = RoadSurfacePlugin::new(RoadSurfaceConfig {
    intensity: 0.3,      // roughness strength
    spatial_freq: 20.0,  // bumps per radian
    full_speed_rad_s: 10.0,
});

let telemetry = TelemetryFrame::default();
let ffb_out = plugin.process(0.5, &telemetry, 0.001);
// ffb_out is the modified FFB signal in [-1.0, 1.0]
```

### Telemetry Logger Plugin

```rust
use openracing_plugin_examples::telemetry_logger::{TelemetryLoggerConfig, TelemetryLoggerPlugin};
use openracing_plugin_abi::TelemetryFrame;

let mut logger = TelemetryLoggerPlugin::new(TelemetryLoggerConfig {
    decimation: 10,   // record every 10th tick
    capacity: 1024,   // ring buffer size
});

let frame = TelemetryFrame::new(1_000_000);
logger.record(&frame);

// Drain entries for file/network export
let entries = logger.drain();
```

### Dashboard Overlay Plugin

```rust
use openracing_plugin_examples::dashboard_overlay::{DashboardConfig, DashboardOverlayPlugin};
use openracing_plugin_abi::TelemetryFrame;

let dash = DashboardOverlayPlugin::new(DashboardConfig::default());
let telemetry = TelemetryFrame::default();

let data = dash.compute(&telemetry, 6500.0, 4, 0b0001);
// data.gear_char == '4', data.shift_light, data.speed_kmh, etc.
```

## Real-Time Safety

All three plugins follow the OpenRacing RT constraints:

- **No heap allocations** after construction (road_surface, dashboard_overlay)
- **Pre-allocated buffers** for any storage (telemetry_logger ring buffer)
- **Bounded, deterministic** computation
- **No I/O, locks, or syscalls** in hot paths
