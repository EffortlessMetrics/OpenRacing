//! Fuzzes calibration curve parsing and evaluation.
//!
//! Covers:
//! - JSON deserialization of `AxisCalibration`, `DeviceCalibration`, and
//!   `CalibrationPoint`.
//! - `AxisCalibration::apply()` with boundary/edge-case raw values.
//! - Builder methods: `with_center()`, `with_deadzone()`.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_calibration_curve

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_calibration::{AxisCalibration, CalibrationPoint, DeviceCalibration};

fuzz_target!(|data: &[u8]| {
    // --- JSON deserialization paths ---
    if let Ok(json) = core::str::from_utf8(data) {
        let _ = serde_json::from_str::<AxisCalibration>(json);
        let _ = serde_json::from_str::<CalibrationPoint>(json);
        let _ = serde_json::from_str::<DeviceCalibration>(json);
    }

    // --- Binary-driven evaluation path ---
    // Use raw bytes to construct AxisCalibration parameters and exercise apply().
    if data.len() >= 10 {
        let min = u16::from_le_bytes([data[0], data[1]]);
        let max = u16::from_le_bytes([data[2], data[3]]);
        let center = u16::from_le_bytes([data[4], data[5]]);
        let dz_min = u16::from_le_bytes([data[6], data[7]]);
        let dz_max = u16::from_le_bytes([data[8], data[9]]);

        let cal = AxisCalibration::new(min, max)
            .with_center(center)
            .with_deadzone(dz_min, dz_max);

        // Exercise apply() with several raw values from remaining data.
        for chunk in data[10..].chunks(2) {
            if chunk.len() == 2 {
                let raw = u16::from_le_bytes([chunk[0], chunk[1]]);
                let result = cal.apply(raw);
                // Sanity: result should be a finite f32.
                assert!(result.is_finite());
            }
        }

        // Also test boundary values.
        let _ = cal.apply(0);
        let _ = cal.apply(u16::MAX);
        let _ = cal.apply(min);
        let _ = cal.apply(max);
    }
});
