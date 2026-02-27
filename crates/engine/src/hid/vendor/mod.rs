//! Vendor-specific protocol handlers
//!
//! This module provides abstractions for vendor-specific device initialization,
//! configuration, and quirks handling.
#![deny(static_mut_refs)]

pub mod asetek;
pub mod fanatec;
pub mod heusinkveld;
pub mod logitech;
pub mod moza;
pub mod moza_direct;
pub mod ffbeast;
pub mod openffboard;
pub mod simagic;
pub mod simucube;
pub mod thrustmaster;
pub mod vrs;

#[cfg(test)]
mod asetek_tests;
#[cfg(test)]
mod fanatec_tests;
#[cfg(test)]
mod heusinkveld_tests;
#[cfg(test)]
mod logitech_tests;
#[cfg(test)]
mod moza_tests;
#[cfg(test)]
mod ffbeast_tests;
#[cfg(test)]
mod openffboard_tests;
#[cfg(test)]
mod simagic_tests;
#[cfg(test)]
mod simucube_tests;
#[cfg(test)]
mod thrustmaster_tests;
#[cfg(test)]
mod vrs_tests;

pub use racing_wheel_hid_moza_protocol::{DeviceWriter, FfbConfig, VendorProtocol};

/// Get the appropriate vendor protocol handler for a device
pub fn get_vendor_protocol(vendor_id: u16, product_id: u16) -> Option<Box<dyn VendorProtocol>> {
    match vendor_id {
        0x0EB7 => Some(Box::new(fanatec::FanatecProtocol::new(
            vendor_id, product_id,
        ))),
        0x046D => Some(Box::new(logitech::LogitechProtocol::new(
            vendor_id, product_id,
        ))),
        0x346E => Some(Box::new(moza::MozaProtocol::new(product_id))),
        0x044F => Some(Box::new(thrustmaster::ThrustmasterProtocolHandler::new(
            vendor_id, product_id,
        ))),
        // STM VID: shared by Simagic legacy AND VRS DirectForce Pro (0xA3xx PIDs)
        0x0483 => {
            if vrs::is_vrs_product(product_id) {
                Some(Box::new(vrs::VrsProtocolHandler::new(vendor_id, product_id)))
            } else {
                Some(Box::new(simagic::SimagicProtocol::new(vendor_id, product_id)))
            }
        }
        // OpenMoko VID: shared by Simagic legacy AND Heusinkveld (0x115x PIDs)
        0x16D0 => {
            if heusinkveld::is_heusinkveld_product(product_id) {
                Some(Box::new(heusinkveld::HeusinkveldProtocolHandler::new(
                    vendor_id, product_id,
                )))
            } else {
                Some(Box::new(simagic::SimagicProtocol::new(vendor_id, product_id)))
            }
        }
        // Simagic EVO and modern (including 0x2D5C)
        0x3670 | 0x2D5C => Some(Box::new(simagic::SimagicProtocol::new(
            vendor_id, product_id,
        ))),
        // Simucube 2 Sport/Pro/Ultimate
        0x2D6A => Some(Box::new(simucube::SimucubeProtocolHandler::new(
            vendor_id, product_id,
        ))),
        // Asetek Forte/Invicta/LaPrima
        0x2E5A => Some(Box::new(asetek::AsetekProtocolHandler::new(
            vendor_id, product_id,
        ))),
        // OpenFFBoard (pid.codes open hardware VID)
        0x1209 => {
            if openffboard::is_openffboard_product(product_id) {
                Some(Box::new(openffboard::OpenFFBoardHandler::new(
                    vendor_id, product_id,
                )))
            } else {
                None
            }
        }
        // FFBeast open-source direct drive controller
        0x045B => {
            if ffbeast::is_ffbeast_product(product_id) {
                Some(Box::new(ffbeast::FFBeastHandler::new(vendor_id, product_id)))
            } else {
                None
            }
        }
        _ => None,
    }
}