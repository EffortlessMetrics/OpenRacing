//! Virtual Cube Controls device for integration and e2e testing.
//!
//! `VirtualCubeControlsDevice` implements `DeviceWriter` so protocol code can be tested
//! without real hardware. It records feature reports and output reports in order
//! and supports disconnect/reconnect simulation.

use racing_wheel_engine::hid::vendor::cube_controls::{
    CubeControlsModel, CubeControlsProtocolHandler,
};
use racing_wheel_hid_moza_protocol::{DeviceWriter, VendorProtocol};
use std::collections::VecDeque;

pub const CUBE_CONTROLS_VENDOR_ID: u16 = 0x0483;

pub const PRODUCT_ID_GT_PRO: u16 = 0x0C73;
pub const PRODUCT_ID_FORMULA_PRO: u16 = 0x0C74;
pub const PRODUCT_ID_CSX3: u16 = 0x0C75;

pub const MAX_OUTPUT_HISTORY: usize = 16;

pub struct VirtualCubeControlsDevice {
    pub product_id: u16,
    pub vendor_id: u16,
    feature_reports: Vec<Vec<u8>>,
    output_reports: VecDeque<Vec<u8>>,
    connected: bool,
    fail_writes: bool,
}

impl VirtualCubeControlsDevice {
    pub fn new(product_id: u16) -> Self {
        Self {
            product_id,
            vendor_id: CUBE_CONTROLS_VENDOR_ID,
            feature_reports: Vec::new(),
            output_reports: VecDeque::new(),
            connected: true,
            fail_writes: false,
        }
    }

    pub fn new_failing(product_id: u16) -> Self {
        let mut d = Self::new(product_id);
        d.fail_writes = true;
        d
    }

    pub fn disconnect(&mut self) {
        self.connected = false;
        self.fail_writes = true;
    }

    pub fn reconnect(&mut self) {
        self.connected = true;
        self.fail_writes = false;
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn feature_reports(&self) -> &[Vec<u8>] {
        &self.feature_reports
    }

    pub fn output_reports(&self) -> &VecDeque<Vec<u8>> {
        &self.output_reports
    }

    pub fn last_output_report(&self) -> Option<&Vec<u8>> {
        self.output_reports.back()
    }

    pub fn sent_feature_report_id(&self, report_id: u8) -> bool {
        self.feature_reports
            .iter()
            .any(|r| r.first().copied() == Some(report_id))
    }

    pub fn feature_reports_with_id(&self, report_id: u8) -> Vec<&Vec<u8>> {
        self.feature_reports
            .iter()
            .filter(|r| r.first().copied() == Some(report_id))
            .collect()
    }

    pub fn clear_records(&mut self) {
        self.feature_reports.clear();
        self.output_reports.clear();
    }

    pub fn model(&self) -> CubeControlsModel {
        CubeControlsModel::from_product_id(self.product_id)
    }
}

impl DeviceWriter for VirtualCubeControlsDevice {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_writes {
            return Err("VirtualCubeControlsDevice: simulated write failure".into());
        }
        let len = data.len();
        self.feature_reports.push(data.to_vec());
        Ok(len)
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_writes {
            return Err("VirtualCubeControlsDevice: simulated write failure".into());
        }
        let len = data.len();
        if self.output_reports.len() >= MAX_OUTPUT_HISTORY {
            self.output_reports.pop_front();
        }
        self.output_reports.push_back(data.to_vec());
        Ok(len)
    }
}

pub struct CubeControlsScenario {
    pub protocol: CubeControlsProtocolHandler,
    pub device: VirtualCubeControlsDevice,
}

impl CubeControlsScenario {
    pub fn wheelbase(product_id: u16) -> Self {
        Self {
            protocol: CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, product_id),
            device: VirtualCubeControlsDevice::new(product_id),
        }
    }

    pub fn wheelbase_failing(product_id: u16) -> Self {
        Self {
            protocol: CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, product_id),
            device: VirtualCubeControlsDevice::new_failing(product_id),
        }
    }

    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.protocol.initialize_device(&mut self.device)
    }

    pub fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}
