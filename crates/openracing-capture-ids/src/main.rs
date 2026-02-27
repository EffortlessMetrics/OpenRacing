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
        let n = device.read_timeout(&mut buf, 100).context("HID read error")?;
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
        let n = device.read_timeout(&mut buf, 100).context("HID read error")?;
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

