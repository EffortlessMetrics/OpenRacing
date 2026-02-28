//! FFBeast vendor protocol handler.
//!
//! [FFBeast](https://github.com/HF-Robotics/FFBeast) is an open-source
//! direct-drive force feedback controller that uses standard USB HID PID
//! force effects with an optional custom command layer.
//!
//! ## Device IDs
//! - Vendor ID: `0x045B` (`USB_VENDOR_ID_FFBEAST` in the Linux kernel)
//! - Product ID `0x58F9`: joystick
//! - Product ID `0x5968`: rudder
//! - Product ID `0x59D7`: wheel
//!
//! ## Protocol
//! Uses standard HID PID constant-force reports for real-time torque output.
//! Global gain and FFB enable are controlled through vendor-defined HID feature
//! reports on the same interface.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info};

pub use racing_wheel_hid_ffbeast_protocol::{
    CONSTANT_FORCE_REPORT_LEN, FFBEAST_PRODUCT_ID_WHEEL, FFBEAST_VENDOR_ID, FFBeastTorqueEncoder,
    build_enable_ffb, build_set_gain, is_ffbeast_product,
};

/// Default maximum torque for FFBeast in Newton-metres.
///
/// FFBeast torque capacity depends on the motor and PSU configuration.
/// 20 Nm is a reasonable default for popular high-end builds; the user can
/// tune this in their profile.
const DEFAULT_MAX_TORQUE_NM: f32 = 20.0;

/// Default encoder CPR.
///
/// FFBeast supports various encoders; 65535 CPR (16-bit) is typical.
const DEFAULT_ENCODER_CPR: u32 = 65_535;

/// FFBeast vendor protocol handler.
pub struct FFBeastHandler {
    vendor_id: u16,
    product_id: u16,
    #[allow(dead_code)]
    encoder: FFBeastTorqueEncoder,
}

impl FFBeastHandler {
    /// Create a handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        debug!(
            "Created FFBeastHandler VID=0x{:04X} PID=0x{:04X}",
            vendor_id, product_id
        );
        Self {
            vendor_id,
            product_id,
            encoder: FFBeastTorqueEncoder,
        }
    }
}

impl VendorProtocol for FFBeastHandler {
    fn initialize_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Initialising FFBeast VID=0x{:04X} PID=0x{:04X}",
            self.vendor_id, self.product_id
        );
        // Enable FFB output at full gain.
        writer.write_feature_report(&build_enable_ffb(true))?;
        writer.write_feature_report(&build_set_gain(0xFF))?;
        Ok(())
    }

    fn send_feature_report(
        &self,
        writer: &mut dyn DeviceWriter,
        report_id: u8,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        const MAX: usize = 64;
        if data.len() + 1 > MAX {
            return Err(format!(
                "Feature report too large for FFBeast transport: {} bytes",
                data.len() + 1
            )
            .into());
        }
        let mut buf = [0u8; MAX];
        buf[0] = report_id;
        buf[1..(data.len() + 1)].copy_from_slice(data);
        writer.write_feature_report(&buf[..(data.len() + 1)])?;
        Ok(())
    }

    fn shutdown_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!(
            "Shutting down FFBeast VID=0x{:04X} PID=0x{:04X}",
            self.vendor_id, self.product_id
        );
        writer.write_feature_report(&build_enable_ffb(false))?;
        Ok(())
    }

    fn get_ffb_config(&self) -> FfbConfig {
        FfbConfig {
            fix_conditional_direction: false,
            uses_vendor_usage_page: false,
            required_b_interval: Some(1),
            max_torque_nm: DEFAULT_MAX_TORQUE_NM,
            encoder_cpr: DEFAULT_ENCODER_CPR,
        }
    }

    fn is_v2_hardware(&self) -> bool {
        false
    }

    fn output_report_id(&self) -> Option<u8> {
        use racing_wheel_hid_ffbeast_protocol::CONSTANT_FORCE_REPORT_ID;
        Some(CONSTANT_FORCE_REPORT_ID)
    }

    fn output_report_len(&self) -> Option<usize> {
        Some(CONSTANT_FORCE_REPORT_LEN)
    }
}
