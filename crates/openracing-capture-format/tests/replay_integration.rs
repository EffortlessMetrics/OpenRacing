//! Integration tests: replay synthetic captures through real protocol parsers.

use openracing_capture_format::replay::replay_parse;
use openracing_capture_format::{CaptureError, build_synthetic_session};

// ── Cammus ───────────────────────────────────────────────────────────────────

#[test]
fn replay_cammus_synthetic() -> Result<(), CaptureError> {
    let session = build_synthetic_session("cammus", 50)
        .ok_or_else(|| CaptureError::UnsupportedVersion("cammus not found".into()))?;

    // Cammus parser expects raw bytes *without* report ID prefix — the payload
    // from our capture is already the raw report body.  But replay_parse
    // prepends the report_id.  Cammus `parse()` needs ≥ 12 bytes regardless of
    // report ID, so we slice past the prepended ID.
    let result = replay_parse(&session, |data: &[u8]| {
        if data.len() < 2 {
            return None;
        }
        // Strip the report_id byte we prepend and pass raw payload.
        racing_wheel_hid_cammus_protocol::report::parse(&data[1..]).ok()
    });

    assert!(!result.parsed.is_empty(), "expected some parsed records");
    assert_eq!(result.failed, 0, "no cammus records should fail to parse");
    // Spot-check first parsed value
    let first = &result.parsed[0].value;
    assert!(first.steering >= -1.0 && first.steering <= 1.0);
    assert!(first.throttle >= 0.0 && first.throttle <= 1.0);
    Ok(())
}

// ── Fanatec ──────────────────────────────────────────────────────────────────

#[test]
fn replay_fanatec_synthetic() -> Result<(), CaptureError> {
    let session = build_synthetic_session("fanatec", 50)
        .ok_or_else(|| CaptureError::UnsupportedVersion("fanatec not found".into()))?;

    // Fanatec parse_standard_report expects report_id as first byte (0x01).
    let result = replay_parse(&session, |data: &[u8]| {
        racing_wheel_hid_fanatec_protocol::parse_standard_report(data)
    });

    assert!(
        !result.parsed.is_empty(),
        "expected some parsed Fanatec records"
    );
    let first = &result.parsed[0].value;
    assert!(first.steering >= -1.0 && first.steering <= 1.0);
    Ok(())
}

// ── Simagic ──────────────────────────────────────────────────────────────────

#[test]
fn replay_simagic_synthetic() -> Result<(), CaptureError> {
    let session = build_synthetic_session("simagic", 50)
        .ok_or_else(|| CaptureError::UnsupportedVersion("simagic not found".into()))?;

    // Simagic parse_input_report expects raw bytes, no report_id prefix.
    // Our synthetic simagic uses report_id=0x00 so first byte is 0x00.
    let result = replay_parse(&session, |data: &[u8]| {
        // Skip the 0x00 report_id byte we prepend.
        if data.is_empty() {
            return None;
        }
        racing_wheel_hid_simagic_protocol::parse_input_report(&data[1..])
    });

    assert!(
        !result.parsed.is_empty(),
        "expected some parsed Simagic records"
    );
    Ok(())
}

// ── Thrustmaster ─────────────────────────────────────────────────────────────

#[test]
fn replay_thrustmaster_synthetic() -> Result<(), CaptureError> {
    let session = build_synthetic_session("thrustmaster", 50)
        .ok_or_else(|| CaptureError::UnsupportedVersion("thrustmaster not found".into()))?;

    // Thrustmaster parse_input_report expects report_id 0x01 as first byte.
    let result = replay_parse(&session, |data: &[u8]| {
        racing_wheel_hid_thrustmaster_protocol::parse_input_report(data)
    });

    assert!(
        !result.parsed.is_empty(),
        "expected some parsed Thrustmaster records"
    );
    let first = &result.parsed[0].value;
    assert!(first.steering >= -1.0 && first.steering <= 1.0);
    Ok(())
}

// ── VRS ──────────────────────────────────────────────────────────────────────

#[test]
fn replay_vrs_synthetic() -> Result<(), CaptureError> {
    let session = build_synthetic_session("vrs", 50)
        .ok_or_else(|| CaptureError::UnsupportedVersion("vrs not found".into()))?;

    // VRS parse_input_report expects raw bytes without report_id prefix.
    let result = replay_parse(&session, |data: &[u8]| {
        if data.is_empty() {
            return None;
        }
        racing_wheel_hid_vrs_protocol::parse_input_report(&data[1..])
    });

    assert!(
        !result.parsed.is_empty(),
        "expected some parsed VRS records"
    );
    Ok(())
}

// ── Round-trip: build → JSON → load → replay ─────────────────────────────────

#[test]
fn json_roundtrip_then_replay() -> Result<(), CaptureError> {
    let session = build_synthetic_session("cammus", 20)
        .ok_or_else(|| CaptureError::UnsupportedVersion("cammus not found".into()))?;

    let json = session.to_json()?;
    let restored = openracing_capture_format::CaptureSession::from_json(&json)?;

    let result = replay_parse(&restored, |data: &[u8]| {
        if data.len() < 2 {
            return None;
        }
        racing_wheel_hid_cammus_protocol::report::parse(&data[1..]).ok()
    });

    assert_eq!(result.parsed.len(), 20);
    assert_eq!(result.failed, 0);
    Ok(())
}
