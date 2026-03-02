#![deny(static_mut_refs)]

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hidapi::HidApi;
use serde::{Deserialize, Serialize};

/// Capture raw HID reports from connected racing wheel devices.
#[derive(Parser)]
#[command(
    name = "hid-capture",
    about = "HID device report capture tool for test fixture generation"
)]
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
    println!(
        "{:<8} {:<8} {:<12} {:<20} Product",
        "VID", "PID", "Usage Page", "Manufacturer"
    );
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
    device
        .set_blocking_mode(false)
        .context("Failed to set non-blocking mode")?;

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
        let json =
            serde_json::to_string_pretty(&capture_file).context("Failed to serialize captures")?;
        std::fs::write(path, json)
            .with_context(|| format!("Failed to write output file '{path}'"))?;
        println!("Captures saved to '{path}'.");
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let api = HidApi::new().context("Failed to initialize HidApi")?;

    match &cli.command {
        Commands::List => list_devices(&api),
        Commands::Capture {
            vid,
            pid,
            duration,
            output,
        } => capture_device(&api, *vid, *pid, *duration, output.as_deref()),
    }
}

// ── BDD-style scenario tests ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ═══ Scenario: VID/PID Hex Parsing ══════════════════════════════════════

    /// GIVEN a valid hex string with 0x prefix
    /// WHEN parse_hex_u16 is called
    /// THEN it returns the correct u16 value
    #[test]
    fn given_hex_with_0x_prefix_when_parsed_then_correct_u16_returned() {
        assert_eq!(parse_hex_u16("0x0EB7"), Ok(0x0EB7));
        assert_eq!(parse_hex_u16("0x0001"), Ok(0x0001));
        assert_eq!(parse_hex_u16("0X046D"), Ok(0x046D));
    }

    /// GIVEN a valid hex string without the 0x prefix
    /// WHEN parse_hex_u16 is called
    /// THEN it returns the correct u16 value
    #[test]
    fn given_hex_without_prefix_when_parsed_then_correct_u16_returned() {
        assert_eq!(parse_hex_u16("346E"), Ok(0x346E));
        assert_eq!(parse_hex_u16("FFFF"), Ok(0xFFFF));
        assert_eq!(parse_hex_u16("0000"), Ok(0x0000));
    }

    /// GIVEN an invalid hex string
    /// WHEN parse_hex_u16 is called
    /// THEN it returns a descriptive error
    #[test]
    fn given_invalid_hex_string_when_parsed_then_error_returned() {
        assert!(parse_hex_u16("ZZZZ").is_err());
        assert!(parse_hex_u16("xyz").is_err());
    }

    // ═══ Scenario: Capture Report Serialization ═════════════════════════════

    /// GIVEN a CaptureReport with valid fields
    /// WHEN serialized to JSON and deserialized back
    /// THEN all fields are preserved in the roundtrip
    #[test]
    fn given_capture_report_when_roundtripped_via_json_then_fields_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let report = CaptureReport {
            timestamp_us: 1_000_000,
            report_id: 0x01,
            data: "0x01 0x02 0x03".to_string(),
        };
        let json = serde_json::to_string(&report)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert_eq!(restored.timestamp_us, 1_000_000);
        assert_eq!(restored.report_id, 0x01);
        assert_eq!(restored.data, "0x01 0x02 0x03");
        Ok(())
    }

    /// GIVEN a CaptureFile with vendor/product IDs and multiple captures
    /// WHEN serialized to pretty JSON and deserialized
    /// THEN the full structure including all reports is preserved
    #[test]
    fn given_capture_file_with_reports_when_roundtripped_then_structure_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![
                CaptureReport {
                    timestamp_us: 100,
                    report_id: 0x01,
                    data: "0x01 0x80 0x00".to_string(),
                },
                CaptureReport {
                    timestamp_us: 200,
                    report_id: 0x02,
                    data: "0x02 0x90 0xFF".to_string(),
                },
            ],
        };
        let json = serde_json::to_string_pretty(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.vendor_id, "0x046D");
        assert_eq!(restored.product_id, "0x0002");
        assert_eq!(restored.captures.len(), 2);
        assert_eq!(restored.captures[0].timestamp_us, 100);
        assert_eq!(restored.captures[0].report_id, 0x01);
        assert_eq!(restored.captures[1].timestamp_us, 200);
        assert_eq!(restored.captures[1].report_id, 0x02);
        Ok(())
    }

    /// GIVEN an empty captures list
    /// WHEN serialized as a CaptureFile
    /// THEN the file deserializes with zero captures
    #[test]
    fn given_empty_captures_when_serialized_then_zero_captures_in_output()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x0000".to_string(),
            product_id: "0x0000".to_string(),
            captures: vec![],
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert!(restored.captures.is_empty());
        Ok(())
    }

    // ═══ Scenario: Hex Parsing Edge Cases ═══════════════════════════════════

    /// GIVEN the maximum u16 hex value
    /// WHEN parse_hex_u16 is called
    /// THEN it returns 0xFFFF
    #[test]
    fn given_max_u16_hex_when_parsed_then_returns_max_value() {
        assert_eq!(parse_hex_u16("0xFFFF"), Ok(0xFFFF));
    }

    /// GIVEN a hex value that overflows u16
    /// WHEN parse_hex_u16 is called
    /// THEN it returns an error
    #[test]
    fn given_overflow_hex_when_parsed_then_error_returned() {
        assert!(parse_hex_u16("0x10000").is_err());
        assert!(parse_hex_u16("0xFFFFF").is_err());
    }

    /// GIVEN an empty string
    /// WHEN parse_hex_u16 is called
    /// THEN it returns an error
    #[test]
    fn given_empty_string_when_parsed_then_error_returned() {
        assert!(parse_hex_u16("").is_err());
    }

    /// GIVEN a hex string with mixed case
    /// WHEN parse_hex_u16 is called
    /// THEN it parses correctly regardless of case
    #[test]
    fn given_mixed_case_hex_when_parsed_then_correct_value_returned() {
        assert_eq!(parse_hex_u16("0xAbCd"), Ok(0xABCD));
        assert_eq!(parse_hex_u16("abcd"), Ok(0xABCD));
    }

    /// GIVEN just the "0x" prefix with no digits
    /// WHEN parse_hex_u16 is called
    /// THEN it returns an error
    #[test]
    fn given_bare_0x_prefix_when_parsed_then_error_returned() {
        assert!(parse_hex_u16("0x").is_err());
        assert!(parse_hex_u16("0X").is_err());
    }

    /// GIVEN a single hex digit
    /// WHEN parse_hex_u16 is called
    /// THEN it returns the correct value
    #[test]
    fn given_single_digit_hex_when_parsed_then_correct_value_returned() {
        assert_eq!(parse_hex_u16("0"), Ok(0));
        assert_eq!(parse_hex_u16("F"), Ok(15));
        assert_eq!(parse_hex_u16("0xA"), Ok(10));
    }

    /// GIVEN a hex string with leading zeros
    /// WHEN parse_hex_u16 is called
    /// THEN leading zeros are handled correctly
    #[test]
    fn given_leading_zeros_when_parsed_then_correct_value_returned() {
        assert_eq!(parse_hex_u16("0x0001"), Ok(1));
        assert_eq!(parse_hex_u16("0x00FF"), Ok(255));
        assert_eq!(parse_hex_u16("0001"), Ok(1));
    }

    // ═══ Scenario: CaptureReport Field Boundaries ═══════════════════════════

    /// GIVEN a CaptureReport with zero-valued fields
    /// WHEN serialized and deserialized
    /// THEN zero values are preserved
    #[test]
    fn given_zero_valued_report_when_roundtripped_then_zeros_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let report = CaptureReport {
            timestamp_us: 0,
            report_id: 0x00,
            data: String::new(),
        };
        let json = serde_json::to_string(&report)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert_eq!(restored.timestamp_us, 0);
        assert_eq!(restored.report_id, 0x00);
        assert!(restored.data.is_empty());
        Ok(())
    }

    /// GIVEN a CaptureReport with maximum field values
    /// WHEN serialized and deserialized
    /// THEN max values are preserved
    #[test]
    fn given_max_valued_report_when_roundtripped_then_max_values_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let report = CaptureReport {
            timestamp_us: u64::MAX,
            report_id: 0xFF,
            data: "0xFF".repeat(64),
        };
        let json = serde_json::to_string(&report)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert_eq!(restored.timestamp_us, u64::MAX);
        assert_eq!(restored.report_id, 0xFF);
        assert_eq!(restored.data.len(), report.data.len());
        Ok(())
    }

    // ═══ Scenario: Capture Session Management ═══════════════════════════════

    /// GIVEN multiple CaptureReports added in sequence
    /// WHEN stored in a CaptureFile
    /// THEN the insertion order is preserved after serialization
    #[test]
    fn given_sequential_reports_when_stored_in_file_then_order_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: Vec::new(),
        };
        for i in 0..10u64 {
            file.captures.push(CaptureReport {
                timestamp_us: i * 1000,
                report_id: (i as u8) % 4,
                data: format!("0x{i:02X}"),
            });
        }
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.captures.len(), 10);
        for (i, report) in restored.captures.iter().enumerate() {
            assert_eq!(report.timestamp_us, (i as u64) * 1000);
        }
        Ok(())
    }

    /// GIVEN a CaptureFile with monotonically increasing timestamps
    /// WHEN deserialized
    /// THEN timestamps are in strictly ascending order
    #[test]
    fn given_monotonic_timestamps_when_deserialized_then_ascending_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x0EB7".to_string(),
            product_id: "0x0001".to_string(),
            captures: vec![
                CaptureReport {
                    timestamp_us: 100,
                    report_id: 1,
                    data: "0x01".into(),
                },
                CaptureReport {
                    timestamp_us: 200,
                    report_id: 1,
                    data: "0x02".into(),
                },
                CaptureReport {
                    timestamp_us: 300,
                    report_id: 1,
                    data: "0x03".into(),
                },
            ],
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        for window in restored.captures.windows(2) {
            assert!(
                window[0].timestamp_us < window[1].timestamp_us,
                "timestamps must be monotonically increasing"
            );
        }
        Ok(())
    }

    /// GIVEN a large number of capture reports
    /// WHEN serialized and deserialized
    /// THEN all reports survive the roundtrip
    #[test]
    fn given_many_reports_when_roundtripped_then_all_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let captures: Vec<CaptureReport> = (0..1000)
            .map(|i| CaptureReport {
                timestamp_us: i * 1000,
                report_id: (i % 256) as u8,
                data: format!("0x{:02X} 0x{:02X}", i % 256, (i / 256) % 256),
            })
            .collect();
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC24F".to_string(),
            captures,
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.captures.len(), 1000);
        assert_eq!(restored.captures[0].timestamp_us, 0);
        assert_eq!(restored.captures[999].timestamp_us, 999_000);
        Ok(())
    }

    // ═══ Scenario: File Format JSON Structure ═══════════════════════════════

    /// GIVEN a CaptureFile
    /// WHEN serialized to JSON
    /// THEN the JSON contains the expected top-level keys
    #[test]
    fn given_capture_file_when_serialized_then_json_has_expected_keys()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![],
        };
        let json = serde_json::to_string(&file)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        assert!(value.get("vendor_id").is_some());
        assert!(value.get("product_id").is_some());
        assert!(value.get("captures").is_some());
        assert!(value.get("captures").and_then(|v| v.as_array()).is_some());
        Ok(())
    }

    /// GIVEN a CaptureReport in a file
    /// WHEN serialized to JSON
    /// THEN each report has timestamp_us, report_id, and data fields
    #[test]
    fn given_report_in_file_when_serialized_then_report_has_expected_fields()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x0EB7".to_string(),
            product_id: "0x0001".to_string(),
            captures: vec![CaptureReport {
                timestamp_us: 42,
                report_id: 0x07,
                data: "0x07 0xFF".to_string(),
            }],
        };
        let json = serde_json::to_string(&file)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        let report = &value["captures"][0];
        assert_eq!(report["timestamp_us"], 42);
        assert_eq!(report["report_id"], 7);
        assert_eq!(report["data"], "0x07 0xFF");
        Ok(())
    }

    /// GIVEN a CaptureFile serialized to pretty JSON
    /// WHEN compared to compact JSON
    /// THEN both deserialize to identical structures
    #[test]
    fn given_pretty_and_compact_json_when_deserialized_then_identical()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x0EB7".to_string(),
            product_id: "0x0001".to_string(),
            captures: vec![CaptureReport {
                timestamp_us: 500,
                report_id: 0x03,
                data: "0x03 0x10".to_string(),
            }],
        };
        let compact = serde_json::to_string(&file)?;
        let pretty = serde_json::to_string_pretty(&file)?;
        assert_ne!(
            compact, pretty,
            "pretty and compact should differ in formatting"
        );
        let from_compact: CaptureFile = serde_json::from_str(&compact)?;
        let from_pretty: CaptureFile = serde_json::from_str(&pretty)?;
        assert_eq!(from_compact.vendor_id, from_pretty.vendor_id);
        assert_eq!(from_compact.product_id, from_pretty.product_id);
        assert_eq!(from_compact.captures.len(), from_pretty.captures.len());
        assert_eq!(
            from_compact.captures[0].timestamp_us,
            from_pretty.captures[0].timestamp_us
        );
        Ok(())
    }

    // ═══ Scenario: File I/O Roundtrip ═══════════════════════════════════════

    /// GIVEN a CaptureFile written to a temporary file
    /// WHEN read back and deserialized
    /// THEN the full structure is preserved
    #[test]
    fn given_capture_file_written_to_disk_when_read_back_then_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: vec![
                CaptureReport {
                    timestamp_us: 1000,
                    report_id: 0x01,
                    data: "0x01 0x80 0x7F".to_string(),
                },
                CaptureReport {
                    timestamp_us: 2000,
                    report_id: 0x01,
                    data: "0x01 0x81 0x80".to_string(),
                },
            ],
        };
        let dir = std::env::temp_dir().join("hid_capture_test");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("test_capture.json");
        let json = serde_json::to_string_pretty(&file)?;
        std::fs::write(&path, &json)?;
        let read_back = std::fs::read_to_string(&path)?;
        let restored: CaptureFile = serde_json::from_str(&read_back)?;
        assert_eq!(restored.vendor_id, "0x046D");
        assert_eq!(restored.product_id, "0xC266");
        assert_eq!(restored.captures.len(), 2);
        assert_eq!(restored.captures[0].data, "0x01 0x80 0x7F");
        assert_eq!(restored.captures[1].data, "0x01 0x81 0x80");
        // cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
        Ok(())
    }

    // ═══ Scenario: Error Handling ════════════════════════════════════════════

    /// GIVEN malformed JSON missing required fields
    /// WHEN deserialized as a CaptureFile
    /// THEN deserialization fails
    #[test]
    fn given_malformed_json_when_deserialized_as_capture_file_then_error() {
        let bad_json = r#"{"vendor_id": "0x046D"}"#;
        let result = serde_json::from_str::<CaptureFile>(bad_json);
        assert!(result.is_err());
    }

    /// GIVEN JSON with wrong field types
    /// WHEN deserialized as a CaptureReport
    /// THEN deserialization fails
    #[test]
    fn given_wrong_field_types_when_deserialized_as_report_then_error() {
        // timestamp_us should be u64, not string
        let bad_json = r#"{"timestamp_us": "not_a_number", "report_id": 1, "data": "0x01"}"#;
        let result = serde_json::from_str::<CaptureReport>(bad_json);
        assert!(result.is_err());
    }

    /// GIVEN JSON with extra unknown fields
    /// WHEN deserialized as a CaptureFile
    /// THEN deserialization succeeds (serde default ignores unknown fields)
    #[test]
    fn given_extra_fields_when_deserialized_then_succeeds() -> Result<(), Box<dyn std::error::Error>>
    {
        let json = r#"{
            "vendor_id": "0x046D",
            "product_id": "0x0002",
            "captures": [],
            "extra_field": "should be ignored"
        }"#;
        let file: CaptureFile = serde_json::from_str(json)?;
        assert_eq!(file.vendor_id, "0x046D");
        assert!(file.captures.is_empty());
        Ok(())
    }

    /// GIVEN completely invalid JSON
    /// WHEN deserialized as a CaptureFile
    /// THEN deserialization fails
    #[test]
    fn given_invalid_json_when_deserialized_then_error() {
        let not_json = "this is not json at all";
        let result = serde_json::from_str::<CaptureFile>(not_json);
        assert!(result.is_err());
    }

    /// GIVEN an empty JSON object
    /// WHEN deserialized as a CaptureFile
    /// THEN deserialization fails due to missing required fields
    #[test]
    fn given_empty_json_object_when_deserialized_then_error() {
        let empty = "{}";
        let result = serde_json::from_str::<CaptureFile>(empty);
        assert!(result.is_err());
    }

    /// GIVEN JSON with report_id exceeding u8 range
    /// WHEN deserialized as a CaptureReport
    /// THEN deserialization fails
    #[test]
    fn given_report_id_overflow_when_deserialized_then_error() {
        let bad_json = r#"{"timestamp_us": 100, "report_id": 256, "data": "0x01"}"#;
        let result = serde_json::from_str::<CaptureReport>(bad_json);
        assert!(result.is_err());
    }

    // ═══ Scenario: Device Filtering Helpers ═════════════════════════════════

    /// GIVEN vendor and product IDs
    /// WHEN formatted as hex strings for a CaptureFile
    /// THEN the format matches the expected "0xNNNN" pattern
    #[test]
    fn given_vid_pid_when_formatted_then_matches_hex_pattern() {
        let vid: u16 = 0x046D;
        let pid: u16 = 0xC266;
        let vid_str = format!("0x{vid:04X}");
        let pid_str = format!("0x{pid:04X}");
        assert_eq!(vid_str, "0x046D");
        assert_eq!(pid_str, "0xC266");
    }

    /// GIVEN a hex-formatted VID/PID string from a CaptureFile
    /// WHEN parsed back to u16 using parse_hex_u16
    /// THEN it returns the original numeric value
    #[test]
    fn given_formatted_vid_pid_when_parsed_back_then_original_value_restored() {
        let original_vid: u16 = 0x0EB7;
        let original_pid: u16 = 0x0001;
        let vid_str = format!("0x{original_vid:04X}");
        let pid_str = format!("0x{original_pid:04X}");
        assert_eq!(parse_hex_u16(&vid_str), Ok(original_vid));
        assert_eq!(parse_hex_u16(&pid_str), Ok(original_pid));
    }

    /// GIVEN zero VID and PID
    /// WHEN formatted and parsed
    /// THEN roundtrip produces zero
    #[test]
    fn given_zero_vid_pid_when_roundtripped_then_zero_returned() {
        let vid_str = format!("0x{:04X}", 0u16);
        let pid_str = format!("0x{:04X}", 0u16);
        assert_eq!(vid_str, "0x0000");
        assert_eq!(pid_str, "0x0000");
        assert_eq!(parse_hex_u16(&vid_str), Ok(0u16));
        assert_eq!(parse_hex_u16(&pid_str), Ok(0u16));
    }

    // ═══ Scenario: Report Playback and Data Integrity ═══════════════════════

    /// GIVEN a sequence of capture reports representing a playback session
    /// WHEN computing inter-report intervals from timestamps
    /// THEN the intervals match the expected deltas
    #[test]
    fn given_capture_sequence_when_computing_intervals_then_deltas_correct() {
        let captures = [
            CaptureReport {
                timestamp_us: 1000,
                report_id: 1,
                data: "0x01".into(),
            },
            CaptureReport {
                timestamp_us: 2000,
                report_id: 1,
                data: "0x02".into(),
            },
            CaptureReport {
                timestamp_us: 3500,
                report_id: 1,
                data: "0x03".into(),
            },
            CaptureReport {
                timestamp_us: 4000,
                report_id: 1,
                data: "0x04".into(),
            },
        ];
        let intervals: Vec<u64> = captures
            .windows(2)
            .map(|w| w[1].timestamp_us - w[0].timestamp_us)
            .collect();
        assert_eq!(intervals, vec![1000, 1500, 500]);
    }

    /// GIVEN capture reports with various report_id values
    /// WHEN filtered by a specific report_id
    /// THEN only matching reports are returned
    #[test]
    fn given_mixed_report_ids_when_filtered_then_only_matching_returned() {
        let captures = [
            CaptureReport {
                timestamp_us: 100,
                report_id: 0x01,
                data: "a".into(),
            },
            CaptureReport {
                timestamp_us: 200,
                report_id: 0x02,
                data: "b".into(),
            },
            CaptureReport {
                timestamp_us: 300,
                report_id: 0x01,
                data: "c".into(),
            },
            CaptureReport {
                timestamp_us: 400,
                report_id: 0x03,
                data: "d".into(),
            },
            CaptureReport {
                timestamp_us: 500,
                report_id: 0x01,
                data: "e".into(),
            },
        ];
        let filtered: Vec<&CaptureReport> =
            captures.iter().filter(|r| r.report_id == 0x01).collect();
        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered[0].data, "a");
        assert_eq!(filtered[1].data, "c");
        assert_eq!(filtered[2].data, "e");
    }

    /// GIVEN a CaptureFile with captures
    /// WHEN the total session duration is computed
    /// THEN it equals the difference between first and last timestamps
    #[test]
    fn given_captures_when_computing_session_duration_then_correct() {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: vec![
                CaptureReport {
                    timestamp_us: 1_000_000,
                    report_id: 1,
                    data: "0x01".into(),
                },
                CaptureReport {
                    timestamp_us: 1_500_000,
                    report_id: 1,
                    data: "0x02".into(),
                },
                CaptureReport {
                    timestamp_us: 6_000_000,
                    report_id: 1,
                    data: "0x03".into(),
                },
            ],
        };
        let duration = file
            .captures
            .last()
            .map(|l| l.timestamp_us)
            .zip(file.captures.first().map(|f| f.timestamp_us))
            .map(|(last, first)| last - first);
        assert_eq!(duration, Some(5_000_000));
    }

    /// GIVEN a hex string with whitespace
    /// WHEN parse_hex_u16 is called
    /// THEN it fails (no implicit whitespace trimming)
    #[test]
    fn given_hex_with_whitespace_when_parsed_then_error() {
        assert!(parse_hex_u16(" 0x0001").is_err());
        assert!(parse_hex_u16("0x0001 ").is_err());
    }
}
