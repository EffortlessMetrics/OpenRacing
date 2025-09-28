# ADR-0005: Plugin Architecture

**Status:** Accepted  
**Date:** 2024-01-15  
**Authors:** Architecture Team, Security Team  
**Reviewers:** Engineering Team, Community Team  
**Related ADRs:** ADR-0002 (IPC Transport), ADR-0004 (RT Scheduling)

## Context

The racing wheel software needs extensibility for community contributions while maintaining RT guarantees and system stability. The plugin system must support:

1. Safe telemetry processing and LED mapping (60-200Hz)
2. Fast DSP filter nodes in RT pipeline (1kHz)
3. Crash isolation to prevent service failures
4. Security through sandboxing and capability restrictions
5. Performance budgets with automatic enforcement

## Decision

Implement a two-tier plugin architecture:

**Safe Plugins (WASM/WASI):**
- Sandboxed execution environment with capability-based permissions
- 60-200Hz update rate for telemetry processing and LED mapping
- No direct file/network access without explicit capability grants
- Crash isolation with automatic restart and backoff
- Manifest-declared resource requirements and permissions

**Fast Plugins (Native, Optional):**
- Loaded in isolated `wheel-dsp` helper process
- SPSC shared memory communication with RT thread
- Microsecond-level timing budgets with watchdog enforcement
- ABI versioning with semantic compatibility checking
- Quarantine policy for repeatedly failing plugins

**Plugin Manifest Format:**
```json
{
  "schema": "wheel.plugin/1",
  "name": "custom-telemetry-processor",
  "version": "1.0.0",
  "type": "safe",
  "capabilities": ["telemetry.read", "led.write"],
  "update_rate_hz": 60,
  "memory_limit_mb": 16,
  "author": "Community Developer",
  "signature": "base64-encoded-ed25519-signature"
}
```

## Rationale

- **Safety First**: WASM sandboxing prevents most security and stability issues
- **Performance Path**: Native plugins enable RT-critical DSP operations
- **Isolation**: Helper process crash doesn't affect main service
- **Community**: Safe plugins lower barrier to entry for contributions
- **Enforcement**: Automatic budget enforcement prevents RT deadline misses

## Consequences

### Positive
- Community can extend functionality without compromising system stability
- Clear performance boundaries with automatic enforcement
- Security through sandboxing and capability restrictions
- Graceful degradation when plugins fail or exceed budgets
- ABI versioning enables plugin ecosystem evolution

### Negative
- Increased complexity in plugin loading and lifecycle management
- WASM runtime overhead for safe plugins
- IPC overhead for helper process communication
- Additional testing surface for plugin compatibility

### Neutral
- Plugin development requires understanding of timing constraints
- Native plugins require code signing for distribution
- Performance budgets may need tuning based on hardware capabilities

## Alternatives Considered

1. **Single Native Plugin Model**: Rejected due to security and stability risks
2. **Lua Scripting**: Rejected due to performance overhead and limited ecosystem
3. **Dynamic Library Loading**: Rejected due to crash isolation difficulties
4. **Process-per-Plugin**: Rejected due to IPC overhead and resource usage

## Implementation Notes

**Safe Plugin Interface:**
```rust
pub trait SafePlugin {
    fn manifest(&self) -> PluginManifest;
    fn process_telemetry(&mut self, input: &NormalizedTelemetry) -> Result<PluginOutput>;
    fn process_led_mapping(&mut self, input: &LedMappingInput) -> Result<LedPattern>;
}
```

**Fast Plugin Interface (C ABI):**
```c
typedef struct {
    float ffb_in, torque_out, wheel_speed;
    uint64_t ts_ns;
    uint32_t budget_us;  // Time budget for this tick
} frame_t;

typedef struct {
    void* (*create)(const uint8_t* config, size_t len);
    void (*process)(void* state, frame_t* frame);  // Must be RT-safe
    void (*destroy)(void* state);
    uint32_t abi_version;
} plugin_vtbl_t;
```

**Budget Enforcement:**
- Safe plugins: Memory and CPU time limits enforced by WASM runtime
- Fast plugins: Microsecond budgets with watchdog timer
- Violation policy: Warning → throttling → quarantine → disable

**Security Model:**
- WASM plugins run with minimal capabilities by default
- Native plugins require Ed25519 code signing
- Capability manifest declares required permissions
- User approval required for sensitive capabilities

## Compliance & Verification

- Plugin SDK with examples and documentation
- Automated testing of plugin lifecycle and error handling
- Performance validation of budget enforcement
- Security audit of capability system and sandboxing
- Compatibility testing across plugin ABI versions

**Test Coverage:**
- Unit tests for plugin loading and manifest validation
- Integration tests with sample plugins for each type
- Fault injection tests for crash isolation
- Performance tests for budget enforcement
- Security tests for capability restrictions

## References

- Requirements: PLUG-01 (Isolation), PLUG-02 (Contracts), PLUG-03 (Compatibility)
- Design Document: Plugin Architecture
- WASM/WASI Specification: https://wasi.dev/
- Plugin Development Guide: `docs/plugin-development.md`