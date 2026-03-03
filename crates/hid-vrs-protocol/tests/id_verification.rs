//! Cross-reference tests for VRS DirectForce Pro VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_vrs_protocol::{VRS_PRODUCT_ID, VRS_VENDOR_ID};

/// VRS VID must be 0x0483 (STMicroelectronics, shared with Simagic legacy).
///
/// Source: USB VID registry; VRS DirectForce Pro community reports.
#[test]
fn vendor_id_is_0483() {
    assert_eq!(
        VRS_VENDOR_ID, 0x0483,
        "VRS VID changed — update ids.rs and SOURCES.md"
    );
}

/// VRS DirectForce Pro PID must be 0xA355.
///
/// Source: JacKeTUs/linux-steering-wheels (Platinum support entry).
#[test]
fn directforce_pro_pid_is_a355() {
    assert_eq!(
        VRS_PRODUCT_ID, 0xA355,
        "VRS DirectForce Pro PID changed — update ids.rs and SOURCES.md"
    );
}

/// All known VRS product IDs must be unique (exhaustive, deterministic check).
#[test]
fn all_pids_unique_exhaustive() {
    use racing_wheel_hid_vrs_protocol::product_ids;

    let pids: [(&str, u16); 8] = [
        ("DIRECTFORCE_PRO", product_ids::DIRECTFORCE_PRO),
        ("DIRECTFORCE_PRO_V2", product_ids::DIRECTFORCE_PRO_V2),
        ("R295", product_ids::R295),
        ("PEDALS", product_ids::PEDALS),
        ("PEDALS_V1", product_ids::PEDALS_V1),
        ("PEDALS_V2", product_ids::PEDALS_V2),
        ("HANDBRAKE", product_ids::HANDBRAKE),
        ("SHIFTER", product_ids::SHIFTER),
    ];

    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(
                pids[i].1, pids[j].1,
                "PID collision: {} (0x{:04X}) == {} (0x{:04X})",
                pids[i].0, pids[i].1, pids[j].0, pids[j].1
            );
        }
    }
}

/// VRS_VENDOR_ID must not equal any known product ID.
#[test]
fn vid_not_pid_collision() {
    use racing_wheel_hid_vrs_protocol::product_ids;

    let pids = [
        product_ids::DIRECTFORCE_PRO,
        product_ids::DIRECTFORCE_PRO_V2,
        product_ids::R295,
        product_ids::PEDALS,
        product_ids::PEDALS_V1,
        product_ids::PEDALS_V2,
        product_ids::HANDBRAKE,
        product_ids::SHIFTER,
    ];

    for &pid in &pids {
        assert_ne!(
            VRS_VENDOR_ID, pid,
            "VRS_VENDOR_ID (0x{:04X}) collides with PID 0x{:04X}",
            VRS_VENDOR_ID, pid
        );
    }
}
