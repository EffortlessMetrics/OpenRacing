//! Comprehensive tests for the Thrustmaster HID protocol crate.
//!
//! Covers:
//! 1. Input report parsing for T300/T500/T-GT/TS-XW and other supported models
//! 2. Output report construction (FFB, LED, mode switching)
//! 3. Device identification via PID for all supported wheelbases
//! 4. Axis parsing precision and dead zone handling
//! 5. Edge cases: short reports, invalid PIDs, mode transitions
//! 6. Property tests for encoding/decoding
//! 7. Known constant validation

use racing_wheel_hid_thrustmaster_protocol as tm;
use racing_wheel_hid_thrustmaster_protocol::{
    EFFECT_REPORT_LEN, Model, THRUSTMASTER_VENDOR_ID, ThrustmasterConstantForceEncoder,
    ThrustmasterInitState, ThrustmasterProtocol, init_protocol, product_ids,
};

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Input report parsing per model via ThrustmasterProtocol
// ═══════════════════════════════════════════════════════════════════════════════

/// Helper: build a valid 16-byte input report with given steering and pedal values.
fn build_input_report(steering: u16, throttle: u8, brake: u8, clutch: u8) -> Vec<u8> {
    let [s_lo, s_hi] = steering.to_le_bytes();
    vec![
        0x01, s_lo, s_hi, throttle, brake, clutch, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00,
    ]
}

mod input_parsing_per_model {
    use super::*;

    /// All known wheelbase PIDs that should successfully parse input reports.
    const WHEELBASE_PIDS: &[u16] = &[
        product_ids::T150,
        product_ids::TMX,
        product_ids::T300_RS,
        product_ids::T300_RS_PS4,
        product_ids::T300_RS_GT,
        product_ids::TX_RACING,
        product_ids::TX_RACING_ORIG,
        product_ids::T500_RS,
        product_ids::T248,
        product_ids::T248X,
        product_ids::TS_PC_RACER,
        product_ids::TS_XW,
        product_ids::TS_XW_GIP,
        product_ids::T_GT_II_GT,
        product_ids::T818,
        product_ids::T80,
        product_ids::T80_FERRARI_488,
        product_ids::NASCAR_PRO_FF2,
        product_ids::FGT_RUMBLE_FORCE,
        product_ids::RGT_FF_CLUTCH,
        product_ids::FGT_FORCE_FEEDBACK,
        product_ids::F430_FORCE_FEEDBACK,
    ];

    #[test]
    fn all_wheelbases_parse_center_steering() -> Result<(), String> {
        let report = build_input_report(0x8000, 0, 0, 0);
        for &pid in WHEELBASE_PIDS {
            let proto = ThrustmasterProtocol::new(pid);
            let state = proto
                .parse_input(&report)
                .ok_or(format!("PID 0x{pid:04X} failed to parse center steering"))?;
            assert!(
                state.steering.abs() < 0.001,
                "PID 0x{pid:04X}: center steering should be ~0.0, got {}",
                state.steering
            );
        }
        Ok(())
    }

    #[test]
    fn t300rs_parses_full_range_steering() -> Result<(), String> {
        let proto = ThrustmasterProtocol::new(product_ids::T300_RS);

        // Full left
        let report_left = build_input_report(0x0000, 0, 0, 0);
        let state_left = proto
            .parse_input(&report_left)
            .ok_or("T300RS left parse failed")?;
        assert!(
            (state_left.steering + 1.0).abs() < 0.001,
            "T300RS full left should be ~-1.0, got {}",
            state_left.steering
        );

        // Full right
        let report_right = build_input_report(0xFFFF, 0, 0, 0);
        let state_right = proto
            .parse_input(&report_right)
            .ok_or("T300RS right parse failed")?;
        assert!(
            (state_right.steering - 1.0).abs() < 0.001,
            "T300RS full right should be ~1.0, got {}",
            state_right.steering
        );

        Ok(())
    }

    #[test]
    fn t500rs_parses_pedals() -> Result<(), String> {
        let proto = ThrustmasterProtocol::new(product_ids::T500_RS);
        let report = build_input_report(0x8000, 0xFF, 0x80, 0x40);
        let state = proto
            .parse_input(&report)
            .ok_or("T500RS pedal parse failed")?;
        assert!(
            (state.throttle - 1.0).abs() < 0.01,
            "T500RS throttle at 0xFF should be ~1.0"
        );
        assert!(
            (state.brake - 0.502).abs() < 0.01,
            "T500RS brake at 0x80 should be ~0.502"
        );
        assert!(
            (state.clutch - 0.251).abs() < 0.01,
            "T500RS clutch at 0x40 should be ~0.251"
        );
        Ok(())
    }

    #[test]
    fn tgt_ii_gt_mode_parses_input() -> Result<(), String> {
        let proto = ThrustmasterProtocol::new(product_ids::T_GT_II_GT);
        let report = build_input_report(0x4000, 0x80, 0x00, 0x00);
        let state = proto
            .parse_input(&report)
            .ok_or("T-GT II GT mode parse failed")?;
        // 0x4000 = 16384 → (16384 - 32768) / 32768 = -0.5
        assert!(
            (state.steering + 0.5).abs() < 0.001,
            "T-GT II steering at 0x4000 should be ~-0.5, got {}",
            state.steering
        );
        Ok(())
    }

    #[test]
    fn ts_xw_parses_buttons_and_hat() -> Result<(), String> {
        let proto = ThrustmasterProtocol::new(product_ids::TS_XW);
        let mut report = build_input_report(0x8000, 0, 0, 0);
        report[6] = 0xFF;
        report[7] = 0xFF;
        report[8] = 0x03; // hat = 3 (left)
        let state = proto
            .parse_input(&report)
            .ok_or("TS-XW button parse failed")?;
        assert_eq!(state.buttons, 0xFFFF, "TS-XW all buttons should be set");
        assert_eq!(state.hat, 0x03, "TS-XW hat should be 3 (left)");
        Ok(())
    }

    #[test]
    fn ts_xw_gip_parses_paddles() -> Result<(), String> {
        let proto = ThrustmasterProtocol::new(product_ids::TS_XW_GIP);
        let mut report = build_input_report(0x8000, 0, 0, 0);
        report[9] = 0x01; // right paddle only
        let state = proto
            .parse_input(&report)
            .ok_or("TS-XW GIP paddle parse failed")?;
        assert!(state.paddle_right, "right paddle should be pressed");
        assert!(!state.paddle_left, "left paddle should not be pressed");
        Ok(())
    }

    #[test]
    fn tx_racing_both_pids_parse_identically() -> Result<(), String> {
        let report = build_input_report(0xC000, 0x40, 0x80, 0xC0);
        let proto_active = ThrustmasterProtocol::new(product_ids::TX_RACING);
        let proto_orig = ThrustmasterProtocol::new(product_ids::TX_RACING_ORIG);
        let state_a = proto_active
            .parse_input(&report)
            .ok_or("TX active parse failed")?;
        let state_b = proto_orig
            .parse_input(&report)
            .ok_or("TX orig parse failed")?;
        assert!(
            (state_a.steering - state_b.steering).abs() < f32::EPSILON,
            "TX Racing both PIDs should parse identically"
        );
        assert!(
            (state_a.throttle - state_b.throttle).abs() < f32::EPSILON,
            "TX Racing throttle should match"
        );
        Ok(())
    }

    #[test]
    fn t248_parses_input() -> Result<(), String> {
        let proto = ThrustmasterProtocol::new(product_ids::T248);
        let report = build_input_report(0x8000, 0xFF, 0xFF, 0xFF);
        let state = proto.parse_input(&report).ok_or("T248 parse failed")?;
        assert!(
            (state.throttle - 1.0).abs() < 0.001,
            "T248 full throttle should be ~1.0"
        );
        assert!(
            (state.brake - 1.0).abs() < 0.001,
            "T248 full brake should be ~1.0"
        );
        assert!(
            (state.clutch - 1.0).abs() < 0.001,
            "T248 full clutch should be ~1.0"
        );
        Ok(())
    }

    #[test]
    fn tlcm_is_not_classified_as_pedal_in_identify_device() {
        // T-LCM PID (0xB371) falls through to Unknown category in identify_device
        // because no match arm explicitly categorizes it as Pedals.
        // The protocol handler will still parse input reports for it.
        let proto = ThrustmasterProtocol::new(product_ids::T_LCM);
        assert!(!proto.is_pedals(), "T-LCM is not currently classified as Pedals");
        assert_eq!(proto.model(), Model::TLCM);
        assert!(!proto.supports_ffb(), "T-LCM does not support FFB");
    }

    #[test]
    fn t150_and_tmx_parse_identically() -> Result<(), String> {
        let report = build_input_report(0x6000, 0xAA, 0x55, 0x33);
        let state_t150 = ThrustmasterProtocol::new(product_ids::T150)
            .parse_input(&report)
            .ok_or("T150 parse failed")?;
        let state_tmx = ThrustmasterProtocol::new(product_ids::TMX)
            .parse_input(&report)
            .ok_or("TMX parse failed")?;
        assert!(
            (state_t150.steering - state_tmx.steering).abs() < f32::EPSILON,
            "T150 and TMX should parse steering identically"
        );
        assert!(
            (state_t150.throttle - state_tmx.throttle).abs() < f32::EPSILON,
            "T150 and TMX should parse throttle identically"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Output report construction (FFB, mode switching)
// ═══════════════════════════════════════════════════════════════════════════════

mod output_report_construction {
    use super::*;

    #[test]
    fn constant_force_encoder_half_torque_per_model() {
        // For each model, verify half-torque encodes to ~5000
        let models_with_torque: &[(u16, f32)] = &[
            (product_ids::T300_RS, 4.0),
            (product_ids::T500_RS, 5.0),
            (product_ids::TS_XW, 6.0),
            (product_ids::T818, 10.0),
            (product_ids::T150, 2.5),
        ];
        for &(pid, expected_max) in models_with_torque {
            let proto = ThrustmasterProtocol::new(pid);
            let enc = proto.create_encoder();
            let mut out = [0u8; EFFECT_REPORT_LEN];
            enc.encode(expected_max / 2.0, &mut out);
            let mag = i16::from_le_bytes([out[2], out[3]]);
            assert_eq!(
                mag, 5000,
                "PID 0x{pid:04X}: half torque should encode to 5000, got {mag}"
            );
        }
    }

    #[test]
    fn encoder_trait_dispatches_correctly() {
        let enc = ThrustmasterConstantForceEncoder::new(6.0);
        let trait_enc: &dyn tm::ThrustmasterEffectEncoder = &enc;
        let mut out = [0u8; EFFECT_REPORT_LEN];
        trait_enc.encode(3.0, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, 5000, "trait dispatch should encode correctly");
    }

    #[test]
    fn encoder_trait_encode_zero() {
        let enc = ThrustmasterConstantForceEncoder::new(6.0);
        let trait_enc: &dyn tm::ThrustmasterEffectEncoder = &enc;
        let mut out = [0u8; EFFECT_REPORT_LEN];
        trait_enc.encode_zero(&mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, 0, "encode_zero via trait should produce magnitude 0");
        assert_eq!(out[0], 0x23, "report ID should be CONSTANT_FORCE");
    }

    #[test]
    fn encoder_tiny_max_torque_floors_to_001() {
        // max_torque_nm of 0.0 should floor to 0.01
        let enc = ThrustmasterConstantForceEncoder::new(0.0);
        let mut out = [0u8; EFFECT_REPORT_LEN];
        enc.encode(0.005, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        // 0.005 / 0.01 = 0.5 → 5000
        assert_eq!(
            mag, 5000,
            "tiny max_torque should floor to 0.01, got mag={mag}"
        );
    }

    #[test]
    fn init_sequence_contains_correct_reports() {
        let proto = ThrustmasterProtocol::new(product_ids::T300_RS);
        let seq = proto.build_init_sequence();
        assert_eq!(seq.len(), 4, "init sequence should have 4 reports");

        // First: gain=0
        assert_eq!(seq[0], vec![0x81, 0x00], "first report: device gain zero");
        // Second: gain=0xFF (default)
        assert_eq!(seq[1], vec![0x81, 0xFF], "second report: device gain full");
        // Third: actuator enable
        assert_eq!(seq[2], vec![0x82, 0x01], "third report: actuator enable");
        // Fourth: set range (1080 for T300RS)
        assert_eq!(seq[3][0], 0x80, "fourth report: set range report ID");
        let range_degrees = u16::from_le_bytes([seq[3][2], seq[3][3]]);
        assert_eq!(
            range_degrees, 1080,
            "T300RS init should set range to 1080°"
        );
    }

    #[test]
    fn init_sequence_with_custom_gain() {
        let mut proto = ThrustmasterProtocol::new(product_ids::T248);
        proto.set_gain(0x80);
        let seq = proto.build_init_sequence();
        assert_eq!(seq[1], vec![0x81, 0x80], "custom gain should be 0x80");
    }

    #[test]
    fn init_sequence_with_custom_range() {
        let mut proto = ThrustmasterProtocol::new(product_ids::T300_RS);
        proto.set_rotation_range(540);
        let seq = proto.build_init_sequence();
        let range_degrees = u16::from_le_bytes([seq[3][2], seq[3][3]]);
        assert_eq!(range_degrees, 540, "custom range should be 540°");
    }

    #[test]
    fn spring_effect_encodes_negative_center() {
        let effect = tm::build_spring_effect(-1000, 2000);
        assert_eq!(effect[0], 0x22, "EFFECT_OP report ID");
        assert_eq!(effect[1], 0x40, "SPRING effect type");
        let center = i16::from_le_bytes([effect[3], effect[4]]);
        assert_eq!(center, -1000, "center should be -1000");
        let stiffness = u16::from_le_bytes([effect[5], effect[6]]);
        assert_eq!(stiffness, 2000, "stiffness should be 2000");
    }

    #[test]
    fn damper_and_friction_have_distinct_type_codes() {
        let damper = tm::build_damper_effect(100);
        let friction = tm::build_friction_effect(100, 200);
        assert_ne!(
            damper[1], friction[1],
            "damper (0x41) and friction (0x43) must have different type codes"
        );
    }

    #[test]
    fn kernel_open_and_close_differ() {
        let open = tm::build_kernel_open_command();
        let close = tm::build_kernel_close_command();
        assert_ne!(open, close, "open and close commands must differ");
        assert_eq!(open[0], close[0], "both use same command prefix 0x01");
        assert_eq!(open[1], 0x05, "open code is 0x05");
        assert_eq!(close[1], 0x00, "close code is 0x00");
    }

    #[test]
    fn kernel_autocenter_value_zero_disables() {
        let cmds = tm::build_kernel_autocenter_commands(0);
        let value = u16::from_le_bytes([cmds[1][2], cmds[1][3]]);
        assert_eq!(value, 0, "autocenter value 0 should encode as 0");
    }

    #[test]
    fn kernel_autocenter_value_max() {
        let cmds = tm::build_kernel_autocenter_commands(0xFFFF);
        let value = u16::from_le_bytes([cmds[1][2], cmds[1][3]]);
        assert_eq!(value, 0xFFFF, "autocenter max value should round-trip");
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Device identification via PID for all supported wheelbases
// ═══════════════════════════════════════════════════════════════════════════════

mod device_identification {
    use super::*;

    /// Every known PID must map to the correct Model variant.
    #[test]
    fn all_pids_map_to_correct_model() {
        let expected: &[(u16, Model)] = &[
            (product_ids::T150, Model::T150),
            (product_ids::TMX, Model::TMX),
            (product_ids::T300_RS, Model::T300RS),
            (product_ids::T300_RS_PS4, Model::T300RSPS4),
            (product_ids::T300_RS_GT, Model::T300RSGT),
            (product_ids::TX_RACING, Model::TXRacing),
            (product_ids::TX_RACING_ORIG, Model::TXRacing),
            (product_ids::T500_RS, Model::T500RS),
            (product_ids::T248, Model::T248),
            (product_ids::T248X, Model::T248X),
            (product_ids::TS_PC_RACER, Model::TSPCRacer),
            (product_ids::TS_XW, Model::TSXW),
            (product_ids::TS_XW_GIP, Model::TSXW),
            (product_ids::T_GT_II_GT, Model::TGTII),
            (product_ids::T818, Model::T818),
            (product_ids::T80, Model::T80),
            (product_ids::T80_FERRARI_488, Model::T80),
            (product_ids::NASCAR_PRO_FF2, Model::NascarProFF2),
            (product_ids::FGT_RUMBLE_FORCE, Model::FGTRumbleForce),
            (product_ids::RGT_FF_CLUTCH, Model::RGTFF),
            (product_ids::FGT_FORCE_FEEDBACK, Model::FGTForceFeedback),
            (product_ids::F430_FORCE_FEEDBACK, Model::F430ForceFeedback),
            (product_ids::T_LCM, Model::TLCM),
        ];
        for &(pid, expected_model) in expected {
            let actual = Model::from_product_id(pid);
            assert_eq!(
                actual, expected_model,
                "PID 0x{pid:04X} should map to {expected_model:?}, got {actual:?}"
            );
        }
    }

    /// TPR_PEDALS maps to Unknown (pedals, no FFB).
    #[test]
    fn tpr_pedals_maps_to_unknown() {
        assert_eq!(
            Model::from_product_id(product_ids::TPR_PEDALS),
            Model::Unknown
        );
    }

    /// FFB_WHEEL_GENERIC (pre-init) maps to Unknown.
    #[test]
    fn generic_ffb_wheel_maps_to_unknown() {
        assert_eq!(
            Model::from_product_id(product_ids::FFB_WHEEL_GENERIC),
            Model::Unknown
        );
    }

    /// All dual-PID models resolve to the same Model variant.
    #[test]
    fn dual_pid_models_resolve_consistently() {
        // TX Racing: active and original PIDs
        assert_eq!(
            Model::from_product_id(product_ids::TX_RACING),
            Model::from_product_id(product_ids::TX_RACING_ORIG)
        );
        // TS-XW: USB and GIP PIDs
        assert_eq!(
            Model::from_product_id(product_ids::TS_XW),
            Model::from_product_id(product_ids::TS_XW_GIP)
        );
        // T80: standard and Ferrari 488
        assert_eq!(
            Model::from_product_id(product_ids::T80),
            Model::from_product_id(product_ids::T80_FERRARI_488)
        );
    }

    /// identify_device returns correct category and FFB support for all known wheelbases.
    #[test]
    fn identify_device_categories_correct() {
        // Wheelbases with FFB
        let ffb_wheelbases = [
            product_ids::T150,
            product_ids::T300_RS,
            product_ids::T500_RS,
            product_ids::T818,
            product_ids::TS_XW,
            product_ids::TS_PC_RACER,
            product_ids::T248,
        ];
        for pid in ffb_wheelbases {
            let ident = tm::identify_device(pid);
            assert_eq!(
                ident.category,
                tm::ThrustmasterDeviceCategory::Wheelbase,
                "PID 0x{pid:04X} should be Wheelbase"
            );
            assert!(
                ident.supports_ffb,
                "PID 0x{pid:04X} should support FFB"
            );
        }

        // T80: wheelbase without FFB
        let ident_t80 = tm::identify_device(product_ids::T80);
        assert_eq!(ident_t80.category, tm::ThrustmasterDeviceCategory::Wheelbase);
        assert!(!ident_t80.supports_ffb);
    }

    /// Model names all contain "Thrustmaster" and are non-empty.
    #[test]
    fn all_model_names_valid() {
        let all_models = [
            Model::T150,
            Model::TMX,
            Model::T300RS,
            Model::T300RSPS4,
            Model::T300RSGT,
            Model::TXRacing,
            Model::T500RS,
            Model::T248,
            Model::T248X,
            Model::TGT,
            Model::TGTII,
            Model::TSPCRacer,
            Model::TSXW,
            Model::T818,
            Model::T80,
            Model::NascarProFF2,
            Model::FGTRumbleForce,
            Model::RGTFF,
            Model::FGTForceFeedback,
            Model::F430ForceFeedback,
            Model::T3PA,
            Model::T3PAPro,
            Model::TLCM,
            Model::TLCMPro,
            Model::Unknown,
        ];
        for model in all_models {
            let name = model.name();
            assert!(!name.is_empty(), "{model:?} name must not be empty");
            assert!(
                name.contains("Thrustmaster"),
                "{model:?} name must contain 'Thrustmaster', got '{name}'"
            );
        }
    }

    /// Protocol family classification is correct for all major models.
    #[test]
    fn protocol_family_classification() {
        use tm::ProtocolFamily;
        // T300 family
        let t300_models = [
            Model::T300RS,
            Model::T300RSPS4,
            Model::T300RSGT,
            Model::TXRacing,
            Model::T248,
            Model::T248X,
            Model::TSPCRacer,
            Model::TSXW,
            Model::TGTII,
        ];
        for model in t300_models {
            assert_eq!(
                model.protocol_family(),
                ProtocolFamily::T300,
                "{model:?} should be T300 family"
            );
        }
        // T150 family
        assert_eq!(Model::T150.protocol_family(), ProtocolFamily::T150);
        assert_eq!(Model::TMX.protocol_family(), ProtocolFamily::T150);
        // T500 family
        assert_eq!(Model::T500RS.protocol_family(), ProtocolFamily::T500);
        // Unknown
        assert_eq!(Model::Unknown.protocol_family(), ProtocolFamily::Unknown);
        assert_eq!(Model::T80.protocol_family(), ProtocolFamily::Unknown);
        assert_eq!(Model::TGT.protocol_family(), ProtocolFamily::Unknown);
        assert_eq!(Model::T3PA.protocol_family(), ProtocolFamily::Unknown);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Axis parsing precision and dead zone handling
// ═══════════════════════════════════════════════════════════════════════════════

mod axis_precision {
    use super::*;

    #[test]
    fn steering_near_center_precision() -> Result<(), String> {
        // Values near center (0x8000) should produce small normalized values
        let offsets: &[(u16, f32)] = &[
            (0x8000, 0.0),          // exact center
            (0x8001, 1.0 / 32768.0), // one LSB right of center
            (0x7FFF, -1.0 / 32768.0), // one LSB left of center
        ];
        for &(raw, expected) in offsets {
            let report = build_input_report(raw, 0, 0, 0);
            let state =
                tm::parse_input_report(&report).ok_or("parse failed")?;
            assert!(
                (state.steering - expected).abs() < 1e-4,
                "steering at 0x{raw:04X}: expected {expected}, got {}",
                state.steering
            );
        }
        Ok(())
    }

    #[test]
    fn steering_quarter_positions() -> Result<(), String> {
        // Quarter positions: -0.5, -0.25, 0.25, 0.5
        let cases: &[(u16, f32)] = &[
            (0x4000, -0.5),  // quarter left
            (0x6000, -0.25), // eighth left
            (0xA000, 0.25),  // eighth right
            (0xC000, 0.5),   // quarter right
        ];
        for &(raw, expected) in cases {
            let report = build_input_report(raw, 0, 0, 0);
            let state =
                tm::parse_input_report(&report).ok_or("parse failed")?;
            assert!(
                (state.steering - expected).abs() < 0.001,
                "steering at 0x{raw:04X}: expected {expected}, got {}",
                state.steering
            );
        }
        Ok(())
    }

    #[test]
    fn pedal_boundary_values() -> Result<(), String> {
        // Test min, mid, and max for each pedal axis
        let cases: &[(u8, f32)] = &[
            (0, 0.0),
            (1, 1.0 / 255.0),
            (127, 127.0 / 255.0),
            (128, 128.0 / 255.0),
            (254, 254.0 / 255.0),
            (255, 1.0),
        ];
        for &(raw_val, expected) in cases {
            let report = build_input_report(0x8000, raw_val, raw_val, raw_val);
            let state =
                tm::parse_input_report(&report).ok_or("parse failed")?;
            assert!(
                (state.throttle - expected).abs() < 0.001,
                "throttle at {raw_val}: expected {expected}, got {}",
                state.throttle
            );
            assert!(
                (state.brake - expected).abs() < 0.001,
                "brake at {raw_val}: expected {expected}, got {}",
                state.brake
            );
            assert!(
                (state.clutch - expected).abs() < 0.001,
                "clutch at {raw_val}: expected {expected}, got {}",
                state.clutch
            );
        }
        Ok(())
    }

    #[test]
    fn pedal_raw_normalize_boundary() {
        // Raw pedal axes: 0 normalizes to 0.0, 255 normalizes to 1.0
        let raw = tm::ThrustmasterPedalAxesRaw {
            throttle: 0,
            brake: 255,
            clutch: None,
        };
        let norm = raw.normalize();
        assert!((norm.throttle - 0.0).abs() < f32::EPSILON);
        assert!((norm.brake - 1.0).abs() < f32::EPSILON);
        assert!(norm.clutch.is_none());
    }

    #[test]
    fn pedal_raw_normalize_with_clutch() {
        let raw = tm::ThrustmasterPedalAxesRaw {
            throttle: 128,
            brake: 64,
            clutch: Some(192),
        };
        let norm = raw.normalize();
        assert!((norm.throttle - 128.0 / 255.0).abs() < 0.001);
        assert!((norm.brake - 64.0 / 255.0).abs() < 0.001);
        let clutch = norm.clutch.ok_or("clutch should be Some");
        assert!(clutch.is_ok());
    }

    #[test]
    fn hat_switch_all_positions() -> Result<(), String> {
        let hat_values = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        for hat in hat_values {
            let mut report = build_input_report(0x8000, 0, 0, 0);
            report[8] = hat;
            let state = tm::parse_input_report(&report)
                .ok_or(format!("parse failed for hat={hat}"))?;
            assert_eq!(
                state.hat,
                hat & 0x0F,
                "hat value {hat} should be preserved"
            );
        }
        Ok(())
    }

    #[test]
    fn hat_switch_masks_upper_nibble() -> Result<(), String> {
        let mut report = build_input_report(0x8000, 0, 0, 0);
        report[8] = 0xF3; // upper nibble should be masked
        let state = tm::parse_input_report(&report).ok_or("parse failed")?;
        assert_eq!(state.hat, 0x03, "hat should mask to lower nibble only");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Edge cases: short reports, invalid PIDs, mode transitions
// ═══════════════════════════════════════════════════════════════════════════════

mod edge_cases {
    use super::*;

    #[test]
    fn parse_report_exactly_10_bytes() -> Result<(), String> {
        let data = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
        let state = tm::parse_input_report(&data).ok_or("10-byte report should parse")?;
        assert!(
            state.steering.abs() < 0.001,
            "minimum-length report should parse center steering"
        );
        Ok(())
    }

    #[test]
    fn parse_report_9_bytes_fails() {
        let data = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08];
        assert!(
            tm::parse_input_report(&data).is_none(),
            "9-byte report should fail"
        );
    }

    #[test]
    fn parse_empty_report() {
        assert!(
            tm::parse_input_report(&[]).is_none(),
            "empty report should return None"
        );
    }

    #[test]
    fn parse_report_id_zero() {
        let data = [0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
        assert!(
            tm::parse_input_report(&data).is_none(),
            "report ID 0x00 should be rejected"
        );
    }

    #[test]
    fn parse_pedal_report_exactly_3_bytes() -> Result<(), String> {
        let data = [0x10, 0x20, 0x30];
        let pedals = tm::input::parse_pedal_report(&data).ok_or("parse failed")?;
        assert_eq!(pedals.throttle, 0x10);
        assert_eq!(pedals.brake, 0x20);
        assert_eq!(pedals.clutch, Some(0x30));
        Ok(())
    }

    #[test]
    fn parse_pedal_report_2_bytes_fails() {
        assert!(tm::input::parse_pedal_report(&[0x10, 0x20]).is_none());
    }

    #[test]
    fn parse_pedal_report_1_byte_fails() {
        assert!(tm::input::parse_pedal_report(&[0x10]).is_none());
    }

    #[test]
    fn parse_pedal_report_empty_fails() {
        assert!(tm::input::parse_pedal_report(&[]).is_none());
    }

    #[test]
    fn invalid_pid_produces_unknown_model() {
        let invalid_pids = [0x0000, 0x0001, 0xDEAD, 0xFFFF, 0x1234];
        for pid in invalid_pids {
            assert_eq!(
                Model::from_product_id(pid),
                Model::Unknown,
                "PID 0x{pid:04X} should map to Unknown"
            );
        }
    }

    #[test]
    fn unknown_model_properties() {
        let model = Model::Unknown;
        assert!(!model.supports_ffb());
        assert_eq!(model.protocol_family(), tm::ProtocolFamily::Unknown);
        assert!(model.init_switch_value().is_none());
        assert_eq!(model.max_rotation_deg(), 900); // default fallback
        assert!((model.max_torque_nm() - 4.0).abs() < 0.01); // default
    }

    // ── Mode transitions ─────────────────────────────────────────────────

    #[test]
    fn init_state_transitions_complete_lifecycle() {
        let mut proto = ThrustmasterProtocol::new(product_ids::T300_RS);
        assert_eq!(proto.init_state(), ThrustmasterInitState::Uninitialized);

        proto.init();
        assert_eq!(proto.init_state(), ThrustmasterInitState::Ready);

        proto.reset();
        assert_eq!(proto.init_state(), ThrustmasterInitState::Uninitialized);

        // Re-init should work
        proto.init();
        assert_eq!(proto.init_state(), ThrustmasterInitState::Ready);
    }

    #[test]
    fn double_init_stays_ready() {
        let mut proto = ThrustmasterProtocol::new(product_ids::T300_RS);
        proto.init();
        proto.init();
        assert_eq!(proto.init_state(), ThrustmasterInitState::Ready);
    }

    #[test]
    fn double_reset_stays_uninitialized() {
        let mut proto = ThrustmasterProtocol::new(product_ids::T300_RS);
        proto.reset();
        proto.reset();
        assert_eq!(proto.init_state(), ThrustmasterInitState::Uninitialized);
    }

    #[test]
    fn new_with_config_preserves_values() {
        let proto = ThrustmasterProtocol::new_with_config(0xBEEF, 8.5, 720);
        assert_eq!(proto.product_id(), 0xBEEF);
        assert_eq!(proto.model(), Model::Unknown);
        assert!((proto.max_torque_nm() - 8.5).abs() < 0.001);
        assert_eq!(proto.rotation_range(), 720);
        assert_eq!(proto.init_state(), ThrustmasterInitState::Uninitialized);
    }

    #[test]
    fn new_with_config_floors_tiny_torque() {
        let proto = ThrustmasterProtocol::new_with_config(0x1234, 0.001, 900);
        // Should be floored to at least 0.01
        assert!(
            proto.max_torque_nm() >= 0.01,
            "torque should be floored to >= 0.01"
        );
    }

    #[test]
    fn protocol_default_is_t300rs() {
        let proto = ThrustmasterProtocol::default();
        assert_eq!(proto.model(), Model::T300RS);
        assert!(proto.supports_ffb());
        assert_eq!(proto.rotation_range(), 1080);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Property tests for encoding/decoding
// ═══════════════════════════════════════════════════════════════════════════════

mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Any valid input report (ID 0x01, >= 10 bytes) must parse successfully.
        #[test]
        fn prop_valid_reports_always_parse(
            steer: u16,
            throttle: u8,
            brake: u8,
            clutch: u8,
            buttons: u16,
            hat: u8,
            paddles: u8,
        ) {
            let [s_lo, s_hi] = steer.to_le_bytes();
            let [b_lo, b_hi] = buttons.to_le_bytes();
            let data = vec![0x01, s_lo, s_hi, throttle, brake, clutch, b_lo, b_hi, hat, paddles];
            let result = tm::parse_input_report(&data);
            prop_assert!(result.is_some(), "valid report must always parse");
        }

        /// Parsed steering monotonically increases with raw value.
        #[test]
        fn prop_steering_monotonic(a: u16, b: u16) {
            if a <= b { return Ok(()); }
            let report_a = build_input_report(a, 0, 0, 0);
            let report_b = build_input_report(b, 0, 0, 0);
            if let (Some(sa), Some(sb)) = (
                tm::parse_input_report(&report_a),
                tm::parse_input_report(&report_b),
            ) {
                prop_assert!(
                    sa.steering >= sb.steering,
                    "steering must be monotonic: raw {a} > {b} but {} < {}",
                    sa.steering,
                    sb.steering
                );
            }
        }

        /// Throttle/brake/clutch monotonically increase with raw value.
        #[test]
        fn prop_pedal_axes_monotonic(a: u8, b: u8) {
            if a <= b { return Ok(()); }
            let report_a = build_input_report(0x8000, a, a, a);
            let report_b = build_input_report(0x8000, b, b, b);
            if let (Some(sa), Some(sb)) = (
                tm::parse_input_report(&report_a),
                tm::parse_input_report(&report_b),
            ) {
                prop_assert!(sa.throttle >= sb.throttle);
                prop_assert!(sa.brake >= sb.brake);
                prop_assert!(sa.clutch >= sb.clutch);
            }
        }

        /// Buttons round-trip: encoded u16 LE in report bytes 6-7 decodes identically.
        #[test]
        fn prop_buttons_roundtrip(buttons: u16) {
            let mut report = build_input_report(0x8000, 0, 0, 0);
            let [lo, hi] = buttons.to_le_bytes();
            report[6] = lo;
            report[7] = hi;
            if let Some(state) = tm::parse_input_report(&report) {
                prop_assert_eq!(state.buttons, buttons, "buttons must round-trip");
            }
        }

        /// Paddle bits round-trip correctly.
        #[test]
        fn prop_paddles_roundtrip(paddle_byte: u8) {
            let mut report = build_input_report(0x8000, 0, 0, 0);
            report[9] = paddle_byte;
            if let Some(state) = tm::parse_input_report(&report) {
                prop_assert_eq!(
                    state.paddle_right,
                    (paddle_byte & 0x01) != 0,
                    "right paddle bit must round-trip"
                );
                prop_assert_eq!(
                    state.paddle_left,
                    (paddle_byte & 0x02) != 0,
                    "left paddle bit must round-trip"
                );
            }
        }

        /// Constant force encoder produces symmetric output for ±torque.
        #[test]
        fn prop_encoder_symmetry(
            max in 0.1_f32..=20.0_f32,
            frac in 0.0_f32..=1.0_f32,
        ) {
            let torque = max * frac;
            let enc = ThrustmasterConstantForceEncoder::new(max);
            let mut out_pos = [0u8; EFFECT_REPORT_LEN];
            let mut out_neg = [0u8; EFFECT_REPORT_LEN];
            enc.encode(torque, &mut out_pos);
            enc.encode(-torque, &mut out_neg);
            let mag_pos = i16::from_le_bytes([out_pos[2], out_pos[3]]);
            let mag_neg = i16::from_le_bytes([out_neg[2], out_neg[3]]);
            // Due to integer truncation, allow ±1 asymmetry
            prop_assert!(
                (mag_pos + mag_neg).abs() <= 1,
                "symmetric torques must produce symmetric magnitudes: +{mag_pos} vs {mag_neg}"
            );
        }

        /// Protocol new_with_config always preserves custom PID and range.
        #[test]
        fn prop_new_with_config_preserves(
            pid: u16,
            range in 100u16..=1080u16,
            torque in 0.1_f32..=20.0_f32,
        ) {
            let proto = ThrustmasterProtocol::new_with_config(pid, torque, range);
            prop_assert_eq!(proto.product_id(), pid);
            prop_assert_eq!(proto.rotation_range(), range);
            prop_assert!(proto.max_torque_nm() >= 0.01);
        }

        /// set_gain/set_rotation_range round-trip through the protocol.
        #[test]
        fn prop_gain_and_range_setters(gain: u8, range: u16) {
            let mut proto = ThrustmasterProtocol::new(product_ids::T300_RS);
            proto.set_gain(gain);
            proto.set_rotation_range(range);
            prop_assert_eq!(proto.gain(), gain);
            prop_assert_eq!(proto.rotation_range(), range);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Known constant validation
// ═══════════════════════════════════════════════════════════════════════════════

mod known_constants {
    use super::*;

    #[test]
    fn vendor_id_is_correct() {
        assert_eq!(THRUSTMASTER_VENDOR_ID, 0x044F);
    }

    #[test]
    fn effect_report_len_is_8() {
        assert_eq!(EFFECT_REPORT_LEN, 8);
    }

    #[test]
    fn standard_input_report_id_is_0x01() {
        assert_eq!(tm::input::STANDARD_INPUT_REPORT_ID, 0x01);
    }

    #[test]
    fn output_report_ids_correct() {
        assert_eq!(tm::output::report_ids::VENDOR_SET_RANGE, 0x80);
        assert_eq!(tm::output::report_ids::DEVICE_GAIN, 0x81);
        assert_eq!(tm::output::report_ids::ACTUATOR_ENABLE, 0x82);
        assert_eq!(tm::output::report_ids::CONSTANT_FORCE, 0x23);
        assert_eq!(tm::output::report_ids::EFFECT_OP, 0x22);
    }

    #[test]
    fn output_commands_correct() {
        assert_eq!(tm::output::commands::SET_RANGE, 0x01);
        assert_eq!(tm::output::commands::ENABLE, 0x01);
        assert_eq!(tm::output::commands::DISABLE, 0x00);
    }

    #[test]
    fn effect_type_constants_correct() {
        assert_eq!(tm::output::EFFECT_TYPE_CONSTANT, 0x26);
        assert_eq!(tm::output::EFFECT_TYPE_RAMP, 0x27);
        assert_eq!(tm::output::EFFECT_TYPE_SPRING, 0x40);
        assert_eq!(tm::output::EFFECT_TYPE_DAMPER, 0x41);
        assert_eq!(tm::output::EFFECT_TYPE_FRICTION, 0x43);
    }

    #[test]
    fn t150_command_bytes_correct() {
        assert_eq!(tm::t150::CMD_RANGE, 0x40);
        assert_eq!(tm::t150::CMD_EFFECT, 0x41);
        assert_eq!(tm::t150::CMD_GAIN, 0x43);
        assert_eq!(tm::t150::SUBCMD_RANGE, 0x11);
    }

    #[test]
    fn t150_effect_type_values_correct() {
        assert_eq!(tm::T150EffectType::Constant.as_u16(), 0x4000);
        assert_eq!(tm::T150EffectType::Sine.as_u16(), 0x4022);
        assert_eq!(tm::T150EffectType::SawtoothUp.as_u16(), 0x4023);
        assert_eq!(tm::T150EffectType::SawtoothDown.as_u16(), 0x4024);
        assert_eq!(tm::T150EffectType::Spring.as_u16(), 0x4040);
        assert_eq!(tm::T150EffectType::Damper.as_u16(), 0x4041);
    }

    #[test]
    fn init_protocol_constants_correct() {
        assert_eq!(init_protocol::MODEL_QUERY_REQUEST, 73);
        assert_eq!(init_protocol::MODE_SWITCH_REQUEST, 83);
        assert_eq!(init_protocol::MODEL_QUERY_REQUEST_TYPE, 0xC1);
        assert_eq!(init_protocol::MODE_SWITCH_REQUEST_TYPE, 0x41);
        assert_eq!(init_protocol::MODEL_RESPONSE_LEN, 0x0010);
    }

    #[test]
    fn init_protocol_setup_interrupts_count() {
        assert_eq!(init_protocol::SETUP_INTERRUPTS.len(), 5);
        // First interrupt starts with 0x42
        assert_eq!(init_protocol::SETUP_INTERRUPTS[0][0], 0x42);
    }

    #[test]
    fn init_protocol_known_models_entries() {
        assert_eq!(init_protocol::KNOWN_MODELS.len(), 7);

        // Verify specific entries
        let t150_entry = init_protocol::KNOWN_MODELS
            .iter()
            .find(|(code, _, _)| *code == 0x0306);
        assert!(t150_entry.is_some(), "T150 RS entry must exist");
        if let Some((_, switch, name)) = t150_entry {
            assert_eq!(*switch, 0x0006);
            assert!(name.contains("T150"));
        }

        let t500_entry = init_protocol::KNOWN_MODELS
            .iter()
            .find(|(code, _, _)| *code == 0x0002);
        assert!(t500_entry.is_some(), "T500 RS entry must exist");
        if let Some((_, switch, _)) = t500_entry {
            assert_eq!(*switch, 0x0002);
        }
    }

    #[test]
    fn init_switch_values_match_known_models() {
        // T150 family: switch value 0x0006
        assert_eq!(Model::T150.init_switch_value(), Some(0x0006));
        assert_eq!(Model::TMX.init_switch_value(), Some(0x0006));

        // T300 family: switch value 0x0005
        assert_eq!(Model::T300RS.init_switch_value(), Some(0x0005));
        assert_eq!(Model::T248.init_switch_value(), Some(0x0005));
        assert_eq!(Model::TGTII.init_switch_value(), Some(0x0005));

        // T500: switch value 0x0002
        assert_eq!(Model::T500RS.init_switch_value(), Some(0x0002));

        // No switch value for these
        assert!(Model::T80.init_switch_value().is_none());
        assert!(Model::Unknown.init_switch_value().is_none());
        assert!(Model::TGT.init_switch_value().is_none());
        assert!(Model::T3PA.init_switch_value().is_none());
    }

    #[test]
    fn all_pid_constants_are_nonzero_and_unique() {
        let all_pids = [
            product_ids::FFB_WHEEL_GENERIC,
            product_ids::T150,
            product_ids::TMX,
            product_ids::T300_RS,
            product_ids::T300_RS_PS4,
            product_ids::T300_RS_GT,
            product_ids::TX_RACING,
            product_ids::TX_RACING_ORIG,
            product_ids::T500_RS,
            product_ids::T248,
            product_ids::T248X,
            product_ids::TS_PC_RACER,
            product_ids::TS_XW,
            product_ids::TS_XW_GIP,
            product_ids::T_GT_II_GT,
            product_ids::T818,
            product_ids::T80,
            product_ids::T80_FERRARI_488,
            product_ids::NASCAR_PRO_FF2,
            product_ids::FGT_RUMBLE_FORCE,
            product_ids::RGT_FF_CLUTCH,
            product_ids::FGT_FORCE_FEEDBACK,
            product_ids::F430_FORCE_FEEDBACK,
            product_ids::TPR_PEDALS,
            product_ids::T_LCM,
        ];

        for &pid in &all_pids {
            assert_ne!(pid, 0, "PID 0x{pid:04X} must be nonzero");
        }

        // Check uniqueness
        for i in 0..all_pids.len() {
            for j in (i + 1)..all_pids.len() {
                assert_ne!(
                    all_pids[i], all_pids[j],
                    "PIDs at index {i} and {j} must be unique: 0x{:04X}",
                    all_pids[i]
                );
            }
        }
    }

    #[test]
    fn max_torque_ranges_are_reasonable() {
        let all_models = [
            Model::T150,
            Model::TMX,
            Model::T300RS,
            Model::T300RSPS4,
            Model::T300RSGT,
            Model::TXRacing,
            Model::T500RS,
            Model::T248,
            Model::T248X,
            Model::TGT,
            Model::TGTII,
            Model::TSPCRacer,
            Model::TSXW,
            Model::T818,
            Model::T80,
            Model::NascarProFF2,
            Model::FGTRumbleForce,
            Model::RGTFF,
            Model::FGTForceFeedback,
            Model::F430ForceFeedback,
            Model::T3PA,
            Model::T3PAPro,
            Model::TLCM,
            Model::TLCMPro,
            Model::Unknown,
        ];
        for model in all_models {
            let torque = model.max_torque_nm();
            assert!(
                torque >= 0.0,
                "{model:?} torque must be >= 0.0, got {torque}"
            );
            assert!(
                torque <= 25.0,
                "{model:?} torque must be <= 25.0 Nm (reasonable bound), got {torque}"
            );
            assert!(torque.is_finite(), "{model:?} torque must be finite");
        }
    }

    #[test]
    fn max_rotation_ranges_are_bounded() {
        let all_models = [
            Model::T150,
            Model::TMX,
            Model::T300RS,
            Model::T500RS,
            Model::T248,
            Model::T818,
            Model::T80,
            Model::Unknown,
        ];
        for model in all_models {
            let rot = model.max_rotation_deg();
            assert!(
                rot > 0 && rot <= 1080,
                "{model:?} rotation must be in (0, 1080], got {rot}"
            );
        }
    }

    #[test]
    fn ffb_support_correctness() {
        // Models that MUST support FFB
        let ffb_models = [
            Model::T150,
            Model::TMX,
            Model::T300RS,
            Model::T300RSPS4,
            Model::T300RSGT,
            Model::TXRacing,
            Model::T500RS,
            Model::T248,
            Model::T248X,
            Model::TGT,
            Model::TGTII,
            Model::TSPCRacer,
            Model::TSXW,
            Model::T818,
            Model::NascarProFF2,
            Model::FGTRumbleForce,
            Model::RGTFF,
            Model::FGTForceFeedback,
            Model::F430ForceFeedback,
        ];
        for model in ffb_models {
            assert!(model.supports_ffb(), "{model:?} must support FFB");
        }

        // Models that MUST NOT support FFB
        let no_ffb_models = [
            Model::T80,
            Model::T3PA,
            Model::T3PAPro,
            Model::TLCM,
            Model::TLCMPro,
            Model::Unknown,
        ];
        for model in no_ffb_models {
            assert!(!model.supports_ffb(), "{model:?} must NOT support FFB");
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional: T150/TMX wire-format edge cases
// ═══════════════════════════════════════════════════════════════════════════════

mod t150_edge_cases {
    use super::*;

    #[test]
    fn t150_effect_type_from_u16_rejects_near_misses() {
        // Values close to valid codes but not exact
        let near_misses = [0x3FFF, 0x4001, 0x4021, 0x4025, 0x403F, 0x4042];
        for val in near_misses {
            assert_eq!(
                tm::T150EffectType::from_u16(val),
                None,
                "0x{val:04X} should not be a valid effect type"
            );
        }
    }

    #[test]
    fn t150_all_effect_types_round_trip() {
        let all_types = [
            tm::T150EffectType::Constant,
            tm::T150EffectType::Sine,
            tm::T150EffectType::SawtoothUp,
            tm::T150EffectType::SawtoothDown,
            tm::T150EffectType::Spring,
            tm::T150EffectType::Damper,
        ];
        for ty in all_types {
            let wire = ty.as_u16();
            let decoded = tm::T150EffectType::from_u16(wire);
            assert_eq!(decoded, Some(ty), "{ty:?} must round-trip through as_u16/from_u16");
        }
    }

    #[test]
    fn t150_stop_is_play_zero() {
        for id in [0u8, 1, 127, 255] {
            let stop = tm::encode_stop_effect_t150(id);
            let play = tm::encode_play_effect_t150(id, 0, 0);
            assert_eq!(stop, play, "stop must equal play(id={id}, 0, 0)");
        }
    }

    #[test]
    fn t150_range_boundary_values() {
        // Min and max u16 values
        let min = tm::encode_range_t150(0);
        assert_eq!(min[0], 0x40);
        assert_eq!(min[1], 0x11);
        assert_eq!(u16::from_le_bytes([min[2], min[3]]), 0);

        let max = tm::encode_range_t150(u16::MAX);
        assert_eq!(u16::from_le_bytes([max[2], max[3]]), u16::MAX);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional: Kernel command edge cases
// ═══════════════════════════════════════════════════════════════════════════════

mod kernel_command_edge_cases {
    use super::*;

    #[test]
    fn kernel_range_clamping_boundary() {
        // 39 → clamps to 40; 1081 → clamps to 1080
        assert_eq!(
            tm::build_kernel_range_command(39),
            tm::build_kernel_range_command(40)
        );
        assert_eq!(
            tm::build_kernel_range_command(1081),
            tm::build_kernel_range_command(1080)
        );
        // 40 and 1080 should not clamp
        assert_ne!(
            tm::build_kernel_range_command(40),
            tm::build_kernel_range_command(41)
        );
    }

    #[test]
    fn kernel_range_scaling_factor_is_0x3c() {
        // 100 * 0x3C = 100 * 60 = 6000 = 0x1770
        let cmd = tm::build_kernel_range_command(100);
        let value = u16::from_le_bytes([cmd[2], cmd[3]]);
        assert_eq!(value, 6000, "100° * 0x3C should be 6000");
    }

    #[test]
    fn kernel_gain_shift_examples() {
        // gain=256 (0x0100) → byte 1 = 1
        assert_eq!(tm::build_kernel_gain_command(0x0100), [0x02, 0x01]);
        // gain=32768 (0x8000) → byte 1 = 0x80
        assert_eq!(tm::build_kernel_gain_command(0x8000), [0x02, 0x80]);
        // gain=255 (0x00FF) → byte 1 = 0 (lower byte is discarded)
        assert_eq!(tm::build_kernel_gain_command(0x00FF), [0x02, 0x00]);
    }
}
