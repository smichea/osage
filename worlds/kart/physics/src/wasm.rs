// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! WebAssembly interface for clients, numbers only — no allocation shared
//! with JavaScript, no pointers:
//!
//! - `track_clear()`, `track_point(x, z)` for each point, then
//!   `track_close(half_width, verge)`;
//! - `kart_step(x, z, heading, speed, slide, drift, charge, boost, spin,
//!   rocket, throttle, steer, drift_button)` advances one step; `kart_get(i)` reads
//!   field `i` of the result, in that order.

use std::cell::RefCell;

use crate::{Controls, Kart, Track, step};

thread_local! {
    static POINTS: RefCell<Vec<[f32; 2]>> = const { RefCell::new(Vec::new()) };
    static TRACK: RefCell<Option<Track>> = const { RefCell::new(None) };
    static KART: RefCell<Kart> = RefCell::new(Kart::default());
}

#[unsafe(no_mangle)]
pub extern "C" fn track_clear() {
    POINTS.with_borrow_mut(Vec::clear);
}

#[unsafe(no_mangle)]
pub extern "C" fn track_point(x: f32, z: f32) {
    POINTS.with_borrow_mut(|points| points.push([x, z]));
}

/// 1 once the track is ready, 0 if it has fewer than three points.
#[unsafe(no_mangle)]
pub extern "C" fn track_close(half_width: f32, verge: f32) -> u32 {
    let points = POINTS.with_borrow_mut(std::mem::take);
    if points.len() < 3 {
        return 0;
    }
    TRACK.set(Some(Track::new(points, half_width, verge)));
    1
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn kart_step(
    x: f32,
    z: f32,
    heading: f32,
    speed: f32,
    slide: f32,
    drift: f32,
    charge: f32,
    boost: f32,
    spin: f32,
    rocket: f32,
    throttle: f32,
    steer: f32,
    drift_button: u32,
) {
    let mut kart = Kart { x, z, heading, speed, slide, drift, charge, boost, spin, rocket };
    TRACK.with_borrow(|track| {
        if let Some(track) = track {
            step(&mut kart, Controls { throttle, steer, drift: drift_button != 0 }, track);
        }
    });
    KART.set(kart);
}

#[unsafe(no_mangle)]
pub extern "C" fn kart_get(field: u32) -> f32 {
    KART.with_borrow(|k| match field {
        0 => k.x,
        1 => k.z,
        2 => k.heading,
        3 => k.speed,
        4 => k.slide,
        5 => k.drift,
        6 => k.charge,
        7 => k.boost,
        8 => k.spin,
        9 => k.rocket,
        _ => f32::NAN,
    })
}
