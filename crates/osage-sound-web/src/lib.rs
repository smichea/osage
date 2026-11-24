// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! osage-sound for a browser: the engine built for `wasm32-unknown-unknown`
//! and played in an AudioWorklet, whose render quantum (128 frames) is the
//! engine's block. There is no thread and no queue: the worklet asks for a
//! block when the output needs it, and the engine makes it then, with every
//! pose told until that moment — a pose is heard within one block, plus the
//! output's own buffer.
//!
//! The interface is a handful of functions over numbers and the module's
//! memory, as `osage-kart-physics` gives its own (no bindings crate):
//!
//! - `sound_text(len)`: room for `len` bytes of text, where the caller
//!   writes a scene (the JSON of a glTF asset) or a node's path;
//! - `sound_open(quality, headphones, start)`: the engine on the scene in
//!   the text, heard at `quality` (0 low, 1 medium, 2 high, 3 reference),
//!   for headphones (1) or loudspeakers (0), from the instant `start`, s,
//!   the sound already on its way made; 1 if it opened, 0 if not
//!   (`sound_error` and `sound_error_len` then tell why);
//! - `sound_node(len)`: the node whose path (identifiers joined by ` / `) is
//!   the text's first `len` bytes, or −1; `sound_listener()`: the
//!   listener's node;
//! - `sound_drive(node, x, y, z, qx, qy, qz, qw, vx, vy, vz)`: the node
//!   placed in its parent and going on at a velocity, from the next block
//!   (`Engine::drive`);
//! - `sound_render()`: the next block, at the address it returns: 128
//!   samples of the left ear, then 128 of the right;
//! - `sound_rate()`, `sound_block()`: 48 000 and 128.

use std::cell::RefCell;

use osage_sound::{
    ears::Listening,
    engine::{BLOCK, Engine, Options, Pose, Quality, RATE},
    render::LEAD,
    scene::Scene,
    space::{Placement, Quat, vec3},
};

struct Playing {
    engine: Engine,
    out: Vec<f32>,
}

thread_local! {
    static TEXT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static ERROR: RefCell<String> = const { RefCell::new(String::new()) };
    static PLAYING: RefCell<Option<Playing>> = const { RefCell::new(None) };
}

/// The engine on `scene`, from the instant `start` with the sound already
/// on its way made: what an offline render does before its first sample.
pub fn open(scene: Scene, quality: Quality, listening: Listening, start: f64) -> Engine {
    let mut engine = Engine::new(scene, Options { quality, listening, start: start - LEAD, ..Options::default() });
    let (mut l, mut r) = ([0.0f32; BLOCK], [0.0f32; BLOCK]);
    for _ in 0..(LEAD * RATE as f64 / BLOCK as f64).round() as usize {
        engine.render(&mut l, &mut r);
    }
    engine
}

/// Room for `len` bytes of text; the caller writes them at the address
/// returned.
#[unsafe(no_mangle)]
pub extern "C" fn sound_text(len: usize) -> *mut u8 {
    TEXT.with_borrow_mut(|text| {
        text.clear();
        text.resize(len, 0);
        text.as_mut_ptr()
    })
}

fn text(len: usize) -> String {
    TEXT.with_borrow(|text| String::from_utf8_lossy(&text[..len.min(text.len())]).into_owned())
}

#[unsafe(no_mangle)]
pub extern "C" fn sound_open(quality: u32, headphones: u32, start: f64) -> u32 {
    let scene = match Scene::from_json(&text(usize::MAX)) {
        Ok(scene) => scene,
        Err(why) => {
            ERROR.with_borrow_mut(|error| *error = why.to_string());
            return 0;
        }
    };
    let quality = Quality::ALL[(quality as usize).min(Quality::ALL.len() - 1)];
    let listening = if headphones != 0 { Listening::Headphones } else { Listening::Speakers };
    let engine = open(scene, quality, listening, start);
    PLAYING.with_borrow_mut(|playing| *playing = Some(Playing { engine, out: vec![0.0; 2 * BLOCK] }));
    1
}

/// Why the last scene did not open: its address, and its length in bytes.
#[unsafe(no_mangle)]
pub extern "C" fn sound_error() -> *const u8 {
    ERROR.with_borrow(|error| error.as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn sound_error_len() -> usize {
    ERROR.with_borrow(String::len)
}

#[unsafe(no_mangle)]
pub extern "C" fn sound_node(len: usize) -> i32 {
    let path = text(len);
    let ids: Vec<&str> = path.split(" / ").collect();
    PLAYING.with_borrow(|playing| playing.as_ref().and_then(|p| p.engine.node(&ids)).map_or(-1, |node| node as i32))
}

#[unsafe(no_mangle)]
pub extern "C" fn sound_listener() -> i32 {
    PLAYING.with_borrow(|playing| playing.as_ref().map_or(-1, |p| p.engine.scene().listener as i32))
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn sound_drive(
    node: u32,
    x: f64,
    y: f64,
    z: f64,
    qx: f64,
    qy: f64,
    qz: f64,
    qw: f64,
    vx: f64,
    vy: f64,
    vz: f64,
) {
    PLAYING.with_borrow_mut(|playing| {
        let Some(p) = playing.as_mut() else { return };
        if node as usize >= p.engine.scene().tree.nodes.len() {
            return;
        }
        let placement = Placement { translation: vec3(x, y, z), rotation: Quat { x: qx, y: qy, z: qz, w: qw } };
        p.engine.drive(node as usize, Pose { placement, velocity: vec3(vx, vy, vz) });
    });
}

/// The next block: 128 samples of the left ear, then 128 of the right, at
/// the address returned (null if no scene is open).
#[unsafe(no_mangle)]
pub extern "C" fn sound_render() -> *const f32 {
    PLAYING.with_borrow_mut(|playing| {
        let Some(p) = playing.as_mut() else { return std::ptr::null() };
        let (mut l, mut r) = ([0.0f32; BLOCK], [0.0f32; BLOCK]);
        p.engine.render(&mut l, &mut r);
        p.out[..BLOCK].copy_from_slice(&l);
        p.out[BLOCK..].copy_from_slice(&r);
        p.out.as_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn sound_rate() -> u32 {
    RATE
}

#[unsafe(no_mangle)]
pub extern "C" fn sound_block() -> u32 {
    BLOCK as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use osage_sound::render::render;

    const SCENE: &str = r#"{
      "asset": {"version": "2.0", "extras": {
        "osageObject": {"version": 1, "id": "test", "collisionMaterials": {}, "acousticMaterials": {}},
        "osageSound": {"version": 0, "contactSounds": {}}}},
      "scene": 0,
      "scenes": [{"nodes": [0], "extras": {"osageSound": {"seconds": 1, "seed": 6,
        "listener": {"node": ["world", "ears"]}, "reverb": {"secondsT60": [0.5, 0.4, 0.3]},
        "air": {"celsius": 20, "humidityPercent": 50}}}}],
      "nodes": [
        {"children": [1, 2], "extras": {"osage": {"id": "world"}}},
        {"extras": {"osage": {"id": "ears"}}},
        {"translation": [-30, 0, -5], "extras": {"osage": {"id": "car"}, "osageSound": {
          "motion": {"alongM": [1, 0, 0], "atS": 0, "speedKmh": 72},
          "emitters": [{"id": "horn", "voice": "tone", "params": {"hz": 700}}]}}}
      ]
    }"#;

    fn write(text: &str) -> usize {
        let at = sound_text(text.len());
        // SAFETY: `sound_text` made room for exactly these bytes.
        unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), at, text.len()) };
        text.len()
    }

    #[test]
    fn block_by_block_it_is_the_offline_render() {
        write(SCENE);
        assert_eq!(sound_open(1, 1, 0.0), 1);
        let offline =
            render(&Scene::from_json(SCENE).unwrap(), &Options { quality: Quality::MEDIUM, ..Options::default() }, 1.0);
        for b in 0..offline.left.len() / BLOCK {
            let out = sound_render();
            // SAFETY: `sound_render` returns two blocks of samples that live until the next call.
            let out = unsafe { std::slice::from_raw_parts(out, 2 * BLOCK) };
            assert_eq!(&out[..BLOCK], &offline.left[b * BLOCK..(b + 1) * BLOCK], "block {b}");
            assert_eq!(&out[BLOCK..], &offline.right[b * BLOCK..(b + 1) * BLOCK], "block {b}");
        }
    }

    #[test]
    fn nodes_are_found_by_their_paths_and_a_bad_scene_says_why() {
        write(SCENE);
        assert_eq!(sound_open(0, 0, 0.0), 1);
        let len = write("world / car");
        assert_eq!(sound_node(len), 2);
        let len = write("world / boat");
        assert_eq!(sound_node(len), -1);
        assert_eq!(sound_listener(), 1);
        write("{}");
        assert_eq!(sound_open(0, 0, 0.0), 0);
        assert!(sound_error_len() > 0);
    }
}
