//! Simagic HID protocol: input parsing, device identification, and FFB encoding.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested and fuzzed without
//! hardware or OS-level HID plumbing.
//!
//! Supports Simagic wheelbases: Alpha, Alpha Mini, Alpha EVO, M10, Neo series.
//!
//! # Protocol reference
//!
//! The authoritative open-source reference for Simagic HID protocol is the
//! JacKeTUs/simagic-ff Linux kernel driver (GPL-2.0):
//! <https://github.com/JacKeTUs/simagic-ff>
//!
//! Device compatibility is tracked at:
//! <https://github.com/JacKeTUs/linux-steering-wheels>
//!
//! # Speculative wire format — READ THIS FIRST
//!
//! **This crate's report IDs and byte layouts are an abstraction layer that
//! does NOT match the actual Simagic hardware wire protocol.** The real
//! protocol, as documented in JacKeTUs/simagic-ff `hid-simagic.c` (commit
//! 52e73e7), uses HID PID (Physical Interface Device) semantics with 64-byte
//! output reports sent via `hid_hw_request`. The kernel driver report IDs are:
//!
//! | Report ID | Define                       | Purpose                          |
//! |-----------|------------------------------|----------------------------------|
//! | `0x01`    | `SM_SET_EFFECT_REPORT`       | Create/configure effect          |
//! | `0x03`    | `SM_SET_CONDITION_REPORT`    | Spring/damper/friction/inertia   |
//! | `0x04`    | `SM_SET_PERIODIC_REPORT`     | Sine waveforms                   |
//! | `0x05`    | `SM_SET_CONSTANT_REPORT`     | Constant force magnitude         |
//! | `0x0a`    | `SM_EFFECT_OPERATION_REPORT` | Play/stop effect                 |
//! | `0x12`    | `SM_SET_ENVELOPE_REPORT`     | Envelope parameters              |
//! | `0x40`    | `SM_SET_GAIN`                | Device-wide FFB gain             |
//! | `0x80`    | (settings set)               | Write wheel settings (Feature)   |
//! | `0x81`    | (settings get)               | Read wheel status (Feature)      |
//!
//! This crate instead uses custom report IDs (`0x11`–`0x17`, `0x20`–`0x40`).
//! These must be translated to the real wire protocol by the transport layer
//! before being sent to hardware. See [`output`] for per-report documentation
//! of both our abstraction and the kernel driver's actual format.
//!
//! # VID/PID summary
//!
//! - **Legacy** (VID `0x0483`, PID `0x0522`): M10, Alpha Mini, Alpha, Alpha Ultimate
//! - **EVO gen** (VID `0x3670`): EVO Sport (`0x0500`), EVO (`0x0501`), EVO Pro (`0x0502`)
//!
//! See [`ids`] module for detailed source citations.

#![deny(static_mut_refs)]
#![deny(clippy::unwrap_used)]

pub mod ids;
pub mod input;
pub mod output;
pub mod types;

pub use ids::{SIMAGIC_VENDOR_ID, product_ids};
pub use input::{SimagicInputState, parse_input_report};
pub use output::{
    CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SPRING_REPORT_LEN,
    SimagicConstantForceEncoder, SimagicDamperEncoder, SimagicFrictionEncoder,
    SimagicSpringEncoder, build_device_gain, build_led_report, build_rotation_range,
    build_sine_effect, build_square_effect, build_triangle_effect,
};
pub use types::{
    SimagicDeviceCategory, SimagicDeviceIdentity, SimagicFfbEffectType, SimagicModel,
    SimagicPedalAxes, SimagicPedalAxesRaw, SimagicShifterState, identify_device,
    is_wheelbase_product,
};

impl SimagicPedalAxesRaw {
    pub fn normalize(self) -> SimagicPedalAxes {
        const MAX: f32 = u16::MAX as f32;
        SimagicPedalAxes {
            throttle: self.throttle as f32 / MAX,
            brake: self.brake as f32 / MAX,
            clutch: self.clutch as f32 / MAX,
            handbrake: self.handbrake as f32 / MAX,
        }
    }
}
