// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! R11 — holding a frame rate. A tier is chosen for a class of hardware,
//! but one machine of the class is slower than another, and the same
//! machine slower on a busy part of the world. Given the frame rate the
//! person wants ([`FrameTarget`]), this module climbs down a ladder of
//! cheaper pictures until frames come at that rate, and back up when there
//! is room:
//!
//! 1. fewer pixels (down to 80 % in each direction);
//! 2. the costly screen effects off (ambient occlusion, contact shadows,
//!    motion blur, bloom);
//! 3. fewer pixels again (70 %), then the sun's shadows off;
//! 4. fewer pixels still, down to a third.
//!
//! The rate is read as the median of the last frames, so that a single
//! slow frame (a shader compiled, a texture uploaded) moves nothing. Going
//! up is a trial: with vertical sync, a frame rate held at the screen's
//! cannot say how much room is left, so a rung up is tried, and when frames
//! then come too slowly it is undone and the next trial waits twice as long.
//!
//! The rate wanted is the application's (its person's) choice; without one
//! nothing changes, as in the bench.

use std::collections::VecDeque;

use bevy::{
    pbr::{ContactShadows, ScreenSpaceAmbientOcclusion},
    post_process::{bloom::Bloom, motion_blur::MotionBlur},
    prelude::*,
    window::{Monitor, PrimaryMonitor},
};

use crate::{OsageCamera, Sun};

/// Frames a second wanted; `None` keeps the tier's picture whatever the rate.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameTarget(pub Option<f32>);

/// One step of the ladder: how much the picture gives up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rung {
    /// Share of the tier's picture size drawn, in each direction.
    pub scale: f32,
    /// The costly screen effects are on.
    pub effects: bool,
    /// The sun casts shadows.
    pub shadows: bool,
}

const fn rung(scale: f32, effects: bool, shadows: bool) -> Rung {
    Rung { scale, effects, shadows }
}

/// From the tier's own picture down to the cheapest one.
pub const LADDER: [Rung; 9] = [
    rung(1.0, true, true),
    rung(0.9, true, true),
    rung(0.8, true, true),
    rung(0.8, false, true),
    rung(0.7, false, true),
    rung(0.7, false, false),
    rung(0.55, false, false),
    rung(0.45, false, false),
    rung(0.33, false, false),
];

/// Where the picture is on the ladder, and the rate it gets: for the
/// application to show.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Pace {
    /// Index in [`LADDER`].
    pub rung: usize,
    /// Frames a second lately (median).
    pub fps: f32,
    /// Frames a second aimed at: the target, never above the screen's rate.
    pub aim: Option<f32>,
}

impl Default for Pace {
    fn default() -> Self {
        Self { rung: 0, fps: 0.0, aim: None }
    }
}

impl Pace {
    pub fn rung(&self) -> Rung {
        LADDER[self.rung]
    }
}

/// Frames looked at.
const WINDOW: usize = 30;
/// Seconds after a change (or the start) before judging again: the picture
/// is resized, and pipelines may compile.
const SETTLE: f32 = 1.5;
/// Seconds at the rate wanted before a first trial up.
const FIRST_TRIAL: f32 = 4.0;
/// The longest wait between trials, seconds.
const LONGEST_TRIAL: f32 = 64.0;
/// A trial up that fails within this many seconds is undone.
const TRIAL: f32 = 3.0;
/// Below this share of the aim, a rung down; under `FAR`, two.
const SLOW: f32 = 0.92;
const FAR: f32 = 0.6;
/// At or above this share of the aim, there may be room.
const ROOM: f32 = 0.97;

/// The decisions, apart from Bevy: frame times in, rungs out.
#[derive(Debug, Clone)]
pub struct Governor {
    frames: VecDeque<f32>,
    rung: usize,
    /// Seconds since the last change.
    still: f32,
    /// Seconds to wait at the aim before trying a rung up.
    wait: f32,
    /// The last change was a trial up, not yet confirmed.
    trying: bool,
}

impl Default for Governor {
    fn default() -> Self {
        Self { frames: VecDeque::with_capacity(WINDOW), rung: 0, still: 0.0, wait: FIRST_TRIAL, trying: false }
    }
}

impl Governor {
    pub fn rung(&self) -> usize {
        self.rung
    }

    /// Frames a second lately: the median of the last frames.
    pub fn fps(&self) -> f32 {
        if self.frames.is_empty() {
            return 0.0;
        }
        let mut sorted: Vec<f32> = self.frames.iter().copied().collect();
        sorted.sort_by(f32::total_cmp);
        let median = sorted[sorted.len() / 2];
        if median > 0.0 { 1.0 / median } else { 0.0 }
    }

    /// One frame of `seconds`, aiming at `aim` frames a second: the rung to draw at.
    pub fn frame(&mut self, seconds: f32, aim: f32) -> usize {
        if self.frames.len() == WINDOW {
            self.frames.pop_front();
        }
        self.frames.push_back(seconds);
        self.still += seconds;
        if self.trying && self.still > TRIAL {
            // The trial held: the next one may come soon.
            self.trying = false;
            self.wait = FIRST_TRIAL;
        }
        if self.still < SETTLE || self.frames.len() < WINDOW {
            return self.rung;
        }
        let fps = self.fps();
        let last = LADDER.len() - 1;
        if fps < aim * SLOW && self.rung < last {
            if self.trying {
                // No room after all: back down, and wait longer next time.
                self.wait = (self.wait * 2.0).min(LONGEST_TRIAL);
                self.trying = false;
            }
            let steps = if fps < aim * FAR { 2 } else { 1 };
            self.change((self.rung + steps).min(last), false);
        } else if fps >= aim * ROOM && self.rung > 0 && self.still >= self.wait {
            self.change(self.rung - 1, true);
        }
        self.rung
    }

    fn change(&mut self, rung: usize, trying: bool) {
        self.rung = rung;
        self.trying = trying;
        self.still = 0.0;
        self.frames.clear();
    }
}

pub struct PacePlugin;

impl Plugin for PacePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameTarget>()
            .init_resource::<Pace>()
            .init_resource::<Driver>()
            .add_systems(Last, (govern, effects, shadows).chain());
    }
}

#[derive(Resource, Default)]
struct Driver(Governor);

/// The screen's refresh rate, frames a second, when it is known: the
/// primary screen's, or (where the system names none, as Wayland) the
/// fastest screen's.
fn refresh(monitors: &Query<(&Monitor, Has<PrimaryMonitor>)>) -> Option<f32> {
    let hz = |m: &Monitor| m.refresh_rate_millihertz.map(|mhz| mhz as f32 / 1000.0).filter(|hz| *hz > 1.0);
    let primary = monitors.iter().filter(|(_, primary)| *primary).find_map(|(m, _)| hz(m));
    primary.or_else(|| monitors.iter().filter_map(|(m, _)| hz(m)).reduce(f32::max))
}

fn govern(
    time: Res<Time<Real>>,
    target: Res<FrameTarget>,
    monitors: Query<(&Monitor, Has<PrimaryMonitor>)>,
    mut driver: ResMut<Driver>,
    mut pace: ResMut<Pace>,
) {
    let seconds = time.delta_secs();
    if seconds <= 0.0 {
        return;
    }
    // Never more than the screen shows: with vertical sync, more cannot come.
    let aim = target.0.map(|fps| refresh(&monitors).map_or(fps, |hz| fps.min(hz)));
    // The rate that decided, before a change forgets it.
    let before = driver.0.fps();
    let rung = match aim {
        Some(aim) => driver.0.frame(seconds, aim),
        None => {
            driver.0 = Governor::default();
            driver.0.frame(seconds, 0.0)
        }
    };
    let next = Pace { rung, fps: if rung == pace.rung { driver.0.fps() } else { before }, aim };
    if *pace != next {
        if pace.rung != next.rung {
            let rung = LADDER[next.rung];
            info!(
                "pace: {:.0} fps for {:.0} wanted: rung {} ({:.0} % size, effects {}, shadows {})",
                next.fps,
                aim.unwrap_or(0.0),
                next.rung,
                rung.scale * 100.0,
                if rung.effects { "on" } else { "off" },
                if rung.shadows { "on" } else { "off" },
            );
        }
        *pace = next;
    }
}

/// What a camera gave up while the effects are off, to give back.
#[derive(Component, Default)]
struct Stash {
    occlusion: Option<ScreenSpaceAmbientOcclusion>,
    contact: Option<ContactShadows>,
    blur: Option<MotionBlur>,
    bloom: Option<Bloom>,
}

type Effects<'a> = (
    Entity,
    Option<&'a ScreenSpaceAmbientOcclusion>,
    Option<&'a ContactShadows>,
    Option<&'a MotionBlur>,
    Option<&'a Bloom>,
    Option<&'a Stash>,
);

fn effects(mut commands: Commands, pace: Res<Pace>, cameras: Query<Effects, With<OsageCamera>>) {
    let on = pace.rung().effects;
    for (entity, occlusion, contact, blur, bloom, stash) in &cameras {
        let mut camera = commands.entity(entity);
        match (on, stash) {
            (false, None) => {
                let stash = Stash {
                    occlusion: occlusion.cloned(),
                    contact: contact.copied(),
                    blur: blur.cloned(),
                    bloom: bloom.cloned(),
                };
                camera.remove::<(ScreenSpaceAmbientOcclusion, ContactShadows, MotionBlur, Bloom)>().insert(stash);
            }
            (true, Some(stash)) => {
                camera.remove::<Stash>();
                if let Some(c) = stash.occlusion.clone() {
                    camera.insert(c);
                }
                if let Some(c) = stash.contact {
                    camera.insert(c);
                }
                if let Some(c) = stash.blur.clone() {
                    camera.insert(c);
                }
                if let Some(c) = stash.bloom.clone() {
                    camera.insert(c);
                }
            }
            _ => {}
        }
    }
}

/// A sun whose shadows were turned off here, to turn back on.
#[derive(Component)]
struct Unshadowed;

fn shadows(
    mut commands: Commands,
    pace: Res<Pace>,
    mut suns: Query<(Entity, &mut DirectionalLight, Has<Unshadowed>), With<Sun>>,
) {
    let on = pace.rung().shadows;
    for (entity, mut light, unshadowed) in &mut suns {
        if !on && light.shadow_maps_enabled {
            light.shadow_maps_enabled = false;
            commands.entity(entity).insert(Unshadowed);
        } else if on && unshadowed {
            light.shadow_maps_enabled = true;
            commands.entity(entity).remove::<Unshadowed>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `seconds` of frames, each `frame` seconds long.
    fn run(governor: &mut Governor, seconds: f32, mut frame: impl FnMut(usize) -> f32, aim: f32) -> usize {
        let mut elapsed = 0.0;
        while elapsed < seconds {
            let length = frame(governor.rung());
            elapsed += length;
            governor.frame(length, aim);
        }
        governor.rung()
    }

    /// A machine whose frame time falls with each rung by a fifth.
    fn machine(full: f32) -> impl Fn(usize) -> f32 {
        move |rung| full * 0.8f32.powi(rung as i32)
    }

    #[test]
    fn a_fast_machine_keeps_the_full_picture() {
        let mut governor = Governor::default();
        assert_eq!(run(&mut governor, 30.0, |_| 1.0 / 144.0, 60.0), 0);
    }

    #[test]
    fn a_slow_machine_climbs_down_until_the_rate_is_reached() {
        let mut governor = Governor::default();
        let slow = machine(1.0 / 25.0);
        run(&mut governor, 30.0, &slow, 60.0);
        let rung = governor.rung();
        assert!(rung >= 3, "rung {rung}");
        assert!(1.0 / slow(rung) >= 60.0 * SLOW || rung == LADDER.len() - 1);
    }

    #[test]
    fn a_hitch_alone_changes_nothing() {
        let mut governor = Governor::default();
        let mut n = 0;
        let rung = run(
            &mut governor,
            20.0,
            |_| {
                n += 1;
                if n % 200 == 0 { 0.5 } else { 1.0 / 60.0 }
            },
            60.0,
        );
        assert_eq!(rung, 0);
    }

    #[test]
    fn with_vertical_sync_it_tries_up_and_comes_back_without_hunting() {
        // Rung 2 and above make 60 (the screen's rate); rungs 0 and 1 make 45.
        let screen = |rung: usize| if rung >= 2 { 1.0 / 60.0 } else { 1.0 / 45.0 };
        let mut governor = Governor::default();
        run(&mut governor, 10.0, screen, 60.0);
        assert_eq!(governor.rung(), 2);
        // Over two minutes the failed trials space out: few changes.
        let mut changes = 0;
        let mut last = governor.rung();
        let mut elapsed = 0.0;
        while elapsed < 120.0 {
            let length = screen(governor.rung());
            elapsed += length;
            let rung = governor.frame(length, 60.0);
            if rung != last {
                changes += 1;
                last = rung;
            }
        }
        assert!(changes <= 12, "{changes} changes");
        assert!(governor.rung() >= 1);
    }

    #[test]
    fn room_brings_the_picture_back() {
        let mut governor = Governor::default();
        run(&mut governor, 20.0, |_| 1.0 / 20.0, 60.0);
        assert!(governor.rung() > 3);
        assert_eq!(run(&mut governor, 120.0, |_| 1.0 / 200.0, 60.0), 0);
    }
}
