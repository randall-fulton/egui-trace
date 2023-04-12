use std::io::Result;
fn main() -> Result<()> {
    prost_build::Config::new()
        .include_file("_includes.rs")
        .compile_protos(
            &["opentelemetry-proto/opentelemetry/proto/collector/trace/v1/trace_service.proto"],
            &["opentelemetry-proto/"],
        )?;
    Ok(())
}
