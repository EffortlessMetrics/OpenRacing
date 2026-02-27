//! Cammus FFB output report encoding.
//!
//! # Output report layout (8 bytes)
//! | Offset | Field       | Value                            |
//! |--------|-------------|----------------------------------|
//! | 0      | report ID   | 0x01                             |
//! | 1–2    | torque      | i16 LE, ±0x7FFF = ±full scale   |
//! | 3      | game mode   | 0x01 = game, 0x00 = config       |
//! | 4–7    | reserved    | 0x00                             |

/// FFB output report length in bytes (including the report ID byte).
pub const FFB_REPORT_LEN: usize = 8;

/// Report ID for Cammus FFB output reports.
pub const FFB_REPORT_ID: u8 = 0x01;

/// Game mode byte value – sent during active FFB operation.
pub const MODE_GAME: u8 = 0x01;

/// Config mode byte value – sent during configuration / idle.
pub const MODE_CONFIG: u8 = 0x00;

/// Encode a normalised torque command as a Cammus FFB output report.
///
/// `torque_normalized` is clamped to −1.0 … +1.0 before encoding.
/// Returns an 8-byte array ready to be sent as a USB HID output report.
pub fn encode_torque(torque_normalized: f32) -> [u8; FFB_REPORT_LEN] {
    let clamped = torque_normalized.clamp(-1.0, 1.0);
    let raw = (clamped * i16::MAX as f32) as i16;
    let bytes = raw.to_le_bytes();
    [
        FFB_REPORT_ID,
        bytes[0],
        bytes[1],
        MODE_GAME,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

/// Encode a safe-state (zero torque) command.
pub fn encode_stop() -> [u8; FFB_REPORT_LEN] {
    encode_torque(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_zero() {
        let report = encode_torque(0.0);
        assert_eq!(report[0], FFB_REPORT_ID);
        assert_eq!(report[1], 0x00);
        assert_eq!(report[2], 0x00);
        assert_eq!(report[3], MODE_GAME);
        assert_eq!(&report[4..], &[0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn encode_full_positive() {
        let report = encode_torque(1.0);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        assert_eq!(raw, i16::MAX);
    }

    #[test]
    fn encode_full_negative() {
        let report = encode_torque(-1.0);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        assert_eq!(raw, -i16::MAX);
    }

    #[test]
    fn encode_clamps_over() {
        assert_eq!(encode_torque(2.0), encode_torque(1.0));
    }

    #[test]
    fn encode_clamps_under() {
        assert_eq!(encode_torque(-2.0), encode_torque(-1.0));
    }

    #[test]
    fn sign_preservation() {
        let pos = encode_torque(0.5);
        assert!(i16::from_le_bytes([pos[1], pos[2]]) > 0);

        let neg = encode_torque(-0.5);
        assert!(i16::from_le_bytes([neg[1], neg[2]]) < 0);
    }

    #[test]
    fn monotonic() {
        let v = |t: f32| {
            let r = encode_torque(t);
            i16::from_le_bytes([r[1], r[2]])
        };
        assert!(v(0.25) < v(0.5));
        assert!(v(0.5) < v(0.75));
    }

    #[test]
    fn stop_is_zero_torque() {
        assert_eq!(encode_stop(), encode_torque(0.0));
    }

    #[test]
    fn report_length() {
        assert_eq!(encode_torque(0.5).len(), FFB_REPORT_LEN);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Sign is preserved: positive input gives positive raw, negative gives negative.
        #[test]
        fn prop_sign_preserved(torque in -1.0f32..=1.0f32) {
            let report = encode_torque(torque);
            let raw = i16::from_le_bytes([report[1], report[2]]);
            if torque > 0.001 {
                prop_assert!(raw > 0, "positive torque {torque} must yield positive raw {raw}");
            } else if torque < -0.001 {
                prop_assert!(raw < 0, "negative torque {torque} must yield negative raw {raw}");
            }
        }

        /// Inputs >= 1.0 saturate to i16::MAX; inputs <= -1.0 saturate to -i16::MAX.
        #[test]
        fn prop_saturates_at_boundary(torque in -100.0f32..=100.0f32) {
            let report = encode_torque(torque);
            let raw = i16::from_le_bytes([report[1], report[2]]);
            if torque >= 1.0 {
                prop_assert_eq!(raw, i16::MAX, "torque >= 1.0 must saturate to i16::MAX");
            } else if torque <= -1.0 {
                prop_assert_eq!(raw, -i16::MAX, "torque <= -1.0 must saturate to -i16::MAX");
            }
        }

        /// Report ID byte is always FFB_REPORT_ID for any input.
        #[test]
        fn prop_report_id_always_correct(torque in -100.0f32..=100.0f32) {
            let report = encode_torque(torque);
            prop_assert_eq!(report[0], FFB_REPORT_ID);
        }

        /// Game mode byte (offset 3) is always MODE_GAME for any input.
        #[test]
        fn prop_mode_byte_always_game(torque in -100.0f32..=100.0f32) {
            let report = encode_torque(torque);
            prop_assert_eq!(report[3], MODE_GAME);
        }

        /// Reserved bytes [4..8] are always zero.
        #[test]
        fn prop_reserved_bytes_zero(torque in -100.0f32..=100.0f32) {
            let report = encode_torque(torque);
            prop_assert_eq!(&report[4..], &[0x00u8, 0x00, 0x00, 0x00]);
        }

        /// encode_stop is idempotent: repeated calls return the same bytes.
        #[test]
        fn prop_encode_stop_idempotent(_x in 0u8..=1u8) {
            prop_assert_eq!(encode_stop(), encode_stop());
        }

        /// encode_stop always equals encode_torque(0.0).
        #[test]
        fn prop_encode_stop_equals_zero_torque(_x in 0u8..=1u8) {
            prop_assert_eq!(encode_stop(), encode_torque(0.0));
        }

        /// Encoding is monotone: higher torque yields higher (or equal) raw value.
        #[test]
        fn prop_monotone(a in -1.0f32..=1.0f32, b in -1.0f32..=1.0f32) {
            let r_a = encode_torque(a);
            let r_b = encode_torque(b);
            let raw_a = i16::from_le_bytes([r_a[1], r_a[2]]);
            let raw_b = i16::from_le_bytes([r_b[1], r_b[2]]);
            if a > b {
                prop_assert!(raw_a >= raw_b, "monotone: torque {a} > {b} but raw {raw_a} < {raw_b}");
            } else if a < b {
                prop_assert!(raw_a <= raw_b, "monotone: torque {a} < {b} but raw {raw_a} > {raw_b}");
            }
        }

        /// Report length is always FFB_REPORT_LEN regardless of input.
        #[test]
        fn prop_report_length(torque in -100.0f32..=100.0f32) {
            prop_assert_eq!(encode_torque(torque).len(), FFB_REPORT_LEN);
        }
    }
}
