//! Property-based tests for FFBeast state report parsing.

use proptest::prelude::*;
use racing_wheel_hid_ffbeast_protocol::input::{
    FFBeastStateReport, STATE_REPORT_ID, STATE_REPORT_MIN_LEN,
};

proptest! {
    #![proptest_config(ProptestConfig { cases: 1000, timeout: 60_000, ..ProptestConfig::default() })]

    #[test]
    fn parse_never_panics(data in proptest::collection::vec(any::<u8>(), 0..100)) {
        let _ = FFBeastStateReport::parse(&data);
    }

    #[test]
    fn parse_with_id_never_panics(data in proptest::collection::vec(any::<u8>(), 0..100)) {
        let _ = FFBeastStateReport::parse_with_id(&data);
    }

    #[test]
    fn valid_reports_always_parse(data in proptest::collection::vec(any::<u8>(), STATE_REPORT_MIN_LEN..=64)) {
        let result = FFBeastStateReport::parse(&data);
        prop_assert!(result.is_some(), "report of len {} should parse", data.len());
    }

    #[test]
    fn position_torque_roundtrip(pos in -10000i16..=10000i16, trq in -10000i16..=10000i16) {
        let mut data = vec![0u8; STATE_REPORT_MIN_LEN];
        let pos_bytes = pos.to_le_bytes();
        let trq_bytes = trq.to_le_bytes();
        data[5] = pos_bytes[0];
        data[6] = pos_bytes[1];
        data[7] = trq_bytes[0];
        data[8] = trq_bytes[1];
        let r = FFBeastStateReport::parse(&data);
        prop_assert!(r.is_some());
        if let Some(r) = r {
            prop_assert_eq!(r.position, pos);
            prop_assert_eq!(r.torque, trq);
        }
    }

    #[test]
    fn position_normalized_bounded(pos in -10000i16..=10000i16) {
        let mut data = vec![0u8; STATE_REPORT_MIN_LEN];
        let bytes = pos.to_le_bytes();
        data[5] = bytes[0];
        data[6] = bytes[1];
        let r = FFBeastStateReport::parse(&data);
        prop_assert!(r.is_some());
        if let Some(r) = r {
            let normalized = r.position_normalized();
            prop_assert!((-1.0..=1.0).contains(&normalized),
                "position_normalized() = {} out of range", normalized);
        }
    }

    #[test]
    fn torque_normalized_bounded(trq in -10000i16..=10000i16) {
        let mut data = vec![0u8; STATE_REPORT_MIN_LEN];
        let bytes = trq.to_le_bytes();
        data[7] = bytes[0];
        data[8] = bytes[1];
        let r = FFBeastStateReport::parse(&data);
        prop_assert!(r.is_some());
        if let Some(r) = r {
            let normalized = r.torque_normalized();
            prop_assert!((-1.0..=1.0).contains(&normalized),
                "torque_normalized() = {} out of range", normalized);
        }
    }

    #[test]
    fn firmware_version_roundtrip(
        release_type in any::<u8>(),
        major in any::<u8>(),
        minor in any::<u8>(),
        patch in any::<u8>(),
    ) {
        let mut data = vec![0u8; STATE_REPORT_MIN_LEN];
        data[0] = release_type;
        data[1] = major;
        data[2] = minor;
        data[3] = patch;
        let r = FFBeastStateReport::parse(&data);
        prop_assert!(r.is_some());
        if let Some(r) = r {
            prop_assert_eq!(r.firmware_version.release_type, release_type);
            prop_assert_eq!(r.firmware_version.major, major);
            prop_assert_eq!(r.firmware_version.minor, minor);
            prop_assert_eq!(r.firmware_version.patch, patch);
        }
    }

    #[test]
    fn short_reports_fail(len in 0usize..STATE_REPORT_MIN_LEN) {
        let data = vec![0u8; len];
        prop_assert!(FFBeastStateReport::parse(&data).is_none());
    }

    #[test]
    fn parse_with_id_correct_id_succeeds(data in proptest::collection::vec(any::<u8>(), STATE_REPORT_MIN_LEN..=64)) {
        let mut full = vec![STATE_REPORT_ID];
        full.extend_from_slice(&data);
        let result = FFBeastStateReport::parse_with_id(&full);
        prop_assert!(result.is_some());
    }

    #[test]
    fn parse_with_id_wrong_id_fails(
        id in 0u8..=0xFF,
        data in proptest::collection::vec(any::<u8>(), STATE_REPORT_MIN_LEN..=64),
    ) {
        prop_assume!(id != STATE_REPORT_ID);
        let mut full = vec![id];
        full.extend_from_slice(&data);
        prop_assert!(FFBeastStateReport::parse_with_id(&full).is_none());
    }
}
