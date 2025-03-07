fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/p11_grpc.proto")?;
    Ok(())
}
