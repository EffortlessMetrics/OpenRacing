#![deny(static_mut_refs)]

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hidapi::HidApi;
use serde::{Deserialize, Serialize};

/// Capture raw HID reports from connected racing wheel devices.
#[derive(Parser)]
#[command(name = "hid-capture", about = "HID device report capture tool for test fixture generation")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all connected HID devices
    List,
    /// Capture raw HID reports from a specific device
    Capture {
        /// Vendor ID (hex, e.g. 0x0EB7)
        #[arg(long, value_parser = parse_hex_u16)]
        vid: u16,
        /// Product ID (hex, e.g. 0x0001)
        #[arg(long, value_parser = parse_hex_u16)]
        pid: u16,
        /// Capture duration in seconds (default: 5)
        #[arg(long, default_value = "5")]
        duration: u64,
        /// Save captures to a JSON file instead of printing
        #[arg(long)]
        output: Option<String>,
    },
}

fn parse_hex_u16(s: &str) -> Result<u16, String> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(s, 16).map_err(|e| format!("invalid hex value '{s}': {e}"))
}

#[derive(Debug, Serialize, Deserialize)]
struct CaptureReport {
    timestamp_us: u64,
    report_id: u8,
    data: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CaptureFile {
    vendor_id: String,
    product_id: String,
    captures: Vec<CaptureReport>,
}

fn list_devices(api: &HidApi) -> Result<()> {
    let devices: Vec<_> = api.device_list().collect();
    if devices.is_empty() {
        println!("No HID devices found.");
        return Ok(());
    }
    println!("{:<8} {:<8} {:<12} {:<20} {}", "VID", "PID", "Usage Page", "Manufacturer", "Product");
    println!("{}", "-".repeat(80));
    for dev in devices {
        println!(
            "{:<8} {:<8} {:<12} {:<20} {}",
            format!("0x{:04X}", dev.vendor_id()),
            format!("0x{:04X}", dev.product_id()),
            format!("0x{:04X}", dev.usage_page()),
            dev.manufacturer_string().unwrap_or("(unknown)"),
            dev.product_string().unwrap_or("(unknown)"),
        );
    }
    Ok(())
}

fn capture_device(
    api: &HidApi,
    vid: u16,
    pid: u16,
    duration_secs: u64,
    output: Option<&str>,
) -> Result<()> {
    let device = api
        .open(vid, pid)
        .with_context(|| format!("Failed to open device VID=0x{vid:04X} PID=0x{pid:04X}"))?;

    // Non-blocking read: returns immediately if no data available
    device.set_blocking_mode(false).context("Failed to set non-blocking mode")?;

    let start = Instant::now();
    let epoch_start = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64;

    let deadline = Duration::from_secs(duration_secs);
    let mut buf = [0u8; 64];
    let mut captures: Vec<CaptureReport> = Vec::new();

    println!("Capturing from VID=0x{vid:04X} PID=0x{pid:04X} for {duration_secs}s...");

    while start.elapsed() < deadline {
        match device.read(&mut buf) {
            Ok(0) => {
                // No data yet; yield briefly
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            Ok(n) => {
                let elapsed_us = start.elapsed().as_micros() as u64;
                let timestamp_us = epoch_start + elapsed_us;
                let report_id = buf[0];
                let hex = buf[..n]
                    .iter()
                    .map(|b| format!("0x{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ");

                if output.is_none() {
                    println!("[+{elapsed_us:>10}µs] id=0x{report_id:02X}  {hex}");
                }

                captures.push(CaptureReport {
                    timestamp_us,
                    report_id,
                    data: hex,
                });
            }
            Err(e) => {
                eprintln!("Read error: {e}");
                break;
            }
        }
    }

    println!("Captured {} report(s).", captures.len());

    if let Some(path) = output {
        let capture_file = CaptureFile {
            vendor_id: format!("0x{vid:04X}"),
            product_id: format!("0x{pid:04X}"),
            captures,
        };
        let json = serde_json::to_string_pretty(&capture_file).context("Failed to serialize captures")?;
        std::fs::write(path, json).with_context(|| format!("Failed to write output file '{path}'"))?;
        println!("Captures saved to '{path}'.");
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let api = HidApi::new().context("Failed to initialize HidApi")?;

    match &cli.command {
        Commands::List => list_devices(&api),
        Commands::Capture { vid, pid, duration, output } => {
            capture_device(&api, *vid, *pid, *duration, output.as_deref())
        }
    }
}
