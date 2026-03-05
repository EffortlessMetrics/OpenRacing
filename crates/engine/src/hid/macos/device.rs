//! macOS IOKit HID device implementation.
//!
//! Provides `MacOSHidDevice`, which wraps an `IOHIDDeviceRef` and implements
//! the engine's `HidDevice` trait. All IOKit FFI is behind
//! `#[cfg(target_os = "macos")]`; on other platforms only the mock-based
//! types are available.

use super::{
    IOKitDeviceDescriptor, MacHidError, ReportKind, classify_report, validate_report_length,
};

use std::fmt;

// ---------------------------------------------------------------------------
// Device state (platform-agnostic representation)
// ---------------------------------------------------------------------------

/// State machine for an IOKit HID device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    /// Device has been discovered but not opened.
    Discovered,
    /// Device file handle is open and ready for I/O.
    Open,
    /// Device has been closed (explicitly or due to removal).
    Closed,
    /// Device encountered a fatal error.
    Error,
}

impl fmt::Display for DeviceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Discovered => write!(f, "discovered"),
            Self::Open => write!(f, "open"),
            Self::Closed => write!(f, "closed"),
            Self::Error => write!(f, "error"),
        }
    }
}

// ---------------------------------------------------------------------------
// Platform-agnostic device handle
// ---------------------------------------------------------------------------

/// Maximum HID report size for racing wheels.
pub const MAX_HID_REPORT_SIZE: usize = 64;

/// Handle representing a macOS HID device, usable from any platform for
/// testing (via the mock constructor) or from macOS via IOKit FFI.
pub struct MacOSDeviceHandle {
    descriptor: IOKitDeviceDescriptor,
    state: DeviceState,
    /// Pre-allocated report buffer (avoids RT-path allocations).
    report_buffer: [u8; MAX_HID_REPORT_SIZE],
    /// Ring of most-recent input reports for diagnostics / polling.
    last_input_report: Option<Vec<u8>>,
    /// Cumulative I/O error counter.
    io_error_count: u32,
    /// Timestamp of last successful read.
    last_read: Option<std::time::Instant>,
}

impl MacOSDeviceHandle {
    /// Create a device handle from a parsed descriptor.
    pub fn new(descriptor: IOKitDeviceDescriptor) -> Self {
        Self {
            descriptor,
            state: DeviceState::Discovered,
            report_buffer: [0u8; MAX_HID_REPORT_SIZE],
            last_input_report: None,
            io_error_count: 0,
            last_read: None,
        }
    }

    // -- Accessors --

    pub fn descriptor(&self) -> &IOKitDeviceDescriptor {
        &self.descriptor
    }

    pub fn state(&self) -> DeviceState {
        self.state
    }

    pub fn vendor_id(&self) -> u16 {
        self.descriptor.vendor_id
    }

    pub fn product_id(&self) -> u16 {
        self.descriptor.product_id
    }

    pub fn device_path(&self) -> String {
        self.descriptor.device_path()
    }

    pub fn io_error_count(&self) -> u32 {
        self.io_error_count
    }

    pub fn last_read_time(&self) -> Option<std::time::Instant> {
        self.last_read
    }

    pub fn report_buffer(&self) -> &[u8; MAX_HID_REPORT_SIZE] {
        &self.report_buffer
    }

    pub fn report_buffer_mut(&mut self) -> &mut [u8; MAX_HID_REPORT_SIZE] {
        &mut self.report_buffer
    }

    pub fn last_input_report(&self) -> Option<&[u8]> {
        self.last_input_report.as_deref()
    }

    // -- State transitions --

    /// Mark the device as open.
    pub fn mark_open(&mut self) -> Result<(), MacHidError> {
        match self.state {
            DeviceState::Discovered => {
                self.state = DeviceState::Open;
                Ok(())
            }
            other => Err(MacHidError::InvalidMatchingDict(format!(
                "cannot open device in state: {other}"
            ))),
        }
    }

    /// Mark the device as closed.
    pub fn mark_closed(&mut self) {
        self.state = DeviceState::Closed;
    }

    /// Mark a fatal error.
    pub fn mark_error(&mut self) {
        self.state = DeviceState::Error;
    }

    /// Record a successful report read.
    pub fn record_input_report(&mut self, data: &[u8]) {
        self.last_input_report = Some(data.to_vec());
        self.last_read = Some(std::time::Instant::now());
    }

    /// Record an I/O error.
    pub fn record_io_error(&mut self) {
        self.io_error_count = self.io_error_count.saturating_add(1);
    }

    // -- Report helpers --

    /// Classify the last received input report.
    pub fn classify_last_report(&self) -> Option<ReportKind> {
        self.last_input_report.as_deref().and_then(classify_report)
    }

    /// Validate that the given data has the expected minimum length.
    pub fn validate_report(&self, data: &[u8], min_len: usize) -> Result<(), MacHidError> {
        validate_report_length(data, min_len)
    }

    /// Prepare an output report in the pre-allocated buffer.
    ///
    /// Copies `data` into the internal buffer and returns the buffer length
    /// actually used. This avoids heap allocation in the RT path.
    pub fn prepare_output_report(&mut self, data: &[u8]) -> Result<usize, MacHidError> {
        if data.len() > MAX_HID_REPORT_SIZE {
            return Err(MacHidError::MalformedReport {
                expected_min: 0,
                actual: data.len(),
            });
        }
        self.report_buffer[..data.len()].copy_from_slice(data);
        // Zero-fill remainder
        for b in &mut self.report_buffer[data.len()..] {
            *b = 0;
        }
        Ok(data.len())
    }
}

// ---------------------------------------------------------------------------
// IOKit FFI-backed device operations (macOS only)
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
pub mod ffi_device {
    use super::*;
    use crate::hid::macos::iokit_ffi::{self, *};
    use crate::hid::macos::{HIDElement, IOHIDElementType, MacHidError};
    use std::ffi::c_void;

    /// Open an IOKit HID device for exclusive access.
    pub fn open_device(device_ref: IOHIDDeviceRef) -> Result<(), MacHidError> {
        let ret = unsafe { IOHIDDeviceOpen(device_ref, K_IOHID_OPTIONS_TYPE_SEIZE_DEVICE) };
        iokit_ffi::check_io_return(ret)
    }

    /// Close a previously opened IOKit HID device.
    pub fn close_device(device_ref: IOHIDDeviceRef) -> Result<(), MacHidError> {
        let ret = unsafe { IOHIDDeviceClose(device_ref, K_IOHID_OPTIONS_TYPE_NONE) };
        iokit_ffi::check_io_return(ret)
    }

    /// Send an output (or feature) report to the device.
    pub fn write_report(
        device_ref: IOHIDDeviceRef,
        report_type: IOHIDReportType,
        report_id: u8,
        data: &[u8],
    ) -> Result<(), MacHidError> {
        let ret = unsafe {
            IOHIDDeviceSetReport(
                device_ref,
                report_type,
                report_id as CFIndex,
                data.as_ptr(),
                data.len() as CFIndex,
            )
        };
        iokit_ffi::check_io_return(ret)
    }

    /// Read a feature report from the device (synchronous).
    pub fn read_feature_report(
        device_ref: IOHIDDeviceRef,
        report_id: u8,
        buf: &mut [u8],
    ) -> Result<usize, MacHidError> {
        let mut length = buf.len() as CFIndex;
        let ret = unsafe {
            IOHIDDeviceGetReport(
                device_ref,
                IOHIDReportType::Feature,
                report_id as CFIndex,
                buf.as_mut_ptr(),
                &mut length,
            )
        };
        iokit_ffi::check_io_return(ret)?;
        Ok(length as usize)
    }

    /// Read the device descriptor by querying IOKit properties and elements.
    pub fn read_descriptor(
        device_ref: IOHIDDeviceRef,
    ) -> Result<IOKitDeviceDescriptor, MacHidError> {
        let vid = iokit_ffi::device_int_property(device_ref, K_IOHID_VENDOR_ID_KEY)
            .ok_or(MacHidError::IOReturn(K_IO_RETURN_ERROR))? as u16;
        let pid = iokit_ffi::device_int_property(device_ref, K_IOHID_PRODUCT_ID_KEY)
            .ok_or(MacHidError::IOReturn(K_IO_RETURN_ERROR))? as u16;
        let version = iokit_ffi::device_int_property(device_ref, K_IOHID_VERSION_NUMBER_KEY)
            .unwrap_or(0) as u16;
        let location_id =
            iokit_ffi::device_int_property(device_ref, K_IOHID_LOCATION_ID_KEY).unwrap_or(0) as u32;
        let primary_usage_page =
            iokit_ffi::device_int_property(device_ref, K_IOHID_PRIMARY_USAGE_PAGE_KEY).unwrap_or(0)
                as u32;
        let primary_usage = iokit_ffi::device_int_property(device_ref, K_IOHID_PRIMARY_USAGE_KEY)
            .unwrap_or(0) as u32;

        let manufacturer = iokit_ffi::device_string_property(device_ref, K_IOHID_MANUFACTURER_KEY);
        let product = iokit_ffi::device_string_property(device_ref, K_IOHID_PRODUCT_KEY);
        let serial_number =
            iokit_ffi::device_string_property(device_ref, K_IOHID_SERIAL_NUMBER_KEY);
        let transport = iokit_ffi::device_string_property(device_ref, K_IOHID_TRANSPORT_KEY);

        // Enumerate HID elements
        let elements = enumerate_elements(device_ref);

        Ok(IOKitDeviceDescriptor {
            vendor_id: vid,
            product_id: pid,
            version_number: version,
            manufacturer,
            product,
            serial_number,
            transport,
            primary_usage_page,
            primary_usage,
            elements,
            location_id,
        })
    }

    /// Enumerate all HID elements on a device.
    fn enumerate_elements(device_ref: IOHIDDeviceRef) -> Vec<HIDElement> {
        let mut elements = Vec::new();
        let cf_array = unsafe {
            IOHIDDeviceCopyMatchingElements(device_ref, std::ptr::null(), K_IOHID_OPTIONS_TYPE_NONE)
        };
        if cf_array.is_null() {
            return elements;
        }

        let count = unsafe { CFArrayGetCount(cf_array) };
        for i in 0..count {
            let elem_ref = unsafe { CFArrayGetValueAtIndex(cf_array, i) } as IOHIDElementRef;
            if elem_ref.is_null() {
                continue;
            }
            let raw_type = unsafe { IOHIDElementGetType(elem_ref) };
            let Some(element_type) = IOHIDElementType::from_raw(raw_type) else {
                continue;
            };

            elements.push(HIDElement {
                element_type,
                usage_page: unsafe { IOHIDElementGetUsagePage(elem_ref) },
                usage: unsafe { IOHIDElementGetUsage(elem_ref) },
                logical_min: unsafe { IOHIDElementGetLogicalMin(elem_ref) } as i64,
                logical_max: unsafe { IOHIDElementGetLogicalMax(elem_ref) } as i64,
                physical_min: unsafe { IOHIDElementGetPhysicalMin(elem_ref) } as i64,
                physical_max: unsafe { IOHIDElementGetPhysicalMax(elem_ref) } as i64,
                report_size: unsafe { IOHIDElementGetReportSize(elem_ref) },
                report_count: unsafe { IOHIDElementGetReportCount(elem_ref) },
                report_id: unsafe { IOHIDElementGetReportID(elem_ref) },
            });
        }

        unsafe { CFRelease(cf_array as CFTypeRef) };
        elements
    }

    /// Register an input report callback on the device.
    pub fn register_input_callback(
        device_ref: IOHIDDeviceRef,
        buffer: &mut [u8; MAX_HID_REPORT_SIZE],
        callback: IOHIDReportCallback,
        context: *mut c_void,
    ) {
        unsafe {
            IOHIDDeviceRegisterInputReportCallback(
                device_ref,
                buffer.as_mut_ptr(),
                buffer.len() as CFIndex,
                callback,
                context,
            );
        }
    }

    /// Schedule the device on the current run loop.
    pub fn schedule_with_run_loop(device_ref: IOHIDDeviceRef) {
        unsafe {
            let rl = CFRunLoopGetCurrent();
            IOHIDDeviceScheduleWithRunLoop(device_ref, rl, kCFRunLoopDefaultMode);
        }
    }

    /// Unschedule the device from the current run loop.
    pub fn unschedule_from_run_loop(device_ref: IOHIDDeviceRef) {
        unsafe {
            let rl = CFRunLoopGetCurrent();
            IOHIDDeviceUnscheduleFromRunLoop(device_ref, rl, kCFRunLoopDefaultMode);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (platform-agnostic, using mock data)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::macos::{HIDElement, IOHIDElementType, usage, usage_page};

    fn make_test_descriptor() -> IOKitDeviceDescriptor {
        IOKitDeviceDescriptor {
            vendor_id: 0x346E,
            product_id: 0x0004,
            version_number: 0x0100,
            manufacturer: Some("Moza".to_string()),
            product: Some("R5".to_string()),
            serial_number: Some("SN001".to_string()),
            transport: Some("USB".to_string()),
            primary_usage_page: usage_page::GENERIC_DESKTOP,
            primary_usage: usage::WHEEL,
            location_id: 0x14100000,
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

    #[test]
    fn test_device_handle_initial_state() -> Result<(), Box<dyn std::error::Error>> {
        let handle = MacOSDeviceHandle::new(make_test_descriptor());
        assert_eq!(handle.state(), DeviceState::Discovered);
        assert_eq!(handle.vendor_id(), 0x346E);
        assert_eq!(handle.product_id(), 0x0004);
        assert_eq!(handle.io_error_count(), 0);
        assert!(handle.last_read_time().is_none());
        assert!(handle.last_input_report().is_none());
        Ok(())
    }

    #[test]
    fn test_device_handle_state_transitions() -> Result<(), Box<dyn std::error::Error>> {
        let mut handle = MacOSDeviceHandle::new(make_test_descriptor());
        assert_eq!(handle.state(), DeviceState::Discovered);

        handle.mark_open()?;
        assert_eq!(handle.state(), DeviceState::Open);

        handle.mark_closed();
        assert_eq!(handle.state(), DeviceState::Closed);
        Ok(())
    }

    #[test]
    fn test_device_handle_open_from_invalid_state() -> Result<(), Box<dyn std::error::Error>> {
        let mut handle = MacOSDeviceHandle::new(make_test_descriptor());
        handle.mark_open()?;
        handle.mark_closed();

        let result = handle.mark_open();
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_device_handle_record_input_report() -> Result<(), Box<dyn std::error::Error>> {
        let mut handle = MacOSDeviceHandle::new(make_test_descriptor());
        handle.record_input_report(&[0x02, 0xAA, 0xBB]);

        let report = handle.last_input_report().ok_or("no report")?;
        assert_eq!(report, &[0x02, 0xAA, 0xBB]);
        assert!(handle.last_read_time().is_some());
        Ok(())
    }

    #[test]
    fn test_device_handle_classify_last_report() -> Result<(), Box<dyn std::error::Error>> {
        let mut handle = MacOSDeviceHandle::new(make_test_descriptor());
        handle.record_input_report(&[0x21, 0x01, 0x02]);

        let kind = handle.classify_last_report().ok_or("no classification")?;
        assert_eq!(kind, ReportKind::Telemetry);
        Ok(())
    }

    #[test]
    fn test_device_handle_prepare_output_report() -> Result<(), Box<dyn std::error::Error>> {
        let mut handle = MacOSDeviceHandle::new(make_test_descriptor());
        let len = handle.prepare_output_report(&[0x20, 0x01, 0x02])?;
        assert_eq!(len, 3);
        assert_eq!(&handle.report_buffer()[..3], &[0x20, 0x01, 0x02]);
        // Remainder is zeroed
        assert!(handle.report_buffer()[3..].iter().all(|&b| b == 0));
        Ok(())
    }

    #[test]
    fn test_device_handle_prepare_output_report_too_large() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut handle = MacOSDeviceHandle::new(make_test_descriptor());
        let oversized = vec![0xAA; MAX_HID_REPORT_SIZE + 1];
        let result = handle.prepare_output_report(&oversized);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_device_handle_io_error_tracking() -> Result<(), Box<dyn std::error::Error>> {
        let mut handle = MacOSDeviceHandle::new(make_test_descriptor());
        assert_eq!(handle.io_error_count(), 0);

        handle.record_io_error();
        assert_eq!(handle.io_error_count(), 1);

        handle.record_io_error();
        assert_eq!(handle.io_error_count(), 2);
        Ok(())
    }

    #[test]
    fn test_device_handle_mark_error() -> Result<(), Box<dyn std::error::Error>> {
        let mut handle = MacOSDeviceHandle::new(make_test_descriptor());
        handle.mark_error();
        assert_eq!(handle.state(), DeviceState::Error);
        Ok(())
    }

    #[test]
    fn test_device_state_display() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(format!("{}", DeviceState::Discovered), "discovered");
        assert_eq!(format!("{}", DeviceState::Open), "open");
        assert_eq!(format!("{}", DeviceState::Closed), "closed");
        assert_eq!(format!("{}", DeviceState::Error), "error");
        Ok(())
    }

    #[test]
    fn test_device_handle_path() -> Result<(), Box<dyn std::error::Error>> {
        let handle = MacOSDeviceHandle::new(make_test_descriptor());
        let path = handle.device_path();
        assert!(path.contains("14100000"));
        Ok(())
    }

    #[test]
    fn test_device_handle_validate_report() -> Result<(), Box<dyn std::error::Error>> {
        let handle = MacOSDeviceHandle::new(make_test_descriptor());
        handle.validate_report(&[0x01, 0x02, 0x03], 3)?;
        let result = handle.validate_report(&[0x01], 3);
        assert!(result.is_err());
        Ok(())
    }
}
