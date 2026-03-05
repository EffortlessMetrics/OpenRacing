//! Safe Rust wrappers around macOS IOKit HID FFI.
//!
//! This module provides the raw C bindings and thin safe wrappers for the
//! IOKit HID Manager API. Everything here is `#[cfg(target_os = "macos")]`
//! at the module level (see `mod.rs`).

#![allow(non_camel_case_types, non_upper_case_globals, dead_code)]

use std::ffi::c_void;

// ---------------------------------------------------------------------------
// Core Foundation opaque types
// ---------------------------------------------------------------------------

pub type CFIndex = isize;
pub type CFTypeRef = *const c_void;
pub type CFAllocatorRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFNumberRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFMutableDictionaryRef = *mut c_void;
pub type CFSetRef = *const c_void;
pub type CFRunLoopRef = *const c_void;
pub type CFRunLoopMode = CFStringRef;
pub type CFArrayRef = *const c_void;
pub type CFTypeID = u64;

// ---------------------------------------------------------------------------
// IOKit HID opaque types
// ---------------------------------------------------------------------------

pub type IOHIDManagerRef = *mut c_void;
pub type IOHIDDeviceRef = *mut c_void;
pub type IOHIDElementRef = *mut c_void;
pub type IOHIDValueRef = *mut c_void;
pub type IOReturn = i32;
pub type IOOptionBits = u32;

// ---------------------------------------------------------------------------
// IOReturn status codes
// ---------------------------------------------------------------------------

pub const K_IO_RETURN_SUCCESS: IOReturn = 0;
pub const K_IO_RETURN_ERROR: IOReturn = 0x2bc;
pub const K_IO_RETURN_NO_DEVICE: IOReturn = 0x2c0;
pub const K_IO_RETURN_NOT_OPEN: IOReturn = 0x2cd;
pub const K_IO_RETURN_EXCLUSIVE_ACCESS: IOReturn = 0x2c5;
pub const K_IO_RETURN_NOT_PERMITTED: IOReturn = 0x2ce;
pub const K_IO_RETURN_BAD_ARGUMENT: IOReturn = 0x2c2;

// ---------------------------------------------------------------------------
// IOHIDManager options
// ---------------------------------------------------------------------------

pub const K_IOHID_MANAGER_OPTION_NONE: IOOptionBits = 0;
pub const K_IOHID_MANAGER_OPTION_USE_PERSISTENT_PROPERTIES: IOOptionBits = 1;

// ---------------------------------------------------------------------------
// IOHIDDevice open options
// ---------------------------------------------------------------------------

pub const K_IOHID_OPTIONS_TYPE_NONE: IOOptionBits = 0;
pub const K_IOHID_OPTIONS_TYPE_SEIZE_DEVICE: IOOptionBits = 1;

// ---------------------------------------------------------------------------
// IOHIDReportType
// ---------------------------------------------------------------------------

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IOHIDReportType {
    Input = 0,
    Output = 1,
    Feature = 2,
}

// ---------------------------------------------------------------------------
// CFNumber types
// ---------------------------------------------------------------------------

pub const K_CF_NUMBER_SINT32_TYPE: i32 = 3;
pub const K_CF_NUMBER_SINT64_TYPE: i32 = 4;

// ---------------------------------------------------------------------------
// IOKit property keys (matching IOKit C header constants)
// ---------------------------------------------------------------------------

pub const K_IOHID_VENDOR_ID_KEY: &str = "VendorID";
pub const K_IOHID_PRODUCT_ID_KEY: &str = "ProductID";
pub const K_IOHID_TRANSPORT_KEY: &str = "Transport";
pub const K_IOHID_MANUFACTURER_KEY: &str = "Manufacturer";
pub const K_IOHID_PRODUCT_KEY: &str = "Product";
pub const K_IOHID_SERIAL_NUMBER_KEY: &str = "SerialNumber";
pub const K_IOHID_VERSION_NUMBER_KEY: &str = "VersionNumber";
pub const K_IOHID_LOCATION_ID_KEY: &str = "LocationID";
pub const K_IOHID_PRIMARY_USAGE_PAGE_KEY: &str = "PrimaryUsagePage";
pub const K_IOHID_PRIMARY_USAGE_KEY: &str = "PrimaryUsage";
pub const K_IOHID_MAX_INPUT_REPORT_SIZE_KEY: &str = "MaxInputReportSize";
pub const K_IOHID_MAX_OUTPUT_REPORT_SIZE_KEY: &str = "MaxOutputReportSize";
pub const K_IOHID_MAX_FEATURE_REPORT_SIZE_KEY: &str = "MaxFeatureReportSize";
pub const K_IOHID_DEVICE_USAGE_PAGE_KEY: &str = "DeviceUsagePage";
pub const K_IOHID_DEVICE_USAGE_KEY: &str = "DeviceUsage";

// ---------------------------------------------------------------------------
// Callback type aliases
// ---------------------------------------------------------------------------

/// Callback fired when a matching device is added/removed.
pub type IOHIDDeviceCallback = unsafe extern "C" fn(
    context: *mut c_void,
    result: IOReturn,
    sender: *mut c_void,
    device: IOHIDDeviceRef,
);

/// Callback fired when an input report is received.
pub type IOHIDReportCallback = unsafe extern "C" fn(
    context: *mut c_void,
    result: IOReturn,
    sender: *mut c_void,
    report_type: IOHIDReportType,
    report_id: u32,
    report: *const u8,
    report_length: CFIndex,
);

/// Callback fired when an input value changes.
pub type IOHIDValueCallback = unsafe extern "C" fn(
    context: *mut c_void,
    result: IOReturn,
    sender: *mut c_void,
    value: IOHIDValueRef,
);

// ---------------------------------------------------------------------------
// Extern "C" declarations — linked via `-framework IOKit -framework CoreFoundation`
// ---------------------------------------------------------------------------

unsafe extern "C" {
    // -- Core Foundation --
    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFRetain(cf: CFTypeRef) -> CFTypeRef;

    pub fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    pub fn CFRunLoopRun();
    pub fn CFRunLoopStop(rl: CFRunLoopRef);

    pub static kCFRunLoopDefaultMode: CFRunLoopMode;

    pub fn CFDictionaryCreateMutable(
        allocator: CFAllocatorRef,
        capacity: CFIndex,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFMutableDictionaryRef;

    pub fn CFDictionarySetValue(dict: CFMutableDictionaryRef, key: CFTypeRef, value: CFTypeRef);
    pub fn CFDictionaryGetValue(dict: CFDictionaryRef, key: CFTypeRef) -> CFTypeRef;

    pub fn CFNumberCreate(
        allocator: CFAllocatorRef,
        the_type: i32,
        value_ptr: *const c_void,
    ) -> CFNumberRef;
    pub fn CFNumberGetValue(number: CFNumberRef, the_type: i32, value_ptr: *mut c_void) -> bool;

    pub fn CFStringCreateWithCString(
        allocator: CFAllocatorRef,
        c_str: *const i8,
        encoding: u32,
    ) -> CFStringRef;
    pub fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut i8,
        buffer_size: CFIndex,
        encoding: u32,
    ) -> bool;
    pub fn CFStringGetLength(the_string: CFStringRef) -> CFIndex;

    pub fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
    pub fn CFArrayGetValueAtIndex(array: CFArrayRef, idx: CFIndex) -> CFTypeRef;

    pub fn CFSetGetCount(set: CFSetRef) -> CFIndex;
    pub fn CFSetGetValues(set: CFSetRef, values: *mut CFTypeRef);

    pub static kCFTypeDictionaryKeyCallBacks: c_void;
    pub static kCFTypeDictionaryValueCallBacks: c_void;
    pub static kCFAllocatorDefault: CFAllocatorRef;

    // -- IOHIDManager --
    pub fn IOHIDManagerCreate(allocator: CFAllocatorRef, options: IOOptionBits) -> IOHIDManagerRef;

    pub fn IOHIDManagerSetDeviceMatching(manager: IOHIDManagerRef, matching: CFDictionaryRef);

    pub fn IOHIDManagerSetDeviceMatchingMultiple(manager: IOHIDManagerRef, multiple: CFArrayRef);

    pub fn IOHIDManagerOpen(manager: IOHIDManagerRef, options: IOOptionBits) -> IOReturn;

    pub fn IOHIDManagerClose(manager: IOHIDManagerRef, options: IOOptionBits) -> IOReturn;

    pub fn IOHIDManagerCopyDevices(manager: IOHIDManagerRef) -> CFSetRef;

    pub fn IOHIDManagerRegisterDeviceMatchingCallback(
        manager: IOHIDManagerRef,
        callback: IOHIDDeviceCallback,
        context: *mut c_void,
    );

    pub fn IOHIDManagerRegisterDeviceRemovalCallback(
        manager: IOHIDManagerRef,
        callback: IOHIDDeviceCallback,
        context: *mut c_void,
    );

    pub fn IOHIDManagerScheduleWithRunLoop(
        manager: IOHIDManagerRef,
        run_loop: CFRunLoopRef,
        run_loop_mode: CFRunLoopMode,
    );

    pub fn IOHIDManagerUnscheduleFromRunLoop(
        manager: IOHIDManagerRef,
        run_loop: CFRunLoopRef,
        run_loop_mode: CFRunLoopMode,
    );

    // -- IOHIDDevice --
    pub fn IOHIDDeviceOpen(device: IOHIDDeviceRef, options: IOOptionBits) -> IOReturn;
    pub fn IOHIDDeviceClose(device: IOHIDDeviceRef, options: IOOptionBits) -> IOReturn;

    pub fn IOHIDDeviceGetProperty(device: IOHIDDeviceRef, key: CFStringRef) -> CFTypeRef;
    pub fn IOHIDDeviceSetReport(
        device: IOHIDDeviceRef,
        report_type: IOHIDReportType,
        report_id: CFIndex,
        report: *const u8,
        report_length: CFIndex,
    ) -> IOReturn;
    pub fn IOHIDDeviceGetReport(
        device: IOHIDDeviceRef,
        report_type: IOHIDReportType,
        report_id: CFIndex,
        report: *mut u8,
        report_length: *mut CFIndex,
    ) -> IOReturn;

    pub fn IOHIDDeviceRegisterInputReportCallback(
        device: IOHIDDeviceRef,
        report: *mut u8,
        report_length: CFIndex,
        callback: IOHIDReportCallback,
        context: *mut c_void,
    );

    pub fn IOHIDDeviceRegisterRemovalCallback(
        device: IOHIDDeviceRef,
        callback: IOHIDDeviceCallback,
        context: *mut c_void,
    );

    pub fn IOHIDDeviceScheduleWithRunLoop(
        device: IOHIDDeviceRef,
        run_loop: CFRunLoopRef,
        run_loop_mode: CFRunLoopMode,
    );

    pub fn IOHIDDeviceUnscheduleFromRunLoop(
        device: IOHIDDeviceRef,
        run_loop: CFRunLoopRef,
        run_loop_mode: CFRunLoopMode,
    );

    pub fn IOHIDDeviceCopyMatchingElements(
        device: IOHIDDeviceRef,
        matching: CFDictionaryRef,
        options: IOOptionBits,
    ) -> CFArrayRef;

    // -- IOHIDElement --
    pub fn IOHIDElementGetType(element: IOHIDElementRef) -> u32;
    pub fn IOHIDElementGetUsagePage(element: IOHIDElementRef) -> u32;
    pub fn IOHIDElementGetUsage(element: IOHIDElementRef) -> u32;
    pub fn IOHIDElementGetLogicalMin(element: IOHIDElementRef) -> CFIndex;
    pub fn IOHIDElementGetLogicalMax(element: IOHIDElementRef) -> CFIndex;
    pub fn IOHIDElementGetPhysicalMin(element: IOHIDElementRef) -> CFIndex;
    pub fn IOHIDElementGetPhysicalMax(element: IOHIDElementRef) -> CFIndex;
    pub fn IOHIDElementGetReportSize(element: IOHIDElementRef) -> u32;
    pub fn IOHIDElementGetReportCount(element: IOHIDElementRef) -> u32;
    pub fn IOHIDElementGetReportID(element: IOHIDElementRef) -> u32;

    // -- IOHIDValue --
    pub fn IOHIDValueGetElement(value: IOHIDValueRef) -> IOHIDElementRef;
    pub fn IOHIDValueGetIntegerValue(value: IOHIDValueRef) -> CFIndex;
    pub fn IOHIDValueGetLength(value: IOHIDValueRef) -> CFIndex;
    pub fn IOHIDValueGetBytePtr(value: IOHIDValueRef) -> *const u8;
}

// ---------------------------------------------------------------------------
// Safe wrappers
// ---------------------------------------------------------------------------

/// RAII guard that calls `CFRelease` on drop.
pub struct CfType {
    ptr: CFTypeRef,
}

impl CfType {
    /// Wrap a non-null CF reference for automatic release.
    ///
    /// # Safety
    /// `ptr` must be a valid Core Foundation object that the caller owns.
    pub unsafe fn from_owned(ptr: CFTypeRef) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr })
        }
    }

    pub fn as_ptr(&self) -> CFTypeRef {
        self.ptr
    }
}

impl Drop for CfType {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { CFRelease(self.ptr) };
        }
    }
}

// CfType is not Send/Sync by default — IOKit objects are tied to the run loop
// thread that created them.

/// Create a `CFString` from a Rust `&str`.
///
/// Returns `None` if the C string conversion fails. The returned `CfType`
/// owns the CF reference.
pub fn cfstring_from_str(s: &str) -> Option<CfType> {
    use std::ffi::CString;
    let cs = CString::new(s).ok()?;
    let cf = unsafe {
        CFStringCreateWithCString(
            kCFAllocatorDefault,
            cs.as_ptr(),
            0x0600_0100, // kCFStringEncodingUTF8
        )
    };
    unsafe { CfType::from_owned(cf as CFTypeRef) }
}

/// Create a `CFNumber` from an `i32`.
pub fn cfnumber_from_i32(val: i32) -> Option<CfType> {
    let cf = unsafe {
        CFNumberCreate(
            kCFAllocatorDefault,
            K_CF_NUMBER_SINT32_TYPE,
            &val as *const i32 as *const c_void,
        )
    };
    unsafe { CfType::from_owned(cf as CFTypeRef) }
}

/// Read a `CFNumber` as an `i64`.
pub fn cfnumber_to_i64(num: CFNumberRef) -> Option<i64> {
    if num.is_null() {
        return None;
    }
    let mut val: i64 = 0;
    let ok = unsafe {
        CFNumberGetValue(
            num,
            K_CF_NUMBER_SINT64_TYPE,
            &mut val as *mut i64 as *mut c_void,
        )
    };
    if ok { Some(val) } else { None }
}

/// Read a string property from an `IOHIDDevice`.
pub fn device_string_property(device: IOHIDDeviceRef, key: &str) -> Option<String> {
    let cf_key = cfstring_from_str(key)?;
    let cf_val = unsafe { IOHIDDeviceGetProperty(device, cf_key.as_ptr() as CFStringRef) };
    if cf_val.is_null() {
        return None;
    }
    let mut buf = [0i8; 512];
    let ok = unsafe {
        CFStringGetCString(
            cf_val as CFStringRef,
            buf.as_mut_ptr(),
            buf.len() as CFIndex,
            0x0600_0100,
        )
    };
    if !ok {
        return None;
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(
        String::from_utf8_lossy(unsafe {
            std::slice::from_raw_parts(buf.as_ptr() as *const u8, len)
        })
        .into_owned(),
    )
}

/// Read an integer property from an `IOHIDDevice`.
pub fn device_int_property(device: IOHIDDeviceRef, key: &str) -> Option<i64> {
    let cf_key = cfstring_from_str(key)?;
    let cf_val = unsafe { IOHIDDeviceGetProperty(device, cf_key.as_ptr() as CFStringRef) };
    cfnumber_to_i64(cf_val as CFNumberRef)
}

/// Convert an `IOReturn` to a `Result`.
pub fn check_io_return(ret: IOReturn) -> Result<(), super::MacHidError> {
    if ret == K_IO_RETURN_SUCCESS {
        Ok(())
    } else {
        Err(super::MacHidError::IOReturn(ret))
    }
}

/// Build a `CFMutableDictionary` matching a specific VID/PID pair.
pub fn matching_dict_for_vid_pid(vid: u16, pid: u16) -> Option<CfType> {
    unsafe {
        let dict = CFDictionaryCreateMutable(
            kCFAllocatorDefault,
            2,
            &kCFTypeDictionaryKeyCallBacks as *const _ as *const c_void,
            &kCFTypeDictionaryValueCallBacks as *const _ as *const c_void,
        );
        if dict.is_null() {
            return None;
        }
        let key_vid = cfstring_from_str(K_IOHID_VENDOR_ID_KEY)?;
        let key_pid = cfstring_from_str(K_IOHID_PRODUCT_ID_KEY)?;
        let val_vid = cfnumber_from_i32(vid as i32)?;
        let val_pid = cfnumber_from_i32(pid as i32)?;
        CFDictionarySetValue(dict, key_vid.as_ptr(), val_vid.as_ptr());
        CFDictionarySetValue(dict, key_pid.as_ptr(), val_pid.as_ptr());
        CfType::from_owned(dict as CFTypeRef)
    }
}

/// Build a usage-page + usage matching dictionary.
pub fn matching_dict_for_usage(usage_page: u32, usage: u32) -> Option<CfType> {
    unsafe {
        let dict = CFDictionaryCreateMutable(
            kCFAllocatorDefault,
            2,
            &kCFTypeDictionaryKeyCallBacks as *const _ as *const c_void,
            &kCFTypeDictionaryValueCallBacks as *const _ as *const c_void,
        );
        if dict.is_null() {
            return None;
        }
        let key_page = cfstring_from_str(K_IOHID_DEVICE_USAGE_PAGE_KEY)?;
        let key_usage = cfstring_from_str(K_IOHID_DEVICE_USAGE_KEY)?;
        let val_page = cfnumber_from_i32(usage_page as i32)?;
        let val_usage = cfnumber_from_i32(usage as i32)?;
        CFDictionarySetValue(dict, key_page.as_ptr(), val_page.as_ptr());
        CFDictionarySetValue(dict, key_usage.as_ptr(), val_usage.as_ptr());
        CfType::from_owned(dict as CFTypeRef)
    }
}
