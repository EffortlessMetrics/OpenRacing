//! Cross-check tests for Cammus constants against the Linux kernel
//! mainline (`hid-ids.h`, `hid-universal-pidff.c`).
//!
//! Cammus VID/PIDs are confirmed in the upstream kernel (≥6.15).
//! These tests pin the IDs and verify the PIDFF re-exports.

use racing_wheel_hid_cammus_protocol::ids::{
    PRODUCT_C5, PRODUCT_C12, PRODUCT_CP5_PEDALS, PRODUCT_LC100_PEDALS, VENDOR_ID,
};

// ── VID/PID — Linux kernel mainline confirmed ──────────────────────────────

/// USB_VENDOR_ID_CAMMUS = 0x3416.
#[test]
fn vendor_id_matches_kernel() {
    assert_eq!(VENDOR_ID, 0x3416, "USB_VENDOR_ID_CAMMUS");
}

/// USB_DEVICE_ID_CAMMUS_C5 = 0x0301.
#[test]
fn c5_pid_matches_kernel() {
    assert_eq!(PRODUCT_C5, 0x0301, "USB_DEVICE_ID_CAMMUS_C5");
}

/// USB_DEVICE_ID_CAMMUS_C12 = 0x0302.
#[test]
fn c12_pid_matches_kernel() {
    assert_eq!(PRODUCT_C12, 0x0302, "USB_DEVICE_ID_CAMMUS_C12");
}

/// CP5 Pedals PID confirmed via simracing-hwdb (v3416p1018).
#[test]
fn cp5_pedals_pid_matches_community() {
    assert_eq!(PRODUCT_CP5_PEDALS, 0x1018);
}

/// LC100 Pedals PID confirmed via simracing-hwdb (v3416p1019).
#[test]
fn lc100_pedals_pid_matches_community() {
    assert_eq!(PRODUCT_LC100_PEDALS, 0x1019);
}

// ── PIDFF standard report IDs (from effects module / pidff-common) ─────────

/// Cammus uses standard USB HID PID — verify the re-exported constants
/// match the PID 1.01 specification values.
#[test]
fn pidff_report_ids_match_spec() {
    use racing_wheel_hid_cammus_protocol::effects::report_ids;
    assert_eq!(report_ids::SET_EFFECT, 0x01);
    assert_eq!(report_ids::SET_ENVELOPE, 0x02);
    assert_eq!(report_ids::SET_CONDITION, 0x03);
    assert_eq!(report_ids::SET_PERIODIC, 0x04);
    assert_eq!(report_ids::SET_CONSTANT_FORCE, 0x05);
    assert_eq!(report_ids::SET_RAMP_FORCE, 0x06);
    assert_eq!(report_ids::EFFECT_OPERATION, 0x0A);
    assert_eq!(report_ids::BLOCK_FREE, 0x0B);
    assert_eq!(report_ids::DEVICE_CONTROL, 0x0C);
    assert_eq!(report_ids::DEVICE_GAIN, 0x0D);
}

/// PIDFF effect type enum values.
#[test]
fn pidff_effect_types_match_spec() {
    use racing_wheel_hid_cammus_protocol::effects::EffectType;
    assert_eq!(EffectType::Constant as u8, 1);
    assert_eq!(EffectType::Ramp as u8, 2);
    assert_eq!(EffectType::Square as u8, 3);
    assert_eq!(EffectType::Sine as u8, 4);
    assert_eq!(EffectType::Triangle as u8, 5);
    assert_eq!(EffectType::SawtoothUp as u8, 6);
    assert_eq!(EffectType::SawtoothDown as u8, 7);
    assert_eq!(EffectType::Spring as u8, 8);
    assert_eq!(EffectType::Damper as u8, 9);
    assert_eq!(EffectType::Inertia as u8, 10);
    assert_eq!(EffectType::Friction as u8, 11);
}

// ── PID uniqueness ─────────────────────────────────────────────────────────

/// All Cammus product IDs must be distinct.
#[test]
fn all_product_ids_are_unique() {
    let all: &[(u16, &str)] = &[
        (PRODUCT_C5, "C5"),
        (PRODUCT_C12, "C12"),
        (PRODUCT_CP5_PEDALS, "CP5_PEDALS"),
        (PRODUCT_LC100_PEDALS, "LC100_PEDALS"),
    ];
    for (i, &(a, na)) in all.iter().enumerate() {
        for &(b, nb) in &all[i + 1..] {
            assert_ne!(a, b, "duplicate PID {a:#06x}: {na} and {nb}");
        }
    }
}

// ── Encoder cross-checks ──────────────────────────────────────────────────

/// Constant force encoder produces correct PIDFF report ID.
#[test]
fn constant_force_report_id() -> Result<(), Box<dyn std::error::Error>> {
    let buf = racing_wheel_hid_cammus_protocol::encode_set_constant_force(1, -5000);
    assert_eq!(buf[0], 0x05, "SET_CONSTANT_FORCE report ID");
    Ok(())
}

/// Device control encoder produces correct PIDFF report ID.
#[test]
fn device_control_report_id() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_cammus_protocol::effects::device_control;
    let buf =
        racing_wheel_hid_cammus_protocol::encode_device_control(device_control::ENABLE_ACTUATORS);
    assert_eq!(buf[0], 0x0C, "DEVICE_CONTROL report ID");
    Ok(())
}
