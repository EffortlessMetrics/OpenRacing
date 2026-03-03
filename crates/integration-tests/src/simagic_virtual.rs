//! Virtual Simagic device for integration and e2e testing.
//!
//! `VirtualSimagicDevice` implements `DeviceWriter` so protocol code can be
//! tested without real hardware. It records feature reports and output reports
//! in order and supports disconnect/reconnect simulation.

use racing_wheel_engine::hid::vendor::simagic::{SimagicProtocol, vendor_ids};
use racing_wheel_hid_moza_protocol::{DeviceWriter, VendorProtocol};
use std::collections::VecDeque;

/// Maximum output report history retained by the virtual device.
pub const MAX_OUTPUT_HISTORY: usize = 16;

/// A software stand-in for a Simagic HID device used in integration tests.
pub struct VirtualSimagicDevice {
    pub product_id: u16,
    pub vendor_id: u16,
    feature_reports: Vec<Vec<u8>>,
    output_reports: VecDeque<Vec<u8>>,
    connected: bool,
    fail_writes: bool,
}

impl VirtualSimagicDevice {
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

    /// Simulate a device disconnect.
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

    /// Clear all recorded reports.
    pub fn clear_records(&mut self) {
        self.feature_reports.clear();
        self.output_reports.clear();
    }
}

impl DeviceWriter for VirtualSimagicDevice {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_writes {
            return Err("VirtualSimagicDevice: simulated write failure".into());
        }
        let len = data.len();
        self.feature_reports.push(data.to_vec());
        Ok(len)
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_writes {
            return Err("VirtualSimagicDevice: simulated write failure".into());
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
pub struct SimagicScenario {
    pub protocol: SimagicProtocol,
    pub device: VirtualSimagicDevice,
}

impl SimagicScenario {
    /// Create a scenario for a legacy Simagic device (VID 0x0483).
    pub fn legacy(product_id: u16) -> Self {
        let vid = vendor_ids::SIMAGIC_STM;
        Self {
            protocol: SimagicProtocol::new(vid, product_id),
            device: VirtualSimagicDevice::new(vid, product_id),
        }
    }

    /// Create a scenario for an EVO-generation device (VID 0x3670).
    pub fn evo(product_id: u16) -> Self {
        let vid = vendor_ids::SIMAGIC_EVO;
        Self {
            protocol: SimagicProtocol::new(vid, product_id),
            device: VirtualSimagicDevice::new(vid, product_id),
        }
    }

    /// Create a failing legacy scenario.
    pub fn legacy_failing(product_id: u16) -> Self {
        let vid = vendor_ids::SIMAGIC_STM;
        Self {
            protocol: SimagicProtocol::new(vid, product_id),
            device: VirtualSimagicDevice::new_failing(vid, product_id),
        }
    }

    /// Create a failing EVO scenario.
    pub fn evo_failing(product_id: u16) -> Self {
        let vid = vendor_ids::SIMAGIC_EVO;
        Self {
            protocol: SimagicProtocol::new(vid, product_id),
            device: VirtualSimagicDevice::new_failing(vid, product_id),
        }
    }

    /// Run `initialize_device` and return whether it succeeded.
    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.protocol.initialize_device(&mut self.device)
    }
}
