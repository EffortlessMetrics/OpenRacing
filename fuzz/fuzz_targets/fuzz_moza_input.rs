//! Fuzzes the full Moza protocol input parsing path.
//!
//! Exercises `parse_input_state` for all known Moza product IDs, as well as
//! `identify_device`, `is_wheelbase_product`, and `verify_signature` with
//! arbitrary byte inputs. None of these operations must panic.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_moza_input

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_moza_protocol::{
    DeviceSignature, MOZA_VENDOR_ID, MozaDirectTorqueEncoder, MozaModel, MozaProtocol,
    REPORT_LEN, identify_device, is_wheelbase_product, product_ids, verify_signature,
};

/// All known Moza product IDs exercised on every fuzz iteration.
const ALL_PIDS: &[u16] = &[
    product_ids::R3_V1,
    product_ids::R3_V2,
    product_ids::R5_V1,
    product_ids::R5_V2,
    product_ids::R9_V1,
    product_ids::R9_V2,
    product_ids::R12_V1,
    product_ids::R12_V2,
    product_ids::R16_R21_V1,
    product_ids::R16_R21_V2,
    product_ids::SR_P_PEDALS,
    product_ids::HGP_SHIFTER,
    product_ids::SGP_SHIFTER,
    product_ids::HBP_HANDBRAKE,
];

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // -- Parse input state through all known product IDs --------------------
    for &pid in ALL_PIDS {
        let protocol = MozaProtocol::new(pid);
        let _ = protocol.parse_input_state(data);
    }

    // -- Arbitrary PID from fuzz input ---------------------------------------
    if data.len() >= 2 {
        let pid = u16::from_le_bytes([data[0], data[1]]);
        let protocol = MozaProtocol::new(pid);
        let _ = protocol.parse_input_state(&data[2..]);

        // Identification and classification must never panic.
        let _ = identify_device(pid);
        let _ = is_wheelbase_product(pid);

        // Signature verification must never panic.
        let sig_moza = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, pid);
        let _ = verify_signature(&sig_moza);

        let vid = if data.len() >= 4 {
            u16::from_le_bytes([data[2], data[3]])
        } else {
            0
        };
        let sig_arb = DeviceSignature::from_vid_pid(vid, pid);
        let _ = verify_signature(&sig_arb);
    }

    // -- Torque encoder with arbitrary max_torque and report bytes -----------
    if data.len() >= 4 {
        let torque_bytes: [u8; 4] = [data[0], data[1], data[2], data[3]];
        let torque_nm = f32::from_le_bytes(torque_bytes);

        for model in [
            MozaModel::R3,
            MozaModel::R5,
            MozaModel::R9,
            MozaModel::R12,
            MozaModel::R16,
            MozaModel::R21,
        ] {
            let enc = MozaDirectTorqueEncoder::new(model.max_torque_nm());
            let mut out = [0u8; REPORT_LEN];
            let _ = enc.encode(torque_nm, 0, &mut out);
        }
    }
});
