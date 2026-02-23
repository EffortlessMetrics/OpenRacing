//! Device identity verification for Moza HID devices.
//!
//! Signature gating ensures that only devices with known-good VID/PID pairs
//! receive handshake sequences, and that high-torque mode is never implicitly
//! enabled.

#![deny(static_mut_refs)]

use crate::ids::{MOZA_VENDOR_ID, product_ids};
use crate::types::MozaDeviceCategory;
use crate::types::identify_device;

/// Fingerprint of a connected USB HID device.
///
/// The caller populates as many fields as the platform provides.
/// `vendor_id` and `product_id` are always required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSignature {
    pub vendor_id: u16,
    pub product_id: u16,
    /// USB interface number (0 for most single-interface HID devices).
    pub interface_number: Option<u8>,
    /// Length of the raw HID report descriptor in bytes, when available.
    pub descriptor_len: Option<u16>,
    /// CRC-32 of the raw HID report descriptor bytes, when available.
    pub descriptor_crc32: Option<u32>,
}

impl DeviceSignature {
    /// Minimal signature from VID/PID only.
    pub const fn from_vid_pid(vendor_id: u16, product_id: u16) -> Self {
        Self {
            vendor_id,
            product_id,
            interface_number: None,
            descriptor_len: None,
            descriptor_crc32: None,
        }
    }
}

/// Result of checking a device signature against the known-safe allowlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureVerdict {
    /// Known Moza wheelbase PID — handshake, input parsing, and FFB permitted.
    KnownWheelbase,
    /// Known Moza peripheral PID — input parsing permitted; no FFB handshake.
    KnownPeripheral,
    /// Moza VID present but PID not in the allowlist.
    UnknownProduct,
    /// Not a Moza device or VID mismatch.
    Rejected,
}

/// Verify a device signature against the static Moza allowlist.
///
/// This is a pure function: it never performs I/O and may be called from any
/// context including test code with constructed signatures.
pub fn verify_signature(sig: &DeviceSignature) -> SignatureVerdict {
    if sig.vendor_id != MOZA_VENDOR_ID {
        return SignatureVerdict::Rejected;
    }

    let identity = identify_device(sig.product_id);
    match identity.category {
        MozaDeviceCategory::Wheelbase => SignatureVerdict::KnownWheelbase,
        MozaDeviceCategory::Pedals
        | MozaDeviceCategory::Shifter
        | MozaDeviceCategory::Handbrake => SignatureVerdict::KnownPeripheral,
        MozaDeviceCategory::Unknown => {
            if is_known_peripheral_pid(sig.product_id) {
                SignatureVerdict::KnownPeripheral
            } else {
                SignatureVerdict::UnknownProduct
            }
        }
    }
}

fn is_known_peripheral_pid(pid: u16) -> bool {
    matches!(
        pid,
        product_ids::SR_P_PEDALS
            | product_ids::HGP_SHIFTER
            | product_ids::SGP_SHIFTER
            | product_ids::HBP_HANDBRAKE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_wheelbase_pids_return_wheelbase_verdict() {
        let wheelbase_pids = [
            product_ids::R5_V1,
            product_ids::R5_V2,
            product_ids::R9_V1,
            product_ids::R9_V2,
            product_ids::R3_V1,
            product_ids::R3_V2,
            product_ids::R12_V1,
            product_ids::R12_V2,
            product_ids::R16_R21_V1,
            product_ids::R16_R21_V2,
        ];

        for pid in wheelbase_pids {
            let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, pid);
            assert_eq!(
                verify_signature(&sig),
                SignatureVerdict::KnownWheelbase,
                "expected KnownWheelbase for pid 0x{pid:04X}"
            );
        }
    }

    #[test]
    fn known_peripheral_pids_return_peripheral_verdict() {
        let peripheral_pids = [
            product_ids::SR_P_PEDALS,
            product_ids::HGP_SHIFTER,
            product_ids::SGP_SHIFTER,
            product_ids::HBP_HANDBRAKE,
        ];

        for pid in peripheral_pids {
            let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, pid);
            assert_eq!(
                verify_signature(&sig),
                SignatureVerdict::KnownPeripheral,
                "expected KnownPeripheral for pid 0x{pid:04X}"
            );
        }
    }

    #[test]
    fn unknown_moza_pid_returns_unknown_product() {
        let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, 0x9999);
        assert_eq!(verify_signature(&sig), SignatureVerdict::UnknownProduct);
    }

    #[test]
    fn non_moza_vid_returns_rejected() {
        let sig = DeviceSignature::from_vid_pid(0x0458, product_ids::R5_V1);
        assert_eq!(verify_signature(&sig), SignatureVerdict::Rejected);
    }

    #[test]
    fn zero_vid_returns_rejected() {
        let sig = DeviceSignature::from_vid_pid(0x0000, 0x0000);
        assert_eq!(verify_signature(&sig), SignatureVerdict::Rejected);
    }
}
