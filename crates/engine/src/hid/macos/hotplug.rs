//! macOS IOKit hot-plug detection via HID Manager callbacks.
//!
//! The hot-plug monitor uses `IOHIDManager` device matching/removal callbacks
//! on a dedicated `CFRunLoop` thread to detect device arrivals and departures.
//!
//! Platform-agnostic event types and the callback dispatcher compile on all
//! platforms; actual IOKit scheduling is `#[cfg(target_os = "macos")]` only.

use super::IOKitDeviceDescriptor;
use std::fmt;

// ---------------------------------------------------------------------------
// Hot-plug event types (platform-agnostic)
// ---------------------------------------------------------------------------

/// Event emitted by the hot-plug monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotplugEvent {
    /// A new device matching our criteria was connected.
    DeviceArrived {
        vendor_id: u16,
        product_id: u16,
        path: String,
    },
    /// A previously known device was disconnected.
    DeviceRemoved { path: String },
}

impl fmt::Display for HotplugEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceArrived {
                vendor_id,
                product_id,
                path,
            } => write!(
                f,
                "device arrived: {:04X}:{:04X} at {path}",
                vendor_id, product_id,
            ),
            Self::DeviceRemoved { path } => write!(f, "device removed: {path}"),
        }
    }
}

impl HotplugEvent {
    /// Create an arrival event from a device descriptor.
    pub fn arrived(desc: &IOKitDeviceDescriptor) -> Self {
        Self::DeviceArrived {
            vendor_id: desc.vendor_id,
            product_id: desc.product_id,
            path: desc.device_path(),
        }
    }

    /// Create a removal event from a device path.
    pub fn removed(path: impl Into<String>) -> Self {
        Self::DeviceRemoved { path: path.into() }
    }

    /// Whether this is an arrival event.
    pub fn is_arrival(&self) -> bool {
        matches!(self, Self::DeviceArrived { .. })
    }

    /// Whether this is a removal event.
    pub fn is_removal(&self) -> bool {
        matches!(self, Self::DeviceRemoved { .. })
    }

    /// Get the device path for this event.
    pub fn path(&self) -> &str {
        match self {
            Self::DeviceArrived { path, .. } | Self::DeviceRemoved { path } => path,
        }
    }
}

// ---------------------------------------------------------------------------
// Hot-plug event collector (platform-agnostic, for testing)
// ---------------------------------------------------------------------------

/// Simple event collector for testing hot-plug event sequences.
#[derive(Debug, Default)]
pub struct HotplugEventCollector {
    events: Vec<HotplugEvent>,
}

impl HotplugEventCollector {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn push(&mut self, event: HotplugEvent) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[HotplugEvent] {
        &self.events
    }

    pub fn arrival_count(&self) -> usize {
        self.events.iter().filter(|e| e.is_arrival()).count()
    }

    pub fn removal_count(&self) -> usize {
        self.events.iter().filter(|e| e.is_removal()).count()
    }

    pub fn total_count(&self) -> usize {
        self.events.len()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Find the last event for a specific path.
    pub fn last_event_for_path(&self, path: &str) -> Option<&HotplugEvent> {
        self.events.iter().rev().find(|e| e.path() == path)
    }
}

// ---------------------------------------------------------------------------
// Hot-plug monitor state (platform-agnostic)
// ---------------------------------------------------------------------------

/// State of the hot-plug monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorState {
    /// Monitor has been created but not started.
    Idle,
    /// Monitor is running and watching for device changes.
    Running,
    /// Monitor has been stopped.
    Stopped,
    /// Monitor encountered an error.
    Error,
}

impl fmt::Display for MonitorState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Running => write!(f, "running"),
            Self::Stopped => write!(f, "stopped"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Platform-agnostic handle for the hot-plug monitor configuration.
pub struct HotplugMonitorConfig {
    /// Whether to match racing wheel devices only.
    pub racing_wheels_only: bool,
    /// Optional vendor/product filter.
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
}

impl HotplugMonitorConfig {
    /// Default configuration: match racing wheels only.
    pub fn racing_wheels() -> Self {
        Self {
            racing_wheels_only: true,
            vendor_id: None,
            product_id: None,
        }
    }

    /// Match a specific vendor+product.
    pub fn specific_device(vid: u16, pid: u16) -> Self {
        Self {
            racing_wheels_only: false,
            vendor_id: Some(vid),
            product_id: Some(pid),
        }
    }

    /// Match all HID devices.
    pub fn all_devices() -> Self {
        Self {
            racing_wheels_only: false,
            vendor_id: None,
            product_id: None,
        }
    }
}

impl Default for HotplugMonitorConfig {
    fn default() -> Self {
        Self::racing_wheels()
    }
}

// ---------------------------------------------------------------------------
// IOKit-backed hot-plug monitor (macOS only)
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
pub mod ffi_monitor {
    use super::*;
    use crate::hid::macos::device::ffi_device;
    use crate::hid::macos::iokit_ffi::{self, *};
    use crate::hid::macos::{MacHidError, usage, usage_page};
    use std::ffi::c_void;
    use std::sync::{Arc, Mutex};

    /// Wrapper around `CFRunLoopRef` to implement `Send`.
    ///
    /// Safety: `CFRunLoopStop` is documented as thread-safe by Apple, and that
    /// is the only cross-thread operation we perform with this pointer.
    struct SendableCFRunLoopRef(CFRunLoopRef);
    // SAFETY: CFRunLoopStop is thread-safe per Apple docs.
    unsafe impl Send for SendableCFRunLoopRef {}

    /// IOKit-backed hot-plug monitor that runs a CFRunLoop on a background thread.
    pub struct IOKitHotplugMonitor {
        state: Arc<Mutex<MonitorState>>,
        events: Arc<Mutex<Vec<HotplugEvent>>>,
        run_loop_ref: Arc<Mutex<Option<SendableCFRunLoopRef>>>,
    }

    // Safety: The CFRunLoopRef is only used via CFRunLoopStop which is thread-safe.
    unsafe impl Send for IOKitHotplugMonitor {}
    unsafe impl Sync for IOKitHotplugMonitor {}

    impl IOKitHotplugMonitor {
        /// Create a new monitor (does not start it).
        pub fn new() -> Self {
            Self {
                state: Arc::new(Mutex::new(MonitorState::Idle)),
                events: Arc::new(Mutex::new(Vec::new())),
                run_loop_ref: Arc::new(Mutex::new(None)),
            }
        }

        /// Start the monitor on a background thread.
        pub fn start(&self, config: HotplugMonitorConfig) -> Result<(), MacHidError> {
            let state = Arc::clone(&self.state);
            let events = Arc::clone(&self.events);
            let run_loop_store = Arc::clone(&self.run_loop_ref);

            std::thread::spawn(move || {
                unsafe {
                    let manager =
                        IOHIDManagerCreate(kCFAllocatorDefault, K_IOHID_MANAGER_OPTION_NONE);
                    if manager.is_null() {
                        let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                        *s = MonitorState::Error;
                        return;
                    }

                    // Set matching
                    if config.racing_wheels_only {
                        if let Some(dict) = iokit_ffi::matching_dict_for_usage(
                            usage_page::GENERIC_DESKTOP,
                            usage::WHEEL,
                        ) {
                            IOHIDManagerSetDeviceMatching(
                                manager,
                                dict.as_ptr() as CFDictionaryRef,
                            );
                        }
                    } else if let (Some(vid), Some(pid)) = (config.vendor_id, config.product_id) {
                        if let Some(dict) = iokit_ffi::matching_dict_for_vid_pid(vid, pid) {
                            IOHIDManagerSetDeviceMatching(
                                manager,
                                dict.as_ptr() as CFDictionaryRef,
                            );
                        }
                    } else {
                        IOHIDManagerSetDeviceMatching(manager, std::ptr::null());
                    }

                    // Register callbacks
                    let ctx = Box::into_raw(Box::new(Arc::clone(&events))) as *mut c_void;
                    IOHIDManagerRegisterDeviceMatchingCallback(
                        manager,
                        device_matched_callback,
                        ctx,
                    );
                    IOHIDManagerRegisterDeviceRemovalCallback(
                        manager,
                        device_removed_callback,
                        ctx,
                    );

                    // Schedule on run loop
                    let rl = CFRunLoopGetCurrent();
                    IOHIDManagerScheduleWithRunLoop(manager, rl, kCFRunLoopDefaultMode);

                    // Store the run loop ref for stopping later
                    {
                        let mut store = run_loop_store.lock().unwrap_or_else(|e| e.into_inner());
                        *store = Some(SendableCFRunLoopRef(rl));
                    }

                    // Open manager
                    let ret = IOHIDManagerOpen(manager, K_IOHID_OPTIONS_TYPE_NONE);
                    if ret != K_IO_RETURN_SUCCESS {
                        let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                        *s = MonitorState::Error;
                        CFRelease(manager as CFTypeRef);
                        return;
                    }

                    {
                        let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                        *s = MonitorState::Running;
                    }

                    // Run the loop (blocks until stopped)
                    CFRunLoopRun();

                    // Cleanup
                    IOHIDManagerUnscheduleFromRunLoop(manager, rl, kCFRunLoopDefaultMode);
                    let _ = IOHIDManagerClose(manager, K_IOHID_OPTIONS_TYPE_NONE);
                    CFRelease(manager as CFTypeRef);

                    // Clean up context
                    let _ = Box::from_raw(ctx as *mut Arc<Mutex<Vec<HotplugEvent>>>);

                    {
                        let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                        *s = MonitorState::Stopped;
                    }
                }
            });

            Ok(())
        }

        /// Stop the monitor.
        pub fn stop(&self) {
            let rl = {
                let store = self.run_loop_ref.lock().unwrap_or_else(|e| e.into_inner());
                store.as_ref().map(|s| s.0)
            };
            if let Some(rl) = rl {
                unsafe { CFRunLoopStop(rl) };
            }
        }

        /// Get the current state.
        pub fn state(&self) -> MonitorState {
            let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            *s
        }

        /// Drain all pending events.
        pub fn drain_events(&self) -> Vec<HotplugEvent> {
            let mut events = self.events.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::take(&mut *events)
        }
    }

    /// Callback fired when a matching device is connected.
    unsafe extern "C" fn device_matched_callback(
        context: *mut c_void,
        _result: IOReturn,
        _sender: *mut c_void,
        device: IOHIDDeviceRef,
    ) {
        if context.is_null() || device.is_null() {
            return;
        }
        // SAFETY: `context` was created via `Box::into_raw(Box::new(Arc::clone(…)))` in `start`.
        let events = unsafe { &*(context as *const Arc<Mutex<Vec<HotplugEvent>>>) };

        let vid = iokit_ffi::device_int_property(device, K_IOHID_VENDOR_ID_KEY).unwrap_or(0);
        let pid = iokit_ffi::device_int_property(device, K_IOHID_PRODUCT_ID_KEY).unwrap_or(0);
        let location = iokit_ffi::device_int_property(device, K_IOHID_LOCATION_ID_KEY).unwrap_or(0);
        let path = format!("IOService:/AppleUSBDevice@{:08X}", location);

        let event = HotplugEvent::DeviceArrived {
            vendor_id: vid as u16,
            product_id: pid as u16,
            path,
        };

        let mut ev = events.lock().unwrap_or_else(|e| e.into_inner());
        ev.push(event);
    }

    /// Callback fired when a matching device is disconnected.
    unsafe extern "C" fn device_removed_callback(
        context: *mut c_void,
        _result: IOReturn,
        _sender: *mut c_void,
        device: IOHIDDeviceRef,
    ) {
        if context.is_null() || device.is_null() {
            return;
        }
        // SAFETY: `context` was created via `Box::into_raw(Box::new(Arc::clone(…)))` in `start`.
        let events = unsafe { &*(context as *const Arc<Mutex<Vec<HotplugEvent>>>) };

        let location = iokit_ffi::device_int_property(device, K_IOHID_LOCATION_ID_KEY).unwrap_or(0);
        let path = format!("IOService:/AppleUSBDevice@{:08X}", location);

        let event = HotplugEvent::DeviceRemoved { path };

        let mut ev = events.lock().unwrap_or_else(|e| e.into_inner());
        ev.push(event);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::macos::{IOKitDeviceDescriptor, usage, usage_page};

    fn make_test_descriptor(vid: u16, pid: u16, location: u32) -> IOKitDeviceDescriptor {
        IOKitDeviceDescriptor {
            vendor_id: vid,
            product_id: pid,
            version_number: 0x0100,
            manufacturer: Some("Test".to_string()),
            product: Some("Wheel".to_string()),
            serial_number: None,
            transport: Some("USB".to_string()),
            primary_usage_page: usage_page::GENERIC_DESKTOP,
            primary_usage: usage::WHEEL,
            location_id: location,
            elements: vec![],
        }
    }

    // -- HotplugEvent tests --

    #[test]
    fn test_event_arrived() -> Result<(), Box<dyn std::error::Error>> {
        let desc = make_test_descriptor(0x346E, 0x0004, 0x1000);
        let event = HotplugEvent::arrived(&desc);
        assert!(event.is_arrival());
        assert!(!event.is_removal());
        assert!(event.path().contains("00001000"));
        Ok(())
    }

    #[test]
    fn test_event_removed() -> Result<(), Box<dyn std::error::Error>> {
        let event = HotplugEvent::removed("IOService:/AppleUSBDevice@00001000");
        assert!(event.is_removal());
        assert!(!event.is_arrival());
        assert_eq!(event.path(), "IOService:/AppleUSBDevice@00001000");
        Ok(())
    }

    #[test]
    fn test_event_display_arrived() -> Result<(), Box<dyn std::error::Error>> {
        let event = HotplugEvent::DeviceArrived {
            vendor_id: 0x346E,
            product_id: 0x0004,
            path: "IOService:/dev0".to_string(),
        };
        let msg = format!("{event}");
        assert!(msg.contains("346E"));
        assert!(msg.contains("0004"));
        assert!(msg.contains("arrived"));
        Ok(())
    }

    #[test]
    fn test_event_display_removed() -> Result<(), Box<dyn std::error::Error>> {
        let event = HotplugEvent::DeviceRemoved {
            path: "IOService:/dev0".to_string(),
        };
        let msg = format!("{event}");
        assert!(msg.contains("removed"));
        assert!(msg.contains("IOService:/dev0"));
        Ok(())
    }

    // -- HotplugEventCollector tests --

    #[test]
    fn test_collector_empty() -> Result<(), Box<dyn std::error::Error>> {
        let collector = HotplugEventCollector::new();
        assert_eq!(collector.total_count(), 0);
        assert_eq!(collector.arrival_count(), 0);
        assert_eq!(collector.removal_count(), 0);
        Ok(())
    }

    #[test]
    fn test_collector_push_and_count() -> Result<(), Box<dyn std::error::Error>> {
        let mut collector = HotplugEventCollector::new();
        let desc = make_test_descriptor(0x346E, 0x0004, 0x1000);

        collector.push(HotplugEvent::arrived(&desc));
        collector.push(HotplugEvent::removed(desc.device_path()));
        collector.push(HotplugEvent::arrived(&desc));

        assert_eq!(collector.total_count(), 3);
        assert_eq!(collector.arrival_count(), 2);
        assert_eq!(collector.removal_count(), 1);
        Ok(())
    }

    #[test]
    fn test_collector_last_event_for_path() -> Result<(), Box<dyn std::error::Error>> {
        let mut collector = HotplugEventCollector::new();
        let desc = make_test_descriptor(0x346E, 0x0004, 0x1000);
        let path = desc.device_path();

        collector.push(HotplugEvent::arrived(&desc));
        collector.push(HotplugEvent::removed(&path));

        let last = collector
            .last_event_for_path(&path)
            .ok_or("no event found")?;
        assert!(last.is_removal());
        Ok(())
    }

    #[test]
    fn test_collector_clear() -> Result<(), Box<dyn std::error::Error>> {
        let mut collector = HotplugEventCollector::new();
        collector.push(HotplugEvent::removed("path"));
        assert_eq!(collector.total_count(), 1);

        collector.clear();
        assert_eq!(collector.total_count(), 0);
        Ok(())
    }

    // -- MonitorState tests --

    #[test]
    fn test_monitor_state_display() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(format!("{}", MonitorState::Idle), "idle");
        assert_eq!(format!("{}", MonitorState::Running), "running");
        assert_eq!(format!("{}", MonitorState::Stopped), "stopped");
        assert_eq!(format!("{}", MonitorState::Error), "error");
        Ok(())
    }

    // -- HotplugMonitorConfig tests --

    #[test]
    fn test_config_racing_wheels() -> Result<(), Box<dyn std::error::Error>> {
        let config = HotplugMonitorConfig::racing_wheels();
        assert!(config.racing_wheels_only);
        assert!(config.vendor_id.is_none());
        assert!(config.product_id.is_none());
        Ok(())
    }

    #[test]
    fn test_config_specific_device() -> Result<(), Box<dyn std::error::Error>> {
        let config = HotplugMonitorConfig::specific_device(0x346E, 0x0004);
        assert!(!config.racing_wheels_only);
        assert_eq!(config.vendor_id, Some(0x346E));
        assert_eq!(config.product_id, Some(0x0004));
        Ok(())
    }

    #[test]
    fn test_config_all_devices() -> Result<(), Box<dyn std::error::Error>> {
        let config = HotplugMonitorConfig::all_devices();
        assert!(!config.racing_wheels_only);
        assert!(config.vendor_id.is_none());
        Ok(())
    }

    #[test]
    fn test_config_default() -> Result<(), Box<dyn std::error::Error>> {
        let config = HotplugMonitorConfig::default();
        assert!(config.racing_wheels_only);
        Ok(())
    }

    // -- Integration-style test with collector --

    #[test]
    fn test_hotplug_sequence_simulation() -> Result<(), Box<dyn std::error::Error>> {
        let mut collector = HotplugEventCollector::new();

        // Simulate: Moza wheel plugged in
        let moza = make_test_descriptor(0x346E, 0x0004, 0x1000);
        collector.push(HotplugEvent::arrived(&moza));

        // Simulate: Fanatec wheel plugged in
        let fanatec = make_test_descriptor(0x0EB7, 0x0024, 0x2000);
        collector.push(HotplugEvent::arrived(&fanatec));

        // Simulate: Moza unplugged
        collector.push(HotplugEvent::removed(moza.device_path()));

        assert_eq!(collector.arrival_count(), 2);
        assert_eq!(collector.removal_count(), 1);

        // The last event for Moza's path should be removal
        let last_moza = collector
            .last_event_for_path(&moza.device_path())
            .ok_or("missing moza event")?;
        assert!(last_moza.is_removal());

        // The last event for Fanatec's path should be arrival
        let last_fanatec = collector
            .last_event_for_path(&fanatec.device_path())
            .ok_or("missing fanatec event")?;
        assert!(last_fanatec.is_arrival());
        Ok(())
    }
}
