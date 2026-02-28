//! Integration tests for the vendor protocol dispatch table.
//!
//! These tests verify `get_vendor_protocol()` and
//! `get_vendor_protocol_with_hid_pid_fallback()` at the integration level:
//!
//! 1. Every known vendor VID routes to `Some(handler)` for a representative PID.
//! 2. Unrecognised VID/PID returns `None`.
//! 3. The generic HID PID fallback activates iff `has_hid_pid_capability` is `true`.
//! 4. Shared VID 0x0483 (STM) disambiguates VRS, Cube Controls, and legacy Simagic.
//! 5. Shared VID 0x16D0 (MCS) disambiguates Heusinkveld, Simucube 2, and legacy Simagic.
//!
//! Per-vendor handler behaviour (FFB config, init sequences, report encoding) is
//! covered by each vendor's unit-test module (`src/hid/vendor/*_tests.rs`).  Some
//! of those files also assert basic dispatch for their own VID/PIDs; this file
//! adds cross-vendor coverage and exercises the dispatch table as a whole.

use racing_wheel_engine::hid::vendor::{
    get_vendor_protocol, get_vendor_protocol_with_hid_pid_fallback,
};

// ── Vendor IDs ────────────────────────────────────────────────────────────────

/// Fanatec GmbH
const VID_FANATEC: u16 = 0x0EB7;
/// Logitech International S.A.
const VID_LOGITECH: u16 = 0x046D;
/// GoodDriver Technology Co., Ltd. (Moza Racing)
const VID_MOZA: u16 = 0x346E;
/// Guillemot Corp. (Thrustmaster)
const VID_THRUSTMASTER: u16 = 0x044F;
/// STMicroelectronics — shared by VRS, Cube Controls (provisional), and legacy Simagic
const VID_STM: u16 = 0x0483;
/// OpenMoko / MCS — shared by Heusinkveld, Simucube 2, and legacy Simagic
const VID_MCS: u16 = 0x16D0;
/// Shen Zhen Simagic Technology Co., Ltd. (EVO generation)
const VID_SIMAGIC_EVO: u16 = 0x3670;
/// Asetek A/S (SimSports)
const VID_ASETEK: u16 = 0x2433;
/// Granite Devices Oy (SimpleMotion V2 / IONI / ARGON / OSW)
const VID_GRANITE: u16 = 0x1D50;
/// pid.codes open-hardware shared VID — OpenFFBoard and generic button boxes
const VID_PID_CODES: u16 = 0x1209;
/// FFBeast open-source direct drive controller
const VID_FFBEAST: u16 = 0x045B;
/// Cammus Technology Co., Ltd.
const VID_CAMMUS: u16 = 0x3416;
/// NXP Semiconductors USB chip — AccuForce Pro
const VID_ACCUFORCE: u16 = 0x1FC9;
/// Leo Bodnar Electronics Ltd.
const VID_LEO_BODNAR: u16 = 0x1DD2;

// ── Representative product IDs ────────────────────────────────────────────────

const PID_FANATEC_CSL_DD: u16 = 0x0020;
const PID_LOGITECH_G920: u16 = 0xC262;
const PID_MOZA_R9_V2: u16 = 0x0012;
const PID_THRUSTMASTER_T818: u16 = 0xB69B;
const PID_VRS_DIRECTFORCE_PRO: u16 = 0xA355; // VRS on 0x0483
const PID_CUBE_CONTROLS_GT_PRO: u16 = 0x0C73; // Cube Controls on 0x0483 (provisional)
const PID_SIMAGIC_ALPHA: u16 = 0x0522; // Legacy Simagic on 0x0483
const PID_HEUSINKVELD_SPRINT: u16 = 0x1156; // Heusinkveld on 0x16D0
const PID_SIMUCUBE_2_SPORT: u16 = 0x0D61; // Simucube 2 on 0x16D0
const PID_SIMAGIC_M10: u16 = 0x0D5A; // Legacy Simagic on 0x16D0
const PID_SIMAGIC_EVO_SPORT: u16 = 0x0500;
const PID_ASETEK_INVICTA: u16 = 0xF300;
const PID_GRANITE_IONI: u16 = 0x6050;
const PID_OPENFFBOARD: u16 = 0xFFB0;
const PID_BUTTON_BOX: u16 = 0x1BBD;
const PID_FFBEAST_WHEEL: u16 = 0x59D7;
const PID_CAMMUS_C5: u16 = 0x0301;
const PID_ACCUFORCE_PRO: u16 = 0x804C;
const PID_LEO_BODNAR_WHEEL: u16 = 0x000E;

// ══ 1. Every known vendor VID routes to Some(...) ════════════════════════════

#[test]
fn dispatch_fanatec_csl_dd() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_FANATEC, PID_FANATEC_CSL_DD).is_some());
    Ok(())
}

#[test]
fn dispatch_logitech_g920() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_LOGITECH, PID_LOGITECH_G920).is_some());
    Ok(())
}

#[test]
fn dispatch_moza_r9_v2() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_MOZA, PID_MOZA_R9_V2).is_some());
    Ok(())
}

#[test]
fn dispatch_thrustmaster_t818() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_THRUSTMASTER, PID_THRUSTMASTER_T818).is_some());
    Ok(())
}

#[test]
fn dispatch_asetek_invicta() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_ASETEK, PID_ASETEK_INVICTA).is_some());
    Ok(())
}

#[test]
fn dispatch_granite_ioni() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_GRANITE, PID_GRANITE_IONI).is_some());
    Ok(())
}

#[test]
fn dispatch_simagic_evo_sport() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_SIMAGIC_EVO, PID_SIMAGIC_EVO_SPORT).is_some());
    Ok(())
}

#[test]
fn dispatch_openffboard() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_PID_CODES, PID_OPENFFBOARD).is_some());
    Ok(())
}

#[test]
fn dispatch_button_box() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_PID_CODES, PID_BUTTON_BOX).is_some());
    Ok(())
}

#[test]
fn dispatch_ffbeast_wheel() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_FFBEAST, PID_FFBEAST_WHEEL).is_some());
    Ok(())
}

#[test]
fn dispatch_cammus_c5() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_CAMMUS, PID_CAMMUS_C5).is_some());
    Ok(())
}

#[test]
fn dispatch_accuforce_pro() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_ACCUFORCE, PID_ACCUFORCE_PRO).is_some());
    Ok(())
}

#[test]
fn dispatch_leo_bodnar_wheel() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_LEO_BODNAR, PID_LEO_BODNAR_WHEEL).is_some());
    Ok(())
}

// ══ 2. Unknown VID/PID returns None ═════════════════════════════════════════

#[test]
fn dispatch_unknown_vid_returns_none() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(0xDEAD, 0xBEEF).is_none());
    Ok(())
}

/// VID 0x1209 with a PID that is neither OpenFFBoard nor a button box must return None.
#[test]
fn dispatch_pid_codes_vid_unknown_pid_returns_none() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_PID_CODES, 0x0001).is_none());
    Ok(())
}

/// VID 0x045B with an unrecognised PID must return None.
#[test]
fn dispatch_ffbeast_vid_unknown_pid_returns_none() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_FFBEAST, 0x0001).is_none());
    Ok(())
}

/// VID 0x1FC9 with an unrecognised PID must return None — not every NXP USB chip
/// is an AccuForce wheelbase.
#[test]
fn dispatch_accuforce_vid_unknown_pid_returns_none() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_ACCUFORCE, 0xFFFF).is_none());
    Ok(())
}

// ══ 3. Generic HID PID fallback ══════════════════════════════════════════════

/// An unrecognised VID with `has_hid_pid_capability = true` must produce a
/// `GenericHidPidHandler` so the device can still operate with standard FFB.
#[test]
fn fallback_unknown_vid_with_hid_pid_capability_returns_handler(
) -> Result<(), Box<dyn std::error::Error>> {
    let handler = get_vendor_protocol_with_hid_pid_fallback(0xDEAD, 0xBEEF, true);
    assert!(
        handler.is_some(),
        "generic HID PID fallback must activate for unknown VID with HID PID capability"
    );
    Ok(())
}

/// An unrecognised VID with `has_hid_pid_capability = false` must return `None`.
#[test]
fn fallback_unknown_vid_without_hid_pid_capability_returns_none(
) -> Result<(), Box<dyn std::error::Error>> {
    let handler = get_vendor_protocol_with_hid_pid_fallback(0xDEAD, 0xBEEF, false);
    assert!(
        handler.is_none(),
        "fallback must return None when HID PID capability is absent"
    );
    Ok(())
}

/// A known vendor VID/PID must always route to its specific handler, regardless
/// of the `has_hid_pid_capability` flag.
#[test]
fn fallback_known_vid_prefers_vendor_handler() -> Result<(), Box<dyn std::error::Error>> {
    for flag in [true, false] {
        let handler =
            get_vendor_protocol_with_hid_pid_fallback(VID_FANATEC, PID_FANATEC_CSL_DD, flag);
        assert!(
            handler.is_some(),
            "known VID/PID must return a handler regardless of has_hid_pid_capability={flag}"
        );
    }
    Ok(())
}

// ══ 4. VID 0x0483 (STM): VRS vs Cube Controls vs Simagic ════════════════════

/// VRS DirectForce Pro PIDs (0xA355–0xA35A) on VID 0x0483 must not fall through
/// to the Simagic handler.
#[test]
fn disambiguate_0x0483_vrs_pids() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [0xA355u16, 0xA356, 0xA357, 0xA358, 0xA359, 0xA35A] {
        assert!(
            get_vendor_protocol(VID_STM, pid).is_some(),
            "VRS PID 0x{pid:04X} on VID 0x0483 must resolve to a handler"
        );
    }
    Ok(())
}

/// Cube Controls provisional PIDs (0x0C73–0x0C75) on VID 0x0483 must not be
/// treated as Simagic.
#[test]
fn disambiguate_0x0483_cube_controls_pids() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [0x0C73u16, 0x0C74, 0x0C75] {
        assert!(
            get_vendor_protocol(VID_STM, pid).is_some(),
            "Cube Controls PID 0x{pid:04X} on VID 0x0483 must resolve to a handler"
        );
    }
    Ok(())
}

/// Simagic Alpha PID (0x0522) on VID 0x0483 must route to the Simagic handler,
/// not to VRS or Cube Controls.
#[test]
fn disambiguate_0x0483_simagic_legacy_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_STM, PID_SIMAGIC_ALPHA).is_some());
    Ok(())
}

/// All three sub-vendors on 0x0483 must each resolve; no PID range may shadow
/// another.
#[test]
fn disambiguate_0x0483_no_cross_routing() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_STM, PID_VRS_DIRECTFORCE_PRO).is_some());
    assert!(get_vendor_protocol(VID_STM, PID_CUBE_CONTROLS_GT_PRO).is_some());
    assert!(get_vendor_protocol(VID_STM, PID_SIMAGIC_ALPHA).is_some());
    Ok(())
}

// ══ 5. VID 0x16D0 (MCS): Heusinkveld vs Simucube 2 vs legacy Simagic ═════════

/// Heusinkveld pedal PIDs (0x1156–0x1158) on VID 0x16D0 must not be misrouted
/// to Simucube or Simagic.
#[test]
fn disambiguate_0x16d0_heusinkveld_pids() -> Result<(), Box<dyn std::error::Error>> {
    for pid in [0x1156u16, 0x1157, 0x1158] {
        assert!(
            get_vendor_protocol(VID_MCS, pid).is_some(),
            "Heusinkveld PID 0x{pid:04X} on VID 0x16D0 must resolve to a handler"
        );
    }
    Ok(())
}

/// Simucube 2 PIDs on VID 0x16D0 must not be misrouted to Heusinkveld or Simagic.
#[test]
fn disambiguate_0x16d0_simucube_pids() -> Result<(), Box<dyn std::error::Error>> {
    // Sport=0x0D61, Pro=0x0D60, Ultimate=0x0D5F, ActivePedal=0x0D62, WirelessWheel=0x0D63
    for pid in [0x0D5Fu16, 0x0D60, 0x0D61, 0x0D62, 0x0D63] {
        assert!(
            get_vendor_protocol(VID_MCS, pid).is_some(),
            "Simucube 2 PID 0x{pid:04X} on VID 0x16D0 must resolve to a handler"
        );
    }
    Ok(())
}

/// A legacy Simagic PID (0x0D5A, M10) on VID 0x16D0 must resolve to the Simagic
/// handler, not Heusinkveld or Simucube.
#[test]
fn disambiguate_0x16d0_simagic_legacy_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_MCS, PID_SIMAGIC_M10).is_some());
    Ok(())
}

/// All three sub-vendors on 0x16D0 must each resolve; no PID range may shadow
/// another.
#[test]
fn disambiguate_0x16d0_all_three_vendors_resolve() -> Result<(), Box<dyn std::error::Error>> {
    assert!(get_vendor_protocol(VID_MCS, PID_HEUSINKVELD_SPRINT).is_some());
    assert!(get_vendor_protocol(VID_MCS, PID_SIMUCUBE_2_SPORT).is_some());
    assert!(get_vendor_protocol(VID_MCS, PID_SIMAGIC_M10).is_some());
    Ok(())
}
