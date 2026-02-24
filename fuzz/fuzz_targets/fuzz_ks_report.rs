//! Fuzzes the KS control surface report map parser.
//!
//! Uses a representative KS layout (button block + hat + combined-axis clutch)
//! to exercise the `KsReportMap::parse()` path across arbitrary input bytes.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_ks_report

#![deny(static_mut_refs)]
#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_ks::{KsAxisSource, KsClutchMode, KsReportMap};

fuzz_target!(|data: &[u8]| {
    // Minimal representative KS map (buttons at 11, hat at 27, combined clutch at 7)
    let mut map = KsReportMap::empty();
    // Keep report_id unset so fuzzing does not trivially short-circuit on byte[0].
    map.report_id = None;
    map.buttons_offset = Some(11);
    map.hat_offset = Some(27);
    map.clutch_mode_hint = KsClutchMode::CombinedAxis;
    map.clutch_combined_axis = Some(KsAxisSource::new(7, false));

    // Must never panic on arbitrary bytes.
    let _ = map.parse(0, data);
});
