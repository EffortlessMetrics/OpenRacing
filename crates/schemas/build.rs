// TODO: Re-enable protobuf generation once protoc is available
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Skip protobuf generation for now
    println!("cargo:rerun-if-changed=proto/wheel.proto");
    Ok(())
}