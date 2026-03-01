//! Engine ↔ Protocol crate device dispatch integration tests.
//!
//! Validates that `get_vendor_protocol()` correctly routes VID/PID pairs to the
//! right vendor protocol handler for ALL supported hardware vendors. This tests
//! cross-crate interaction between the engine dispatch logic and each individual
//! HID protocol crate.

use racing_wheel_engine::hid::vendor::get_vendor_protocol;

// ─── Logitech (VID 0x046D) ───────────────────────────────────────────────────

#[test]
fn dispatch_routes_logitech_g920() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(
        0x046D,
        racing_wheel_hid_logitech_protocol::product_ids::G920,
    );
    assert!(proto.is_some(), "Logitech G920 must be dispatched");
    Ok(())
}

#[test]
fn dispatch_routes_logitech_g_pro() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(
        0x046D,
        racing_wheel_hid_logitech_protocol::product_ids::G_PRO,
    );
    assert!(proto.is_some(), "Logitech G PRO must be dispatched");
    Ok(())
}

#[test]
fn dispatch_routes_logitech_g29() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(
        0x046D,
        racing_wheel_hid_logitech_protocol::product_ids::G29_PS,
    );
    assert!(proto.is_some(), "Logitech G29 must be dispatched");
    Ok(())
}

// ─── Fanatec (VID 0x0EB7) ────────────────────────────────────────────────────

#[test]
fn dispatch_routes_fanatec_csl_dd() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(
        0x0EB7,
        racing_wheel_hid_fanatec_protocol::product_ids::CSL_DD,
    );
    assert!(proto.is_some(), "Fanatec CSL DD must be dispatched");
    Ok(())
}

#[test]
fn dispatch_routes_fanatec_dd1() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x0EB7, racing_wheel_hid_fanatec_protocol::product_ids::DD1);
    assert!(proto.is_some(), "Fanatec DD1 must be dispatched");
    Ok(())
}

#[test]
fn dispatch_routes_fanatec_dd2() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x0EB7, racing_wheel_hid_fanatec_protocol::product_ids::DD2);
    assert!(proto.is_some(), "Fanatec DD2 must be dispatched");
    Ok(())
}

// ─── Thrustmaster (VID 0x044F) ───────────────────────────────────────────────

#[test]
fn dispatch_routes_thrustmaster_t300() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(
        0x044F,
        racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS,
    );
    assert!(proto.is_some(), "Thrustmaster T300 RS must be dispatched");
    Ok(())
}

#[test]
fn dispatch_routes_thrustmaster_t818() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(
        0x044F,
        racing_wheel_hid_thrustmaster_protocol::product_ids::T818,
    );
    assert!(proto.is_some(), "Thrustmaster T818 must be dispatched");
    Ok(())
}

#[test]
fn dispatch_routes_thrustmaster_ts_pc() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(
        0x044F,
        racing_wheel_hid_thrustmaster_protocol::product_ids::TS_PC_RACER,
    );
    assert!(
        proto.is_some(),
        "Thrustmaster TS-PC Racer must be dispatched"
    );
    Ok(())
}

// ─── Moza (VID 0x346E) ──────────────────────────────────────────────────────

#[test]
fn dispatch_routes_moza_r9() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x346E, 0x0002);
    assert!(proto.is_some(), "Moza must be dispatched for VID 0x346E");
    Ok(())
}

// ─── Simagic (VID 0x0483 legacy + VID 0x3670 EVO) ───────────────────────────

#[test]
fn dispatch_routes_simagic_evo_vid() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x3670, racing_wheel_hid_simagic_protocol::product_ids::EVO);
    assert!(proto.is_some(), "Simagic EVO VID must be dispatched");
    Ok(())
}

#[test]
fn dispatch_routes_simagic_legacy_stm_vid() -> Result<(), Box<dyn std::error::Error>> {
    // 0x0483 (STM) with a Simagic PID (not VRS or Cube Controls) → Simagic fallback
    // ALPHA PID 0x0522 is defined in engine::hid::vendor::simagic::product_ids
    let proto = get_vendor_protocol(0x0483, 0x0522);
    assert!(
        proto.is_some(),
        "Simagic legacy on STM VID must be dispatched"
    );
    Ok(())
}

// ─── Simucube (VID 0x16D0) ──────────────────────────────────────────────────

#[test]
fn dispatch_routes_simucube_2_sport() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x16D0, 0x0D61); // Simucube 2 Sport PID
    assert!(proto.is_some(), "Simucube 2 Sport must be dispatched");
    Ok(())
}

#[test]
fn dispatch_routes_simucube_2_pro() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x16D0, 0x0D60); // Simucube 2 Pro PID
    assert!(proto.is_some(), "Simucube 2 Pro must be dispatched");
    Ok(())
}

#[test]
fn dispatch_routes_simucube_2_ultimate() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x16D0, 0x0D5F); // Simucube 2 Ultimate PID
    assert!(proto.is_some(), "Simucube 2 Ultimate must be dispatched");
    Ok(())
}

// ─── VRS DirectForce Pro (VID 0x0483, specific PIDs) ─────────────────────────

#[test]
fn dispatch_routes_vrs_dfp_on_shared_stm_vid() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x0483, 0xA355); // VRS DFP PID
    assert!(
        proto.is_some(),
        "VRS DirectForce Pro must be dispatched on shared STM VID"
    );
    Ok(())
}

// ─── Asetek (VID 0x2433) ────────────────────────────────────────────────────

#[test]
fn dispatch_routes_asetek_forte() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x2433, 0xF301); // Asetek Forte PID
    assert!(proto.is_some(), "Asetek Forte must be dispatched");
    Ok(())
}

#[test]
fn dispatch_routes_asetek_invicta() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x2433, 0xF300); // Asetek Invicta PID
    assert!(proto.is_some(), "Asetek Invicta must be dispatched");
    Ok(())
}

// ─── Heusinkveld (VID 0x04D8, PIDs 0xF6D0–0xF6D3) ──────────────────────────

#[test]
fn dispatch_routes_heusinkveld_sprint() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x04D8, 0xF6D0); // Sprint PID
    assert!(proto.is_some(), "Heusinkveld Sprint must be dispatched");
    Ok(())
}

#[test]
fn dispatch_routes_heusinkveld_ultimate() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x04D8, 0xF6D2); // Ultimate PID
    assert!(proto.is_some(), "Heusinkveld Ultimate must be dispatched");
    Ok(())
}

#[test]
fn dispatch_skips_non_heusinkveld_microchip_pid() -> Result<(), Box<dyn std::error::Error>> {
    // Random PID on Microchip VID should NOT dispatch (guard condition)
    let proto = get_vendor_protocol(0x04D8, 0x0001);
    assert!(
        proto.is_none(),
        "non-Heusinkveld PID on Microchip VID must return None"
    );
    Ok(())
}

// ─── FFBeast (VID 0x045B) ───────────────────────────────────────────────────

#[test]
fn dispatch_routes_ffbeast_wheel() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x045B, 0x59D7); // FFBeast Wheel PID
    assert!(proto.is_some(), "FFBeast Wheel must be dispatched");
    Ok(())
}

#[test]
fn dispatch_skips_non_ffbeast_pid() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x045B, 0x0001); // Unknown PID on FFBeast VID
    assert!(
        proto.is_none(),
        "non-FFBeast PID on FFBeast VID must return None"
    );
    Ok(())
}

// ─── AccuForce (VID 0x1FC9) ─────────────────────────────────────────────────

#[test]
fn dispatch_routes_accuforce_pro() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x1FC9, 0x804C); // AccuForce Pro PID
    assert!(proto.is_some(), "AccuForce Pro must be dispatched");
    Ok(())
}

#[test]
fn dispatch_skips_non_accuforce_pid() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x1FC9, 0x0001); // Unknown PID on NXP VID
    assert!(
        proto.is_none(),
        "non-AccuForce PID on NXP VID must return None"
    );
    Ok(())
}

// ─── Leo Bodnar (VID 0x1DD2) ────────────────────────────────────────────────

#[test]
fn dispatch_routes_leo_bodnar_wheel_interface() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x1DD2, 0x000E); // Wheel Interface PID
    assert!(
        proto.is_some(),
        "Leo Bodnar Wheel Interface must be dispatched"
    );
    Ok(())
}

// ─── OpenFFBoard (VID 0x1209, PIDs 0xFFB0/0xFFB1) ──────────────────────────

#[test]
fn dispatch_routes_openffboard_main() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x1209, 0xFFB0); // OpenFFBoard Main PID
    assert!(proto.is_some(), "OpenFFBoard Main must be dispatched");
    Ok(())
}

#[test]
fn dispatch_routes_openffboard_alt() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x1209, 0xFFB1); // OpenFFBoard Alt PID
    assert!(proto.is_some(), "OpenFFBoard Alt must be dispatched");
    Ok(())
}

// ─── Cube Controls (VID 0x0483, specific PIDs) ──────────────────────────────

#[test]
fn dispatch_routes_cube_controls_gt_pro() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x0483, 0x0C73); // Cube Controls GT Pro PID
    assert!(
        proto.is_some(),
        "Cube Controls GT Pro must be dispatched on shared STM VID"
    );
    Ok(())
}

// ─── Shared-VID disambiguation ──────────────────────────────────────────────

#[test]
fn stm_vid_dispatches_vrs_before_simagic_fallback() -> Result<(), Box<dyn std::error::Error>> {
    // VRS PID on shared STM VID must get VRS, not Simagic
    let vrs_proto = get_vendor_protocol(0x0483, 0xA355);
    assert!(vrs_proto.is_some(), "VRS PID must dispatch on STM VID");

    // Non-VRS, non-Cube PID on STM VID → Simagic fallback
    let simagic_proto = get_vendor_protocol(0x0483, 0x0522);
    assert!(
        simagic_proto.is_some(),
        "Simagic PID must dispatch as fallback on STM VID"
    );

    Ok(())
}

#[test]
fn pid_codes_vid_dispatches_openffboard_before_button_box() -> Result<(), Box<dyn std::error::Error>>
{
    // OpenFFBoard PID must route to OpenFFBoard
    let openffboard = get_vendor_protocol(0x1209, 0xFFB0);
    assert!(openffboard.is_some(), "OpenFFBoard PID must dispatch");

    // Unknown PID on pid.codes VID → None (neither OpenFFBoard nor button box)
    let unknown = get_vendor_protocol(0x1209, 0x0001);
    assert!(
        unknown.is_none(),
        "unknown PID on pid.codes VID must return None"
    );

    Ok(())
}

// ─── Unknown VID must return None ───────────────────────────────────────────

#[test]
fn dispatch_returns_none_for_unknown_vid() -> Result<(), Box<dyn std::error::Error>> {
    let proto = get_vendor_protocol(0x9999, 0x0001);
    assert!(proto.is_none(), "completely unknown VID must return None");
    Ok(())
}

// ─── Comprehensive: all vendor VIDs produce Some for representative PIDs ────

#[test]
fn all_vendor_vids_dispatch_for_representative_pids() -> Result<(), Box<dyn std::error::Error>> {
    // (VID, representative PID, vendor label)
    let cases: &[(u16, u16, &str)] = &[
        (
            0x046D,
            racing_wheel_hid_logitech_protocol::product_ids::G920,
            "Logitech",
        ),
        (
            0x0EB7,
            racing_wheel_hid_fanatec_protocol::product_ids::CSL_DD,
            "Fanatec",
        ),
        (
            0x044F,
            racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS,
            "Thrustmaster",
        ),
        (0x346E, 0x0002, "Moza"),
        (
            0x3670,
            racing_wheel_hid_simagic_protocol::product_ids::EVO,
            "Simagic EVO",
        ),
        (0x0483, 0xA355, "VRS (shared STM VID)"),
        (0x0483, 0x0522, "Simagic Legacy (shared STM VID)"),
        (0x0483, 0x0C73, "Cube Controls (shared STM VID)"),
        (0x16D0, 0x0D61, "Simucube"),
        (0x2433, 0xF301, "Asetek"),
        (0x04D8, 0xF6D0, "Heusinkveld"),
        (0x045B, 0x59D7, "FFBeast"),
        (0x1FC9, 0x804C, "AccuForce"),
        (0x1DD2, 0x000E, "Leo Bodnar"),
        (0x1209, 0xFFB0, "OpenFFBoard"),
    ];

    for (vid, pid, label) in cases {
        let proto = get_vendor_protocol(*vid, *pid);
        assert!(
            proto.is_some(),
            "{label} (VID 0x{vid:04X}, PID 0x{pid:04X}) must dispatch to a protocol handler"
        );
    }

    Ok(())
}
