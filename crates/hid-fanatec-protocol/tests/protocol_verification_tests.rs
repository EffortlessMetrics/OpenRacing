//! Protocol verification tests for the Fanatec HID protocol implementation.
//!
//! These tests cross-reference our constants, encoding, and report formats
//! against the community Linux kernel driver `gotzl/hid-fanatecff`
//! (https://github.com/gotzl/hid-fanatecff) and other public sources.
//!
//! ## Sources cited
//!
//! | # | Source | What it confirms |
//! |---|--------|------------------|
//! | 1 | `gotzl/hid-fanatecff` `hid-ftec.h` | VID, all PIDs, rim IDs, quirk flags |
//! | 2 | `gotzl/hid-fanatecff` `hid-ftec.c` device table | PID↔quirk mapping, max range per device |
//! | 3 | `gotzl/hid-fanatecff` `hid-ftecff.c` | FFB slot protocol, TRANSLATE_FORCE macro, |
//! |   |                                      | stop-all cmd, range sequence, fix_values, |
//! |   |                                      | LED/display/rumble wire format, 7-segment |
//! | 4 | `gotzl/hid-fanatecff` README         | Known-device list, PID↔product-name map |
//! | 5 | `wine-mirror/wine` `winebus.sys`     | PID 0x1839 = ClubSport Pedals v1/v2 |
//! | 6 | `linux-hardware.org`                 | VID 0x0EB7 = "Endor" hardware probes |
//! | 7 | `libsdl-org/SDL` `SDL_joystick.c`    | VID 0x0EB7 = Fanatec wheel vendor |
//! | 8 | `JacKeTUs/simracing-hwdb`            | Shifter 0x1A92, Handbrake 0x1A93 |

use racing_wheel_hid_fanatec_protocol::{
    CONSTANT_FORCE_REPORT_LEN, FANATEC_VENDOR_ID, FanatecConstantForceEncoder, FanatecModel,
    FanatecPedalModel, FanatecRimId, LED_REPORT_LEN, MAX_ROTATION_DEGREES, MIN_ROTATION_DEGREES,
    build_display_report, build_kernel_range_sequence, build_led_report,
    build_rotation_range_report, build_rumble_report, build_set_gain_report, build_stop_all_report,
    is_pedal_product, is_wheelbase_product, led_commands, product_ids, rim_ids,
};

// ════════════════════════════════════════════════════════════════════════════
// § 1. VID / PID verification against documented values
// ════════════════════════════════════════════════════════════════════════════

/// VID 0x0EB7 = Endor AG (Fanatec).
/// Source [1]: `hid-ftec.h` → `#define FANATEC_VENDOR_ID 0x0eb7`
/// Source [6]: `linux-hardware.org` → VID 0x0EB7 = "Endor"
/// Source [7]: `libsdl-org/SDL` → VID 0x0EB7 recognized as Fanatec
#[test]
fn vid_matches_community_driver() {
    assert_eq!(FANATEC_VENDOR_ID, 0x0EB7);
}

/// All driver-confirmed wheelbase PIDs.
/// Source [1]: `hid-ftec.h` defines; Source [2]: `hid-ftec.c` device table.
#[test]
fn verified_wheelbase_pids_match_driver() -> Result<(), Box<dyn std::error::Error>> {
    // hid-ftec.h: CLUBSPORT_V2_WHEELBASE_DEVICE_ID 0x0001
    assert_eq!(product_ids::CLUBSPORT_V2, 0x0001);
    // hid-ftec.h: CLUBSPORT_V25_WHEELBASE_DEVICE_ID 0x0004
    assert_eq!(product_ids::CLUBSPORT_V2_5, 0x0004);
    // hid-ftec.h: CSL_ELITE_PS4_WHEELBASE_DEVICE_ID 0x0005
    assert_eq!(product_ids::CSL_ELITE_PS4, 0x0005);
    // hid-ftec.h: PODIUM_WHEELBASE_DD1_DEVICE_ID 0x0006
    assert_eq!(product_ids::DD1, 0x0006);
    // hid-ftec.h: PODIUM_WHEELBASE_DD2_DEVICE_ID 0x0007
    assert_eq!(product_ids::DD2, 0x0007);
    // hid-ftec.h: CSR_ELITE_WHEELBASE_DEVICE_ID 0x0011
    assert_eq!(product_ids::CSR_ELITE, 0x0011);
    // hid-ftec.h: CSL_DD_WHEELBASE_DEVICE_ID 0x0020
    assert_eq!(product_ids::CSL_DD, 0x0020);
    // hid-ftec.h: CSL_ELITE_WHEELBASE_DEVICE_ID 0x0E03
    assert_eq!(product_ids::CSL_ELITE, 0x0E03);
    Ok(())
}

/// All driver-confirmed pedal PIDs.
/// Source [1]: `hid-ftec.h` defines; Source [2]: `hid-ftec.c` device table.
#[test]
fn verified_pedal_pids_match_driver() -> Result<(), Box<dyn std::error::Error>> {
    // hid-ftec.h: CLUBSPORT_PEDALS_V3_DEVICE_ID 0x183b
    assert_eq!(product_ids::CLUBSPORT_PEDALS_V3, 0x183B);
    // hid-ftec.h: CSL_ELITE_PEDALS_DEVICE_ID 0x6204
    assert_eq!(product_ids::CSL_ELITE_PEDALS, 0x6204);
    // hid-ftec.h: CSL_LC_PEDALS_DEVICE_ID 0x6205
    assert_eq!(product_ids::CSL_PEDALS_LC, 0x6205);
    // hid-ftec.h: CSL_LC_V2_PEDALS_DEVICE_ID 0x6206
    assert_eq!(product_ids::CSL_PEDALS_V2, 0x6206);
    Ok(())
}

/// ClubSport Pedals V1/V2 PID from Wine HIDRAW whitelist.
/// Source [5]: `wine-mirror/wine` `winebus.sys/main.c` → pid == 0x1839
#[test]
fn clubsport_pedals_v1v2_pid_matches_wine() {
    assert_eq!(product_ids::CLUBSPORT_PEDALS_V1_V2, 0x1839);
}

/// Accessory PIDs from simracing-hwdb.
/// Source [8]: `JacKeTUs/simracing-hwdb` `90-fanatec.hwdb`
#[test]
fn accessory_pids_match_hwdb() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_ids::CLUBSPORT_SHIFTER, 0x1A92);
    assert_eq!(product_ids::CLUBSPORT_HANDBRAKE, 0x1A93);
    Ok(())
}

/// Unverified PIDs (GT DD Pro 0x0024, ClubSport DD+ 0x01E9) must still be
/// present for backwards compatibility. The README confirms these devices
/// enumerate as PID 0x0020 in PC mode.
/// Source [4]: README → "0EB7:0020 FANATEC CSL DD / DD Pro / ClubSport DD"
#[test]
fn unverified_pids_present_for_compat() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_ids::GT_DD_PRO, 0x0024);
    assert_eq!(product_ids::CLUBSPORT_DD, 0x01E9);
    // Both should still be classified as wheelbases
    assert!(is_wheelbase_product(product_ids::GT_DD_PRO));
    assert!(is_wheelbase_product(product_ids::CLUBSPORT_DD));
    Ok(())
}

/// Every verified wheelbase PID must be recognized by `is_wheelbase_product`.
#[test]
fn all_verified_wheelbases_classified() -> Result<(), Box<dyn std::error::Error>> {
    let wheelbases = [
        product_ids::CLUBSPORT_V2,
        product_ids::CLUBSPORT_V2_5,
        product_ids::CSL_ELITE_PS4,
        product_ids::DD1,
        product_ids::DD2,
        product_ids::CSR_ELITE,
        product_ids::CSL_DD,
        product_ids::CSL_ELITE,
    ];
    for pid in wheelbases {
        assert!(
            is_wheelbase_product(pid),
            "PID 0x{pid:04X} must be classified as wheelbase"
        );
    }
    Ok(())
}

/// Every verified pedal PID must be recognized by `is_pedal_product`.
#[test]
fn all_verified_pedals_classified() -> Result<(), Box<dyn std::error::Error>> {
    let pedals = [
        product_ids::CLUBSPORT_PEDALS_V1_V2,
        product_ids::CLUBSPORT_PEDALS_V3,
        product_ids::CSL_ELITE_PEDALS,
        product_ids::CSL_PEDALS_LC,
        product_ids::CSL_PEDALS_V2,
    ];
    for pid in pedals {
        assert!(
            is_pedal_product(pid),
            "PID 0x{pid:04X} must be classified as pedal"
        );
    }
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 2. Rim ID verification
// ════════════════════════════════════════════════════════════════════════════

/// Verified rim IDs from `hid-ftec.h`.
/// Source [1]: `hid-ftec.h` rim ID defines
#[test]
fn verified_rim_ids_match_driver() -> Result<(), Box<dyn std::error::Error>> {
    // CSL_STEERING_WHEEL_P1_V2 0x08
    assert_eq!(rim_ids::CSL_ELITE_P1, 0x08);
    // CSL_ELITE_STEERING_WHEEL_WRC_ID 0x12
    assert_eq!(rim_ids::WRC, 0x12);
    // CSL_ELITE_STEERING_WHEEL_MCLAREN_GT3_V2_ID 0x0b
    assert_eq!(rim_ids::MCLAREN_GT3_V2, 0x0B);
    // CLUBSPORT_STEERING_WHEEL_FORMULA_V2_ID 0x0a
    assert_eq!(rim_ids::FORMULA_V2, 0x0A);
    // PODIUM_STEERING_WHEEL_PORSCHE_911_GT3_R_ID 0x0c
    assert_eq!(rim_ids::PORSCHE_911_GT3_R, 0x0C);
    Ok(())
}

/// All rim IDs must round-trip through `FanatecRimId::from_byte`.
#[test]
fn rim_id_round_trip_all() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u8, FanatecRimId)] = &[
        (rim_ids::BMW_GT2, FanatecRimId::BmwGt2),
        (rim_ids::FORMULA_V2, FanatecRimId::FormulaV2),
        (rim_ids::FORMULA_V2_5, FanatecRimId::FormulaV25),
        (rim_ids::CSL_ELITE_P1, FanatecRimId::CslEliteP1),
        (rim_ids::MCLAREN_GT3_V2, FanatecRimId::McLarenGt3V2),
        (rim_ids::PORSCHE_911_GT3_R, FanatecRimId::Porsche911Gt3R),
        (rim_ids::PORSCHE_918_RSR, FanatecRimId::Porsche918Rsr),
        (rim_ids::CLUBSPORT_RS, FanatecRimId::ClubSportRs),
        (rim_ids::WRC, FanatecRimId::Wrc),
        (rim_ids::PODIUM_HUB, FanatecRimId::PodiumHub),
    ];
    for &(byte, ref expected) in cases {
        let actual = FanatecRimId::from_byte(byte);
        assert_eq!(
            &actual, expected,
            "Rim byte 0x{byte:02X} decoded wrong: got {actual:?}, want {expected:?}"
        );
    }
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 3. Torque encoding at boundaries
// ════════════════════════════════════════════════════════════════════════════

/// Zero torque must produce 0x0000 (signed zero).
/// Source [3]: `TRANSLATE_FORCE(0)` = `(0 + 0x8000)` = unsigned 0x8000 center;
///   our encoder uses signed encoding where 0 → 0x0000.
#[test]
fn torque_encoding_zero() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(8.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(0.0, 0, &mut buf);
    let raw = i16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(raw, 0, "zero torque must encode as i16 zero");
    Ok(())
}

/// Full positive torque → i16::MAX = 0x7FFF.
/// Source [3]: `TRANSLATE_FORCE(0x7FFF)` = `0xFFFF` (unsigned full positive).
///   Signed equivalent: +32767.
#[test]
fn torque_encoding_full_positive() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(20.0); // DD1 max torque
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(20.0, 0, &mut buf);
    let raw = i16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(
        raw,
        i16::MAX,
        "full positive torque must be i16::MAX (0x7FFF)"
    );
    // LE encoding check: 0x7FFF → [0xFF, 0x7F]
    assert_eq!(buf[2], 0xFF);
    assert_eq!(buf[3], 0x7F);
    Ok(())
}

/// Full negative torque → i16::MIN = 0x8000.
/// Source [3]: `TRANSLATE_FORCE(-0x8000)` = `0x0000` (unsigned full negative).
///   Signed equivalent: -32768.
#[test]
fn torque_encoding_full_negative() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(25.0); // DD2 max torque
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(-25.0, 0, &mut buf);
    let raw = i16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(
        raw,
        i16::MIN,
        "full negative torque must be i16::MIN (0x8000)"
    );
    // LE encoding: 0x8000 → [0x00, 0x80]
    assert_eq!(buf[2], 0x00);
    assert_eq!(buf[3], 0x80);
    Ok(())
}

/// Torque beyond max must be clamped to ±max.
#[test]
fn torque_encoding_clamps_to_max() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(8.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    // 100 Nm is way over 8 Nm
    enc.encode(100.0, 0, &mut buf);
    let raw_pos = i16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(
        raw_pos,
        i16::MAX,
        "over-max positive must clamp to i16::MAX"
    );

    enc.encode(-100.0, 0, &mut buf);
    let raw_neg = i16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(
        raw_neg,
        i16::MIN,
        "over-max negative must clamp to i16::MIN"
    );
    Ok(())
}

/// Half torque (~50%) must produce approximately half of i16::MAX.
#[test]
fn torque_encoding_half() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(8.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(4.0, 0, &mut buf);
    let raw = i16::from_le_bytes([buf[2], buf[3]]);
    // 50% of 32767 ≈ 16384
    assert!(
        raw > 16_000 && raw < 16_500,
        "half torque expected ~16384, got {raw}"
    );
    Ok(())
}

/// Zero max_torque_nm must produce zero output regardless of input.
#[test]
fn torque_encoding_zero_max_returns_zero() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(0.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(10.0, 0, &mut buf);
    let raw = i16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(raw, 0, "zero max_torque must always encode as zero");
    Ok(())
}

/// Constant force report structure must match protocol:
/// [report_id=0x01, cmd=0x01, force_lo, force_hi, 0, 0, 0, 0]
/// Source [3]: Slot 0 constant force uses cmd 0x08 in the driver's slot
///   protocol; our abstraction uses cmd byte 0x01 in the report.
#[test]
fn constant_force_report_structure() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(8.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let len = enc.encode(4.0, 0, &mut buf);
    assert_eq!(len, 8, "report must be 8 bytes");
    assert_eq!(buf[0], 0x01, "byte 0 must be report ID 0x01");
    assert_eq!(buf[1], 0x01, "byte 1 must be constant force command 0x01");
    // bytes 4-7 must be zero (reserved)
    assert_eq!(&buf[4..8], &[0u8; 4], "reserved bytes must be zero");
    Ok(())
}

/// encode_zero must clear force bytes to zero.
#[test]
fn encode_zero_produces_zero_force() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(8.0);
    let mut buf = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode_zero(&mut buf);
    assert_eq!(buf[0], 0x01, "report ID");
    assert_eq!(buf[1], 0x01, "command");
    assert_eq!(buf[2], 0x00, "force lo");
    assert_eq!(buf[3], 0x00, "force hi");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 4. Tuning parameter ranges and model classification
// ════════════════════════════════════════════════════════════════════════════

/// Per-model max torque values from Fanatec product specs.
/// Source [1]: `hid-ftec.c` device table confirms quirk flags per model.
///   Torque values are not in the driver — they're from official specs.
#[test]
fn model_max_torque_per_device() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(FanatecModel, f32)] = &[
        (FanatecModel::Dd1, 20.0),
        (FanatecModel::Dd2, 25.0),
        (FanatecModel::CslDd, 8.0),
        (FanatecModel::GtDdPro, 8.0),
        (FanatecModel::ClubSportDd, 12.0),
        (FanatecModel::CslElite, 6.0),
        (FanatecModel::ClubSportV2, 8.0),
        (FanatecModel::ClubSportV25, 8.0),
        (FanatecModel::CsrElite, 5.0),
    ];
    for &(model, expected_nm) in cases {
        let actual = model.max_torque_nm();
        assert!(
            (actual - expected_nm).abs() < 0.01,
            "{model:?}: expected {expected_nm} Nm, got {actual} Nm"
        );
    }
    Ok(())
}

/// Max rotation degrees per model, verified against `ftec_probe` in
/// `hid-ftec.c` which sets `max_range` per product ID.
/// Source [2]: `ftec_probe()` → ClubSport/CSR = 900, CSL Elite = 1090 (auto),
///   DD1/DD2/CSL DD = 2530 (auto). Our constants use the actual limits
///   (1080, 2520) without the auto sentinel.
#[test]
fn model_max_rotation_per_device() -> Result<(), Box<dyn std::error::Error>> {
    // DD bases: 2520° (driver uses 2530 as auto sentinel)
    assert_eq!(FanatecModel::Dd1.max_rotation_degrees(), 2520);
    assert_eq!(FanatecModel::Dd2.max_rotation_degrees(), 2520);
    assert_eq!(FanatecModel::CslDd.max_rotation_degrees(), 2520);
    assert_eq!(FanatecModel::GtDdPro.max_rotation_degrees(), 2520);
    assert_eq!(FanatecModel::ClubSportDd.max_rotation_degrees(), 2520);
    // CSL Elite: 1080° (driver uses 1090 as auto sentinel)
    assert_eq!(FanatecModel::CslElite.max_rotation_degrees(), 1080);
    // Belt-driven: 900°
    assert_eq!(FanatecModel::ClubSportV2.max_rotation_degrees(), 900);
    assert_eq!(FanatecModel::ClubSportV25.max_rotation_degrees(), 900);
    assert_eq!(FanatecModel::CsrElite.max_rotation_degrees(), 900);
    Ok(())
}

/// HIGHRES quirk flag assignment matches the driver device table.
/// Source [2]: `hid-ftec.c` device table — FTEC_HIGHRES on DD1, DD2, CSL DD.
#[test]
fn highres_flag_matches_driver() -> Result<(), Box<dyn std::error::Error>> {
    // DD devices have FTEC_HIGHRES
    assert!(FanatecModel::Dd1.is_highres());
    assert!(FanatecModel::Dd2.is_highres());
    assert!(FanatecModel::CslDd.is_highres());
    assert!(FanatecModel::GtDdPro.is_highres());
    assert!(FanatecModel::ClubSportDd.is_highres());
    // Belt-driven do NOT
    assert!(!FanatecModel::CslElite.is_highres());
    assert!(!FanatecModel::ClubSportV2.is_highres());
    assert!(!FanatecModel::ClubSportV25.is_highres());
    assert!(!FanatecModel::CsrElite.is_highres());
    Ok(())
}

/// CSR Elite skips fix_values; all others apply it.
/// Source [3]: `send_report_request_to_device()` in `hid-ftecff.c`:
///   `if (hdev->product != CSR_ELITE_WHEELBASE_DEVICE_ID) { fix_values(...); }`
#[test]
fn needs_sign_fix_matches_driver() -> Result<(), Box<dyn std::error::Error>> {
    // CSR Elite does NOT need the fix
    assert!(
        !FanatecModel::CsrElite.needs_sign_fix(),
        "CSR Elite must skip fix_values per hid-ftecff.c"
    );
    // All other verified bases DO need it
    assert!(FanatecModel::Dd1.needs_sign_fix());
    assert!(FanatecModel::Dd2.needs_sign_fix());
    assert!(FanatecModel::CslDd.needs_sign_fix());
    assert!(FanatecModel::CslElite.needs_sign_fix());
    assert!(FanatecModel::ClubSportV2.needs_sign_fix());
    assert!(FanatecModel::ClubSportV25.needs_sign_fix());
    Ok(())
}

/// Minimum rotation range is 90° as set in `ftec_probe`.
/// Source [2]: `drv_data->min_range = 90;`
#[test]
fn min_rotation_degrees_is_90() {
    assert_eq!(MIN_ROTATION_DEGREES, 90);
}

/// Maximum protocol-level rotation is 2520° (DD maximum).
/// Source [2]: DD1/DD2/CSL DD set `max_range = 2530` (auto sentinel);
///   the actual limit is 2520.
#[test]
fn max_rotation_degrees_is_2520() {
    assert_eq!(MAX_ROTATION_DEGREES, 2520);
}

/// Pedal axis count per model.
#[test]
fn pedal_axis_counts() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(FanatecPedalModel::ClubSportV3.axis_count(), 3);
    assert_eq!(FanatecPedalModel::CslPedalsLc.axis_count(), 3);
    assert_eq!(FanatecPedalModel::CslPedalsV2.axis_count(), 3);
    assert_eq!(FanatecPedalModel::ClubSportV1V2.axis_count(), 2);
    assert_eq!(FanatecPedalModel::CslElitePedals.axis_count(), 2);
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 5. Known-good byte sequences from community sources
// ════════════════════════════════════════════════════════════════════════════

/// Stop-all-effects command: `[0xf3, 0, 0, 0, 0, 0, 0]`.
/// Source [3]: `ftecff_stop_effects()` in `hid-ftecff.c`
#[test]
fn stop_all_byte_sequence() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_stop_all_report();
    // Our report is 8 bytes with report ID prepended.
    // The stop command byte is 0x0F in our abstraction; the driver sends
    // 0xf3 as byte[0] of a 7-byte payload. Verify structure is consistent.
    assert_eq!(report[0], 0x01, "report ID must be 0x01");
    assert_eq!(report[1], 0x0F, "stop-all command byte");
    assert_eq!(&report[2..8], &[0u8; 6], "remaining bytes must be zero");
    Ok(())
}

/// Steering range sequence (3 steps) per `ftec_set_range()` in `hid-ftecff.c`.
/// Source [3]: Three reports sent in order:
///   1. `[0xf5, 0, 0, 0, 0, 0, 0]` — reset
///   2. `[0xf8, 0x09, 0x01, 0x06, 0x01, 0, 0]` — prepare
///   3. `[0xf8, 0x81, range_lo, range_hi, 0, 0, 0]` — set range
#[test]
fn kernel_range_sequence_900_degrees() -> Result<(), Box<dyn std::error::Error>> {
    let seq = build_kernel_range_sequence(900);
    // Step 1: reset
    assert_eq!(seq[0], [0xF5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    // Step 2: prepare
    assert_eq!(seq[1], [0xF8, 0x09, 0x01, 0x06, 0x01, 0x00, 0x00]);
    // Step 3: 900 = 0x0384 → lo=0x84, hi=0x03
    assert_eq!(seq[2], [0xF8, 0x81, 0x84, 0x03, 0x00, 0x00, 0x00]);
    Ok(())
}

/// Range sequence at DD maximum (2520°).
/// Source [3]: `ftec_set_range()` byte layout
#[test]
fn kernel_range_sequence_2520_degrees() -> Result<(), Box<dyn std::error::Error>> {
    let seq = build_kernel_range_sequence(2520);
    // 2520 = 0x09D8 → lo=0xD8, hi=0x09
    assert_eq!(seq[2], [0xF8, 0x81, 0xD8, 0x09, 0x00, 0x00, 0x00]);
    Ok(())
}

/// Range sequence at minimum (90°).
/// Source [2]: `drv_data->min_range = 90` in `ftec_probe()`
#[test]
fn kernel_range_sequence_min_90() -> Result<(), Box<dyn std::error::Error>> {
    let seq = build_kernel_range_sequence(90);
    // 90 = 0x005A → lo=0x5A, hi=0x00
    assert_eq!(seq[2], [0xF8, 0x81, 0x5A, 0x00, 0x00, 0x00, 0x00]);
    Ok(())
}

/// Range clamping: below min → 90°, above max → 2520°.
#[test]
fn kernel_range_sequence_clamping() -> Result<(), Box<dyn std::error::Error>> {
    let below = build_kernel_range_sequence(10);
    let below_range = u16::from_le_bytes([below[2][2], below[2][3]]);
    assert_eq!(below_range, 90, "below min must clamp to 90");

    let above = build_kernel_range_sequence(5000);
    let above_range = u16::from_le_bytes([above[2][2], above[2][3]]);
    assert_eq!(above_range, 2520, "above max must clamp to 2520");
    Ok(())
}

/// Rotation range report structure.
#[test]
fn rotation_range_report_360() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_rotation_range_report(360);
    assert_eq!(report[0], 0x01, "report ID");
    assert_eq!(report[1], 0x12, "SET_ROTATION_RANGE command");
    let range = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(range, 360);
    assert_eq!(&report[4..8], &[0u8; 4]);
    Ok(())
}

/// Set gain report: byte 1 = 0x10, byte 2 = gain clamped to [0, 100].
#[test]
fn set_gain_report_format() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_gain_report(75);
    assert_eq!(report[0], 0x01, "report ID");
    assert_eq!(report[1], 0x10, "SET_GAIN command");
    assert_eq!(report[2], 75, "gain value");

    let clamped = build_set_gain_report(200);
    assert_eq!(clamped[2], 100, "gain must clamp to 100");

    let zero = build_set_gain_report(0);
    assert_eq!(zero[2], 0, "gain 0 must pass through");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 6. LED / display / rumble report format
// ════════════════════════════════════════════════════════════════════════════

/// LED report format: [0x08, 0x80, bitmask_lo, bitmask_hi, brightness, 0, 0, 0]
/// Source [3]: `ftec_set_leds()` in `hid-ftecff.c` sends wheel LEDs as:
///   `[0xf8, 0x09, 0x08, leds_hi, leds_lo, 0, 0]`
///   Note: driver reverses bit order for LED bar. Our abstraction uses
///   report ID 0x08 and command 0x80 with standard bit order.
#[test]
fn led_report_format() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(LED_REPORT_LEN, 8, "LED report must be 8 bytes");
    let report = build_led_report(0b1010_1010_0101_0101, 200);
    assert_eq!(report[0], 0x08, "report ID = LED_DISPLAY");
    assert_eq!(
        report[1],
        led_commands::REV_LIGHTS,
        "command = REV_LIGHTS (0x80)"
    );
    assert_eq!(report[1], 0x80);
    // bitmask 0xAA55 → lo=0x55, hi=0xAA
    assert_eq!(report[2], 0x55, "bitmask low byte");
    assert_eq!(report[3], 0xAA, "bitmask high byte");
    assert_eq!(report[4], 200, "brightness");
    assert_eq!(&report[5..8], &[0u8; 3], "reserved bytes");
    Ok(())
}

/// LED report with all LEDs off.
#[test]
fn led_report_all_off() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_led_report(0, 0);
    assert_eq!(report[2], 0x00);
    assert_eq!(report[3], 0x00);
    assert_eq!(report[4], 0x00);
    Ok(())
}

/// LED report with all LEDs on (9 LEDs = 0x01FF).
/// Source [3]: `#define LEDS 9` in `hid-ftec.h`
#[test]
fn led_report_all_9_leds_on() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_led_report(0x01FF, 255);
    assert_eq!(report[2], 0xFF, "bitmask lo: all 8 lower bits set");
    assert_eq!(report[3], 0x01, "bitmask hi: bit 8 set (9th LED)");
    assert_eq!(report[4], 255, "max brightness");
    Ok(())
}

/// Display report format: [0x08, 0x81, mode, d0, d1, d2, brightness, 0]
/// Source [3]: `ftec_set_display()` in `hid-ftecff.c` sends display as:
///   `[0xf8, 0x09, 0x01, 0x02, seg1, seg2, seg3]` with 7-segment encoding.
///   Our abstraction uses report ID 0x08 and command 0x81 with raw digit bytes.
#[test]
fn display_report_format() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_display_report(0x00, [b'1', b'2', b'3'], 128);
    assert_eq!(report[0], 0x08, "report ID = LED_DISPLAY");
    assert_eq!(report[1], led_commands::DISPLAY, "command = DISPLAY (0x81)");
    assert_eq!(report[1], 0x81);
    assert_eq!(report[2], 0x00, "mode byte");
    assert_eq!(report[3], b'1', "digit 0");
    assert_eq!(report[4], b'2', "digit 1");
    assert_eq!(report[5], b'3', "digit 2");
    assert_eq!(report[6], 128, "brightness");
    assert_eq!(report[7], 0, "reserved");
    Ok(())
}

/// Display report auto mode.
#[test]
fn display_report_auto_mode() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_display_report(0x01, [0x00, 0x00, 0x00], 0);
    assert_eq!(report[2], 0x01, "auto mode");
    assert_eq!(report[6], 0, "brightness off");
    Ok(())
}

/// Rumble report format: [0x08, 0x82, left, right, duration_10ms, 0, 0, 0]
/// Source [3]: `ftec_set_rumble()` in `hid-ftec.c` sends rumble as:
///   `[0xf8, 0x09, 0x01, 0x03, val_hi, val_mid, val_lo]` for wheelbase,
///   `[0xf8, 0x09, 0x01, 0x04, ...]` for pedals.
///   Our abstraction uses report ID 0x08 and command 0x82.
#[test]
fn rumble_report_format() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_rumble_report(180, 90, 50);
    assert_eq!(report[0], 0x08, "report ID = LED_DISPLAY");
    assert_eq!(report[1], led_commands::RUMBLE, "command = RUMBLE (0x82)");
    assert_eq!(report[1], 0x82);
    assert_eq!(report[2], 180, "left motor intensity");
    assert_eq!(report[3], 90, "right motor intensity");
    assert_eq!(report[4], 50, "duration in 10ms units");
    assert_eq!(&report[5..8], &[0u8; 3], "reserved bytes");
    Ok(())
}

/// Rumble stop command: all zeros.
#[test]
fn rumble_report_stop() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_rumble_report(0, 0, 0);
    assert_eq!(report[2], 0);
    assert_eq!(report[3], 0);
    assert_eq!(report[4], 0);
    Ok(())
}

/// Rumble max intensity.
#[test]
fn rumble_report_max_intensity() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_rumble_report(255, 255, 255);
    assert_eq!(report[2], 255, "max left intensity");
    assert_eq!(report[3], 255, "max right intensity");
    assert_eq!(report[4], 255, "max duration ~2.55s");
    Ok(())
}

/// LED command constants must match our abstraction layer.
#[test]
fn led_command_constants() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(led_commands::REV_LIGHTS, 0x80);
    assert_eq!(led_commands::DISPLAY, 0x81);
    assert_eq!(led_commands::RUMBLE, 0x82);
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 7. Model classification from product ID
// ════════════════════════════════════════════════════════════════════════════

/// Each PID must classify to the correct model variant.
/// Source [2]: `hid-ftec.c` device table maps PID → quirk flags per device.
#[test]
fn model_from_product_id_all_verified() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u16, FanatecModel)] = &[
        (0x0001, FanatecModel::ClubSportV2),
        (0x0004, FanatecModel::ClubSportV25),
        (0x0005, FanatecModel::CslElite), // CSL Elite PS4
        (0x0E03, FanatecModel::CslElite), // CSL Elite PC
        (0x0006, FanatecModel::Dd1),
        (0x0007, FanatecModel::Dd2),
        (0x0011, FanatecModel::CsrElite),
        (0x0020, FanatecModel::CslDd),
        (0x0024, FanatecModel::GtDdPro),     // unverified PID
        (0x01E9, FanatecModel::ClubSportDd), // unverified PID
    ];
    for &(pid, ref expected) in cases {
        let actual = FanatecModel::from_product_id(pid);
        assert_eq!(
            &actual, expected,
            "PID 0x{pid:04X} must classify as {expected:?}, got {actual:?}"
        );
    }
    Ok(())
}

/// Unknown PIDs must classify as Unknown with safe defaults.
#[test]
fn unknown_pid_classifies_safely() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(0xDEAD);
    assert_eq!(model, FanatecModel::Unknown);
    assert!(
        (model.max_torque_nm() - 5.0).abs() < 0.01,
        "Unknown must default to safe 5 Nm"
    );
    assert_eq!(model.max_rotation_degrees(), 900);
    assert!(!model.supports_1000hz());
    assert!(!model.is_highres());
    Ok(())
}

/// Pedal models classify correctly.
#[test]
fn pedal_model_from_product_id() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        FanatecPedalModel::from_product_id(0x1839),
        FanatecPedalModel::ClubSportV1V2
    );
    assert_eq!(
        FanatecPedalModel::from_product_id(0x183B),
        FanatecPedalModel::ClubSportV3
    );
    assert_eq!(
        FanatecPedalModel::from_product_id(0x6204),
        FanatecPedalModel::CslElitePedals
    );
    assert_eq!(
        FanatecPedalModel::from_product_id(0x6205),
        FanatecPedalModel::CslPedalsLc
    );
    assert_eq!(
        FanatecPedalModel::from_product_id(0x6206),
        FanatecPedalModel::CslPedalsV2
    );
    assert_eq!(
        FanatecPedalModel::from_product_id(0xFFFF),
        FanatecPedalModel::Unknown
    );
    Ok(())
}

/// Pedals must NOT be classified as wheelbases and vice versa.
#[test]
fn pedal_wheelbase_mutual_exclusion() -> Result<(), Box<dyn std::error::Error>> {
    let pedal_pids = [0x1839, 0x183B, 0x6204, 0x6205, 0x6206];
    for pid in pedal_pids {
        assert!(is_pedal_product(pid), "0x{pid:04X} must be pedal");
        assert!(
            !is_wheelbase_product(pid),
            "0x{pid:04X} must NOT be wheelbase"
        );
    }
    Ok(())
}
