//! BDD end-to-end tests for the Asetek protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify the pure builder-pattern
//! output report API without real USB hardware.

use hid_asetek_protocol::{
    AsetekError, AsetekInputReport, AsetekModel, AsetekOutputReport, ASETEK_FORTE_PID,
    ASETEK_INVICTA_PID, ASETEK_LAPRIMA_PID, ASETEK_TONY_KANAAN_PID, ASETEK_VENDOR_ID,
    REPORT_SIZE_OUTPUT, VENDOR_ID, asetek_model_from_info, is_asetek_device,
};

// ─── Scenario 1: Invicta torque encoding at model max ─────────────────────────

#[test]
fn scenario_invicta_torque_at_model_max_encodes_correctly() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: Invicta model with 27 Nm max torque
    let model = AsetekModel::from_product_id(ASETEK_INVICTA_PID);
    assert_eq!(model, AsetekModel::Invicta);
    let max_nm = model.max_torque_nm();
    assert!((max_nm - 27.0).abs() < f32::EPSILON);

    // When: output report built at model max torque
    let report = AsetekOutputReport::new(1).with_torque(max_nm);
    let data = report.build()?;

    // Then: torque_cnm encodes as 2700 (27.0 * 100) in little-endian at bytes 2..4
    let torque_cnm = i16::from_le_bytes([data[2], data[3]]);
    assert_eq!(torque_cnm, 2700, "Invicta max 27 Nm → 2700 cNm");

    Ok(())
}

// ─── Scenario 2: Forte torque encoding at model max ───────────────────────────

#[test]
fn scenario_forte_torque_at_model_max_encodes_correctly() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: Forte model with 18 Nm max torque
    let model = AsetekModel::from_product_id(ASETEK_FORTE_PID);
    assert_eq!(model, AsetekModel::Forte);
    let max_nm = model.max_torque_nm();
    assert!((max_nm - 18.0).abs() < f32::EPSILON);

    // When: output report built at Forte's max torque
    let data = AsetekOutputReport::new(2).with_torque(max_nm).build()?;

    // Then: torque_cnm encodes as 1800 (18.0 * 100)
    let torque_cnm = i16::from_le_bytes([data[2], data[3]]);
    assert_eq!(torque_cnm, 1800, "Forte max 18 Nm → 1800 cNm");

    Ok(())
}

// ─── Scenario 3: LaPrima torque encoding at model max ─────────────────────────

#[test]
fn scenario_laprima_torque_at_model_max_encodes_correctly() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: LaPrima model with 12 Nm max torque
    let model = AsetekModel::from_product_id(ASETEK_LAPRIMA_PID);
    assert_eq!(model, AsetekModel::LaPrima);
    let max_nm = model.max_torque_nm();
    assert!((max_nm - 12.0).abs() < f32::EPSILON);

    // When: output report built at LaPrima's max torque
    let data = AsetekOutputReport::new(3).with_torque(max_nm).build()?;

    // Then: torque_cnm encodes as 1200 (12.0 * 100)
    let torque_cnm = i16::from_le_bytes([data[2], data[3]]);
    assert_eq!(torque_cnm, 1200, "LaPrima max 12 Nm → 1200 cNm");

    Ok(())
}

// ─── Scenario 4: zero torque produces all-zero torque bytes ───────────────────

#[test]
fn scenario_zero_torque_produces_zero_output() -> Result<(), Box<dyn std::error::Error>> {
    // Given: default output report (torque = 0)
    let report = AsetekOutputReport::new(0);

    // When: built
    let data = report.build()?;

    // Then: torque bytes are both zero
    let torque_cnm = i16::from_le_bytes([data[2], data[3]]);
    assert_eq!(torque_cnm, 0, "zero torque must encode as 0 cNm");

    // Then: also verify via explicit with_torque(0.0)
    let data2 = AsetekOutputReport::new(0).with_torque(0.0).build()?;
    let torque2 = i16::from_le_bytes([data2[2], data2[3]]);
    assert_eq!(torque2, 0, "explicit 0.0 Nm must also encode as 0 cNm");

    Ok(())
}

// ─── Scenario 5: positive and negative torque sign preservation ───────────────

#[test]
fn scenario_positive_and_negative_torque_sign_preserved() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: a positive torque value
    let pos_data = AsetekOutputReport::new(10).with_torque(5.0).build()?;

    // Then: torque_cnm is positive 500
    let pos_cnm = i16::from_le_bytes([pos_data[2], pos_data[3]]);
    assert_eq!(pos_cnm, 500, "+5.0 Nm → +500 cNm");

    // Given: a negative torque value (opposite direction)
    let neg_data = AsetekOutputReport::new(11).with_torque(-5.0).build()?;

    // Then: torque_cnm is negative -500
    let neg_cnm = i16::from_le_bytes([neg_data[2], neg_data[3]]);
    assert_eq!(neg_cnm, -500, "-5.0 Nm → -500 cNm");

    // Then: signs are opposite
    assert_eq!(pos_cnm, -neg_cnm, "positive and negative must be symmetric");

    Ok(())
}

// ─── Scenario 6: saturation clamps at global MAX_TORQUE_NM ───────────────────

#[test]
fn scenario_saturation_clamps_at_max_torque() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a torque value far exceeding the global max (27 Nm)
    let over_data = AsetekOutputReport::new(20).with_torque(100.0).build()?;

    // Then: clamped to MAX_TORQUE_NM (2700 cNm)
    let over_cnm = i16::from_le_bytes([over_data[2], over_data[3]]);
    assert_eq!(
        over_cnm, 2700,
        "100 Nm must clamp to MAX_TORQUE_NM (27 Nm = 2700 cNm)"
    );

    // Given: negative torque far below the global min (-27 Nm)
    let under_data = AsetekOutputReport::new(21).with_torque(-100.0).build()?;

    // Then: clamped to -MAX_TORQUE_NM (-2700 cNm)
    let under_cnm = i16::from_le_bytes([under_data[2], under_data[3]]);
    assert_eq!(
        under_cnm, -2700,
        "-100 Nm must clamp to -MAX_TORQUE_NM (-2700 cNm)"
    );

    Ok(())
}

// ─── Scenario 7: sequence number encodes correctly across increments ──────────

#[test]
fn scenario_sequence_number_increments_correctly() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a series of sequence numbers
    for seq in [0u16, 1, 255, 256, 1000, u16::MAX] {
        // When: report built with that sequence
        let data = AsetekOutputReport::new(seq).build()?;

        // Then: bytes 0..2 contain the sequence in little-endian
        let decoded_seq = u16::from_le_bytes([data[0], data[1]]);
        assert_eq!(
            decoded_seq, seq,
            "sequence {seq} must round-trip through report bytes"
        );
    }

    Ok(())
}

// ─── Scenario 8: report byte layout matches protocol spec ─────────────────────

#[test]
fn scenario_report_byte_layout_matches_spec() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a report with known field values
    let data = AsetekOutputReport::new(0x1234)
        .with_torque(10.0)
        .with_led(0xAB, 0xCD)
        .build()?;

    // Then: total length is REPORT_SIZE_OUTPUT (32 bytes)
    assert_eq!(
        data.len(),
        REPORT_SIZE_OUTPUT,
        "report must be exactly {REPORT_SIZE_OUTPUT} bytes"
    );

    // Then: bytes 0-1 = sequence 0x1234 little-endian
    assert_eq!(data[0], 0x34, "sequence low byte");
    assert_eq!(data[1], 0x12, "sequence high byte");

    // Then: bytes 2-3 = torque_cnm 1000 (10.0 * 100) little-endian
    let torque = i16::from_le_bytes([data[2], data[3]]);
    assert_eq!(torque, 1000, "10.0 Nm → 1000 cNm");

    // Then: byte 4 = led_mode
    assert_eq!(data[4], 0xAB, "led_mode at offset 4");

    // Then: byte 5 = led_value
    assert_eq!(data[5], 0xCD, "led_value at offset 5");

    // Then: remaining bytes 6..32 are zero-padded
    assert!(
        data[6..].iter().all(|&b| b == 0),
        "bytes 6..32 must be zero-padded"
    );

    Ok(())
}

// ─── Scenario 9: NaN torque is treated as zero ────────────────────────────────

#[test]
fn scenario_nan_torque_treated_as_zero() -> Result<(), Box<dyn std::error::Error>> {
    // Given: NaN input to with_torque
    // (f32::NAN.clamp() returns NAN, NAN * 100.0 = NAN, NAN as i16 = 0 via saturating cast)
    let data = AsetekOutputReport::new(30).with_torque(f32::NAN).build()?;

    // Then: torque_cnm is 0 (Rust saturating float-to-int cast)
    let torque_cnm = i16::from_le_bytes([data[2], data[3]]);
    assert_eq!(torque_cnm, 0, "NaN torque must saturate to 0 cNm");

    Ok(())
}

// ─── Scenario 10: Inf torque clamps to max ────────────────────────────────────

#[test]
fn scenario_inf_torque_clamps_to_max() -> Result<(), Box<dyn std::error::Error>> {
    // Given: +Inf torque (clamp(+Inf, -27, 27) = 27)
    let pos_data = AsetekOutputReport::new(31)
        .with_torque(f32::INFINITY)
        .build()?;
    let pos_cnm = i16::from_le_bytes([pos_data[2], pos_data[3]]);
    assert_eq!(pos_cnm, 2700, "+Inf must clamp to MAX_TORQUE_NM (2700 cNm)");

    // Given: -Inf torque (clamp(-Inf, -27, 27) = -27)
    let neg_data = AsetekOutputReport::new(32)
        .with_torque(f32::NEG_INFINITY)
        .build()?;
    let neg_cnm = i16::from_le_bytes([neg_data[2], neg_data[3]]);
    assert_eq!(
        neg_cnm, -2700,
        "-Inf must clamp to -MAX_TORQUE_NM (-2700 cNm)"
    );

    Ok(())
}

// ─── Scenario 11: input report round-trip parse ───────────────────────────────

#[test]
fn scenario_input_report_round_trip_parse() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a raw 16-byte input buffer with known values
    //   seq=0x0005, angle=90000 (90.0°), speed=1800, torque=1500 (15.0 Nm),
    //   temp=45, status=0x03 (connected+enabled)
    let mut raw = [0u8; 20];
    raw[0..2].copy_from_slice(&5u16.to_le_bytes()); // sequence
    raw[2..6].copy_from_slice(&90000i32.to_le_bytes()); // wheel_angle
    raw[6..8].copy_from_slice(&1800i16.to_le_bytes()); // wheel_speed
    raw[8..10].copy_from_slice(&1500i16.to_le_bytes()); // torque
    raw[10] = 45; // temperature
    raw[11] = 0x03; // status

    // When: parsed
    let report = AsetekInputReport::parse(&raw)?;

    // Then: fields decode correctly
    assert_eq!(report.sequence, 5);
    assert!((report.wheel_angle_degrees() - 90.0).abs() < 0.01);
    assert_eq!(report.torque, 1500);
    assert!((report.applied_torque_nm() - 15.0).abs() < 0.01);
    assert_eq!(report.temperature, 45);
    assert!(report.is_connected());
    assert!(report.is_enabled());

    Ok(())
}

// ─── Scenario 12: input report rejects too-short buffer ───────────────────────

#[test]
fn scenario_input_report_rejects_short_buffer() {
    // Given: a buffer shorter than 16 bytes
    let short = [0u8; 10];

    // When: parsed
    let result = AsetekInputReport::parse(&short);

    // Then: returns InvalidReportSize error
    let Err(err) = result else {
        panic!("expected InvalidReportSize error for short buffer");
    };
    assert!(
        matches!(err, AsetekError::InvalidReportSize { expected: 16, actual: 10 }),
        "expected InvalidReportSize, got: {err:?}"
    );
}

// ─── Scenario 13: model identification from vendor/product IDs ────────────────

#[test]
fn scenario_model_identification_from_vendor_product_ids() {
    // Given: all known Asetek product IDs
    // Then: vendor ID constants match
    assert_eq!(VENDOR_ID, 0x2433);
    assert_eq!(ASETEK_VENDOR_ID, 0x2433);
    assert!(is_asetek_device(VENDOR_ID));

    // Then: each PID maps to the correct model
    assert_eq!(
        asetek_model_from_info(ASETEK_VENDOR_ID, ASETEK_INVICTA_PID),
        AsetekModel::Invicta
    );
    assert_eq!(
        asetek_model_from_info(ASETEK_VENDOR_ID, ASETEK_FORTE_PID),
        AsetekModel::Forte
    );
    assert_eq!(
        asetek_model_from_info(ASETEK_VENDOR_ID, ASETEK_LAPRIMA_PID),
        AsetekModel::LaPrima
    );
    assert_eq!(
        asetek_model_from_info(ASETEK_VENDOR_ID, ASETEK_TONY_KANAAN_PID),
        AsetekModel::TonyKanaan
    );

    // Then: wrong vendor ID returns Unknown
    assert_eq!(
        asetek_model_from_info(0x0000, ASETEK_INVICTA_PID),
        AsetekModel::Unknown
    );

    // Then: unknown PID returns Unknown
    assert_eq!(
        asetek_model_from_info(ASETEK_VENDOR_ID, 0xFFFF),
        AsetekModel::Unknown
    );
}

// ─── Scenario 14: Tony Kanaan Edition shares Invicta torque ───────────────────

#[test]
fn scenario_tony_kanaan_shares_invicta_torque() -> Result<(), Box<dyn std::error::Error>> {
    // Given: Tony Kanaan model (Invicta-based, 27 Nm)
    let tk = AsetekModel::from_product_id(ASETEK_TONY_KANAAN_PID);
    assert_eq!(tk, AsetekModel::TonyKanaan);

    let invicta = AsetekModel::from_product_id(ASETEK_INVICTA_PID);

    // Then: same max torque as Invicta
    assert!(
        (tk.max_torque_nm() - invicta.max_torque_nm()).abs() < f32::EPSILON,
        "Tony Kanaan must share Invicta's 27 Nm max"
    );

    // When: both built at max torque
    let tk_data = AsetekOutputReport::new(0)
        .with_torque(tk.max_torque_nm())
        .build()?;
    let inv_data = AsetekOutputReport::new(0)
        .with_torque(invicta.max_torque_nm())
        .build()?;

    // Then: identical torque encoding
    assert_eq!(
        tk_data[2..4],
        inv_data[2..4],
        "TK and Invicta must produce identical torque bytes"
    );

    Ok(())
}

// ─── Scenario 15: default output report has safe zero state ───────────────────

#[test]
fn scenario_default_report_safe_zero_state() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a default-constructed output report
    let report = AsetekOutputReport::default();

    // Then: all fields are zero
    assert_eq!(report.sequence, 0);
    assert_eq!(report.torque_cNm, 0);
    assert_eq!(report.led_mode, 0);
    assert_eq!(report.led_value, 0);

    // When: built
    let data = report.build()?;

    // Then: entire 32-byte report is all zeros
    assert!(
        data.iter().all(|&b| b == 0),
        "default report must be all-zero bytes"
    );

    Ok(())
}
