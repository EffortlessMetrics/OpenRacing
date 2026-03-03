//! Leo Bodnar vendor protocol handler.
//!
//! [Leo Bodnar](https://www.leobodnar.com/) is a UK manufacturer of popular DIY
//! sim racing USB interfaces.
//!
//! ## Device IDs
//! - Vendor ID: `0x1DD2`
//! - Product ID `0x000E`: USB Sim Racing Wheel Interface (HID PID force feedback)
//! - Product ID `0x000C`: BBI-32 Button Box (input-only)
//! - Product ID `0x1301`: SLI-Pro Shift Light Indicator (input-only, estimated)
//! - Product ID `0x0001`: USB Joystick (input-only, no FFB)
//!
//! ## Protocol
//! The USB Sim Racing Wheel Interface (PID `0x000E`) uses standard USB HID PID
//! (Usage Page `0x000F`) for force feedback. No proprietary protocol extension is
//! required; the RT engine drives it through the standard HID PID constant-force
//! path. All other Leo Bodnar product IDs are input-only peripherals.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use racing_wheel_hid_leo_bodnar_protocol::{
    MAX_REPORT_BYTES, PID_BBI32, PID_FFB_JOYSTICK, PID_SLI_M, PID_USB_JOYSTICK,
    PID_WHEEL_INTERFACE, WHEEL_DEFAULT_MAX_TORQUE_NM, WHEEL_ENCODER_CPR, is_leo_bodnar_ffb_pid,
};
use tracing::{debug, info};

/// Leo Bodnar vendor ID.
pub use racing_wheel_hid_leo_bodnar_protocol::VENDOR_ID as LEO_BODNAR_VENDOR_ID;

/// USB Sim Racing Wheel Interface product ID (HID PID force feedback).
pub const LEO_BODNAR_PID_WHEEL: u16 = PID_WHEEL_INTERFACE;
/// BBI-32 Button Box product ID (input-only).
pub const LEO_BODNAR_PID_BBI32: u16 = PID_BBI32;
/// SLI-Pro Shift Light Indicator product ID (input-only, estimated).
pub const LEO_BODNAR_PID_SLIM: u16 = PID_SLI_M;
/// USB Joystick product ID (input-only, no FFB).
pub const LEO_BODNAR_PID_JOYSTICK: u16 = PID_USB_JOYSTICK;
/// FFB Joystick product ID (HID PID force feedback joystick).
pub const LEO_BODNAR_PID_FFB_JOYSTICK: u16 = PID_FFB_JOYSTICK;

/// Returns `true` if the given product ID is a Leo Bodnar FFB-capable device.
pub fn is_leo_bodnar_ffb_product(product_id: u16) -> bool {
    is_leo_bodnar_ffb_pid(product_id)
}

/// Leo Bodnar vendor protocol handler.
pub struct LeoBodnarHandler {
    vendor_id: u16,
    product_id: u16,
}

impl LeoBodnarHandler {
    /// Create a handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        debug!(
            "Created LeoBodnarHandler VID=0x{:04X} PID=0x{:04X}",
            vendor_id, product_id
        );
        Self {
            vendor_id,
            product_id,
        }
    }

    /// Returns `true` if this device supports HID PID force feedback.
    pub fn supports_pid_ffb(&self) -> bool {
        is_leo_bodnar_ffb_product(self.product_id)
    }
}

impl VendorProtocol for LeoBodnarHandler {
    fn initialize_device(
        &self,
        _writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.supports_pid_ffb() {
            info!(
                "Initialising Leo Bodnar USB Sim Racing Wheel Interface \
                 VID=0x{:04X} PID=0x{:04X} (standard HID PID)",
                self.vendor_id, self.product_id
            );
        } else {
            info!(
                "Leo Bodnar input-only device ready \
                 VID=0x{:04X} PID=0x{:04X} (no FFB init required)",
                self.vendor_id, self.product_id
            );
        }
        Ok(())
    }

    fn send_feature_report(
        &self,
        writer: &mut dyn DeviceWriter,
        report_id: u8,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        if data.len() + 1 > MAX_REPORT_BYTES {
            return Err(format!(
                "Feature report too large for Leo Bodnar transport: {} bytes",
                data.len() + 1
            )
            .into());
        }
        let mut buf = [0u8; MAX_REPORT_BYTES];
        buf[0] = report_id;
        buf[1..(data.len() + 1)].copy_from_slice(data);
        writer.write_feature_report(&buf[..(data.len() + 1)])?;
        Ok(())
    }

    fn get_ffb_config(&self) -> FfbConfig {
        if self.supports_pid_ffb() {
            FfbConfig {
                fix_conditional_direction: false,
                uses_vendor_usage_page: false,
                required_b_interval: None,
                max_torque_nm: WHEEL_DEFAULT_MAX_TORQUE_NM,
                encoder_cpr: WHEEL_ENCODER_CPR,
            }
        } else {
            FfbConfig {
                fix_conditional_direction: false,
                uses_vendor_usage_page: false,
                required_b_interval: None,
                max_torque_nm: 0.0,
                encoder_cpr: 0,
            }
        }
    }

    fn is_v2_hardware(&self) -> bool {
        false
    }

    fn output_report_id(&self) -> Option<u8> {
        // Standard HID PID — report ID is determined from the device descriptor at
        // runtime. No fixed vendor report ID is assigned.
        None
    }

    fn output_report_len(&self) -> Option<usize> {
        None
    }
}
