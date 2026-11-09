// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Compiles `spec/proto` with protox (no system `protoc` needed) and keeps the
//! encoded descriptor set, which the server sends as the Welcome schema bundle.

use std::{env, fs, path::PathBuf};

use protox::prost::Message;

const FILES: &[&str] = &["osage/math/v1/math.proto", "osage/v0/protocol.proto", "osage/components/v1/components.proto"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("../../spec/proto");
    println!("cargo:rerun-if-changed={}", spec.display());

    let descriptors = protox::compile(FILES, [&spec])?;
    let out = PathBuf::from(env::var("OUT_DIR")?);
    fs::write(out.join("osage_descriptors.bin"), descriptors.encode_to_vec())?;

    prost_build::Config::new().compile_fds(descriptors)?;
    Ok(())
}
