//! Virtual Moza device for integration and e2e testing.
//!
//! `VirtualMozaDevice` implements `DeviceWriter` so protocol code can be tested
//! without real hardware. It records feature reports and output reports in order
//! and supports disconnect/reconnect simulation.

use racing_wheel_hid_moza_protocol::{DeviceWriter, FfbMode, MozaProtocol, MozaRetryPolicy};
use std::collections::VecDeque;

/// Maximum torque write history retained by the virtual device.
pub const MAX_TORQUE_HISTORY: usize = 16;

/// A software stand-in for a Moza HID device used in integration tests.
///
/// Records all feature reports and output reports written to it so tests
/// can assert on the exact bytes sent.
pub struct VirtualMozaDevice {
    pub product_id: u16,
    pub vendor_id: u16,
    feature_reports: Vec<Vec<u8>>,
    output_reports: VecDeque<Vec<u8>>,
    input_report_queue: VecDeque<Vec<u8>>,
    connected: bool,
    pub tick: u32,
    fail_writes: bool,
}

impl VirtualMozaDevice {
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        Self {
            product_id,
            vendor_id,
            feature_reports: Vec::new(),
            output_reports: VecDeque::new(),
            input_report_queue: VecDeque::new(),
            connected: true,
            tick: 0,
            fail_writes: false,
        }
    }

    /// Create a device that fails all write operations (simulates I/O errors).
    pub fn new_failing(vendor_id: u16, product_id: u16) -> Self {
        let mut d = Self::new(vendor_id, product_id);
        d.fail_writes = true;
        d
    }

    /// Queue a raw input report to be returned by `next_input_report()`.
    pub fn enqueue_input_report(&mut self, report: Vec<u8>) {
        self.input_report_queue.push_back(report);
    }

    /// Dequeue the next pending input report, if any.
    pub fn next_input_report(&mut self) -> Option<Vec<u8>> {
        self.input_report_queue.pop_front()
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

    /// All output reports written since creation.
    pub fn output_reports(&self) -> &VecDeque<Vec<u8>> {
        &self.output_reports
    }

    /// Last output report written, if any.
    pub fn last_output_report(&self) -> Option<&Vec<u8>> {
        self.output_reports.back()
    }

    /// True when feature_reports contains a report whose first byte matches `report_id`.
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

impl DeviceWriter for VirtualMozaDevice {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_writes {
            return Err("VirtualMozaDevice: simulated write failure".into());
        }
        let len = data.len();
        self.feature_reports.push(data.to_vec());
        Ok(len)
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_writes {
            return Err("VirtualMozaDevice: simulated write failure".into());
        }
        let len = data.len();
        if self.output_reports.len() >= MAX_TORQUE_HISTORY {
            self.output_reports.pop_front();
        }
        self.output_reports.push_back(data.to_vec());
        Ok(len)
    }
}

/// Helpers for BDD-style scenario setup.
pub struct MozaScenario {
    pub protocol: MozaProtocol,
    pub device: VirtualMozaDevice,
}

impl MozaScenario {
    /// Create a scenario for a known wheelbase product with safe defaults.
    pub fn wheelbase(product_id: u16) -> Self {
        use racing_wheel_hid_moza_protocol::MOZA_VENDOR_ID;
        Self {
            protocol: MozaProtocol::new(product_id),
            device: VirtualMozaDevice::new(MOZA_VENDOR_ID, product_id),
        }
    }

    /// Create a scenario with explicit configuration.
    pub fn wheelbase_with_config(
        product_id: u16,
        ffb_mode: FfbMode,
        high_torque_enabled: bool,
    ) -> Self {
        use racing_wheel_hid_moza_protocol::MOZA_VENDOR_ID;
        Self {
            protocol: MozaProtocol::new_with_config(product_id, ffb_mode, high_torque_enabled),
            device: VirtualMozaDevice::new(MOZA_VENDOR_ID, product_id),
        }
    }

    /// Create a scenario with a failing device (simulates I/O errors).
    pub fn wheelbase_failing(product_id: u16) -> Self {
        use racing_wheel_hid_moza_protocol::MOZA_VENDOR_ID;
        Self {
            protocol: MozaProtocol::new(product_id),
            device: VirtualMozaDevice::new_failing(MOZA_VENDOR_ID, product_id),
        }
    }

    /// Run initialize_device and return whether it succeeded.
    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        use racing_wheel_hid_moza_protocol::VendorProtocol;
        self.protocol.initialize_device(&mut self.device)
    }

    /// Retry policy for bounded-retry scenarios.
    pub fn retry_policy() -> MozaRetryPolicy {
        MozaRetryPolicy::default()
    }
}
