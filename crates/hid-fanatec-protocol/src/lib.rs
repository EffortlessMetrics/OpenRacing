//! Fanatec HID protocol: report parsing, mode switching, and FFB encoding.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested and fuzzed without
//! hardware or OS-level HID plumbing.
//!
//! ## Verification sources
//!
//! VID, PIDs, and report structures have been cross-referenced against:
//! - [`gotzl/hid-fanatecff`](https://github.com/gotzl/hid-fanatecff) — Linux
//!   kernel driver for Fanatec devices (`hid-ftec.h`, `hid-ftec.c`,
//!   `hid-ftecff.c`).
//!
//! ### Protocol details confirmed from the Linux driver
//!
//! **FFB slot protocol** (`hid-ftecff.c`): The driver uses 5 effect slots
//! (constant = cmd 0x08, spring = 0x0b, damper/inertia/friction = 0x0c).
//! Slot commands are 7-byte payloads where byte 0 = `(slot_id << 4) | flags`
//! (bit 0 = active, bit 1 = disable). The `TRANSLATE_FORCE` macro encodes
//! signed force as `(value + 0x8000)`, producing an unsigned 16-bit value
//! where 0x0000 = full negative, 0x8000 = zero, 0xFFFF = full positive.
//! DD1/DD2/CSL DD use 16-bit (highres) encoding (`FTEC_HIGHRES` quirk);
//! older bases use 8-bit.
//!
//! **Rim detection** (`hid-ftec.c:ftecff_raw_event`): The attached rim ID
//! is read from byte `data[0x1f]` of the standard input report (ID 0x01).
//! On change, the driver fires a `kobject_uevent` to notify userspace.
//!
//! **LED / display / rumble** (`hid-ftecff.c`): Wheelbase LEDs use prefix
//! `[0xf8, 0x13, ...]`; wheel LEDs use `[0xf8, 0x09, 0x08, ...]`;
//! display uses `[0xf8, 0x09, 0x01, 0x02, seg1, seg2, seg3]` with
//! 7-segment encoding; rumble uses `[0xf8, 0x09, 0x01, 0x03, ...]`.
//!
//! **Stop all effects** (`hid-ftecff.c:ftecff_stop_effects`): Sends
//! `[0xf3, 0, 0, 0, 0, 0, 0]`.
//!
//! **Steering range** (`hid-ftecff.c:ftec_set_range`): Three-report
//! sequence ending with `[0xf8, 0x81, range_lo, range_hi, 0, 0, 0]`.
//! Max range varies by device: 900° for ClubSport V2/V2.5/CSR Elite,
//! 2520° for DD1/DD2/CSL DD, 1080° for CSL Elite (per `ftec_probe`).
//!
//! **Torque values**: Not present in the Linux driver. The values in this
//! crate (DD1 = 20 Nm, DD2 = 25 Nm, CSL DD/GT DD Pro = 8 Nm,
//! CSL Elite = 6 Nm, etc.) are from Fanatec's official product
//! specifications.

#![deny(static_mut_refs)]

pub mod ids;
pub mod input;
pub mod output;
pub mod types;

pub use ids::led_commands;
pub use ids::{FANATEC_VENDOR_ID, product_ids, rim_ids};
pub use input::{
    FanatecExtendedState, FanatecInputState, FanatecPedalState, parse_extended_report,
    parse_pedal_report, parse_standard_report,
};
pub use output::{
    CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder, LED_REPORT_LEN, MAX_ROTATION_DEGREES,
    MIN_ROTATION_DEGREES, build_display_report, build_kernel_range_sequence, build_led_report,
    build_mode_switch_report, build_rotation_range_report, build_rumble_report,
    build_set_gain_report, build_stop_all_report, fix_report_values,
};
pub use types::{
    FanatecModel, FanatecPedalModel, FanatecRimId, is_pedal_product, is_wheelbase_product,
};
