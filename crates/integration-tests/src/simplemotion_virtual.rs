//! Virtual SimpleMotion V2 device for integration and e2e testing.
//!
//! `VirtualSimpleMotionDevice` implements `DeviceWriter` so protocol code can be
//! tested without real hardware. Records feature and output reports in order and
//! supports disconnect/reconnect simulation.

use racing_wheel_engine::hid::vendor::simplemotion::SimpleMotionProtocolHandler;
use racing_wheel_hid_moza_protocol::{DeviceWriter, VendorProtocol};
use racing_wheel_simplemotion_v2::{
    ARGON_PRODUCT_ID, IONI_PRODUCT_ID, IONI_PRODUCT_ID_PREMIUM, IONI_VENDOR_ID, TORQUE_COMMAND_LEN,
    TorqueCommandEncoder,
};
use std::collections::VecDeque;

/// Maximum output report history retained by the virtual device.
pub const MAX_OUTPUT_HISTORY: usize = 32;

/// A software stand-in for a SimpleMotion V2 device used in integration tests.
pub struct VirtualSimpleMotionDevice {
    pub product_id: u16,
    pub vendor_id: u16,
    feature_reports: Vec<Vec<u8>>,
    output_reports: VecDeque<Vec<u8>>,
    connected: bool,
    fail_writes: bool,
}

impl VirtualSimpleMotionDevice {
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

    /// True when `output_reports` contains a report whose second byte matches `cmd_type`.
    pub fn sent_output_cmd_type(&self, cmd_type_byte: u8) -> bool {
        self.output_reports
            .iter()
            .any(|r| r.get(2).copied() == Some(cmd_type_byte))
    }

    /// Clear all recorded reports.
    pub fn clear_records(&mut self) {
        self.feature_reports.clear();
        self.output_reports.clear();
    }
}

impl DeviceWriter for VirtualSimpleMotionDevice {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_writes {
            return Err("VirtualSimpleMotionDevice: simulated write failure".into());
        }
        let len = data.len();
        self.feature_reports.push(data.to_vec());
        Ok(len)
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_writes {
            return Err("VirtualSimpleMotionDevice: simulated write failure".into());
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
pub struct SimpleMotionScenario {
    pub protocol: SimpleMotionProtocolHandler,
    pub device: VirtualSimpleMotionDevice,
    pub encoder: TorqueCommandEncoder,
}

impl SimpleMotionScenario {
    /// Create an IONI (Simucube 1) scenario.
    pub fn ioni() -> Self {
        let identity = racing_wheel_simplemotion_v2::identify_device(IONI_PRODUCT_ID);
        Self {
            protocol: SimpleMotionProtocolHandler::new(IONI_VENDOR_ID, IONI_PRODUCT_ID),
            device: VirtualSimpleMotionDevice::new(IONI_VENDOR_ID, IONI_PRODUCT_ID),
            encoder: TorqueCommandEncoder::new(identity.max_torque_nm.unwrap_or(15.0)),
        }
    }

    /// Create an IONI Premium (Simucube 2) scenario.
    pub fn ioni_premium() -> Self {
        let identity = racing_wheel_simplemotion_v2::identify_device(IONI_PRODUCT_ID_PREMIUM);
        Self {
            protocol: SimpleMotionProtocolHandler::new(IONI_VENDOR_ID, IONI_PRODUCT_ID_PREMIUM),
            device: VirtualSimpleMotionDevice::new(IONI_VENDOR_ID, IONI_PRODUCT_ID_PREMIUM),
            encoder: TorqueCommandEncoder::new(identity.max_torque_nm.unwrap_or(35.0)),
        }
    }

    /// Create an ARGON (Simucube Sport) scenario.
    pub fn argon() -> Self {
        let identity = racing_wheel_simplemotion_v2::identify_device(ARGON_PRODUCT_ID);
        Self {
            protocol: SimpleMotionProtocolHandler::new(IONI_VENDOR_ID, ARGON_PRODUCT_ID),
            device: VirtualSimpleMotionDevice::new(IONI_VENDOR_ID, ARGON_PRODUCT_ID),
            encoder: TorqueCommandEncoder::new(identity.max_torque_nm.unwrap_or(10.0)),
        }
    }

    /// Create an IONI scenario with a failing device (simulates I/O errors).
    pub fn ioni_failing() -> Self {
        let identity = racing_wheel_simplemotion_v2::identify_device(IONI_PRODUCT_ID);
        Self {
            protocol: SimpleMotionProtocolHandler::new(IONI_VENDOR_ID, IONI_PRODUCT_ID),
            device: VirtualSimpleMotionDevice::new_failing(IONI_VENDOR_ID, IONI_PRODUCT_ID),
            encoder: TorqueCommandEncoder::new(identity.max_torque_nm.unwrap_or(15.0)),
        }
    }

    /// Run `initialize_device` and return whether it succeeded.
    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.protocol.initialize_device(&mut self.device)
    }

    /// Run `shutdown_device` (no-op for SimpleMotion; tested for completeness).
    pub fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.protocol.shutdown_device(&mut self.device)
    }

    /// Encode and dispatch a torque command to the virtual device.
    pub fn write_torque(&mut self, torque_nm: f32) -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = [0u8; TORQUE_COMMAND_LEN];
        self.encoder.encode(torque_nm, &mut buf);
        self.device.write_output_report(&buf)?;
        Ok(())
    }
}

pub use racing_wheel_simplemotion_v2::{
    ARGON_PRODUCT_ID as PID_ARGON, IONI_PRODUCT_ID as PID_IONI,
    IONI_PRODUCT_ID_PREMIUM as PID_IONI_PREMIUM, IONI_VENDOR_ID as VENDOR_ID,
};
