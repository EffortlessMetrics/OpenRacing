# FFBeast Protocol Documentation

**Status**: ✅ Supported

FFBeast is an open-source direct-drive force feedback controller from [HF-Robotics/FFBeast](https://github.com/HF-Robotics/FFBeast). It uses standard USB HID PID (Physical Interface Device) for force effects, compatible with DirectInput on Windows and `evdev` on Linux, with vendor-defined feature reports for configuration.

## Device Identification

| Model | VID | PID | Notes |
|-------|-----|-----|-------|
| FFBeast Joystick | `0x045B` | `0x58F9` | `USB_VENDOR_ID_FFBEAST` in Linux kernel |
| FFBeast Rudder | `0x045B` | `0x5968` | |
| FFBeast Wheel | `0x045B` | `0x59D7` | |

VID `0x045B` is assigned to Hitachi, Ltd. and used by FFBeast in community builds.

## Protocol Overview

FFBeast implements standard USB HID PID for force effects and exposes vendor-defined HID feature reports (on the same interface) for device configuration. No proprietary initialization handshake is needed before sending force effects; the 2-step feature report sequence below is sufficient to arm the motor.

### Initialization Sequence

1. **Enable FFB** — write feature report `0x60` with `enable = 0x01`
2. **Set full gain** — write feature report `0x61` with `gain = 0xFF`

Both reports are 3 bytes: `[REPORT_ID, VALUE, 0x00]`.

```
Feature report 0x60 — enable FFB:
  [0x60, 0x01, 0x00]   → enable
  [0x60, 0x00, 0x00]   → disable

Feature report 0x61 — set gain:
  [0x61, 0xFF, 0x00]   → full scale (255 = 100%)
  [0x61, 0x80, 0x00]   → half scale (128 ≈ 50%)
```

### Shutdown Sequence

Write feature report `0x60` with `enable = 0x00` to cut motor power cleanly.

## Output Reports (Force Feedback)

### Constant Force Report (report ID `0x01`)

Real-time torque is delivered as a 5-byte HID output report at up to 1kHz:

```
Byte 0:   Report ID = 0x01
Bytes 1–2: signed i16 LE — torque in [-10000, 10000]
Bytes 3–4: reserved (0x00)
```

- Value `+10000` = full positive torque (device maximum)
- Value `-10000` = full negative torque
- Value `0` = no torque / safe state

**Encoding**: normalize physical torque to `[-1.0, 1.0]`, then multiply by `10000` and cast to `i16`.

```rust
let raw = (torque_normalized.clamp(-1.0, 1.0) * 10_000.0) as i16;
let [lo, hi] = raw.to_le_bytes();
[0x01, lo, hi, 0x00, 0x00]
```

## Feature Reports (Configuration)

| Report ID | Direction | Purpose |
|-----------|-----------|---------|
| `0x60` | Output | Enable / disable FFB output |
| `0x61` | Output | Set global gain `[0, 255]` |

Reports larger than 64 bytes are rejected by the transport layer.

## Default Configuration

| Parameter | Default | Notes |
|-----------|---------|-------|
| Max torque | 20 Nm | Tunable via profile; depends on motor + PSU |
| Encoder CPR | 65 535 (16-bit) | Configurable per encoder type |
| Update interval | 1 ms (1 kHz) | `required_b_interval = Some(1)` |

## OpenRacing Implementation

| Component | Location |
|-----------|----------|
| Protocol constants & encoder | `crates/hid-ffbeast-protocol/src/` |
| Vendor handler | `crates/engine/src/hid/vendor/ffbeast.rs` |
| Handler tests | `crates/engine/src/hid/vendor/ffbeast_tests.rs` |
| udev rules | `packaging/linux/99-racing-wheel-suite.rules` |

### Crate public API (`hid-ffbeast-protocol`)

```rust
// IDs
const FFBEAST_VENDOR_ID: u16 = 0x045B;
const FFBEAST_PRODUCT_ID_JOYSTICK: u16 = 0x58F9;
const FFBEAST_PRODUCT_ID_RUDDER: u16 = 0x5968;
const FFBEAST_PRODUCT_ID_WHEEL: u16 = 0x59D7;
fn is_ffbeast_product(pid: u16) -> bool;

// Output
const CONSTANT_FORCE_REPORT_ID: u8 = 0x01;
const CONSTANT_FORCE_REPORT_LEN: usize = 5;
const GAIN_REPORT_ID: u8 = 0x61;

struct FFBeastTorqueEncoder;
impl FFBeastTorqueEncoder {
    fn encode(&self, torque_normalized: f32) -> [u8; 5];
}

fn build_enable_ffb(enabled: bool) -> [u8; 3];
fn build_set_gain(gain: u8) -> [u8; 3];
```

## Safety Considerations

- Default max torque of 20 Nm is a conservative default for common FFBeast builds; high-end OSW configurations can deliver 30 Nm or more. Tune `max_torque_nm` in your device profile.
- Always send `build_enable_ffb(false)` on shutdown to prevent the motor from holding torque while the controller is unattended.
- The watchdog in the engine RT loop will send a zero-torque report within one tick if communication is lost.

## Resources

- [FFBeast GitHub repository](https://github.com/HF-Robotics/FFBeast)
- [Linux kernel `hid-ids.h` — USB_VENDOR_ID_FFBEAST](https://elixir.bootlin.com/linux/latest/source/drivers/hid/hid-ids.h)
- [USB HID PID specification](https://www.usb.org/document-library/hid-usage-tables-14)
