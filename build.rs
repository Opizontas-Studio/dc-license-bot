fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 编译 proto/registry.proto 生成 gRPC 客户端代码
    tonic_build::compile_protos("proto/registry.proto")?;
    // 编译 proto/license_management.proto 生成业务 gRPC 代码
    tonic_build::compile_protos("proto/license_management.proto")?;
    Ok(())
}
