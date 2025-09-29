# Telemetry Field Coverage and Normalization Mapping

This document describes the telemetry field coverage and normalization mapping for supported racing games, as required by GI-03.

## Normalized Telemetry Fields

The racing wheel software uses a normalized telemetry structure to provide consistent data across different games:

```rust
pub struct NormalizedTelemetry {
    pub ffb_scalar: f32,        // Force feedback scalar (-1.0 to 1.0)
    pub rpm: f32,               // Engine RPM
    pub speed_ms: f32,          // Vehicle speed in m/s
    pub slip_ratio: f32,        // Tire slip ratio (0.0 to 1.0+)
    pub gear: i8,               // Current gear (-1 for reverse, 0 for neutral, 1+ for forward)
    pub flags: TelemetryFlags,  // Session flags (yellow, checkered, etc.)
    pub car_id: Option<String>, // Car/vehicle identifier
    pub track_id: Option<String>, // Track identifier
}
```

## Game-Specific Field Mappings

### iRacing

**Telemetry Method:** Shared Memory  
**Update Rate:** 60 Hz  
**SDK:** iRacing SDK

| Normalized Field | iRacing Field | Type | Notes |
|------------------|---------------|------|-------|
| `ffb_scalar` | `SteeringWheelTorque` | float | Direct mapping, already normalized |
| `rpm` | `RPM` | float | Engine RPM |
| `speed_ms` | `Speed` | float | Already in m/s |
| `slip_ratio` | `LFslipRatio` | float | Left front tire slip ratio |
| `gear` | `Gear` | int | Direct mapping |
| `flags` | `SessionFlags` | bitfield | Requires flag parsing |
| `car_id` | `CarIdx` | int | Car index in session |
| `track_id` | `TrackId` | int | Track identifier |

**Coverage:** ✅ Full coverage of all normalized fields

**Special Notes:**
- iRacing provides multiple tire slip ratios (LF, RF, LR, RR). We use left front as primary.
- SessionFlags is a bitfield that needs parsing for yellow flags, checkered flag, etc.
- Speed is already in m/s, no conversion needed.

### Assetto Corsa Competizione (ACC)

**Telemetry Method:** UDP Broadcast  
**Update Rate:** 100 Hz  
**Protocol:** ACC Broadcasting API

| Normalized Field | ACC Field | Type | Notes |
|------------------|-----------|------|-------|
| `ffb_scalar` | `steerAngle` | float | Steering angle, needs conversion to FFB |
| `rpm` | `rpms` | float | Engine RPM |
| `speed_ms` | `speedKmh` | float | Speed in km/h, convert to m/s |
| `slip_ratio` | `wheelSlip` | float array | Array of 4 wheel slip values |
| `gear` | `gear` | int | Direct mapping |
| `flags` | `flag` | enum | Session flag enumeration |
| `car_id` | `carModel` | string | Car model name |
| `track_id` | `track` | string | Track name |

**Coverage:** ✅ Full coverage with conversions

**Special Notes:**
- Speed conversion: `speed_ms = speedKmh / 3.6`
- wheelSlip is an array [FL, FR, RL, RR], we use front-left index 0
- steerAngle needs conversion to FFB scalar (implementation-dependent)
- Flag enumeration maps to normalized flag bitfield

## Field Coverage Matrix

| Game | FFB Scalar | RPM | Speed | Slip Ratio | Gear | Flags | Car ID | Track ID |
|------|------------|-----|-------|------------|------|-------|--------|----------|
| iRacing | ✅ Direct | ✅ Direct | ✅ Direct | ✅ LF Tire | ✅ Direct | ✅ Bitfield | ✅ Index | ✅ ID |
| ACC | ⚠️ Convert | ✅ Direct | ⚠️ Convert | ✅ FL Wheel | ✅ Direct | ✅ Enum | ✅ Model | ✅ Name |

**Legend:**
- ✅ Direct: Field maps directly without conversion
- ⚠️ Convert: Field requires unit conversion or calculation
- ❌ Missing: Field not available in game telemetry

## Conversion Functions

### Speed Conversion (ACC)
```rust
fn convert_speed_kmh_to_ms(speed_kmh: f32) -> f32 {
    speed_kmh / 3.6
}
```

### Steering to FFB Conversion (ACC)
```rust
fn convert_steering_to_ffb(steer_angle: f32, max_angle: f32) -> f32 {
    (steer_angle / max_angle).clamp(-1.0, 1.0)
}
```

### Flag Conversion
```rust
// iRacing SessionFlags bitfield
const IRACING_FLAG_CHECKERED: u32 = 0x00000001;
const IRACING_FLAG_WHITE: u32 = 0x00000002;
const IRACING_FLAG_GREEN: u32 = 0x00000004;
const IRACING_FLAG_YELLOW: u32 = 0x00000008;

// ACC Flag enumeration
enum ACCFlag {
    None = 0,
    Blue = 1,
    Yellow = 2,
    Black = 3,
    White = 4,
    Checkered = 5,
    Penalty = 6,
}
```

## Update Rates and Latency

| Game | Native Rate | Normalized Rate | Latency Target |
|------|-------------|-----------------|----------------|
| iRacing | 60 Hz | 60 Hz | < 16.7 ms |
| ACC | 100 Hz | 100 Hz | < 10 ms |

## Configuration Requirements

### iRacing Configuration
File: `Documents/iRacing/app.ini`
```ini
[Telemetry]
telemetryDiskFile=1
```

### ACC Configuration
File: `Documents/Assetto Corsa Competizione/Config/broadcasting.json`
```json
{
  "updListenerPort": 9996,
  "connectionId": "",
  "broadcastingPort": 9000,
  "commandPassword": "",
  "updateRateHz": 100
}
```

## Future Game Support

When adding support for new games, ensure:

1. **Field Mapping:** Document how each normalized field maps to game-specific fields
2. **Conversions:** Implement any necessary unit conversions or calculations
3. **Coverage:** Identify any missing fields and document limitations
4. **Update Rate:** Document native and achievable update rates
5. **Configuration:** Define required configuration file changes
6. **Testing:** Add golden file tests for configuration generation

## Testing Coverage

All field mappings are covered by automated tests in:
- `tests/golden_tests.rs` - Configuration generation tests
- `tests/telemetry_mapping_tests.rs` - Field mapping validation tests

Each game integration must pass:
- ✅ Configuration file generation matches golden files
- ✅ Field mapping produces expected normalized output
- ✅ Update rate meets latency requirements
- ✅ Error handling for missing/invalid data