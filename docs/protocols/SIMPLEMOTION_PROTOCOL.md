# SimpleMotion V2 / Granite Devices IONI & ARGON Protocol Documentation

**Status**: ✅ Supported

SimpleMotion V2 is a binary command protocol developed by [Granite Devices](https://granitedevices.com/) for their IONI and ARGON servo drive families. It is used in community Open Sim Wheel (OSW) direct-drive wheel bases and in the original Simucube 1 hardware.

## Device Identification

| Model | VID | PID | Max Torque | Notes |
|-------|-----|-----|------------|-------|
| IONI Servo Drive / Simucube 1 | `0x1D50` | `0x6050` | 15 Nm | Standard IONI |
| IONI Premium / Simucube 2 | `0x1D50` | `0x6051` | 35 Nm | V2 hardware |
| ARGON Servo Drive / Simucube Sport / OSW | `0x1D50` | `0x6052` | 10 Nm | V2 hardware |

VID `0x1D50` is the OpenMoko community USB VID widely used for open-hardware projects.

> **Note:** Simucube 2 Sport/Pro/Ultimate (VID `0x2D6A`) is a different product line using plug-and-play HID PIDFF, documented in [SIMUCUBE_PROTOCOL.md](SIMUCUBE_PROTOCOL.md).

## Protocol Overview

SimpleMotion V2 uses a 15-byte binary frame over a USB HID bulk endpoint (or RS485 for legacy setups). All frames carry a CRC-8 checksum for integrity verification. The device responds to commands with 64-byte feedback reports.

### Frame Structure (Command — 15 bytes)

```
Byte  0:     Report ID  = 0x01
Byte  1:     Sequence number (u8, wrapping)
Bytes 2–3:   Command type (u16 LE)
Bytes 4–5:   Parameter address (u16 LE)  — for parameter commands
Bytes 6–9:   Parameter value (i32 LE)   — for SetParameter
Bytes 10–13: Data payload (i32 LE)      — for SetTorque / SetVelocity / SetPosition
Byte  14:    CRC-8 checksum over bytes 0–13
```

### CRC-8 Algorithm

Polynomial `0x07`, initial value `0x00`, input and output not reflected.

```rust
fn compute_crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0x00;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            if crc & 0x80 != 0 { crc = (crc << 1) ^ 0x07; }
            else { crc <<= 1; }
        }
    }
    crc
}
```

### Command Types

| Code (u16 LE) | Name | Description |
|---------------|------|-------------|
| `0x0001` | `GetParameter` | Read a parameter by address |
| `0x0002` | `SetParameter` | Write a parameter by address |
| `0x0003` | `GetStatus` | Request device status |
| `0x0010` | `SetTorque` | Set real-time torque (FFB path) |
| `0x0011` | `SetVelocity` | Set velocity setpoint |
| `0x0012` | `SetPosition` | Set position setpoint |
| `0x0013` | `SetZero` | Set current position as zero reference |
| `0xFFFF` | `Reset` | Soft reset the device |

### Status Codes (feedback byte 2)

| Value | Meaning |
|-------|---------|
| `0` | Ok |
| `1` | Error |
| `2` | Busy |
| `3` | NotReady |
| other | Unknown |

## Initialization Sequence

1. **Enable motor drive** — send `SetParameter` with address `0x1001`, value `1`:

```
[0x01, seq, 0x02, 0x00,   // report ID, seq, SetParameter
 0x01, 0x10,              // param address 0x1001
 0x01, 0x00, 0x00, 0x00,  // value = 1 (enable)
 0x00, 0x00, 0x00, 0x00,  // data unused
 crc]
```

No further initialization is required for standard FFB operation.

## Output Reports (Force Feedback)

### SetTorque Command

Real-time torque is delivered at up to 1kHz (IONI supports up to 20kHz over RS485):

```
Bytes 10–13: Q8.8 fixed-point torque value (i32 LE)
```

**Torque encoding** (Q8.8 signed fixed-point, normalized):
- Map physical torque in `[-max_nm, +max_nm]` → `[-32767, +32767]`
- Formula: `raw = (torque_nm / max_torque_nm).clamp(-1.0, 1.0) * 32767.0`

**Optional velocity feed-forward** (SetTorque with velocity):
- Pack torque in bits 31–16, velocity (Q8.8 RPM) in bits 15–0:
  ```rust
  let combined = ((torque_q8_8 as i32) << 16) | (velocity_q8_8 as i32 & 0xFFFF);
  ```

### SetZero Command

Sets the current encoder position as the zero reference (home position):

```
[0x01, seq, 0x13, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, crc]
```

## Input Reports (Feedback — 64 bytes)

```
Byte  0:     Report ID = 0x02
Byte  1:     Sequence number (echoes command seq)
Byte  2:     Status code
Byte  3:     Reserved
Bytes 4–7:   Motor position (i32 LE, encoder counts)
Bytes 8–11:  Motor velocity (i32 LE, encoder counts/s)
Bytes 12–13: Motor torque (i16 LE, raw Q8.8 value)
Bytes 14–15: Bus voltage (u16 LE, mV)
Bytes 16–17: Motor current (i16 LE, mA)
Byte  18:    Temperature (i8, °C)
Bytes 19–63: Reserved
```

**Derived values**:
```
position_degrees = position / encoder_cpr * 360.0
velocity_rpm     = velocity / encoder_cpr * 60.0
torque_nm        = torque_raw * torque_constant / 256.0
```

Default encoder CPR: **131 072** (17-bit) for SimpleMotion V2 devices.

## Feature Reports

SimpleMotion V2 feature reports follow the same 15-byte frame format, using the `SetParameter` / `GetParameter` command types. The transport maximum is 64 bytes.

## Configuration Parameters (selected)

| Address | Name | Notes |
|---------|------|-------|
| `0x1001` | Enable drive | 1 = enabled, 0 = disabled |

Refer to [Granite Devices developer wiki](https://granitedevices.com/wiki/SimpleMotion_V2) for the full parameter table.

## OpenRacing Implementation

| Component | Location |
|-----------|----------|
| Protocol constants, encoders, parsers | `crates/simplemotion-v2/src/` |
| Vendor handler | `crates/engine/src/hid/vendor/simplemotion.rs` |
| Handler tests | `crates/engine/src/hid/vendor/simplemotion_tests.rs` |
| udev rules | `packaging/linux/99-racing-wheel-suite.rules` |

### Crate public API (`simplemotion-v2`)

```rust
// IDs
const IONI_VENDOR_ID: u16 = 0x1D50;
const IONI_PRODUCT_ID: u16 = 0x6050;
const IONI_PRODUCT_ID_PREMIUM: u16 = 0x6051;
const ARGON_PRODUCT_ID: u16 = 0x6052;
fn identify_device(pid: u16) -> SmDeviceIdentity;
fn is_wheelbase_product(pid: u16) -> bool;

// Commands
const TORQUE_COMMAND_LEN: usize = 15;
struct TorqueCommandEncoder { /* max_torque_nm, torque_constant, seq */ }
impl TorqueCommandEncoder {
    fn new(max_torque_nm: f32) -> Self;
    fn encode(&mut self, torque_nm: f32, out: &mut [u8; 15]) -> usize;
    fn encode_with_velocity(&mut self, torque_nm: f32, velocity_rpm: f32, out: &mut [u8; 15]) -> usize;
    fn encode_zero(&mut self, out: &mut [u8; 15]) -> usize;
}
fn build_device_enable(enable: bool, seq: u8) -> [u8; 15];
fn build_set_parameter(param_addr: u16, value: i32, seq: u8) -> [u8; 15];
fn build_get_parameter(param_addr: u16, seq: u8) -> [u8; 15];
fn build_get_status(seq: u8) -> [u8; 15];
fn build_set_zero_position(seq: u8) -> [u8; 15];

// Feedback
fn parse_feedback_report(data: &[u8]) -> SmResult<SmFeedbackState>;
struct SmFeedbackState {
    seq: u8,
    status: SmStatus,
    motor: SmMotorFeedback,  // position, velocity, torque
    bus_voltage: u16,        // mV
    motor_current: i16,      // mA
    temperature: i8,         // °C
    connected: bool,
}
```

## Safety Considerations

- The engine watchdog sends `encode_zero()` within one RT tick if communication is lost.
- IONI Premium (`0x6051`) and ARGON (`0x6052`) are classified as V2 hardware (`is_v2_hardware() = true`) and support higher torque profiles.
- The `uses_vendor_usage_page = true` flag means this device does not rely on standard HID PID usage pages; all FFB is sent via the binary SimpleMotion V2 protocol.
- Always call `build_device_enable(false, seq)` on shutdown to release motor control.

## Resources

- [Granite Devices developer wiki — SimpleMotion V2](https://granitedevices.com/wiki/SimpleMotion_V2)
- [OpenFFBoard firmware (community reference)](https://github.com/Ultrawipf/OpenFFBoard)
- [Simucube 1 community resources](https://community.granitedevices.com/)
