//! macOS IOKit HID adapter — data structures and platform-agnostic logic
//!
//! This module defines the macOS-specific HID device abstractions using IOKit
//! concepts: device matching dictionaries, HID element descriptors, report
//! parsing, and capability detection for racing wheel peripherals.
//!
//! All data structures and parsing logic compile on every platform. Only actual
//! IOKit FFI calls are gated behind `#[cfg(target_os = "macos")]`.

use super::HidDeviceInfo;
use racing_wheel_schemas::prelude::*;
use std::fmt;

// ---------------------------------------------------------------------------
// IOKit HID usage pages and usages (from IOHIDUsageTables.h)
// ---------------------------------------------------------------------------

/// IOKit HID usage page constants.
pub mod usage_page {
    /// Generic Desktop usage page — contains joystick, game pad, wheel, etc.
    pub const GENERIC_DESKTOP: u32 = 0x01;
    /// Simulation Controls usage page — racing / flight sim axes.
    pub const SIMULATION: u32 = 0x02;
    /// Physical Interface Device (PID) — force feedback.
    pub const PID: u32 = 0x0F;
    /// Vendor-defined usage page base.
    pub const VENDOR_DEFINED_START: u32 = 0xFF00;
}

/// IOKit HID usage constants within Generic Desktop page.
pub mod usage {
    pub const JOYSTICK: u32 = 0x04;
    pub const GAME_PAD: u32 = 0x05;
    pub const MULTI_AXIS_CONTROLLER: u32 = 0x08;
    pub const WHEEL: u32 = 0x38;
    pub const X: u32 = 0x30;
    pub const Y: u32 = 0x31;
    pub const Z: u32 = 0x32;
    pub const RX: u32 = 0x33;
    pub const RY: u32 = 0x34;
    pub const RZ: u32 = 0x35;
    pub const HAT_SWITCH: u32 = 0x39;
}

// ---------------------------------------------------------------------------
// IOKit matching dictionary builder (platform-agnostic representation)
// ---------------------------------------------------------------------------

/// Key-value pairs that model an IOKit matching dictionary.
///
/// On macOS these feed into `IOServiceMatching` / `IOHIDManager` device
/// matching. On other platforms the struct is used for unit-testing the
/// matching logic without any FFI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IOKitMatchingDict {
    pub entries: Vec<(String, MatchValue)>,
}

/// Typed values that can appear in an IOKit matching dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchValue {
    Integer(u64),
    String(String),
    Boolean(bool),
}

impl IOKitMatchingDict {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn with_usage_page(mut self, page: u32) -> Self {
        self.entries.push((
            "DeviceUsagePage".to_string(),
            MatchValue::Integer(u64::from(page)),
        ));
        self
    }

    pub fn with_usage(mut self, usage: u32) -> Self {
        self.entries.push((
            "DeviceUsage".to_string(),
            MatchValue::Integer(u64::from(usage)),
        ));
        self
    }

    pub fn with_vendor_id(mut self, vid: u16) -> Self {
        self.entries
            .push(("VendorID".to_string(), MatchValue::Integer(u64::from(vid))));
        self
    }

    pub fn with_product_id(mut self, pid: u16) -> Self {
        self.entries
            .push(("ProductID".to_string(), MatchValue::Integer(u64::from(pid))));
        self
    }

    pub fn with_transport(mut self, transport: &str) -> Self {
        self.entries.push((
            "Transport".to_string(),
            MatchValue::String(transport.to_string()),
        ));
        self
    }

    /// Look up an integer entry by key.
    pub fn get_integer(&self, key: &str) -> Option<u64> {
        self.entries.iter().find_map(|(k, v)| {
            if k == key
                && let MatchValue::Integer(n) = v
            {
                return Some(*n);
            }
            None
        })
    }

    /// Check whether a device (vid, pid, usage_page, usage) satisfies this dict.
    pub fn matches_device(
        &self,
        vid: u16,
        pid: u16,
        device_usage_page: u32,
        device_usage: u32,
    ) -> bool {
        for (key, value) in &self.entries {
            let ok = match (key.as_str(), value) {
                ("VendorID", MatchValue::Integer(v)) => u64::from(vid) == *v,
                ("ProductID", MatchValue::Integer(v)) => u64::from(pid) == *v,
                ("DeviceUsagePage", MatchValue::Integer(v)) => u64::from(device_usage_page) == *v,
                ("DeviceUsage", MatchValue::Integer(v)) => u64::from(device_usage) == *v,
                // Transport and other string keys are not checked against
                // numeric device properties — they match unconditionally here.
                _ => true,
            };
            if !ok {
                return false;
            }
        }
        true
    }
}

impl Default for IOKitMatchingDict {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// HID element descriptor
// ---------------------------------------------------------------------------

/// IOKit HID element type (mirrors IOHIDElementType).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum IOHIDElementType {
    InputMisc = 1,
    InputButton = 2,
    InputAxis = 3,
    Output = 129,
    Feature = 257,
    Collection = 513,
}

impl IOHIDElementType {
    /// Parse from the raw `IOHIDElementType` integer returned by IOKit.
    pub fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            1 => Some(Self::InputMisc),
            2 => Some(Self::InputButton),
            3 => Some(Self::InputAxis),
            129 => Some(Self::Output),
            257 => Some(Self::Feature),
            513 => Some(Self::Collection),
            _ => None,
        }
    }

    pub fn is_input(self) -> bool {
        matches!(self, Self::InputMisc | Self::InputButton | Self::InputAxis)
    }

    pub fn is_output(self) -> bool {
        matches!(self, Self::Output)
    }
}

impl fmt::Display for IOHIDElementType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputMisc => write!(f, "Input (Misc)"),
            Self::InputButton => write!(f, "Input (Button)"),
            Self::InputAxis => write!(f, "Input (Axis)"),
            Self::Output => write!(f, "Output"),
            Self::Feature => write!(f, "Feature"),
            Self::Collection => write!(f, "Collection"),
        }
    }
}

/// Platform-agnostic representation of a single IOKit HID element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HIDElement {
    pub element_type: IOHIDElementType,
    pub usage_page: u32,
    pub usage: u32,
    pub logical_min: i64,
    pub logical_max: i64,
    pub physical_min: i64,
    pub physical_max: i64,
    pub report_size: u32,
    pub report_count: u32,
    pub report_id: u32,
}

impl HIDElement {
    /// Bit-width of this element in the report.
    pub fn bit_width(&self) -> u32 {
        self.report_size * self.report_count
    }

    /// Whether the element has a useful logical range (max > min).
    pub fn has_range(&self) -> bool {
        self.logical_max > self.logical_min
    }

    /// Normalize a raw integer value to `[0.0, 1.0]` based on the logical range.
    ///
    /// Returns `None` when the range is degenerate (max ≤ min).
    pub fn normalize(&self, raw: i64) -> Option<f64> {
        if !self.has_range() {
            return None;
        }
        let clamped = raw.clamp(self.logical_min, self.logical_max);
        let span = (self.logical_max - self.logical_min) as f64;
        Some((clamped - self.logical_min) as f64 / span)
    }

    /// Normalize a raw value to the signed range `[-1.0, 1.0]`, with the
    /// midpoint of the logical range mapping to `0.0`.
    pub fn normalize_signed(&self, raw: i64) -> Option<f64> {
        self.normalize(raw).map(|n| n * 2.0 - 1.0)
    }
}

// ---------------------------------------------------------------------------
// Parsed device descriptor (collection of elements)
// ---------------------------------------------------------------------------

/// Parsed top-level description of a macOS HID device.
#[derive(Debug, Clone)]
pub struct IOKitDeviceDescriptor {
    pub vendor_id: u16,
    pub product_id: u16,
    pub version_number: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
    pub transport: Option<String>,
    pub primary_usage_page: u32,
    pub primary_usage: u32,
    pub elements: Vec<HIDElement>,
    /// Location ID — unique per USB port on macOS.
    pub location_id: u32,
}

impl IOKitDeviceDescriptor {
    /// Build a device path string from the location ID (mirrors IOKit behaviour).
    pub fn device_path(&self) -> String {
        format!("IOService:/AppleUSBDevice@{:08X}", self.location_id)
    }

    /// Count elements of a specific type.
    pub fn count_elements(&self, typ: IOHIDElementType) -> usize {
        self.elements
            .iter()
            .filter(|e| e.element_type == typ)
            .count()
    }

    /// Detect whether this device has PID (force feedback) output elements.
    pub fn has_pid_outputs(&self) -> bool {
        self.elements
            .iter()
            .any(|e| e.usage_page == usage_page::PID && e.element_type.is_output())
    }

    /// Detect whether this looks like a racing wheel (usage page + usage).
    pub fn is_racing_wheel(&self) -> bool {
        self.primary_usage_page == usage_page::GENERIC_DESKTOP
            && (self.primary_usage == usage::WHEEL
                || self.primary_usage == usage::JOYSTICK
                || self.primary_usage == usage::GAME_PAD
                || self.primary_usage == usage::MULTI_AXIS_CONTROLLER)
    }

    /// Find the steering axis element (Generic Desktop / X).
    pub fn steering_element(&self) -> Option<&HIDElement> {
        self.elements.iter().find(|e| {
            e.usage_page == usage_page::GENERIC_DESKTOP
                && e.usage == usage::X
                && e.element_type.is_input()
        })
    }

    /// Find all button elements.
    pub fn button_elements(&self) -> Vec<&HIDElement> {
        self.elements
            .iter()
            .filter(|e| e.element_type == IOHIDElementType::InputButton)
            .collect()
    }

    /// Convert to the engine-level `HidDeviceInfo`.
    pub fn to_hid_device_info(&self) -> Result<HidDeviceInfo, Box<dyn std::error::Error>> {
        let device_id: DeviceId = format!(
            "macos-hid-{:04x}-{:04x}-{:08x}",
            self.vendor_id, self.product_id, self.location_id
        )
        .parse()?;

        let capabilities = self.detect_capabilities();

        Ok(HidDeviceInfo {
            device_id,
            vendor_id: self.vendor_id,
            product_id: self.product_id,
            serial_number: self.serial_number.clone(),
            manufacturer: self.manufacturer.clone(),
            product_name: self.product.clone(),
            path: self.device_path(),
            interface_number: None,
            usage_page: Some(self.primary_usage_page as u16),
            usage: Some(self.primary_usage as u16),
            report_descriptor_len: None,
            report_descriptor_crc32: None,
            capabilities,
        })
    }

    /// Heuristic capability detection from the element list.
    fn detect_capabilities(&self) -> DeviceCapabilities {
        let has_pid = self.has_pid_outputs();
        let has_steering = self.steering_element().is_some();

        // Estimate max torque: if PID outputs exist, assume a sane default;
        // real values come from the feature report at runtime.
        let max_torque = if has_pid {
            TorqueNm::new(10.0).unwrap_or(TorqueNm::ZERO)
        } else {
            TorqueNm::ZERO
        };

        // Look for encoder CPR info in feature reports (usage_page == PID,
        // element type == Feature). For now default to 0 (unknown).
        let encoder_cpr: u16 = 0;

        DeviceCapabilities {
            supports_pid: has_pid,
            supports_raw_torque_1khz: has_pid && has_steering,
            supports_health_stream: false,
            supports_led_bus: false,
            max_torque,
            encoder_cpr,
            min_report_period_us: if has_pid { 1000 } else { 8000 },
        }
    }
}

// ---------------------------------------------------------------------------
// IOKit HID report parsing helpers
// ---------------------------------------------------------------------------

/// Classification of an incoming HID report based on its report-ID byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind {
    /// Standard input report with axes / buttons.
    Input,
    /// Telemetry / health report (OWP-1 0x21).
    Telemetry,
    /// Device capabilities feature report (OWP-1 0x01).
    Capabilities,
    /// Vendor-specific report.
    VendorSpecific(u8),
    /// Unknown / unrecognised.
    Unknown(u8),
}

/// Classify a raw HID report by its first byte (report ID).
pub fn classify_report(data: &[u8]) -> Option<ReportKind> {
    let &report_id = data.first()?;
    Some(match report_id {
        0x01 => ReportKind::Capabilities,
        0x02..=0x0F => ReportKind::Input,
        0x20 => ReportKind::Input, // OWP-1 torque echoes back as input
        0x21 => ReportKind::Telemetry,
        0x80..=0xFE => ReportKind::VendorSpecific(report_id),
        _ => ReportKind::Unknown(report_id),
    })
}

/// Error type for macOS HID operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacHidError {
    /// IOKit returned a non-zero status code.
    IOReturn(i32),
    /// Device was disconnected while in use.
    DeviceRemoved,
    /// A matching dictionary could not be built (programmer error).
    InvalidMatchingDict(String),
    /// A required HID element was not found on the device.
    MissingElement { usage_page: u32, usage: u32 },
    /// Report data was too short or malformed.
    MalformedReport { expected_min: usize, actual: usize },
}

impl fmt::Display for MacHidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IOReturn(code) => write!(f, "IOKit error: 0x{code:08X}"),
            Self::DeviceRemoved => write!(f, "HID device removed"),
            Self::InvalidMatchingDict(msg) => {
                write!(f, "invalid matching dictionary: {msg}")
            }
            Self::MissingElement { usage_page, usage } => {
                write!(
                    f,
                    "missing HID element: usage_page=0x{usage_page:04X} usage=0x{usage:04X}"
                )
            }
            Self::MalformedReport {
                expected_min,
                actual,
            } => {
                write!(
                    f,
                    "malformed report: expected >= {expected_min} bytes, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for MacHidError {}

/// Validate that a raw report buffer has the expected minimum length.
pub fn validate_report_length(data: &[u8], expected_min: usize) -> Result<(), MacHidError> {
    if data.len() < expected_min {
        Err(MacHidError::MalformedReport {
            expected_min,
            actual: data.len(),
        })
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Racing-wheel matching dictionary presets
// ---------------------------------------------------------------------------

/// Build a matching dictionary that finds racing wheel HID devices.
pub fn racing_wheel_matching_dict() -> IOKitMatchingDict {
    IOKitMatchingDict::new()
        .with_usage_page(usage_page::GENERIC_DESKTOP)
        .with_usage(usage::WHEEL)
}

/// Build a matching dictionary for a specific vendor+product over USB.
pub fn device_matching_dict(vid: u16, pid: u16) -> IOKitMatchingDict {
    IOKitMatchingDict::new()
        .with_vendor_id(vid)
        .with_product_id(pid)
        .with_transport("USB")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- IOKitMatchingDict builder tests --

    #[test]
    fn test_matching_dict_builder_basic() -> Result<(), Box<dyn std::error::Error>> {
        let dict = IOKitMatchingDict::new()
            .with_vendor_id(0x346E)
            .with_product_id(0x0004);

        assert_eq!(dict.get_integer("VendorID"), Some(0x346E));
        assert_eq!(dict.get_integer("ProductID"), Some(0x0004));
        assert_eq!(dict.get_integer("Transport"), None);
        Ok(())
    }

    #[test]
    fn test_matching_dict_usage_page_filter() -> Result<(), Box<dyn std::error::Error>> {
        let dict = racing_wheel_matching_dict();

        assert_eq!(
            dict.get_integer("DeviceUsagePage"),
            Some(u64::from(usage_page::GENERIC_DESKTOP))
        );
        assert_eq!(
            dict.get_integer("DeviceUsage"),
            Some(u64::from(usage::WHEEL))
        );
        Ok(())
    }

    #[test]
    fn test_matching_dict_matches_correct_device() -> Result<(), Box<dyn std::error::Error>> {
        let dict = IOKitMatchingDict::new()
            .with_vendor_id(0x346E)
            .with_usage_page(usage_page::GENERIC_DESKTOP)
            .with_usage(usage::WHEEL);

        // Moza wheel
        assert!(dict.matches_device(0x346E, 0x0004, usage_page::GENERIC_DESKTOP, usage::WHEEL));
        // Wrong vendor
        assert!(!dict.matches_device(0x0EB7, 0x0004, usage_page::GENERIC_DESKTOP, usage::WHEEL));
        // Wrong usage
        assert!(!dict.matches_device(0x346E, 0x0004, usage_page::GENERIC_DESKTOP, usage::JOYSTICK));
        Ok(())
    }

    #[test]
    fn test_matching_dict_empty_matches_everything() -> Result<(), Box<dyn std::error::Error>> {
        let dict = IOKitMatchingDict::new();
        assert!(dict.matches_device(0xFFFF, 0xFFFF, 0x01, 0x04));
        Ok(())
    }

    #[test]
    fn test_device_matching_dict_includes_transport() -> Result<(), Box<dyn std::error::Error>> {
        let dict = device_matching_dict(0x0EB7, 0x0024);

        assert_eq!(dict.get_integer("VendorID"), Some(0x0EB7));
        assert_eq!(dict.get_integer("ProductID"), Some(0x0024));

        // Transport is a string entry — get_integer returns None.
        assert_eq!(dict.get_integer("Transport"), None);

        // But the entry is present.
        let transport = dict
            .entries
            .iter()
            .find(|(k, _)| k == "Transport")
            .map(|(_, v)| v);
        assert_eq!(transport, Some(&MatchValue::String("USB".to_string())));
        Ok(())
    }

    // -- IOHIDElementType tests --

    #[test]
    fn test_element_type_from_raw_valid() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            IOHIDElementType::from_raw(1),
            Some(IOHIDElementType::InputMisc)
        );
        assert_eq!(
            IOHIDElementType::from_raw(2),
            Some(IOHIDElementType::InputButton)
        );
        assert_eq!(
            IOHIDElementType::from_raw(3),
            Some(IOHIDElementType::InputAxis)
        );
        assert_eq!(
            IOHIDElementType::from_raw(129),
            Some(IOHIDElementType::Output)
        );
        assert_eq!(
            IOHIDElementType::from_raw(257),
            Some(IOHIDElementType::Feature)
        );
        assert_eq!(
            IOHIDElementType::from_raw(513),
            Some(IOHIDElementType::Collection)
        );
        Ok(())
    }

    #[test]
    fn test_element_type_from_raw_invalid() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(IOHIDElementType::from_raw(0), None);
        assert_eq!(IOHIDElementType::from_raw(4), None);
        assert_eq!(IOHIDElementType::from_raw(999), None);
        Ok(())
    }

    #[test]
    fn test_element_type_is_input() -> Result<(), Box<dyn std::error::Error>> {
        assert!(IOHIDElementType::InputMisc.is_input());
        assert!(IOHIDElementType::InputButton.is_input());
        assert!(IOHIDElementType::InputAxis.is_input());
        assert!(!IOHIDElementType::Output.is_input());
        assert!(!IOHIDElementType::Feature.is_input());
        assert!(!IOHIDElementType::Collection.is_input());
        Ok(())
    }

    #[test]
    fn test_element_type_is_output() -> Result<(), Box<dyn std::error::Error>> {
        assert!(IOHIDElementType::Output.is_output());
        assert!(!IOHIDElementType::InputAxis.is_output());
        assert!(!IOHIDElementType::Feature.is_output());
        Ok(())
    }

    #[test]
    fn test_element_type_display() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(format!("{}", IOHIDElementType::InputAxis), "Input (Axis)");
        assert_eq!(format!("{}", IOHIDElementType::Output), "Output");
        assert_eq!(format!("{}", IOHIDElementType::Feature), "Feature");
        Ok(())
    }

    // -- HIDElement tests --

    fn make_steering_element() -> HIDElement {
        HIDElement {
            element_type: IOHIDElementType::InputAxis,
            usage_page: usage_page::GENERIC_DESKTOP,
            usage: usage::X,
            logical_min: 0,
            logical_max: 65535,
            physical_min: -900,
            physical_max: 900,
            report_size: 16,
            report_count: 1,
            report_id: 1,
        }
    }

    fn make_button_element(index: u32) -> HIDElement {
        HIDElement {
            element_type: IOHIDElementType::InputButton,
            usage_page: 0x09, // Button page
            usage: index,
            logical_min: 0,
            logical_max: 1,
            physical_min: 0,
            physical_max: 1,
            report_size: 1,
            report_count: 1,
            report_id: 1,
        }
    }

    fn make_pid_output_element() -> HIDElement {
        HIDElement {
            element_type: IOHIDElementType::Output,
            usage_page: usage_page::PID,
            usage: 0x25, // Set Effect Report
            logical_min: 0,
            logical_max: 255,
            physical_min: 0,
            physical_max: 255,
            report_size: 8,
            report_count: 1,
            report_id: 2,
        }
    }

    #[test]
    fn test_element_bit_width() -> Result<(), Box<dyn std::error::Error>> {
        let elem = make_steering_element();
        assert_eq!(elem.bit_width(), 16);

        let btn = make_button_element(1);
        assert_eq!(btn.bit_width(), 1);
        Ok(())
    }

    #[test]
    fn test_element_has_range() -> Result<(), Box<dyn std::error::Error>> {
        let elem = make_steering_element();
        assert!(elem.has_range());

        let degenerate = HIDElement {
            logical_min: 5,
            logical_max: 5,
            ..make_steering_element()
        };
        assert!(!degenerate.has_range());
        Ok(())
    }

    #[test]
    fn test_element_normalize_center() -> Result<(), Box<dyn std::error::Error>> {
        let elem = make_steering_element();
        let mid = (elem.logical_min + elem.logical_max) / 2;
        let norm = elem.normalize(mid).ok_or("normalize returned None")?;
        // Midpoint of [0, 65535] ≈ 0.5
        assert!((norm - 0.5).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_element_normalize_boundaries() -> Result<(), Box<dyn std::error::Error>> {
        let elem = make_steering_element();

        let at_min = elem.normalize(elem.logical_min).ok_or("None at min")?;
        assert!((at_min).abs() < f64::EPSILON);

        let at_max = elem.normalize(elem.logical_max).ok_or("None at max")?;
        assert!((at_max - 1.0).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn test_element_normalize_clamps() -> Result<(), Box<dyn std::error::Error>> {
        let elem = make_steering_element();

        let below = elem.normalize(elem.logical_min - 100).ok_or("None below")?;
        assert!((below).abs() < f64::EPSILON);

        let above = elem.normalize(elem.logical_max + 100).ok_or("None above")?;
        assert!((above - 1.0).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn test_element_normalize_degenerate_returns_none() -> Result<(), Box<dyn std::error::Error>> {
        let elem = HIDElement {
            logical_min: 10,
            logical_max: 10,
            ..make_steering_element()
        };
        assert!(elem.normalize(10).is_none());
        Ok(())
    }

    #[test]
    fn test_element_normalize_signed() -> Result<(), Box<dyn std::error::Error>> {
        let elem = make_steering_element();

        let left = elem
            .normalize_signed(elem.logical_min)
            .ok_or("None at min")?;
        assert!((left - (-1.0)).abs() < f64::EPSILON);

        let right = elem
            .normalize_signed(elem.logical_max)
            .ok_or("None at max")?;
        assert!((right - 1.0).abs() < f64::EPSILON);

        let center = elem
            .normalize_signed((elem.logical_min + elem.logical_max) / 2)
            .ok_or("None at center")?;
        assert!(center.abs() < 0.001);
        Ok(())
    }

    // -- IOKitDeviceDescriptor tests --

    fn make_wheel_descriptor() -> IOKitDeviceDescriptor {
        IOKitDeviceDescriptor {
            vendor_id: 0x346E,
            product_id: 0x0004,
            version_number: 0x0100,
            manufacturer: Some("Gudsen / Moza".to_string()),
            product: Some("Moza R5".to_string()),
            serial_number: Some("MZ-R5-00001".to_string()),
            transport: Some("USB".to_string()),
            primary_usage_page: usage_page::GENERIC_DESKTOP,
            primary_usage: usage::WHEEL,
            location_id: 0x14100000,
            elements: vec![
                make_steering_element(),
                make_button_element(1),
                make_button_element(2),
                make_button_element(3),
                make_pid_output_element(),
            ],
        }
    }

    #[test]
    fn test_descriptor_device_path() -> Result<(), Box<dyn std::error::Error>> {
        let desc = make_wheel_descriptor();
        assert_eq!(desc.device_path(), "IOService:/AppleUSBDevice@14100000");
        Ok(())
    }

    #[test]
    fn test_descriptor_count_elements() -> Result<(), Box<dyn std::error::Error>> {
        let desc = make_wheel_descriptor();
        assert_eq!(desc.count_elements(IOHIDElementType::InputAxis), 1);
        assert_eq!(desc.count_elements(IOHIDElementType::InputButton), 3);
        assert_eq!(desc.count_elements(IOHIDElementType::Output), 1);
        assert_eq!(desc.count_elements(IOHIDElementType::Feature), 0);
        Ok(())
    }

    #[test]
    fn test_descriptor_has_pid_outputs() -> Result<(), Box<dyn std::error::Error>> {
        let desc = make_wheel_descriptor();
        assert!(desc.has_pid_outputs());

        let no_pid = IOKitDeviceDescriptor {
            elements: vec![make_steering_element()],
            ..make_wheel_descriptor()
        };
        assert!(!no_pid.has_pid_outputs());
        Ok(())
    }

    #[test]
    fn test_descriptor_is_racing_wheel() -> Result<(), Box<dyn std::error::Error>> {
        let wheel = make_wheel_descriptor();
        assert!(wheel.is_racing_wheel());

        let joystick = IOKitDeviceDescriptor {
            primary_usage: usage::JOYSTICK,
            ..make_wheel_descriptor()
        };
        assert!(joystick.is_racing_wheel());

        let keyboard = IOKitDeviceDescriptor {
            primary_usage_page: 0x07, // Keyboard
            primary_usage: 0x06,
            ..make_wheel_descriptor()
        };
        assert!(!keyboard.is_racing_wheel());
        Ok(())
    }

    #[test]
    fn test_descriptor_steering_element() -> Result<(), Box<dyn std::error::Error>> {
        let desc = make_wheel_descriptor();
        let steering = desc.steering_element().ok_or("no steering element found")?;
        assert_eq!(steering.usage_page, usage_page::GENERIC_DESKTOP);
        assert_eq!(steering.usage, usage::X);
        assert_eq!(steering.bit_width(), 16);
        Ok(())
    }

    #[test]
    fn test_descriptor_button_elements() -> Result<(), Box<dyn std::error::Error>> {
        let desc = make_wheel_descriptor();
        let buttons = desc.button_elements();
        assert_eq!(buttons.len(), 3);
        Ok(())
    }

    #[test]
    fn test_descriptor_to_hid_device_info() -> Result<(), Box<dyn std::error::Error>> {
        let desc = make_wheel_descriptor();
        let info = desc.to_hid_device_info()?;

        assert_eq!(info.vendor_id, 0x346E);
        assert_eq!(info.product_id, 0x0004);
        assert_eq!(info.product_name.as_deref(), Some("Moza R5"));
        assert_eq!(info.serial_number.as_deref(), Some("MZ-R5-00001"));
        assert_eq!(info.manufacturer.as_deref(), Some("Gudsen / Moza"));
        assert_eq!(info.usage_page, Some(usage_page::GENERIC_DESKTOP as u16));
        assert_eq!(info.usage, Some(usage::WHEEL as u16));
        assert!(info.path.contains("14100000"));
        Ok(())
    }

    #[test]
    fn test_descriptor_capability_detection_with_pid() -> Result<(), Box<dyn std::error::Error>> {
        let desc = make_wheel_descriptor();
        let info = desc.to_hid_device_info()?;

        assert!(info.capabilities.supports_pid);
        assert!(info.capabilities.supports_raw_torque_1khz);
        assert_eq!(info.capabilities.min_report_period_us, 1000);
        Ok(())
    }

    #[test]
    fn test_descriptor_capability_detection_without_pid() -> Result<(), Box<dyn std::error::Error>>
    {
        let desc = IOKitDeviceDescriptor {
            elements: vec![make_steering_element(), make_button_element(1)],
            ..make_wheel_descriptor()
        };
        let info = desc.to_hid_device_info()?;

        assert!(!info.capabilities.supports_pid);
        assert!(!info.capabilities.supports_raw_torque_1khz);
        assert_eq!(info.capabilities.min_report_period_us, 8000);
        Ok(())
    }

    #[test]
    fn test_descriptor_device_info_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let desc = make_wheel_descriptor();
        let hid_info = desc.to_hid_device_info()?;
        let device_info = hid_info.to_device_info();

        assert_eq!(device_info.name, "Moza R5");
        assert_eq!(device_info.vendor_id, 0x346E);
        assert_eq!(device_info.product_id, 0x0004);
        assert!(device_info.is_connected);
        Ok(())
    }

    // -- Report classification tests --

    #[test]
    fn test_classify_report_capabilities() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            classify_report(&[0x01, 0x00]),
            Some(ReportKind::Capabilities)
        );
        Ok(())
    }

    #[test]
    fn test_classify_report_input_range() -> Result<(), Box<dyn std::error::Error>> {
        for id in 0x02..=0x0F {
            assert_eq!(
                classify_report(&[id, 0xAA]),
                Some(ReportKind::Input),
                "report ID 0x{id:02X} should be Input"
            );
        }
        assert_eq!(classify_report(&[0x20, 0x00]), Some(ReportKind::Input));
        Ok(())
    }

    #[test]
    fn test_classify_report_telemetry() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            classify_report(&[0x21, 0x01, 0x02]),
            Some(ReportKind::Telemetry)
        );
        Ok(())
    }

    #[test]
    fn test_classify_report_vendor_specific() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            classify_report(&[0x80]),
            Some(ReportKind::VendorSpecific(0x80))
        );
        assert_eq!(
            classify_report(&[0xFE]),
            Some(ReportKind::VendorSpecific(0xFE))
        );
        Ok(())
    }

    #[test]
    fn test_classify_report_unknown() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(classify_report(&[0x00]), Some(ReportKind::Unknown(0x00)));
        assert_eq!(classify_report(&[0xFF]), Some(ReportKind::Unknown(0xFF)));
        Ok(())
    }

    #[test]
    fn test_classify_report_empty_returns_none() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(classify_report(&[]), None);
        Ok(())
    }

    // -- MacHidError tests --

    #[test]
    fn test_error_display_io_return() -> Result<(), Box<dyn std::error::Error>> {
        let err = MacHidError::IOReturn(0x0E00_02BC);
        let msg = format!("{err}");
        assert!(msg.contains("IOKit error"));
        Ok(())
    }

    #[test]
    fn test_error_display_device_removed() -> Result<(), Box<dyn std::error::Error>> {
        let err = MacHidError::DeviceRemoved;
        assert_eq!(format!("{err}"), "HID device removed");
        Ok(())
    }

    #[test]
    fn test_error_display_missing_element() -> Result<(), Box<dyn std::error::Error>> {
        let err = MacHidError::MissingElement {
            usage_page: 0x01,
            usage: 0x30,
        };
        let msg = format!("{err}");
        assert!(msg.contains("0x0001"));
        assert!(msg.contains("0x0030"));
        Ok(())
    }

    #[test]
    fn test_error_display_malformed_report() -> Result<(), Box<dyn std::error::Error>> {
        let err = MacHidError::MalformedReport {
            expected_min: 12,
            actual: 4,
        };
        let msg = format!("{err}");
        assert!(msg.contains("12"));
        assert!(msg.contains("4"));
        Ok(())
    }

    #[test]
    fn test_validate_report_length_ok() -> Result<(), Box<dyn std::error::Error>> {
        validate_report_length(&[0x01, 0x02, 0x03], 3)?;
        validate_report_length(&[0x01, 0x02, 0x03, 0x04], 3)?;
        Ok(())
    }

    #[test]
    fn test_validate_report_length_too_short() -> Result<(), Box<dyn std::error::Error>> {
        let result = validate_report_length(&[0x01, 0x02], 5);
        assert!(result.is_err());
        let err = result.err().ok_or("expected error")?;
        assert_eq!(
            err,
            MacHidError::MalformedReport {
                expected_min: 5,
                actual: 2,
            }
        );
        Ok(())
    }

    #[test]
    fn test_error_is_std_error() -> Result<(), Box<dyn std::error::Error>> {
        // Verify MacHidError can be used as a boxed std::error::Error.
        let err: Box<dyn std::error::Error> = Box::new(MacHidError::DeviceRemoved);
        assert!(!err.to_string().is_empty());
        Ok(())
    }

    // -- Matching dictionary with real-world VID/PID combos --

    #[test]
    fn test_racing_wheel_dict_matches_moza() -> Result<(), Box<dyn std::error::Error>> {
        let dict = racing_wheel_matching_dict();
        assert!(dict.matches_device(0x346E, 0x0004, usage_page::GENERIC_DESKTOP, usage::WHEEL,));
        Ok(())
    }

    #[test]
    fn test_racing_wheel_dict_rejects_keyboard() -> Result<(), Box<dyn std::error::Error>> {
        let dict = racing_wheel_matching_dict();
        // Keyboard usage page 0x07 / usage 0x06
        assert!(!dict.matches_device(0x046D, 0xC32B, 0x07, 0x06));
        Ok(())
    }

    #[test]
    fn test_device_dict_specific_product() -> Result<(), Box<dyn std::error::Error>> {
        let dict = device_matching_dict(0x0EB7, 0x0024);
        // Correct product
        assert!(dict.matches_device(0x0EB7, 0x0024, usage_page::GENERIC_DESKTOP, usage::WHEEL));
        // Wrong product ID
        assert!(!dict.matches_device(0x0EB7, 0x0020, usage_page::GENERIC_DESKTOP, usage::WHEEL));
        Ok(())
    }

    // -- Edge-case descriptor tests --

    #[test]
    fn test_descriptor_no_elements() -> Result<(), Box<dyn std::error::Error>> {
        let desc = IOKitDeviceDescriptor {
            elements: vec![],
            ..make_wheel_descriptor()
        };
        assert!(!desc.has_pid_outputs());
        assert!(desc.steering_element().is_none());
        assert!(desc.button_elements().is_empty());
        assert_eq!(desc.count_elements(IOHIDElementType::InputAxis), 0);
        Ok(())
    }

    #[test]
    fn test_descriptor_no_product_name_generates_fallback() -> Result<(), Box<dyn std::error::Error>>
    {
        let desc = IOKitDeviceDescriptor {
            product: None,
            ..make_wheel_descriptor()
        };
        let info = desc.to_hid_device_info()?;
        let device_info = info.to_device_info();
        assert!(device_info.name.contains("346E"));
        assert!(device_info.name.contains("0004"));
        Ok(())
    }

    #[test]
    fn test_usage_page_constants() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(usage_page::GENERIC_DESKTOP, 0x01);
        assert_eq!(usage_page::SIMULATION, 0x02);
        assert_eq!(usage_page::PID, 0x0F);
        assert_eq!(usage_page::VENDOR_DEFINED_START, 0xFF00);
        Ok(())
    }

    #[test]
    fn test_usage_constants() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(usage::WHEEL, 0x38);
        assert_eq!(usage::JOYSTICK, 0x04);
        assert_eq!(usage::X, 0x30);
        assert_eq!(usage::HAT_SWITCH, 0x39);
        Ok(())
    }
}
