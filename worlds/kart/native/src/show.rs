// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The race as television shows it (design/04 G4.4, D-063): while the karts
//! wait on the grid, a camera flies the circuit, low over the road; when
//! the player crosses the line for the last time, the last seconds before
//! it are shown again, slowed, from beside the kart and then from the
//! trackside by the line.
//!
//! The whole race is kept too (D-082), thirty times a second: between
//! races — on the results and on the grid — R shows it again as
//! television would, cutting between cameras by the track, a helicopter's
//! and one on the kart; left and right follow another kart, up and down
//! run it faster or slower, Space pauses, R goes back. The countdown of the
//! next race ends it.
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
/// The whole race is kept this often, s.
const SAMPLE: f32 = 1.0 / 30.0;
/// The speeds a whole-race replay runs at.
const SPEEDS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];
/// Television's cameras by the track: one every so many metres.
const CAMERA_EVERY: f32 = 42.0;
/// The flyover's speed along the circuit, m/s, and its camera's height.
const FLY: f32 = 17.0;
const FLY_HEIGHT: f32 = 11.0;

pub struct ShowPlugin;

impl Plugin for ShowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Show>()
            .add_systems(Update, present.in_set(Stage::Simulate).in_set(Shown).after(crate::play::Summarised));
    }
}

/// After the show: what overrides it (the photo mode) runs after.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Shown;

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
    /// The whole race, thirty times a second, from its countdown.
    race: Vec<(f32, Vec<(Entity, Kart)>)>,
    /// Whether the race was on, a frame ago (a new countdown starts a new tape).
    racing: bool,
    /// The whole race shown again, when it is.
    watching: Option<Watching>,
}

/// A whole-race replay as it runs.
#[derive(Clone, Copy, Debug)]
struct Watching {
    /// The time on the tape now.
    shown: f32,
    /// Which of [`SPEEDS`]; whether it is paused.
    speed: usize,
    paused: bool,
    /// The kart followed, as an index into the tape's karts.
    follow: usize,
    /// The shot, and how many have been cut to.
    shot: Shot,
    cuts: u32,
}

/// What the camera shows of the kart followed.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shot {
    /// From a camera by the track, `along` metres round, on one side.
    Trackside { along: f32, side: f32, height: f32 },
    /// From a helicopter behind and above it, until the time on the tape.
    Helicopter { until: f32 },
    /// From the kart itself, behind the driver, until the time on the tape.
    Onboard { until: f32 },
}

impl Show {
    /// Whether the whole race is being shown again: the arrow keys are its.
    pub fn watching(&self) -> bool {
        self.watching.is_some()
    }
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

/// A tape's karts at `t`, each between its two frames either side.
fn frame_at(tape: &[(f32, Vec<(Entity, Kart)>)], t: f32) -> Vec<(Entity, Kart)> {
    let after = tape.partition_point(|(time, _)| *time < t).min(tape.len().saturating_sub(1));
    let Some((tb, b)) = tape.get(after) else { return Vec::new() };
    let Some((ta, a)) = after.checked_sub(1).and_then(|i| tape.get(i)) else { return b.clone() };
    let f = if tb > ta { ((t - ta) / (tb - ta)).clamp(0.0, 1.0) } else { 1.0 };
    b.iter()
        .map(|&(entity, kb)| match a.iter().find(|(e, _)| *e == entity) {
            Some(&(_, ka)) => (entity, blend(ka, kb, f)),
            None => (entity, kb),
        })
        .collect()
}

fn blend(a: Kart, b: Kart, f: f32) -> Kart {
    Kart {
        x: a.x + (b.x - a.x) * f,
        z: a.z + (b.z - a.z) * f,
        heading: a.heading + wrap(b.heading - a.heading) * f,
        speed: a.speed + (b.speed - a.speed) * f,
        height: a.height + (b.height - a.height) * f,
        ..b
    }
}

/// Whether `along` (m round the circuit) is in a tunnel.
fn in_tunnel(course: &Course, along: f32) -> bool {
    let length = course.0.track.length();
    course.0.spans.iter().any(|&(kind, from, span)| {
        kind == osage_kart_tracks::SpanKind::Tunnel && (along - from).rem_euclid(length) <= span
    })
}

/// A small hash in 0–1 of a number.
fn hash(x: f32) -> f32 {
    let h = (x.to_bits().wrapping_mul(0x9E37_79B1) ^ 0x85EB_CA77).rotate_left(13).wrapping_mul(0xC2B2_AE35);
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Television's next camera by the track for a kart `along` metres round:
/// the first ahead of it, on alternate sides, at heights of their own;
/// none in a tunnel.
fn trackside(course: &Course, along: f32) -> Option<Shot> {
    let length = course.0.track.length();
    let n = ((along + 10.0) / CAMERA_EVERY).ceil();
    let at = (n * CAMERA_EVERY).rem_euclid(length);
    if in_tunnel(course, at) || in_tunnel(course, along) {
        return None;
    }
    let side = if (n as i64).rem_euclid(2) == 0 { 1.0 } else { -1.0 };
    Some(Shot::Trackside { along: at, side, height: 1.3 + 4.5 * hash(n) })
}

/// The next shot of a replay, the `cuts`-th: mostly by the track, now and
/// then from the helicopter or the kart; on the kart in a tunnel.
fn next_shot(course: &Course, along: f32, shown: f32, cuts: u32) -> Shot {
    match cuts % 5 {
        2 if !in_tunnel(course, along) => Shot::Helicopter { until: shown + 5.0 },
        4 => Shot::Onboard { until: shown + 4.0 },
        _ => trackside(course, along).unwrap_or(Shot::Onboard { until: shown + 3.0 }),
    }
}

/// Whether a shot is over for a kart `along` metres round at `shown`.
fn over(course: &Course, shot: Shot, along: f32, shown: f32) -> bool {
    let length = course.0.track.length();
    match shot {
        Shot::Trackside { along: at, .. } => {
            // How far past the camera the kart is, the short way round.
            let past = (along - at + length / 2.0).rem_euclid(length) - length / 2.0;
            past > 12.0 || past < -80.0 || in_tunnel(course, along)
        }
        Shot::Helicopter { until } => shown >= until || shown < until - 10.0 || in_tunnel(course, along),
        Shot::Onboard { until } => shown >= until || shown < until - 10.0,
    }
}

/// Where a shot's camera is and looks, for a kart.
fn framing(course: &Course, shot: Shot, kart: &Kart) -> (Vec3, Vec3) {
    let track = &course.0.track;
    let at = Vec3::new(kart.x, kart.height, kart.z);
    let forward = Vec3::new(kart.heading.sin(), 0.0, kart.heading.cos());
    let right = Vec3::new(kart.heading.cos(), 0.0, -kart.heading.sin());
    match shot {
        Shot::Trackside { along, side, height } => {
            let ([x, z], heading) = track.at(along);
            let across = Vec3::new(heading.cos(), 0.0, -heading.sin());
            let off = track.half_width + track.verge + osage_kart_land::BARRIER + 2.5;
            let foot = Vec3::new(x, track.lift(along).0, z) + across * (side * off);
            (foot + Vec3::Y * height, at + Vec3::Y * 0.5)
        }
        Shot::Helicopter { .. } => (at - forward * 13.0 + right * 3.0 + Vec3::Y * 8.5, at + forward * 5.0),
        Shot::Onboard { .. } => (at - forward * 1.0 + Vec3::Y * 1.15, at + forward * 12.0 + Vec3::Y * 0.7),
    }
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
    remotes: Query<&crate::play::Remote>,
    photo: Res<crate::photo::Photo>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let now = time.elapsed_secs();
    let Some(course) = course else { return };
    // The photo mode has the camera, and the picture is frozen.
    if photo.is_on() {
        return;
    }
    // A viewpoint the show did not set is the player's: no show.
    if viewpoint.is_some() && !show.owns_view {
        return;
    }
    // The whole race (D-082): kept from its countdown, thirty times a
    // second; shown again between races.
    let on = matches!(view.stage, Lights::Count(_) | Lights::Go | Lights::Racing | Lights::Results(_));
    if on && !show.racing {
        show.race.clear();
        show.watching = None;
    }
    show.racing = on;
    if matches!(view.stage, Lights::Count(_) | Lights::Go | Lights::Racing) {
        show.watching = None;
    }
    let between = matches!(view.stage, Lights::Waiting | Lights::Ready(_) | Lights::Results(_));
    let kept = show.race.last().map_or(0.0, |(t, _)| *t) - show.race.first().map_or(0.0, |(t, _)| *t);
    if keys.just_pressed(KeyCode::KeyR) && between {
        show.watching = match show.watching {
            Some(_) => None,
            None if kept > 3.0 => {
                let start = show.race[0].0;
                let mine = followed.single().ok();
                let follow = show.race[0].1.iter().position(|(e, _)| Some(*e) == mine).unwrap_or(0);
                show.mode = Mode::Live;
                Some(Watching {
                    shown: start,
                    speed: 2,
                    paused: false,
                    follow,
                    shot: Shot::Onboard { until: start + 4.0 },
                    cuts: 0,
                })
            }
            None => None,
        };
    }
    if on
        && show.watching.is_none()
        && !matches!(show.mode, Mode::Replay { .. })
        && show.race.last().is_none_or(|(t, _)| now - *t >= SAMPLE)
    {
        let frame = karts.iter().map(|(entity, pose)| (entity, pose.kart)).collect();
        show.race.push((now, frame));
    }
    if let Some(mut watch) = show.watching {
        let (first, last) = (show.race[0].0, show.race[show.race.len() - 1].0);
        // The keys: another kart, faster or slower, pause.
        let count = show.race[0].1.len().max(1);
        let step = i64::from(keys.just_pressed(KeyCode::ArrowRight)) - i64::from(keys.just_pressed(KeyCode::ArrowLeft));
        if step != 0 {
            watch.follow = (watch.follow as i64 + step).rem_euclid(count as i64) as usize;
            watch.cuts = 0;
            watch.shot = Shot::Onboard { until: watch.shown };
        }
        if keys.just_pressed(KeyCode::ArrowUp) {
            watch.speed = (watch.speed + 1).min(SPEEDS.len() - 1);
        }
        if keys.just_pressed(KeyCode::ArrowDown) {
            watch.speed = watch.speed.saturating_sub(1);
        }
        if keys.just_pressed(KeyCode::Space) {
            watch.paused = !watch.paused;
        }
        if !watch.paused {
            watch.shown += time.delta_secs() * SPEEDS[watch.speed];
        }
        // From the start again at the end.
        if watch.shown > last {
            watch.shown = first;
            watch.cuts = 0;
            watch.shot = Shot::Onboard { until: first };
        }
        let frame = frame_at(&show.race, watch.shown);
        for (entity, then) in &frame {
            if let Ok((_, mut pose)) = karts.get_mut(*entity) {
                pose.kart = *then;
            }
        }
        if let Some(&(entity, kart)) = frame.get(watch.follow % frame.len().max(1)) {
            let along = course.0.track.locate(kart.x, kart.z).along;
            if over(&course, watch.shot, along, watch.shown) {
                watch.cuts += 1;
                watch.shot = next_shot(&course, along, watch.shown, watch.cuts);
            }
            let (from, to) = framing(&course, watch.shot, &kart);
            commands.insert_resource(Viewpoint { from, to });
            show.owns_view = true;
            let name = remotes
                .get(entity)
                .ok()
                .and_then(|remote| view.karts.iter().find(|e| e.id == remote.0))
                .map_or_else(|| format!("kart {}", watch.follow + 1), |e| e.name.clone());
            let lap_time = crate::hud::clock(((watch.shown - first) * 1000.0) as i64);
            let speed = SPEEDS[watch.speed];
            let pace = if watch.paused { "PAUSED".to_owned() } else { format!("x{speed}") };
            view.watching = format!(
                "{name}  ·  {lap_time}  ·  {pace}\nLEFT/RIGHT another kart  ·  UP/DOWN speed  ·  SPACE pause  ·  R back"
            );
        }
        view.replay = true;
        let pace = if watch.paused { 0.0 } else { SPEEDS[watch.speed] };
        commands.insert_resource(crate::effects::Pace(pace));
        show.watching = Some(watch);
        return;
    }
    view.watching.clear();
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
    // The particles slow with the replay (they are made at the karts' real speeds).
    commands.insert_resource(crate::effects::Pace(if view.replay { SLOW } else { 1.0 }));
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
    fn a_whole_race_replay_reads_the_tape_between_its_frames() {
        let (a, b) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        let tape = vec![(0.0, vec![(a, kart(0.0, 0.0)), (b, kart(4.0, 0.0))]), (1.0, vec![(a, kart(10.0, 0.0))])];
        let half = frame_at(&tape, 0.5);
        assert_eq!(half.len(), 1, "the frame after says who is there");
        assert!((half[0].1.x - 5.0).abs() < 1e-5);
        assert_eq!(frame_at(&tape, -3.0)[0].1.x, 0.0, "before it: its first frame");
        assert_eq!(frame_at(&tape, 9.0)[0].1.x, 10.0, "after it: its last");
        assert!(frame_at(&[], 1.0).is_empty());
    }

    #[test]
    fn television_cuts_to_a_camera_ahead_and_never_by_the_track_in_a_tunnel() {
        let def = osage_kart_tracks::by_id("mountain").unwrap();
        let course = Course(osage_world_kart::Circuit::of(&def));
        let track = &course.0.track;
        let length = track.length();
        let mut cuts = 0;
        let mut shot = next_shot(&course, 0.0, 0.0, cuts);
        let mut kinds = [0; 3];
        // A kart round the lap at 20 m/s, a frame each 30th of a second.
        for frame in 0..(length / 20.0 * 30.0) as usize {
            let shown = frame as f32 / 30.0;
            let along = (shown * 20.0).rem_euclid(length);
            if over(&course, shot, along, shown) {
                cuts += 1;
                shot = next_shot(&course, along, shown, cuts);
            }
            match shot {
                Shot::Trackside { along: at, .. } => {
                    kinds[0] += 1;
                    assert!(!in_tunnel(&course, along), "no trackside camera in a tunnel, at {along}");
                    let ahead = (at - along + length / 2.0).rem_euclid(length) - length / 2.0;
                    assert!((-12.5..80.0).contains(&ahead), "the camera near the kart: {ahead} m");
                }
                Shot::Helicopter { .. } => {
                    kinds[1] += 1;
                    assert!(!in_tunnel(&course, along), "no helicopter over a tunnel");
                }
                Shot::Onboard { .. } => kinds[2] += 1,
            }
            let ([x, z], heading) = track.at(along);
            let kart = Kart { x, z, heading, ..Kart::default() };
            let (from, to) = framing(&course, shot, &kart);
            assert!(from.is_finite() && from.distance(to) > 1.0);
        }
        assert!(kinds.iter().all(|n| *n > 0), "every kind of shot: {kinds:?}");
        assert!(cuts > 15, "television cuts: {cuts}");
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
