//! Fuzzes the KS report map parser across multiple representative configurations.
//!
//! Exercises the `KsReportMap::parse()` path with varied clutch modes,
//! report-ID gating, and axis offsets to cover different control surface layouts.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_ks_report_variants

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_ks::{KsAxisSource, KsClutchMode, KsReportMap};

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // Use the first byte to select a configuration variant, fuzz the rest.
    let variant = data[0] % 4;
    let payload = &data[1..];

    let mut map = KsReportMap::empty();

    match variant {
        0 => {
            // Split-axis clutch with report-ID filter.
            map.report_id = Some(0x01);
            map.buttons_offset = Some(11);
            map.hat_offset = Some(27);
            map.clutch_mode_hint = KsClutchMode::IndependentAxis;
            map.clutch_left_axis = Some(KsAxisSource::new(5, false));
            map.clutch_right_axis = Some(KsAxisSource::new(7, false));
        }
        1 => {
            // Combined clutch, no report-ID, inverted axis.
            map.report_id = None;
            map.buttons_offset = Some(8);
            map.hat_offset = None;
            map.clutch_mode_hint = KsClutchMode::CombinedAxis;
            map.clutch_combined_axis = Some(KsAxisSource::new(3, true));
        }
        2 => {
            // Button-based clutch.
            map.report_id = Some(0x02);
            map.buttons_offset = Some(14);
            map.hat_offset = Some(20);
            map.clutch_mode_hint = KsClutchMode::Button;
        }
        _ => {
            // Minimal map — no optional fields.
            map.report_id = None;
            map.buttons_offset = None;
            map.hat_offset = None;
            map.clutch_mode_hint = KsClutchMode::CombinedAxis;
        }
    }

    // Must never panic on arbitrary bytes.
    let _ = map.parse(0, payload);
});
