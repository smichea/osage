// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Decodes a block file and writes its pictures as raw RGBA, one after the
//! other: `peek BLOCK.ivf SERIES OUT.rgba`.

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let series = match args[1].as_str() {
        "depth" => osage_hologram_stream::Series::Depth,
        "normal" => osage_hologram_stream::Series::Normal,
        _ => osage_hologram_stream::Series::View,
    };
    let pictures = osage_hologram_stream::decode::block(&std::fs::read(&args[0])?, series)?;
    let mut out = Vec::new();
    for p in &pictures {
        out.extend_from_slice(&p.rgba);
    }
    std::fs::write(&args[2], out)?;
    println!("{} pictures of {}x{}", pictures.len(), pictures[0].width, pictures[0].height);
    Ok(())
}
