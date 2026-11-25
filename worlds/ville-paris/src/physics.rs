// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

pub(crate) mod first_contact;

use crate::people::Model;
use crate::replay::{Body, Grid, Replay, Route, Search};
use crate::scene::Sector;
use anyhow::{Result, ensure};
use bevy::prelude::{Quat, Transform, Vec3};
use osage_dynamics::{Capsule, Engine, Part, Pose, Settings, Simulation};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{self, BufRead, Write as IoWrite};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fmt::Write,
    fs,
    path::Path,
    str::FromStr,
    time::Instant,
};
use tempfile::NamedTempFile;

pub const CAPSULE: Capsule = Capsule { radius: 0.28, half_segment: 0.60, mass: 75.0, friction: 0.5 };
const CENTER_HEIGHT: f32 = CAPSULE.half_segment + CAPSULE.radius;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Physics {
    Point,
    Rigid(Engine),
}

impl fmt::Display for Physics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Point => f.write_str("point"),
            Self::Rigid(e) => e.fmt(f),
        }
    }
}

impl FromStr for Physics {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(if s == "point" { Self::Point } else { Self::Rigid(s.parse()?) })
    }
}

pub(crate) fn box_transform(pose: &Pose, half_extents: [f32; 3]) -> Transform {
    Transform::from_translation(pose.center.into())
        .with_rotation(Quat::from_array(pose.rotation_xyzw))
        .with_scale(Vec3::from_array(half_extents) * 2.)
}

pub enum Actor {
    Point(Body),
    Rigid(Box<Simulation>, Vec<Part>),
}

impl Actor {
    pub fn new(choice: Physics, model: &Model, sector: &Sector, r: &Replay, route: &Route) -> Result<Self> {
        ensure!(
            route.ragdoll_activation.is_none()
                || matches!(choice, Physics::Rigid(_)) && !matches!(model, Model::Capsule),
            "ragdoll activation requires an articulated actor"
        );
        match choice {
            Physics::Point => {
                ensure!(route.moving_surface().is_none(), "a moving surface requires rigid physics");
                ensure!(matches!(model, Model::Capsule), "the point control uses a capsule marker");
                Ok(Self::Point(Body::new(route)))
            }
            Physics::Rigid(engine) => {
                let mut center = route.start_m;
                center[1] += CENTER_HEIGHT;
                let cfg = Settings {
                    tick_hz: r.tick_hz,
                    gravity: r.gravity_m_s2,
                    terminal_speed: r.terminal_m_s,
                    capsule: CAPSULE,
                };
                let (mut sim, parts) = if let Some(parts) = model.parts() {
                    let sim = Simulation::from_rig(
                        engine,
                        &sector.colliders,
                        route.bonnet.as_slice(),
                        cfg,
                        &parts,
                        osage_dynamics::RigStart {
                            origin_m: route.start_m,
                            velocity_m_s: route.velocity_m_s,
                            animation: route.ragdoll_activation.as_ref(),
                        },
                    )?;
                    (sim, parts)
                } else {
                    (
                        Simulation::new(
                            engine,
                            &sector.colliders,
                            route.bonnet.as_slice(),
                            cfg,
                            center,
                            route.velocity_m_s,
                        )?,
                        Vec::new(),
                    )
                };
                if let Some(lift) = route.lift {
                    sim.add_lift(lift)?;
                    if let Some(hz) = route.lift_contact_hz {
                        let capsule;
                        let rig = if parts.is_empty() {
                            capsule = [CAPSULE.part()];
                            &capsule[..]
                        } else {
                            &parts
                        };
                        sim.plan_lift_approach(rig, f64::from(route.ticks) / f64::from(r.tick_hz), hz)?;
                        if let Some(travel) = route.lift_response_travel_m {
                            sim.sample_lift_response(rig, travel)?;
                        }
                    }
                }
                Ok(Self::Rigid(Box::new(sim), parts))
            }
        }
    }

    pub fn advance(&mut self, tick: u32, r: &Replay, scene: &Sector, grid: &Grid) -> Result<()> {
        match self {
            Self::Point(body) => {
                body.step(tick, r, &scene.supports, Search::Grid(grid));
                Ok(())
            }
            Self::Rigid(sim, _) => sim.step(),
        }
    }

    fn pose(&self, index: usize) -> Pose {
        match self {
            Self::Point(body) => Pose {
                center: (body.foot + Vec3::Y * CENTER_HEIGHT).to_array(),
                rotation_xyzw: [0., 0., 0., 1.],
                linear_velocity: body.velocity.to_array(),
                angular_velocity: [0.; 3],
            },
            Self::Rigid(sim, _) => sim.part_poses[index],
        }
    }

    pub fn transform(&self, index: usize) -> Transform {
        let p = self.pose(index);
        Transform::from_translation(p.center.into()).with_rotation(Quat::from_array(p.rotation_xyzw))
    }

    pub fn moving_surface_transform(&self, route: &Route) -> Option<Transform> {
        let Self::Rigid(sim, _) = self else {
            return None;
        };
        let half_extents = route.moving_half_extents()?;
        let p = sim.box_poses.first()?;
        Some(box_transform(p, half_extents))
    }
}

/// Measure the same public rigid step used by the viewer, after a chosen replay prefix.
pub(crate) fn time_steps(
    engine: Engine,
    model: &Model,
    scene: &Sector,
    replay: &Replay,
    route_id: Option<&str>,
    warmup_ticks: u32,
    inputs: &Value,
) -> Result<Value> {
    let mut results = Vec::new();
    for route in replay.routes.iter().filter(|r| route_id.is_none_or(|id| r.id == id)) {
        ensure!(warmup_ticks < route.ticks, "timing window is empty on {}", route.id);
        let Actor::Rigid(mut sim, _) = Actor::new(Physics::Rigid(engine), model, scene, replay, route)? else {
            unreachable!()
        };
        for _ in 0..warmup_ticks {
            sim.step()?;
        }
        let snapshot = |s: &Simulation| json!({"parts":s.part_poses, "boxes":s.box_poses, "contacts":s.contacts});
        let initial = snapshot(&sim);
        let mut samples = Vec::new();
        let mut motionless_ticks = 0u32;
        let mut contact_rows = 0u64;
        for _ in warmup_ticks..route.ticks {
            let start = Instant::now();
            sim.step()?;
            samples.push(start.elapsed().as_nanos() as u64);
            let motionless = sim
                .part_poses
                .iter()
                .chain(&sim.box_poses)
                .all(|body| body.linear_velocity == [0.; 3] && body.angular_velocity == [0.; 3]);
            motionless_ticks += u32::from(motionless);
            contact_rows += sim.contacts.len() as u64;
        }
        let total: u64 = samples.iter().sum();
        results.push(json!({
            "id":route.id, "start_tick":warmup_ticks, "end_tick":route.ticks,
            "substeps_per_tick":sim.substeps_per_tick(), "step_elapsed_ns":crate::bench::distribution(samples),
            "sum_step_elapsed_ns":total, "motionless_ticks":motionless_ticks, "contact_rows":contact_rows,
            "initial":initial, "final":snapshot(&sim)
        }));
    }
    ensure!(!results.is_empty(), "no replay route matches the timing request");
    Ok(json!({"format":"osage/physics-timing/1", "engine":engine, "actor":model.to_string(),
        "tick_hz":replay.tick_hz, "inputs":inputs, "routes":results}))
}

struct Recording {
    support_footprints: BTreeMap<String, osage_dynamics::support_footprint::SupportFootprint>,
    summary: Value,
    costs: Value,
}

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
enum TraceFormat {
    #[serde(rename = "osage/dynamics-trace/1")]
    Basic,
    #[serde(rename = "osage/dynamics-trace/2")]
    Owners,
    #[serde(rename = "osage/dynamics-trace/3")]
    Triangles,
}

/// Distance between the two reconstructions of each joint anchor. Both equal the bind anchor at tick zero.
fn max_anchor_gap(parts: &[Part], poses: &[Pose]) -> f32 {
    let world_anchor = |i: usize, anchor: Vec3| {
        let p = &parts[i];
        let joint_local = p.rotation.inverse() * (anchor - p.center);
        Vec3::from_array(poses[i].center) + Quat::from_array(poses[i].rotation_xyzw) * joint_local
    };
    parts
        .iter()
        .enumerate()
        .filter_map(|(i, part)| {
            let parent = part.parent?;
            Some(world_anchor(i, part.anchor).distance(world_anchor(parent, part.anchor)))
        })
        .fold(0., f32::max)
}

fn part_table(model: &Model) -> Value {
    if matches!(model, Model::Capsule) {
        return json!([{"name":"capsule","parent":null,"radius_m":CAPSULE.radius,"half_segment_m":CAPSULE.half_segment,"mass_kg":CAPSULE.mass}]);
    }
    json!(
        model
            .parts()
            .unwrap()
            .into_iter()
            .map(|p| {
                let mut entry = json!({
                    "name":p.name, "parent":p.parent, "center_m":p.center.to_array(),
                    "rotation_xyzw":p.rotation.to_array(), "radius_m":p.radius, "half_segment_m":p.half_segment,
                    "mass_kg":p.mass, "anchor_m":p.anchor.to_array(),"limits_deg":p.limits_deg
                });
                if !p.swing_envelope.is_independent() {
                    entry["swing_envelope"] = json!(p.swing_envelope);
                }
                if let Some(inertia) = p.inertia {
                    entry["inertia_kg_m2"] = json!(inertia.to_array());
                    entry["joint_frame_xyzw"] = json!(p.joint_rotation.to_array());
                    entry["material"] = json!(p.material);
                }
                entry
            })
            .collect::<Vec<_>>()
    )
}

fn record(
    engine: Engine,
    model: &Model,
    scene: &Sector,
    r: &Replay,
    inputs: &Value,
    format: TraceFormat,
    mut row: impl FnMut(&[u8]) -> Result<()>,
) -> Result<Recording> {
    let mut digest = Sha256::new();
    let mut emit = |line: &str| -> Result<()> {
        digest.update(line.as_bytes());
        row(line.as_bytes())
    };
    emit("route,tick,cx,cy,cz,qx,qy,qz,qw,vx,vy,vz,wx,wy,wz,contacts_json,parts_json,boxes_json\n")?;
    let mut trace = String::new();
    let mut substeps = 1;
    let mut outcomes = Vec::new();
    let mut support_footprints = BTreeMap::new();
    let mut times = Vec::new();
    let mut builds = Vec::new();
    let grid = Grid::new(&scene.supports);
    let mut rows = 0;
    let mut surfaces: Vec<_> = scene.colliders.iter().map(|s| s.id.as_str()).collect();
    for name in ["bonnet", "lift"] {
        if r.routes.iter().any(|route| route.moving_surface() == Some(name)) {
            surfaces.push(name);
        }
    }
    for route in &r.routes {
        let start = Instant::now();
        let mut actor = Actor::new(Physics::Rigid(engine), model, scene, r, route)?;
        builds.push(start.elapsed().as_nanos() as u64);
        let mut first_seen = BTreeSet::new();
        let surface_name = |i| route.surface_name(&scene.colliders, i).expect("contact with an installed surface");
        let moving_index = route.moving_surface().and_then(|name| surfaces.iter().position(|&id| id == name));
        let remap = moving_index.filter(|&i| i != scene.colliders.len());
        let mut impacts = Vec::new();
        let mut first_swept_impact_s = None;
        let mut max_joint_mm = 0.;
        let mut final_joint_mm = 0.;
        for tick in 0..=route.ticks {
            if tick > 0 {
                let start = Instant::now();
                actor.advance(tick, r, scene, &grid)?;
                times.push(start.elapsed().as_nanos() as u64);
            }
            let Actor::Rigid(sim, parts) = &actor else { unreachable!() };
            substeps = sim.substeps_per_tick();
            if let Some(offset) = sim.swept_impact_offset_s {
                first_swept_impact_s
                    .get_or_insert(f64::from(tick.saturating_sub(1)) / f64::from(r.tick_hz) + f64::from(offset));
            }
            let p = sim.pose;
            trace.clear();
            write!(trace, "{},{}", route.id, tick)?;
            for n in p.center.into_iter().chain(p.rotation_xyzw).chain(p.linear_velocity).chain(p.angular_velocity) {
                write!(trace, ",{n}")?;
            }
            final_joint_mm = (1000. * max_anchor_gap(parts, &sim.part_poses)).round();
            max_joint_mm = f32::max(max_joint_mm, final_joint_mm);
            let contacts = &sim.contacts;
            let encoded = if format == TraceFormat::Triangles && remap.is_none() {
                serde_json::to_string(contacts)?
            } else {
                let mapped: Vec<_> = contacts
                    .iter()
                    .copied()
                    .map(|mut c| {
                        if c.surface == scene.colliders.len()
                            && let Some(index) = remap
                        {
                            c.surface = index;
                        }
                        if format != TraceFormat::Triangles {
                            c.surface_triangle = None;
                        }
                        c
                    })
                    .collect();
                serde_json::to_string(&mapped)?
            }
            .replace('"', "\"\"");
            let limbs = serde_json::to_string(&sim.part_poses[1..])?.replace('"', "\"\"");
            let boxes = serde_json::to_string(&sim.box_poses)?.replace('"', "\"\"");
            writeln!(trace, ",\"{encoded}\",\"{limbs}\",\"{boxes}\"")?;
            emit(&trace)?;
            rows += 1;
            let mut starting = BTreeMap::<usize, (u8, BTreeSet<usize>)>::new();
            for contact in contacts.iter().filter(|c| !first_seen.contains(&c.surface)) {
                let (substep, parts) =
                    starting.entry(contact.surface).or_insert_with(|| (contact.substep, BTreeSet::new()));
                if *substep == contact.substep {
                    parts.insert(contact.part);
                }
            }
            let mut hits: Vec<_> = starting.into_iter().collect();
            hits.sort_by_key(|(surface, (substep, _))| (*substep, *surface));
            for (surface, (substep, indices)) in hits {
                first_seen.insert(surface);
                let names: Vec<_> =
                    indices.into_iter().map(|i| parts.get(i).map_or("capsule", |p| p.name.as_str())).collect();
                let id = surface_name(surface);
                let mut impact = json!({"tick":tick,"substep":substep,"surface":id,"parts":names});
                if format != TraceFormat::Basic
                    && let Some(path) = scene.object.as_ref().and_then(|obj| obj.collision_path(id))
                {
                    impact["nodePath"] = json!(path);
                }
                if format == TraceFormat::Triangles {
                    let faces: BTreeSet<_> = contacts
                        .iter()
                        .filter(|c| c.surface == surface && c.substep == substep)
                        .filter_map(|c| c.surface_triangle)
                        .collect();
                    if !faces.is_empty() {
                        impact["source_triangles"] = json!(faces);
                    }
                }
                impacts.push(impact);
            }
        }
        let Actor::Rigid(sim, parts) = &actor else { unreachable!() };
        // The standalone capsule has one simulated body and no articulation table.
        let capsule;
        let bodies = if parts.is_empty() {
            capsule = [CAPSULE.part()];
            &capsule[..]
        } else {
            parts.as_slice()
        };
        let footprint = osage_dynamics::support_footprint::project(
            bodies,
            &sim.part_poses,
            &sim.contacts,
            substeps,
            0.001,
            0.1,
            |id| id.checked_sub(scene.colliders.len()).and_then(|i| sim.box_poses.get(i)).copied(),
        )?;
        support_footprints.insert(route.id.clone(), footprint);
        let reached = first_seen.iter().any(|&i| surface_name(i) == route.expected_support);
        let mut outcome = json!({"id":route.id,"expected_support":route.expected_support,"bonnet":route.bonnet,"target_contacted":reached,
            "first_contacts":impacts,"end_root_m":sim.pose.center,"end_box_m":sim.box_poses.first().map(|p|p.center),"max_anchor_gap_mm":max_joint_mm as u32,"final_anchor_gap_mm":final_joint_mm as u32});
        if let Some(lift) = route.lift {
            outcome["lift"] = json!(lift);
        }
        if let Some(time) = first_swept_impact_s {
            outcome["first_swept_impact_s"] = json!(time);
        }
        if let Some(report) = sim.lift_approach() {
            outcome["lift_approach"] = serde_json::to_value(report)?;
        }
        outcomes.push(outcome);
    }
    let mut summary = json!({"format":format,"engine":engine,"actor":model.to_string(),"inputs":inputs,
        "tick_hz":r.tick_hz,"substeps_per_tick":substeps,"trace_rows":rows,"trace_sha256":crate::bench::hex(&digest.finalize()),"outcomes":outcomes,
        "surfaces":surfaces,
        "parts":part_table(model),"friction":CAPSULE.friction,"restitution":0,
        "lock_sha256":crate::bench::sha(include_bytes!("../../../Cargo.lock"))});
    if matches!(model, Model::Imported(_)) {
        // Per-body coefficients remain in the part table when the actor has different surfaces.
        let entries = summary["parts"].as_array().unwrap();
        let common = ["friction", "restitution"].map(|key| {
            let first = &entries[0]["material"][key];
            entries.iter().all(|v| v["material"][key] == *first).then(|| first.clone())
        });
        for (key, value) in ["friction", "restitution"].into_iter().zip(common) {
            match value {
                Some(v) if summary[key].as_f64() != v.as_f64() => summary[key] = v,
                None => {
                    summary.as_object_mut().unwrap().remove(key);
                }
                _ => {}
            }
        }
    }
    if scene.object.is_some() {
        summary["surface_materials"] =
            serde_json::to_value(scene.colliders.iter().map(|s| (&s.id, s.material)).collect::<BTreeMap<_, _>>())?;
    }
    let costs = json!({"step_ns":crate::bench::distribution(times),"world_build_ns":builds});
    Ok(Recording { summary, costs, support_footprints })
}

pub fn bench(dir: &Path, engine: Engine, model: &Model, scene: &Sector, r: &Replay, inputs: &Value) -> Result<()> {
    fs::create_dir_all(dir)?;
    let publication =
        fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(dir.join(".dynamics.lock"))?;
    let mut csv = NamedTempFile::new_in(dir)?;
    let run = {
        let mut writer = io::BufWriter::new(&mut csv);
        let run = record(engine, model, scene, r, inputs, TraceFormat::Triangles, |row| Ok(writer.write_all(row)?))?;
        writer.flush()?;
        run
    };
    let frames = run.summary["trace_rows"].as_u64().unwrap();
    let receipt = json!({"run":run.summary,"timings":run.costs,"support_footprints":run.support_footprints});
    let mut metadata = NamedTempFile::new_in(dir)?;
    serde_json::to_writer_pretty(&mut metadata, &receipt)?;
    // Publish the two files as one run for concurrent writers and verifying readers.
    publication.lock()?;
    csv.persist(dir.join("dynamics.csv"))?;
    metadata.persist(dir.join("dynamics.json"))?;
    println!(
        "{engine}: {frames} states, {} ns per step at p95\n{}",
        receipt["timings"]["step_ns"]["p95"], receipt["run"]["outcomes"]
    );
    Ok(())
}

pub fn verify(dir: &Path, engine: Engine, model: &Model, scene: &Sector, r: &Replay, inputs: &Value) -> Result<()> {
    let publication = match fs::File::open(dir.join(".dynamics.lock")) {
        Ok(file) => Some(file),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(file) = &publication {
        file.lock_shared()?;
    }
    let receipt: Value = serde_json::from_slice(&fs::read(dir.join("dynamics.json"))?)?;
    ensure!(
        receipt["run"]["inputs"] == *inputs
            && receipt["run"]["engine"] == engine.to_string()
            && receipt["run"]["actor"] == model.to_string(),
        "scene, replay, engine or actor mismatch"
    );
    let mut csv = io::BufReader::new(fs::File::open(dir.join("dynamics.csv"))?);
    let mut original = Vec::new();
    let format = serde_json::from_value(receipt["run"]["format"].clone())?;
    let run = record(engine, model, scene, r, inputs, format, |row| {
        original.clear();
        csv.read_until(b'\n', &mut original)?;
        ensure!(original == row, "different poses or contacts on replay");
        Ok(())
    })?;
    ensure!(
        csv.fill_buf()?.is_empty()
            && receipt["run"] == serde_json::from_slice::<Value>(&serde_json::to_vec(&run.summary)?)?,
        "different poses or contacts on replay"
    );
    println!("Verified {engine} poses and contacts for {} states", run.summary["trace_rows"]);
    Ok(())
}

/// Predict impact from the exact piecewise ballistic path. Dynamic targets are solved by the rigid replay.
pub fn predict(scene: &Sector, replay: &Replay, route_id: Option<&str>, inputs: &Value) -> Result<Value> {
    use osage_dynamics::ballistic::{BallisticScene, Flight};
    let geometry = BallisticScene::new(&scene.colliders)?;
    let mut cases = Vec::new();
    for route in replay.routes.iter().filter(|r| route_id.is_none_or(|id| id == r.id)) {
        ensure!(route.moving_surface().is_none(), "prediction requires stationary scene geometry");
        let mut center = route.start_m.map(f64::from);
        center[1] += f64::from(CAPSULE.half_segment) + f64::from(CAPSULE.radius);
        let hit = geometry.first_impact(Flight {
            center: center.into(),
            velocity: route.velocity_m_s.map(f64::from).into(),
            half_segment: CAPSULE.half_segment.into(),
            radius: CAPSULE.radius.into(),
            gravity: replay.gravity_m_s2.into(),
            terminal_speed: replay.terminal_m_s.into(),
            duration: f64::from(route.ticks) / f64::from(replay.tick_hz),
        })?;
        let hit = hit.map(|h| {
            let id = &scene.colliders[h.surface].id;
            json!({"seconds":h.seconds,"center_m":h.center,"point_m":h.point,"normal":h.normal,"surface":id,
                "triangle":h.triangle,"nodePath":scene.object.as_ref().and_then(|o| o.collision_path(id))})
        });
        cases.push(json!({"id":route.id,"hit":hit}));
    }
    ensure!(!cases.is_empty(), "unknown prediction route");
    Ok(json!({"format":"osage/flight-impact/1","inputs":inputs,"cases":cases}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::Clock;
    use osage_dynamics::Contact;

    #[derive(PartialEq)]
    struct Frame {
        limbs: Vec<Pose>,
        boxes: Vec<Pose>,
        contacts: Vec<Contact>,
    }

    fn replay_actor(
        engine: Engine,
        model: &Model,
        scene: &Sector,
        r: &Replay,
        route: &Route,
        display_hz: u64,
    ) -> Vec<Frame> {
        let grid = Grid::new(&scene.supports);
        let mut actor = Actor::new(Physics::Rigid(engine), model, scene, r, route).unwrap();
        let mut frames = Vec::new();
        let mut clock = Clock::default();
        let mut frame = 0;
        while frames.len() < route.ticks as usize {
            let nanos = |f: u64| (f as u128) * 1_000_000_000 / u128::from(display_hz);
            let due = clock.advance(nanos(frame + 1) - nanos(frame), r.tick_hz);
            frame += 1;
            for _ in 0..due.min(route.ticks - frames.len() as u32) {
                actor.advance(frames.len() as u32 + 1, r, scene, &grid).unwrap();
                let Actor::Rigid(sim, _) = &actor else { unreachable!() };
                frames.push(Frame {
                    limbs: sim.part_poses.clone(),
                    boxes: sim.box_poses.clone(),
                    contacts: sim.contacts.clone(),
                });
            }
        }
        frames
    }

    #[test]
    fn terminal_speed_drop_stays_near_the_deck_after_impact() {
        let scene = Sector::read(crate::scene::DEFAULT).unwrap();
        let r = Replay::read(crate::replay::DEFAULT, scene.colliders.iter().map(|s| s.id.as_str())).unwrap();
        let route = &r.routes[0];
        let floor = scene.supports.iter().find(|s| s.id == route.expected_support).unwrap().y;
        for engine in [Engine::Rapier, Engine::Jolt] {
            let frames = replay_actor(engine, &Model::Human, &scene, &r, route, 60);
            let hit = frames.iter().position(|f| !f.contacts.is_empty()).unwrap();
            assert!(
                frames[hit..].iter().all(|f| f.limbs[0].center[1] < floor + 2.),
                "{engine} launched the pelvis more than 2 m above the deck"
            );
            assert!(frames.last().unwrap().limbs[0].center[1] < floor + 0.55, "{engine} did not settle on the deck");
        }
    }

    #[test]
    fn standing_lift_replays_the_same_contacts_and_poses_at_three_display_rates() {
        let scene = Sector::read(crate::scene::DEFAULT).unwrap();
        let r = Replay::read(include_bytes!("../fixtures/lift.json"), scene.colliders.iter().map(|s| s.id.as_str()))
            .unwrap();
        let route = &r.routes[0];
        for engine in [Engine::Rapier, Engine::Jolt] {
            let frames = |hz| replay_actor(engine, &Model::Capsule, &scene, &r, route, hz);
            let control = frames(60);
            assert!(control.iter().all(|f| f.contacts.iter().any(|c| c.surface == scene.colliders.len())));
            for hz in [30, 144] {
                assert!(control == frames(hz), "{engine} differs at {hz} Hz");
            }
        }
    }

    #[test]
    fn offset_landings_keep_both_feet_within_the_bonnet_contact_margin() {
        let scene = Sector::read(crate::scene::DEFAULT).unwrap();
        let mut replay =
            Replay::read(include_bytes!("../fixtures/bonnet.json"), scene.colliders.iter().map(|s| s.id.as_str()))
                .unwrap();
        for (start, velocity) in
            [([0.125, 10.425, 6.75], [0.65, -1.825, 0.]), ([0.125, 10.475, 6.79], [0.71, -1.825, 0.])]
        {
            replay.routes[0].start_m = start;
            replay.routes[0].velocity_m_s = velocity;
            let route = &replay.routes[0];
            let half = Vec3::from_array(route.bonnet.unwrap().half_extents_m);
            let Actor::Rigid(mut sim, rig) =
                Actor::new(Physics::Rigid(Engine::Rapier), &Model::Human, &scene, &replay, route).unwrap()
            else {
                unreachable!()
            };
            let feet = ["foot_l", "foot_r"].map(|name| rig.iter().position(|p| p.name == name).unwrap());
            let mut samples = 0;
            let mut hit = false;
            let mut peak = 0_f32;
            for _ in 0..replay.tick_hz {
                sim.step_with_substeps(|state| {
                    hit |= state.contacts.iter().any(|c| c.surface == scene.colliders.len());
                    if !hit || samples == 7 {
                        return;
                    }
                    samples += 1;
                    let plate = state.boxes[0];
                    let inverse = Quat::from_array(plate.rotation_xyzw).inverse();
                    for foot in feet {
                        let part = &rig[foot];
                        let pose = state.parts[foot];
                        let center = inverse * (Vec3::from(pose.center) - Vec3::from(plate.center));
                        let axis = inverse * (Quat::from_array(pose.rotation_xyzw) * Vec3::Y);
                        let tip = center - axis.y.signum() * part.half_segment * axis;
                        assert!(tip.x.abs() < half.x && tip.z.abs() < half.z, "foot left the measured top face");
                        peak = peak.max(half.y - tip.y + part.radius);
                    }
                })
                .unwrap();
                if samples == 7 {
                    break;
                }
            }
            assert_eq!(samples, 7, "missing first contact or its 25 ms continuation");
            assert!(peak > 0. && peak < 0.02, "offset {start:?}: {peak} m of foot penetration");

            // Continue through the landing. A second burst of refinement during
            // rolling previously left a foot rotating on the parked support.
            let mut still_ticks = 0;
            for _ in 0..15 * replay.tick_hz {
                sim.step().unwrap();
                let still = sim
                    .part_poses
                    .iter()
                    .chain(&sim.box_poses)
                    .all(|pose| pose.linear_velocity == [0.; 3] && pose.angular_velocity == [0.; 3]);
                still_ticks = if still { still_ticks + 1 } else { 0 };
            }
            assert!(still_ticks >= replay.tick_hz / 2, "offset {start:?}: rig or bonnet remained active");
        }
    }

    #[test]
    fn moving_bonnet_hits_the_rig_and_replays_at_three_display_rates() {
        let scene = Sector::read(crate::scene::DEFAULT).unwrap();
        let r = Replay::read(include_bytes!("../fixtures/bonnet.json"), scene.colliders.iter().map(|s| s.id.as_str()))
            .unwrap();
        let route = &r.routes[1];
        let parts = crate::people::human();
        assert_eq!(parts.len(), 15);
        assert!((parts.iter().map(|p| p.mass).sum::<f32>() - 75.).abs() < 0.001);
        for engine in [Engine::Rapier, Engine::Jolt] {
            let control = replay_actor(engine, &Model::Human, &scene, &r, route, 60);
            let first = control.iter().find(|f| !f.contacts.is_empty()).unwrap();
            assert!(first.contacts.iter().any(|c| c.surface == scene.colliders.len()), "{engine} missed the bonnet");
            assert!(first.boxes[0].linear_velocity[0] > 0.1, "{engine} did not hit a moving box");
            let final_state = control.last().unwrap();
            let gap = max_anchor_gap(&parts, &final_state.limbs);
            assert!(gap < 0.02, "{engine} final joint gap was {gap} m");
            for hz in [30, 144] {
                assert!(
                    control == replay_actor(engine, &Model::Human, &scene, &r, route, hz),
                    "{engine} differs at {hz} Hz"
                );
            }
        }
    }
}
