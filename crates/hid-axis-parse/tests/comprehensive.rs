use racing_wheel_hid_axis_parse::parse_u16_le_at;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn boundary_values_round_trip() -> TestResult {
    let zero = parse_u16_le_at(&[0x00, 0x00], 0).ok_or("expected zero parse")?;
    assert_eq!(zero, 0);

    let max = parse_u16_le_at(&[0xFF, 0xFF], 0).ok_or("expected max parse")?;
    assert_eq!(max, u16::MAX);
    Ok(())
}

#[test]
fn saturating_offset_does_not_panic() {
    assert_eq!(parse_u16_le_at(&[0xFF, 0xFF], usize::MAX), None);
}
