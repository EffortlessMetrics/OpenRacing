#![deny(static_mut_refs)]

mod replay;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use crc32fast::Hasher;
use hidapi::HidApi;
use serde::Serialize;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ── Enumerate output types ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct DescriptorInfo {
    len: usize,
    crc32: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hex: Option<String>,
}

#[derive(Debug, Serialize)]
struct HidIdentity {
    vendor_id: u16,
    product_id: u16,
    vendor_id_hex: String,
    product_id_hex: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    product: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    serial: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    interface_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage_page: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<u16>,

    path: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    report_descriptor: Option<DescriptorInfo>,
}

#[derive(Debug, Serialize)]
struct Capture {
    captured_at_utc: String,
    host: HostInfo,
    devices: Vec<HidIdentity>,
}

#[derive(Debug, Serialize)]
struct HostInfo {
    os: String,
    arch: String,
}

// ── CLI ─────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "openracing-capture-ids",
    about = "HID device enumeration, capture, replay, and inspection tool"
)]
struct Cli {
    /// Vendor ID (hex, e.g. 0x346E). Defaults to 0x346E for enumeration.
    #[arg(long, value_name = "HEX")]
    vid: Option<String>,

    /// Product ID (hex, e.g. 0x0002). Required for --record and --inspect.
    #[arg(long, value_name = "HEX")]
    pid: Option<String>,

    /// Record HID input reports to the specified JSON Lines file
    #[arg(long, value_name = "FILE")]
    record: Option<PathBuf>,

    /// Replay a captured JSON Lines file
    #[arg(long, value_name = "FILE")]
    replay: Option<PathBuf>,

    /// Speed multiplier for --replay (default: 1.0 = real-time)
    #[arg(long, default_value = "1.0", value_name = "MULTIPLIER")]
    speed: f64,

    /// Continuously read and print live HID input reports from the device
    #[arg(long)]
    inspect: bool,

    /// Duration in seconds for --record and --inspect (default: 30)
    #[arg(long, default_value = "30", value_name = "N")]
    duration_secs: u64,

    /// Include full report descriptor hex in enumeration output
    #[arg(long)]
    descriptor_hex: bool,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn hex_u16(v: u16) -> String {
    format!("0x{v:04X}")
}

/// Parse a VID/PID string in hex (`0x1234`) or decimal (`4660`) form.
fn parse_hex_id(raw: &str) -> Result<u16> {
    let raw = raw.trim();
    let digits = if raw.starts_with("0x") || raw.starts_with("0X") {
        &raw[2..]
    } else {
        raw
    };
    u16::from_str_radix(digits, 16)
        .or_else(|_| raw.parse::<u16>())
        .with_context(|| format!("invalid ID value '{raw}', expected hex (0x1234) or decimal"))
}

fn captured_at_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

/// On Linux, try to read the HID report descriptor from sysfs.
fn try_read_linux_report_descriptor(hid_path: &str, include_hex: bool) -> Option<DescriptorInfo> {
    if !hid_path.starts_with("/dev/hidraw") {
        return None;
    }
    let node = std::path::Path::new(hid_path).file_name()?.to_str()?;
    let sysfs = format!("/sys/class/hidraw/{node}/device/report_descriptor");
    let bytes = fs::read(&sysfs).ok()?;

    let mut hasher = Hasher::new();
    hasher.update(&bytes);
    let crc = hasher.finalize();

    let hex = if include_hex {
        Some(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
    } else {
        None
    };

    Some(DescriptorInfo {
        len: bytes.len(),
        crc32: format!("0x{crc:08X}"),
        hex,
    })
}

// ── Vendor decode ────────────────────────────────────────────────────────────

/// Decode a raw HID report for a known vendor.
///
/// Returns a human-readable description when the VID and report format are
/// recognised; `None` for unknown vendors or unrecognised report layouts.
pub fn decode_report(vid: u16, data: &[u8]) -> Option<String> {
    match vid {
        0x346E => decode_moza_report(data),
        0x046D => decode_logitech_report(data),
        _ => None,
    }
}

fn decode_moza_report(data: &[u8]) -> Option<String> {
    let input = racing_wheel_moza_wheelbase_report::parse_wheelbase_input_report(data)?;
    Some(format!(
        "MOZA: steering={:.3} throttle={:.3} brake={:.3}",
        input.steering as f32 / 65535.0,
        input.pedals.throttle as f32 / 65535.0,
        input.pedals.brake as f32 / 65535.0,
    ))
}

fn decode_logitech_report(data: &[u8]) -> Option<String> {
    let state = racing_wheel_hid_logitech_protocol::parse_input_report(data)?;
    Some(format!(
        "Logitech: steering={:.3} throttle={:.3} brake={:.3} buttons={:04X}",
        state.steering, state.throttle, state.brake, state.buttons,
    ))
}

// ── Modes ────────────────────────────────────────────────────────────────────

fn run_enumerate(vid: u16, include_descriptor_hex: bool) -> Result<()> {
    let api = HidApi::new().context("failed to initialise HID API")?;
    let mut devices: Vec<HidIdentity> = Vec::new();

    for d in api.device_list() {
        if d.vendor_id() != vid {
            continue;
        }

        let path = d.path().to_string_lossy().to_string();
        let report_descriptor = if cfg!(target_os = "linux") {
            try_read_linux_report_descriptor(&path, include_descriptor_hex)
        } else {
            None
        };

        devices.push(HidIdentity {
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            vendor_id_hex: hex_u16(d.vendor_id()),
            product_id_hex: hex_u16(d.product_id()),
            manufacturer: d.manufacturer_string().map(str::to_string),
            product: d.product_string().map(str::to_string),
            serial: d.serial_number().map(str::to_string),
            interface_number: Some(d.interface_number()),
            usage_page: Some(d.usage_page()),
            usage: Some(d.usage()),
            path,
            report_descriptor,
        });
    }

    devices.sort_by_key(|d| {
        (
            d.product_id,
            d.interface_number.unwrap_or(-1),
            d.usage_page.unwrap_or(0),
            d.usage.unwrap_or(0),
        )
    });

    let capture = Capture {
        captured_at_utc: captured_at_utc(),
        host: HostInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        },
        devices,
    };

    println!("{}", serde_json::to_string_pretty(&capture)?);
    Ok(())
}

fn run_record(vid: u16, pid: u16, output: &Path, duration_secs: u64) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop_clone = Arc::clone(&stop);
        ctrlc::set_handler(move || {
            stop_clone.store(true, Ordering::Relaxed);
        })
        .context("failed to install Ctrl-C handler")?;
    }

    let api = HidApi::new().context("failed to initialise HID API")?;
    let device = api
        .open(vid, pid)
        .with_context(|| format!("failed to open device {vid:04X}:{pid:04X}"))?;

    let file = fs::File::create(output)
        .with_context(|| format!("failed to create output file '{}'", output.display()))?;
    let mut writer = BufWriter::new(file);

    let deadline = Instant::now() + Duration::from_secs(duration_secs);
    let mut buf = [0u8; 64];
    let mut count: usize = 0;

    eprintln!(
        "Recording {vid:04X}:{pid:04X} → '{}' for up to {duration_secs}s (Ctrl-C to stop early)",
        output.display()
    );

    while !stop.load(Ordering::Relaxed) && Instant::now() < deadline {
        let n = device
            .read_timeout(&mut buf, 100)
            .context("HID read error")?;
        if n == 0 {
            continue;
        }

        let ts_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        let report_hex: String = buf[..n].iter().map(|b| format!("{b:02x}")).collect();
        let line = serde_json::json!({
            "ts_ns": ts_ns,
            "vid": format!("0x{vid:04X}"),
            "pid": format!("0x{pid:04X}"),
            "report": report_hex,
        });
        writeln!(writer, "{line}").context("failed to write to capture file")?;
        count += 1;
    }

    writer.flush().context("failed to flush capture file")?;
    eprintln!("Recorded {count} reports → '{}'", output.display());
    Ok(())
}

fn run_inspect(vid: u16, pid: u16, duration_secs: u64) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop_clone = Arc::clone(&stop);
        ctrlc::set_handler(move || {
            stop_clone.store(true, Ordering::Relaxed);
        })
        .context("failed to install Ctrl-C handler")?;
    }

    let api = HidApi::new().context("failed to initialise HID API")?;
    let device = api
        .open(vid, pid)
        .with_context(|| format!("failed to open device {vid:04X}:{pid:04X}"))?;

    let deadline = Instant::now() + Duration::from_secs(duration_secs);
    let mut buf = [0u8; 64];
    let mut last_ts: Option<u64> = None;

    eprintln!("Inspecting {vid:04X}:{pid:04X} for up to {duration_secs}s (Ctrl-C to stop)");

    while !stop.load(Ordering::Relaxed) && Instant::now() < deadline {
        let n = device
            .read_timeout(&mut buf, 100)
            .context("HID read error")?;
        if n == 0 {
            continue;
        }

        let ts_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        let delta_us = match last_ts {
            Some(prev) => ts_ns.saturating_sub(prev) / 1_000,
            None => 0,
        };
        last_ts = Some(ts_ns);

        let data = &buf[..n];
        let report_id = data.first().copied().unwrap_or(0);
        let hex: String = data
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let ascii: String = data
            .iter()
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();

        println!("[Δ{delta_us:>7}μs] id=0x{report_id:02X} hex=[{hex}] ascii=[{ascii}]");

        if let Some(decoded) = decode_report(vid, data) {
            println!("  {decoded}");
        }
    }

    Ok(())
}

// ── Entry point ──────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.inspect {
        let vid_str = cli
            .vid
            .as_deref()
            .ok_or_else(|| anyhow!("--vid is required for --inspect"))?;
        let pid_str = cli
            .pid
            .as_deref()
            .ok_or_else(|| anyhow!("--pid is required for --inspect"))?;
        let vid = parse_hex_id(vid_str)?;
        let pid = parse_hex_id(pid_str)?;
        run_inspect(vid, pid, cli.duration_secs)?;
    } else if let Some(output) = cli.record {
        let vid_str = cli
            .vid
            .as_deref()
            .ok_or_else(|| anyhow!("--vid is required for --record"))?;
        let pid_str = cli
            .pid
            .as_deref()
            .ok_or_else(|| anyhow!("--pid is required for --record"))?;
        let vid = parse_hex_id(vid_str)?;
        let pid = parse_hex_id(pid_str)?;
        run_record(vid, pid, &output, cli.duration_secs)?;
    } else if let Some(input) = cli.replay {
        replay::replay_file(&input, cli.speed)?;
    } else {
        let vid = cli
            .vid
            .as_deref()
            .map(parse_hex_id)
            .transpose()?
            .unwrap_or(0x346E);
        run_enumerate(vid, cli.descriptor_hex)?;
    }

    Ok(())
}

// ── BDD-style scenario tests ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::{CapturedReport, decode_hex, parse_capture_line, parse_vid_str};

    // ═══ Scenario 1: Device Identification Flow ═════════════════════════════

    /// GIVEN a USB HID device with MOZA vendor ID (0x346E)
    /// WHEN a valid wheelbase input report is decoded
    /// THEN it should be identified as a MOZA device
    /// AND steering, throttle, and brake values should be present
    #[test]
    fn given_moza_vid_when_valid_report_decoded_then_identified_as_moza_with_axes()
    -> anyhow::Result<()> {
        // Minimal MOZA wheelbase report: id=0x01, steering=0x8000, throttle=0, brake=0
        let report: [u8; 7] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00];

        let text = decode_report(0x346E, &report)
            .ok_or_else(|| anyhow!("MOZA report with valid format should decode"))?;
        assert!(
            text.starts_with("MOZA:"),
            "Expected MOZA prefix, got: {text}"
        );
        assert!(
            text.contains("steering="),
            "Missing steering field in: {text}"
        );
        assert!(
            text.contains("throttle="),
            "Missing throttle field in: {text}"
        );
        assert!(text.contains("brake="), "Missing brake field in: {text}");
        Ok(())
    }

    /// GIVEN a USB HID device with Logitech vendor ID (0x046D)
    /// WHEN a valid input report is decoded
    /// THEN it should be identified as a Logitech device
    /// AND the correct protocol handler formats steering, throttle, brake, and buttons
    #[test]
    fn given_logitech_vid_when_valid_report_decoded_then_identified_as_logitech_with_axes()
    -> anyhow::Result<()> {
        // Logitech report: id=0x01, steering=center(0x8000), axes=0, hat=neutral(0x08)
        let report: [u8; 10] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];

        let text = decode_report(0x046D, &report)
            .ok_or_else(|| anyhow!("Logitech report with valid format should decode"))?;
        assert!(
            text.starts_with("Logitech:"),
            "Expected Logitech prefix, got: {text}"
        );
        assert!(
            text.contains("steering="),
            "Missing steering field in: {text}"
        );
        assert!(
            text.contains("throttle="),
            "Missing throttle field in: {text}"
        );
        assert!(text.contains("brake="), "Missing brake field in: {text}");
        assert!(
            text.contains("buttons="),
            "Missing buttons field in: {text}"
        );
        Ok(())
    }

    /// GIVEN a MOZA device report with full-deflection axis values
    /// WHEN the report is decoded
    /// THEN the protocol handler should produce correctly normalised values
    #[test]
    fn given_moza_full_deflection_when_decoded_then_values_normalised_correctly()
    -> anyhow::Result<()> {
        // steering=0xFFFF (max), throttle=0xFFFF (max), brake=0x0000 (min)
        let report: [u8; 7] = [0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00];

        let text = decode_report(0x346E, &report)
            .ok_or_else(|| anyhow!("MOZA full-deflection report should decode"))?;
        assert!(
            text.contains("steering=1.000"),
            "Expected full steering, got: {text}"
        );
        assert!(
            text.contains("throttle=1.000"),
            "Expected full throttle, got: {text}"
        );
        assert!(
            text.contains("brake=0.000"),
            "Expected zero brake, got: {text}"
        );
        Ok(())
    }

    /// GIVEN a Logitech device with full-left steering and full throttle
    /// WHEN the report is decoded
    /// THEN the normalised values should reflect correct axis positions
    #[test]
    fn given_logitech_full_left_and_throttle_when_decoded_then_values_correct() -> anyhow::Result<()>
    {
        // steering=0x0000 (full left → -1.0), throttle=0xFF (full), brake=0x00
        let report: [u8; 10] = [0x01, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];

        let text = decode_report(0x046D, &report)
            .ok_or_else(|| anyhow!("Logitech full-left report should decode"))?;
        assert!(
            text.contains("steering=-1.000"),
            "Expected full-left steering, got: {text}"
        );
        assert!(
            text.contains("throttle=1.000"),
            "Expected full throttle, got: {text}"
        );
        assert!(
            text.contains("brake=0.000"),
            "Expected zero brake, got: {text}"
        );
        Ok(())
    }

    // ═══ Scenario 2: Capture Session Flow ═══════════════════════════════════

    /// GIVEN a captured JSON Lines entry from a MOZA device
    /// WHEN the capture line is parsed and report bytes are decoded
    /// THEN the input report should be parsed correctly
    /// AND vendor-specific decode should produce human-readable output
    #[test]
    fn given_moza_capture_line_when_parsed_and_decoded_then_pipeline_produces_output()
    -> anyhow::Result<()> {
        // MOZA capture: id=0x01, steering=0x8000, throttle=0, brake=0
        let line =
            r#"{"ts_ns":1000000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#;

        // When: parse the capture line
        let entry = parse_capture_line(line)?;
        assert_eq!(entry.vid, "0x346E");
        assert_eq!(entry.pid, "0x0002");

        // When: decode the hex report to bytes
        let bytes = decode_hex(&entry.report)?;
        assert_eq!(bytes.len(), 7);
        assert_eq!(bytes[0], 0x01, "report ID should be 0x01");

        // Then: vendor-specific decode succeeds
        let vid = parse_vid_str(&entry.vid)?;
        assert_eq!(vid, 0x346E);
        let decoded = decode_report(vid, &bytes)
            .ok_or_else(|| anyhow!("MOZA report should decode via pipeline"))?;
        assert!(decoded.starts_with("MOZA:"));
        Ok(())
    }

    /// GIVEN a captured JSON Lines entry from a Logitech device
    /// WHEN the full capture-to-decode pipeline is executed
    /// THEN the output is correctly formatted with all expected fields
    #[test]
    fn given_logitech_capture_line_when_full_pipeline_executed_then_output_formatted()
    -> anyhow::Result<()> {
        // Logitech capture: id=0x01, steering=center, all zero, hat=neutral
        let line =
            r#"{"ts_ns":2000000000,"vid":"0x046D","pid":"0x0001","report":"01008000000000000800"}"#;

        let entry = parse_capture_line(line)?;
        let bytes = decode_hex(&entry.report)?;
        assert_eq!(bytes.len(), 10);

        let vid = parse_vid_str(&entry.vid)?;
        let decoded = decode_report(vid, &bytes)
            .ok_or_else(|| anyhow!("Logitech report should decode via pipeline"))?;
        assert!(decoded.starts_with("Logitech:"));
        assert!(
            decoded.contains("steering=0.000"),
            "Center steering should be 0.000, got: {decoded}"
        );
        Ok(())
    }

    /// GIVEN a captured report with timestamps
    /// WHEN multiple capture lines are parsed sequentially
    /// THEN timestamps are preserved and monotonically increasing
    #[test]
    fn given_sequential_captures_when_parsed_then_timestamps_monotonically_increasing()
    -> anyhow::Result<()> {
        let lines = [
            r#"{"ts_ns":1000000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1001000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1002000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
        ];

        let mut prev_ts = 0u64;
        for line in &lines {
            let entry = parse_capture_line(line)?;
            assert!(
                entry.ts_ns > prev_ts,
                "Timestamps must increase: {} <= {}",
                entry.ts_ns,
                prev_ts
            );
            prev_ts = entry.ts_ns;
        }
        Ok(())
    }

    // ═══ Scenario 3: Unknown Device Handling ════════════════════════════════

    /// GIVEN an unknown VID that is not MOZA or Logitech
    /// WHEN decode_report is called
    /// THEN it should return None (generic HID fallback)
    /// AND no vendor-specific parsing should be attempted
    #[test]
    fn given_unknown_vid_when_decode_attempted_then_returns_none() {
        let report: [u8; 10] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];

        assert!(
            decode_report(0xFFFF, &report).is_none(),
            "Unknown VID 0xFFFF should return None"
        );
        assert!(
            decode_report(0x0000, &report).is_none(),
            "Zero VID should return None"
        );
        assert!(
            decode_report(0x1234, &report).is_none(),
            "Arbitrary VID should return None"
        );
    }

    /// GIVEN a known VID but a malformed report (too short)
    /// WHEN decode_report is called
    /// THEN it should return None rather than crashing
    #[test]
    fn given_known_vid_with_malformed_report_when_decoded_then_returns_none_gracefully() {
        // MOZA VID with too-short report (< 7 bytes minimum)
        let short_report: [u8; 2] = [0x01, 0x00];
        assert!(
            decode_report(0x346E, &short_report).is_none(),
            "Short MOZA report should return None"
        );

        // Logitech VID with too-short report (< 10 bytes minimum)
        let short_report: [u8; 4] = [0x01, 0x00, 0x80, 0x00];
        assert!(
            decode_report(0x046D, &short_report).is_none(),
            "Short Logitech report should return None"
        );
    }

    /// GIVEN a known VID but a wrong report ID
    /// WHEN decode_report is called
    /// THEN it should return None (report ID mismatch)
    #[test]
    fn given_known_vid_with_wrong_report_id_when_decoded_then_returns_none() {
        // MOZA report with wrong report ID (0x02 instead of 0x01)
        let report: [u8; 7] = [0x02, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00];
        assert!(
            decode_report(0x346E, &report).is_none(),
            "Wrong report ID for MOZA should return None"
        );

        // Logitech report with wrong report ID
        let report: [u8; 10] = [0x02, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
        assert!(
            decode_report(0x046D, &report).is_none(),
            "Wrong report ID for Logitech should return None"
        );
    }

    /// GIVEN an empty report buffer
    /// WHEN decode_report is called for any VID
    /// THEN it should return None without panicking
    #[test]
    fn given_empty_report_when_decoded_then_returns_none_safely() {
        let empty: [u8; 0] = [];
        assert!(decode_report(0x346E, &empty).is_none());
        assert!(decode_report(0x046D, &empty).is_none());
        assert!(decode_report(0xFFFF, &empty).is_none());
    }

    // ═══ Scenario 4: Multi-Device Handling ══════════════════════════════════

    /// GIVEN multiple devices with different VIDs connected simultaneously
    /// WHEN each device report is decoded
    /// THEN they should use separate protocol handlers
    /// AND no data from one device leaks into another
    #[test]
    fn given_multiple_devices_when_decoded_then_separate_protocol_handlers_used()
    -> anyhow::Result<()> {
        // Device A: MOZA wheelbase at center
        let moza_report: [u8; 7] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00];
        // Device B: Logitech wheel at center
        let logi_report: [u8; 10] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];

        let moza_text = decode_report(0x346E, &moza_report)
            .ok_or_else(|| anyhow!("MOZA device should decode"))?;
        let logi_text = decode_report(0x046D, &logi_report)
            .ok_or_else(|| anyhow!("Logitech device should decode"))?;

        // Then: each uses its own protocol handler prefix
        assert!(
            moza_text.starts_with("MOZA:"),
            "Device A should use MOZA handler"
        );
        assert!(
            logi_text.starts_with("Logitech:"),
            "Device B should use Logitech handler"
        );
        // Then: no cross-contamination between device outputs
        assert!(
            !moza_text.contains("Logitech"),
            "MOZA output must not contain Logitech data"
        );
        assert!(
            !logi_text.contains("MOZA"),
            "Logitech output must not contain MOZA data"
        );
        Ok(())
    }

    /// GIVEN multiple reports from different devices interleaved in a capture
    /// WHEN each capture line is parsed and decoded independently
    /// THEN each report is routed to the correct vendor handler
    /// AND report data is not shared between devices
    #[test]
    fn given_interleaved_multi_device_captures_when_decoded_then_no_conflicts() -> anyhow::Result<()>
    {
        let capture_lines = [
            r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1001,"vid":"0x046D","pid":"0x0001","report":"01008000000000000800"}"#,
            r#"{"ts_ns":1002,"vid":"0x346E","pid":"0x0002","report":"01ffff00000000"}"#,
            r#"{"ts_ns":1003,"vid":"0x046D","pid":"0x0001","report":"0100800000ff00000800"}"#,
        ];

        let mut moza_outputs = Vec::new();
        let mut logi_outputs = Vec::new();

        for line in &capture_lines {
            let entry = parse_capture_line(line)?;
            let bytes = decode_hex(&entry.report)?;
            let vid = parse_vid_str(&entry.vid)?;

            if let Some(text) = decode_report(vid, &bytes) {
                match vid {
                    0x346E => moza_outputs.push(text),
                    0x046D => logi_outputs.push(text),
                    _ => {}
                }
            }
        }

        assert_eq!(moza_outputs.len(), 2, "Should have 2 MOZA reports");
        assert_eq!(logi_outputs.len(), 2, "Should have 2 Logitech reports");

        for output in &moza_outputs {
            assert!(
                output.starts_with("MOZA:"),
                "All MOZA outputs should use MOZA handler"
            );
        }
        for output in &logi_outputs {
            assert!(
                output.starts_with("Logitech:"),
                "All Logitech outputs should use Logitech handler"
            );
        }
        Ok(())
    }

    // ═══ Scenario: VID/PID Parsing Helpers ══════════════════════════════════

    /// GIVEN a VID/PID as hex string
    /// WHEN parse_hex_id is called
    /// THEN it returns the correct u16 value
    #[test]
    fn given_hex_vid_pid_when_parsed_then_correct_u16_returned() -> anyhow::Result<()> {
        assert_eq!(parse_hex_id("0x346E")?, 0x346E);
        assert_eq!(parse_hex_id("0x046D")?, 0x046D);
        assert_eq!(parse_hex_id("0X00FF")?, 0x00FF);
        Ok(())
    }

    /// GIVEN the hex_u16 formatter
    /// WHEN a u16 value is formatted
    /// THEN it produces the canonical 0xNNNN representation
    #[test]
    fn given_u16_value_when_formatted_then_canonical_hex_representation() {
        assert_eq!(hex_u16(0x346E), "0x346E");
        assert_eq!(hex_u16(0x046D), "0x046D");
        assert_eq!(hex_u16(0x0000), "0x0000");
        assert_eq!(hex_u16(0xFFFF), "0xFFFF");
    }

    /// GIVEN a CapturedReport constructed manually
    /// WHEN serialized and deserialized
    /// THEN all fields survive the roundtrip
    #[test]
    fn given_captured_report_when_roundtripped_then_fields_preserved() -> anyhow::Result<()> {
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
}
