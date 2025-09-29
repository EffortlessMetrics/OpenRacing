use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Tell cargo about our custom cfg
    println!("cargo::rustc-check-cfg=cfg(has_protoc)");
    
    let proto_dir = PathBuf::from("proto");
    let proto_files = &[proto_dir.join("wheel.proto")];
    
    // Try to compile protobuf files, but don't fail if protoc is not available
    match compile_protos(&proto_dir, proto_files) {
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
            eprintln!("  - On Windows: Download from https://github.com/protocolbuffers/protobuf/releases");
            
            // Create a stub file to prevent compilation errors
            create_protobuf_stubs()?;
        }
    }
    
    // Tell cargo to rerun if proto files change
    println!("cargo:rerun-if-changed=proto/wheel.proto");
    println!("cargo:rerun-if-changed=schemas/profile.schema.json");
    
    Ok(())
}

fn compile_protos(proto_dir: &PathBuf, proto_files: &[PathBuf]) -> Result<(), Box<dyn std::error::Error>> {
    // Configure protobuf compilation
    let config = prost_build::Config::new();
    
    // Configure tonic for gRPC service generation
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_with_config(config, proto_files, &[proto_dir.clone()])?;
    
    Ok(())
}

fn create_protobuf_stubs() -> Result<(), Box<dyn std::error::Error>> {
    // This function is no longer needed since we have proper protobuf generation
    // But we keep it for backward compatibility
    Ok(())
}