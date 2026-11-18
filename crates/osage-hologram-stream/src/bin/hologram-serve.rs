// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Serves packed holograms (D-134):
//!
//!     hologram-serve ROOT [ADDRESS] [HORIZON]
//!
//! ROOT holds folders written by `hologram-pack`. A client asks
//! `/need?path=P&x=X&y=Y&z=Z&speed=S` with where its viewer is in the
//! hologram's own frame and how fast it goes (m/s); the answer is the
//! blocks of views it can see the hologram from within HORIZON seconds
//! (2 unless given): every direction from the hologram's photographed
//! sphere towards any place the viewer can reach by then. The blocks are
//! then at `/h/P/<file>`. Listens on ADDRESS (0.0.0.0:7882 unless given).

use std::{collections::HashMap, net::TcpListener, path::PathBuf, sync::Arc, sync::Mutex};

use anyhow::Context as _;
use osage_hologram_stream::{Packed, http, views_within};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().context("hologram-serve ROOT [ADDRESS] [HORIZON]")?);
    let address = args.next().unwrap_or_else(|| "0.0.0.0:7882".to_owned());
    let horizon: f32 = args.next().map_or(Ok(2.0), |h| h.parse())?;
    let listener = TcpListener::bind(&address).with_context(|| format!("listening on {address}"))?;
    let packed: Arc<Mutex<HashMap<String, Arc<Packed>>>> = Arc::default();
    let (folder, cache) = (root.clone(), packed.clone());
    let need: Arc<http::Need> = Arc::new(move |query: &str| {
        let fields: HashMap<&str, &str> = query.split('&').filter_map(|kv| kv.split_once('=')).collect();
        let path = fields.get("path").copied().filter(|p| !p.contains("..") && !p.starts_with('/'))?;
        let number = |key: &str| fields.get(key).and_then(|v| v.parse::<f32>().ok());
        let (x, y, z, speed) = (number("x")?, number("y")?, number("z")?, number("speed").unwrap_or(0.0));
        let packed = {
            let mut cache = cache.lock().ok()?;
            match cache.get(path) {
                Some(packed) => packed.clone(),
                None => {
                    let text = std::fs::read_to_string(folder.join(path).join(Packed::FILE)).ok()?;
                    let packed = Arc::new(serde_json::from_str::<Packed>(&text).ok()?);
                    cache.insert(path.to_owned(), packed.clone());
                    packed
                }
            }
        };
        let to_eye = [x - packed.centre[0], y - packed.centre[1], z - packed.centre[2]];
        let distance = (to_eye[0] * to_eye[0] + to_eye[1] * to_eye[1] + to_eye[2] * to_eye[2]).sqrt();
        // Every direction from a point of the sphere to a place the viewer
        // can reach within the horizon.
        let spread = speed.max(0.0) * horizon + packed.radius;
        let half_angle = if distance <= spread { std::f32::consts::PI } else { (spread / distance).asin() };
        let views = views_within(&packed.grid(), to_eye, half_angle);
        serde_json::to_string(&packed.tiling.blocks_of(views)).ok()
    });
    http::serve(listener, root.clone(), need)?;
    println!("hologram-serve: {} on {address}, horizon {horizon} s", root.display());
    loop {
        std::thread::park();
    }
}
