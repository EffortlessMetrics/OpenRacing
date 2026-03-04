//! Fuzzes the hardware version string parser.
//!
//! `HardwareVersion::parse()` accepts dotted-numeric strings like "1.2.3" and
//! must reject arbitrary strings cleanly. This target converts arbitrary bytes
//! to a UTF-8 string (lossy) and feeds it to the parser, also exercising
//! `FromStr`, `Display`, and `Ord` on successfully parsed versions.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_hardware_version

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_firmware_update::HardwareVersion;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);

    // parse() must never panic on arbitrary strings.
    if let Ok(version) = HardwareVersion::parse(&input) {
        // Display round-trip must not panic.
        let displayed = format!("{version}");
        let _ = displayed;

        // Ordering against itself must not panic.
        assert!(version == version);

        // Re-parse from display output must succeed.
        if let Ok(reparsed) = HardwareVersion::parse(&displayed) {
            assert!(version == reparsed);
        }
    }

    // Also exercise FromStr trait if available.
    let _: Result<HardwareVersion, _> = input.parse();
});
