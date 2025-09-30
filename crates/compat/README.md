# Compatibility Layer for Schema Migration

This crate provides compatibility traits to ease migration from old field names to new field names in telemetry and configuration structs. It is designed to be used only in test builds and never ships in release builds.

## Purpose

During schema evolution, field names may change for clarity or consistency. This compatibility layer allows test code to gradually migrate from old field names to new ones without breaking existing tests during the transition period.

## Field Mappings

The `TelemetryCompat` trait provides the following field mappings:

| Old Field Name | New Field Name | Conversion |
|----------------|----------------|------------|
| `temp_c` | `temperature_c` | Direct mapping |
| `faults` | `fault_flags` | Direct mapping |
| `wheel_angle_mdeg` | `wheel_angle_deg` | Degrees to millidegrees (×1000) |
| `wheel_speed_mrad_s` | `wheel_speed_rad_s` | Rad/s to mrad/s (×1000) |
| `sequence` | *(removed)* | Always returns 0 |

## Usage

1. Add the compat crate as a dev-dependency:
```toml
[dev-dependencies]
compat = { path = "../compat" }
```

2. Import and implement the trait for your TelemetryData type:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use compat::TelemetryCompat;
    
    impl TelemetryCompat for TelemetryData {
        fn temp_c(&self) -> u8 { self.temperature_c }
        fn faults(&self) -> u8 { self.fault_flags }
        fn wheel_angle_mdeg(&self) -> i32 { (self.wheel_angle_deg * 1000.0) as i32 }
        fn wheel_speed_mrad_s(&self) -> i32 { (self.wheel_speed_rad_s * 1000.0) as i32 }
        fn sequence(&self) -> u32 { 0 }
    }
    
    #[test]
    fn test_with_old_field_names() {
        let telemetry = TelemetryData { /* ... */ };
        
        // Old code (to be migrated)
        assert_eq!(telemetry.temp_c(), 25);
        
        // New code (migration target)
        assert_eq!(telemetry.temperature_c, 25);
    }
}
```

## Migration Policy

- **Deprecation Window**: Old field names are supported through this compatibility layer for one minor version
- **Usage Tracking**: CI tracks compatibility layer usage to ensure it trends downward over time
- **Removal Timeline**: Compatibility shims will be removed in the next minor version after migration is complete

## CI Integration

The compatibility layer includes CI integration to track usage:

- `scripts/track_compat_usage.py` - Counts usage of compatibility methods
- `.github/workflows/compat-tracking.yml` - CI workflow that fails if usage increases
- Usage reports are generated as CI artifacts for monitoring migration progress

## Design Principles

1. **Test-Only**: This crate is gated to only compile in test builds
2. **Temporary**: Designed to be removed once migration is complete
3. **Trackable**: Usage is monitored to ensure downward trend
4. **Non-Breaking**: Provides smooth migration path without breaking existing tests

## Examples

See `examples/usage_example.rs` for a complete example of how to use the compatibility layer during schema migration.