// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Lists the tiles of light a street needs (D-136), for the baker:
//!
//!     paris-tiles STREET [X,Z RADIUS] > tiles.json
//!
//! STREET holds the walk grid of `tools/bistro/export.py`. For every place
//! a walker can stand (within RADIUS m of X, Z if given), the tiles of the
//! block its eyes are in; each once, with where its probe stands and how
//! it looks (the glTF's frame: x east, y up, z south).

use std::collections::BTreeSet;

use bevy::math::Vec2;
use osage_world_paris::{
    tiles::{Block, Tile},
    walk::{EYES, Walk},
};

fn main() {
    let mut args = std::env::args().skip(1);
    let usage = "paris-tiles STREET [X,Z RADIUS]";
    let street = std::path::PathBuf::from(args.next().expect(usage));
    let around = args.next().map(|xz| {
        let (x, z) = xz.split_once(',').expect("X,Z");
        Vec2::new(x.parse().expect("X"), z.parse().expect("Z"))
    });
    let radius: f32 = args.next().map_or(f32::INFINITY, |r| r.parse().expect("RADIUS"));
    let walk = Walk::parse(
        &std::fs::read_to_string(street.join("walk.json")).expect("walk.json"),
        &std::fs::read(street.join("walk.bin")).expect("walk.bin"),
    )
    .expect("the walk grid");
    let mut tiles: BTreeSet<Tile> = BTreeSet::new();
    let mut places = 0;
    for p in walk.places() {
        if around.is_some_and(|c| c.distance(Vec2::new(p.x, p.z)) > radius) {
            continue;
        }
        places += 1;
        tiles.extend(Block::of(p + bevy::math::Vec3::Y * EYES).tiles());
    }
    let list: Vec<serde_json::Value> = tiles
        .iter()
        .map(|t| {
            serde_json::json!({
                "key": t.key(),
                "centre": t.centre().to_array(),
                "normal": t.face.normal().to_array(),
                "up": t.face.up().to_array(),
            })
        })
        .collect();
    eprintln!("paris-tiles: {places} places to stand, {} tiles", list.len());
    println!("{}", serde_json::to_string_pretty(&list).expect("JSON"));
}
