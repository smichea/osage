// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The race as television shows it (design/04 G4.4, D-063): while the karts
//! wait on the grid, a camera flies the circuit, low over the road; when
//! the player crosses the line for the last time, the last seconds before
//! it are shown again, slowed, from beside the kart and then from the
//! trackside by the line.
//!
//! Only in a game: the bench's frames stay the same. The camera is set
//! through [`Viewpoint`], which the chase camera eases out of when the
//! show ends; a viewpoint given on the command line is left alone.

use std::collections::VecDeque;

use bevy::prelude::*;
use osage_kart_physics::{Kart, wrap};

use crate::{
    hud::{Lights, RaceView},
    scene::{Course, Pose, Stage, Viewpoint},
};

/// Seconds of the race kept, for the finish to be shown again.
const KEPT: f32 = 6.0;
/// Seconds before the line the replay starts, and after it that it ends.
const BEFORE: f32 = 3.2;
const AFTER: f32 = 0.8;
/// How fast the replay runs: half speed.
const SLOW: f32 = 0.5;
/// The flyover's speed along the circuit, m/s, and its camera's height.
const FLY: f32 = 17.0;
const FLY_HEIGHT: f32 = 11.0;

pub struct ShowPlugin;

impl Plugin for ShowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Show>().add_systems(Update, present.in_set(Stage::Simulate).after(crate::play::Summarised));
    }
}

/// Where every kart was, frame by frame: (seconds, each kart).
type Tape = VecDeque<(f32, Vec<(Entity, Kart)>)>;

#[derive(Resource, Default)]
pub struct Show {
    tape: Tape,
    mode: Mode,
    /// Whether the player had finished, a frame ago.
    finished: bool,
    /// The show set the viewpoint (one from the command line is not its own).
    owns_view: bool,
    /// How far along the circuit the flyover is, m.
    flown: f32,
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
enum Mode {
    #[default]
    Live,
    /// The player crossed the line at this time: the replay waits for what
    /// follows it to be on the tape.
    Finished { at: f32, kart: Entity },
    /// The replay: the line was crossed at `at`; `shown` is the time on the
    /// tape now; `line` is where the kart crossed it, facing `heading`.
    Replay { at: f32, kart: Entity, shown: f32, line: Vec3, heading: f32 },
}

/// The kart on the tape at `t`, between the two frames either side.
fn taped(tape: &Tape, kart: Entity, t: f32) -> Option<Kart> {
    let after = tape.iter().position(|(time, _)| *time >= t)?;
    let find = |frame: &(f32, Vec<(Entity, Kart)>)| frame.1.iter().find(|(e, _)| *e == kart).map(|(_, k)| *k);
    let before = after.checked_sub(1).map(|i| &tape[i]).and_then(find);
    let b = find(&tape[after]).or(before)?;
    let Some(a) = before else { return Some(b) };
    let before = after - 1;
    let (ta, tb) = (tape[before].0, tape[after].0);
    let f = if tb > ta { ((t - ta) / (tb - ta)).clamp(0.0, 1.0) } else { 1.0 };
    Some(Kart {
        x: a.x + (b.x - a.x) * f,
        z: a.z + (b.z - a.z) * f,
        heading: a.heading + wrap(b.heading - a.heading) * f,
        speed: a.speed + (b.speed - a.speed) * f,
        height: a.height + (b.height - a.height) * f,
        ..b
    })
}

/// Where the flyover's camera is and looks, `flown` metres along the circuit.
pub fn flyover(course: &Course, flown: f32, time: f32) -> (Vec3, Vec3) {
    let track = &course.0.track;
    let point = |along: f32| {
        let ([x, z], heading) = track.at(along.rem_euclid(track.length()));
        (Vec3::new(x, track.lift(along.rem_euclid(track.length())).0, z), heading)
    };
    let (here, heading) = point(flown);
    let (ahead, _) = point(flown + 32.0);
    // Swaying a little from side to side, as a camera on a drone would.
    let right = Vec3::new(-heading.cos(), 0.0, heading.sin());
    let sway = right * (time * 0.21).sin() * track.half_width * 0.6;
    (here + Vec3::Y * FLY_HEIGHT + sway, ahead + Vec3::Y * 1.0)
}

#[allow(clippy::too_many_arguments)]
fn present(
    mut commands: Commands,
    time: Res<Time>,
    course: Option<Res<Course>>,
    viewpoint: Option<Res<Viewpoint>>,
    mut show: ResMut<Show>,
    mut view: ResMut<RaceView>,
    mut karts: Query<(Entity, &mut Pose)>,
    followed: Query<Entity, With<crate::scene::Followed>>,
) {
    let now = time.elapsed_secs();
    let Some(course) = course else { return };
    // A viewpoint the show did not set is the player's: no show.
    if viewpoint.is_some() && !show.owns_view {
        return;
    }
    let me = view.me.and_then(|i| view.karts.get(i));
    let finished = me.is_some_and(|me| me.finish_ms > 0);
    let just_finished = finished && !show.finished;
    show.finished = finished;

    if !matches!(show.mode, Mode::Replay { .. }) {
        let frame = karts.iter().map(|(entity, pose)| (entity, pose.kart)).collect();
        show.tape.push_back((now, frame));
        while show.tape.front().is_some_and(|(t, _)| *t < now - KEPT) {
            show.tape.pop_front();
        }
    }
    if just_finished && let Ok(kart) = followed.single() {
        show.mode = Mode::Finished { at: now, kart };
    }
    let mut wanted: Option<(Vec3, Vec3)> = None;
    match show.mode {
        Mode::Finished { at, kart } if now >= at + AFTER => {
            let line = taped(&show.tape, kart, at).unwrap_or_default();
            info!("show: the finish again, slowed, from {:.0} m before the line", line.speed * BEFORE);
            show.mode = Mode::Replay {
                at,
                kart,
                shown: at - BEFORE,
                line: Vec3::new(line.x, line.height, line.z),
                heading: line.heading,
            };
        }
        Mode::Replay { at, kart, shown, line, heading } => {
            let shown = shown + time.delta_secs() * SLOW;
            if shown >= at + AFTER || karts.get(kart).is_err() {
                info!("show: back to the race");
                show.mode = Mode::Live;
                show.tape.clear();
            } else {
                show.mode = Mode::Replay { at, kart, shown, line, heading };
                for (entity, mut pose) in &mut karts {
                    if let Some(then) = taped(&show.tape, entity, shown) {
                        pose.kart = then;
                    }
                }
                let then = taped(&show.tape, kart, shown).unwrap_or_default();
                let at_kart = Vec3::new(then.x, then.height + 0.6, then.z);
                let forward = Vec3::new(then.heading.sin(), 0.0, then.heading.cos());
                let left = Vec3::new(then.heading.cos(), 0.0, -then.heading.sin());
                wanted = Some(if shown < at - 1.2 {
                    // Beside the kart, low, a little ahead: the driver at work.
                    (at_kart + left * 3.2 + forward * 2.2 + Vec3::Y * 0.2, at_kart)
                } else {
                    // By the line, beyond the barrier, the kart coming past.
                    let side = Vec3::new(heading.cos(), 0.0, -heading.sin());
                    let road = course.0.track.half_width + 4.5;
                    let ahead = Vec3::new(heading.sin(), 0.0, heading.cos());
                    (line + side * road + ahead * 6.0 + Vec3::Y * 1.6, at_kart)
                });
            }
        }
        _ => {}
    }
    view.replay = matches!(show.mode, Mode::Replay { .. });
    // On the grid, waiting: the circuit flown over.
    if wanted.is_none() && matches!(view.stage, Lights::Waiting | Lights::Ready(_)) {
        show.flown += FLY * time.delta_secs();
        wanted = Some(flyover(&course, show.flown, now));
    }
    match wanted {
        Some((from, to)) => {
            if !show.owns_view {
                info!("show: {}", if view.replay { "replay" } else { "flying over the circuit" });
            }
            commands.insert_resource(Viewpoint { from, to });
            show.owns_view = true;
        }
        None if show.owns_view => {
            info!("show: the chase camera again");
            commands.remove_resource::<Viewpoint>();
            show.owns_view = false;
            // The next flyover starts over the grid.
            show.flown = 0.0;
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kart(x: f32, heading: f32) -> Kart {
        Kart { x, heading, speed: x, ..Kart::default() }
    }

    #[test]
    fn the_tape_gives_a_kart_between_its_frames() {
        let (a, b) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        let tape: Tape =
            [(0.0, vec![(a, kart(0.0, 3.1)), (b, kart(5.0, 0.0))]), (1.0, vec![(a, kart(10.0, -3.1))])].into();
        let half = taped(&tape, a, 0.5).unwrap();
        assert!((half.x - 5.0).abs() < 1e-5 && (half.speed - 5.0).abs() < 1e-5, "{half:?}");
        // Across the turn of the angle, the short way round.
        assert!(half.heading.abs() > 3.1, "{}", half.heading);
        assert_eq!(taped(&tape, a, -1.0).unwrap().x, 0.0, "before the tape: its first frame");
        assert_eq!(taped(&tape, b, 0.5).unwrap().x, 5.0, "missing from a frame: the one there is");
        assert!(taped(&tape, a, 2.0).is_none(), "nothing after the tape");
    }

    #[test]
    fn the_flyover_flies_over_the_road_and_looks_ahead() {
        let course = Course(osage_world_kart::Circuit::new());
        let track = &course.0.track;
        for step in 0..40 {
            let flown = step as f32 * track.length() / 40.0;
            let (from, to) = flyover(&course, flown, step as f32);
            let below = track.locate(from.x, from.z);
            assert!(below.lateral.abs() <= track.half_width, "over the road: {below:?}");
            assert!(from.y > FLY_HEIGHT - 0.1 && to.y < from.y, "above, looking down the road");
            assert!(from.distance(to) > 20.0, "looking ahead");
        }
    }
}
