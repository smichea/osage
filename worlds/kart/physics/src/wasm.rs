// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! WebAssembly interface for clients, numbers only — no allocation shared
//! with JavaScript, no pointers:
//!
//! - `track_clear()`, `track_point(x, z)` for each point, `track_crest(along,
//!   rise, fall, height)` for each crest, then `track_close(half_width, verge)`;
//! - `track_lift(along)`: the road's height that far along (over a crest);
//! - `kart_set(i, value)` sets field `i` of the kart, `kart_step(throttle,
//!   steer, drift_button)` advances it one step, `kart_get(i)` reads field
//!   `i`. Fields, in order: x, z, heading, speed, slide, drift, charge,
//!   boost, spin, rocket, height, climb, trick, stall, draft, held.

use std::cell::RefCell;

use crate::{Controls, Crest, Kart, Track, step};

thread_local! {
    static POINTS: RefCell<Vec<[f32; 2]>> = const { RefCell::new(Vec::new()) };
    static CRESTS: RefCell<Vec<Crest>> = const { RefCell::new(Vec::new()) };
    static TRACK: RefCell<Option<Track>> = const { RefCell::new(None) };
    static KART: RefCell<Kart> = RefCell::new(Kart::default());
}

#[unsafe(no_mangle)]
pub extern "C" fn track_clear() {
    POINTS.with_borrow_mut(Vec::clear);
    CRESTS.with_borrow_mut(Vec::clear);
}

#[unsafe(no_mangle)]
pub extern "C" fn track_point(x: f32, z: f32) {
    POINTS.with_borrow_mut(|points| points.push([x, z]));
}

#[unsafe(no_mangle)]
pub extern "C" fn track_crest(along: f32, rise: f32, fall: f32, height: f32) {
    CRESTS.with_borrow_mut(|crests| crests.push(Crest { along, rise, fall, height }));
}

/// The road's height `along` metres along the centre line: 0 but over a crest.
#[unsafe(no_mangle)]
pub extern "C" fn track_lift(along: f32) -> f32 {
    TRACK.with_borrow(|track| track.as_ref().map_or(0.0, |track| track.lift(along).0))
}

/// 1 once the track is ready, 0 if it has fewer than three points.
#[unsafe(no_mangle)]
pub extern "C" fn track_close(half_width: f32, verge: f32) -> u32 {
    let points = POINTS.with_borrow_mut(std::mem::take);
    let crests = CRESTS.with_borrow_mut(std::mem::take);
    if points.len() < 3 {
        return 0;
    }
    TRACK.set(Some(Track::new(points, half_width, verge).with_crests(crests)));
    1
}

fn field(kart: &mut Kart, i: u32) -> Option<&mut f32> {
    Some(match i {
        0 => &mut kart.x,
        1 => &mut kart.z,
        2 => &mut kart.heading,
        3 => &mut kart.speed,
        4 => &mut kart.slide,
        5 => &mut kart.drift,
        6 => &mut kart.charge,
        7 => &mut kart.boost,
        8 => &mut kart.spin,
        9 => &mut kart.rocket,
        10 => &mut kart.height,
        11 => &mut kart.climb,
        12 => &mut kart.trick,
        13 => &mut kart.stall,
        14 => &mut kart.draft,
        15 => &mut kart.held,
        _ => return None,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn kart_set(i: u32, value: f32) {
    KART.with_borrow_mut(|kart| {
        if let Some(f) = field(kart, i) {
            *f = value;
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn kart_step(throttle: f32, steer: f32, drift_button: u32) {
    TRACK.with_borrow(|track| {
        if let Some(track) = track {
            KART.with_borrow_mut(|kart| step(kart, Controls { throttle, steer, drift: drift_button != 0 }, track));
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn kart_get(i: u32) -> f32 {
    KART.with_borrow_mut(|kart| field(kart, i).map_or(f32::NAN, |f| *f))
}
