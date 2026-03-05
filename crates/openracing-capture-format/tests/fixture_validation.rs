//! Tests that load the on-disk fixture JSON files and validate them.

use openracing_capture_format::CaptureSession;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures");

const VENDORS: &[&str] = &[
    "moza",
    "fanatec",
    "thrustmaster",
    "simagic",
    "cammus",
    "vrs",
    "leo_bodnar",
    "cube_controls",
];

#[test]
fn all_fixtures_load_and_validate() -> Result<(), Box<dyn std::error::Error>> {
    for vendor in VENDORS {
        let path = format!("{FIXTURE_DIR}/{vendor}_synthetic.json");
        let json =
            std::fs::read_to_string(&path).map_err(|e| format!("failed to read {path}: {e}"))?;
        let session = CaptureSession::from_json(&json)
            .map_err(|e| format!("failed to parse {vendor}: {e}"))?;

        // Basic structural checks
        assert!(
            session.metadata.synthetic,
            "{vendor}: expected synthetic flag"
        );
        assert_eq!(
            session.metadata.format_version, "1.0",
            "{vendor}: wrong version"
        );
        assert!(
            session.records.len() >= 10,
            "{vendor}: expected >= 10 records"
        );
        session
            .validate_timestamps()
            .map_err(|e| format!("{vendor}: {e}"))?;
    }
    Ok(())
}
