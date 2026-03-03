//! Protocol verification tests for the Asetek SimSports HID protocol implementation.
//!
//! These tests cross-reference our constants against the Linux kernel mainline
//! source, community hardware databases, and USB vendor registries.
//!
//! ## Sources cited
//!
//! | # | Source | What it confirms |
//! |---|--------|------------------|
//! | 1 | Linux kernel `hid-ids.h` (mainline) | `USB_VENDOR_ID_ASETEK = 0x2433`, Invicta `0xf300`, Forte `0xf301`, La Prima `0xf303`, Tony Kanaan `0xf306` |
//! | 2 | Linux kernel `hid-universal-pidff.c` | All four wheelbase PIDs in device table (no quirk flags) |
//! | 3 | JacKeTUs/linux-steering-wheels | Invicta/Forte/La Prima/Tony Kanaan — all Gold support |
//! | 4 | the-sz.com / devicehunt.com | VID `0x2433` = "Asetek A/S" / "ASETEK" |
//! | 5 | JacKeTUs/simracing-hwdb `90-asetek.hwdb` | Invicta Pedals `v2433pF100`, Forte Pedals `v2433pF101` |
//! | 6 | moonrail/asetek_wheelbase_cli | La Prima PID `0xF303` in udev rules |
//! | 7 | tolgayilmaz86/MuscleMemoryTrainer | La Prima Pedals `0xF102` in device_presets.py |

use hid_asetek_protocol::{
    AsetekModel, AsetekOutputReport, ASETEK_FORTE_PEDALS_PID, ASETEK_FORTE_PID,
    ASETEK_INVICTA_PEDALS_PID, ASETEK_INVICTA_PID, ASETEK_LAPRIMA_PEDALS_PID,
    ASETEK_LAPRIMA_PID, ASETEK_TONY_KANAAN_PID, ASETEK_VENDOR_ID, MAX_TORQUE_NM,
    PRODUCT_ID_FORTE, PRODUCT_ID_INVICTA, PRODUCT_ID_LAPRIMA, REPORT_SIZE_INPUT,
    REPORT_SIZE_OUTPUT, VENDOR_ID, asetek_model_from_info, is_asetek_device,
};

// ════════════════════════════════════════════════════════════════════════════
// § 1. VID verification against kernel and USB vendor databases
// ════════════════════════════════════════════════════════════════════════════

/// VID `0x2433` = Asetek A/S.
/// Source [1]: Linux kernel `hid-ids.h` → `#define USB_VENDOR_ID_ASETEK 0x2433`
/// Source [4]: the-sz.com → "Asetek A/S", devicehunt.com → "ASETEK"
#[test]
fn vid_matches_kernel_and_vendor_databases() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        ASETEK_VENDOR_ID, 0x2433,
        "Asetek VID must be 0x2433 (confirmed in Linux kernel hid-ids.h)"
    );
    // lib.rs also re-exports VENDOR_ID — must match
    assert_eq!(VENDOR_ID, ASETEK_VENDOR_ID, "VENDOR_ID must equal ASETEK_VENDOR_ID");
    Ok(())
}

/// `is_asetek_device()` must accept the confirmed VID.
#[test]
fn is_asetek_device_accepts_confirmed_vid() -> Result<(), Box<dyn std::error::Error>> {
    assert!(is_asetek_device(ASETEK_VENDOR_ID));
    assert!(!is_asetek_device(0x0000));
    assert!(!is_asetek_device(0x0483)); // STM VID
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 2. Wheelbase PID verification against Linux kernel mainline
// ════════════════════════════════════════════════════════════════════════════

/// Invicta PID `0xF300` — confirmed in Linux kernel mainline.
/// Source [1]: `#define USB_DEVICE_ID_ASETEK_INVICTA 0xf300`
/// Source [2]: `hid-universal-pidff.c` device table
/// Source [3]: linux-steering-wheels → Gold support
#[test]
fn invicta_pid_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(ASETEK_INVICTA_PID, 0xF300, "Invicta PID must be 0xF300");
    assert_eq!(PRODUCT_ID_INVICTA, ASETEK_INVICTA_PID, "PRODUCT_ID_INVICTA must match");
    Ok(())
}

/// Forte PID `0xF301` — confirmed in Linux kernel mainline.
/// Source [1]: `#define USB_DEVICE_ID_ASETEK_FORTE 0xf301`
/// Source [2]: `hid-universal-pidff.c` device table
/// Source [3]: linux-steering-wheels → Gold support
#[test]
fn forte_pid_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(ASETEK_FORTE_PID, 0xF301, "Forte PID must be 0xF301");
    assert_eq!(PRODUCT_ID_FORTE, ASETEK_FORTE_PID, "PRODUCT_ID_FORTE must match");
    Ok(())
}

/// La Prima PID `0xF303` — confirmed in Linux kernel mainline.
/// Source [1]: `#define USB_DEVICE_ID_ASETEK_LA_PRIMA 0xf303`
/// Source [2]: `hid-universal-pidff.c` device table
/// Source [3]: linux-steering-wheels → Gold support
/// Source [6]: moonrail/asetek_wheelbase_cli udev rules → PID `0xF303`
#[test]
fn laprima_pid_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(ASETEK_LAPRIMA_PID, 0xF303, "La Prima PID must be 0xF303");
    assert_eq!(PRODUCT_ID_LAPRIMA, ASETEK_LAPRIMA_PID, "PRODUCT_ID_LAPRIMA must match");
    Ok(())
}

/// Tony Kanaan PID `0xF306` — confirmed in Linux kernel mainline.
/// Source [1]: `#define USB_DEVICE_ID_ASETEK_TONY_KANAAN 0xf306`
/// Source [2]: `hid-universal-pidff.c` device table
/// Source [3]: linux-steering-wheels → Gold support
#[test]
fn tony_kanaan_pid_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        ASETEK_TONY_KANAAN_PID, 0xF306,
        "Tony Kanaan PID must be 0xF306"
    );
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 3. Pedal PID verification
// ════════════════════════════════════════════════════════════════════════════

/// Invicta Pedals PID `0xF100` — confirmed in simracing-hwdb.
/// Source [5]: `v2433pF100` in `90-asetek.hwdb` ("Asetek Invicta Pedals")
#[test]
fn invicta_pedals_pid_matches_hwdb() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        ASETEK_INVICTA_PEDALS_PID, 0xF100,
        "Invicta Pedals PID must be 0xF100"
    );
    Ok(())
}

/// Forte Pedals PID `0xF101` — confirmed in simracing-hwdb.
/// Source [5]: `v2433pF101` in `90-asetek.hwdb` ("Asetek Forte Pedals")
#[test]
fn forte_pedals_pid_matches_hwdb() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        ASETEK_FORTE_PEDALS_PID, 0xF101,
        "Forte Pedals PID must be 0xF101"
    );
    Ok(())
}

/// La Prima Pedals PID `0xF102` — community-sourced.
/// Source [7]: tolgayilmaz86/MuscleMemoryTrainer device_presets.py
/// Follows the `0xF10x` pedal pattern (Invicta=F100, Forte=F101, La Prima=F102).
#[test]
fn laprima_pedals_pid_follows_pattern() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        ASETEK_LAPRIMA_PEDALS_PID, 0xF102,
        "La Prima Pedals PID must be 0xF102"
    );
    // Verify the pattern: each pedal PID increments by 1
    assert_eq!(
        ASETEK_FORTE_PEDALS_PID - ASETEK_INVICTA_PEDALS_PID, 1,
        "Forte - Invicta pedal PID delta must be 1"
    );
    assert_eq!(
        ASETEK_LAPRIMA_PEDALS_PID - ASETEK_FORTE_PEDALS_PID, 1,
        "La Prima - Forte pedal PID delta must be 1"
    );
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 4. Model classification and torque ratings
// ════════════════════════════════════════════════════════════════════════════

/// Invicta: 27 Nm direct drive.
#[test]
fn invicta_torque_27nm() -> Result<(), Box<dyn std::error::Error>> {
    let model = AsetekModel::from_product_id(ASETEK_INVICTA_PID);
    assert_eq!(model, AsetekModel::Invicta);
    assert!(
        (model.max_torque_nm() - 27.0).abs() < f32::EPSILON,
        "Invicta must be 27 Nm"
    );
    Ok(())
}

/// Forte: 18 Nm direct drive.
#[test]
fn forte_torque_18nm() -> Result<(), Box<dyn std::error::Error>> {
    let model = AsetekModel::from_product_id(ASETEK_FORTE_PID);
    assert_eq!(model, AsetekModel::Forte);
    assert!(
        (model.max_torque_nm() - 18.0).abs() < f32::EPSILON,
        "Forte must be 18 Nm"
    );
    Ok(())
}

/// La Prima: 12 Nm direct drive.
#[test]
fn laprima_torque_12nm() -> Result<(), Box<dyn std::error::Error>> {
    let model = AsetekModel::from_product_id(ASETEK_LAPRIMA_PID);
    assert_eq!(model, AsetekModel::LaPrima);
    assert!(
        (model.max_torque_nm() - 12.0).abs() < f32::EPSILON,
        "La Prima must be 12 Nm"
    );
    Ok(())
}

/// Tony Kanaan: 27 Nm (Invicta-based special edition).
#[test]
fn tony_kanaan_torque_27nm() -> Result<(), Box<dyn std::error::Error>> {
    let model = AsetekModel::from_product_id(ASETEK_TONY_KANAAN_PID);
    assert_eq!(model, AsetekModel::TonyKanaan);
    assert!(
        (model.max_torque_nm() - 27.0).abs() < f32::EPSILON,
        "Tony Kanaan must be 27 Nm (Invicta-based)"
    );
    Ok(())
}

/// Pedal models must have 0 Nm torque (input-only devices).
#[test]
fn pedal_models_zero_torque() -> Result<(), Box<dyn std::error::Error>> {
    for &pid in &[
        ASETEK_INVICTA_PEDALS_PID,
        ASETEK_FORTE_PEDALS_PID,
        ASETEK_LAPRIMA_PEDALS_PID,
    ] {
        let model = AsetekModel::from_product_id(pid);
        assert!(
            model.max_torque_nm().abs() < f32::EPSILON,
            "Pedal model for PID 0x{pid:04X} must have 0 Nm torque"
        );
    }
    Ok(())
}

/// MAX_TORQUE_NM = 27.0 (Invicta, the highest-torque model).
#[test]
fn max_torque_constant() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        (MAX_TORQUE_NM - 27.0).abs() < f32::EPSILON,
        "MAX_TORQUE_NM must be 27.0 (Invicta)"
    );
    Ok(())
}

/// `asetek_model_from_info()` must reject wrong VID.
#[test]
fn model_from_info_wrong_vid() -> Result<(), Box<dyn std::error::Error>> {
    let model = asetek_model_from_info(0x0000, ASETEK_FORTE_PID);
    assert_eq!(
        model,
        AsetekModel::Unknown,
        "wrong VID must return Unknown"
    );
    Ok(())
}

/// `asetek_model_from_info()` with correct VID resolves correctly.
#[test]
fn model_from_info_correct_vid() -> Result<(), Box<dyn std::error::Error>> {
    let model = asetek_model_from_info(ASETEK_VENDOR_ID, ASETEK_FORTE_PID);
    assert_eq!(model, AsetekModel::Forte);
    Ok(())
}

/// Unknown PID resolves to `AsetekModel::Unknown`.
#[test]
fn unknown_pid_resolves_to_unknown() -> Result<(), Box<dyn std::error::Error>> {
    let model = AsetekModel::from_product_id(0xFFFF);
    assert_eq!(model, AsetekModel::Unknown);
    Ok(())
}

/// Display names are non-empty and mention "Asetek".
#[test]
fn display_names_are_descriptive() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        AsetekModel::Forte,
        AsetekModel::Invicta,
        AsetekModel::LaPrima,
        AsetekModel::TonyKanaan,
        AsetekModel::InvictaPedals,
        AsetekModel::FortePedals,
        AsetekModel::LaPrimaPedals,
    ];
    for model in &models {
        let name = model.display_name();
        assert!(
            name.contains("Asetek"),
            "{model:?} display name must contain 'Asetek', got: {name}"
        );
    }
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 5. Report format constants
// ════════════════════════════════════════════════════════════════════════════

/// Input and output reports are 32 bytes each.
#[test]
fn report_sizes() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(REPORT_SIZE_INPUT, 32, "input report must be 32 bytes");
    assert_eq!(REPORT_SIZE_OUTPUT, 32, "output report must be 32 bytes");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 6. Output report encoding
// ════════════════════════════════════════════════════════════════════════════

/// Torque encoding uses centi-Newton-metres: 10.5 Nm → 1050 cNm.
#[test]
fn torque_encoding_cnm() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekOutputReport::new(1).with_torque(10.5);
    assert_eq!(
        report.torque_cNm, 1050,
        "10.5 Nm must encode as 1050 cNm"
    );
    Ok(())
}

/// Zero torque in default report.
#[test]
fn default_report_zero_torque() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekOutputReport::default();
    assert_eq!(report.torque_cNm, 0, "default torque must be 0");
    assert_eq!(report.sequence, 0, "default sequence must be 0");
    Ok(())
}

/// Torque clamping: must not exceed ±MAX_TORQUE_NM (27 Nm → 2700 cNm).
#[test]
fn torque_clamping() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekOutputReport::new(0).with_torque(100.0);
    assert_eq!(
        report.torque_cNm, 2700,
        "100 Nm must clamp to 27 Nm = 2700 cNm"
    );
    let report = AsetekOutputReport::new(0).with_torque(-100.0);
    assert_eq!(
        report.torque_cNm, -2700,
        "-100 Nm must clamp to -27 Nm = -2700 cNm"
    );
    Ok(())
}

/// Built output report must be exactly REPORT_SIZE_OUTPUT bytes.
#[test]
fn built_report_size() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekOutputReport::new(42).with_torque(15.0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_eq!(
        data.len(),
        REPORT_SIZE_OUTPUT,
        "built report must be {REPORT_SIZE_OUTPUT} bytes"
    );
    Ok(())
}

/// Sequence number is preserved in the built report (first 2 bytes, LE).
#[test]
fn sequence_preserved_in_built_report() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekOutputReport::new(0x1234);
    let data = report.build().map_err(|e| e.to_string())?;
    let seq = u16::from_le_bytes([data[0], data[1]]);
    assert_eq!(seq, 0x1234, "sequence number must be preserved");
    Ok(())
}
