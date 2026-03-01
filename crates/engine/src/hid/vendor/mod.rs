//! Vendor-specific protocol handlers
//!
//! This module provides abstractions for vendor-specific device initialization,
//! configuration, and quirks handling.
#![deny(static_mut_refs)]

pub mod accuforce;
pub mod asetek;
pub mod button_box;
pub mod cammus;
pub mod cube_controls;
pub mod fanatec;
pub mod ffbeast;
pub mod generic_hid_pid;
pub mod heusinkveld;
pub mod leo_bodnar;
pub mod logitech;
pub mod moza;
pub mod moza_direct;
pub mod openffboard;
pub mod simagic;
pub mod simplemotion;
pub mod simucube;
pub mod thrustmaster;
pub mod vrs;

#[cfg(test)]
mod accuforce_tests;
#[cfg(test)]
mod asetek_tests;
#[cfg(test)]
mod button_box_tests;
#[cfg(test)]
mod cammus_tests;
#[cfg(test)]
mod cube_controls_tests;
#[cfg(test)]
mod fanatec_tests;
#[cfg(test)]
mod ffbeast_tests;
#[cfg(test)]
mod generic_hid_pid_tests;
#[cfg(test)]
mod heusinkveld_tests;
#[cfg(test)]
mod leo_bodnar_tests;
#[cfg(test)]
mod logitech_tests;
#[cfg(test)]
mod moza_tests;
#[cfg(test)]
mod openffboard_tests;
#[cfg(test)]
mod simagic_tests;
#[cfg(test)]
mod simplemotion_tests;
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
        // STM VID: shared by Simagic legacy, VRS DirectForce Pro (0xA3xx PIDs),
        // and provisional Cube Controls assignments (0x0C7x PIDs).
        0x0483 => {
            if vrs::is_vrs_product(product_id) {
                Some(Box::new(vrs::VrsProtocolHandler::new(
                    vendor_id, product_id,
                )))
            } else if cube_controls::is_cube_controls_product(product_id) {
                Some(Box::new(cube_controls::CubeControlsProtocolHandler::new(
                    vendor_id, product_id,
                )))
            } else {
                Some(Box::new(simagic::SimagicProtocol::new(
                    vendor_id, product_id,
                )))
            }
        }
        // Microchip VID (0x04D8): used by Heusinkveld pedals (PIDs 0xF6D0–0xF6D3).
        // VID is shared by many Microchip PIC-based devices; disambiguate by PID.
        0x04D8 if heusinkveld::is_heusinkveld_product(product_id) => Some(Box::new(
            heusinkveld::HeusinkveldProtocolHandler::new(vendor_id, product_id),
        )),
        // OpenMoko/MCS VID (0x16D0): Simucube 2 (0x0D5x),
        // and legacy Simagic/Simucube 1 (0x0D5A/0x0D5B). Disambiguate by product_id.
        0x16D0 => {
            if simucube::is_simucube_product(product_id) {
                Some(Box::new(simucube::SimucubeProtocolHandler::new(
                    vendor_id, product_id,
                )))
            } else {
                // Legacy Simagic / Simucube 1 devices
                Some(Box::new(simagic::SimagicProtocol::new(
                    vendor_id, product_id,
                )))
            }
        }
        // Simagic EVO generation (VID 0x3670 = Shen Zhen Simagic Technology Co., Ltd.)
        0x3670 => Some(Box::new(simagic::SimagicProtocol::new(
            vendor_id, product_id,
        ))),
        // Asetek SimSports (VID 0x2433 = Asetek A/S)
        0x2433 => Some(Box::new(asetek::AsetekProtocolHandler::new(
            vendor_id, product_id,
        ))),
        // Granite Devices SimpleMotion V2 (IONI, ARGON, OSW)
        0x1D50 => Some(Box::new(simplemotion::SimpleMotionProtocolHandler::new(
            vendor_id, product_id,
        ))),
        // pid.codes shared VID: OpenFFBoard FFB controllers + generic button boxes
        0x1209 => {
            if openffboard::is_openffboard_product(product_id) {
                Some(Box::new(openffboard::OpenFFBoardHandler::new(
                    vendor_id, product_id,
                )))
            } else if button_box::is_button_box_product(product_id) {
                Some(Box::new(button_box::ButtonBoxProtocolHandler::new(
                    vendor_id, product_id,
                )))
            } else {
                None
            }
        }
        // FFBeast open-source direct drive controller
        0x045B => {
            if ffbeast::is_ffbeast_product(product_id) {
                Some(Box::new(ffbeast::FFBeastHandler::new(
                    vendor_id, product_id,
                )))
            } else {
                None
            }
        }
        // Cammus Technology Co., Ltd. (C5, C12 direct drive wheelbases)
        0x3416 => Some(Box::new(cammus::CammusProtocolHandler::new(
            vendor_id, product_id,
        ))),
        // SimExperience AccuForce Pro (NXP USB chip VID)
        0x1FC9 => {
            if accuforce::is_accuforce_product(product_id) {
                Some(Box::new(accuforce::AccuForceProtocolHandler::new(
                    vendor_id, product_id,
                )))
            } else {
                None
            }
        }
        // Leo Bodnar USB sim racing interfaces and peripherals
        0x1DD2 => Some(Box::new(leo_bodnar::LeoBodnarHandler::new(
            vendor_id, product_id,
        ))),
        _ => None,
    }
}

/// Get the vendor protocol handler for a device, falling back to a generic HID PID
/// handler when no specific vendor is matched and the device advertises standard
/// USB HID PID (Usage Page `0x000F`) force feedback capabilities.
///
/// This covers community builds, AccuForce Pro (VID `0x16D0`), and assorted
/// Chinese direct-drive controllers not otherwise identified.
pub fn get_vendor_protocol_with_hid_pid_fallback(
    vendor_id: u16,
    product_id: u16,
    has_hid_pid_capability: bool,
) -> Option<Box<dyn VendorProtocol>> {
    if let Some(handler) = get_vendor_protocol(vendor_id, product_id) {
        return Some(handler);
    }
    if has_hid_pid_capability {
        Some(Box::new(generic_hid_pid::GenericHidPidHandler::new(
            vendor_id, product_id,
        )))
    } else {
        None
    }
}
