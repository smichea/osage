// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

mod channels;
mod coarse;
mod detail;
mod directory;
mod impact;
mod link;
mod live;
mod models;
mod observers;
mod planning;
mod reader;
mod solver;
mod source;
mod support;
mod tiles;
mod visibility;

use crate::{
    bench::sha,
    people::Model,
    physics::Physics,
    replay::{self, Replay},
    view,
};
use anyhow::{Context, Result, bail, ensure};
use reader::{Change, Kind, Lane};
use serde_json::{Value, json};
use solver::World;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use tiles::Index;

const HELP: &str = "ville-paris --paging --pages DIR_OR_HTTP_URL [--index-pin SHA256:BYTES_OR_UPDATE_REFERENCE] [--wall-clock] [--no-visuals] [--trace-body] [--support-footprint MAX_GAP_M] [--actor capsule|human | --actor-rig FILE.glb] [--content-cache DIR] [--image-decode worker|inline] [--model-decode worker|inline] [--model-fingerprints] --out RECEIPT.json [--route rail|road|roof] [--replay FILE.json] [--physics-radius M] [--physics-plan trajectory|radius] [--physics-observers FILE] [--visual-radius M] [--delay-ms MS] [--mbps-per-kind RATE] [--seed N --jitter-ms MS] [--outage START_TICK,RESUME_TICK] [--late-collision proxy|unprotected] [--all-resident] [--watch X,Z] [--hold visual|physics --hold-ticks N] [--acoustic-ray X,Y,Z X,Y,Z]... [--collision-ray X,Y,Z X,Y,Z]... [--collision-ray-actor] [--impact-wav FILE.wav --listening speakers|headphones --direct-only --acoustic-plan forecast|contact --acoustic-delay-ms MS] [--lod-publication changed|scan] [--gpu-trace FILE.json --gpu-stride N --gpu-indices compact|u32 --gpu-groups auto|off|on --gpu-scene-index incremental|full --gpu-bindings stable|rebuild --gpu-owner SOURCE_NODE --gpu-withhold-owner SOURCE_NODE] [--tick N --shot FILE.png]";
#[derive(Default, serde::Serialize)]
struct PassTime {
    ticks: u32,
    total: u64,
    max: u64,
}
impl PassTime {
    fn finish(&mut self, start: Option<std::time::Instant>) {
        if let Some(start) = start {
            let ns = start.elapsed().as_nanos() as u64;
            self.ticks += 1;
            self.total += ns;
            self.max = self.max.max(ns);
        }
    }
}

pub struct Options {
    source: source::Source,
    shape_link: bool,
    physics_radius: f32,
    predictive_physics: bool,
    visual_radius: f32,
    delay_ms: u32,
    mbps: f64,
    link: link::Link,
    all: bool,
    hold: Option<Kind>,
    watched: Option<usize>,
    hold_ticks: u32,
    protect: bool,
}

const LEGACY_VISUAL_RADIUS: f32 = 48.;

impl Default for Options {
    fn default() -> Self {
        Self {
            source: source::Source::Directory(PathBuf::new()),
            shape_link: false,
            physics_radius: 64.,
            predictive_physics: true,
            visual_radius: 96.,
            delay_ms: 400,
            mbps: 20.,
            link: Default::default(),
            all: false,
            hold: None,
            watched: None,
            hold_ticks: 0,
            protect: true,
        }
    }
}

pub fn cli(mut args: impl Iterator<Item = String>) -> Result<()> {
    let mut o = Options::default();
    let (mut dir, mut out, mut shot, mut replay_path) = (None, None, None, None);
    let mut cache = None;
    let mut wall_clock = false;
    let mut trace_body = false;
    let mut visuals = true;
    let mut index_pin = None;
    let mut selected_actor = None;
    let mut actor_rig = None;
    let mut model_fingerprints = false;
    let mut decode = source::WorkerDecode::default();
    let mut route = "rail".to_string();
    let mut gpu_trace = None;
    let mut acoustic_rays = Vec::new();
    let mut collision_rays = Vec::new();
    let mut collision_ray_actor = false;
    let mut footprint_gap: Option<f32> = None;
    let mut impact_wav = None;
    let mut playback = crate::impact::Playback { listening: crate::impact::Listening::Speakers, direct_only: false };
    let mut acoustic_forecast = true;
    let mut acoustic_delay_ms: Option<f64> = None;
    let mut group_culling = visibility::Mode::Auto;
    let mut full_index = false;
    let mut reuse_bindings = true;
    let mut scan_lods = false;
    let mut group_watch: Option<usize> = None;
    let mut group_withhold: Option<usize> = None;
    let mut gpu_stride = 1u32;
    let mut compact_indices = true;
    let mut custom_visual_radius = false;
    let mut custom_physics_radius = false;
    let mut physics_plan = None;
    let mut observer_path = None;
    let mut watch: Option<[i32; 2]> = None;
    let mut tick = None;
    while let Some(a) = args.next() {
        let mut val = || args.next().with_context(|| format!("missing value for {a}"));
        o.shape_link |= matches!(a.as_str(), "--delay-ms" | "--mbps-per-kind" | "--jitter-ms" | "--outage");
        match &*a {
            "--help" | "-h" => {
                let default = Options::default();
                println!(
                    "{HELP}\nDefaults: physics {} m, visual {} m (legacy directories {LEGACY_VISUAL_RADIUS} m), latency {} ms, {} Mbit/s per kind, protected support. The default collision planner follows the actor trajectory. An explicit physics radius alone selects the fixed-radius control.",
                    default.physics_radius, default.visual_radius, default.delay_ms, default.mbps
                );
                return Ok(());
            }
            "--gpu-scene-index" => {
                full_index = match val()?.as_str() {
                    "incremental" => false,
                    "full" => true,
                    _ => anyhow::bail!("--gpu-scene-index needs incremental or full"),
                };
            }
            "--gpu-bindings" => {
                reuse_bindings = match val()?.as_str() {
                    "stable" => true,
                    "rebuild" => false,
                    _ => bail!("--gpu-bindings needs stable or rebuild"),
                };
            }
            "--gpu-groups" => {
                group_culling = match val()?.as_str() {
                    "auto" => visibility::Mode::Auto,
                    "on" => visibility::Mode::On,
                    "off" => visibility::Mode::Off,
                    _ => anyhow::bail!("--gpu-groups needs auto, on or off"),
                }
            }
            "--impact-wav" => impact_wav = Some(PathBuf::from(val()?)),
            "--direct-only" => playback.direct_only = true,
            "--listening" => {
                playback.listening = match val()?.as_str() {
                    "speakers" => crate::impact::Listening::Speakers,
                    "headphones" => crate::impact::Listening::Headphones,
                    other => bail!("unknown listening mode {other}, expected speakers or headphones"),
                };
            }
            "--acoustic-plan" => {
                acoustic_forecast = match val()?.as_str() {
                    "forecast" => true,
                    "contact" => false,
                    _ => bail!("--acoustic-plan expects forecast or contact"),
                };
            }
            "--acoustic-delay-ms" => {
                let delay: f64 = val()?.parse()?;
                ensure!(delay.is_finite() && delay >= 0., "invalid acoustic delay");
                acoustic_delay_ms = Some(delay);
            }
            "--support-footprint" => {
                let gap: f32 = val()?.parse()?;
                anyhow::ensure!(gap.is_finite() && gap >= 0., "invalid support footprint gap");
                footprint_gap = Some(gap);
            }
            "--collision-ray-actor" => collision_ray_actor = true,
            "--collision-ray" => collision_rays.push((crate::point(val()?)?, crate::point(val()?)?)),
            "--acoustic-ray" => acoustic_rays.push((crate::point(val()?)?, crate::point(val()?)?)),
            "--lod-publication" => {
                scan_lods = match val()?.as_str() {
                    "changed" => false,
                    "scan" => true,
                    _ => bail!("unknown LOD publication mode"),
                }
            }
            "--gpu-owner" => group_watch = Some(val()?.parse()?),
            "--gpu-withhold-owner" => group_withhold = Some(val()?.parse()?),
            "--gpu-indices" => {
                compact_indices = match val()?.as_str() {
                    "compact" => true,
                    "u32" => false,
                    _ => anyhow::bail!("GPU indices must be compact or u32"),
                }
            }
            "--gpu-trace" => gpu_trace = Some(PathBuf::from(val()?)),
            "--gpu-stride" => gpu_stride = val()?.parse()?,
            "--pages" => dir = Some(val()?),
            "--wall-clock" => wall_clock = true,
            "--no-visuals" => visuals = false,
            "--index-pin" => index_pin = Some(val()?.parse::<directory::Pin>()?),
            "--trace-body" => trace_body = true,
            "--actor" => selected_actor = Some(val()?.parse::<Model>()?),
            "--actor-rig" => actor_rig = Some(fs::read(val()?)?),
            "--model-fingerprints" => model_fingerprints = true,
            "--content-cache" => cache = Some(PathBuf::from(val()?)),
            "--image-decode" | "--model-decode" => {
                let enabled = match val()?.as_str() {
                    "worker" => true,
                    "inline" => false,
                    _ => bail!("{a} expects worker or inline"),
                };
                if a == "--model-decode" {
                    decode.models = enabled;
                } else {
                    decode.images = enabled;
                }
            }
            "--out" => out = Some(PathBuf::from(val()?)),
            "--route" => route = val()?,
            "--replay" => replay_path = Some(PathBuf::from(val()?)),
            "--physics-radius" => {
                o.physics_radius = val()?.parse()?;
                custom_physics_radius = true;
            }
            "--physics-observers" => observer_path = Some(PathBuf::from(val()?)),
            "--physics-plan" => {
                physics_plan = Some(match val()?.as_str() {
                    "trajectory" => true,
                    "radius" => false,
                    _ => bail!("--physics-plan expects trajectory or radius"),
                })
            }
            "--visual-radius" => {
                o.visual_radius = val()?.parse()?;
                custom_visual_radius = true;
            }
            "--delay-ms" => o.delay_ms = val()?.parse()?,
            "--seed" => o.link.seed = val()?.parse()?,
            "--jitter-ms" => o.link.jitter_ms = val()?.parse()?,
            "--outage" => {
                let range = val()?;
                let (start, end) = range.split_once(',').context("expected disconnection and resumption ticks")?;
                let [a, b] = [start.parse()?, end.parse()?];
                ensure!(a < b, "outage must end after it starts");
                o.link.outage = Some([a, b]);
            }
            "--mbps-per-kind" => o.mbps = val()?.parse()?,
            "--all-resident" => o.all = true,
            "--late-collision" => {
                o.protect = match &*val()? {
                    "proxy" => true,
                    "unprotected" => false,
                    _ => bail!("expected proxy or unprotected"),
                }
            }
            "--watch" => {
                let v = val()?;
                let (x, z) = v.split_once(',').context("expected X,Z")?;
                watch = Some([x.parse()?, z.parse()?]);
            }
            "--hold" => {
                o.hold = Some(match &*val()? {
                    "visual" => Kind::Visual,
                    "physics" => Kind::Physics,
                    _ => bail!("expected visual or physics"),
                });
            }
            "--hold-ticks" => o.hold_ticks = val()?.parse()?,
            "--tick" => tick = Some(val()?.parse::<u32>()?),
            "--shot" => shot = Some(PathBuf::from(val()?)),
            _ => bail!("unknown option {a}\n{HELP}"),
        }
    }
    ensure!(
        [o.physics_radius, o.visual_radius].iter().all(|v| v.is_finite() && *v >= 0.)
            && o.mbps.is_finite()
            && o.mbps > 0.,
        "invalid read profile"
    );
    ensure!(
        o.hold.is_none() == (o.hold_ticks == 0) && (o.hold.is_none() || (watch.is_some() && !o.all)),
        "a hold requires a watched tile and a positive tick count"
    );
    ensure!(
        visuals || (shot.is_none() && gpu_trace.is_none() && !model_fingerprints && o.hold != Some(Kind::Visual)),
        "visual output and visual holds require visual streaming"
    );
    o.predictive_physics = physics_plan.unwrap_or(!custom_physics_radius);
    o.source = source::Source::open(dir.context(HELP)?, cache, decode, wall_clock)?;
    let dir = &o.source;
    let out = out.context(HELP)?;
    ensure!(impact_wav.as_ref() != Some(&out), "the WAV and paging receipt need different paths");
    let mut idx = Index::load(dir, index_pin.as_ref(), visuals)?;
    if !idx.demand() && !custom_visual_radius {
        o.visual_radius = LEGACY_VISUAL_RADIUS;
    }
    let replay_bytes = if let Some(path) = replay_path { fs::read(path)? } else { replay::DEFAULT.to_vec() };
    let replay = Replay::read(
        &replay_bytes,
        idx.tiles
            .iter()
            .filter_map(|t| t.physics.as_ref())
            .flat_map(|l| l.ids.iter().map(String::as_str))
            .chain(idx.tiles.iter().all(|t| t.physics.is_none()).then_some("none")),
    )?;
    let extra_observers = observer_path.map(|p| observers::Tracks::read(&fs::read(p)?)).transpose()?;
    ensure!(selected_actor.is_none() || actor_rig.is_none(), "choose either --actor or --actor-rig");
    let actor_sha = actor_rig.as_ref().map(|b| sha(b));
    let model = match actor_rig {
        Some(bytes) => Model::read_rig(&bytes)?,
        None => selected_actor.unwrap_or(Model::Capsule),
    };
    let articulated = !matches!(model, Model::Capsule);
    let route_at = replay.routes.iter().position(|r| r.id == route).context("unknown route")?;
    let r = &replay.routes[route_at];
    let moving = r.moving_surface().is_some();
    let final_tick = tick.unwrap_or(r.ticks);
    ensure!(final_tick <= r.ticks, "tick outside route");
    let target =
        watch.map(|pos| idx.tiles.iter().position(|t| [t.x, t.z] == pos).context("unknown watch tile")).transpose()?;
    o.watched = target;
    let embedded_parent = idx.inline_parent.is_some();
    let (parent, mut library) = if visuals {
        (Some(tiles::Visual::load_parent(&mut idx, dir)?), models::Library::load(&mut idx, dir)?)
    } else {
        (None, models::Library::default())
    };
    let index_bytes = idx.encoded_bytes as u64;
    ensure!(gpu_stride > 0, "GPU frame stride must be positive");
    let record_frames = gpu_trace.is_some() || !idx.basisu_resources.is_empty();
    let needs_device = gpu_trace.is_some()
        || shot.is_some()
            && (record_frames
                || !matches!(model, Model::Capsule)
                || moving
                || group_culling != visibility::Mode::Off
                || group_withhold.is_some());
    let mut live = if needs_device {
        Some(live::Live::new(
            replay.viewport,
            replay.vertical_fov_deg,
            compact_indices,
            reuse_bindings,
            &idx,
            visibility::Settings { mode: group_culling, full_index, watch: group_watch, withhold: group_withhold },
            &model,
        )?)
    } else {
        None
    };
    if let (Some(renderer), Some(half_extents)) = (&mut live, r.moving_half_extents()) {
        renderer.add_moving_box(half_extents);
    }
    let bc7_limit = live.as_ref().and_then(live::Live::bc7_limit);
    idx.select_basisu(bc7_limit);
    if needs_device && !idx.basisu_resources.is_empty() {
        o.source.configure_bc7(bc7_limit, decode)?;
    }
    let dir = &o.source;
    let mut world = World::new_with_model(&replay, r, &model)?;
    let mut motions = Vec::new();
    let mut acoustic_forecasts = Vec::new();
    let mut next_bounds = Vec::new();
    let global_support =
        o.all || r.bonnet.is_some() || extra_observers.is_some() || (impact_wav.is_some() && acoustic_forecast);
    if global_support || !o.protect || moving || articulated {
        idx.resolve_all_addresses(dir)?;
    }
    let mut support = support::Guard::load(dir, &mut idx, o.protect, global_support)?;
    if r.bonnet.is_some() {
        world.set_static_bounds(&support.static_bounds(&idx))?;
    }
    let mut impact_audio = impact_wav
        .as_ref()
        .map(|path| impact::Audio::new(path, playback, acoustic_forecast, acoustic_delay_ms, &idx, &o, replay.tick_hz))
        .transpose()?;
    let landing_plan =
        (impact_audio.is_some() && acoustic_forecast).then(|| planning::Landing::new(&idx, &support)).transpose()?;
    let horizon_s =
        (acoustic_delay_ms.unwrap_or(f64::from(o.delay_ms)) / 1000. + 2. / f64::from(replay.tick_hz)).max(1.);
    let mut lanes = [Lane::new(Kind::Physics), Lane::new(Kind::Visual)];
    let mut choices = if visuals { detail::Choices::new(&idx, scan_lods)? } else { detail::Choices::default() };
    let coarse_buffer_bytes = parent.as_ref().map_or(0, |p| models::sector_buffer_bytes(&p.scene));
    let mut peak_visual_buffer_bytes = coarse_buffer_bytes + library.io.resident_buffer_bytes;
    let mut local_buffer_bytes = 0;
    let mut fine = BTreeMap::new();
    let mut pending = BTreeMap::new();
    let mut pending_physics = BTreeMap::new();
    let mut colliders = BTreeMap::new();
    let mut first = None;
    let mut first_full = None;
    let mut rejected = Vec::new();
    let mut rejected_visual = Vec::new();
    let mut rejected_models = Vec::new();
    let mut image_fallbacks = Vec::new();
    let mut min_foot_after_contact: Option<f32> = None;
    let mut minimum_foot_y = world.foot()[1];
    let mut state_contact = None;
    let mut tick_work_us = Vec::new();
    let mut last_footprint = None;
    let mut body_samples = Vec::new();
    let mut selection = PassTime::default();
    let mut lod_publication = PassTime::default();
    let mut changed_visual_resources = Vec::new();
    // A cold spawn may need a regional file immediately below the actor. Start the
    // replay clock after that read so network delay cannot create a burst of catch-up ticks.
    if !idx.support_regions.is_empty() && !global_support && o.protect {
        world.part_bounds(&mut next_bounds);
        next_bounds.extend(world.box_bounds());
        support.prepare_step(0, &idx, dir, &world, &next_bounds, || {
            lanes[0].upcoming_physical_pages(&idx, world.motion(&replay))
        })?;
    }
    let start = std::time::Instant::now();
    for n in 0..=final_tick {
        if dir.paced() {
            let due = start + std::time::Duration::from_secs_f64(f64::from(n) / f64::from(replay.tick_hz));
            if let Some(remaining) = due.checked_duration_since(std::time::Instant::now()) {
                std::thread::sleep(remaining);
            }
        }
        let begin_tick = std::time::Instant::now();
        let (eye, aim) = r.camera_at(n);
        let actor = world.foot().into();
        if !choices.is_empty() {
            let view = osage_objects::DetailView::new(
                eye.as_dvec3(),
                (aim - eye).as_dvec3(),
                f64::from(replay.vertical_fov_deg).to_radians(),
                replay.viewport[1],
                0.1,
            )
            .context("invalid district view")?;
            let timing = gpu_trace.is_some().then(std::time::Instant::now);
            choices.request(&idx, &mut lanes[1], detail::Frame { tick: n, actor, view, viewport: replay.viewport }, &o);
            selection.finish(timing);
        }
        world.part_bounds(&mut next_bounds);
        next_bounds.extend(world.box_bounds());
        if n > 0 {
            support.prepare_step(n, &idx, dir, &world, &next_bounds, || {
                lanes[0].upcoming_physical_pages(&idx, world.motion(&replay))
            })?;
        }
        let mut primary_required = support.required_sweeps(&idx, &next_bounds);
        support.require_speculative(&idx, &world, &mut primary_required);
        if extra_observers.is_some() || articulated || moving {
            let primary = observers::Focus { id: observers::PRIMARY, foot: actor, motion: world.motion(&replay) };
            let mut observers = extra_observers
                .as_ref()
                .map_or_else(|| vec![primary], |tracks| tracks.active(n, primary, replay.gravity_m_s2));
            let mut required = BTreeMap::from([(observers::PRIMARY, primary_required.clone())]);
            for focus in observers.iter().skip(1) {
                required.insert(focus.id, support.required_bounds(&idx, focus.next_bounds(replay.tick_hz)));
            }
            if articulated {
                world.part_motions(&replay, &mut motions);
                observers.splice(
                    0..1,
                    motions.iter().map(|&motion| observers::Focus {
                        id: observers::PRIMARY,
                        foot: motion.center.as_vec3(),
                        motion,
                    }),
                );
            }
            if let Some(motion) = world.box_motion(&replay) {
                observers.push(observers::Focus { id: observers::PRIMARY, foot: motion.center.as_vec3(), motion });
            }
            lanes[0].set_observers(n, replay.tick_hz, &observers, &required, &idx, &o);
        } else {
            lanes[0].required_pages.clone_from(&primary_required);
            if o.predictive_physics && !o.all {
                lanes[0].forecast(n, replay.tick_hz, world.motion(&replay), &idx, &o);
            }
        }
        changed_visual_resources.clear();
        for lane in lanes.iter_mut().filter(|lane| visuals || lane.kind == Kind::Physics) {
            let mut changes = lane.update(n, replay.tick_hz, [actor, eye], &idx, &o, |i| {
                support.prepare_address(n, replay.tick_hz, i, &idx, dir)
            })?;
            if lane.kind == Kind::Physics {
                for i in lane.waiting.iter().copied() {
                    if support.prepare_tile(n, i, &idx, dir)? {
                        changes.push(Change::Install(
                            i,
                            pending_physics.remove(&i).context("missing staged collision page")?,
                        ));
                    }
                }
            }
            for change in changes {
                if let Change::LoadModel(id, _) | Change::EvictModel(id) = &change {
                    changed_visual_resources.push(*id);
                }
                match change {
                    Change::LoadModel(i, ticket) => match library.fetch(i, &idx, dir, ticket.as_ref()) {
                        Ok(()) => lane.model_loaded(n, i, &idx),
                        Err(error) if idx.resource(i).file.ends_with(".ktx2") => {
                            let candidate = idx.resource(i).file.clone();
                            ensure!(idx.fallback_image(i), "missing core image after BasisU failure");
                            image_fallbacks
                                .push(json!({"tick":n,"image":i-idx.model_resources.len(),"candidate":candidate,
                                "fallback":idx.resource(i).file,"reason":error.to_string()}));
                            if library.reuse_fallback(i, &idx)? {
                                lane.model_loaded(n, i, &idx);
                            } else if o.all {
                                let fallback = dir
                                    .start(idx.resource(i), true)
                                    .map_err(|_| anyhow::anyhow!("preload queue unavailable"))?;
                                library.fetch(i, &idx, dir, fallback.as_ref())?;
                                lane.model_loaded(n, i, &idx);
                            }
                        }
                        Err(e) if !o.all => {
                            if i < idx.model_resources.len() {
                                library.io.rejected += 1;
                            } else {
                                library.image_io.rejected += 1;
                            }
                            lane.model_failed(n, i, &idx, replay.tick_hz);
                            rejected_models
                                .push(json!({"tick":n,"model":i,"file":idx.resource(i).file,"error":e.to_string()}));
                        }
                        Err(e) => return Err(e),
                    },
                    Change::EvictModel(i) => library.remove(i, &idx)?,
                    Change::DropWaiting(i) => {
                        if lane.kind == Kind::Physics {
                            pending_physics.remove(&i);
                        } else {
                            pending.remove(&i);
                        }
                    }
                    Change::InstallBundled(i) => {
                        let handles = support.install_bundled(n, i, &idx, &mut world)?;
                        colliders.insert(i, handles);
                        lane.install_bundled(n, i, &idx);
                    }
                    Change::Install(i, ticket) => {
                        let t = &idx.tiles[i];
                        let l = lane.kind.layer(t).unwrap();
                        match lane.kind {
                            Kind::Visual => {
                                let visual = match l
                                    .content
                                    .received(dir, ticket.as_ref())
                                    .and_then(|bytes| models::Page::read(&bytes, &idx, &library))
                                {
                                    Ok(visual) => visual,
                                    Err(e) if dir.paced() && !o.all => {
                                        lane.reject(n, i, &idx, replay.tick_hz);
                                        rejected_visual.push(json!({"tick":n,"tile":[t.x,t.z],"file":l.content.file,"error":e.to_string()}));
                                        continue;
                                    }
                                    Err(e) => return Err(e),
                                };
                                ensure!(
                                    visual.triangles == l.triangles,
                                    "visual triangle count differs from directory"
                                );
                                if let Some(physical) = &t.physics
                                    && idx.support.is_some()
                                    && idx.object_profile.is_none()
                                {
                                    ensure!(
                                        physical.source_nodes.iter().all(|n| visual.owners.contains(n)),
                                        "collision owner has no matching visual piece"
                                    );
                                }
                                if idx.demand() {
                                    choices.validate_page(i, &visual, l.adaptive_only)?;
                                    let fixed = visual
                                        .owners
                                        .iter()
                                        .zip(&visual.draws)
                                        .filter(|(owner, _)| !choices.is_adaptive(**owner))
                                        .map(|(_, d)| d);
                                    let uses: BTreeSet<_> = fixed
                                        .clone()
                                        .filter_map(|d| match d {
                                            models::Draw::Shared(j, _) => Some(idx.model_variants[*j].resource),
                                            _ => None,
                                        })
                                        .collect();
                                    ensure!(
                                        uses.into_iter().eq(l.uses.iter().copied()),
                                        "model dependencies differ from directory"
                                    );
                                    let image_uses: BTreeSet<_> = fixed
                                        .filter_map(|draw| match draw {
                                            models::Draw::Shared(v, _) => Some(&idx.model_variants[*v].materials),
                                            _ => None,
                                        })
                                        .flatten()
                                        .filter_map(|&i| idx.material_palette[i].pbr.texture.as_ref())
                                        .map(|info| idx.textures[info.index].source)
                                        .chain(visual.image_bindings.iter().map(|(id, _)| *id))
                                        .collect();
                                    ensure!(
                                        image_uses.into_iter().eq(l.uses_images.iter().copied()),
                                        "image dependencies differ from directory"
                                    );
                                    lane.stage(n, i, &idx);
                                    pending.insert(i, visual);
                                    continue;
                                }
                                fine.insert(i, visual);
                            }
                            Kind::Physics => {
                                let file = idx.physical_file(i).context("unresolved collision page")?;
                                let bytes = file.received(dir, ticket.as_ref());
                                if bytes.is_ok() && !support.prepare_tile(n, i, &idx, dir)? {
                                    lane.stage(n, i, &idx);
                                    pending_physics.insert(i, ticket);
                                    continue;
                                }
                                let loaded = bytes.and_then(|bytes| {
                                    if (ticket.is_none() || o.all)
                                        && let Some(r) = &mut lane.reservations
                                    {
                                        r.verified_on_install(n, i, &idx, bytes.len(), o.all);
                                    }
                                    support.install_full(n, &idx, i, &bytes, &mut world)
                                });
                                match loaded {
                                    Ok(handles) => {
                                        colliders.insert(i, handles);
                                    }
                                    Err(e)
                                        if matches!(support.mode, "certified_support" | "wait_for_full") && !o.all =>
                                    {
                                        lane.reject(n, i, &idx, replay.tick_hz);
                                        rejected.push(
                                            json!({"tick":n,"tile":[t.x,t.z],"file":file.file,"error":e.to_string()}),
                                        );
                                        continue;
                                    }
                                    Err(e) => return Err(e),
                                }
                            }
                        }
                        lane.install(n, i, &idx);
                    }
                    Change::Evict(i) => match lane.kind {
                        Kind::Visual => {
                            fine.remove(&i);
                        }
                        Kind::Physics => world.remove(colliders.remove(&i).context("missing active collider page")?),
                    },
                }
            }
            let publish_timing =
                (lane.kind == Kind::Visual && !choices.is_empty() && gpu_trace.is_some()).then(std::time::Instant::now);
            if lane.kind == Kind::Visual {
                choices.publish(n, lane, &mut fine, &idx, &changed_visual_resources);
            }
            if lane.kind == Kind::Visual && !pending.is_empty() {
                let ready: Vec<_> = pending
                    .keys()
                    .copied()
                    .filter(|&i| {
                        choices.ready(i, idx.tiles[i].visual.adaptive_only)
                            && idx.dependencies(&idx.tiles[i].visual).all(|id| lane.model_resident.contains(&id))
                    })
                    .collect();
                for i in ready {
                    let mut page = pending.remove(&i).expect("pending visual page");
                    page.bind_images(&library)?;
                    choices.activate(i, &mut page, &idx);
                    fine.insert(i, page);
                    lane.install(n, i, &idx);
                }
            }
            lod_publication.finish(publish_timing);
        }
        local_buffer_bytes = fine.values().chain(pending.values()).map(|p| p.local_buffer_bytes).sum::<usize>();
        peak_visual_buffer_bytes =
            peak_visual_buffer_bytes.max(coarse_buffer_bytes + local_buffer_bytes + library.io.resident_buffer_bytes);
        let withheld = target.filter(|_| o.hold == Some(Kind::Physics) && n < o.hold_ticks);
        support.sync(n, &idx, dir, &colliders, withheld, &mut world)?;
        let stepping = n > 0 && support.can_step(n, &idx, &primary_required, &colliders, &world, &next_bounds);
        if trace_body {
            let motion = world.motion(&replay);
            body_samples.push(json!({"tick":n,"center_m":motion.center.to_array(),"foot_m":world.foot(),"velocity_m_s":motion.velocity.to_array(),"supported":motion.gravity==0.,"step":stepping}));
        }
        let mut solved_landing = None;
        if stepping {
            let (contact, landing) = world.step(n, &replay, impact_audio.is_some())?;
            solved_landing = landing;
            if first_full.is_none() && contact.as_ref().is_some_and(|c| c.geometry == "full") {
                first_full = contact.clone();
            }
            if first.is_none()
                && let Some(hit) = contact
            {
                let landed = hit.tile.map(|tile| {
                    idx.tiles
                        .iter()
                        .position(|t| [t.x, t.z] == tile)
                        .expect("static contact identifies its loaded tile")
                });
                let visual_has_owner =
                    landed.zip(hit.source_node).zip(parent.as_ref()).filter(|_| idx.support.is_some()).map(
                        |((i, node), parent)| match fine.get(&i) {
                            Some(detail) => detail.owners.contains(&node),
                            None => {
                                let t = &idx.tiles[i];
                                parent.owners[t.parent_first..t.parent_first + t.parent_count].contains(&node)
                            }
                        },
                    );
                state_contact = Some(json!({
                    "landing_tile_has_fine": landed.map(|i| fine.contains_key(&i)),
                    "watch_has_fine": target.map(|i| fine.contains_key(&i)),
                    "landing_tile_has_physics": landed.map(|i| colliders.contains_key(&i)),
                    "source_path":idx.path(hit.source_node),"visual_has_same_source":visual_has_owner
                }));
                first = Some(hit);
            }
            let y = world.foot()[1];
            minimum_foot_y = minimum_foot_y.min(y);
            if first.is_some() {
                min_foot_after_contact = Some(min_foot_after_contact.map_or(y, |low| low.min(y)));
            }
        }
        if trace_body && (articulated || moving) {
            let row = body_samples.last_mut().expect("body sample precedes solve");
            row["parts_after_step"] = serde_json::to_value(world.part_poses())?;
            if moving {
                row["boxes_after_step"] = serde_json::to_value(world.box_poses())?;
                if stepping && r.bonnet.is_some() {
                    row["swept_impact_offset_s"] = json!(world.swept_impact_offset_s());
                }
            }
            row["contacts_after_step"] = if stepping { serde_json::to_value(world.contacts())? } else { json!([]) };
        }
        if let Some(gap) = footprint_gap
            && (trace_body || n == final_tick)
        {
            let footprint = osage_dynamics::support_footprint::project(
                world.parts(),
                world.part_poses(),
                if stepping { world.contacts() } else { &[] },
                world.substeps(),
                gap,
                0.1,
                |id| world.box_index(id).map(|i| world.box_poses()[i]),
            )?;
            if trace_body {
                body_samples.last_mut().expect("recorded body tick")["support_footprint"] = json!(&footprint);
            }
            last_footprint = Some(footprint);
        }
        if let Some(audio) = &mut impact_audio {
            acoustic_forecasts.clear();
            if (articulated || !world.sound_contact_active())
                && let Some(plan) = &landing_plan
            {
                let remaining_s = f64::from(r.ticks - n) / f64::from(replay.tick_hz);
                let horizon = horizon_s.min(remaining_s);
                if horizon > 0. {
                    if articulated || r.bonnet.is_some() {
                        world.part_motions(&replay, &mut motions);
                    }
                    if articulated {
                        plan.predict_rig(&world, &motions, n, replay.tick_hz, horizon, &mut acoustic_forecasts)?;
                    } else if let Some(hit) = plan.predict(world.motion(&replay), n, replay.tick_hz, horizon)? {
                        acoustic_forecasts.push((0, hit));
                    }
                    if let Some(bonnet) = r.bonnet {
                        planning::moving::predict(
                            &world,
                            &replay,
                            bonnet,
                            &motions,
                            n,
                            horizon,
                            &mut acoustic_forecasts,
                        );
                    }
                }
            }
            let contacts = if articulated {
                if stepping { audio.reactions(&world, n) } else { Vec::new() }
            } else if moving {
                if stepping { audio.capsule_reactions(&world, n) } else { Vec::new() }
            } else {
                solved_landing.into_iter().collect()
            };
            audio.tick(n, r, &idx, dir, contacts, acoustic_forecasts.drain(..))?;
        }
        if ((record_frames && n.is_multiple_of(gpu_stride)) || n == final_tick)
            && let Some(renderer) = &mut live
        {
            renderer.publish(&idx, parent.as_ref().context("missing visual parent")?, &fine, &library)?;
            let (eye, aim) = r.camera_at(n);
            renderer.frame(n, eye, aim, world.part_poses(), world.box_poses().first())?;
        }
        if dir.paced() {
            tick_work_us.push(begin_tick.elapsed().as_micros() as u64);
        }
    }
    let resident_triangles: usize = fine.values().map(|s| s.triangles).sum();
    let shared_references: Vec<_> = fine
        .values()
        .flat_map(|p| p.draws.iter())
        .filter_map(|d| match d {
            models::Draw::Shared(i, _) => Some(*i),
            _ => None,
        })
        .collect();
    let distinct: BTreeSet<_> = shared_references.iter().copied().collect();
    let geometry: BTreeSet<_> = distinct.iter().map(|&j| idx.model_variants.get(j).map_or(j, |v| v.resource)).collect();
    let bootstrap_visual_bytes = if parent.is_some() && !embedded_parent { idx.parent.bytes } else { 0 };
    let (parent_scene, parent_owners) = parent.map(|p| (Some(p.scene), p.owners)).unwrap_or_default();
    let cold_bytes = index_bytes
        + (bootstrap_visual_bytes
            + library.io.read_bytes
            + library.image_io.read_bytes
            + (support.stats.encoded_bytes - support.stats.embedded_bytes)
            + support.stats.synchronous_legacy_bytes
            + lanes[0].stats.read_bytes
            + lanes[1].stats.read_bytes) as u64;
    let parent_mask = || {
        idx.tiles.iter().enumerate().flat_map(|(i, t)| {
            let page = fine.get(&i);
            let owners = &parent_owners;
            (t.parent_first..t.parent_first + t.parent_count)
                .map(move |n| page.is_none_or(|p| p.needs_parent(owners.get(n))))
        })
    };
    let fallback_count = if visuals { parent_mask().filter(|&keep| keep).count() } else { 0 };
    let mut result = json!({"format":"ville-paris/paging/1","source_sha256":idx.source_sha256,"replay_sha256":sha(&replay_bytes),"route":route,"tick":final_tick,"clock_hz":replay.tick_hz,"expected_support":r.expected_support,
        "tile_edge_m":idx.edge_m,"district_tiles":idx.tiles.len(),"district_physics_tiles":idx.tiles.iter().filter(|t|t.physics.is_some()).count(),"source_instances":idx.source_instances,"source_triangles":idx.source_triangles,
        "bootstrap_visual_bytes":bootstrap_visual_bytes,"coarse_triangles":parent_scene.as_ref().map_or(0, |p| p.triangles),"settings":{"visual_streaming":visuals,"physics_radius_m":o.physics_radius,"physics_plan":if o.predictive_physics {"trajectory"} else {"radius"},"visual_radius_m":o.visual_radius,"delay_ms":o.delay_ms,"link":o.link,"mbps_per_kind":o.mbps,"max_in_flight_per_kind":4,"eviction_hysteresis_m":idx.edge_m*0.5,"all_resident":o.all,"watch":watch,"hold":o.hold,"hold_ticks":o.hold_ticks},
        "physics":lanes[0].stats,"visual":lanes[1].stats,"physics_events":lanes[0].events,"physics_forecasts":lanes[0].plan.requests,"physics_service_samples":lanes[0].plan.samples,"visual_events":lanes[1].events,"first_contact":first,"at_contact":state_contact,"final_foot_m":world.foot(),
        "support":{"mode":support.mode,"stats":support.stats,"duplicate_layer_contact_ticks":world.duplicate_layer_contact_ticks,"events":support.events,"rejected_full_pages":rejected,"first_full_contact":first_full,"minimum_foot_y_after_contact_m":min_foot_after_contact,"minimum_foot_y_m":minimum_foot_y},
        "storage":{"format":idx.format,"index_bytes":index_bytes,"cold_file_bytes":cold_bytes,"model_library_reads":u8::from(visuals && idx.models.is_some() && !embedded_parent),
                   "model_library_bytes":library.encoded_bytes,"model_library_decoded_bytes":library.decoded_bytes,"shared_models":library.len(),
                   "visual_cpu_buffers":{"coarse":coarse_buffer_bytes,"local":local_buffer_bytes,"shared":library.io.resident_buffer_bytes,
                    "total":coarse_buffer_bytes+local_buffer_bytes+library.io.resident_buffer_bytes,"peak_total":peak_visual_buffer_bytes},"model_io":library.io,"image_fallbacks":image_fallbacks,"image_io":library.image_io,"model_events":lanes[1].model_events,"rejected_models":rejected_models,"decoded_pages_waiting":pending.len(),"visual_lods":choices.report()},
        "published":{"fine_tiles":fine.len(),"fine_triangles":resident_triangles,"coarse_instances":fallback_count,"watch_is_fine":target.map(|i|fine.contains_key(&i)),
                     "shared_model_instances":shared_references.len(),"distinct_shared_draw_models":distinct.len(),"distinct_shared_resource_meshes":geometry.len()}});
    if let Some(regions) = support.region_report() {
        result["support"]["regions"] = regions;
    }
    let mut acoustic_bytes = 0u64;
    if let Some(report) = world.lift_approach() {
        result["lift_approach"] = serde_json::to_value(report)?;
    }
    if let Some(footprint) = last_footprint {
        result["support_footprint"] = json!(footprint);
    }
    if let Some(pin) = index_pin {
        result["storage"]["index_sha256"] = json!(pin.file.sha256);
    }
    if articulated || r.lift.is_some() || collision_ray_actor {
        result["actor"] = json!({"model":model.to_string(),"rig_sha256":actor_sha,"substeps_per_tick":world.substeps(),
            "parts":world.parts().iter().map(|p|json!({"name":p.name,"parent":p.parent,"radius_m":p.radius,"half_segment_m":p.half_segment,"mass_kg":p.mass})).collect::<Vec<_>>()});
    }
    if collision_ray_actor && let Model::Imported(rig) = &model {
        result["actor"]["bindings"] = json!(rig.bindings);
    }
    if let Some(body) = r.bonnet {
        result["bonnet"] = json!({"definition": body, "final_pose": world.box_poses().first()});
    }
    if let Some(lift) = r.lift {
        result["lift"] = json!({"definition": lift, "final_pose": world.box_poses().first()});
    }
    if let Some(audio) = impact_audio {
        let audio = audio.finish()?;
        acoustic_bytes = audio["read_bytes"].as_u64().context("missing impact audio read count")?;
        result["impact_audio"] = audio;
    }
    if let Some(pool) = &lanes[0].reservations {
        result["physics_observers"] = pool.report();
    }
    if trace_body {
        result["body_before_step"] = json!(body_samples);
        if articulated || moving {
            result["contact_surfaces"] = json!(
                (0..world.surface_count())
                    .map(|i| {
                        let (id, node) = world.surface(i).expect("allocated surface identity");
                        json!({"id":id,"source_node":node,"source_path":idx.path(node)})
                    })
                    .collect::<Vec<_>>()
            );
        }
    }
    if !collision_rays.is_empty() {
        result["collision_probes"] =
            json!(world.probe_segments(&idx, &support, &colliders, &collision_rays, collision_ray_actor)?);
    }
    if !acoustic_rays.is_empty() {
        let series = channels::probe_series(&idx, dir, &acoustic_rays)?;
        acoustic_bytes += series.read_bytes as u64;
        result["acoustic_probe"] = series.probes.last().expect("nonempty ray series").clone();
        result["acoustic_probes"] = Value::Array(series.probes);
        result["acoustic_io"] = series.io;
    }
    result["storage"]["cold_file_bytes"] = json!(cold_bytes + acoustic_bytes);
    if model_fingerprints {
        result["storage"]["model_fingerprints"] = library.fingerprints(&idx, &fine);
    }
    if let Some((kind, stats)) = dir.traffic() {
        tick_work_us.sort_unstable();
        let quantile = |percent: usize| tick_work_us[(tick_work_us.len() * percent).div_ceil(100) - 1];
        result["transport"] = json!({"kind":kind,"wall_clock":true,"tick_work_us":{"p50":quantile(50),"p95":quantile(95),"p99":quantile(99),"max":tick_work_us.last()},"additional_link_profile":o.shape_link,"io":stats,"rejected_visual_pages":rejected_visual});
    }
    if let Some(mut live) = live {
        if let Some(path) = &shot {
            live.capture(path)?;
        }
        let mut report = live.report();
        report["source_sha256"] = json!(idx.source_sha256);
        report["route"] = json!(route);
        report["replay_sha256"] = json!(sha(&replay_bytes));
        report["clock_hz"] = json!(replay.tick_hz);
        report["viewport"] = json!(replay.viewport);
        report["frame_stride_ticks"] = json!(gpu_stride);
        report["lod_selection_work"] = json!(&choices.work);
        report["lod_selection_cpu_ns"] = json!(&selection);
        report["lod_publication_cpu_ns"] = json!(&lod_publication);
        report["lod_publication_work"] = json!(&choices.publication);
        report["lod_publication_mode"] = json!(if scan_lods { "scan" } else { "changed" });
        if let Some(path) = gpu_trace {
            write_json(&path, &report)?;
        }
    } else if let Some(png) = shot {
        let mut scene = parent_scene.context("missing screenshot parent")?;
        let mut selected = parent_mask();
        scene.instances.retain(|_| selected.next().expect("coarse range validated"));
        let mut shared_draws = BTreeMap::new();
        let mut palette = view::InstanceMaterials::default();
        for (id, v) in idx.model_variants.iter().enumerate() {
            let materials = if distinct.contains(&id) {
                v.materials.iter().map(|&i| library.material(&idx, i)).collect::<Result<Vec<_>>>()?
            } else {
                Vec::new()
            };
            palette.variants.push(materials);
        }
        for page in fine.into_values() {
            page.publish(&mut scene, &mut library, &mut shared_draws, &idx, &mut palette)?;
        }
        ensure!(shared_draws.len() == geometry.len(), "shared draw publication mismatch");
        let (eye, aim) = r.camera_at(final_tick);
        let keys = [0, 1].map(|t| json!({"tick":t,"eye":eye.to_array(),"aim":aim.to_array()}));
        let shot_replay = json!({"version":1,"tick_hz":replay.tick_hz,"gravity_m_s2":replay.gravity_m_s2,"terminal_m_s":replay.terminal_m_s,"viewport":replay.viewport,"vertical_fov_deg":replay.vertical_fov_deg,
            "routes":[{"id":route,"ticks":1,"start_m":world.foot(),"velocity_m_s":[0,0,0],"expected_support":r.expected_support,"camera":keys}]});
        let photo = Replay::read(&serde_json::to_vec(&shot_replay)?, [r.expected_support.as_str()])?;
        view::run_with_materials(
            scene,
            photo,
            view::Options {
                listening: None,
                selected: 0,
                shot: Some(png),
                tick: 0,
                timing: None,
                full_detail: true,
                physics: Physics::Point,
                model: Model::Capsule,
            },
            palette,
        )?;
    }
    write_json(&out, &result)
}
fn write_json(path: &Path, data: &Value) -> Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    serde_json::to_writer(tmp.as_file_mut(), data)?;
    tmp.persist(path)?;
    Ok(())
}
