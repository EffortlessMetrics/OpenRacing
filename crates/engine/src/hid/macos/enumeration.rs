//! macOS IOKit HID device enumeration.
//!
//! Provides `MacOSHidEnumerator` for discovering connected HID devices using
//! `IOHIDManager`. The enumerator logic is split into platform-agnostic
//! filtering (compiles everywhere) and IOKit FFI calls (macOS only).

use super::{IOKitDeviceDescriptor, IOKitMatchingDict, usage, usage_page};

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// VID/PID filter (platform-agnostic)
// ---------------------------------------------------------------------------

/// A vendor/product filter for device enumeration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceFilter {
    /// Required vendor ID (None = match any).
    pub vendor_id: Option<u16>,
    /// Required product ID (None = match any).
    pub product_id: Option<u16>,
    /// Required usage page (None = match any).
    pub usage_page: Option<u32>,
    /// Required usage (None = match any).
    pub usage: Option<u32>,
}

impl DeviceFilter {
    pub fn new() -> Self {
        Self {
            vendor_id: None,
            product_id: None,
            usage_page: None,
            usage: None,
        }
    }

    pub fn with_vendor_id(mut self, vid: u16) -> Self {
        self.vendor_id = Some(vid);
        self
    }

    pub fn with_product_id(mut self, pid: u16) -> Self {
        self.product_id = Some(pid);
        self
    }

    pub fn with_usage_page(mut self, page: u32) -> Self {
        self.usage_page = Some(page);
        self
    }

    pub fn with_usage(mut self, u: u32) -> Self {
        self.usage = Some(u);
        self
    }

    /// Check whether a device descriptor passes this filter.
    pub fn matches(&self, desc: &IOKitDeviceDescriptor) -> bool {
        if let Some(vid) = self.vendor_id
            && desc.vendor_id != vid
        {
            return false;
        }
        if let Some(pid) = self.product_id
            && desc.product_id != pid
        {
            return false;
        }
        if let Some(page) = self.usage_page
            && desc.primary_usage_page != page
        {
            return false;
        }
        if let Some(u) = self.usage
            && desc.primary_usage != u
        {
            return false;
        }
        true
    }

    /// Convert this filter into an `IOKitMatchingDict` for use with IOHIDManager.
    pub fn to_matching_dict(&self) -> IOKitMatchingDict {
        let mut dict = IOKitMatchingDict::new();
        if let Some(vid) = self.vendor_id {
            dict = dict.with_vendor_id(vid);
        }
        if let Some(pid) = self.product_id {
            dict = dict.with_product_id(pid);
        }
        if let Some(page) = self.usage_page {
            dict = dict.with_usage_page(page);
        }
        if let Some(u) = self.usage {
            dict = dict.with_usage(u);
        }
        dict
    }
}

impl Default for DeviceFilter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Enumeration result
// ---------------------------------------------------------------------------

/// Result of a device enumeration pass.
#[derive(Debug, Clone)]
pub struct EnumerationResult {
    /// Devices discovered in this pass.
    pub devices: Vec<IOKitDeviceDescriptor>,
    /// Devices that are new since the last pass.
    pub added: Vec<IOKitDeviceDescriptor>,
    /// Devices that disappeared since the last pass.
    pub removed: Vec<String>,
}

// ---------------------------------------------------------------------------
// Enumerator state machine (platform-agnostic)
// ---------------------------------------------------------------------------

/// Platform-agnostic device enumerator that tracks known devices and detects
/// additions/removals via path-based diffing.
pub struct MacOSHidEnumerator {
    filter: DeviceFilter,
    /// Map of device_path → descriptor for currently known devices.
    known_devices: HashMap<String, IOKitDeviceDescriptor>,
}

impl MacOSHidEnumerator {
    /// Create an enumerator with the default racing-wheel filter.
    pub fn racing_wheels() -> Self {
        Self {
            filter: DeviceFilter::new()
                .with_usage_page(usage_page::GENERIC_DESKTOP)
                .with_usage(usage::WHEEL),
            known_devices: HashMap::new(),
        }
    }

    /// Create an enumerator that accepts all HID devices.
    pub fn all_devices() -> Self {
        Self {
            filter: DeviceFilter::new(),
            known_devices: HashMap::new(),
        }
    }

    /// Create an enumerator with a custom filter.
    pub fn with_filter(filter: DeviceFilter) -> Self {
        Self {
            filter,
            known_devices: HashMap::new(),
        }
    }

    /// Current filter.
    pub fn filter(&self) -> &DeviceFilter {
        &self.filter
    }

    /// Number of currently known devices.
    pub fn device_count(&self) -> usize {
        self.known_devices.len()
    }

    /// Get a known device by path.
    pub fn get_device(&self, path: &str) -> Option<&IOKitDeviceDescriptor> {
        self.known_devices.get(path)
    }

    /// List all currently known device paths.
    pub fn known_paths(&self) -> Vec<String> {
        self.known_devices.keys().cloned().collect()
    }

    /// Process a fresh set of descriptors from the system and compute
    /// additions / removals relative to the known set.
    pub fn process_enumeration(
        &mut self,
        system_devices: Vec<IOKitDeviceDescriptor>,
    ) -> EnumerationResult {
        let filtered: Vec<IOKitDeviceDescriptor> = system_devices
            .into_iter()
            .filter(|d| self.filter.matches(d))
            .collect();

        let mut new_map = HashMap::new();
        let mut added = Vec::new();

        for desc in &filtered {
            let path = desc.device_path();
            if !self.known_devices.contains_key(&path) {
                added.push(desc.clone());
            }
            new_map.insert(path, desc.clone());
        }

        let removed: Vec<String> = self
            .known_devices
            .keys()
            .filter(|p| !new_map.contains_key(p.as_str()))
            .cloned()
            .collect();

        self.known_devices = new_map;

        EnumerationResult {
            devices: filtered,
            added,
            removed,
        }
    }

    /// Manually register a device (e.g. from a hotplug callback).
    pub fn register_device(&mut self, desc: IOKitDeviceDescriptor) -> bool {
        if !self.filter.matches(&desc) {
            return false;
        }
        let path = desc.device_path();
        self.known_devices.insert(path, desc);
        true
    }

    /// Manually remove a device by path (e.g. from a removal callback).
    pub fn remove_device(&mut self, path: &str) -> Option<IOKitDeviceDescriptor> {
        self.known_devices.remove(path)
    }

    /// Clear all known devices.
    pub fn clear(&mut self) {
        self.known_devices.clear();
    }
}

// ---------------------------------------------------------------------------
// IOKit FFI-backed enumeration (macOS only)
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
pub mod ffi_enumeration {
    use super::*;
    use crate::hid::macos::device::ffi_device;
    use crate::hid::macos::iokit_ffi::{self, *};
    use std::ffi::c_void;

    /// Create an IOHIDManager, open it, and enumerate matching devices.
    pub fn enumerate_system_devices(
        filter: &DeviceFilter,
    ) -> Result<Vec<IOKitDeviceDescriptor>, MacHidError> {
        unsafe {
            let manager = IOHIDManagerCreate(kCFAllocatorDefault, K_IOHID_MANAGER_OPTION_NONE);
            if manager.is_null() {
                return Err(MacHidError::IOReturn(K_IO_RETURN_ERROR));
            }

            // Build the matching dictionary from the filter
            if let Some(vid) = filter.vendor_id {
                if let Some(pid) = filter.product_id {
                    if let Some(dict) = iokit_ffi::matching_dict_for_vid_pid(vid, pid) {
                        IOHIDManagerSetDeviceMatching(manager, dict.as_ptr() as CFDictionaryRef);
                    }
                }
            } else if let (Some(page), Some(u)) = (filter.usage_page, filter.usage) {
                if let Some(dict) = iokit_ffi::matching_dict_for_usage(page, u) {
                    IOHIDManagerSetDeviceMatching(manager, dict.as_ptr() as CFDictionaryRef);
                }
            } else {
                // Match everything
                IOHIDManagerSetDeviceMatching(manager, std::ptr::null());
            }

            let ret = IOHIDManagerOpen(manager, K_IOHID_OPTIONS_TYPE_NONE);
            if ret != K_IO_RETURN_SUCCESS {
                CFRelease(manager as CFTypeRef);
                return Err(MacHidError::IOReturn(ret));
            }

            let device_set = IOHIDManagerCopyDevices(manager);
            let mut descriptors = Vec::new();

            if !device_set.is_null() {
                let count = CFSetGetCount(device_set);
                let mut device_refs = vec![std::ptr::null() as CFTypeRef; count as usize];
                CFSetGetValues(device_set, device_refs.as_mut_ptr());

                for &dev_ref in &device_refs {
                    if dev_ref.is_null() {
                        continue;
                    }
                    match ffi_device::read_descriptor(dev_ref as IOHIDDeviceRef) {
                        Ok(desc) => descriptors.push(desc),
                        Err(_) => continue,
                    }
                }

                CFRelease(device_set as CFTypeRef);
            }

            let _ = IOHIDManagerClose(manager, K_IOHID_OPTIONS_TYPE_NONE);
            CFRelease(manager as CFTypeRef);

            Ok(descriptors)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::macos::{HIDElement, IOHIDElementType, usage, usage_page};

    fn make_wheel_descriptor(vid: u16, pid: u16, location: u32) -> IOKitDeviceDescriptor {
        IOKitDeviceDescriptor {
            vendor_id: vid,
            product_id: pid,
            version_number: 0x0100,
            manufacturer: Some("TestVendor".to_string()),
            product: Some("TestWheel".to_string()),
            serial_number: Some(format!("SN-{vid:04X}-{pid:04X}")),
            transport: Some("USB".to_string()),
            primary_usage_page: usage_page::GENERIC_DESKTOP,
            primary_usage: usage::WHEEL,
            location_id: location,
            elements: vec![HIDElement {
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
            }],
        }
    }

    fn make_joystick_descriptor(vid: u16, pid: u16, location: u32) -> IOKitDeviceDescriptor {
        IOKitDeviceDescriptor {
            primary_usage: usage::JOYSTICK,
            ..make_wheel_descriptor(vid, pid, location)
        }
    }

    // -- DeviceFilter tests --

    #[test]
    fn test_filter_matches_any() -> Result<(), Box<dyn std::error::Error>> {
        let filter = DeviceFilter::new();
        let desc = make_wheel_descriptor(0x346E, 0x0004, 0x1000);
        assert!(filter.matches(&desc));
        Ok(())
    }

    #[test]
    fn test_filter_matches_vendor_id() -> Result<(), Box<dyn std::error::Error>> {
        let filter = DeviceFilter::new().with_vendor_id(0x346E);
        assert!(filter.matches(&make_wheel_descriptor(0x346E, 0x0004, 0x1000)));
        assert!(!filter.matches(&make_wheel_descriptor(0x0EB7, 0x0004, 0x2000)));
        Ok(())
    }

    #[test]
    fn test_filter_matches_vid_pid() -> Result<(), Box<dyn std::error::Error>> {
        let filter = DeviceFilter::new()
            .with_vendor_id(0x346E)
            .with_product_id(0x0004);
        assert!(filter.matches(&make_wheel_descriptor(0x346E, 0x0004, 0x1000)));
        assert!(!filter.matches(&make_wheel_descriptor(0x346E, 0x0005, 0x1000)));
        Ok(())
    }

    #[test]
    fn test_filter_matches_usage_page() -> Result<(), Box<dyn std::error::Error>> {
        let filter = DeviceFilter::new()
            .with_usage_page(usage_page::GENERIC_DESKTOP)
            .with_usage(usage::WHEEL);
        assert!(filter.matches(&make_wheel_descriptor(0x346E, 0x0004, 0x1000)));
        assert!(!filter.matches(&make_joystick_descriptor(0x346E, 0x0004, 0x1000)));
        Ok(())
    }

    #[test]
    fn test_filter_to_matching_dict() -> Result<(), Box<dyn std::error::Error>> {
        let filter = DeviceFilter::new()
            .with_vendor_id(0x346E)
            .with_product_id(0x0004);
        let dict = filter.to_matching_dict();
        assert_eq!(dict.get_integer("VendorID"), Some(0x346E));
        assert_eq!(dict.get_integer("ProductID"), Some(0x0004));
        Ok(())
    }

    // -- Enumerator tests --

    #[test]
    fn test_enumerator_initial_state() -> Result<(), Box<dyn std::error::Error>> {
        let enumerator = MacOSHidEnumerator::racing_wheels();
        assert_eq!(enumerator.device_count(), 0);
        assert!(enumerator.known_paths().is_empty());
        Ok(())
    }

    #[test]
    fn test_enumerator_process_first_pass() -> Result<(), Box<dyn std::error::Error>> {
        let mut enumerator = MacOSHidEnumerator::racing_wheels();
        let devices = vec![
            make_wheel_descriptor(0x346E, 0x0004, 0x1000),
            make_wheel_descriptor(0x0EB7, 0x0024, 0x2000),
        ];

        let result = enumerator.process_enumeration(devices);
        assert_eq!(result.devices.len(), 2);
        assert_eq!(result.added.len(), 2);
        assert!(result.removed.is_empty());
        assert_eq!(enumerator.device_count(), 2);
        Ok(())
    }

    #[test]
    fn test_enumerator_detects_removal() -> Result<(), Box<dyn std::error::Error>> {
        let mut enumerator = MacOSHidEnumerator::racing_wheels();
        let moza = make_wheel_descriptor(0x346E, 0x0004, 0x1000);
        let fanatec = make_wheel_descriptor(0x0EB7, 0x0024, 0x2000);

        // First pass: two devices
        enumerator.process_enumeration(vec![moza.clone(), fanatec.clone()]);
        assert_eq!(enumerator.device_count(), 2);

        // Second pass: only Moza remains
        let result = enumerator.process_enumeration(vec![moza]);
        assert_eq!(result.devices.len(), 1);
        assert!(result.added.is_empty());
        assert_eq!(result.removed.len(), 1);
        assert!(result.removed[0].contains("00002000"));
        assert_eq!(enumerator.device_count(), 1);
        Ok(())
    }

    #[test]
    fn test_enumerator_detects_addition() -> Result<(), Box<dyn std::error::Error>> {
        let mut enumerator = MacOSHidEnumerator::racing_wheels();
        let moza = make_wheel_descriptor(0x346E, 0x0004, 0x1000);
        let fanatec = make_wheel_descriptor(0x0EB7, 0x0024, 0x2000);

        enumerator.process_enumeration(vec![moza.clone()]);
        assert_eq!(enumerator.device_count(), 1);

        let result = enumerator.process_enumeration(vec![moza, fanatec]);
        assert_eq!(result.added.len(), 1);
        assert_eq!(result.added[0].vendor_id, 0x0EB7);
        assert_eq!(enumerator.device_count(), 2);
        Ok(())
    }

    #[test]
    fn test_enumerator_filters_non_wheel() -> Result<(), Box<dyn std::error::Error>> {
        let mut enumerator = MacOSHidEnumerator::racing_wheels();
        let wheel = make_wheel_descriptor(0x346E, 0x0004, 0x1000);
        let joystick = make_joystick_descriptor(0x046D, 0xC001, 0x3000);

        let result = enumerator.process_enumeration(vec![wheel, joystick]);
        // Only the wheel passes the filter
        assert_eq!(result.devices.len(), 1);
        assert_eq!(result.devices[0].vendor_id, 0x346E);
        Ok(())
    }

    #[test]
    fn test_enumerator_all_devices() -> Result<(), Box<dyn std::error::Error>> {
        let mut enumerator = MacOSHidEnumerator::all_devices();
        let wheel = make_wheel_descriptor(0x346E, 0x0004, 0x1000);
        let joystick = make_joystick_descriptor(0x046D, 0xC001, 0x3000);

        let result = enumerator.process_enumeration(vec![wheel, joystick]);
        assert_eq!(result.devices.len(), 2);
        Ok(())
    }

    #[test]
    fn test_enumerator_manual_register() -> Result<(), Box<dyn std::error::Error>> {
        let mut enumerator = MacOSHidEnumerator::racing_wheels();
        let wheel = make_wheel_descriptor(0x346E, 0x0004, 0x1000);

        assert!(enumerator.register_device(wheel));
        assert_eq!(enumerator.device_count(), 1);

        // Joystick should not register with the racing wheel filter
        let joystick = make_joystick_descriptor(0x046D, 0xC001, 0x3000);
        assert!(!enumerator.register_device(joystick));
        assert_eq!(enumerator.device_count(), 1);
        Ok(())
    }

    #[test]
    fn test_enumerator_manual_remove() -> Result<(), Box<dyn std::error::Error>> {
        let mut enumerator = MacOSHidEnumerator::racing_wheels();
        let wheel = make_wheel_descriptor(0x346E, 0x0004, 0x1000);
        let path = wheel.device_path();

        enumerator.register_device(wheel);
        assert_eq!(enumerator.device_count(), 1);

        let removed = enumerator.remove_device(&path);
        assert!(removed.is_some());
        assert_eq!(enumerator.device_count(), 0);
        Ok(())
    }

    #[test]
    fn test_enumerator_clear() -> Result<(), Box<dyn std::error::Error>> {
        let mut enumerator = MacOSHidEnumerator::all_devices();
        enumerator.register_device(make_wheel_descriptor(0x346E, 0x0004, 0x1000));
        enumerator.register_device(make_wheel_descriptor(0x0EB7, 0x0024, 0x2000));
        assert_eq!(enumerator.device_count(), 2);

        enumerator.clear();
        assert_eq!(enumerator.device_count(), 0);
        Ok(())
    }

    #[test]
    fn test_enumerator_stable_pass() -> Result<(), Box<dyn std::error::Error>> {
        let mut enumerator = MacOSHidEnumerator::racing_wheels();
        let devices = vec![make_wheel_descriptor(0x346E, 0x0004, 0x1000)];

        // First pass
        enumerator.process_enumeration(devices.clone());
        // Second pass with same devices
        let result = enumerator.process_enumeration(devices);
        assert!(result.added.is_empty());
        assert!(result.removed.is_empty());
        assert_eq!(result.devices.len(), 1);
        Ok(())
    }

    #[test]
    fn test_enumerator_get_device() -> Result<(), Box<dyn std::error::Error>> {
        let mut enumerator = MacOSHidEnumerator::all_devices();
        let wheel = make_wheel_descriptor(0x346E, 0x0004, 0x1000);
        let path = wheel.device_path();
        enumerator.register_device(wheel);

        let dev = enumerator.get_device(&path).ok_or("device not found")?;
        assert_eq!(dev.vendor_id, 0x346E);
        assert!(enumerator.get_device("nonexistent").is_none());
        Ok(())
    }
}
