//! Roundtrip property-based tests for the Simucube HID protocol.
//!
//! Verifies encode→decode and decode→encode consistency for:
//! - SimucubeHidReport: joystick axes + buttons roundtrip
//! - SimucubeInputReport: diagnostics fields roundtrip
//! - SimucubeOutputReport: build→parse byte roundtrip
//! - Model identification roundtrip
#![allow(clippy::redundant_closure)]

use hid_simucube_protocol::{
    EffectType, SimucubeHidReport, SimucubeInputReport, SimucubeModel, SimucubeOutputReport,
};
use proptest::prelude::*;

// ── HID joystick report roundtrip ───────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Steering u16 LE written at bytes [0..2] must parse back exactly.
    #[test]
    fn prop_hid_report_steering_roundtrip(steering: u16) {
        let mut data = [0u8; 32];
        data[0] = (steering & 0xFF) as u8;
        data[1] = (steering >> 8) as u8;
        let report = SimucubeHidReport::parse(&data);
        prop_assert!(report.is_ok());
        if let Ok(r) = report {
            prop_assert_eq!(r.steering, steering, "steering must round-trip");
        }
    }

    /// Y axis u16 LE written at bytes [2..4] must parse back exactly.
    #[test]
    fn prop_hid_report_y_axis_roundtrip(y_axis: u16) {
        let mut data = [0u8; 32];
        data[2] = (y_axis & 0xFF) as u8;
        data[3] = (y_axis >> 8) as u8;
        let report = SimucubeHidReport::parse(&data);
        prop_assert!(report.is_ok());
        if let Ok(r) = report {
            prop_assert_eq!(r.y_axis, y_axis, "y_axis must round-trip");
        }
    }

    /// Button bytes [16..32] must round-trip through the buttons field.
    #[test]
    fn prop_hid_report_buttons_roundtrip(
        b0: u8, b1: u8, b2: u8, b3: u8,
        b4: u8, b5: u8, b6: u8, b7: u8,
    ) {
        let mut data = [0u8; 32];
        data[16] = b0; data[17] = b1; data[18] = b2; data[19] = b3;
        data[20] = b4; data[21] = b5; data[22] = b6; data[23] = b7;
        let report = SimucubeHidReport::parse(&data);
        prop_assert!(report.is_ok());
        if let Ok(r) = report {
            prop_assert_eq!(r.buttons[0], b0);
            prop_assert_eq!(r.buttons[1], b1);
            prop_assert_eq!(r.buttons[2], b2);
            prop_assert_eq!(r.buttons[3], b3);
        }
    }

    /// Reports shorter than 32 bytes must return Err, not panic.
    #[test]
    fn prop_hid_report_short_error(len in 0usize..=31usize) {
        let data = vec![0u8; len];
        let result = SimucubeHidReport::parse(&data);
        prop_assert!(result.is_err(), "short report of len {len} must return Err");
    }
}

// ── Input diagnostic report roundtrip ───────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Sequence u16 LE at bytes [0..2] must round-trip.
    #[test]
    fn prop_input_report_sequence_roundtrip(seq: u16) {
        let mut data = [0u8; 16];
        data[0] = (seq & 0xFF) as u8;
        data[1] = (seq >> 8) as u8;
        let report = SimucubeInputReport::parse(&data);
        prop_assert!(report.is_ok());
        if let Ok(r) = report {
            prop_assert_eq!(r.sequence, seq, "sequence must round-trip");
        }
    }

    /// Wheel angle u32 LE at bytes [2..6] must round-trip.
    #[test]
    fn prop_input_report_angle_roundtrip(angle: u32) {
        let mut data = [0u8; 16];
        let bytes = angle.to_le_bytes();
        data[2] = bytes[0]; data[3] = bytes[1];
        data[4] = bytes[2]; data[5] = bytes[3];
        let report = SimucubeInputReport::parse(&data);
        prop_assert!(report.is_ok());
        if let Ok(r) = report {
            prop_assert_eq!(r.wheel_angle_raw, angle, "angle must round-trip");
        }
    }

    /// Torque i16 LE at bytes [8..10] must round-trip.
    #[test]
    fn prop_input_report_torque_roundtrip(torque: i16) {
        let mut data = [0u8; 16];
        let bytes = torque.to_le_bytes();
        data[8] = bytes[0]; data[9] = bytes[1];
        let report = SimucubeInputReport::parse(&data);
        prop_assert!(report.is_ok());
        if let Ok(r) = report {
            prop_assert_eq!(r.torque_nm, torque, "torque must round-trip");
        }
    }

    /// Temperature at byte [10] must round-trip.
    #[test]
    fn prop_input_report_temp_roundtrip(temp: u8) {
        let mut data = [0u8; 16];
        data[10] = temp;
        let report = SimucubeInputReport::parse(&data);
        prop_assert!(report.is_ok());
        if let Ok(r) = report {
            prop_assert_eq!(r.temperature_c, temp, "temperature must round-trip");
        }
    }

    /// Reports shorter than 16 bytes must return Err, not panic.
    #[test]
    fn prop_input_report_short_error(len in 0usize..=15usize) {
        let data = vec![0u8; len];
        let result = SimucubeInputReport::parse(&data);
        prop_assert!(result.is_err(), "short report of len {len} must return Err");
    }
}

// ── Output report build→inspect roundtrip ───────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Output report build: sequence, torque, and RGB must appear at the correct
    /// byte offsets in the built report.
    #[test]
    fn prop_output_report_roundtrip(
        seq: u16,
        torque in -3200i16..=3200i16,
        r: u8,
        g: u8,
        b: u8,
    ) {
        let report = SimucubeOutputReport::new(seq)
            .with_torque(torque as f32)
            .with_rgb(r, g, b);
        let built = report.build();
        prop_assert!(built.is_ok(), "build must not fail");
        if let Ok(data) = built {
            prop_assert!(data.len() >= 11, "report must be at least 11 bytes");
            // Byte 0 = report ID 0x01
            prop_assert_eq!(data[0], 0x01, "report ID must be 0x01");
            // Bytes 1-2 = sequence LE
            let decoded_seq = u16::from_le_bytes([data[1], data[2]]);
            prop_assert_eq!(decoded_seq, seq, "sequence must round-trip");
            // Bytes 5-7 = RGB
            prop_assert_eq!(data[5], r, "red must round-trip");
            prop_assert_eq!(data[6], g, "green must round-trip");
            prop_assert_eq!(data[7], b, "blue must round-trip");
        }
    }

    /// Output report with effect: effect type byte must match.
    #[test]
    fn prop_output_report_effect_roundtrip(
        seq: u16,
        effect_param: u16,
    ) {
        let effects = [
            EffectType::None,
            EffectType::Constant,
            EffectType::Spring,
            EffectType::Damper,
            EffectType::Sine,
        ];
        for effect in &effects {
            let report = SimucubeOutputReport::new(seq)
                .with_effect(*effect, effect_param);
            let built = report.build();
            prop_assert!(built.is_ok());
            if let Ok(data) = built {
                prop_assert_eq!(data[8], *effect as u8, "effect type must match");
                let decoded_param = u16::from_le_bytes([data[9], data[10]]);
                prop_assert_eq!(decoded_param, effect_param, "effect param must round-trip");
            }
        }
    }

    /// Model identification is deterministic.
    #[test]
    fn prop_model_deterministic(pid: u16) {
        let m1 = SimucubeModel::from_product_id(pid);
        let m2 = SimucubeModel::from_product_id(pid);
        prop_assert_eq!(m1, m2);
    }
}
