//! Telemetry frame ABI for real-time plugin communication.
//!
//! This module defines the telemetry frame structure used for
//! high-frequency data exchange between host and plugins.

use crate::constants::PLUG_ABI_VERSION;

/// Telemetry frame for real-time plugin communication.
///
/// All integers are stored in little-endian format.
/// Field names updated to match new schema conventions.
///
/// # ABI Stability
///
/// This structure has a fixed size of 32 bytes and 8-byte alignment.
/// The layout is guaranteed to be stable across versions.
///
/// # Memory Layout
///
/// | Offset | Size | Field           |
/// |--------|------|-----------------|
/// | 0      | 8    | timestamp_us    |
/// | 8      | 4    | wheel_angle_deg |
/// | 12     | 4    | wheel_speed_rad |
/// | 16     | 4    | temperature_c   |
/// | 20     | 4    | fault_flags     |
/// | 24     | 4    | _pad            |
/// | 28     | 4    | (reserved)      |
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TelemetryFrame {
    /// Timestamp in microseconds (little-endian)
    pub timestamp_us: u64,

    /// Wheel angle in degrees (not millidegrees)
    /// Range: -1800.0 to +1800.0 degrees for 5-turn wheels
    pub wheel_angle_deg: f32,

    /// Wheel speed in radians per second (not mrad/s)
    /// Positive values indicate clockwise rotation
    pub wheel_speed_rad_s: f32,

    /// Temperature in degrees Celsius (not temp_c)
    /// Typical range: 20-80°C for normal operation
    pub temperature_c: f32,

    /// Fault flags bitfield (not faults)
    /// Each bit represents a specific fault condition
    pub fault_flags: u32,

    /// Padding to ensure 8-byte alignment
    pub _pad: u32,
}

impl Default for TelemetryFrame {
    fn default() -> Self {
        Self {
            timestamp_us: 0,
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 20.0,
            fault_flags: 0,
            _pad: 0,
        }
    }
}

impl TelemetryFrame {
    /// Create a new telemetry frame with timestamp.
    #[must_use]
    pub fn new(timestamp_us: u64) -> Self {
        Self {
            timestamp_us,
            ..Default::default()
        }
    }

    /// Create a telemetry frame with all fields.
    #[must_use]
    pub fn with_values(
        timestamp_us: u64,
        wheel_angle_deg: f32,
        wheel_speed_rad_s: f32,
        temperature_c: f32,
        fault_flags: u32,
    ) -> Self {
        Self {
            timestamp_us,
            wheel_angle_deg,
            wheel_speed_rad_s,
            temperature_c,
            fault_flags,
            _pad: 0,
        }
    }

    /// Convert frame to byte array for IPC.
    ///
    /// The output is always 32 bytes in little-endian format.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[0..8].copy_from_slice(&self.timestamp_us.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.wheel_angle_deg.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.wheel_speed_rad_s.to_le_bytes());
        bytes[16..20].copy_from_slice(&self.temperature_c.to_le_bytes());
        bytes[20..24].copy_from_slice(&self.fault_flags.to_le_bytes());
        bytes[24..28].copy_from_slice(&self._pad.to_le_bytes());
        bytes
    }

    /// Create frame from byte array.
    ///
    /// # Panics
    ///
    /// Does not panic; all byte patterns produce valid frames.
    #[must_use]
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            timestamp_us: u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            wheel_angle_deg: f32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            wheel_speed_rad_s: f32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
            temperature_c: f32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
            fault_flags: u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
            _pad: u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
        }
    }

    /// Get the ABI version this frame format is compatible with.
    #[must_use]
    pub const fn abi_version() -> u32 {
        PLUG_ABI_VERSION
    }

    /// Check if temperature is in normal operating range.
    #[must_use]
    pub fn is_temperature_normal(&self) -> bool {
        (20.0..=80.0).contains(&self.temperature_c)
    }

    /// Check if wheel angle is within typical range.
    #[must_use]
    pub fn is_angle_valid(&self) -> bool {
        (-1800.0..=1800.0).contains(&self.wheel_angle_deg)
    }

    /// Check if there are any fault flags set.
    #[must_use]
    pub const fn has_faults(&self) -> bool {
        self.fault_flags != 0
    }
}

static_assertions::const_assert_eq!(std::mem::size_of::<TelemetryFrame>(), 32);
static_assertions::const_assert_eq!(std::mem::align_of::<TelemetryFrame>(), 8);

#[cfg(feature = "serde")]
impl serde::Serialize for TelemetryFrame {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("TelemetryFrame", 6)?;
        s.serialize_field("timestamp_us", &self.timestamp_us)?;
        s.serialize_field("wheel_angle_deg", &self.wheel_angle_deg)?;
        s.serialize_field("wheel_speed_rad_s", &self.wheel_speed_rad_s)?;
        s.serialize_field("temperature_c", &self.temperature_c)?;
        s.serialize_field("fault_flags", &self.fault_flags)?;
        s.serialize_field("_pad", &self._pad)?;
        s.end()
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for TelemetryFrame {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct TelemetryFrameHelper {
            timestamp_us: u64,
            wheel_angle_deg: f32,
            wheel_speed_rad_s: f32,
            temperature_c: f32,
            fault_flags: u32,
            #[serde(default)]
            _pad: u32,
        }

        let helper = TelemetryFrameHelper::deserialize(deserializer)?;
        Ok(Self {
            timestamp_us: helper.timestamp_us,
            wheel_angle_deg: helper.wheel_angle_deg,
            wheel_speed_rad_s: helper.wheel_speed_rad_s,
            temperature_c: helper.temperature_c,
            fault_flags: helper.fault_flags,
            _pad: helper._pad,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_frame_size_and_alignment() {
        assert_eq!(std::mem::size_of::<TelemetryFrame>(), 32);
        assert_eq!(std::mem::align_of::<TelemetryFrame>(), 8);
    }

    #[test]
    fn test_telemetry_frame_default() {
        let frame = TelemetryFrame::default();
        assert_eq!(frame.timestamp_us, 0);
        assert_eq!(frame.wheel_angle_deg, 0.0);
        assert_eq!(frame.wheel_speed_rad_s, 0.0);
        assert_eq!(frame.temperature_c, 20.0);
        assert_eq!(frame.fault_flags, 0);
    }

    #[test]
    fn test_telemetry_frame_new() {
        let frame = TelemetryFrame::new(12345);
        assert_eq!(frame.timestamp_us, 12345);
        assert_eq!(frame.wheel_angle_deg, 0.0);
    }

    #[test]
    fn test_telemetry_frame_byte_serialization() {
        let frame =
            TelemetryFrame::with_values(1234567890, 45.5, std::f32::consts::PI, 65.0, 0x12345678);

        let bytes = frame.to_bytes();
        let restored = TelemetryFrame::from_bytes(&bytes);

        assert_eq!(frame.timestamp_us, restored.timestamp_us);
        assert_eq!(frame.wheel_angle_deg, restored.wheel_angle_deg);
        assert_eq!(frame.wheel_speed_rad_s, restored.wheel_speed_rad_s);
        assert_eq!(frame.temperature_c, restored.temperature_c);
        assert_eq!(frame.fault_flags, restored.fault_flags);
    }

    #[test]
    fn test_telemetry_frame_validation() {
        let valid_frame = TelemetryFrame::with_values(0, 90.0, 1.0, 45.0, 0);
        assert!(valid_frame.is_temperature_normal());
        assert!(valid_frame.is_angle_valid());
        assert!(!valid_frame.has_faults());

        let hot_frame = TelemetryFrame::with_values(0, 0.0, 0.0, 85.0, 0);
        assert!(!hot_frame.is_temperature_normal());

        let fault_frame = TelemetryFrame::with_values(0, 0.0, 0.0, 20.0, 0x01);
        assert!(fault_frame.has_faults());
    }
}
