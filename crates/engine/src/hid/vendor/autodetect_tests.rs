//! Comprehensive tests for device auto-detection, enumeration, and matching logic.
//!
//! Covers VID/PID matching for all supported vendors, device type classification,
//! multi-device scenarios, hot-plug enumeration, device priority ordering, and
//! fallback behaviour when exact match fails.

use super::*;
use crate::device::{VirtualDevice, VirtualHidPort};
use crate::ports::HidPort;
use racing_wheel_schemas::prelude::DeviceId;

// ═══════════════════════════════════════════════════════════════════════════
// 1. VID/PID matching for all supported vendors
// ═══════════════════════════════════════════════════════════════════════════

// ── Fanatec ────────────────────────────────────────────────────────────────

#[test]
fn fanatec_wheelbases_match_vendor_protocol() -> Result<(), Box<dyn std::error::Error>> {
    use fanatec::{FANATEC_VENDOR_ID, is_wheelbase_product, product_ids};

    let wheelbase_pids = [
        product_ids::DD1,
        product_ids::DD2,
        product_ids::CSL_DD,
        product_ids::CSL_ELITE,
        product_ids::CSL_ELITE_PS4,
        product_ids::CLUBSPORT_V2,
        product_ids::CLUBSPORT_V2_5,
        product_ids::CSR_ELITE,
    ];

    for pid in wheelbase_pids {
        assert!(
            is_wheelbase_product(pid),
            "Fanatec PID 0x{pid:04X} should be a wheelbase"
        );
        assert!(
            get_vendor_protocol(FANATEC_VENDOR_ID, pid).is_some(),
            "Fanatec VID+PID 0x{:04X}:0x{pid:04X} should yield a protocol handler",
            FANATEC_VENDOR_ID
        );
    }
    Ok(())
}

#[test]
fn fanatec_pedals_not_classified_as_wheelbase() {
    use fanatec::{is_pedal_product, is_wheelbase_product, product_ids};

    assert!(is_pedal_product(product_ids::CLUBSPORT_PEDALS_V1_V2));
    assert!(!is_wheelbase_product(product_ids::CLUBSPORT_PEDALS_V1_V2));
}

// ── Moza ───────────────────────────────────────────────────────────────────

#[test]
fn moza_wheelbase_identification() -> Result<(), Box<dyn std::error::Error>> {
    use moza::{
        MOZA_VENDOR_ID, MozaDeviceCategory, identify_device, is_wheelbase_product, product_ids,
    };

    let wb_pids = [
        product_ids::R3_V1,
        product_ids::R5_V1,
        product_ids::R9_V1,
        product_ids::R12_V1,
        product_ids::R16_R21_V1,
        product_ids::R3_V2,
        product_ids::R5_V2,
        product_ids::R9_V2,
        product_ids::R12_V2,
        product_ids::R16_R21_V2,
    ];

    for pid in wb_pids {
        assert!(
            is_wheelbase_product(pid),
            "Moza PID 0x{pid:04X} should be a wheelbase"
        );
        let identity = identify_device(pid);
        assert_eq!(
            identity.category,
            MozaDeviceCategory::Wheelbase,
            "Moza PID 0x{pid:04X} should identify as Wheelbase category"
        );
        assert!(
            get_vendor_protocol(MOZA_VENDOR_ID, pid).is_some(),
            "Moza wheelbase 0x{pid:04X} should yield a vendor protocol"
        );
    }
    Ok(())
}

#[test]
fn moza_peripherals_classified_correctly() {
    use moza::{MozaDeviceCategory, identify_device, product_ids};

    let pedals = identify_device(product_ids::SR_P_PEDALS);
    assert_eq!(pedals.category, MozaDeviceCategory::Pedals);

    let hgp = identify_device(product_ids::HGP_SHIFTER);
    assert_eq!(hgp.category, MozaDeviceCategory::Shifter);

    let sgp = identify_device(product_ids::SGP_SHIFTER);
    assert_eq!(sgp.category, MozaDeviceCategory::Shifter);

    let hbp = identify_device(product_ids::HBP_HANDBRAKE);
    assert_eq!(hbp.category, MozaDeviceCategory::Handbrake);
}

// ── Simagic ────────────────────────────────────────────────────────────────

#[test]
fn simagic_legacy_stm_vid_dispatches() {
    use simagic::vendor_ids;

    // Legacy STM VID should yield a Simagic handler (not VRS) for known Simagic PIDs
    let handler = get_vendor_protocol(vendor_ids::SIMAGIC_STM, simagic::product_ids::ALPHA);
    assert!(handler.is_some(), "Simagic Alpha on STM VID should match");
}

#[test]
fn simagic_evo_vid_dispatches() {
    use simagic::{product_ids, vendor_ids};

    for pid in [
        product_ids::EVO_SPORT,
        product_ids::EVO,
        product_ids::EVO_PRO,
    ] {
        let handler = get_vendor_protocol(vendor_ids::SIMAGIC_EVO, pid);
        assert!(
            handler.is_some(),
            "Simagic EVO PID 0x{pid:04X} on EVO VID should match"
        );
    }
}

#[test]
fn simagic_model_classification() {
    use simagic::{SimagicModel, SimagicProtocol, product_ids, vendor_ids};

    // EVO generation
    let evo_sport = SimagicProtocol::new(vendor_ids::SIMAGIC_EVO, product_ids::EVO_SPORT);
    assert_eq!(evo_sport.model(), SimagicModel::EvoSport);

    let evo = SimagicProtocol::new(vendor_ids::SIMAGIC_EVO, product_ids::EVO);
    assert_eq!(evo.model(), SimagicModel::Evo);

    let evo_pro = SimagicProtocol::new(vendor_ids::SIMAGIC_EVO, product_ids::EVO_PRO);
    assert_eq!(evo_pro.model(), SimagicModel::EvoPro);

    // Unknown EVO PID
    let unknown_evo = SimagicProtocol::new(vendor_ids::SIMAGIC_EVO, 0xFFFF);
    assert_eq!(unknown_evo.model(), SimagicModel::EvoUnknown);

    // Legacy
    let alpha = SimagicProtocol::new(vendor_ids::SIMAGIC_STM, product_ids::ALPHA);
    assert_eq!(alpha.model(), SimagicModel::Alpha);
}

// ── Logitech ───────────────────────────────────────────────────────────────

#[test]
fn logitech_wheel_products_match() {
    use logitech::{LOGITECH_VENDOR_ID, is_wheel_product, product_ids};

    let wheel_pids = [
        product_ids::G25,
        product_ids::G27,
        product_ids::G29_PS,
        product_ids::G920,
        product_ids::G923,
        product_ids::G923_PS,
        product_ids::G923_XBOX,
    ];

    for pid in wheel_pids {
        assert!(
            is_wheel_product(pid),
            "Logitech PID 0x{pid:04X} should be a wheel"
        );
        assert!(
            get_vendor_protocol(LOGITECH_VENDOR_ID, pid).is_some(),
            "Logitech wheel 0x{pid:04X} should yield vendor protocol"
        );
    }
}

#[test]
fn logitech_unknown_pid_still_gets_protocol() {
    use logitech::LOGITECH_VENDOR_ID;

    // Logitech VID always dispatches to LogitechProtocol regardless of PID
    let handler = get_vendor_protocol(LOGITECH_VENDOR_ID, 0xBEEF);
    assert!(
        handler.is_some(),
        "Logitech VID should always yield a protocol handler"
    );
}

// ── Thrustmaster ───────────────────────────────────────────────────────────

#[test]
fn thrustmaster_wheel_and_pedal_classification() {
    use thrustmaster::{THRUSTMASTER_VENDOR_ID, is_wheel_product, product_ids};

    let wheel_pids = [
        product_ids::T150,
        product_ids::T300_RS,
        product_ids::T500_RS,
        product_ids::TMX,
        product_ids::T248,
        product_ids::TS_PC_RACER,
        product_ids::TS_XW,
    ];

    for pid in wheel_pids {
        assert!(
            is_wheel_product(pid),
            "Thrustmaster PID 0x{pid:04X} should be a wheel"
        );
        assert!(
            get_vendor_protocol(THRUSTMASTER_VENDOR_ID, pid).is_some(),
            "Thrustmaster wheel 0x{pid:04X} should yield vendor protocol"
        );
    }

    // T_LCM is classified as Unknown by identify_device (not yet explicitly a Pedal),
    // so it is NOT a wheel
    assert!(!is_wheel_product(product_ids::T_LCM));
}

// ── VRS ────────────────────────────────────────────────────────────────────

#[test]
fn vrs_products_dispatch_correctly() {
    use vrs::{VRS_VENDOR_ID, is_vrs_product, is_wheelbase_product, product_ids};

    // DirectForce Pro is both a VRS product and a wheelbase
    assert!(is_vrs_product(product_ids::DIRECTFORCE_PRO));
    assert!(is_wheelbase_product(product_ids::DIRECTFORCE_PRO));

    // R295 is a wheelbase
    assert!(is_wheelbase_product(product_ids::R295));
    assert!(is_vrs_product(product_ids::R295));

    // Pedals are a VRS product but not a wheelbase
    assert!(is_vrs_product(product_ids::PEDALS));
    assert!(!is_wheelbase_product(product_ids::PEDALS));

    // VRS VID is shared with Simagic STM — VRS PID should route to VRS handler
    let handler = get_vendor_protocol(VRS_VENDOR_ID, product_ids::DIRECTFORCE_PRO);
    assert!(handler.is_some(), "VRS DFP should yield a vendor protocol");
}

#[test]
fn vrs_vid_with_non_vrs_pid_falls_to_simagic() {
    use simagic::{product_ids as sim_pids, vendor_ids};

    // STM VID with Simagic Alpha PID should NOT be dispatched as VRS
    let handler = get_vendor_protocol(vendor_ids::SIMAGIC_STM, sim_pids::ALPHA);
    assert!(
        handler.is_some(),
        "STM VID + Alpha PID should match Simagic"
    );

    // VRS products should be resolved when using VRS_VENDOR_ID
    let vrs_handler = get_vendor_protocol(vrs::VRS_VENDOR_ID, vrs::product_ids::DIRECTFORCE_PRO);
    assert!(vrs_handler.is_some());
}

// ── Simucube ───────────────────────────────────────────────────────────────

#[test]
fn simucube_products_match() {
    use simucube::{
        SIMUCUBE_1_PID, SIMUCUBE_2_PRO_PID, SIMUCUBE_2_SPORT_PID, SIMUCUBE_2_ULTIMATE_PID,
        SIMUCUBE_ACTIVE_PEDAL_PID, SIMUCUBE_VENDOR_ID, SIMUCUBE_WIRELESS_WHEEL_PID,
        is_simucube_product,
    };

    let all_pids = [
        SIMUCUBE_1_PID,
        SIMUCUBE_2_SPORT_PID,
        SIMUCUBE_2_PRO_PID,
        SIMUCUBE_2_ULTIMATE_PID,
        SIMUCUBE_ACTIVE_PEDAL_PID,
        SIMUCUBE_WIRELESS_WHEEL_PID,
    ];

    for pid in all_pids {
        assert!(
            is_simucube_product(pid),
            "Simucube PID 0x{pid:04X} should be recognized"
        );
        assert!(
            get_vendor_protocol(SIMUCUBE_VENDOR_ID, pid).is_some(),
            "Simucube VID+PID 0x{pid:04X} should yield vendor protocol"
        );
    }
}

#[test]
fn simucube_vs_simagic_vid_disambiguation() {
    use simucube::{SIMUCUBE_2_PRO_PID, SIMUCUBE_VENDOR_ID};

    // Simucube 2 Pro on OpenMoko VID should dispatch to Simucube
    let handler = get_vendor_protocol(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_PRO_PID);
    assert!(handler.is_some());

    // A non-Simucube PID on OpenMoko VID should dispatch to Simagic
    let handler = get_vendor_protocol(SIMUCUBE_VENDOR_ID, 0x0099);
    assert!(
        handler.is_some(),
        "0x16D0 non-Simucube PID falls to Simagic"
    );
}

// ── Cammus ─────────────────────────────────────────────────────────────────

#[test]
fn cammus_products_match() {
    use cammus::{
        CAMMUS_C5_PID, CAMMUS_C12_PID, CAMMUS_CP5_PEDALS_PID, CAMMUS_LC100_PEDALS_PID,
        CAMMUS_VENDOR_ID, CammusModel, is_cammus_product,
    };

    assert!(is_cammus_product(CAMMUS_C5_PID));
    assert!(is_cammus_product(CAMMUS_C12_PID));
    assert!(is_cammus_product(CAMMUS_CP5_PEDALS_PID));
    assert!(is_cammus_product(CAMMUS_LC100_PEDALS_PID));

    assert_eq!(CammusModel::from_product_id(CAMMUS_C5_PID), CammusModel::C5);
    assert_eq!(
        CammusModel::from_product_id(CAMMUS_C12_PID),
        CammusModel::C12
    );
    assert_eq!(CammusModel::from_product_id(0xFFFF), CammusModel::Unknown);

    assert!(get_vendor_protocol(CAMMUS_VENDOR_ID, CAMMUS_C5_PID).is_some());
    assert!(get_vendor_protocol(CAMMUS_VENDOR_ID, CAMMUS_C12_PID).is_some());
}

// ── Asetek ─────────────────────────────────────────────────────────────────

#[test]
fn asetek_products_match() {
    use asetek::{ASETEK_FORTE_PID, ASETEK_INVICTA_PID, ASETEK_LAPRIMA_PID, ASETEK_VENDOR_ID};

    for pid in [ASETEK_INVICTA_PID, ASETEK_FORTE_PID, ASETEK_LAPRIMA_PID] {
        assert!(
            get_vendor_protocol(ASETEK_VENDOR_ID, pid).is_some(),
            "Asetek PID 0x{pid:04X} should yield vendor protocol"
        );
    }
}

// ── Heusinkveld ────────────────────────────────────────────────────────────

#[test]
fn heusinkveld_multi_vid_matching() {
    use heusinkveld::{
        HEUSINKVELD_HANDBRAKE_V1_PID, HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
        HEUSINKVELD_HANDBRAKE_V2_PID, HEUSINKVELD_LEGACY_SPRINT_PID, HEUSINKVELD_LEGACY_VENDOR_ID,
        HEUSINKVELD_SHIFTER_PID, HEUSINKVELD_SHIFTER_VENDOR_ID, HEUSINKVELD_SPRINT_PID,
        HEUSINKVELD_ULTIMATE_PID, HEUSINKVELD_VENDOR_ID, is_heusinkveld_product,
    };

    // Current firmware VID
    assert!(is_heusinkveld_product(HEUSINKVELD_SPRINT_PID));
    assert!(is_heusinkveld_product(HEUSINKVELD_ULTIMATE_PID));
    assert!(is_heusinkveld_product(HEUSINKVELD_HANDBRAKE_V2_PID));

    assert!(get_vendor_protocol(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID).is_some());

    // Legacy Microchip VID
    assert!(is_heusinkveld_product(HEUSINKVELD_LEGACY_SPRINT_PID));
    assert!(
        get_vendor_protocol(HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_LEGACY_SPRINT_PID).is_some()
    );

    // Handbrake V1 (Silicon Labs VID)
    assert!(is_heusinkveld_product(HEUSINKVELD_HANDBRAKE_V1_PID));
    assert!(
        get_vendor_protocol(
            HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
            HEUSINKVELD_HANDBRAKE_V1_PID
        )
        .is_some()
    );

    // Shifter (VID 0xA020)
    assert!(is_heusinkveld_product(HEUSINKVELD_SHIFTER_PID));
    assert!(get_vendor_protocol(HEUSINKVELD_SHIFTER_VENDOR_ID, HEUSINKVELD_SHIFTER_PID).is_some());
}

#[test]
fn heusinkveld_model_from_info_classification() {
    use heusinkveld::{
        HEUSINKVELD_HANDBRAKE_V1_PID, HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
        HEUSINKVELD_HANDBRAKE_V2_PID, HEUSINKVELD_LEGACY_SPRINT_PID, HEUSINKVELD_LEGACY_VENDOR_ID,
        HEUSINKVELD_SHIFTER_PID, HEUSINKVELD_SHIFTER_VENDOR_ID, HEUSINKVELD_SPRINT_PID,
        HEUSINKVELD_VENDOR_ID, HeusinkveldModel, heusinkveld_model_from_info,
    };

    let sprint = heusinkveld_model_from_info(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID);
    assert_eq!(sprint, HeusinkveldModel::Sprint);

    let legacy_sprint =
        heusinkveld_model_from_info(HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_LEGACY_SPRINT_PID);
    assert_eq!(legacy_sprint, HeusinkveldModel::Sprint);

    let hb_v1 = heusinkveld_model_from_info(
        HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
        HEUSINKVELD_HANDBRAKE_V1_PID,
    );
    assert_eq!(hb_v1, HeusinkveldModel::HandbrakeV1);

    let hb_v2 = heusinkveld_model_from_info(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_HANDBRAKE_V2_PID);
    assert_eq!(hb_v2, HeusinkveldModel::HandbrakeV2);

    let shifter =
        heusinkveld_model_from_info(HEUSINKVELD_SHIFTER_VENDOR_ID, HEUSINKVELD_SHIFTER_PID);
    assert_eq!(shifter, HeusinkveldModel::SequentialShifter);
}

// ── OpenFFBoard ────────────────────────────────────────────────────────────

#[test]
fn openffboard_product_matching() {
    use openffboard::{OPENFFBOARD_PRODUCT_ID, OPENFFBOARD_VENDOR_ID, is_openffboard_product};

    assert!(is_openffboard_product(OPENFFBOARD_PRODUCT_ID));
    assert!(!is_openffboard_product(0x0000));

    // VID 0x1209 with OpenFFBoard PID should resolve
    assert!(get_vendor_protocol(OPENFFBOARD_VENDOR_ID, OPENFFBOARD_PRODUCT_ID).is_some());
}

// ── FFBeast ────────────────────────────────────────────────────────────────

#[test]
fn ffbeast_product_matching() {
    use ffbeast::{FFBEAST_PRODUCT_ID_WHEEL, FFBEAST_VENDOR_ID, is_ffbeast_product};

    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_WHEEL));
    assert!(!is_ffbeast_product(0x0000));

    assert!(get_vendor_protocol(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_WHEEL).is_some());
}

// ── PXN ────────────────────────────────────────────────────────────────────

#[test]
fn pxn_product_matching() {
    use pxn::{
        PRODUCT_GT987, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE, PRODUCT_V12_LITE_2,
        PXN_VENDOR_ID, PxnModel, is_pxn_product,
    };

    for pid in [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ] {
        assert!(
            is_pxn_product(pid),
            "PXN PID 0x{pid:04X} should be recognized"
        );
        assert!(
            get_vendor_protocol(PXN_VENDOR_ID, pid).is_some(),
            "PXN PID 0x{pid:04X} should yield vendor protocol"
        );
    }

    assert_eq!(PxnModel::from_product_id(PRODUCT_V10), PxnModel::V10);
    assert_eq!(PxnModel::from_product_id(PRODUCT_V12), PxnModel::V12);
    assert_eq!(
        PxnModel::from_product_id(PRODUCT_V12_LITE),
        PxnModel::V12Lite
    );
    assert_eq!(
        PxnModel::from_product_id(PRODUCT_V12_LITE_2),
        PxnModel::V12LiteSe
    );
    assert_eq!(PxnModel::from_product_id(PRODUCT_GT987), PxnModel::Gt987);
    assert_eq!(PxnModel::from_product_id(0xFFFF), PxnModel::Unknown);
}

// ── Leo Bodnar ─────────────────────────────────────────────────────────────

#[test]
fn leo_bodnar_ffb_vs_input_only() {
    use leo_bodnar::{
        LEO_BODNAR_PID_BBI32, LEO_BODNAR_PID_FFB_JOYSTICK, LEO_BODNAR_PID_JOYSTICK,
        LEO_BODNAR_PID_SLIM, LEO_BODNAR_PID_WHEEL, LEO_BODNAR_VENDOR_ID, is_leo_bodnar_ffb_product,
    };

    // FFB-capable
    assert!(is_leo_bodnar_ffb_product(LEO_BODNAR_PID_WHEEL));
    assert!(is_leo_bodnar_ffb_product(LEO_BODNAR_PID_FFB_JOYSTICK));

    // Input-only (not FFB)
    assert!(!is_leo_bodnar_ffb_product(LEO_BODNAR_PID_BBI32));
    assert!(!is_leo_bodnar_ffb_product(LEO_BODNAR_PID_JOYSTICK));
    assert!(!is_leo_bodnar_ffb_product(LEO_BODNAR_PID_SLIM));

    // Leo Bodnar VID always dispatches
    assert!(get_vendor_protocol(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_WHEEL).is_some());
}

// ── Button Box ─────────────────────────────────────────────────────────────

#[test]
fn button_box_product_matching() {
    use button_box::{PRODUCT_ID_BUTTON_BOX, VENDOR_ID_GENERIC, is_button_box_product};

    assert!(is_button_box_product(PRODUCT_ID_BUTTON_BOX));
    assert!(!is_button_box_product(0x0000));

    // Button box on pid.codes VID should resolve (not conflict with OpenFFBoard)
    assert!(get_vendor_protocol(VENDOR_ID_GENERIC, PRODUCT_ID_BUTTON_BOX).is_some());
}

#[test]
fn pid_codes_vid_disambiguates_openffboard_vs_button_box() {
    use button_box::{PRODUCT_ID_BUTTON_BOX, VENDOR_ID_GENERIC};
    use openffboard::OPENFFBOARD_PRODUCT_ID;

    // Both share VID 0x1209 — verify correct dispatch
    assert!(get_vendor_protocol(VENDOR_ID_GENERIC, OPENFFBOARD_PRODUCT_ID).is_some());
    assert!(get_vendor_protocol(VENDOR_ID_GENERIC, PRODUCT_ID_BUTTON_BOX).is_some());

    // Unknown PID on pid.codes VID should return None
    assert!(get_vendor_protocol(VENDOR_ID_GENERIC, 0x0001).is_none());
}

// ── AccuForce ──────────────────────────────────────────────────────────────

#[test]
fn accuforce_product_matching() {
    use accuforce::is_accuforce_product;

    assert!(is_accuforce_product(0x804C));
    assert!(!is_accuforce_product(0x0000));

    assert!(get_vendor_protocol(0x1FC9, 0x804C).is_some());
    assert!(get_vendor_protocol(0x1FC9, 0x0000).is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Device type classification (wheelbase, pedal, shifter, handbrake, button box)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn device_type_classification_across_vendors() {
    use fanatec::{is_pedal_product as fan_pedal, is_wheelbase_product as fan_wb};
    use moza::{MozaDeviceCategory, identify_device, product_ids as moza_pids};
    use thrustmaster::is_wheel_product as tm_wheel;
    use vrs::{is_wheelbase_product as vrs_wb, product_ids as vrs_pids};

    // Fanatec: wheelbase vs pedal
    assert!(fan_wb(fanatec::product_ids::DD1));
    assert!(!fan_pedal(fanatec::product_ids::DD1));
    assert!(fan_pedal(fanatec::product_ids::CLUBSPORT_PEDALS_V1_V2));
    assert!(!fan_wb(fanatec::product_ids::CLUBSPORT_PEDALS_V1_V2));

    // Moza: all categories
    assert_eq!(
        identify_device(moza_pids::R9_V2).category,
        MozaDeviceCategory::Wheelbase
    );
    assert_eq!(
        identify_device(moza_pids::SR_P_PEDALS).category,
        MozaDeviceCategory::Pedals
    );
    assert_eq!(
        identify_device(moza_pids::HGP_SHIFTER).category,
        MozaDeviceCategory::Shifter
    );
    assert_eq!(
        identify_device(moza_pids::HBP_HANDBRAKE).category,
        MozaDeviceCategory::Handbrake
    );

    // Thrustmaster: wheel classification
    assert!(tm_wheel(thrustmaster::product_ids::T300_RS));
    assert!(!tm_wheel(thrustmaster::product_ids::T_LCM));

    // VRS: wheelbase vs pedal
    assert!(vrs_wb(vrs_pids::DIRECTFORCE_PRO));
    assert!(!vrs_wb(vrs_pids::PEDALS));
}

#[test]
fn heusinkveld_device_type_classification() {
    use heusinkveld::{
        HEUSINKVELD_HANDBRAKE_V1_PID, HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
        HEUSINKVELD_HANDBRAKE_V2_PID, HEUSINKVELD_SHIFTER_PID, HEUSINKVELD_SHIFTER_VENDOR_ID,
        HEUSINKVELD_SPRINT_PID, HEUSINKVELD_VENDOR_ID, HeusinkveldModel,
        heusinkveld_model_from_info,
    };

    // Pedals
    let sprint = heusinkveld_model_from_info(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID);
    assert_eq!(sprint, HeusinkveldModel::Sprint);
    assert!(sprint.pedal_count() > 0);

    // Handbrake
    let hb_v2 = heusinkveld_model_from_info(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_HANDBRAKE_V2_PID);
    assert_eq!(hb_v2, HeusinkveldModel::HandbrakeV2);

    let hb_v1 = heusinkveld_model_from_info(
        HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
        HEUSINKVELD_HANDBRAKE_V1_PID,
    );
    assert_eq!(hb_v1, HeusinkveldModel::HandbrakeV1);

    // Shifter
    let shifter =
        heusinkveld_model_from_info(HEUSINKVELD_SHIFTER_VENDOR_ID, HEUSINKVELD_SHIFTER_PID);
    assert_eq!(shifter, HeusinkveldModel::SequentialShifter);
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Multi-device scenario handling
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn multi_device_same_vendor() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    let dev1_id: DeviceId = "moza-r9-unit-1".parse()?;
    let dev1 = VirtualDevice::new(dev1_id.clone(), "Moza R9 #1".to_string());
    port.add_device(dev1)?;

    let dev2_id: DeviceId = "moza-r9-unit-2".parse()?;
    let dev2 = VirtualDevice::new(dev2_id.clone(), "Moza R9 #2".to_string());
    port.add_device(dev2)?;

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 2);

    // Both devices should be independently addressable
    let opened1 = port.open_device(&dev1_id).await?;
    let opened2 = port.open_device(&dev2_id).await?;
    assert_eq!(opened1.device_info().id, dev1_id);
    assert_eq!(opened2.device_info().id, dev2_id);
    assert!(opened1.is_connected());
    assert!(opened2.is_connected());

    Ok(())
}

#[tokio::test]
async fn multi_device_mixed_vendors() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    let wb_id: DeviceId = "fanatec-dd1".parse()?;
    let wb = VirtualDevice::new(wb_id.clone(), "Fanatec DD1".to_string());
    port.add_device(wb)?;

    let pedal_id: DeviceId = "heusinkveld-sprint".parse()?;
    let pedal = VirtualDevice::new(pedal_id.clone(), "Heusinkveld Sprint".to_string());
    port.add_device(pedal)?;

    let shifter_id: DeviceId = "moza-hgp-shifter".parse()?;
    let shifter = VirtualDevice::new(shifter_id.clone(), "Moza HGP Shifter".to_string());
    port.add_device(shifter)?;

    let devices = port.list_devices().await?;
    assert_eq!(
        devices.len(),
        3,
        "Three mixed-vendor devices should all be enumerated"
    );

    // Each device should be independently accessible
    let d1 = port.open_device(&wb_id).await?;
    let d2 = port.open_device(&pedal_id).await?;
    let d3 = port.open_device(&shifter_id).await?;
    assert!(d1.is_connected());
    assert!(d2.is_connected());
    assert!(d3.is_connected());

    Ok(())
}

#[tokio::test]
async fn enumerate_empty_port_returns_empty_list() -> Result<(), Box<dyn std::error::Error>> {
    let port = VirtualHidPort::new();
    let devices = port.list_devices().await?;
    assert!(devices.is_empty());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Hot-plug enumeration updates
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn hotplug_add_device_updates_enumeration() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    // Start empty
    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 0);

    // Hot-plug: add a device
    let dev_id: DeviceId = "simucube-2-pro".parse()?;
    let dev = VirtualDevice::new(dev_id.clone(), "Simucube 2 Pro".to_string());
    port.add_device(dev)?;

    // Enumeration should now show the device
    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id, dev_id);

    Ok(())
}

#[tokio::test]
async fn hotplug_remove_device_updates_enumeration() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    let dev_id: DeviceId = "asetek-forte".parse()?;
    let dev = VirtualDevice::new(dev_id.clone(), "Asetek Forte".to_string());
    port.add_device(dev)?;
    assert_eq!(port.list_devices().await?.len(), 1);

    // Hot-unplug
    port.remove_device(&dev_id)?;
    assert_eq!(port.list_devices().await?.len(), 0);

    Ok(())
}

#[tokio::test]
async fn hotplug_add_remove_add_cycle() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    let dev_id: DeviceId = "logitech-g29".parse()?;

    // Plug in
    let dev = VirtualDevice::new(dev_id.clone(), "Logitech G29".to_string());
    port.add_device(dev)?;
    assert_eq!(port.list_devices().await?.len(), 1);

    // Unplug
    port.remove_device(&dev_id)?;
    assert_eq!(port.list_devices().await?.len(), 0);

    // Re-plug
    let dev2 = VirtualDevice::new(dev_id.clone(), "Logitech G29".to_string());
    port.add_device(dev2)?;
    assert_eq!(port.list_devices().await?.len(), 1);
    assert_eq!(port.list_devices().await?[0].id, dev_id);

    Ok(())
}

#[tokio::test]
async fn hotplug_multiple_devices_sequential() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    let ids: Vec<DeviceId> = vec!["dev-a".parse()?, "dev-b".parse()?, "dev-c".parse()?];

    // Add devices one by one
    for (i, id) in ids.iter().enumerate() {
        let dev = VirtualDevice::new(id.clone(), format!("Device {}", i));
        port.add_device(dev)?;
        assert_eq!(port.list_devices().await?.len(), i + 1);
    }

    // Remove the middle device
    port.remove_device(&ids[1])?;
    let remaining = port.list_devices().await?;
    assert_eq!(remaining.len(), 2);
    assert!(remaining.iter().all(|d| d.id != ids[1]));

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Device priority / preference ordering
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn vendor_protocol_dispatch_priority_shared_vids() {
    // VID 0x0483: VRS products should get VRS handler, others get Simagic
    assert!(get_vendor_protocol(0x0483, 0xA355).is_some()); // VRS DFP
    assert!(get_vendor_protocol(0x0483, 0x0522).is_some()); // Simagic Alpha

    // VID 0x16D0: Simucube products get Simucube, others get Simagic
    assert!(get_vendor_protocol(0x16D0, 0x0D60).is_some()); // Simucube 2 Pro
    assert!(get_vendor_protocol(0x16D0, 0x0D5A).is_some()); // Simucube 1 / Simagic M10

    // VID 0x1209: OpenFFBoard gets its handler, button box gets its handler, unknown returns None
    assert!(get_vendor_protocol(0x1209, 0xFFB0).is_some()); // OpenFFBoard
    assert!(get_vendor_protocol(0x1209, 0x1BBD).is_some()); // Button box
    assert!(get_vendor_protocol(0x1209, 0x0001).is_none()); // Unknown

    // VID 0x045B: FFBeast PID gets handler, unknown returns None
    assert!(get_vendor_protocol(0x045B, 0x59D7).is_some()); // FFBeast wheel
    assert!(get_vendor_protocol(0x045B, 0x0001).is_none()); // Unknown

    // VID 0x1FC9: AccuForce PID gets handler, unknown returns None
    assert!(get_vendor_protocol(0x1FC9, 0x804C).is_some()); // AccuForce Pro
    assert!(get_vendor_protocol(0x1FC9, 0x0001).is_none()); // Unknown
}

#[test]
fn vendor_protocol_covers_all_known_vendor_ids() {
    // Verify each known VID resolves to a handler with at least one known PID
    let vendor_pid_pairs: Vec<(u16, u16)> = vec![
        (0x0EB7, 0x0006), // Fanatec DD1
        (0x346E, 0x0004), // Moza R5 V1
        (0x0483, 0x0522), // Simagic Alpha (STM VID)
        (0x3670, 0x0501), // Simagic EVO
        (0x046D, 0xC262), // Logitech G920
        (0x044F, 0xB66E), // Thrustmaster T300RS
        (0x0483, 0xA355), // VRS DirectForce Pro
        (0x16D0, 0x0D60), // Simucube 2 Pro
        (0x3416, 0x0301), // Cammus C5
        (0x2433, 0xF300), // Asetek Invicta
        (0x30B7, 0x1001), // Heusinkveld Sprint
        (0x1209, 0xFFB0), // OpenFFBoard
        (0x045B, 0x59D7), // FFBeast wheel
        (0x11FF, 0x3245), // PXN V10
        (0x1DD2, 0x000E), // Leo Bodnar wheel
        (0x1209, 0x1BBD), // Button box
        (0x1FC9, 0x804C), // AccuForce Pro
        (0x1D50, 0x0001), // SimpleMotion V2
        (0x16D0, 0x0099), // Simagic on OpenMoko VID
    ];

    for (vid, pid) in &vendor_pid_pairs {
        assert!(
            get_vendor_protocol(*vid, *pid).is_some(),
            "VID 0x{vid:04X} PID 0x{pid:04X} should have a vendor protocol"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Fallback behaviour when exact match fails
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn unknown_vid_returns_none() {
    assert!(get_vendor_protocol(0xDEAD, 0xBEEF).is_none());
    assert!(get_vendor_protocol(0x0000, 0x0000).is_none());
    assert!(get_vendor_protocol(0xFFFF, 0xFFFF).is_none());
}

#[test]
fn hid_pid_fallback_when_vendor_unknown() {
    // An unknown VID/PID with HID PID capability should get the generic handler
    let handler = get_vendor_protocol_with_hid_pid_fallback(0xDEAD, 0xBEEF, true);
    assert!(
        handler.is_some(),
        "Unknown device with HID PID should get generic handler"
    );

    // Same device without HID PID capability should get nothing
    let handler = get_vendor_protocol_with_hid_pid_fallback(0xDEAD, 0xBEEF, false);
    assert!(handler.is_none());
}

#[test]
fn hid_pid_fallback_not_used_when_vendor_matches() {
    // Known Fanatec device should get Fanatec handler even with PID fallback enabled
    let handler = get_vendor_protocol_with_hid_pid_fallback(0x0EB7, 0x0006, true);
    assert!(handler.is_some());

    // Known Moza device should get Moza handler
    let handler = get_vendor_protocol_with_hid_pid_fallback(0x346E, 0x0004, true);
    assert!(handler.is_some());
}

#[test]
fn unknown_pid_on_known_vid_behaviour() {
    // Some VIDs always dispatch (Fanatec, Logitech, Thrustmaster, Cammus, Asetek,
    // Leo Bodnar, Simagic EVO, SimpleMotion)
    assert!(get_vendor_protocol(0x0EB7, 0xFFFF).is_some()); // Fanatec — always
    assert!(get_vendor_protocol(0x046D, 0xFFFF).is_some()); // Logitech — always
    assert!(get_vendor_protocol(0x044F, 0xFFFF).is_some()); // Thrustmaster — always
    assert!(get_vendor_protocol(0x3416, 0xFFFF).is_some()); // Cammus — always
    assert!(get_vendor_protocol(0x2433, 0xFFFF).is_some()); // Asetek — always
    assert!(get_vendor_protocol(0x1DD2, 0xFFFF).is_some()); // Leo Bodnar — always
    assert!(get_vendor_protocol(0x3670, 0xFFFF).is_some()); // Simagic EVO — always

    // Some VIDs return None for unknown PIDs (PXN, AccuForce, FFBeast, pid.codes)
    assert!(get_vendor_protocol(0x11FF, 0xFFFF).is_none()); // PXN — guarded
    assert!(get_vendor_protocol(0x1FC9, 0xFFFF).is_none()); // AccuForce — guarded
    assert!(get_vendor_protocol(0x045B, 0xFFFF).is_none()); // FFBeast — guarded
    assert!(get_vendor_protocol(0x1209, 0x0001).is_none()); // pid.codes — guarded
}

#[test]
fn hid_pid_fallback_covers_unknown_chinese_dd_controllers() {
    // Hypothetical Chinese DD controller with HID PID support
    let handler = get_vendor_protocol_with_hid_pid_fallback(0x2B3C, 0x0001, true);
    assert!(
        handler.is_some(),
        "Unknown DD controller with HID PID should get generic handler"
    );

    // Without HID PID support, no handler
    let handler = get_vendor_protocol_with_hid_pid_fallback(0x2B3C, 0x0001, false);
    assert!(handler.is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// Additional edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn moza_v1_and_v2_firmware_both_recognized() {
    use moza::{is_wheelbase_product, product_ids};

    // V1 and V2 firmware versions of the same physical base should both work
    assert!(is_wheelbase_product(product_ids::R5_V1));
    assert!(is_wheelbase_product(product_ids::R5_V2));
    assert!(is_wheelbase_product(product_ids::R9_V1));
    assert!(is_wheelbase_product(product_ids::R9_V2));
    assert!(is_wheelbase_product(product_ids::R16_R21_V1));
    assert!(is_wheelbase_product(product_ids::R16_R21_V2));
}

#[test]
fn cammus_pedals_recognized_as_products() {
    use cammus::{CAMMUS_CP5_PEDALS_PID, CAMMUS_LC100_PEDALS_PID, is_cammus_product};

    assert!(is_cammus_product(CAMMUS_CP5_PEDALS_PID));
    assert!(is_cammus_product(CAMMUS_LC100_PEDALS_PID));
}

#[test]
fn simucube_active_pedal_and_wireless_wheel_recognized() {
    use simucube::{SIMUCUBE_ACTIVE_PEDAL_PID, SIMUCUBE_WIRELESS_WHEEL_PID, is_simucube_product};

    assert!(is_simucube_product(SIMUCUBE_ACTIVE_PEDAL_PID));
    assert!(is_simucube_product(SIMUCUBE_WIRELESS_WHEEL_PID));
}

#[test]
fn moza_unknown_pid_identified_as_unknown_category() {
    use moza::{MozaDeviceCategory, identify_device};

    let unknown = identify_device(0xFFFF);
    assert_eq!(unknown.category, MozaDeviceCategory::Unknown);
}

#[tokio::test]
async fn open_nonexistent_device_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let port = VirtualHidPort::new();
    let fake_id: DeviceId = "nonexistent-device".parse()?;

    let result = port.open_device(&fake_id).await;
    assert!(result.is_err(), "Opening a nonexistent device should fail");
    Ok(())
}
