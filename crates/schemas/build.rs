use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Tell cargo about our custom cfg
    println!("cargo::rustc-check-cfg=cfg(has_protoc)");

    let proto_dir = PathBuf::from("proto");
    let proto_files = &[proto_dir.join("wheel.proto")];

    // Try to compile protobuf files, but don't fail if protoc is not available
    match compile_protos(proto_dir.as_path(), proto_files) {
        Ok(()) => {
            println!("cargo:rustc-cfg=has_protoc");
            println!("Successfully compiled protobuf files");
        }
        Err(e) => {
            eprintln!("Warning: Failed to compile protobuf files: {}", e);
            eprintln!("Protobuf functionality will be disabled.");
            eprintln!("To enable protobuf support, install protoc:");
            eprintln!("  - On Ubuntu/Debian: sudo apt install protobuf-compiler");
            eprintln!("  - On macOS: brew install protobuf");
            eprintln!(
                "  - On Windows: Download from https://github.com/protocolbuffers/protobuf/releases"
            );

            // Create a stub file to prevent compilation errors
            create_protobuf_stubs()?;
        }
    }

    // Tell cargo to rerun if proto files change
    println!("cargo:rerun-if-changed=proto/wheel.proto");
    println!("cargo:rerun-if-changed=schemas/profile.schema.json");

    Ok(())
}

fn compile_protos(
    proto_dir: &std::path::Path,
    proto_files: &[PathBuf],
) -> Result<(), Box<dyn std::error::Error>> {
    // Configure protobuf compilation for deterministic output
    let mut config = prost_build::Config::new();

    // Use vendored protoc for reproducible cross-platform builds.
    // This avoids hard dependency on a system-installed protoc binary.
    if let Ok(protoc_path) = protoc_bin_vendored::protoc_bin_path() {
        config.protoc_executable(protoc_path);
    }

    // Ensure deterministic output by setting consistent options
    config.btree_map(["."]); // Use BTreeMap for deterministic field ordering
    config.bytes(["."]); // Use bytes for binary data

    // Configure tonic for gRPC service generation with deterministic settings
    let proto_dirs = [proto_dir.to_path_buf()];

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_with_config(config, proto_files, &proto_dirs)?;

    Ok(())
}

fn create_protobuf_stubs() -> Result<(), Box<dyn std::error::Error>> {
    // This function is no longer needed since we have proper protobuf generation
    // But we keep it for backward compatibility
    Ok(())
}
