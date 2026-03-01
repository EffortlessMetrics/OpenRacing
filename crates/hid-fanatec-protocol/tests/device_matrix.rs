//! Comprehensive Fanatec device matrix verification.
//!
//! Cross-references every PID, capability flag, and torque rating against the
//! community Linux kernel driver [`gotzl/hid-fanatecff`] and Fanatec's
//! published product specifications.
//!
//! ## Verified sources
//!
//! - **gotzl/hid-fanatecff** `hid-ftec.h` — VID, wheelbase PIDs, pedal PIDs,
//!   rim IDs, and quirk flag definitions (`FTEC_FF`, `FTEC_HIGHRES`,
//!   `FTEC_WHEELBASE_LEDS`, `FTEC_TUNING_MENU`, `FTEC_PEDALS`).
//!   <https://github.com/gotzl/hid-fanatecff/blob/master/hid-ftec.h>
//!
//! - **gotzl/hid-fanatecff** `hid-ftec.c` — `hid_device_id devices[]` table
//!   (quirk assignments per PID) and `ftec_probe` (max_range per device).
//!   <https://github.com/gotzl/hid-fanatecff/blob/master/hid-ftec.c>
//!
//! - **Fanatec product pages** — torque ratings and drive type (DD vs belt).
//!
//! [`gotzl/hid-fanatecff`]: https://github.com/gotzl/hid-fanatecff

use std::collections::HashSet;

use proptest::prelude::*;
use racing_wheel_hid_fanatec_protocol::{
    FANATEC_VENDOR_ID, FanatecModel, FanatecPedalModel, FanatecRimId, is_pedal_product,
    is_wheelbase_product, product_ids, rim_ids,
};

// ── Kernel-driver–verified wheelbase PIDs ───────────────────────────────────
// Source: gotzl/hid-fanatecff hid-ftec.c `hid_device_id devices[]` table.

/// Every wheelbase PID present in the gotzl/hid-fanatecff device table.
///
/// Tuple: (our constant, expected hex value, kernel define name, quirk flags).
/// Quirk flags are bitmasks from hid-ftec.h:
///   FTEC_FF = 0x001, FTEC_PEDALS = 0x002, FTEC_WHEELBASE_LEDS = 0x004,
///   FTEC_HIGHRES = 0x008, FTEC_TUNING_MENU = 0x010.
const KERNEL_WHEELBASES: &[(u16, u16, &str, u16)] = &[
    // hid-ftec.c: { HID_USB_DEVICE(FANATEC_VENDOR_ID, CLUBSPORT_V2_WHEELBASE_DEVICE_ID),
    //               .driver_data = FTEC_FF },
    (
        product_ids::CLUBSPORT_V2,
        0x0001,
        "CLUBSPORT_V2_WHEELBASE_DEVICE_ID",
        0x001,
    ),
    // hid-ftec.c: { ..., CLUBSPORT_V25_WHEELBASE_DEVICE_ID), .driver_data = FTEC_FF },
    (
        product_ids::CLUBSPORT_V2_5,
        0x0004,
        "CLUBSPORT_V25_WHEELBASE_DEVICE_ID",
        0x001,
    ),
    // hid-ftec.c: { ..., CSL_ELITE_PS4_WHEELBASE_DEVICE_ID),
    //               .driver_data = FTEC_FF | FTEC_TUNING_MENU | FTEC_WHEELBASE_LEDS },
    (
        product_ids::CSL_ELITE_PS4,
        0x0005,
        "CSL_ELITE_PS4_WHEELBASE_DEVICE_ID",
        0x001 | 0x010 | 0x004,
    ),
    // hid-ftec.c: { ..., PODIUM_WHEELBASE_DD1_DEVICE_ID),
    //               .driver_data = FTEC_FF | FTEC_TUNING_MENU | FTEC_HIGHRES },
    (
        product_ids::DD1,
        0x0006,
        "PODIUM_WHEELBASE_DD1_DEVICE_ID",
        0x001 | 0x010 | 0x008,
    ),
    // hid-ftec.c: { ..., PODIUM_WHEELBASE_DD2_DEVICE_ID),
    //               .driver_data = FTEC_FF | FTEC_TUNING_MENU | FTEC_HIGHRES },
    (
        product_ids::DD2,
        0x0007,
        "PODIUM_WHEELBASE_DD2_DEVICE_ID",
        0x001 | 0x010 | 0x008,
    ),
    // hid-ftec.c: { ..., CSR_ELITE_WHEELBASE_DEVICE_ID), .driver_data = FTEC_FF },
    (
        product_ids::CSR_ELITE,
        0x0011,
        "CSR_ELITE_WHEELBASE_DEVICE_ID",
        0x001,
    ),
    // hid-ftec.c: { ..., CSL_DD_WHEELBASE_DEVICE_ID),
    //               .driver_data = FTEC_FF | FTEC_TUNING_MENU | FTEC_HIGHRES },
    (
        product_ids::CSL_DD,
        0x0020,
        "CSL_DD_WHEELBASE_DEVICE_ID",
        0x001 | 0x010 | 0x008,
    ),
    // hid-ftec.c: { ..., CSL_ELITE_WHEELBASE_DEVICE_ID),
    //               .driver_data = FTEC_FF | FTEC_TUNING_MENU | FTEC_WHEELBASE_LEDS },
    (
        product_ids::CSL_ELITE,
        0x0E03,
        "CSL_ELITE_WHEELBASE_DEVICE_ID",
        0x001 | 0x010 | 0x004,
    ),
];

/// Kernel-driver–verified pedal PIDs (FTEC_PEDALS quirk).
const KERNEL_PEDALS: &[(u16, u16, &str)] = &[
    // hid-ftec.c: { ..., CLUBSPORT_PEDALS_V3_DEVICE_ID), .driver_data = FTEC_PEDALS },
    (
        product_ids::CLUBSPORT_PEDALS_V3,
        0x183B,
        "CLUBSPORT_PEDALS_V3_DEVICE_ID",
    ),
    // hid-ftec.c: { ..., CSL_ELITE_PEDALS_DEVICE_ID), .driver_data = FTEC_PEDALS },
    (
        product_ids::CSL_ELITE_PEDALS,
        0x6204,
        "CSL_ELITE_PEDALS_DEVICE_ID",
    ),
    // hid-ftec.c: { ..., CSL_LC_PEDALS_DEVICE_ID), .driver_data = FTEC_PEDALS },
    (
        product_ids::CSL_PEDALS_LC,
        0x6205,
        "CSL_LC_PEDALS_DEVICE_ID",
    ),
    // hid-ftec.c: { ..., CSL_LC_V2_PEDALS_DEVICE_ID), .driver_data = FTEC_PEDALS },
    (
        product_ids::CSL_PEDALS_V2,
        0x6206,
        "CSL_LC_V2_PEDALS_DEVICE_ID",
    ),
];

/// Kernel-driver–verified rim IDs (hid-ftec.h defines).
const KERNEL_RIMS: &[(u8, u8, &str)] = &[
    (rim_ids::CSL_ELITE_P1, 0x08, "CSL_STEERING_WHEEL_P1_V2"),
    (
        rim_ids::FORMULA_V2,
        0x0A,
        "CLUBSPORT_STEERING_WHEEL_FORMULA_V2_ID",
    ),
    (
        rim_ids::MCLAREN_GT3_V2,
        0x0B,
        "CSL_ELITE_STEERING_WHEEL_MCLAREN_GT3_V2_ID",
    ),
    (
        rim_ids::PORSCHE_911_GT3_R,
        0x0C,
        "PODIUM_STEERING_WHEEL_PORSCHE_911_GT3_R_ID",
    ),
    // Note: WRC (0x12) shares value with CLUBSPORT_STEERING_WHEEL_F1_IS_ID per hid-ftec.h.
    (rim_ids::WRC, 0x12, "CSL_ELITE_STEERING_WHEEL_WRC_ID"),
];

// ── Additional PIDs in our codebase (unverified — from USB captures) ────────

/// PIDs present in our codebase but NOT in the gotzl/hid-fanatecff driver.
/// These must be documented as unverified in ids.rs.
const EXTRA_WHEELBASE_PIDS: &[(u16, &str)] = &[
    (
        product_ids::GT_DD_PRO,
        "GT DD Pro — PlayStation-specific PID, not in kernel driver",
    ),
    (
        product_ids::CLUBSPORT_DD,
        "ClubSport DD+ — newer device, not in kernel driver",
    ),
];

const EXTRA_PEDAL_PIDS: &[(u16, &str)] = &[(
    product_ids::CLUBSPORT_PEDALS_V1_V2,
    "ClubSport Pedals V1/V2 — older device, not in kernel driver",
)];

// ── 1. All kernel-driver PIDs are present and correct ───────────────────────

#[test]
fn kernel_wheelbase_pids_match() -> Result<(), Box<dyn std::error::Error>> {
    for &(our_const, expected_hex, define_name, _quirks) in KERNEL_WHEELBASES {
        assert_eq!(
            our_const, expected_hex,
            "PID mismatch for kernel define {define_name}: \
             our value {our_const:#06x} != expected {expected_hex:#06x}"
        );
    }
    Ok(())
}

#[test]
fn kernel_pedal_pids_match() -> Result<(), Box<dyn std::error::Error>> {
    for &(our_const, expected_hex, define_name) in KERNEL_PEDALS {
        assert_eq!(
            our_const, expected_hex,
            "PID mismatch for kernel define {define_name}: \
             our value {our_const:#06x} != expected {expected_hex:#06x}"
        );
    }
    Ok(())
}

#[test]
fn kernel_rim_ids_match() -> Result<(), Box<dyn std::error::Error>> {
    for &(our_const, expected_hex, define_name) in KERNEL_RIMS {
        assert_eq!(
            our_const, expected_hex,
            "Rim ID mismatch for kernel define {define_name}: \
             our value {our_const:#04x} != expected {expected_hex:#04x}"
        );
    }
    Ok(())
}

// ── 2. No duplicate PIDs ────────────────────────────────────────────────────

#[test]
fn no_duplicate_wheelbase_pids() -> Result<(), Box<dyn std::error::Error>> {
    let all_wheelbase_pids: Vec<u16> = KERNEL_WHEELBASES
        .iter()
        .map(|&(pid, _, _, _)| pid)
        .chain(EXTRA_WHEELBASE_PIDS.iter().map(|&(pid, _)| pid))
        .collect();
    let mut seen = HashSet::new();
    for pid in &all_wheelbase_pids {
        assert!(seen.insert(pid), "Duplicate wheelbase PID: {pid:#06x}");
    }
    Ok(())
}

#[test]
fn no_duplicate_pedal_pids() -> Result<(), Box<dyn std::error::Error>> {
    let all_pedal_pids: Vec<u16> = KERNEL_PEDALS
        .iter()
        .map(|&(pid, _, _)| pid)
        .chain(EXTRA_PEDAL_PIDS.iter().map(|&(pid, _)| pid))
        .collect();
    let mut seen = HashSet::new();
    for pid in &all_pedal_pids {
        assert!(seen.insert(pid), "Duplicate pedal PID: {pid:#06x}");
    }
    Ok(())
}

#[test]
fn no_overlap_between_wheelbase_and_pedal_pids() -> Result<(), Box<dyn std::error::Error>> {
    let wheelbase_pids: HashSet<u16> = KERNEL_WHEELBASES
        .iter()
        .map(|&(pid, _, _, _)| pid)
        .chain(EXTRA_WHEELBASE_PIDS.iter().map(|&(pid, _)| pid))
        .collect();
    let pedal_pids: HashSet<u16> = KERNEL_PEDALS
        .iter()
        .map(|&(pid, _, _)| pid)
        .chain(EXTRA_PEDAL_PIDS.iter().map(|&(pid, _)| pid))
        .collect();
    let overlap: Vec<_> = wheelbase_pids.intersection(&pedal_pids).collect();
    assert!(
        overlap.is_empty(),
        "PIDs appear in both wheelbase and pedal sets: {overlap:?}"
    );
    Ok(())
}

#[test]
fn no_duplicate_rim_ids() -> Result<(), Box<dyn std::error::Error>> {
    let all_rim_ids: &[u8] = &[
        rim_ids::BMW_GT2,
        rim_ids::FORMULA_V2,
        rim_ids::FORMULA_V2_5,
        rim_ids::CSL_ELITE_P1,
        rim_ids::MCLAREN_GT3_V2,
        rim_ids::PORSCHE_911_GT3_R,
        rim_ids::PORSCHE_918_RSR,
        rim_ids::CLUBSPORT_RS,
        rim_ids::WRC,
        rim_ids::PODIUM_HUB,
    ];
    let mut seen = HashSet::new();
    for &id in all_rim_ids {
        assert!(seen.insert(id), "Duplicate rim ID: {id:#04x}");
    }
    Ok(())
}

// ── 3. Every kernel-driver PID resolves to is_wheelbase/is_pedal correctly ──

#[test]
fn kernel_wheelbases_are_wheelbase_products() -> Result<(), Box<dyn std::error::Error>> {
    for &(pid, _, define_name, _) in KERNEL_WHEELBASES {
        assert!(
            is_wheelbase_product(pid),
            "Kernel wheelbase {define_name} (PID {pid:#06x}) must be recognised by is_wheelbase_product"
        );
        assert!(
            !is_pedal_product(pid),
            "Kernel wheelbase {define_name} (PID {pid:#06x}) must NOT be recognised as a pedal"
        );
    }
    Ok(())
}

#[test]
fn extra_wheelbases_are_wheelbase_products() -> Result<(), Box<dyn std::error::Error>> {
    for &(pid, desc) in EXTRA_WHEELBASE_PIDS {
        assert!(
            is_wheelbase_product(pid),
            "Extra wheelbase '{desc}' (PID {pid:#06x}) must be recognised by is_wheelbase_product"
        );
    }
    Ok(())
}

#[test]
fn kernel_pedals_are_pedal_products() -> Result<(), Box<dyn std::error::Error>> {
    for &(pid, _, define_name) in KERNEL_PEDALS {
        assert!(
            is_pedal_product(pid),
            "Kernel pedal {define_name} (PID {pid:#06x}) must be recognised by is_pedal_product"
        );
        assert!(
            !is_wheelbase_product(pid),
            "Kernel pedal {define_name} (PID {pid:#06x}) must NOT be recognised as a wheelbase"
        );
    }
    Ok(())
}

#[test]
fn extra_pedals_are_pedal_products() -> Result<(), Box<dyn std::error::Error>> {
    for &(pid, desc) in EXTRA_PEDAL_PIDS {
        assert!(
            is_pedal_product(pid),
            "Extra pedal '{desc}' (PID {pid:#06x}) must be recognised by is_pedal_product"
        );
    }
    Ok(())
}

// ── 4. Kernel-driver–verified capability flags ──────────────────────────────

/// FTEC_HIGHRES quirk flag value from hid-ftec.h.
const FTEC_HIGHRES: u16 = 0x008;

#[test]
fn highres_matches_kernel_quirks() -> Result<(), Box<dyn std::error::Error>> {
    for &(pid, _, define_name, quirks) in KERNEL_WHEELBASES {
        let model = FanatecModel::from_product_id(pid);
        let kernel_is_highres = (quirks & FTEC_HIGHRES) != 0;
        assert_eq!(
            model.is_highres(),
            kernel_is_highres,
            "is_highres() mismatch for {define_name} (PID {pid:#06x}): \
             our model {model:?} says {}, kernel quirks say {}",
            model.is_highres(),
            kernel_is_highres
        );
    }
    Ok(())
}

/// Max steering range per device from hid-ftec.c:ftec_probe.
///
/// Source (hid-ftec.c):
/// ```c
/// drv_data->max_range = 1090; // default (CSL Elite) → 1080° real
/// if (product == V2 || product == V2.5 || product == CSR_ELITE)
///     drv_data->max_range = 900;
/// else if (product == DD1 || product == DD2 || product == CSL_DD)
///     drv_data->max_range = 2530; // → 2520° real
/// ```
#[test]
fn max_rotation_matches_kernel_ranges() -> Result<(), Box<dyn std::error::Error>> {
    // DD bases: 2520° (kernel uses 2530 as "auto" sentinel)
    let dd_pids = [product_ids::DD1, product_ids::DD2, product_ids::CSL_DD];
    for pid in dd_pids {
        let model = FanatecModel::from_product_id(pid);
        assert_eq!(
            model.max_rotation_degrees(),
            2520,
            "DD base PID {pid:#06x} should have 2520° max rotation"
        );
    }

    // Belt-driven 900° bases
    let belt_900_pids = [
        product_ids::CLUBSPORT_V2,
        product_ids::CLUBSPORT_V2_5,
        product_ids::CSR_ELITE,
    ];
    for pid in belt_900_pids {
        let model = FanatecModel::from_product_id(pid);
        assert_eq!(
            model.max_rotation_degrees(),
            900,
            "Belt base PID {pid:#06x} should have 900° max rotation"
        );
    }

    // CSL Elite: 1080° (kernel default max_range = 1090, real = 1080°)
    for pid in [product_ids::CSL_ELITE, product_ids::CSL_ELITE_PS4] {
        let model = FanatecModel::from_product_id(pid);
        assert_eq!(
            model.max_rotation_degrees(),
            1080,
            "CSL Elite PID {pid:#06x} should have 1080° max rotation"
        );
    }

    Ok(())
}

// ── 5. Torque ratings are reasonable for each device category ───────────────

/// Direct-drive bases must have torque in [5, 30] Nm.
#[test]
fn dd_bases_have_reasonable_torque() -> Result<(), Box<dyn std::error::Error>> {
    let dd_pids = [
        product_ids::DD1,
        product_ids::DD2,
        product_ids::CSL_DD,
        product_ids::GT_DD_PRO,
        product_ids::CLUBSPORT_DD,
    ];
    for pid in dd_pids {
        let model = FanatecModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        assert!(
            (5.0..=30.0).contains(&torque),
            "DD base {model:?} (PID {pid:#06x}) torque {torque} Nm out of range [5, 30]"
        );
    }
    Ok(())
}

/// Belt-driven bases must have torque in [3, 10] Nm.
#[test]
fn belt_bases_have_reasonable_torque() -> Result<(), Box<dyn std::error::Error>> {
    let belt_pids = [
        product_ids::CLUBSPORT_V2,
        product_ids::CLUBSPORT_V2_5,
        product_ids::CSL_ELITE,
        product_ids::CSL_ELITE_PS4,
        product_ids::CSR_ELITE,
    ];
    for pid in belt_pids {
        let model = FanatecModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        assert!(
            (3.0..=10.0).contains(&torque),
            "Belt base {model:?} (PID {pid:#06x}) torque {torque} Nm out of range [3, 10]"
        );
    }
    Ok(())
}

/// Specific torque values from Fanatec product specifications.
#[test]
fn exact_torque_ratings_match_specs() -> Result<(), Box<dyn std::error::Error>> {
    let expected: &[(u16, f32, &str)] = &[
        (product_ids::DD1, 20.0, "Podium DD1"),
        (product_ids::DD2, 25.0, "Podium DD2"),
        (product_ids::CSL_DD, 8.0, "CSL DD"),
        (product_ids::GT_DD_PRO, 8.0, "GT DD Pro"),
        (product_ids::CLUBSPORT_DD, 12.0, "ClubSport DD+"),
        (product_ids::CLUBSPORT_V2, 8.0, "ClubSport V2"),
        (product_ids::CLUBSPORT_V2_5, 8.0, "ClubSport V2.5"),
        (product_ids::CSL_ELITE, 6.0, "CSL Elite"),
        (product_ids::CSL_ELITE_PS4, 6.0, "CSL Elite PS4"),
        (product_ids::CSR_ELITE, 5.0, "CSR Elite"),
    ];
    for &(pid, expected_torque, name) in expected {
        let model = FanatecModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        assert!(
            (torque - expected_torque).abs() < 0.1,
            "{name} (PID {pid:#06x}): expected {expected_torque} Nm, got {torque} Nm"
        );
    }
    Ok(())
}

// ── 6. Known wheelbases resolve to non-Unknown model ────────────────────────

#[test]
fn all_known_wheelbases_resolve_to_named_model() -> Result<(), Box<dyn std::error::Error>> {
    let all_pids: Vec<u16> = KERNEL_WHEELBASES
        .iter()
        .map(|&(pid, _, _, _)| pid)
        .chain(EXTRA_WHEELBASE_PIDS.iter().map(|&(pid, _)| pid))
        .collect();
    for pid in all_pids {
        let model = FanatecModel::from_product_id(pid);
        assert_ne!(
            model,
            FanatecModel::Unknown,
            "PID {pid:#06x} should resolve to a named FanatecModel, got Unknown"
        );
    }
    Ok(())
}

#[test]
fn all_known_pedals_resolve_to_named_model() -> Result<(), Box<dyn std::error::Error>> {
    let all_pids: Vec<u16> = KERNEL_PEDALS
        .iter()
        .map(|&(pid, _, _)| pid)
        .chain(EXTRA_PEDAL_PIDS.iter().map(|&(pid, _)| pid))
        .collect();
    for pid in all_pids {
        let model = FanatecPedalModel::from_product_id(pid);
        assert_ne!(
            model,
            FanatecPedalModel::Unknown,
            "Pedal PID {pid:#06x} should resolve to a named FanatecPedalModel, got Unknown"
        );
    }
    Ok(())
}

// ── 7. Rim ID round-trips ───────────────────────────────────────────────────

#[test]
fn all_kernel_rim_ids_resolve_to_named_variant() -> Result<(), Box<dyn std::error::Error>> {
    for &(id, _, define_name) in KERNEL_RIMS {
        let rim = FanatecRimId::from_byte(id);
        assert_ne!(
            rim,
            FanatecRimId::Unknown,
            "Kernel rim {define_name} (ID {id:#04x}) should resolve to a named FanatecRimId"
        );
    }
    Ok(())
}

// ── 8. DD vs belt capability consistency ────────────────────────────────────

#[test]
fn dd_bases_support_1000hz() -> Result<(), Box<dyn std::error::Error>> {
    let dd_pids = [
        product_ids::DD1,
        product_ids::DD2,
        product_ids::CSL_DD,
        product_ids::GT_DD_PRO,
        product_ids::CLUBSPORT_DD,
    ];
    for pid in dd_pids {
        let model = FanatecModel::from_product_id(pid);
        assert!(
            model.supports_1000hz(),
            "DD base {model:?} (PID {pid:#06x}) must support 1000 Hz"
        );
    }
    Ok(())
}

#[test]
fn belt_bases_do_not_support_1000hz() -> Result<(), Box<dyn std::error::Error>> {
    let belt_pids = [
        product_ids::CLUBSPORT_V2,
        product_ids::CLUBSPORT_V2_5,
        product_ids::CSL_ELITE,
        product_ids::CSL_ELITE_PS4,
        product_ids::CSR_ELITE,
    ];
    for pid in belt_pids {
        let model = FanatecModel::from_product_id(pid);
        assert!(
            !model.supports_1000hz(),
            "Belt base {model:?} (PID {pid:#06x}) must NOT support 1000 Hz"
        );
    }
    Ok(())
}

#[test]
fn dd_bases_have_16384_cpr() -> Result<(), Box<dyn std::error::Error>> {
    let dd_pids = [
        product_ids::DD1,
        product_ids::DD2,
        product_ids::CSL_DD,
        product_ids::GT_DD_PRO,
        product_ids::CLUBSPORT_DD,
    ];
    for pid in dd_pids {
        let model = FanatecModel::from_product_id(pid);
        assert_eq!(
            model.encoder_cpr(),
            16_384,
            "DD base {model:?} (PID {pid:#06x}) should have 16384 CPR"
        );
    }
    Ok(())
}

#[test]
fn belt_bases_have_4096_cpr() -> Result<(), Box<dyn std::error::Error>> {
    let belt_pids = [
        product_ids::CLUBSPORT_V2,
        product_ids::CLUBSPORT_V2_5,
        product_ids::CSL_ELITE,
        product_ids::CSL_ELITE_PS4,
        product_ids::CSR_ELITE,
    ];
    for pid in belt_pids {
        let model = FanatecModel::from_product_id(pid);
        assert_eq!(
            model.encoder_cpr(),
            4_096,
            "Belt base {model:?} (PID {pid:#06x}) should have 4096 CPR"
        );
    }
    Ok(())
}

// ── 9. CSL Elite PS4 variant maps to same model as CSL Elite PC ─────────────

#[test]
fn csl_elite_ps4_and_pc_map_to_same_model() -> Result<(), Box<dyn std::error::Error>> {
    let pc = FanatecModel::from_product_id(product_ids::CSL_ELITE);
    let ps4 = FanatecModel::from_product_id(product_ids::CSL_ELITE_PS4);
    assert_eq!(
        pc, ps4,
        "CSL Elite PC and PS4 should map to the same FanatecModel"
    );
    assert_eq!(pc, FanatecModel::CslElite);
    Ok(())
}

// ── 10. Vendor ID sanity ────────────────────────────────────────────────────

#[test]
fn vendor_id_is_endor_ag() -> Result<(), Box<dyn std::error::Error>> {
    // hid-ftec.h: #define FANATEC_VENDOR_ID 0x0eb7
    assert_eq!(FANATEC_VENDOR_ID, 0x0EB7);
    Ok(())
}

// ── 11. Pedal axis count consistency ────────────────────────────────────────

#[test]
fn pedal_axis_counts_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let all_pids: Vec<u16> = KERNEL_PEDALS
        .iter()
        .map(|&(pid, _, _)| pid)
        .chain(EXTRA_PEDAL_PIDS.iter().map(|&(pid, _)| pid))
        .collect();
    for pid in all_pids {
        let model = FanatecPedalModel::from_product_id(pid);
        let axes = model.axis_count();
        assert!(
            (2..=3).contains(&axes),
            "Pedal {model:?} (PID {pid:#06x}) axis count {axes} out of range [2, 3]"
        );
    }
    Ok(())
}

// ── 12. sign-fix consistency with kernel driver ─────────────────────────────

/// hid-ftecff.c: CSR Elite skips fix_values; all other wheelbases apply it.
#[test]
fn sign_fix_matches_kernel_behavior() -> Result<(), Box<dyn std::error::Error>> {
    // CSR Elite must NOT need sign fix (kernel skips fix_values for this base).
    let csr = FanatecModel::from_product_id(product_ids::CSR_ELITE);
    assert!(
        !csr.needs_sign_fix(),
        "CSR Elite must not need sign fix (kernel skips fix_values)"
    );

    // All other known wheelbases need sign fix.
    let others = [
        product_ids::CLUBSPORT_V2,
        product_ids::CLUBSPORT_V2_5,
        product_ids::CSL_ELITE_PS4,
        product_ids::CSL_ELITE,
        product_ids::DD1,
        product_ids::DD2,
        product_ids::CSL_DD,
    ];
    for pid in others {
        let model = FanatecModel::from_product_id(pid);
        assert!(
            model.needs_sign_fix(),
            "{model:?} (PID {pid:#06x}) must need sign fix (kernel applies fix_values)"
        );
    }
    Ok(())
}

// ── Property-based tests ────────────────────────────────────────────────────

/// All known wheelbase PIDs (kernel-verified + extras).
const ALL_WHEELBASE_PIDS: [u16; 10] = [
    product_ids::CLUBSPORT_V2,
    product_ids::CLUBSPORT_V2_5,
    product_ids::CSL_ELITE_PS4,
    product_ids::DD1,
    product_ids::DD2,
    product_ids::CSR_ELITE,
    product_ids::CSL_DD,
    product_ids::CSL_ELITE,
    product_ids::GT_DD_PRO,
    product_ids::CLUBSPORT_DD,
];

/// All known pedal PIDs (kernel-verified + extras).
const ALL_PEDAL_PIDS: [u16; 5] = [
    product_ids::CLUBSPORT_PEDALS_V3,
    product_ids::CSL_ELITE_PEDALS,
    product_ids::CSL_PEDALS_LC,
    product_ids::CSL_PEDALS_V2,
    product_ids::CLUBSPORT_PEDALS_V1_V2,
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// A random PID that is NOT in the known wheelbase set must NOT resolve
    /// to a known wheelbase model (unless it happens to be a known PID).
    #[test]
    fn prop_unknown_pid_yields_unknown_or_known(pid: u16) {
        let is_known_wb = ALL_WHEELBASE_PIDS.contains(&pid);
        let model = FanatecModel::from_product_id(pid);
        if !is_known_wb {
            // Could still be a known PID if it matches — proptest generates all u16.
            // We only assert non-known PIDs map to Unknown.
            prop_assert!(
                model == FanatecModel::Unknown,
                "PID {:#06x} is not in the known set but resolved to {:?}", pid, model
            );
        }
    }

    /// Every known wheelbase PID must have is_highres consistent with is_highres().
    #[test]
    fn prop_highres_consistent_across_known_pids(idx in 0usize..10usize) {
        let pid = ALL_WHEELBASE_PIDS[idx];
        let model = FanatecModel::from_product_id(pid);
        // If highres, must also be a DD base (supports 1000Hz or is GT DD Pro / ClubSport DD).
        if model.is_highres() {
            prop_assert!(
                model.max_rotation_degrees() >= 2520,
                "{model:?} is highres but max rotation {} < 2520",
                model.max_rotation_degrees()
            );
        }
    }

    /// Torque must be strictly positive for every known wheelbase PID.
    #[test]
    fn prop_known_wheelbase_torque_positive(idx in 0usize..10usize) {
        let pid = ALL_WHEELBASE_PIDS[idx];
        let model = FanatecModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(
            torque > 0.0,
            "{model:?} (PID {pid:#06x}) must have positive torque, got {torque}"
        );
    }

    /// Pedal models must have 2 or 3 axes.
    #[test]
    fn prop_pedal_axis_count_valid(idx in 0usize..5usize) {
        let pid = ALL_PEDAL_PIDS[idx];
        let model = FanatecPedalModel::from_product_id(pid);
        let axes = model.axis_count();
        prop_assert!(
            (2..=3).contains(&axes),
            "Pedal {model:?} axis count {axes} must be 2 or 3"
        );
    }

    /// A wheelbase PID must never be classified as a pedal, and vice versa.
    #[test]
    fn prop_wheelbase_and_pedal_mutually_exclusive(pid: u16) {
        let wb = is_wheelbase_product(pid);
        let ped = is_pedal_product(pid);
        prop_assert!(
            !(wb && ped),
            "PID {pid:#06x} classified as both wheelbase and pedal"
        );
    }

    /// For every known wheelbase, encoder CPR must be either 4096 or 16384.
    #[test]
    fn prop_encoder_cpr_is_standard(idx in 0usize..10usize) {
        let pid = ALL_WHEELBASE_PIDS[idx];
        let model = FanatecModel::from_product_id(pid);
        let cpr = model.encoder_cpr();
        prop_assert!(
            cpr == 4_096 || cpr == 16_384,
            "{model:?} encoder CPR {cpr} must be 4096 or 16384"
        );
    }

    /// Max rotation must be one of the three kernel-defined ranges.
    #[test]
    fn prop_max_rotation_is_standard(idx in 0usize..10usize) {
        let pid = ALL_WHEELBASE_PIDS[idx];
        let model = FanatecModel::from_product_id(pid);
        let rot = model.max_rotation_degrees();
        prop_assert!(
            rot == 900 || rot == 1080 || rot == 2520,
            "{model:?} max rotation {rot}° must be 900, 1080, or 2520"
        );
    }
}
