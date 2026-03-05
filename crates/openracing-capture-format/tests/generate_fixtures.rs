//! One-shot helper to write synthetic capture fixture JSON files.
//!
//! Run with: cargo test --package openracing-capture-format --test generate_fixtures -- --ignored

use openracing_capture_format::build_synthetic_session;

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
#[ignore] // only run explicitly to regenerate fixtures
fn generate_fixture_files() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    std::fs::create_dir_all(&dir)?;

    for vendor in VENDORS {
        let session = build_synthetic_session(vendor, 20)
            .ok_or_else(|| format!("unknown vendor: {vendor}"))?;
        let json = session.to_json()?;
        let path = dir.join(format!("{vendor}_synthetic.json"));
        std::fs::write(&path, &json)?;
    }
    Ok(())
}
