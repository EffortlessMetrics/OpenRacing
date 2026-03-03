//! Calibration type definitions

use serde::{Deserialize, Serialize};

/// Calibration point for axis
///
/// Maps a single raw sensor reading to a normalized `[0.0, 1.0]` value.
///
/// # Examples
///
/// ```
/// use openracing_calibration::CalibrationPoint;
///
/// let point = CalibrationPoint::new(32768, 0.5);
/// assert_eq!(point.raw, 32768);
/// assert!((point.normalized - 0.5).abs() < f32::EPSILON);
/// ```
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CalibrationPoint {
    /// Raw sensor value (device units, typically 0–65535).
    pub raw: u16,
    /// Expected normalized output in `[0.0, 1.0]`.
    pub normalized: f32,
}

impl CalibrationPoint {
    /// Creates a calibration point from a raw sensor value and its expected normalized output.
    pub fn new(raw: u16, normalized: f32) -> Self {
        Self { raw, normalized }
    }
}

/// Axis calibration data
///
/// Stores the min/max range, optional center point, and dead-zone boundaries
/// for a single input axis. Use [`apply`](AxisCalibration::apply) to convert
/// raw sensor values to normalized `[0.0, 1.0]` output.
///
/// # Examples
///
/// ```
/// use openracing_calibration::AxisCalibration;
///
/// let calib = AxisCalibration::new(0, 65535)
///     .with_center(32768)
///     .with_deadzone(1000, 64535);
///
/// // Mid-range input produces roughly 0.5
/// let value = calib.apply(32768);
/// assert!((value - 0.5).abs() < 0.02);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisCalibration {
    /// Minimum raw value observed (full-left / pedal released).
    pub min: u16,
    /// Optional center position. Used for centering-type axes (e.g., steering).
    pub center: Option<u16>,
    /// Maximum raw value observed (full-right / pedal fully pressed).
    pub max: u16,
    /// Lower dead-zone boundary (raw values below this map to 0.0).
    pub deadzone_min: u16,
    /// Upper dead-zone boundary (raw values above this map to 1.0).
    pub deadzone_max: u16,
}

impl Default for AxisCalibration {
    fn default() -> Self {
        Self {
            min: 0,
            center: None,
            max: 0xFFFF,
            deadzone_min: 0,
            deadzone_max: 0xFFFF,
        }
    }
}

impl AxisCalibration {
    /// Creates an axis calibration with the given raw min/max range.
    ///
    /// Center and dead-zone are unset; use [`with_center`](Self::with_center)
    /// and [`with_deadzone`](Self::with_deadzone) to configure them.
    pub fn new(min: u16, max: u16) -> Self {
        Self {
            min,
            center: None,
            max,
            deadzone_min: 0,
            deadzone_max: 0xFFFF,
        }
    }

    /// Sets the center-point for this axis (e.g., steering wheel straight-ahead).
    pub fn with_center(mut self, center: u16) -> Self {
        self.center = Some(center);
        self
    }

    /// Sets the dead-zone boundaries in raw units.
    ///
    /// Values below `min` map to `0.0`; values above `max` map to `1.0`.
    pub fn with_deadzone(mut self, min: u16, max: u16) -> Self {
        self.deadzone_min = min;
        self.deadzone_max = max;
        self
    }

    /// Converts a raw sensor value to a normalized `[0.0, 1.0]` output,
    /// applying the configured range and dead-zone.
    pub fn apply(&self, raw: u16) -> f32 {
        let range = (self.max - self.min) as f32;
        if range <= 0.0 {
            return 0.5;
        }

        let normalized = ((raw - self.min) as f32 / range).clamp(0.0, 1.0);

        // Apply deadzone
        let dz_min = self.deadzone_min as f32 / range;
        let dz_max = self.deadzone_max as f32 / range;

        if normalized < dz_min {
            return 0.0;
        }
        if normalized > dz_max {
            return 1.0;
        }

        // Remap to 0-1
        (normalized - dz_min) / (dz_max - dz_min)
    }
}

/// Complete device calibration
///
/// Holds the per-axis calibration data for an entire device (steering wheel,
/// pedal set, etc.) along with a human-readable name and schema version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCalibration {
    /// Human-readable device name (e.g., `"Fanatec CSL DD"`).
    pub name: String,
    /// Per-axis calibration data, indexed by axis number.
    pub axes: Vec<AxisCalibration>,
    /// Schema version for forward-compatible serialization.
    pub version: u32,
}

impl Default for DeviceCalibration {
    fn default() -> Self {
        Self {
            name: String::new(),
            axes: Vec::new(),
            version: 1,
        }
    }
}

impl DeviceCalibration {
    /// Creates a new device calibration with the given name and number of axes.
    ///
    /// Each axis starts with a default full-range calibration.
    pub fn new(name: impl Into<String>, axis_count: usize) -> Self {
        Self {
            name: name.into(),
            axes: vec![AxisCalibration::default(); axis_count],
            version: 1,
        }
    }

    /// Returns a mutable reference to the axis at `index`, or `None` if out of bounds.
    pub fn axis(&mut self, index: usize) -> Option<&mut AxisCalibration> {
        self.axes.get_mut(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_axis_calibration_basic() {
        let calib = AxisCalibration::new(0, 65535);

        assert!((calib.apply(0) - 0.0).abs() < 0.01);
        assert!((calib.apply(32768) - 0.5).abs() < 0.01);
        assert!((calib.apply(65535) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_axis_calibration_with_deadzone() {
        let calib = AxisCalibration::new(0, 65535).with_deadzone(1000, 64535);

        assert!((calib.apply(0) - 0.0).abs() < 0.01);
        assert!((calib.apply(32768) - 0.5).abs() < 0.02);
    }

    #[test]
    fn test_axis_calibration_centered() {
        let calib = AxisCalibration::new(0, 65535).with_center(32768);

        assert!(calib.center.is_some());
        assert_eq!(calib.center.expect("center should be set"), 32768);
    }

    #[test]
    fn test_device_calibration() {
        let mut calib = DeviceCalibration::new("Test Device", 2);

        assert_eq!(calib.axes.len(), 2);

        if let Some(a) = calib.axis(0) {
            *a = AxisCalibration::new(0, 1000);
        }

        if let Some(axis) = calib.axis(0) {
            assert_eq!(axis.max, 1000);
        }
    }
}
