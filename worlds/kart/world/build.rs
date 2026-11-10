// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Compiles `proto/kart` with protox and keeps the encoded descriptor set,
//! which the world adds to its Welcome schema bundle.

use std::{env, fs, path::PathBuf};

use protox::prost::Message;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("proto");
    println!("cargo:rerun-if-changed={}", proto.display());

    let descriptors = protox::compile(["kart/v1/kart.proto"], [&proto])?;
    let out = PathBuf::from(env::var("OUT_DIR")?);
    fs::write(out.join("kart_descriptors.bin"), descriptors.encode_to_vec())?;

    prost_build::Config::new().compile_fds(descriptors)?;
    Ok(())
}
