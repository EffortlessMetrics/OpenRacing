//! Generic HID PID fallback vendor protocol handler.
//!
//! This handler is used for any device that advertises standard USB HID PID
//! force feedback capabilities (Usage Page `0x000F`) but is not matched by a
//! specific vendor handler. Typical devices covered by this handler include:
//!
//! - Community-built OSW (Open Sim Wheel) controllers with assorted VIDs
//! - AccuForce Pro (SimExperience, VID `0x16D0`) and similar
//! - Various Chinese direct-drive wheels not otherwise identified
//!
//! ## Protocol
//! The handler issues no vendor-specific initialisation or shutdown reports.
//! The RT engine drives effects via the standard USB HID PID constant-force
//! output path. Maximum torque is set conservatively (8 Nm) because the true
//! hardware capability of an unidentified device is unknown; users should tune
//! the value in their force feedback profile.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info};

/// Conservative maximum torque for unidentified HID PID devices in Newton-metres.
///
/// This is intentionally cautious to avoid overdriving hardware whose rating is
/// unknown. Users can raise the limit in their force feedback profile.
const DEFAULT_MAX_TORQUE_NM: f32 = 8.0;

/// Default encoder CPR for unidentified HID PID devices.
///
/// A 4096-count resolution is a reasonable lower-bound for most devices; the
/// engine will use the value reported by the device descriptor when available.
const DEFAULT_ENCODER_CPR: u32 = 4_096;

/// Generic HID PID vendor protocol handler (fallback for standard HID PID devices).
pub struct GenericHidPidHandler {
    vendor_id: u16,
    product_id: u16,
}

impl GenericHidPidHandler {
    /// Create a handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        debug!(
            "Created GenericHidPidHandler VID=0x{:04X} PID=0x{:04X}",
            vendor_id, product_id
        );
        Self {
            vendor_id,
            product_id,
        }
    }
}

impl VendorProtocol for GenericHidPidHandler {
    fn initialize_device(
        &self,
        _writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Generic HID PID device ready VID=0x{:04X} PID=0x{:04X} \
             (standard HID PID fallback, no vendor-specific init)",
            self.vendor_id, self.product_id
        );
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
                "Feature report too large for generic HID PID transport: {} bytes",
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

    fn get_ffb_config(&self) -> FfbConfig {
        FfbConfig {
            fix_conditional_direction: false,
            uses_vendor_usage_page: false,
            required_b_interval: None,
            max_torque_nm: DEFAULT_MAX_TORQUE_NM,
            encoder_cpr: DEFAULT_ENCODER_CPR,
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
