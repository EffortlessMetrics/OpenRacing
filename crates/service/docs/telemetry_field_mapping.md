# Telemetry Field Coverage and Normalization Mapping

This document describes the telemetry field coverage and normalization mapping for supported racing games, fulfilling requirement GI-03.

## Overview

The racing wheel software normalizes telemetry data from different racing games into a common format. This allows consistent processing regardless of the source game while maintaining game-specific optimizations.

## Normalized Telemetry Structure

All games are normalized to the following common structure:

```rust
pub struct NormalizedTelemetry {
    pub ffb_scalar: f32,        // Force feedback scalar (-1.0 to 1.0)
    pub rpm: f32,               // Engine RPM
    pub speed_ms: f32,          // Vehicle speed in m/s
    pub slip_ratio: f32,        // Tire slip ratio (0.0 to 1.0+)
    pub gear: i8,               // Current gear (-1 for reverse, 0 for neutral, 1+ for forward)
    pub flags: TelemetryFlags,  // Session flags (yellow, checkered, etc.)
    pub car_id: Option<String>, // Car identifier
    pub track_id: Option<String>, // Track identifier
}
```

## Game-Specific Field Mappings

### iRacing

**Telemetry Method:** Shared Memory  
**Update Rate:** 60 Hz  
**Coverage:** ✓ Complete

| Normalized Field | iRacing Field | Type | Notes |
|------------------|---------------|------|-------|
| `ffb_scalar` | `SteeringWheelTorque` | f32 | Direct mapping, already normalized |
| `rpm` | `RPM` | f32 | Engine RPM |
| `speed_ms` | `Speed` | f32 | Already in m/s |
| `slip_ratio` | `LFslipRatio` | f32 | Left front tire slip ratio |
| `gear` | `Gear` | i32 | Cast to i8 |
| `flags` | `SessionFlags` | u32 | Bitfield conversion to TelemetryFlags |
| `car_id` | `CarIdx` | i32 | Convert to string |
| `track_id` | `TrackId` | i32 | Convert to string |

**Additional Available Fields:**
- `RFslipRatio`, `LRslipRatio`, `RRslipRatio` (other tire slip ratios)
- `SteeringWheelAngle` (steering wheel position)
- `Throttle`, `Brake`, `Clutch` (pedal positions)
- `FuelLevel`, `FuelLevelPct` (fuel information)
- `LapCurrentLapTime`, `LapBestLapTime` (timing data)

### Assetto Corsa Competizione (ACC)

**Telemetry Method:** UDP Broadcast  
**Update Rate:** 100 Hz  
**Coverage:** ✓ Complete

| Normalized Field | ACC Field | Type | Notes |
|------------------|-----------|------|-------|
| `ffb_scalar` | `steerAngle` | f32 | Steering angle used as FFB proxy |
| `rpm` | `rpms` | f32 | Engine RPM |
| `speed_ms` | `speedKmh` | f32 | Convert from km/h to m/s (÷ 3.6) |
| `slip_ratio` | `wheelSlip[0]` | f32 | Front left wheel slip |
| `gear` | `gear` | i32 | Cast to i8 |
| `flags` | `flag` | i32 | Convert to TelemetryFlags |
| `car_id` | `carModel` | string | Direct mapping |
| `track_id` | `track` | string | Direct mapping |

**Additional Available Fields:**
- `wheelSlip[1-3]` (other wheel slip values)
- `tyreTemp[0-3]` (tire temperatures)
- `brakePadCompound[0-3]` (brake pad compounds)
- `suspensionTravel[0-3]` (suspension travel)
- `carDamage[0-4]` (damage values)

### Future Games

The system is designed to be extensible. New games can be added by:

1. Adding game configuration to `game_support_matrix.yaml`
2. Implementing a `ConfigWriter` for the game
3. Adding telemetry field mappings
4. Creating golden file tests

## Field Coverage Matrix

| Game | FFB Scalar | RPM | Speed | Slip Ratio | Gear | Flags | Car ID | Track ID |
|------|------------|-----|-------|------------|------|-------|--------|----------|
| iRacing | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| ACC | ✓* | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

*Note: ACC uses steering angle as FFB scalar proxy since direct FFB values are not available via telemetry.

## Telemetry Quality Indicators

### Data Freshness
- **iRacing:** Timestamp available, 60 Hz guaranteed
- **ACC:** Sequence number available, 100 Hz typical

### Data Reliability
- **iRacing:** Very High - Direct from simulation engine
- **ACC:** High - UDP broadcast with packet validation

### Latency Characteristics
- **iRacing:** ~16ms (60 Hz shared memory)
- **ACC:** ~10ms (100 Hz UDP, network dependent)

## Implementation Notes

### Error Handling
- Missing fields are handled gracefully with default values
- Invalid data triggers telemetry loss detection (GI-04)
- Packet loss is tracked and reported

### Performance Considerations
- Telemetry parsing is rate-limited to protect RT thread
- Field mappings are pre-computed at startup
- Memory allocations are minimized in hot path

### Extensibility
- New fields can be added to NormalizedTelemetry without breaking existing games
- Game-specific extensions are supported via additional metadata
- Version compatibility is maintained through feature negotiation

## Testing

Each game integration includes:
- Golden file tests for configuration generation
- Telemetry parsing validation with recorded data
- Field mapping accuracy verification
- Performance benchmarks for parsing overhead

## References

- **GI-01:** One-click telemetry configuration
- **GI-03:** Normalized telemetry publishing
- **GI-04:** Telemetry loss handling
- **Design Document:** Section 4 - Game Integration System