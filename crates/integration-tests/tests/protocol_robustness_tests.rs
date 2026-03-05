//! Protocol robustness tests for all vendor HID protocol parsers.
//!
//! Verifies that parsing functions handle malformed, truncated, and corrupted
//! input gracefully (returning `None` or `Err`, never panicking). This is
//! critical for a safety-critical force feedback system where HID reports may
//! arrive corrupted or truncated from hardware.

// ─── Test helper macros ─────────────────────────────────────────────────────

/// Tests an `Option`-returning parser against malformed inputs.
/// Cases 1-3 assert `None`; cases 4-5 only assert no panic.
macro_rules! option_parser_robustness {
    ($mod_name:ident, |$buf:ident| $parse_expr:expr, $truncated_len:expr) => {
        mod $mod_name {
            #[allow(unused_imports)]
            use super::*;

            #[test]
            fn empty_buffer_returns_none() {
                let $buf: &[u8] = &[];
                assert!(($parse_expr).is_none());
            }

            #[test]
            fn single_byte_returns_none() {
                let $buf: &[u8] = &[0x42];
                assert!(($parse_expr).is_none());
            }

            #[test]
            fn truncated_packet_returns_none() {
                let data = vec![0xAA_u8; $truncated_len];
                let $buf: &[u8] = &data;
                assert!(($parse_expr).is_none());
            }

            #[test]
            fn max_length_no_panic() {
                let data = [0xFF_u8; 64];
                let $buf: &[u8] = &data;
                let _ = $parse_expr;
            }

            #[test]
            fn all_zeros_no_panic() {
                let data = [0x00_u8; 64];
                let $buf: &[u8] = &data;
                let _ = $parse_expr;
            }
        }
    };
}

/// Tests a `Result`-returning parser against malformed inputs.
/// Cases 1-3 assert `Err`; cases 4-5 only assert no panic.
macro_rules! result_parser_robustness {
    ($mod_name:ident, |$buf:ident| $parse_expr:expr, $truncated_len:expr) => {
        mod $mod_name {
            #[allow(unused_imports)]
            use super::*;

            #[test]
            fn empty_buffer_returns_err() {
                let $buf: &[u8] = &[];
                assert!(($parse_expr).is_err());
            }

            #[test]
            fn single_byte_returns_err() {
                let $buf: &[u8] = &[0x42];
                assert!(($parse_expr).is_err());
            }

            #[test]
            fn truncated_packet_returns_err() {
                let data = vec![0xAA_u8; $truncated_len];
                let $buf: &[u8] = &data;
                assert!(($parse_expr).is_err());
            }

            #[test]
            fn max_length_no_panic() {
                let data = [0xFF_u8; 64];
                let $buf: &[u8] = &data;
                let _ = $parse_expr;
            }

            #[test]
            fn all_zeros_no_panic() {
                let data = [0x00_u8; 64];
                let $buf: &[u8] = &data;
                let _ = $parse_expr;
            }
        }
    };
}

/// Tests a parser returning neither `Option` nor `Result` — only verifies no panic.
macro_rules! no_panic_parser_robustness {
    ($mod_name:ident, |$buf:ident| $parse_expr:expr, $truncated_len:expr) => {
        mod $mod_name {
            #[allow(unused_imports)]
            use super::*;

            #[test]
            fn empty_buffer_no_panic() {
                let $buf: &[u8] = &[];
                let _ = $parse_expr;
            }

            #[test]
            fn single_byte_no_panic() {
                let $buf: &[u8] = &[0x42];
                let _ = $parse_expr;
            }

            #[test]
            fn truncated_packet_no_panic() {
                let data = vec![0xAA_u8; $truncated_len];
                let $buf: &[u8] = &data;
                let _ = $parse_expr;
            }

            #[test]
            fn max_length_no_panic() {
                let data = [0xFF_u8; 64];
                let $buf: &[u8] = &data;
                let _ = $parse_expr;
            }

            #[test]
            fn all_zeros_no_panic() {
                let data = [0x00_u8; 64];
                let $buf: &[u8] = &data;
                let _ = $parse_expr;
            }
        }
    };
}

// ─── Moza Racing ────────────────────────────────────────────────────────────

mod moza {
    use racing_wheel_hid_moza_protocol::{MozaProtocol, parse_hbp_report, parse_srp_report};

    const PID: u16 = 0x0004;

    option_parser_robustness!(
        input_state,
        |buf| { MozaProtocol::new(PID).parse_input_state(buf) },
        4
    );

    option_parser_robustness!(
        wheelbase_report,
        |buf| { MozaProtocol::new(PID).parse_wheelbase_report(buf) },
        4
    );

    option_parser_robustness!(
        aggregated_pedal_axes,
        |buf| { MozaProtocol::new(PID).parse_aggregated_pedal_axes(buf) },
        4
    );

    no_panic_parser_robustness!(hbp_report, |buf| { parse_hbp_report(PID, buf) }, 4);

    no_panic_parser_robustness!(srp_report, |buf| { parse_srp_report(PID, buf) }, 4);
}

// ─── Fanatec ────────────────────────────────────────────────────────────────

mod fanatec {
    use racing_wheel_hid_fanatec_protocol::{
        parse_extended_report, parse_pedal_report, parse_standard_report,
    };

    option_parser_robustness!(standard_report, |buf| { parse_standard_report(buf) }, 4);

    option_parser_robustness!(extended_report, |buf| { parse_extended_report(buf) }, 4);

    option_parser_robustness!(pedal_report, |buf| { parse_pedal_report(buf) }, 4);
}

// ─── Logitech ───────────────────────────────────────────────────────────────

mod logitech {
    use racing_wheel_hid_logitech_protocol::parse_input_report;

    option_parser_robustness!(input_report, |buf| { parse_input_report(buf) }, 6);
}

// ─── Thrustmaster ───────────────────────────────────────────────────────────

mod thrustmaster {
    use racing_wheel_hid_thrustmaster_protocol::input::parse_pedal_report;
    use racing_wheel_hid_thrustmaster_protocol::parse_input_report;

    option_parser_robustness!(input_report, |buf| { parse_input_report(buf) }, 4);

    option_parser_robustness!(pedal_report, |buf| { parse_pedal_report(buf) }, 2);
}

// ─── Simagic────────────────────────────────────────────────────────────────

mod simagic {
    use racing_wheel_hid_simagic_protocol::parse_input_report;
    use racing_wheel_hid_simagic_protocol::settings::parse_status1;

    option_parser_robustness!(input_report, |buf| { parse_input_report(buf) }, 4);

    option_parser_robustness!(status1, |buf| { parse_status1(buf) }, 8);
}

// ─── Simucube ───────────────────────────────────────────────────────────────

mod simucube {
    use hid_simucube_protocol::{SimucubeHidReport, SimucubeInputReport, parse_block_load};

    result_parser_robustness!(hid_report, |buf| { SimucubeHidReport::parse(buf) }, 16);

    result_parser_robustness!(input_report, |buf| { SimucubeInputReport::parse(buf) }, 8);

    option_parser_robustness!(block_load, |buf| { parse_block_load(buf) }, 4);
}

// ─── Cammus ─────────────────────────────────────────────────────────────────

mod cammus {
    use racing_wheel_hid_cammus_protocol::parse;

    result_parser_robustness!(input_report, |buf| { parse(buf) }, 6);
}

// ─── FFBeast ────────────────────────────────────────────────────────────────

mod ffbeast {
    use racing_wheel_hid_ffbeast_protocol::{
        FFBeastStateReport, parse_effect_settings, parse_firmware_license, parse_hardware_settings,
    };

    option_parser_robustness!(state_report, |buf| { FFBeastStateReport::parse(buf) }, 4);

    option_parser_robustness!(
        state_report_with_id,
        |buf| { FFBeastStateReport::parse_with_id(buf) },
        4
    );

    option_parser_robustness!(effect_settings, |buf| { parse_effect_settings(buf) }, 4);

    option_parser_robustness!(hardware_settings, |buf| { parse_hardware_settings(buf) }, 4);

    option_parser_robustness!(firmware_license, |buf| { parse_firmware_license(buf) }, 4);
}

// ─── PXN ────────────────────────────────────────────────────────────────────
// PXN protocol crate is encoding-only (PIDFF effects); no parsing functions.

// ─── Asetek ─────────────────────────────────────────────────────────────────

mod asetek {
    use hid_asetek_protocol::AsetekInputReport;

    result_parser_robustness!(input_report, |buf| { AsetekInputReport::parse(buf) }, 8);
}

// ─── OpenFFBoard ────────────────────────────────────────────────────────────

mod openffboard {
    use racing_wheel_hid_openffboard_protocol::{
        OpenFFBoardInputReport, VendorCommand, parse_block_load, parse_pid_pool,
    };

    option_parser_robustness!(
        input_report,
        |buf| { OpenFFBoardInputReport::parse(buf) },
        4
    );

    option_parser_robustness!(vendor_command, |buf| { VendorCommand::parse(buf) }, 4);

    option_parser_robustness!(pid_pool, |buf| { parse_pid_pool(buf) }, 2);

    option_parser_robustness!(block_load_report, |buf| { parse_block_load(buf) }, 4);
}
