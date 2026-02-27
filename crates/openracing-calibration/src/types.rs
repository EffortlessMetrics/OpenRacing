//! Calibration type definitions

use serde::{Deserialize, Serialize};

/// Calibration point for axis
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CalibrationPoint {
    pub raw: u16,
    pub normalized: f32,
}

impl CalibrationPoint {
    pub fn new(raw: u16, normalized: f32) -> Self {
        Self { raw, normalized }
    }
}

/// Axis calibration data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisCalibration {
    pub min: u16,
    pub center: Option<u16>,
    pub max: u16,
    pub deadzone_min: u16,
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
    pub fn new(min: u16, max: u16) -> Self {
        Self {
            min,
            center: None,
            max,
            deadzone_min: 0,
            deadzone_max: 0xFFFF,
        }
    }

    pub fn with_center(mut self, center: u16) -> Self {
        self.center = Some(center);
        self
    }

    pub fn with_deadzone(mut self, min: u16, max: u16) -> Self {
        self.deadzone_min = min;
        self.deadzone_max = max;
        self
    }

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCalibration {
    pub name: String,
    pub axes: Vec<AxisCalibration>,
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
    pub fn new(name: impl Into<String>, axis_count: usize) -> Self {
        Self {
            name: name.into(),
            axes: vec![AxisCalibration::default(); axis_count],
            version: 1,
        }
    }

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
