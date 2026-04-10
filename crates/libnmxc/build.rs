//
// SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0
//
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let proto_dir = manifest_dir.join("../rpc/proto");
    let proto_file = proto_dir.join("nmx_c.proto");

    tonic_prost_build::configure()
        .build_server(false)
        .build_client(true)
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile_protos(&[proto_file], &[proto_dir])?;

    // Server registers the service as NMX_Controller (with underscore); generated code uses NMXController.
    // Rewrite paths so the client matches the server (grpcurl uses nmx_c.NMX_Controller/Hello).
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    let generated = out_dir.join("nmx_c.rs");
    let content = std::fs::read_to_string(&generated)?;
    let content = content.replace("NMXController", "NMX_Controller");
    std::fs::write(generated, content)?;

    Ok(())
}
