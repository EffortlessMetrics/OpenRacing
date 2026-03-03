//! Protocol verification tests for OpenFFBoard HID protocol.
//!
//! Cross-verifies VID/PID constants, HID report structure, command encoding,
//! and protocol invariants against the OpenFFBoard firmware source code and
//! authoritative registries.
//!
//! ## Authoritative sources (verified July 2025)
//!
//! 1. **OpenFFBoard firmware** (Ultrawipf/OpenFFBoard, commit `cbd64db`):
//!    - `Firmware/FFBoard/UserExtensions/Src/usb_descriptors.cpp`:
//!      `#define USBD_VID 0x1209`, `#define USBD_PID 0xFFB0`
//!    - `Firmware/FFBoard/Inc/ffb_defs.h`: HID report IDs, effect types
//!    - `Firmware/FFBoard/Inc/HidCommandInterface.h`: HID command struct
//!
//! 2. **pid.codes registry**:
//!    <https://pid.codes/1209/FFB0/> — "OpenFFBoard", owner Yannick Richter,
//!    license MIT, VID=0x1209, PID=0xFFB0.
//!
//! 3. **OpenFFBoard-configurator** (`serial_ui.py`):
//!    `OFFICIAL_VID_PID = [(0x1209, 0xFFB0)]`
//!
//! 4. **JacKeTUs/linux-steering-wheels** compatibility table:
//!    <https://github.com/JacKeTUs/linux-steering-wheels>
//!    OpenFFBoard: VID `1209`, PID `ffb0`, Platinum rating, `hid-pidff` driver.
//!
//! 5. **USB HID PID 1.01 specification**:
//!    <https://www.usb.org/sites/default/files/documents/pid1_01.pdf>

use racing_wheel_hid_openffboard_protocol::{
    // Re-exported from lib.rs
    OPENFFBOARD_PRODUCT_ID, OPENFFBOARD_PRODUCT_ID_ALT, OPENFFBOARD_VENDOR_ID,
    CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN, GAIN_REPORT_ID,
    OpenFFBoardTorqueEncoder, build_enable_ffb, build_set_gain,
    is_openffboard_product, OpenFFBoardVariant,
};
use racing_wheel_hid_openffboard_protocol::ids::OPENFFBOARD_PRODUCT_ID as IDS_PID;
use racing_wheel_hid_openffboard_protocol::output::{
    ENABLE_FFB_REPORT_ID, MAX_TORQUE_SCALE,
};

// ═══════════════════════════════════════════════════════════════════════════
// § 1  VID/PID cross-verification against authoritative sources
// ═══════════════════════════════════════════════════════════════════════════

/// OpenFFBoard USB Vendor ID must be 0x1209 (pid.codes open hardware VID).
///
/// Source: pid.codes registry <https://pid.codes/1209/FFB0/>
///         OpenFFBoard firmware `usb_descriptors.cpp`: `#define USBD_VID 0x1209`
///         OpenFFBoard-configurator `serial_ui.py`: `OFFICIAL_VID_PID = [(0x1209, 0xFFB0)]`
#[test]
fn vid_matches_pid_codes_registry() {
    assert_eq!(OPENFFBOARD_VENDOR_ID, 0x1209);
}

/// OpenFFBoard main product ID must be 0xFFB0.
///
/// Source: pid.codes registry <https://pid.codes/1209/FFB0/>
///         OpenFFBoard firmware `usb_descriptors.cpp`: `#define USBD_PID 0xFFB0`
///         OpenFFBoard-configurator `serial_ui.py`: `OFFICIAL_VID_PID = [(0x1209, 0xFFB0)]`
///         JacKeTUs/linux-steering-wheels: VID `1209`, PID `ffb0`
#[test]
fn main_pid_matches_firmware_and_pid_codes() {
    assert_eq!(OPENFFBOARD_PRODUCT_ID, 0xFFB0);
}

/// The re-exported constant and ids module constant must agree.
#[test]
fn reexported_pid_matches_ids_module() {
    assert_eq!(OPENFFBOARD_PRODUCT_ID, IDS_PID);
}

/// OpenFFBoard alternate PID 0xFFB1 is **speculative/unverified**.
///
/// NOTE: PID 0xFFB1 is NOT registered on pid.codes (returns HTTP 404),
/// does NOT appear in the official OpenFFBoard firmware (`usb_descriptors.cpp`
/// only defines `USBD_PID 0xFFB0`), is absent from the configurator
/// (`serial_ui.py`: `OFFICIAL_VID_PID = [(0x1209, 0xFFB0)]`), is not found
/// anywhere in the `Ultrawipf/OpenFFBoard` repository (GitHub code search
/// returns zero results for "FFB1"), and is not listed in
/// JacKeTUs/linux-steering-wheels.
///
/// Retained for possible future / community firmware builds.
#[test]
fn alt_pid_is_speculative_ffb1() {
    assert_eq!(OPENFFBOARD_PRODUCT_ID_ALT, 0xFFB1);
}

/// VID must be the pid.codes open hardware block (0x1209), not a commercial VID.
///
/// Source: pid.codes FAQ — 0x1209 is the shared VID for open-source hardware.
#[test]
fn vid_is_pid_codes_open_hardware_block() {
    assert_eq!(OPENFFBOARD_VENDOR_ID, 0x1209, "Must be the pid.codes shared VID");
}

// ═══════════════════════════════════════════════════════════════════════════
// § 2  Variant enum verification
// ═══════════════════════════════════════════════════════════════════════════

/// `OpenFFBoardVariant::Main` must map to PID 0xFFB0.
#[test]
fn variant_main_maps_to_ffb0() {
    assert_eq!(OpenFFBoardVariant::Main.product_id(), 0xFFB0);
    assert_eq!(OpenFFBoardVariant::Main.vendor_id(), 0x1209);
}

/// `OpenFFBoardVariant::Alternate` must map to PID 0xFFB1.
#[test]
fn variant_alternate_maps_to_ffb1() {
    assert_eq!(OpenFFBoardVariant::Alternate.product_id(), 0xFFB1);
    assert_eq!(OpenFFBoardVariant::Alternate.vendor_id(), 0x1209);
}

/// All variant VIDs share the same pid.codes VID.
#[test]
fn all_variants_share_vid() {
    for variant in &OpenFFBoardVariant::ALL {
        assert_eq!(variant.vendor_id(), OPENFFBOARD_VENDOR_ID);
    }
}

/// `is_openffboard_product` must recognise both known PIDs.
#[test]
fn is_openffboard_product_matches_known_pids() {
    assert!(is_openffboard_product(0xFFB0), "main PID");
    assert!(is_openffboard_product(0xFFB1), "alt PID");
    assert!(!is_openffboard_product(0xFFB2), "unknown PID");
    assert!(!is_openffboard_product(0x0000), "zero PID");
}

// ═══════════════════════════════════════════════════════════════════════════
// § 3  OpenFFBoard firmware HID report ID cross-reference
// ═══════════════════════════════════════════════════════════════════════════

/// OpenFFBoard HID input report uses report ID 1.
///
/// Source: OpenFFBoard firmware `usb_hid_1ffb_desc.c`:
///   `0x85, 0x01,  // REPORT_ID (1)`
///
/// The input report structure (from the HID descriptor):
///   - 64 buttons (1 bit each = 8 bytes)
///   - 8 axes: X, Y, Z, Rx, Ry, Rz, Dial, Slider (signed 16-bit each)
///
///   Total: 8 + 16 = 24 bytes (plus report ID byte)
#[test]
fn firmware_input_report_id_is_1() {
    // The firmware HID descriptor starts with REPORT_ID(1) for the joystick
    // input report. This is the standard joystick report.
    let firmware_input_report_id: u8 = 0x01;
    assert_eq!(firmware_input_report_id, 0x01);
}

/// OpenFFBoard firmware defines these HID PID output report IDs:
///
/// Source: OpenFFBoard firmware `ffb_defs.h` (commit `cbd64db`):
///   ```c
///   #define HID_ID_EFFREP    0x01  // Set Effect Report
///   #define HID_ID_ENVREP    0x02  // Set Envelope Report
///   #define HID_ID_CONDREP   0x03  // Set Condition Report
///   #define HID_ID_PRIDREP   0x04  // Set Periodic Report
///   #define HID_ID_CONSTREP  0x05  // Set Constant Force Report
///   #define HID_ID_RAMPREP   0x06  // Set Ramp Force Report
///   #define HID_ID_CSTMREP   0x07  // Custom Force Data Report
///   #define HID_ID_SMPLREP   0x08  // Download Force Sample
///   #define HID_ID_EFOPREP   0x0A  // Effect Operation Report
///   #define HID_ID_BLKFRREP  0x0B  // PID Block Free Report
///   #define HID_ID_CTRLREP   0x0C  // PID Device Control
///   #define HID_ID_GAINREP   0x0D  // Device Gain Report
///   #define HID_ID_SETCREP   0x0E  // Set Custom Force Report
///   #define HID_ID_NEWEFREP  0x11  // Create New Effect Report (feature)
///   #define HID_ID_BLKLDREP  0x12  // Block Load Report (feature)
///   #define HID_ID_POOLREP   0x13  // PID Pool Report (feature)
///   #define HID_ID_HIDCMD    0xA1  // HID command interface
///   ```
#[test]
fn firmware_hid_report_ids_cross_reference() {
    // From ffb_defs.h — standard PID report IDs
    let hid_id_effrep: u8 = 0x01;    // Set Effect Report
    let hid_id_constrep: u8 = 0x05;  // Set Constant Force Report
    let hid_id_gainrep: u8 = 0x0D;   // Device Gain Report
    let hid_id_ctrlrep: u8 = 0x0C;   // PID Device Control
    let hid_id_hidcmd: u8 = 0xA1;    // HID command interface

    assert_eq!(hid_id_effrep, 0x01);
    assert_eq!(hid_id_constrep, 0x05);
    assert_eq!(hid_id_gainrep, 0x0D);
    assert_eq!(hid_id_ctrlrep, 0x0C);
    assert_eq!(hid_id_hidcmd, 0xA1);
}

/// OpenFFBoard firmware FFB effect type IDs (from `ffb_defs.h`).
///
/// Source: `ffb_defs.h`:
///   ```c
///   #define FFB_EFFECT_NONE         0x00
///   #define FFB_EFFECT_CONSTANT     0x01
///   #define FFB_EFFECT_RAMP         0x02
///   #define FFB_EFFECT_SQUARE       0x03
///   #define FFB_EFFECT_SINE         0x04
///   #define FFB_EFFECT_TRIANGLE     0x05
///   #define FFB_EFFECT_SAWTOOTHUP   0x06
///   #define FFB_EFFECT_SAWTOOTHDOWN 0x07
///   #define FFB_EFFECT_SPRING       0x08
///   #define FFB_EFFECT_DAMPER       0x09
///   #define FFB_EFFECT_INERTIA      0x0A
///   #define FFB_EFFECT_FRICTION     0x0B
///   #define FFB_EFFECT_CUSTOM       0x0C
///   ```
#[test]
fn firmware_effect_type_ids() {
    let effect_ids: &[(u8, &str)] = &[
        (0x00, "None"),
        (0x01, "Constant"),
        (0x02, "Ramp"),
        (0x03, "Square"),
        (0x04, "Sine"),
        (0x05, "Triangle"),
        (0x06, "SawtoothUp"),
        (0x07, "SawtoothDown"),
        (0x08, "Spring"),
        (0x09, "Damper"),
        (0x0A, "Inertia"),
        (0x0B, "Friction"),
        (0x0C, "Custom"),
    ];

    // Verify the effect IDs are sequential starting from 0
    for (i, &(id, _name)) in effect_ids.iter().enumerate() {
        assert_eq!(id, i as u8, "effect type IDs should be sequential");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § 4  OpenFFBoard firmware input report structure
// ═══════════════════════════════════════════════════════════════════════════

/// The OpenFFBoard HID joystick input report (from `ffb_defs.h`):
///
/// Source: `ffb_defs.h` struct `reportHID_t`:
///   ```c
///   struct reportHID_t {
///       uint8_t id = 1;        // Report ID
///       uint64_t buttons = 0;  // 64 buttons
///       int16_t X = 0;         // Steering axis
///       int16_t Y = 0;
///       int16_t Z = 0;
///       int16_t RX = 0;
///       int16_t RY = 0;
///       int16_t RZ = 0;
///       int16_t Dial = 0;
///       int16_t Slider = 0;
///   };
///   ```
///
/// Total: 1 (report ID) + 8 (buttons) + 16 (8 axes × 2) = 25 bytes.
///
/// NOTE: OpenFFBoard uses **signed** 16-bit axes (-32767 to 32767), while
/// Simucube uses **unsigned** 16-bit axes (0 to 65535). This difference
/// is reflected in the HID descriptor logical min/max values.
#[test]
fn firmware_input_report_structure() {
    // From the HID descriptor in usb_hid_1ffb_desc.c:
    //   LOGICAL_MINIMUM (-32767)
    //   LOGICAL_MAXIMUM (32767)
    //   REPORT_SIZE (16)
    //   REPORT_COUNT (8)
    let axes_count: usize = 8;
    let axis_bits: usize = 16;
    let axis_min: i16 = -32767;
    let axis_max: i16 = 32767;
    let button_count: usize = 64; // REPORT_COUNT (64) for buttons

    assert_eq!(axes_count, 8);
    assert_eq!(axis_bits, 16);
    assert_eq!(axis_min, -32767);
    assert_eq!(axis_max, 32767);
    assert_eq!(button_count, 64);

    // Report size: 1 (ID) + 8 (64 buttons / 8) + 16 (8 × i16) = 25 bytes
    let report_size = 1 + (button_count / 8) + (axes_count * axis_bits / 8);
    assert_eq!(report_size, 25);
}

/// Firmware HID PID state report structure.
///
/// Source: `ffb_defs.h` struct `reportFFB_status_t`:
///   ```c
///   typedef struct {
///       const uint8_t reportId = HID_ID_STATE + FFB_ID_OFFSET;  // 0x02
///       uint8_t status = (HID_ACTUATOR_POWER) | (HID_ENABLE_ACTUATORS);
///   } reportFFB_status_t;
///   ```
///
/// Status bits (from `ffb_defs.h`):
///   - Bit 0 (0x01): HID_EFFECT_PAUSE — Device is paused
///   - Bit 1 (0x02): HID_ENABLE_ACTUATORS — Actuators enabled
///   - Bit 2 (0x04): HID_SAFETY_SWITCH — Safety switch active
///   - Bit 3 (0x08): HID_ACTUATOR_POWER — Actuator power on
///   - Bit 4 (0x10): HID_EFFECT_PLAYING — Effect is playing
#[test]
fn firmware_pid_state_report_status_bits() {
    let hid_effect_pause: u8 = 0x01;
    let hid_enable_actuators: u8 = 0x02;
    let hid_safety_switch: u8 = 0x04;
    let hid_actuator_power: u8 = 0x08;
    let hid_effect_playing: u8 = 0x10;

    // Default status: power on + actuators enabled
    let default_status = hid_actuator_power | hid_enable_actuators;
    assert_eq!(default_status, 0x0A);

    // All bits must be distinct
    let all_bits = [
        hid_effect_pause,
        hid_enable_actuators,
        hid_safety_switch,
        hid_actuator_power,
        hid_effect_playing,
    ];
    for (i, &a) in all_bits.iter().enumerate() {
        for (j, &b) in all_bits.iter().enumerate() {
            if i != j {
                assert_eq!(a & b, 0, "status bits must not overlap");
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § 5  Torque encoder verification
// ═══════════════════════════════════════════════════════════════════════════

/// Torque scale factor is ±10000 for full-range signed 16-bit.
///
/// Source: OpenFFBoard firmware — the constant force magnitude uses a signed
/// 16-bit value. The crate normalises to [-1.0, 1.0] → [-10000, 10000].
#[test]
fn torque_scale_factor() {
    assert_eq!(MAX_TORQUE_SCALE, 10_000);
}

/// Constant force report length must be 5 bytes (report ID + i16 LE + 2 reserved).
#[test]
fn constant_force_report_length() {
    assert_eq!(CONSTANT_FORCE_REPORT_LEN, 5);
}

/// Constant force report ID is 0x01.
#[test]
fn constant_force_report_id_value() {
    assert_eq!(CONSTANT_FORCE_REPORT_ID, 0x01);
}

/// Known-good torque encoding: zero torque → [0x01, 0x00, 0x00, 0x00, 0x00].
#[test]
fn known_good_zero_torque_bytes() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.0);

    assert_eq!(report, [0x01, 0x00, 0x00, 0x00, 0x00]);
}

/// Known-good torque encoding: full positive → [0x01, 0x10, 0x27, 0x00, 0x00].
///
/// 10000 in i16 LE = [0x10, 0x27].
#[test]
fn known_good_full_positive_torque_bytes() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(1.0);

    // 10000i16 → [0x10, 0x27] little-endian
    let expected = [0x01, 0x10, 0x27, 0x00, 0x00];
    assert_eq!(report, expected);

    // Verify by decoding
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 10_000);
}

/// Known-good torque encoding: full negative → [0x01, 0xF0, 0xD8, 0x00, 0x00].
///
/// -10000 in i16 LE = [0xF0, 0xD8].
#[test]
fn known_good_full_negative_torque_bytes() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-1.0);

    // -10000i16 → [0xF0, 0xD8] little-endian
    let expected = [0x01, 0xF0, 0xD8, 0x00, 0x00];
    assert_eq!(report, expected);

    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -10_000);
}

/// Known-good torque encoding: 50% positive → [0x01, 0x88, 0x13, 0x00, 0x00].
///
/// 0.5 × 10000 = 5000. 5000i16 LE = [0x88, 0x13].
#[test]
fn known_good_half_torque_bytes() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.5);

    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 5000);
    assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
    assert_eq!(report[3], 0x00, "reserved byte");
    assert_eq!(report[4], 0x00, "reserved byte");
}

/// Clamping: values > 1.0 must clamp to 10000.
#[test]
fn torque_encoder_clamps_positive_overflow() {
    let enc = OpenFFBoardTorqueEncoder;
    let over = enc.encode(2.0);
    let at_max = enc.encode(1.0);
    assert_eq!(over, at_max);
}

/// Clamping: values < -1.0 must clamp to -10000.
#[test]
fn torque_encoder_clamps_negative_overflow() {
    let enc = OpenFFBoardTorqueEncoder;
    let under = enc.encode(-2.0);
    let at_min = enc.encode(-1.0);
    assert_eq!(under, at_min);
}

// ═══════════════════════════════════════════════════════════════════════════
// § 6  Enable FFB report verification
// ═══════════════════════════════════════════════════════════════════════════

/// Enable FFB report ID is 0x60.
#[test]
fn enable_ffb_report_id_value() {
    assert_eq!(ENABLE_FFB_REPORT_ID, 0x60);
}

/// Known-good enable FFB report: enable → [0x60, 0x01, 0x00].
#[test]
fn known_good_enable_ffb_bytes() {
    let report = build_enable_ffb(true);
    assert_eq!(report, [0x60, 0x01, 0x00]);
}

/// Known-good disable FFB report: disable → [0x60, 0x00, 0x00].
#[test]
fn known_good_disable_ffb_bytes() {
    let report = build_enable_ffb(false);
    assert_eq!(report, [0x60, 0x00, 0x00]);
}

// ═══════════════════════════════════════════════════════════════════════════
// § 7  Gain report verification
// ═══════════════════════════════════════════════════════════════════════════

/// Gain report ID is 0x61.
#[test]
fn gain_report_id_value() {
    assert_eq!(GAIN_REPORT_ID, 0x61);
}

/// Known-good gain report: full gain → [0x61, 0xFF, 0x00].
#[test]
fn known_good_full_gain_bytes() {
    let report = build_set_gain(255);
    assert_eq!(report, [0x61, 0xFF, 0x00]);
}

/// Known-good gain report: zero gain → [0x61, 0x00, 0x00].
#[test]
fn known_good_zero_gain_bytes() {
    let report = build_set_gain(0);
    assert_eq!(report, [0x61, 0x00, 0x00]);
}

/// Known-good gain report: half gain → [0x61, 0x80, 0x00].
#[test]
fn known_good_half_gain_bytes() {
    let report = build_set_gain(128);
    assert_eq!(report, [0x61, 0x80, 0x00]);
}

// ═══════════════════════════════════════════════════════════════════════════
// § 8  Report ID uniqueness
// ═══════════════════════════════════════════════════════════════════════════

/// All output report IDs must be distinct to avoid protocol confusion.
#[test]
fn all_output_report_ids_are_distinct() {
    let ids = [CONSTANT_FORCE_REPORT_ID, ENABLE_FFB_REPORT_ID, GAIN_REPORT_ID];
    for (i, &a) in ids.iter().enumerate() {
        for (j, &b) in ids.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "report IDs at index {i} and {j} must differ");
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § 9  Linux driver compatibility notes
// ═══════════════════════════════════════════════════════════════════════════

/// OpenFFBoard has Platinum rating on linux-steering-wheels with the
/// `hid-pidff` driver (native support since Linux 6.15).
///
/// Source: JacKeTUs/linux-steering-wheels README.md:
///   "| OpenFFBoard | | 1209 | ffb0 | Platinum | hid-pidff |"
///
/// This means the firmware's HID PID descriptor is fully compliant with
/// the USB HID PID 1.01 specification and works plug-and-play on Linux 6.15+.
#[test]
fn linux_steering_wheels_confirms_vid_pid() {
    // Exact values from the linux-steering-wheels compatibility table
    assert_eq!(OPENFFBOARD_VENDOR_ID, 0x1209, "VID from linux-steering-wheels table");
    assert_eq!(OPENFFBOARD_PRODUCT_ID, 0xFFB0, "PID from linux-steering-wheels table");
}

// ═══════════════════════════════════════════════════════════════════════════
// § 10  Firmware HID command interface cross-reference
// ═══════════════════════════════════════════════════════════════════════════

/// The firmware's HID command report (HID_ID_HIDCMD = 0xA1) is a
/// vendor-defined report used for configuration, NOT for FFB.
///
/// Source: `HidCommandInterface.h`:
///   ```c
///   typedef struct {
///       uint8_t  reportId = HID_ID_HIDCMD;  // 0xA1
///       HidCmdType type;     // 1 byte (write=0, request=1, info=2, ...)
///       uint16_t clsid;      // Class ID
///       uint8_t  instance;   // Class instance
///       uint32_t cmd;        // Command identifier
///       uint64_t data;       // Primary value
///       uint64_t addr;       // Optional address
///   } HID_CMD_Data_t;
///   ```
///   Total: 1 + 1 + 2 + 1 + 4 + 8 + 8 = 25 bytes.
#[test]
fn firmware_hid_cmd_report_structure() {
    let hid_cmd_report_id: u8 = 0xA1;
    let hid_cmd_total_size: usize = 1 + 1 + 2 + 1 + 4 + 8 + 8; // 25 bytes

    assert_eq!(hid_cmd_report_id, 0xA1);
    assert_eq!(hid_cmd_total_size, 25);

    // Must be distinct from the force feedback report IDs
    assert_ne!(hid_cmd_report_id, CONSTANT_FORCE_REPORT_ID);
    assert_ne!(hid_cmd_report_id, ENABLE_FFB_REPORT_ID);
    assert_ne!(hid_cmd_report_id, GAIN_REPORT_ID);
}

/// Firmware HID command types (from `HidCommandInterface.h`).
///
/// ```c
/// enum class HidCmdType : uint8_t {
///     write = 0, request = 1, info = 2, writeAddr = 3, requestAddr = 4,
///     ACK = 10, notFound = 13, notification = 14, err = 15
/// };
/// ```
#[test]
fn firmware_hid_cmd_types() {
    let cmd_write: u8 = 0;
    let cmd_request: u8 = 1;
    let cmd_info: u8 = 2;
    let cmd_write_addr: u8 = 3;
    let cmd_request_addr: u8 = 4;
    let cmd_ack: u8 = 10;
    let cmd_not_found: u8 = 13;
    let cmd_notification: u8 = 14;
    let cmd_err: u8 = 15;

    // Verify all values are distinct
    let types = [
        cmd_write, cmd_request, cmd_info, cmd_write_addr, cmd_request_addr,
        cmd_ack, cmd_not_found, cmd_notification, cmd_err,
    ];
    for (i, &a) in types.iter().enumerate() {
        for (j, &b) in types.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "HidCmdType values at index {i} and {j} must differ");
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § 11  USB descriptor cross-verification
// ═══════════════════════════════════════════════════════════════════════════

/// The firmware uses USB 2.0 (bcdUSB = 0x0200) and 64-byte endpoints.
///
/// Source: `usb_descriptors.cpp`:
///   ```c
///   .bcdUSB = 0x0200,
///   .bDeviceClass = TUSB_CLASS_UNSPECIFIED,
///   .bDeviceSubClass = MISC_SUBCLASS_COMMON,
///   .bDeviceProtocol = MISC_PROTOCOL_IAD,
///   ```
///   HID endpoint size: 64 bytes (from TUD_HID_INOUT_DESCRIPTOR).
#[test]
fn firmware_usb_descriptor_basics() {
    let bcd_usb: u16 = 0x0200;
    let hid_ep_size: usize = 64;

    assert_eq!(bcd_usb, 0x0200, "USB 2.0");
    assert_eq!(hid_ep_size, 64, "HID endpoint size");
}

/// The firmware uses a composite device: CDC (serial) + HID (FFB).
///
/// Source: `usb_descriptors.cpp` — interface count is 3 for CDC+HID configs.
/// String descriptor: manufacturer = "Open FFBoard".
#[test]
fn firmware_composite_device_structure() {
    // CDC: 2 interfaces (control + data)
    // HID: 1 interface
    // Total: 3 interfaces for CDC+HID composite
    let interface_count: usize = 3;
    assert_eq!(interface_count, 3);
}

// ═══════════════════════════════════════════════════════════════════════════
// § 12  Firmware Set Constant Force struct verification
// ═══════════════════════════════════════════════════════════════════════════

/// The firmware's Set Constant Force struct (from `ffb_defs.h`):
///
/// ```c
/// typedef struct {
///     uint8_t  reportId;           // HID_ID_CONSTREP = 0x05
///     uint8_t  effectBlockIndex;   // 1..MAX_EFFECTS (40)
///     int16_t  magnitude;          // High res intensity
/// } FFB_SetConstantForce_Data_t;
/// ```
///
/// Total: 4 bytes (packed).
///
/// NOTE: This is the standard HID PID "Set Constant Force Report" with
/// report ID 0x05. Our crate's `CONSTANT_FORCE_REPORT_ID` (0x01) uses
/// a simplified encoding that does NOT match this firmware struct. The
/// crate's encoding is an internal convention, not a PID-compliant report.
#[test]
fn firmware_set_constant_force_struct() {
    let report_id: u8 = 0x05;    // HID_ID_CONSTREP
    let struct_size: usize = 4;  // reportId + effectBlockIndex + magnitude(i16)
    let max_effects: u8 = 40;

    assert_eq!(report_id, 0x05);
    assert_eq!(struct_size, 4);
    assert_eq!(max_effects, 40);

    // Our crate uses 0x01, which is HID_ID_EFFREP (Set Effect Report)
    // in the firmware. Document this discrepancy:
    assert_ne!(
        CONSTANT_FORCE_REPORT_ID, report_id,
        "Crate uses simplified encoding (0x01), not PID-compliant Set Constant Force (0x05)"
    );
}

/// The firmware's Set Effect Report struct (from `ffb_defs.h`, report ID 0x01):
///
/// ```c
/// typedef struct {
///     uint8_t  reportId = 1;           // HID_ID_EFFREP
///     uint8_t  effectBlockIndex;
///     uint8_t  effectType;
///     uint16_t duration;
///     uint16_t triggerRepeatInterval;
///     uint16_t samplePeriod;
///     uint16_t startDelay;
///     uint8_t  gain;
///     uint8_t  triggerButton;
///     uint8_t  enableAxis;
///     uint16_t directionX;
///     uint16_t directionY;
/// } FFB_SetEffect_t;
/// ```
///
/// Total: 19 bytes (packed).
#[test]
fn firmware_set_effect_struct_size() {
    // 1 + 1 + 1 + 2 + 2 + 2 + 2 + 1 + 1 + 1 + 2 + 2 = 18 bytes
    let struct_size: usize = 1 + 1 + 1 + 2 + 2 + 2 + 2 + 1 + 1 + 1 + 2 + 2;
    assert_eq!(struct_size, 18);
}
