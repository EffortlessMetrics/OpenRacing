//! Device information types for HID devices

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HidDeviceInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product_name: Option<String>,
    pub path: String,
}

impl HidDeviceInfo {
    pub fn new(vendor_id: u16, product_id: u16, path: String) -> Self {
        Self {
            vendor_id,
            product_id,
            serial_number: None,
            manufacturer: None,
            product_name: None,
            path,
        }
    }

    pub fn with_serial(mut self, serial: impl Into<String>) -> Self {
        self.serial_number = Some(serial.into());
        self
    }

    pub fn with_manufacturer(mut self, manufacturer: impl Into<String>) -> Self {
        self.manufacturer = Some(manufacturer.into());
        self
    }

    pub fn with_product_name(mut self, name: impl Into<String>) -> Self {
        self.product_name = Some(name.into());
        self
    }

    pub fn matches(&self, vendor_id: u16, product_id: u16) -> bool {
        self.vendor_id == vendor_id && self.product_id == product_id
    }

    pub fn display_name(&self) -> String {
        self.product_name
            .clone()
            .or_else(|| self.manufacturer.clone())
            .unwrap_or_else(|| format!("{:04x}:{:04x}", self.vendor_id, self.product_id))
    }
}

impl Default for HidDeviceInfo {
    fn default() -> Self {
        Self {
            vendor_id: 0,
            product_id: 0,
            serial_number: None,
            manufacturer: None,
            product_name: None,
            path: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_info_creation() {
        let info = HidDeviceInfo::new(0x1234, 0x5678, "/dev/hidraw0".to_string());
        assert_eq!(info.vendor_id, 0x1234);
        assert_eq!(info.product_id, 0x5678);
        assert!(info.matches(0x1234, 0x5678));
        assert!(!info.matches(0x1234, 0x9999));
    }

    #[test]
    fn test_device_info_display_name() {
        let info = HidDeviceInfo::new(0x1234, 0x5678, "/dev/hidraw0".to_string())
            .with_product_name("Test Wheel".to_string());
        assert_eq!(info.display_name(), "Test Wheel");

        let info = HidDeviceInfo::new(0x1234, 0x5678, "/dev/hidraw0".to_string())
            .with_manufacturer("Test Co".to_string());
        assert_eq!(info.display_name(), "Test Co");

        let info = HidDeviceInfo::new(0x1234, 0x5678, "/dev/hidraw0".to_string());
        assert_eq!(info.display_name(), "1234:5678");
    }
}
