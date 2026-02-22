use crc32fast::Hasher;
use hidapi::HidApi;
use serde::Serialize;
use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

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

fn hex_u16(v: u16) -> String {
    format!("0x{v:04X}")
}

fn parse_vid(raw: &str) -> Result<u16, Box<dyn Error>> {
    let raw = raw.trim();
    let v = raw.trim_start_matches("0x").trim_start_matches("0X");
    if raw.starts_with("0x") || raw.starts_with("0X") {
        u16::from_str_radix(v, 16)
            .map_err(|_| format!("invalid VID value '{raw}', expected hex like 0x346E").into())
    } else {
        raw.parse::<u16>().or_else(|_| {
            u16::from_str_radix(v, 16).map_err(|_| {
                format!("invalid VID value '{raw}', expected hex or decimal number").into()
            })
        })
    }
}

/// If the HID path looks like /dev/hidrawX, try to read the report descriptor from
/// sysfs. This avoids ioctl complexity and is usually sufficient for identity capture.
fn try_read_linux_report_descriptor(hid_path: &str, include_hex: bool) -> Option<DescriptorInfo> {
    if !hid_path.starts_with("/dev/hidraw") {
        return None;
    }
    let node = Path::new(hid_path).file_name()?.to_str()?;
    let sysfs = format!("/sys/class/hidraw/{node}/device/report_descriptor");
    let bytes = fs::read(&sysfs).ok()?;

    let mut hasher = Hasher::new();
    hasher.update(&bytes);
    let crc = hasher.finalize();

    let hex = if include_hex {
        Some(
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        )
    } else {
        None
    };

    Some(DescriptorInfo {
        len: bytes.len(),
        crc32: format!("0x{crc:08X}"),
        hex,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut vid: u16 = 0x346E;
    let mut include_descriptor_hex = false;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--vid" => {
                let raw = args.next().ok_or("--vid requires a value")?;
                vid = parse_vid(&raw)?;
            }
            "--descriptor-hex" => {
                include_descriptor_hex = true;
            }
            _ => {}
        }
    }

    let api = HidApi::new()?;
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
            os: env::consts::OS.to_string(),
            arch: env::consts::ARCH.to_string(),
        },
        devices,
    };

    println!("{}", serde_json::to_string_pretty(&capture)?);
    Ok(())
}

fn captured_at_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}
