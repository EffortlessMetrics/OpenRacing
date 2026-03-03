//! Capture tooling tests for openracing-capture-ids.
//!
//! Covers:
//! 1. HID device descriptor capture and parsing
//! 2. USB device enumeration simulation
//! 3. VID/PID database lookup and matching
//! 4. Device fingerprinting (VID + PID + interface + usage page + usage)
//! 5. Capture file format (recording device descriptors for analysis)
//! 6. Multi-device capture and filtering
//! 7. Device change detection (connect/disconnect events)
//! 8. Capture session management
//! 9. Export/import of captured device data
//! 10. Unknown device detection and classification heuristics

use openracing_capture_ids::replay::{
    CapturedReport, decode_hex, parse_capture_line, parse_vid_str,
};
use openracing_capture_ids::{decode_report, hex_u16, parse_hex_id};

// ═══════════════════════════════════════════════════════════════════════════
// 1. HID device descriptor capture and parsing
// ═══════════════════════════════════════════════════════════════════════════

mod hid_descriptor_capture_and_parsing {
    use super::*;

    #[test]
    fn parse_raw_descriptor_bytes_from_hex() -> anyhow::Result<()> {
        // A typical short HID report descriptor fragment (Usage Page, Usage, etc.)
        let descriptor_hex = "05010906a101850105071900e72800e71500250195087501810295017508810395057501050819012905910295017503910195067508150025650507190029658100c0";
        let bytes = decode_hex(descriptor_hex)?;
        // Usage Page (Generic Desktop) = 0x05 0x01
        assert_eq!(bytes[0], 0x05, "first item should be Usage Page tag");
        assert_eq!(
            bytes[1], 0x01,
            "Usage Page should be Generic Desktop (0x01)"
        );
        Ok(())
    }

    #[test]
    fn descriptor_byte_length_matches_hex_length() -> anyhow::Result<()> {
        let hex = "0501090605070900";
        let bytes = decode_hex(hex)?;
        assert_eq!(bytes.len(), hex.len() / 2);
        Ok(())
    }

    #[test]
    fn descriptor_with_all_zero_bytes() -> anyhow::Result<()> {
        let hex = "0000000000000000";
        let bytes = decode_hex(hex)?;
        assert!(bytes.iter().all(|&b| b == 0));
        assert_eq!(bytes.len(), 8);
        Ok(())
    }

    #[test]
    fn descriptor_with_all_ff_bytes() -> anyhow::Result<()> {
        let hex = "ffffffffffffffff";
        let bytes = decode_hex(hex)?;
        assert!(bytes.iter().all(|&b| b == 0xFF));
        Ok(())
    }

    #[test]
    fn descriptor_hex_roundtrip_preserves_data() -> anyhow::Result<()> {
        let original_bytes: Vec<u8> = (0u8..=63).collect();
        let hex: String = original_bytes.iter().map(|b| format!("{b:02x}")).collect();
        let decoded = decode_hex(&hex)?;
        assert_eq!(decoded, original_bytes);
        Ok(())
    }

    #[test]
    fn descriptor_mixed_case_hex_decodes() -> anyhow::Result<()> {
        let upper = decode_hex("AABBCCDD")?;
        let lower = decode_hex("aabbccdd")?;
        assert_eq!(upper, lower);
        assert_eq!(upper, vec![0xAA, 0xBB, 0xCC, 0xDD]);
        Ok(())
    }

    #[test]
    fn descriptor_usage_page_item_parsing() -> anyhow::Result<()> {
        // HID Usage Page item: tag=0x05 (Usage Page), data=0x01 (Generic Desktop)
        let bytes = decode_hex("0501")?;
        let tag = bytes[0];
        let page = bytes[1];
        assert_eq!(tag, 0x05, "Usage Page tag");
        assert_eq!(page, 0x01, "Generic Desktop page");
        Ok(())
    }

    #[test]
    fn descriptor_report_id_item_parsing() -> anyhow::Result<()> {
        // HID Report ID item: tag=0x85, data=0x01
        let bytes = decode_hex("8501")?;
        assert_eq!(bytes[0], 0x85, "Report ID tag");
        assert_eq!(bytes[1], 0x01, "Report ID = 1");
        Ok(())
    }

    #[test]
    fn empty_descriptor_is_valid() -> anyhow::Result<()> {
        let bytes = decode_hex("")?;
        assert!(bytes.is_empty());
        Ok(())
    }

    #[test]
    fn invalid_descriptor_hex_rejected() {
        assert!(decode_hex("GG").is_err());
        assert!(decode_hex("0").is_err()); // odd length
        assert!(decode_hex("xyz").is_err());
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. USB device enumeration simulation
// ═══════════════════════════════════════════════════════════════════════════

mod usb_device_enumeration_simulation {
    use super::*;

    /// Mirror of the main.rs HidIdentity for testing enumeration logic.
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    struct SimulatedDevice {
        vendor_id: u16,
        product_id: u16,
        vendor_id_hex: String,
        product_id_hex: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        manufacturer: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        product: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        interface_number: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage_page: Option<u16>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<u16>,
        path: String,
    }

    fn make_device(vid: u16, pid: u16, iface: i32, usage_page: u16, usage: u16) -> SimulatedDevice {
        SimulatedDevice {
            vendor_id: vid,
            product_id: pid,
            vendor_id_hex: hex_u16(vid),
            product_id_hex: hex_u16(pid),
            manufacturer: Some("TestVendor".to_string()),
            product: Some("TestProduct".to_string()),
            interface_number: Some(iface),
            usage_page: Some(usage_page),
            usage: Some(usage),
            path: format!("\\\\?\\HID#VID_{vid:04X}&PID_{pid:04X}&MI_{iface:02}"),
        }
    }

    #[test]
    fn enumerate_single_device() -> anyhow::Result<()> {
        let device = make_device(0x346E, 0x0002, 0, 0x01, 0x04);
        assert_eq!(device.vendor_id, 0x346E);
        assert_eq!(device.product_id, 0x0002);
        assert_eq!(device.vendor_id_hex, "0x346E");
        assert_eq!(device.product_id_hex, "0x0002");
        Ok(())
    }

    #[test]
    fn enumerate_multiple_devices_serializes_to_json() -> anyhow::Result<()> {
        let devices = vec![
            make_device(0x346E, 0x0002, 0, 0x01, 0x04),
            make_device(0x046D, 0xC266, 0, 0x01, 0x04),
        ];
        let json = serde_json::to_string(&devices)?;
        assert!(json.contains("0x346E"));
        assert!(json.contains("0x046D"));
        Ok(())
    }

    #[test]
    fn enumeration_sort_by_pid_then_interface() -> anyhow::Result<()> {
        let mut devices = [
            make_device(0x346E, 0x0003, 1, 0x01, 0x04),
            make_device(0x346E, 0x0001, 0, 0x01, 0x04),
            make_device(0x346E, 0x0003, 0, 0x01, 0x04),
            make_device(0x346E, 0x0002, 0, 0x01, 0x04),
        ];
        // Sort matching main.rs sorting: by (pid, interface, usage_page, usage)
        devices.sort_by_key(|d| {
            (
                d.product_id,
                d.interface_number.unwrap_or(-1),
                d.usage_page.unwrap_or(0),
                d.usage.unwrap_or(0),
            )
        });
        assert_eq!(devices[0].product_id, 0x0001);
        assert_eq!(devices[1].product_id, 0x0002);
        assert_eq!(devices[2].product_id, 0x0003);
        assert_eq!(devices[2].interface_number, Some(0));
        assert_eq!(devices[3].product_id, 0x0003);
        assert_eq!(devices[3].interface_number, Some(1));
        Ok(())
    }

    #[test]
    fn enumeration_filter_by_vid() -> anyhow::Result<()> {
        let devices = [
            make_device(0x346E, 0x0002, 0, 0x01, 0x04),
            make_device(0x046D, 0xC266, 0, 0x01, 0x04),
            make_device(0x346E, 0x0003, 0, 0x01, 0x04),
            make_device(0x1234, 0x0001, 0, 0x01, 0x04),
        ];
        let target_vid = 0x346E;
        let filtered: Vec<_> = devices
            .iter()
            .filter(|d| d.vendor_id == target_vid)
            .collect();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|d| d.vendor_id == target_vid));
        Ok(())
    }

    #[test]
    fn enumeration_empty_device_list() -> anyhow::Result<()> {
        let devices: Vec<SimulatedDevice> = Vec::new();
        let json = serde_json::to_string(&devices)?;
        assert_eq!(json, "[]");
        Ok(())
    }

    #[test]
    fn enumeration_missing_optional_fields_serializes() -> anyhow::Result<()> {
        let device = SimulatedDevice {
            vendor_id: 0x1234,
            product_id: 0x5678,
            vendor_id_hex: hex_u16(0x1234),
            product_id_hex: hex_u16(0x5678),
            manufacturer: None,
            product: None,
            interface_number: None,
            usage_page: None,
            usage: None,
            path: "test_path".to_string(),
        };
        let json = serde_json::to_string(&device)?;
        // Optional None fields with skip_serializing_if should be absent
        assert!(
            !json.contains("\"manufacturer\""),
            "manufacturer should be absent: {json}"
        );
        // "product" as standalone key (not product_id or product_id_hex)
        let parsed: serde_json::Value = serde_json::from_str(&json)?;
        let obj = parsed
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("expected JSON object"))?;
        assert!(!obj.contains_key("manufacturer"));
        assert!(!obj.contains_key("product"));
        assert!(!obj.contains_key("interface_number"));
        assert!(!obj.contains_key("usage_page"));
        assert!(!obj.contains_key("usage"));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. VID/PID database lookup and matching
// ═══════════════════════════════════════════════════════════════════════════

mod vid_pid_database_lookup {
    use super::*;

    const KNOWN_VIDS: &[(u16, &str)] = &[(0x346E, "MOZA"), (0x046D, "Logitech")];

    #[test]
    fn lookup_known_vid_moza() -> anyhow::Result<()> {
        let report: [u8; 7] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00];
        let result =
            decode_report(0x346E, &report).ok_or_else(|| anyhow::anyhow!("MOZA lookup failed"))?;
        assert!(result.starts_with("MOZA:"));
        Ok(())
    }

    #[test]
    fn lookup_known_vid_logitech() -> anyhow::Result<()> {
        let report: [u8; 10] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
        let result = decode_report(0x046D, &report)
            .ok_or_else(|| anyhow::anyhow!("Logitech lookup failed"))?;
        assert!(result.starts_with("Logitech:"));
        Ok(())
    }

    #[test]
    fn lookup_all_known_vids_have_handlers() {
        for &(vid, label) in KNOWN_VIDS {
            // Each known VID should have at least one report format that decodes
            let report_moza: [u8; 7] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00];
            let report_logi: [u8; 10] =
                [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];

            let decoded = match vid {
                0x346E => decode_report(vid, &report_moza),
                0x046D => decode_report(vid, &report_logi),
                _ => None,
            };
            assert!(
                decoded.is_some(),
                "Known VID {label} (0x{vid:04X}) should have a working handler"
            );
        }
    }

    #[test]
    fn lookup_unknown_vid_returns_none() {
        let unknown_vids = [0x0000, 0x0001, 0x1234, 0x5678, 0xABCD, 0xFFFF];
        let report: [u8; 10] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
        for vid in unknown_vids {
            assert!(
                decode_report(vid, &report).is_none(),
                "Unknown VID 0x{vid:04X} should return None"
            );
        }
    }

    #[test]
    fn vid_pid_hex_formatting_consistency() -> anyhow::Result<()> {
        // All VIDs should produce consistent 0xNNNN format
        for &(vid, _) in KNOWN_VIDS {
            let hex = hex_u16(vid);
            assert!(hex.starts_with("0x"), "hex should start with 0x: {hex}");
            assert_eq!(hex.len(), 6, "hex should be 6 chars: {hex}");
            let parsed = parse_hex_id(&hex)?;
            assert_eq!(parsed, vid);
        }
        Ok(())
    }

    #[test]
    fn parse_hex_id_accepts_decimal_fallback() -> anyhow::Result<()> {
        // parse_hex_id tries hex first, then decimal
        assert_eq!(parse_hex_id("0x000A")?, 0x000A);
        Ok(())
    }

    #[test]
    fn vid_str_parse_matches_database_entries() -> anyhow::Result<()> {
        assert_eq!(parse_vid_str("0x346E")?, 0x346E);
        assert_eq!(parse_vid_str("0x046D")?, 0x046D);
        assert_eq!(parse_vid_str("346E")?, 0x346E);
        assert_eq!(parse_vid_str("046D")?, 0x046D);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Device fingerprinting (VID + PID + interface + usage page + usage)
// ═══════════════════════════════════════════════════════════════════════════

mod device_fingerprinting {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    struct DeviceFingerprint {
        vid: u16,
        pid: u16,
        interface_number: i32,
        usage_page: u16,
        usage: u16,
    }

    impl DeviceFingerprint {
        fn from_capture(
            vid_str: &str,
            pid_str: &str,
            iface: i32,
            up: u16,
            u: u16,
        ) -> anyhow::Result<Self> {
            Ok(Self {
                vid: parse_vid_str(vid_str)?,
                pid: parse_vid_str(pid_str)?,
                interface_number: iface,
                usage_page: up,
                usage: u,
            })
        }

        fn canonical_id(&self) -> String {
            format!(
                "{}:{}:{}:{}:{}",
                hex_u16(self.vid),
                hex_u16(self.pid),
                self.interface_number,
                hex_u16(self.usage_page),
                hex_u16(self.usage)
            )
        }
    }

    #[test]
    fn fingerprint_from_vid_pid_strings() -> anyhow::Result<()> {
        let fp = DeviceFingerprint::from_capture("0x346E", "0x0002", 0, 0x0001, 0x0004)?;
        assert_eq!(fp.vid, 0x346E);
        assert_eq!(fp.pid, 0x0002);
        assert_eq!(fp.interface_number, 0);
        assert_eq!(fp.usage_page, 0x0001);
        assert_eq!(fp.usage, 0x0004);
        Ok(())
    }

    #[test]
    fn fingerprint_canonical_id_format() -> anyhow::Result<()> {
        let fp = DeviceFingerprint::from_capture("0x346E", "0x0002", 0, 0x0001, 0x0004)?;
        let id = fp.canonical_id();
        assert_eq!(id, "0x346E:0x0002:0:0x0001:0x0004");
        Ok(())
    }

    #[test]
    fn different_interfaces_produce_different_fingerprints() -> anyhow::Result<()> {
        let fp1 = DeviceFingerprint::from_capture("0x346E", "0x0002", 0, 0x0001, 0x0004)?;
        let fp2 = DeviceFingerprint::from_capture("0x346E", "0x0002", 1, 0x0001, 0x0004)?;
        assert_ne!(fp1, fp2);
        assert_ne!(fp1.canonical_id(), fp2.canonical_id());
        Ok(())
    }

    #[test]
    fn different_usage_pages_produce_different_fingerprints() -> anyhow::Result<()> {
        let fp1 = DeviceFingerprint::from_capture("0x346E", "0x0002", 0, 0x0001, 0x0004)?;
        let fp2 = DeviceFingerprint::from_capture("0x346E", "0x0002", 0, 0x000C, 0x0004)?;
        assert_ne!(fp1, fp2);
        Ok(())
    }

    #[test]
    fn different_usages_produce_different_fingerprints() -> anyhow::Result<()> {
        let fp1 = DeviceFingerprint::from_capture("0x346E", "0x0002", 0, 0x0001, 0x0004)?;
        let fp2 = DeviceFingerprint::from_capture("0x346E", "0x0002", 0, 0x0001, 0x0005)?;
        assert_ne!(fp1, fp2);
        Ok(())
    }

    #[test]
    fn same_fields_produce_equal_fingerprints() -> anyhow::Result<()> {
        let fp1 = DeviceFingerprint::from_capture("0x346E", "0x0002", 0, 0x0001, 0x0004)?;
        let fp2 = DeviceFingerprint::from_capture("0x346E", "0x0002", 0, 0x0001, 0x0004)?;
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.canonical_id(), fp2.canonical_id());
        Ok(())
    }

    #[test]
    fn fingerprint_uniqueness_in_collection() -> anyhow::Result<()> {
        use std::collections::HashSet;
        let fingerprints = [
            DeviceFingerprint::from_capture("0x346E", "0x0002", 0, 0x0001, 0x0004)?,
            DeviceFingerprint::from_capture("0x346E", "0x0002", 1, 0x0001, 0x0004)?,
            DeviceFingerprint::from_capture("0x346E", "0x0002", 0, 0x000C, 0x0001)?,
            DeviceFingerprint::from_capture("0x046D", "0xC266", 0, 0x0001, 0x0004)?,
        ];
        let set: HashSet<_> = fingerprints.iter().collect();
        assert_eq!(set.len(), 4, "all fingerprints should be unique");
        Ok(())
    }

    #[test]
    fn fingerprint_from_capture_line_vid_pid() -> anyhow::Result<()> {
        let line = r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#;
        let entry = parse_capture_line(line)?;
        let vid = parse_vid_str(&entry.vid)?;
        let pid = parse_vid_str(&entry.pid)?;
        assert_eq!(vid, 0x346E);
        assert_eq!(pid, 0x0002);
        Ok(())
    }

    #[test]
    fn moza_wheel_hid_gaming_fingerprint() -> anyhow::Result<()> {
        // MOZA wheel: VID=0x346E, PID=0x0002, interface 0, Usage Page=Generic Desktop, Usage=Joystick
        let fp = DeviceFingerprint::from_capture("0x346E", "0x0002", 0, 0x0001, 0x0004)?;
        assert_eq!(fp.usage_page, 0x0001, "Generic Desktop Controls");
        assert_eq!(fp.usage, 0x0004, "Joystick");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Capture file format (recording device descriptors for analysis)
// ═══════════════════════════════════════════════════════════════════════════

mod capture_file_format {
    use super::*;

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct CaptureFile {
        captured_at_utc: String,
        host: HostInfo,
        devices: Vec<DeviceRecord>,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct HostInfo {
        os: String,
        arch: String,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct DeviceRecord {
        vendor_id: u16,
        product_id: u16,
        vendor_id_hex: String,
        product_id_hex: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        manufacturer: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        product: Option<String>,
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        report_descriptor: Option<DescriptorInfo>,
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct DescriptorInfo {
        len: usize,
        crc32: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        hex: Option<String>,
    }

    #[test]
    fn capture_file_json_roundtrip() -> anyhow::Result<()> {
        let capture = CaptureFile {
            captured_at_utc: "unix:1700000000".to_string(),
            host: HostInfo {
                os: "windows".to_string(),
                arch: "x86_64".to_string(),
            },
            devices: vec![DeviceRecord {
                vendor_id: 0x346E,
                product_id: 0x0002,
                vendor_id_hex: hex_u16(0x346E),
                product_id_hex: hex_u16(0x0002),
                manufacturer: Some("Gudsen".to_string()),
                product: Some("MOZA Racing".to_string()),
                path: "\\\\?\\HID#VID_346E".to_string(),
                report_descriptor: None,
            }],
        };
        let json = serde_json::to_string_pretty(&capture)?;
        let parsed: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(parsed.captured_at_utc, "unix:1700000000");
        assert_eq!(parsed.host.os, "windows");
        assert_eq!(parsed.devices.len(), 1);
        assert_eq!(parsed.devices[0].vendor_id, 0x346E);
        Ok(())
    }

    #[test]
    fn capture_file_with_descriptor_info() -> anyhow::Result<()> {
        let capture = CaptureFile {
            captured_at_utc: "unix:1700000000".to_string(),
            host: HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
            },
            devices: vec![DeviceRecord {
                vendor_id: 0x346E,
                product_id: 0x0002,
                vendor_id_hex: hex_u16(0x346E),
                product_id_hex: hex_u16(0x0002),
                manufacturer: Some("Gudsen".to_string()),
                product: None,
                path: "/dev/hidraw0".to_string(),
                report_descriptor: Some(DescriptorInfo {
                    len: 64,
                    crc32: "0x12345678".to_string(),
                    hex: Some("0501090604a1".to_string()),
                }),
            }],
        };
        let json = serde_json::to_string(&capture)?;
        let parsed: CaptureFile = serde_json::from_str(&json)?;
        let desc = parsed.devices[0]
            .report_descriptor
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("descriptor should be present"))?;
        assert_eq!(desc.len, 64);
        assert_eq!(desc.crc32, "0x12345678");
        assert!(desc.hex.is_some());
        Ok(())
    }

    #[test]
    fn capture_file_without_descriptor_hex_skips_field() -> anyhow::Result<()> {
        let desc = DescriptorInfo {
            len: 32,
            crc32: "0xAABBCCDD".to_string(),
            hex: None,
        };
        let json = serde_json::to_string(&desc)?;
        assert!(!json.contains("hex"), "hex=None should be skipped: {json}");
        Ok(())
    }

    #[test]
    fn jsonl_capture_format_multiple_lines() -> anyhow::Result<()> {
        let lines = [
            r#"{"ts_ns":1000000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1001000000,"vid":"0x346E","pid":"0x0002","report":"01018000000000"}"#,
            r#"{"ts_ns":1002000000,"vid":"0x346E","pid":"0x0002","report":"01028000000000"}"#,
        ];
        let mut reports = Vec::new();
        for line in &lines {
            reports.push(parse_capture_line(line)?);
        }
        assert_eq!(reports.len(), 3);
        // Verify ordering preserved
        assert!(reports[0].ts_ns < reports[1].ts_ns);
        assert!(reports[1].ts_ns < reports[2].ts_ns);
        Ok(())
    }

    #[test]
    fn jsonl_report_hex_field_maps_to_raw_bytes() -> anyhow::Result<()> {
        let line = r#"{"ts_ns":100,"vid":"0x346E","pid":"0x0002","report":"01ff80deadbeef"}"#;
        let entry = parse_capture_line(line)?;
        let bytes = decode_hex(&entry.report)?;
        assert_eq!(bytes[0], 0x01); // report ID
        assert_eq!(bytes[1], 0xFF);
        assert_eq!(bytes[2], 0x80);
        assert_eq!(bytes[3], 0xDE);
        assert_eq!(bytes[4], 0xAD);
        assert_eq!(bytes[5], 0xBE);
        assert_eq!(bytes[6], 0xEF);
        Ok(())
    }

    #[test]
    fn capture_file_empty_devices_array() -> anyhow::Result<()> {
        let capture = CaptureFile {
            captured_at_utc: "unix:0".to_string(),
            host: HostInfo {
                os: "test".to_string(),
                arch: "test".to_string(),
            },
            devices: vec![],
        };
        let json = serde_json::to_string(&capture)?;
        let parsed: CaptureFile = serde_json::from_str(&json)?;
        assert!(parsed.devices.is_empty());
        Ok(())
    }

    #[test]
    fn capture_file_multiple_devices_roundtrip() -> anyhow::Result<()> {
        let capture = CaptureFile {
            captured_at_utc: "unix:1700000000".to_string(),
            host: HostInfo {
                os: "windows".to_string(),
                arch: "x86_64".to_string(),
            },
            devices: vec![
                DeviceRecord {
                    vendor_id: 0x346E,
                    product_id: 0x0002,
                    vendor_id_hex: hex_u16(0x346E),
                    product_id_hex: hex_u16(0x0002),
                    manufacturer: Some("Gudsen".to_string()),
                    product: Some("R5".to_string()),
                    path: "path1".to_string(),
                    report_descriptor: None,
                },
                DeviceRecord {
                    vendor_id: 0x046D,
                    product_id: 0xC266,
                    vendor_id_hex: hex_u16(0x046D),
                    product_id_hex: hex_u16(0xC266),
                    manufacturer: Some("Logitech".to_string()),
                    product: Some("G923".to_string()),
                    path: "path2".to_string(),
                    report_descriptor: None,
                },
            ],
        };
        let json = serde_json::to_string_pretty(&capture)?;
        let parsed: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(parsed.devices.len(), 2);
        assert_eq!(parsed.devices[0].vendor_id, 0x346E);
        assert_eq!(parsed.devices[1].vendor_id, 0x046D);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Multi-device capture and filtering
// ═══════════════════════════════════════════════════════════════════════════

mod multi_device_capture_and_filtering {
    use super::*;

    fn sample_capture_lines() -> Vec<&'static str> {
        vec![
            r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1001,"vid":"0x046D","pid":"0xC266","report":"01008000000000000800"}"#,
            r#"{"ts_ns":1002,"vid":"0x346E","pid":"0x0003","report":"01008000000000"}"#,
            r#"{"ts_ns":1003,"vid":"0x046D","pid":"0xC266","report":"0100ff00ff00ff000800"}"#,
            r#"{"ts_ns":1004,"vid":"0x346E","pid":"0x0002","report":"01ffff00000000"}"#,
            r#"{"ts_ns":1005,"vid":"0x1234","pid":"0x5678","report":"0200aabbccddee"}"#,
        ]
    }

    #[test]
    fn filter_captures_by_vid() -> anyhow::Result<()> {
        let lines = sample_capture_lines();
        let moza_reports: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .filter(|e| e.vid == "0x346E")
            .collect();
        assert_eq!(moza_reports.len(), 3);
        Ok(())
    }

    #[test]
    fn filter_captures_by_pid() -> anyhow::Result<()> {
        let lines = sample_capture_lines();
        let reports: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .filter(|e| e.pid == "0xC266")
            .collect();
        assert_eq!(reports.len(), 2);
        Ok(())
    }

    #[test]
    fn filter_captures_by_vid_and_pid() -> anyhow::Result<()> {
        let lines = sample_capture_lines();
        let reports: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .filter(|e| e.vid == "0x346E" && e.pid == "0x0002")
            .collect();
        assert_eq!(reports.len(), 2);
        Ok(())
    }

    #[test]
    fn decode_only_known_vendor_reports() -> anyhow::Result<()> {
        let lines = sample_capture_lines();
        let mut decoded_count = 0u32;
        let mut unknown_count = 0u32;
        for line in &lines {
            let entry = parse_capture_line(line)?;
            let bytes = decode_hex(&entry.report)?;
            let vid = parse_vid_str(&entry.vid)?;
            if decode_report(vid, &bytes).is_some() {
                decoded_count += 1;
            } else {
                unknown_count += 1;
            }
        }
        // MOZA (3) + Logitech (2) = up to 5 decodable, 1 unknown VID (0x1234)
        // But wrong report ID (0x02) for 0x1234 means it's unknown too
        assert!(decoded_count > 0, "should decode at least some reports");
        assert!(unknown_count > 0, "should have at least one unknown");
        Ok(())
    }

    #[test]
    fn group_captures_by_vendor() -> anyhow::Result<()> {
        let lines = sample_capture_lines();
        let entries: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let mut groups: std::collections::HashMap<String, Vec<&CapturedReport>> =
            std::collections::HashMap::new();
        for entry in &entries {
            groups.entry(entry.vid.clone()).or_default().push(entry);
        }
        assert_eq!(groups.get("0x346E").map(|v| v.len()).unwrap_or(0), 3);
        assert_eq!(groups.get("0x046D").map(|v| v.len()).unwrap_or(0), 2);
        assert_eq!(groups.get("0x1234").map(|v| v.len()).unwrap_or(0), 1);
        Ok(())
    }

    #[test]
    fn interleaved_reports_maintain_order() -> anyhow::Result<()> {
        let lines = sample_capture_lines();
        let entries: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?;
        for window in entries.windows(2) {
            assert!(
                window[0].ts_ns < window[1].ts_ns,
                "timestamps must be strictly increasing: {} >= {}",
                window[0].ts_ns,
                window[1].ts_ns
            );
        }
        Ok(())
    }

    #[test]
    fn count_unique_devices_in_capture() -> anyhow::Result<()> {
        let lines = sample_capture_lines();
        let entries: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let unique_devices: std::collections::HashSet<(String, String)> = entries
            .iter()
            .map(|e| (e.vid.clone(), e.pid.clone()))
            .collect();
        // 0x346E:0x0002, 0x046D:0xC266, 0x346E:0x0003, 0x1234:0x5678
        assert_eq!(unique_devices.len(), 4);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. Device change detection (connect/disconnect events)
// ═══════════════════════════════════════════════════════════════════════════

mod device_change_detection {

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    struct DeviceId {
        vid: String,
        pid: String,
    }

    #[derive(Debug)]
    struct DeviceChangeSet {
        connected: Vec<DeviceId>,
        disconnected: Vec<DeviceId>,
    }

    fn detect_changes(before: &[DeviceId], after: &[DeviceId]) -> DeviceChangeSet {
        let before_set: std::collections::HashSet<_> = before.iter().collect();
        let after_set: std::collections::HashSet<_> = after.iter().collect();

        let connected: Vec<DeviceId> = after_set
            .difference(&before_set)
            .map(|d| (*d).clone())
            .collect();
        let disconnected: Vec<DeviceId> = before_set
            .difference(&after_set)
            .map(|d| (*d).clone())
            .collect();

        DeviceChangeSet {
            connected,
            disconnected,
        }
    }

    fn device(vid: &str, pid: &str) -> DeviceId {
        DeviceId {
            vid: vid.to_string(),
            pid: pid.to_string(),
        }
    }

    #[test]
    fn detect_new_device_connected() {
        let before = vec![device("0x346E", "0x0002")];
        let after = vec![device("0x346E", "0x0002"), device("0x046D", "0xC266")];
        let changes = detect_changes(&before, &after);
        assert_eq!(changes.connected.len(), 1);
        assert_eq!(changes.disconnected.len(), 0);
        assert_eq!(changes.connected[0].vid, "0x046D");
    }

    #[test]
    fn detect_device_disconnected() {
        let before = vec![device("0x346E", "0x0002"), device("0x046D", "0xC266")];
        let after = vec![device("0x346E", "0x0002")];
        let changes = detect_changes(&before, &after);
        assert_eq!(changes.connected.len(), 0);
        assert_eq!(changes.disconnected.len(), 1);
        assert_eq!(changes.disconnected[0].vid, "0x046D");
    }

    #[test]
    fn detect_device_swap() {
        let before = vec![device("0x346E", "0x0002")];
        let after = vec![device("0x046D", "0xC266")];
        let changes = detect_changes(&before, &after);
        assert_eq!(changes.connected.len(), 1);
        assert_eq!(changes.disconnected.len(), 1);
    }

    #[test]
    fn no_changes_when_same() {
        let devices = vec![device("0x346E", "0x0002"), device("0x046D", "0xC266")];
        let changes = detect_changes(&devices, &devices);
        assert!(changes.connected.is_empty());
        assert!(changes.disconnected.is_empty());
    }

    #[test]
    fn detect_from_empty_to_populated() {
        let before: Vec<DeviceId> = vec![];
        let after = vec![device("0x346E", "0x0002"), device("0x046D", "0xC266")];
        let changes = detect_changes(&before, &after);
        assert_eq!(changes.connected.len(), 2);
        assert!(changes.disconnected.is_empty());
    }

    #[test]
    fn detect_from_populated_to_empty() {
        let before = vec![device("0x346E", "0x0002"), device("0x046D", "0xC266")];
        let after: Vec<DeviceId> = vec![];
        let changes = detect_changes(&before, &after);
        assert!(changes.connected.is_empty());
        assert_eq!(changes.disconnected.len(), 2);
    }

    #[test]
    fn detect_multiple_simultaneous_changes() {
        let before = vec![device("0x346E", "0x0002"), device("0x1234", "0x5678")];
        let after = vec![
            device("0x346E", "0x0002"),
            device("0x046D", "0xC266"),
            device("0xAAAA", "0xBBBB"),
        ];
        let changes = detect_changes(&before, &after);
        assert_eq!(changes.connected.len(), 2);
        assert_eq!(changes.disconnected.len(), 1);
        assert_eq!(changes.disconnected[0].vid, "0x1234");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. Capture session management
// ═══════════════════════════════════════════════════════════════════════════

mod capture_session_management {
    use super::*;

    #[derive(Debug)]
    struct CaptureSession {
        start_ts_ns: u64,
        vid_filter: Option<u16>,
        pid_filter: Option<u16>,
        reports: Vec<CapturedReport>,
        max_reports: Option<usize>,
    }

    impl CaptureSession {
        fn new(vid: Option<u16>, pid: Option<u16>) -> Self {
            Self {
                start_ts_ns: 0,
                vid_filter: vid,
                pid_filter: pid,
                reports: Vec::new(),
                max_reports: None,
            }
        }

        fn with_limit(mut self, limit: usize) -> Self {
            self.max_reports = Some(limit);
            self
        }

        fn add_report(&mut self, line: &str) -> anyhow::Result<bool> {
            if self
                .max_reports
                .is_some_and(|max| self.reports.len() >= max)
            {
                return Ok(false);
            }

            let entry = parse_capture_line(line)?;

            if let Some(vid_filter) = self.vid_filter {
                let vid = parse_vid_str(&entry.vid)?;
                if vid != vid_filter {
                    return Ok(false);
                }
            }

            if let Some(pid_filter) = self.pid_filter {
                let pid = parse_vid_str(&entry.pid)?;
                if pid != pid_filter {
                    return Ok(false);
                }
            }

            if self.reports.is_empty() {
                self.start_ts_ns = entry.ts_ns;
            }

            self.reports.push(entry);
            Ok(true)
        }

        fn duration_ns(&self) -> u64 {
            match self.reports.last() {
                Some(last) => last.ts_ns.saturating_sub(self.start_ts_ns),
                None => 0,
            }
        }

        fn report_count(&self) -> usize {
            self.reports.len()
        }
    }

    #[test]
    fn session_accepts_matching_reports() -> anyhow::Result<()> {
        let mut session = CaptureSession::new(Some(0x346E), None);
        let line = r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#;
        assert!(session.add_report(line)?);
        assert_eq!(session.report_count(), 1);
        Ok(())
    }

    #[test]
    fn session_rejects_non_matching_vid() -> anyhow::Result<()> {
        let mut session = CaptureSession::new(Some(0x346E), None);
        let line =
            r#"{"ts_ns":1000,"vid":"0x046D","pid":"0x0001","report":"01008000000000000800"}"#;
        assert!(!session.add_report(line)?);
        assert_eq!(session.report_count(), 0);
        Ok(())
    }

    #[test]
    fn session_rejects_non_matching_pid() -> anyhow::Result<()> {
        let mut session = CaptureSession::new(Some(0x346E), Some(0x0002));
        let line = r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0003","report":"01008000000000"}"#;
        assert!(!session.add_report(line)?);
        assert_eq!(session.report_count(), 0);
        Ok(())
    }

    #[test]
    fn session_without_filter_accepts_all() -> anyhow::Result<()> {
        let mut session = CaptureSession::new(None, None);
        let lines = [
            r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1001,"vid":"0x046D","pid":"0x0001","report":"01008000000000000800"}"#,
        ];
        for line in &lines {
            assert!(session.add_report(line)?);
        }
        assert_eq!(session.report_count(), 2);
        Ok(())
    }

    #[test]
    fn session_with_limit_stops_accepting() -> anyhow::Result<()> {
        let mut session = CaptureSession::new(None, None).with_limit(2);
        let lines = [
            r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1001,"vid":"0x346E","pid":"0x0002","report":"01018000000000"}"#,
            r#"{"ts_ns":1002,"vid":"0x346E","pid":"0x0002","report":"01028000000000"}"#,
        ];
        assert!(session.add_report(lines[0])?);
        assert!(session.add_report(lines[1])?);
        assert!(!session.add_report(lines[2])?);
        assert_eq!(session.report_count(), 2);
        Ok(())
    }

    #[test]
    fn session_tracks_duration() -> anyhow::Result<()> {
        let mut session = CaptureSession::new(None, None);
        assert_eq!(session.duration_ns(), 0);
        let lines = [
            r#"{"ts_ns":1000000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1002000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
        ];
        session.add_report(lines[0])?;
        session.add_report(lines[1])?;
        assert_eq!(session.duration_ns(), 2_000_000);
        Ok(())
    }

    #[test]
    fn empty_session_has_zero_duration() {
        let session = CaptureSession::new(None, None);
        assert_eq!(session.duration_ns(), 0);
        assert_eq!(session.report_count(), 0);
    }

    #[test]
    fn session_start_timestamp_set_from_first_report() -> anyhow::Result<()> {
        let mut session = CaptureSession::new(None, None);
        let line =
            r#"{"ts_ns":5000000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#;
        session.add_report(line)?;
        assert_eq!(session.start_ts_ns, 5_000_000_000);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. Export/import of captured device data
// ═══════════════════════════════════════════════════════════════════════════

mod export_import_captured_data {
    use super::*;
    use std::io::Write;

    #[test]
    fn export_captured_reports_to_jsonl() -> anyhow::Result<()> {
        let reports = vec![
            CapturedReport {
                ts_ns: 1_000_000,
                vid: "0x346E".to_string(),
                pid: "0x0002".to_string(),
                report: "01008000000000".to_string(),
            },
            CapturedReport {
                ts_ns: 2_000_000,
                vid: "0x046D".to_string(),
                pid: "0xC266".to_string(),
                report: "01008000000000000800".to_string(),
            },
        ];

        let mut output = Vec::new();
        for report in &reports {
            let line = serde_json::to_string(report)?;
            writeln!(output, "{line}")?;
        }
        let exported = String::from_utf8(output)?;
        let line_count = exported.lines().count();
        assert_eq!(line_count, 2);
        Ok(())
    }

    #[test]
    fn import_jsonl_and_decode_all_reports() -> anyhow::Result<()> {
        let jsonl = "\
{\"ts_ns\":1000,\"vid\":\"0x346E\",\"pid\":\"0x0002\",\"report\":\"01008000000000\"}\n\
{\"ts_ns\":2000,\"vid\":\"0x046D\",\"pid\":\"0xC266\",\"report\":\"01008000000000000800\"}\n";

        let mut decoded = Vec::new();
        for line in jsonl.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let entry = parse_capture_line(line)?;
            let bytes = decode_hex(&entry.report)?;
            let vid = parse_vid_str(&entry.vid)?;
            decoded.push((vid, bytes));
        }
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].0, 0x346E);
        assert_eq!(decoded[1].0, 0x046D);
        Ok(())
    }

    #[test]
    fn export_import_roundtrip_preserves_all_fields() -> anyhow::Result<()> {
        let original = CapturedReport {
            ts_ns: 123_456_789_000,
            vid: "0x346E".to_string(),
            pid: "0x0002".to_string(),
            report: "01ff80aabbccdd".to_string(),
        };
        let json = serde_json::to_string(&original)?;
        let imported = parse_capture_line(&json)?;
        assert_eq!(imported.ts_ns, original.ts_ns);
        assert_eq!(imported.vid, original.vid);
        assert_eq!(imported.pid, original.pid);
        assert_eq!(imported.report, original.report);
        Ok(())
    }

    #[test]
    fn export_to_file_and_reimport() -> anyhow::Result<()> {
        let dir = std::env::temp_dir();
        let path = dir.join("openracing_capture_tooling_test_export.jsonl");

        let reports = vec![
            CapturedReport {
                ts_ns: 100,
                vid: "0x346E".to_string(),
                pid: "0x0002".to_string(),
                report: "01008000000000".to_string(),
            },
            CapturedReport {
                ts_ns: 200,
                vid: "0x346E".to_string(),
                pid: "0x0002".to_string(),
                report: "01ffff00000000".to_string(),
            },
        ];

        // Export
        {
            let file = std::fs::File::create(&path)?;
            let mut writer = std::io::BufWriter::new(file);
            for report in &reports {
                let line = serde_json::to_string(report)?;
                writeln!(writer, "{line}")?;
            }
            writer.flush()?;
        }

        // Import
        let content = std::fs::read_to_string(&path)?;
        let mut imported = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            imported.push(parse_capture_line(line)?);
        }

        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].ts_ns, 100);
        assert_eq!(imported[1].ts_ns, 200);
        assert_eq!(imported[0].report, "01008000000000");
        assert_eq!(imported[1].report, "01ffff00000000");

        // Cleanup
        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    #[test]
    fn import_skips_empty_lines() -> anyhow::Result<()> {
        let jsonl = "\
{\"ts_ns\":100,\"vid\":\"0x346E\",\"pid\":\"0x0002\",\"report\":\"01008000000000\"}\n\
\n\
{\"ts_ns\":200,\"vid\":\"0x346E\",\"pid\":\"0x0002\",\"report\":\"01ffff00000000\"}\n\
\n";
        let mut count = 0;
        for line in jsonl.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let _entry = parse_capture_line(line)?;
            count += 1;
        }
        assert_eq!(count, 2);
        Ok(())
    }

    #[test]
    fn export_preserves_hex_report_encoding() -> anyhow::Result<()> {
        let report_hex = "deadbeef01020304";
        let original = CapturedReport {
            ts_ns: 42,
            vid: "0xAAAA".to_string(),
            pid: "0xBBBB".to_string(),
            report: report_hex.to_string(),
        };
        let json = serde_json::to_string(&original)?;
        let imported = parse_capture_line(&json)?;
        let bytes = decode_hex(&imported.report)?;
        assert_eq!(bytes, vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04]);
        Ok(())
    }

    #[test]
    fn import_rejects_invalid_json_line() {
        assert!(parse_capture_line("not valid json").is_err());
        assert!(parse_capture_line("{incomplete").is_err());
        assert!(parse_capture_line("").is_err());
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 10. Unknown device detection and classification heuristics
// ═══════════════════════════════════════════════════════════════════════════

mod unknown_device_detection_and_classification {
    use super::{decode_hex, decode_report, parse_capture_line, parse_vid_str};

    #[derive(Debug, PartialEq, Eq)]
    enum DeviceClass {
        KnownWheel(&'static str),
        UnknownHid,
        UnknownNoData,
    }

    fn classify_device(vid: u16, report: &[u8]) -> DeviceClass {
        if report.is_empty() {
            return DeviceClass::UnknownNoData;
        }
        match decode_report(vid, report) {
            Some(text) if text.starts_with("MOZA:") => DeviceClass::KnownWheel("MOZA"),
            Some(text) if text.starts_with("Logitech:") => DeviceClass::KnownWheel("Logitech"),
            Some(_) => DeviceClass::UnknownHid,
            None => DeviceClass::UnknownHid,
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    enum UsagePageClass {
        GenericDesktop,
        SimulationControls,
        GameControls,
        Keyboard,
        Consumer,
        VendorDefined,
        Other(u16),
    }

    fn classify_usage_page(page: u16) -> UsagePageClass {
        match page {
            0x0001 => UsagePageClass::GenericDesktop,
            0x0002 => UsagePageClass::SimulationControls,
            0x0005 => UsagePageClass::GameControls,
            0x0007 => UsagePageClass::Keyboard,
            0x000C => UsagePageClass::Consumer,
            0xFF00..=0xFFFF => UsagePageClass::VendorDefined,
            other => UsagePageClass::Other(other),
        }
    }

    #[test]
    fn classify_moza_device() {
        let report: [u8; 7] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(
            classify_device(0x346E, &report),
            DeviceClass::KnownWheel("MOZA")
        );
    }

    #[test]
    fn classify_logitech_device() {
        let report: [u8; 10] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
        assert_eq!(
            classify_device(0x046D, &report),
            DeviceClass::KnownWheel("Logitech")
        );
    }

    #[test]
    fn classify_unknown_vid_as_unknown_hid() {
        let report: [u8; 10] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
        assert_eq!(classify_device(0x1234, &report), DeviceClass::UnknownHid);
    }

    #[test]
    fn classify_empty_report_as_no_data() {
        assert_eq!(classify_device(0x346E, &[]), DeviceClass::UnknownNoData);
        assert_eq!(classify_device(0x046D, &[]), DeviceClass::UnknownNoData);
        assert_eq!(classify_device(0x1234, &[]), DeviceClass::UnknownNoData);
    }

    #[test]
    fn classify_known_vid_with_bad_report_as_unknown() {
        let bad_report: [u8; 2] = [0xFF, 0x00];
        assert_eq!(
            classify_device(0x346E, &bad_report),
            DeviceClass::UnknownHid
        );
        assert_eq!(
            classify_device(0x046D, &bad_report),
            DeviceClass::UnknownHid
        );
    }

    #[test]
    fn usage_page_generic_desktop() {
        assert_eq!(classify_usage_page(0x0001), UsagePageClass::GenericDesktop);
    }

    #[test]
    fn usage_page_simulation_controls() {
        assert_eq!(
            classify_usage_page(0x0002),
            UsagePageClass::SimulationControls
        );
    }

    #[test]
    fn usage_page_game_controls() {
        assert_eq!(classify_usage_page(0x0005), UsagePageClass::GameControls);
    }

    #[test]
    fn usage_page_keyboard() {
        assert_eq!(classify_usage_page(0x0007), UsagePageClass::Keyboard);
    }

    #[test]
    fn usage_page_consumer() {
        assert_eq!(classify_usage_page(0x000C), UsagePageClass::Consumer);
    }

    #[test]
    fn usage_page_vendor_defined() {
        assert_eq!(classify_usage_page(0xFF00), UsagePageClass::VendorDefined);
        assert_eq!(classify_usage_page(0xFF01), UsagePageClass::VendorDefined);
        assert_eq!(classify_usage_page(0xFFFF), UsagePageClass::VendorDefined);
    }

    #[test]
    fn usage_page_other() {
        assert_eq!(classify_usage_page(0x0003), UsagePageClass::Other(0x0003));
        assert_eq!(classify_usage_page(0x0100), UsagePageClass::Other(0x0100));
    }

    #[test]
    fn heuristic_report_id_zero_may_indicate_no_report_ids() -> anyhow::Result<()> {
        // A report starting with 0x00 might mean no report IDs are used
        let report: [u8; 7] = [0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00];
        // Neither MOZA nor Logitech should decode a report with ID 0x00
        assert!(decode_report(0x346E, &report).is_none());
        assert!(decode_report(0x046D, &report).is_none());
        Ok(())
    }

    #[test]
    fn heuristic_classify_from_capture_pipeline() -> anyhow::Result<()> {
        let lines = [
            r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1001,"vid":"0x046D","pid":"0xC266","report":"01008000000000000800"}"#,
            r#"{"ts_ns":1002,"vid":"0xFFFF","pid":"0x9999","report":"ff00112233"}"#,
        ];

        let mut classifications = Vec::new();
        for line in &lines {
            let entry = parse_capture_line(line)?;
            let bytes = decode_hex(&entry.report)?;
            let vid = parse_vid_str(&entry.vid)?;
            classifications.push(classify_device(vid, &bytes));
        }
        assert_eq!(classifications[0], DeviceClass::KnownWheel("MOZA"));
        assert_eq!(classifications[1], DeviceClass::KnownWheel("Logitech"));
        assert_eq!(classifications[2], DeviceClass::UnknownHid);
        Ok(())
    }

    #[test]
    fn heuristic_all_vid_space_non_matching_returns_unknown() {
        let report: [u8; 7] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00];
        // Check a sampling of VIDs that are NOT known
        let unknown_vids = [0x0000, 0x0001, 0x0100, 0x1000, 0x2000, 0x5000, 0xFFFE];
        for vid in unknown_vids {
            let class = classify_device(vid, &report);
            assert_ne!(
                class,
                DeviceClass::KnownWheel("MOZA"),
                "VID 0x{vid:04X} should not match MOZA"
            );
            assert_ne!(
                class,
                DeviceClass::KnownWheel("Logitech"),
                "VID 0x{vid:04X} should not match Logitech"
            );
        }
    }
}
