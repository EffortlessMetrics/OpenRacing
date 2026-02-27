//! Virtual FFBeast device for integration and e2e testing.
//!
//! `VirtualFFBeastDevice` implements `DeviceWriter` so protocol code can be
//! tested without real hardware. Records feature and output reports in order and
//! supports disconnect/reconnect simulation.

use racing_wheel_engine::hid::vendor::ffbeast::FFBeastHandler;
use racing_wheel_hid_ffbeast_protocol::{
    FFBEAST_PRODUCT_ID_JOYSTICK, FFBEAST_PRODUCT_ID_RUDDER, FFBEAST_PRODUCT_ID_WHEEL,
    FFBEAST_VENDOR_ID,
};
use racing_wheel_hid_moza_protocol::{DeviceWriter, VendorProtocol};
use std::collections::VecDeque;

/// Maximum output report history retained by the virtual device.
pub const MAX_OUTPUT_HISTORY: usize = 16;

/// A software stand-in for an FFBeast HID device used in integration tests.
pub struct VirtualFFBeastDevice {
    pub product_id: u16,
    pub vendor_id: u16,
    feature_reports: Vec<Vec<u8>>,
    output_reports: VecDeque<Vec<u8>>,
    connected: bool,
    fail_writes: bool,
}

impl VirtualFFBeastDevice {
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        Self {
            product_id,
            vendor_id,
            feature_reports: Vec::new(),
            output_reports: VecDeque::new(),
            connected: true,
            fail_writes: false,
        }
    }

    /// Create a device that fails all write operations (simulates I/O errors).
    pub fn new_failing(vendor_id: u16, product_id: u16) -> Self {
        let mut d = Self::new(vendor_id, product_id);
        d.fail_writes = true;
        d
    }

    /// Simulate a device disconnect (subsequent writes return errors).
    pub fn disconnect(&mut self) {
        self.connected = false;
        self.fail_writes = true;
    }

    /// Simulate device reconnect.
    pub fn reconnect(&mut self) {
        self.connected = true;
        self.fail_writes = false;
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// All feature reports written since creation, in order.
    pub fn feature_reports(&self) -> &[Vec<u8>] {
        &self.feature_reports
    }

    /// All output reports written since creation, in order.
    pub fn output_reports(&self) -> &VecDeque<Vec<u8>> {
        &self.output_reports
    }

    /// True when `feature_reports` contains a report whose first byte matches `report_id`.
    pub fn sent_feature_report_id(&self, report_id: u8) -> bool {
        self.feature_reports
            .iter()
            .any(|r| r.first().copied() == Some(report_id))
    }

    /// Return feature reports whose first byte matches `report_id`.
    pub fn feature_reports_with_id(&self, report_id: u8) -> Vec<&Vec<u8>> {
        self.feature_reports
            .iter()
            .filter(|r| r.first().copied() == Some(report_id))
            .collect()
    }

    /// Clear all recorded reports.
    pub fn clear_records(&mut self) {
        self.feature_reports.clear();
        self.output_reports.clear();
    }
}

impl DeviceWriter for VirtualFFBeastDevice {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_writes {
            return Err("VirtualFFBeastDevice: simulated write failure".into());
        }
        let len = data.len();
        self.feature_reports.push(data.to_vec());
        Ok(len)
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_writes {
            return Err("VirtualFFBeastDevice: simulated write failure".into());
        }
        let len = data.len();
        if self.output_reports.len() >= MAX_OUTPUT_HISTORY {
            self.output_reports.pop_front();
        }
        self.output_reports.push_back(data.to_vec());
        Ok(len)
    }
}

/// Helpers for BDD-style scenario setup.
pub struct FFBeastScenario {
    pub protocol: FFBeastHandler,
    pub device: VirtualFFBeastDevice,
}

impl FFBeastScenario {
    /// Create a wheel-PID scenario.
    pub fn wheel() -> Self {
        Self {
            protocol: FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_WHEEL),
            device: VirtualFFBeastDevice::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_WHEEL),
        }
    }

    /// Create a joystick-PID scenario.
    pub fn joystick() -> Self {
        Self {
            protocol: FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_JOYSTICK),
            device: VirtualFFBeastDevice::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_JOYSTICK),
        }
    }

    /// Create a rudder-PID scenario.
    pub fn rudder() -> Self {
        Self {
            protocol: FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_RUDDER),
            device: VirtualFFBeastDevice::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_RUDDER),
        }
    }

    /// Create a failing wheel scenario (simulates I/O errors).
    pub fn wheel_failing() -> Self {
        Self {
            protocol: FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_WHEEL),
            device: VirtualFFBeastDevice::new_failing(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_WHEEL),
        }
    }

    /// Run `initialize_device` and return whether it succeeded.
    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.protocol.initialize_device(&mut self.device)
    }

    /// Run `shutdown_device` and return whether it succeeded.
    pub fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.protocol.shutdown_device(&mut self.device)
    }
}

pub use racing_wheel_hid_ffbeast_protocol::{
    FFBEAST_PRODUCT_ID_JOYSTICK as PID_JOYSTICK,
    FFBEAST_PRODUCT_ID_RUDDER as PID_RUDDER,
    FFBEAST_PRODUCT_ID_WHEEL as PID_WHEEL,
    FFBEAST_VENDOR_ID as VENDOR_ID,
};
