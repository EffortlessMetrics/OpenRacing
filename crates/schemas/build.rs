use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Generate protobuf types
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&out_dir)
        .compile(&["proto/wheel.proto"], &["proto"])?;

    println!("cargo:rerun-if-changed=proto/wheel.proto");
    
    Ok(())
}