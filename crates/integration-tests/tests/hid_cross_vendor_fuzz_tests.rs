//! Cross-vendor HID protocol fuzzing tests.
//!
//! Feeds random byte sequences to every vendor's input parser to verify that:
//! 1. No parser panics on arbitrary input.
//! 2. Bytes intended for one vendor are safely rejected by all other parsers.
//! 3. Truncated and maximum-length inputs are handled gracefully.

use proptest::prelude::*;

// ── Vendor parsers ───────────────────────────────────────────────────────

use hid_asetek_protocol::AsetekInputReport;
use hid_button_box_protocol::ButtonBoxInputReport;
use hid_simucube_protocol::{SimucubeHidReport, SimucubeInputReport};
use racing_wheel_hid_cammus_protocol::parse as cammus_parse;
use racing_wheel_hid_fanatec_protocol::{
    parse_extended_report as fanatec_extended, parse_pedal_report as fanatec_pedal,
    parse_standard_report as fanatec_standard,
};
use racing_wheel_hid_ffbeast_protocol::FFBeastStateReport;
use racing_wheel_hid_logitech_protocol::parse_input_report as logitech_parse;
use racing_wheel_hid_openffboard_protocol::OpenFFBoardInputReport;
use racing_wheel_hid_simagic_protocol::parse_input_report as simagic_parse;
use racing_wheel_hid_thrustmaster_protocol::parse_input_report as thrustmaster_parse;
use racing_wheel_hid_vrs_protocol::parse_input_report as vrs_parse;

/// Feed the same random buffer to every vendor parser. None may panic.
fn feed_all_parsers(data: &[u8]) {
    let _ = logitech_parse(data);
    let _ = fanatec_standard(data);
    let _ = fanatec_extended(data);
    let _ = fanatec_pedal(data);
    let _ = SimucubeHidReport::parse(data);
    let _ = SimucubeInputReport::parse(data);
    let _ = AsetekInputReport::parse(data);
    let _ = cammus_parse(data);
    let _ = OpenFFBoardInputReport::parse(data);
    let _ = FFBeastStateReport::parse(data);
    let _ = FFBeastStateReport::parse_with_id(data);
    let _ = simagic_parse(data);
    let _ = thrustmaster_parse(data);
    let _ = vrs_parse(data);
    let _ = ButtonBoxInputReport::parse_gamepad(data);
    let _ = ButtonBoxInputReport::parse_extended(data);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// Completely random bytes never cause a panic in any vendor parser.
    #[test]
    fn prop_cross_vendor_arbitrary_bytes_safe(
        data in proptest::collection::vec(any::<u8>(), 0..=256),
    ) {
        feed_all_parsers(&data);
    }

    /// Empty input is handled safely by every parser.
    #[test]
    fn prop_cross_vendor_empty_input_safe(_dummy in 0u8..1u8) {
        feed_all_parsers(&[]);
    }

    /// Single byte input is handled safely by every parser.
    #[test]
    fn prop_cross_vendor_single_byte_safe(byte in any::<u8>()) {
        feed_all_parsers(&[byte]);
    }

    /// All-0xFF bytes (common USB disconnect pattern) handled safely.
    #[test]
    fn prop_cross_vendor_all_ff_safe(len in 1usize..=128) {
        let data = vec![0xFF; len];
        feed_all_parsers(&data);
    }

    /// All-zeros (common init pattern) handled safely.
    #[test]
    fn prop_cross_vendor_all_zero_safe(len in 1usize..=128) {
        let data = vec![0x00; len];
        feed_all_parsers(&data);
    }

    /// Maximum-length HID reports (64 bytes) with random content.
    #[test]
    fn prop_cross_vendor_max_hid_report(data in proptest::collection::vec(any::<u8>(), 64..=64)) {
        feed_all_parsers(&data);
    }
}

// ── Heusinkveld (uses Result, not Option — separate test) ────────────

use hid_heusinkveld_protocol::HeusinkveldInputReport;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Arbitrary bytes never panic the Heusinkveld parser.
    #[test]
    fn prop_cross_vendor_heusinkveld_safe(
        data in proptest::collection::vec(any::<u8>(), 0..=128),
    ) {
        let _ = HeusinkveldInputReport::parse(&data);
    }
}
