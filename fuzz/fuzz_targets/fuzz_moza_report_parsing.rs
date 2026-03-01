//! Fuzzes Moza wheelbase low-level report parsers and pedal aggregation.
//!
//! Covers: parse_wheelbase_input_report, parse_wheelbase_pedal_axes, parse_axis,
//! parse_srp_report with multiple product IDs, parse_aggregated_pedal_axes, and
//! MozaDirectTorqueEncoder::encode_zero. Must never panic on arbitrary input.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_moza_report_parsing
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_moza_protocol::{
    MozaDirectTorqueEncoder, MozaProtocol, REPORT_LEN, parse_axis,
    parse_srp_report, parse_wheelbase_input_report, parse_wheelbase_pedal_axes,
    parse_wheelbase_report, product_ids,
};

fuzz_target!(|data: &[u8]| {
    // Low-level wheelbase report parsers must never panic.
    let _ = parse_wheelbase_report(data);
    let _ = parse_wheelbase_input_report(data);
    let _ = parse_wheelbase_pedal_axes(data);

    // parse_axis with arbitrary start offsets.
    if data.len() >= 2 {
        let start = data[0] as usize;
        let _ = parse_axis(&data[1..], start);
    }

    // SRP pedal report parsing with all relevant product IDs.
    let _ = parse_srp_report(product_ids::SR_P_PEDALS, data);

    // Aggregated pedal axes through the protocol layer.
    for &pid in &[
        product_ids::R5_V1,
        product_ids::R9_V1,
        product_ids::R12_V1,
        product_ids::R16_R21_V1,
    ] {
        let protocol = MozaProtocol::new(pid);
        let _ = protocol.parse_aggregated_pedal_axes(data);
    }

    // MozaDirectTorqueEncoder::encode_zero must never panic.
    let enc = MozaDirectTorqueEncoder::new(5.5);
    let mut out = [0u8; REPORT_LEN];
    let _ = enc.encode_zero(&mut out);
});
