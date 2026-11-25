// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{Actor, Physics, part_table};
use crate::people::Model;
use crate::replay::{Replay, Route};
use crate::scene::Sector;
use anyhow::{Result, ensure};
use osage_dynamics::{Engine, Pose};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::VecDeque;

#[derive(Serialize)]
struct Sample {
    step: u64,
    parts: Vec<Pose>,
    #[serde(rename = "box")]
    bonnet: Pose,
}

/// Capture the configured collision interval containing the first bonnet manifold.
fn capture(engine: Engine, model: &Model, scene: &Sector, replay: &Replay, route: &Route) -> Result<Value> {
    let Actor::Rigid(mut sim, _) = Actor::new(Physics::Rigid(engine), model, scene, replay, route)? else {
        unreachable!()
    };
    let target = scene.colliders.len();
    let steps = sim.substeps_per_tick();
    let window = u64::from((replay.tick_hz * u32::from(steps)).div_ceil(20)); // 50 ms on each side.
    let mut samples = VecDeque::from([Sample { step: 0, parts: sim.part_poses.clone(), bonnet: sim.box_poses[0] }]);
    let mut first = None;
    let mut last = None;
    for tick in 1..=route.ticks {
        sim.step_with_substeps(|state| {
            let step = u64::from(tick - 1) * u64::from(steps) + u64::from(state.substep);
            if last.is_some_and(|end| step > end) {
                return;
            }
            let mut parts = if first.is_none() && samples.len() as u64 > window {
                samples.pop_front().unwrap().parts
            } else {
                Vec::new()
            };
            parts.clear();
            parts.extend_from_slice(state.parts);
            samples.push_back(Sample { step, parts, bonnet: state.boxes[0] });
            if first.is_none() && state.contacts.iter().any(|c| c.surface == target) {
                let contacts: Vec<_> = state.contacts.iter().filter(|c| c.surface == target).collect();
                first = Some(json!({"tick":tick,"substep":state.substep,"step":step,"contacts":contacts}));
                last = Some(step + window);
            }
        })?;
        if let Some(hit) = &mut first {
            if hit.get("tick_end").is_none() {
                // Also sampled in the ordinary public CSV, even when the public clock is coarse.
                hit["tick_end"] = json!({"parts":sim.part_poses,"box":sim.box_poses[0]});
            }
            if samples.back().map(|s| s.step) == last {
                break;
            }
        }
    }
    if first.is_none() {
        samples.clear();
    }
    Ok(json!({"id":route.id,"substeps_per_tick":steps,"bonnet":route.bonnet,"first_contact":first,"samples":samples}))
}

pub(crate) fn report(
    engine: Engine,
    model: &Model,
    scene: &Sector,
    replay: &Replay,
    route_id: Option<&str>,
    inputs: &Value,
) -> Result<Value> {
    let routes: Vec<_> = replay
        .routes
        .iter()
        .filter(|r| route_id.is_none_or(|id| r.id == id) && r.bonnet.is_some())
        .map(|r| capture(engine, model, scene, replay, r))
        .collect::<Result<_>>()?;
    ensure!(!routes.is_empty(), "select a replay route containing a bonnet");
    Ok(json!({"format":"osage/bonnet-contact/1","engine":engine,"actor":model.to_string(),"inputs":inputs,
        "tick_hz":replay.tick_hz,"parts":part_table(model),"routes":routes}))
}
