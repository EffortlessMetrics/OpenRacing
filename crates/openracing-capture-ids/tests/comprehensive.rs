use openracing_capture_ids::replay::{
    CapturedReport, decode_hex, parse_capture_line, parse_vid_str,
};
use openracing_capture_ids::{decode_report, hex_u16, parse_hex_id};

// ── VID/PID Capture and Storage ─────────────────────────────────────────────

#[test]
fn hex_u16_formats_all_boundaries() -> anyhow::Result<()> {
    assert_eq!(hex_u16(0x0000), "0x0000");
    assert_eq!(hex_u16(0x346E), "0x346E");
    assert_eq!(hex_u16(0x046D), "0x046D");
    assert_eq!(hex_u16(0xFFFF), "0xFFFF");
    Ok(())
}

#[test]
fn parse_hex_id_with_0x_prefix() -> anyhow::Result<()> {
    assert_eq!(parse_hex_id("0x346E")?, 0x346E);
    assert_eq!(parse_hex_id("0x046D")?, 0x046D);
    assert_eq!(parse_hex_id("0X00FF")?, 0x00FF);
    Ok(())
}

#[test]
fn parse_hex_id_without_prefix() -> anyhow::Result<()> {
    assert_eq!(parse_hex_id("346E")?, 0x346E);
    assert_eq!(parse_hex_id("FFFF")?, 0xFFFF);
    Ok(())
}

#[test]
fn parse_hex_id_trims_whitespace() -> anyhow::Result<()> {
    assert_eq!(parse_hex_id("  0x046D  ")?, 0x046D);
    Ok(())
}

#[test]
fn parse_hex_id_invalid_returns_error() {
    assert!(parse_hex_id("ZZZZ").is_err());
    assert!(parse_hex_id("").is_err());
}

#[test]
fn hex_u16_roundtrip_through_parse() -> anyhow::Result<()> {
    for val in [0u16, 1, 0x346E, 0x046D, 0xFFFF] {
        let formatted = hex_u16(val);
        let parsed = parse_hex_id(&formatted)?;
        assert_eq!(parsed, val, "roundtrip failed for {val}");
    }
    Ok(())
}

// ── Device Fingerprinting (vendor decode) ───────────────────────────────────

#[test]
fn decode_moza_report_produces_moza_prefix() -> anyhow::Result<()> {
    let report: [u8; 7] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00];
    let text = decode_report(0x346E, &report)
        .ok_or_else(|| anyhow::anyhow!("MOZA decode failed"))?;
    assert!(text.starts_with("MOZA:"), "got: {text}");
    assert!(text.contains("steering="));
    assert!(text.contains("throttle="));
    assert!(text.contains("brake="));
    Ok(())
}

#[test]
fn decode_logitech_report_produces_logitech_prefix() -> anyhow::Result<()> {
    let report: [u8; 10] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
    let text = decode_report(0x046D, &report)
        .ok_or_else(|| anyhow::anyhow!("Logitech decode failed"))?;
    assert!(text.starts_with("Logitech:"), "got: {text}");
    assert!(text.contains("steering="));
    assert!(text.contains("buttons="));
    Ok(())
}

#[test]
fn decode_unknown_vid_returns_none() {
    let report: [u8; 10] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
    assert!(decode_report(0xFFFF, &report).is_none());
    assert!(decode_report(0x0000, &report).is_none());
    assert!(decode_report(0x1234, &report).is_none());
}

#[test]
fn decode_known_vid_with_short_report_returns_none() {
    let short: [u8; 2] = [0x01, 0x00];
    assert!(decode_report(0x346E, &short).is_none());
    assert!(decode_report(0x046D, &short).is_none());
}

#[test]
fn decode_known_vid_with_wrong_report_id_returns_none() {
    let moza_wrong_id: [u8; 7] = [0x02, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00];
    assert!(decode_report(0x346E, &moza_wrong_id).is_none());

    let logi_wrong_id: [u8; 10] = [0x02, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
    assert!(decode_report(0x046D, &logi_wrong_id).is_none());
}

#[test]
fn decode_empty_report_returns_none() {
    let empty: [u8; 0] = [];
    assert!(decode_report(0x346E, &empty).is_none());
    assert!(decode_report(0x046D, &empty).is_none());
}

#[test]
fn decode_moza_full_deflection() -> anyhow::Result<()> {
    let report: [u8; 7] = [0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00];
    let text = decode_report(0x346E, &report)
        .ok_or_else(|| anyhow::anyhow!("decode failed"))?;
    assert!(text.contains("steering=1.000"));
    assert!(text.contains("throttle=1.000"));
    assert!(text.contains("brake=0.000"));
    Ok(())
}

// ── Enumeration Patterns (capture line parsing) ─────────────────────────────

#[test]
fn parse_capture_line_all_fields() -> anyhow::Result<()> {
    let line = r#"{"ts_ns":1234567890,"vid":"0x046D","pid":"0x0002","report":"0102030405"}"#;
    let entry = parse_capture_line(line)?;
    assert_eq!(entry.ts_ns, 1_234_567_890);
    assert_eq!(entry.vid, "0x046D");
    assert_eq!(entry.pid, "0x0002");
    assert_eq!(entry.report, "0102030405");
    Ok(())
}

#[test]
fn parse_capture_line_missing_field_fails() {
    let line = r#"{"ts_ns":1234,"vid":"0x046D","pid":"0x0002"}"#;
    assert!(parse_capture_line(line).is_err());
}

#[test]
fn decode_hex_basic() -> anyhow::Result<()> {
    let bytes = decode_hex("0102030405")?;
    assert_eq!(bytes, vec![0x01, 0x02, 0x03, 0x04, 0x05]);
    Ok(())
}

#[test]
fn decode_hex_empty_string() -> anyhow::Result<()> {
    let bytes = decode_hex("")?;
    assert!(bytes.is_empty());
    Ok(())
}

#[test]
fn decode_hex_odd_length_fails() {
    assert!(decode_hex("012").is_err());
}

#[test]
fn decode_hex_invalid_chars_fails() {
    assert!(decode_hex("0xZZ").is_err());
}

#[test]
fn parse_vid_str_with_and_without_prefix() -> anyhow::Result<()> {
    assert_eq!(parse_vid_str("0x046D")?, 0x046D);
    assert_eq!(parse_vid_str("0x346E")?, 0x346E);
    assert_eq!(parse_vid_str("046D")?, 0x046D);
    Ok(())
}

#[test]
fn parse_vid_str_invalid_fails() {
    assert!(parse_vid_str("ZZZZ").is_err());
}

#[test]
fn captured_report_roundtrip() -> anyhow::Result<()> {
    let original = CapturedReport {
        ts_ns: 9_999_999_000,
        vid: "0x346E".to_string(),
        pid: "0x0002".to_string(),
        report: "01ff80aabbccdd".to_string(),
    };
    let json = serde_json::to_string(&original)?;
    let parsed = parse_capture_line(&json)?;
    assert_eq!(parsed.ts_ns, original.ts_ns);
    assert_eq!(parsed.vid, original.vid);
    assert_eq!(parsed.pid, original.pid);
    assert_eq!(parsed.report, original.report);
    Ok(())
}

#[test]
fn sequential_captures_timestamps_increase() -> anyhow::Result<()> {
    let lines = [
        r#"{"ts_ns":1000000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
        r#"{"ts_ns":1001000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
        r#"{"ts_ns":1002000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
    ];
    let mut prev_ts = 0u64;
    for line in &lines {
        let entry = parse_capture_line(line)?;
        assert!(entry.ts_ns > prev_ts, "{} <= {}", entry.ts_ns, prev_ts);
        prev_ts = entry.ts_ns;
    }
    Ok(())
}

#[test]
fn full_capture_decode_pipeline_moza() -> anyhow::Result<()> {
    let line =
        r#"{"ts_ns":1000000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#;
    let entry = parse_capture_line(line)?;
    let bytes = decode_hex(&entry.report)?;
    let vid = parse_vid_str(&entry.vid)?;
    let decoded = decode_report(vid, &bytes)
        .ok_or_else(|| anyhow::anyhow!("pipeline decode failed"))?;
    assert!(decoded.starts_with("MOZA:"));
    Ok(())
}

#[test]
fn full_capture_decode_pipeline_logitech() -> anyhow::Result<()> {
    let line =
        r#"{"ts_ns":2000000000,"vid":"0x046D","pid":"0x0001","report":"01008000000000000800"}"#;
    let entry = parse_capture_line(line)?;
    let bytes = decode_hex(&entry.report)?;
    let vid = parse_vid_str(&entry.vid)?;
    let decoded = decode_report(vid, &bytes)
        .ok_or_else(|| anyhow::anyhow!("pipeline decode failed"))?;
    assert!(decoded.starts_with("Logitech:"));
    Ok(())
}

#[test]
fn interleaved_multi_vendor_captures() -> anyhow::Result<()> {
    let lines = [
        r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
        r#"{"ts_ns":1001,"vid":"0x046D","pid":"0x0001","report":"01008000000000000800"}"#,
        r#"{"ts_ns":1002,"vid":"0x346E","pid":"0x0002","report":"01ffff00000000"}"#,
    ];
    let mut moza_count = 0;
    let mut logi_count = 0;
    for line in &lines {
        let entry = parse_capture_line(line)?;
        let bytes = decode_hex(&entry.report)?;
        let vid = parse_vid_str(&entry.vid)?;
        if let Some(text) = decode_report(vid, &bytes) {
            if text.starts_with("MOZA:") {
                moza_count += 1;
            } else if text.starts_with("Logitech:") {
                logi_count += 1;
            }
        }
    }
    assert_eq!(moza_count, 2);
    assert_eq!(logi_count, 1);
    Ok(())
}

#[test]
fn hex_roundtrip_encode_decode() -> anyhow::Result<()> {
    let original: Vec<u8> = (0u8..=15).collect();
    let hex: String = original.iter().map(|b| format!("{b:02x}")).collect();
    let decoded = decode_hex(&hex)?;
    assert_eq!(decoded, original);
    Ok(())
}
