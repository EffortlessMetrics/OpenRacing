//! Cross-check tests for VRS DirectForce Pro constants against the
//! Linux kernel mainline (`hid-ids.h`, `hid-universal-pidff.c`).
//!
//! These tests pin VID/PIDs and verify the relationship between the
//! two report_ids modules in this crate.

use racing_wheel_hid_vrs_protocol::ids::{VRS_PRODUCT_ID, VRS_VENDOR_ID, product_ids, report_ids};

// ── VID/PID — Linux kernel mainline confirmed ──────────────────────────────

/// USB_VENDOR_ID_VRS = 0x0483 (STMicroelectronics, shared with Simagic legacy).
#[test]
fn vendor_id_matches_kernel() {
    assert_eq!(VRS_VENDOR_ID, 0x0483, "USB_VENDOR_ID_VRS");
}

/// USB_DEVICE_ID_VRS_DFP = 0xa355.
#[test]
fn dfp_pid_matches_kernel() {
    assert_eq!(VRS_PRODUCT_ID, 0xA355, "USB_DEVICE_ID_VRS_DFP");
    assert_eq!(product_ids::DIRECTFORCE_PRO, 0xA355);
}

/// USB_DEVICE_ID_VRS_R295 = 0xa44c.
#[test]
fn r295_pid_matches_kernel() {
    assert_eq!(product_ids::R295, 0xA44C, "USB_DEVICE_ID_VRS_R295");
}

/// Pedals PID confirmed via simracing-hwdb (v0483pA3BE).
#[test]
fn pedals_pid_matches_community() {
    assert_eq!(product_ids::PEDALS, 0xA3BE);
}

// ── Report IDs — device-specific HID descriptor values ─────────────────────

/// These report IDs are the USB HID Report IDs from the VRS DFP's
/// HID descriptor. They are used in `output.rs` to build raw reports.
#[test]
fn report_ids_pin_values() {
    assert_eq!(report_ids::STANDARD_INPUT, 0x01);
    assert_eq!(report_ids::SET_EFFECT, 0x02);
    assert_eq!(report_ids::EFFECT_OPERATION, 0x0A);
    assert_eq!(report_ids::DEVICE_CONTROL, 0x0B);
    assert_eq!(report_ids::CONSTANT_FORCE, 0x11);
    assert_eq!(report_ids::RAMP_FORCE, 0x13);
    assert_eq!(report_ids::SQUARE_EFFECT, 0x14);
    assert_eq!(report_ids::SINE_EFFECT, 0x15);
    assert_eq!(report_ids::TRIANGLE_EFFECT, 0x16);
    assert_eq!(report_ids::SAWTOOTH_UP_EFFECT, 0x17);
    assert_eq!(report_ids::SAWTOOTH_DOWN_EFFECT, 0x18);
    assert_eq!(report_ids::SPRING_EFFECT, 0x19);
    assert_eq!(report_ids::DAMPER_EFFECT, 0x1A);
    assert_eq!(report_ids::FRICTION_EFFECT, 0x1B);
    assert_eq!(report_ids::CUSTOM_FORCE_EFFECT, 0x1C);
    assert_eq!(report_ids::DOWNLOAD_FORCE_SAMPLE, 0x22);
    assert_eq!(report_ids::SET_REPORT, 0x0C);
    assert_eq!(report_ids::GET_REPORT, 0x0D);
}

// ── PIDFF standard report IDs (from effects module / pidff-common) ─────────

/// The effects module re-exports standard PIDFF report IDs from pidff-common.
/// These are the logical PIDFF usage IDs from the USB HID PID 1.01 spec
/// and differ from the device-specific HID Report IDs in `ids::report_ids`.
#[test]
fn pidff_standard_report_ids() {
    use racing_wheel_hid_vrs_protocol::effects::report_ids as pidff;
    assert_eq!(pidff::SET_EFFECT, 0x01, "PID spec: Set Effect");
    assert_eq!(pidff::SET_ENVELOPE, 0x02, "PID spec: Set Envelope");
    assert_eq!(pidff::SET_CONDITION, 0x03, "PID spec: Set Condition");
    assert_eq!(pidff::SET_PERIODIC, 0x04, "PID spec: Set Periodic");
    assert_eq!(
        pidff::SET_CONSTANT_FORCE,
        0x05,
        "PID spec: Set Constant Force"
    );
    assert_eq!(pidff::SET_RAMP_FORCE, 0x06, "PID spec: Set Ramp Force");
    assert_eq!(pidff::EFFECT_OPERATION, 0x0A, "PID spec: Effect Operation");
    assert_eq!(pidff::BLOCK_FREE, 0x0B, "PID spec: Block Free");
    assert_eq!(pidff::DEVICE_CONTROL, 0x0C, "PID spec: Device Control");
    assert_eq!(pidff::DEVICE_GAIN, 0x0D, "PID spec: Device Gain");
}

/// Verify that the two report_ids modules have the documented relationship:
/// ids::report_ids are device-specific HID Report IDs,
/// effects::report_ids are standard PIDFF usage IDs.
/// They MUST NOT be confused — this test ensures they remain distinct.
#[test]
fn dual_report_ids_are_distinct() {
    use racing_wheel_hid_vrs_protocol::effects::report_ids as pidff;

    // Constant force: HID Report ID 0x11 vs PIDFF usage ID 0x05
    assert_ne!(
        report_ids::CONSTANT_FORCE,
        pidff::SET_CONSTANT_FORCE,
        "device-specific and PIDFF report IDs must differ for constant force"
    );
    // Device control: HID Report ID 0x0B vs PIDFF usage ID 0x0C
    assert_ne!(
        report_ids::DEVICE_CONTROL,
        pidff::DEVICE_CONTROL,
        "device-specific and PIDFF report IDs must differ for device control"
    );
}

// ── Unverified PIDs — pin values to detect accidental changes ──────────────

#[test]
fn unverified_pids_pinned() {
    assert_eq!(product_ids::DIRECTFORCE_PRO_V2, 0xA356);
    assert_eq!(product_ids::PEDALS_V2, 0xA358);
    assert_eq!(product_ids::HANDBRAKE, 0xA359);
    assert_eq!(product_ids::SHIFTER, 0xA35A);
}

// ── PID uniqueness ─────────────────────────────────────────────────────────

/// All VRS product IDs must be distinct.
#[test]
fn all_product_ids_are_unique() {
    let all: &[(u16, &str)] = &[
        (product_ids::DIRECTFORCE_PRO, "DFP"),
        (product_ids::DIRECTFORCE_PRO_V2, "DFP_V2"),
        (product_ids::R295, "R295"),
        (product_ids::PEDALS, "PEDALS"),
        (product_ids::PEDALS_V2, "PEDALS_V2"),
        (product_ids::HANDBRAKE, "HANDBRAKE"),
        (product_ids::SHIFTER, "SHIFTER"),
    ];
    for (i, &(a, na)) in all.iter().enumerate() {
        for &(b, nb) in &all[i + 1..] {
            assert_ne!(a, b, "duplicate PID {a:#06x}: {na} and {nb}");
        }
    }
}
