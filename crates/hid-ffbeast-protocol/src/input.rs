//! FFBeast vendor-specific input report parser.
//!
//! Parses the vendor-specific state report sent by FFBeast firmware on
//! HID report ID `0xA3` (GenericInputOutput). This report carries real-time
//! wheel position and torque feedback, plus firmware version metadata.
//!
//! # Source
//! Layout derived from the community-maintained WebHID API:
//! <https://github.com/shubham0x13/ffbeast-wheel-webhid-api>
//! (`src/wheel-api.ts` `handleInputReport`, `src/constants.ts`, `src/enums.ts`)
//!
//! # Report layout (Report ID 0xA3)
//!
//! ```text
//! Offset  Size  Description
//! ------  ----  -------------------------------------------
//!  0       1    Firmware release type (u8)
//!  1       1    Firmware major version (u8)
//!  2       1    Firmware minor version (u8)
//!  3       1    Firmware patch version (u8)
//!  4       1    Registration status (u8: 0=unregistered, 1=registered)
//!  5       2    Position (i16 LE, -10000..+10000)
//!  7       2    Torque feedback (i16 LE, -10000..+10000)
//! ```
//!
//! Minimum 9 bytes of payload (after report ID stripping by the HID layer).
//! The WebHID API strips the report ID before calling the handler, so offsets
//! here are relative to the data payload.

/// HID report ID for the vendor-specific state report.
///
/// Matches `ReportType.GenericInputOutput = 0xA3` from the WebHID API.
pub const STATE_REPORT_ID: u8 = 0xA3;

/// Minimum payload length in bytes (excluding report ID).
pub const STATE_REPORT_MIN_LEN: usize = 9;

/// Maximum raw position/torque value (±10000).
pub const RAW_MAX: i16 = 10_000;

/// Parsed firmware version from the state report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareVersion {
    /// Release type (0 = release, other = pre-release variant).
    pub release_type: u8,
    /// Major version (year-based).
    pub major: u8,
    /// Minor version (incremented when companion app update is needed).
    pub minor: u8,
    /// Patch version.
    pub patch: u8,
}

/// Parsed FFBeast vendor-specific state report.
///
/// Contains real-time wheel position and torque feedback from the controller,
/// along with firmware version metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FFBeastStateReport {
    /// Firmware version embedded in every state report.
    pub firmware_version: FirmwareVersion,
    /// Registration/license status (0 = unregistered, 1 = registered).
    pub is_registered: u8,
    /// Raw wheel position in [-10000, 10000].
    pub position: i16,
    /// Raw torque feedback in [-10000, 10000].
    pub torque: i16,
}

impl FFBeastStateReport {
    /// Parse a state report from raw HID bytes.
    ///
    /// `data` is the report payload **after** the report ID has been stripped
    /// (as done by WebHID and most HID APIs). Must be at least 9 bytes.
    ///
    /// If your HID layer includes the report ID as byte 0, pass `&data[1..]`.
    ///
    /// Returns `None` if the data is too short.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < STATE_REPORT_MIN_LEN {
            return None;
        }

        Some(Self {
            firmware_version: FirmwareVersion {
                release_type: data[0],
                major: data[1],
                minor: data[2],
                patch: data[3],
            },
            is_registered: data[4],
            position: i16::from_le_bytes([data[5], data[6]]),
            torque: i16::from_le_bytes([data[7], data[8]]),
        })
    }

    /// Parse from a buffer that includes the report ID as byte 0.
    ///
    /// Returns `None` if the report ID does not match `STATE_REPORT_ID`
    /// or the buffer is too short.
    pub fn parse_with_id(data: &[u8]) -> Option<Self> {
        if data.is_empty() || data[0] != STATE_REPORT_ID {
            return None;
        }
        Self::parse(&data[1..])
    }

    /// Wheel position normalized to [-1.0, 1.0].
    pub fn position_normalized(&self) -> f32 {
        self.position as f32 / RAW_MAX as f32
    }

    /// Convert position to degrees given the configured motion range.
    ///
    /// `motion_range_deg` is the total motion range in degrees (e.g., 900).
    pub fn position_degrees(&self, motion_range_deg: f32) -> f32 {
        self.position_normalized() * (motion_range_deg / 2.0)
    }

    /// Torque feedback normalized to [-1.0, 1.0].
    pub fn torque_normalized(&self) -> f32 {
        self.torque as f32 / RAW_MAX as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_report(position: i16, torque: i16) -> Vec<u8> {
        let mut data = vec![0u8; STATE_REPORT_MIN_LEN];
        // firmware version: 0.1.2.3
        data[0] = 0; // release_type
        data[1] = 1; // major
        data[2] = 2; // minor
        data[3] = 3; // patch
        data[4] = 1; // registered
        let pos = position.to_le_bytes();
        data[5] = pos[0];
        data[6] = pos[1];
        let trq = torque.to_le_bytes();
        data[7] = trq[0];
        data[8] = trq[1];
        data
    }

    #[test]
    fn parse_valid_report() {
        let data = make_report(5000, -3000);
        let report = FFBeastStateReport::parse(&data);
        assert!(report.is_some());
        let r = report.expect("should parse");
        assert_eq!(r.firmware_version.major, 1);
        assert_eq!(r.firmware_version.minor, 2);
        assert_eq!(r.firmware_version.patch, 3);
        assert_eq!(r.is_registered, 1);
        assert_eq!(r.position, 5000);
        assert_eq!(r.torque, -3000);
    }

    #[test]
    fn parse_rejects_short_data() {
        assert!(FFBeastStateReport::parse(&[0u8; 8]).is_none());
        assert!(FFBeastStateReport::parse(&[]).is_none());
    }

    #[test]
    fn parse_with_id_valid() {
        let mut data = vec![STATE_REPORT_ID];
        data.extend_from_slice(&make_report(1000, 2000));
        let report = FFBeastStateReport::parse_with_id(&data);
        assert!(report.is_some());
        let r = report.expect("should parse");
        assert_eq!(r.position, 1000);
        assert_eq!(r.torque, 2000);
    }

    #[test]
    fn parse_with_id_wrong_id() {
        let mut data = vec![0x01]; // wrong report ID
        data.extend_from_slice(&make_report(0, 0));
        assert!(FFBeastStateReport::parse_with_id(&data).is_none());
    }

    #[test]
    fn parse_with_id_empty() {
        assert!(FFBeastStateReport::parse_with_id(&[]).is_none());
    }

    #[test]
    fn position_normalized_full_positive() {
        let data = make_report(RAW_MAX, 0);
        let r = FFBeastStateReport::parse(&data).expect("should parse");
        assert!((r.position_normalized() - 1.0).abs() < 0.001);
    }

    #[test]
    fn position_normalized_full_negative() {
        let data = make_report(-RAW_MAX, 0);
        let r = FFBeastStateReport::parse(&data).expect("should parse");
        assert!((r.position_normalized() + 1.0).abs() < 0.001);
    }

    #[test]
    fn position_degrees_900_range() {
        let data = make_report(RAW_MAX, 0);
        let r = FFBeastStateReport::parse(&data).expect("should parse");
        let degrees = r.position_degrees(900.0);
        assert!((degrees - 450.0).abs() < 0.1);
    }

    #[test]
    fn torque_normalized_bounds() {
        let data = make_report(0, RAW_MAX);
        let r = FFBeastStateReport::parse(&data).expect("should parse");
        assert!((r.torque_normalized() - 1.0).abs() < 0.001);
    }

    #[test]
    fn zero_values() {
        let data = make_report(0, 0);
        let r = FFBeastStateReport::parse(&data).expect("should parse");
        assert_eq!(r.position, 0);
        assert_eq!(r.torque, 0);
        assert!((r.position_normalized()).abs() < 0.001);
        assert!((r.torque_normalized()).abs() < 0.001);
    }

    #[test]
    fn parse_accepts_longer_data() {
        let mut data = make_report(100, 200);
        data.extend_from_slice(&[0xAB; 55]); // extra bytes
        let r = FFBeastStateReport::parse(&data);
        assert!(r.is_some());
        let r = r.expect("should parse");
        assert_eq!(r.position, 100);
        assert_eq!(r.torque, 200);
    }

    #[test]
    fn firmware_version_fields() {
        let mut data = vec![0u8; STATE_REPORT_MIN_LEN];
        data[0] = 2; // release_type
        data[1] = 24; // major (2024)
        data[2] = 5; // minor
        data[3] = 12; // patch
        data[4] = 0; // unregistered
        let r = FFBeastStateReport::parse(&data).expect("should parse");
        assert_eq!(
            r.firmware_version,
            FirmwareVersion {
                release_type: 2,
                major: 24,
                minor: 5,
                patch: 12,
            }
        );
        assert_eq!(r.is_registered, 0);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Arbitrary bytes never panic FFBeastStateReport::parse.
        #[test]
        fn prop_parse_never_panics(data in proptest::collection::vec(any::<u8>(), 0..=128)) {
            let _ = FFBeastStateReport::parse(&data);
        }

        /// Arbitrary bytes never panic FFBeastStateReport::parse_with_id.
        #[test]
        fn prop_parse_with_id_never_panics(data in proptest::collection::vec(any::<u8>(), 0..=128)) {
            let _ = FFBeastStateReport::parse_with_id(&data);
        }

        /// Valid reports produce finite normalized values.
        #[test]
        fn prop_normalized_values_finite(data in proptest::collection::vec(any::<u8>(), 9..=32)) {
            if let Some(r) = FFBeastStateReport::parse(&data) {
                prop_assert!(r.position_normalized().is_finite());
                prop_assert!(r.torque_normalized().is_finite());
                prop_assert!(r.position_degrees(900.0).is_finite());
            }
        }

        /// Truncated buffers are safely rejected.
        #[test]
        fn prop_truncated_rejected(len in 0usize..9) {
            let data = vec![0u8; len];
            prop_assert!(FFBeastStateReport::parse(&data).is_none());
        }

        /// parse_with_id rejects wrong report IDs.
        #[test]
        fn prop_wrong_id_rejected(id in any::<u8>()) {
            if id != STATE_REPORT_ID {
                let mut data = vec![id];
                data.extend_from_slice(&[0u8; STATE_REPORT_MIN_LEN]);
                prop_assert!(FFBeastStateReport::parse_with_id(&data).is_none());
            }
        }
    }
}
