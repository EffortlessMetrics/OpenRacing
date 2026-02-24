//! Virtual Fanatec device for integration and e2e testing.
//!
//! `VirtualFanatecDevice` implements `DeviceWriter` so protocol code can be tested
//! without real hardware. It records feature reports and output reports in order
//! and supports disconnect/reconnect simulation.

use racing_wheel_engine::hid::vendor::fanatec::FanatecProtocol;
use racing_wheel_hid_fanatec_protocol::FANATEC_VENDOR_ID;
use racing_wheel_hid_moza_protocol::{DeviceWriter, VendorProtocol};
use std::collections::VecDeque;

/// Maximum output report history retained by the virtual device.
pub const MAX_OUTPUT_HISTORY: usize = 16;

/// A software stand-in for a Fanatec HID device used in integration tests.
///
/// Records all feature reports and output reports written to it so tests
/// can assert on the exact bytes sent.
pub struct VirtualFanatecDevice {
    pub product_id: u16,
    pub vendor_id: u16,
    feature_reports: Vec<Vec<u8>>,
    output_reports: VecDeque<Vec<u8>>,
    connected: bool,
    fail_writes: bool,
}

impl VirtualFanatecDevice {
    pub fn new(product_id: u16) -> Self {
        Self {
            product_id,
            vendor_id: FANATEC_VENDOR_ID,
            feature_reports: Vec::new(),
            output_reports: VecDeque::new(),
            connected: true,
            fail_writes: false,
        }
    }

    /// Create a device that fails all write operations (simulates I/O errors).
    pub fn new_failing(product_id: u16) -> Self {
        let mut d = Self::new(product_id);
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

    /// Last output report written, if any.
    pub fn last_output_report(&self) -> Option<&Vec<u8>> {
        self.output_reports.back()
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

    /// Clear all recorded reports (useful for testing idempotency).
    pub fn clear_records(&mut self) {
        self.feature_reports.clear();
        self.output_reports.clear();
    }
}

impl DeviceWriter for VirtualFanatecDevice {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_writes {
            return Err("VirtualFanatecDevice: simulated write failure".into());
        }
        let len = data.len();
        self.feature_reports.push(data.to_vec());
        Ok(len)
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_writes {
            return Err("VirtualFanatecDevice: simulated write failure".into());
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
pub struct FanatecScenario {
    pub protocol: FanatecProtocol,
    pub device: VirtualFanatecDevice,
}

impl FanatecScenario {
    /// Create a scenario for a known wheelbase product.
    pub fn wheelbase(product_id: u16) -> Self {
        Self {
            protocol: FanatecProtocol::new(FANATEC_VENDOR_ID, product_id),
            device: VirtualFanatecDevice::new(product_id),
        }
    }

    /// Create a scenario with a failing device (simulates I/O errors).
    pub fn wheelbase_failing(product_id: u16) -> Self {
        Self {
            protocol: FanatecProtocol::new(FANATEC_VENDOR_ID, product_id),
            device: VirtualFanatecDevice::new_failing(product_id),
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
