// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What stands along the kart circuit when the asset manifest's models are
//! there: tyre walls on the outside of the bends, grass and wild flowers in
//! the meadows, ferns, low plants and bushes, rocks, and pines in groves.
//! Each is an `osage_props::Scatter`: one model, many places, seen up to a
//! distance chosen by its size — a tuft of grass vanishes at forty metres,
//! a pine at three hundred and fifty.
//!
//! [`scatters`] decides every place, from the land and the track alone: the
//! same circuit gives the same roadside on every machine.

use bevy::{gltf::Gltf, prelude::*};
use osage_kart_land::{BARRIER, Frame, Land, Site, noise};
use osage_kart_physics::Track;
use osage_procedural::Dice;
use osage_props::{Scatter, Use, poly_haven};

/// The models, from the asset manifest.
#[derive(Clone)]
pub struct Models {
    pub tyre: Handle<Gltf>,
    pub grass: Handle<Gltf>,
    pub dandelion: Handle<Gltf>,
    pub fern: Handle<Gltf>,
    pub low_plants: [Handle<Gltf>; 2],
    pub bush: Handle<Gltf>,
    pub boulder: Handle<Gltf>,
    pub stone: Handle<Gltf>,
    pub pine: Handle<Gltf>,
}

impl Models {
    pub fn load(server: &AssetServer) -> Self {
        let model = |id| poly_haven(server, id);
        Self {
            tyre: model("old_tyre"),
            grass: model("grass_bermuda_01"),
            dandelion: model("dandelion_01"),
            fern: model("fern_02"),
            low_plants: [model("shrub_03"), model("shrub_04")],
            bush: model("shrub_02"),
            boulder: model("rock_07"),
            stone: model("rock_09"),
            pine: model("pine_sapling_small"),
        }
    }
}

/// How much is placed: what a tier's budget allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    /// Tyre walls, bushes, boulders and pines: few and large.
    Some,
    /// Everything, grass and flowers too.
    Full,
}

/// A tyre lies flat: the model stands on its tread, its axis along Z.
const TYRE: f32 = 0.6;
const TYRE_HEIGHT: f32 = 0.16;

/// Every scatter of the roadside. `clearings` are places kept free (a
/// grandstand, a building).
pub fn scatters(
    land: &Land,
    frames: &[Frame],
    track: &Track,
    clearings: &[Site],
    models: &Models,
    detail: Detail,
) -> Vec<Scatter> {
    let roadside = track.half_width + track.verge + BARRIER;
    let mut dice = Dice(11);
    // Whether a plant may grow here: clear of the barrier by `margin`, not in
    // the lake or on its beach, not in a clearing.
    let free = |at: Vec2, margin: f32| {
        land.distance_to_track(at) > roadside + margin
            && !land.wet(at)
            && !clearings.iter().any(|site| site.centre.distance(at) < site.radius)
    };
    // A point beside the track, `low` to `high` metres beyond the barrier, on
    // either side: plants are where they are seen. Anywhere along the track,
    // not only at its frames, or plants would stand in rows.
    let beside = |dice: &mut Dice, low: f32, high: f32| {
        let i = (dice.roll() * frames.len() as f32) as usize % frames.len();
        let (frame, next) = (&frames[i], &frames[(i + 1) % frames.len()]);
        let t = dice.roll();
        let (at, right) = (frame.at.lerp(next.at, t), frame.right.lerp(next.right, t).normalize_or_zero());
        let side = if dice.roll() < 0.5 { -1.0 } else { 1.0 };
        at + right * side * (roadside + dice.between(low, high))
    };
    let stand = |at: Vec2, sink: f32, turn: f32, size: f32| {
        Transform::from_xyz(at.x, land.height(at) - sink, at.y)
            .with_rotation(Quat::from_rotation_y(turn))
            .with_scale(Vec3::splat(size))
    };
    // Dense models seen far (trees, bushes, rocks) get coarser meshes with distance.
    let scatter = |model: &Handle<Gltf>, places: Vec<Transform>, using: Use, within: f32, shadows: bool| Scatter {
        model: model.clone(),
        places,
        using,
        within: Some(within),
        shadows,
        lods: within >= 60.0,
        tint: None,
    };

    let mut all = vec![scatter(&models.tyre, tyre_walls(frames, track), Use::Whole, 160.0, true)];

    // Pines in groves near the circuit, three sizes of sapling grown tall.
    let mut pines = Vec::new();
    for _ in 0..4000 {
        let at = beside(&mut dice, 8.0, 140.0);
        let (turn, size, chance) = (dice.between(0.0, std::f32::consts::TAU), dice.between(4.0, 7.5), dice.roll());
        if chance < (noise(at / 45.0 + 3.0) - 0.4) * 1.8 && free(at, 8.0) && pines.len() < 260 {
            pines.push(stand(at, 0.1, turn, size));
        }
    }
    all.push(scatter(&models.pine, pines, Use::Variants, 450.0, true));

    // Woods further off, to the edge of the modelled land: groves on the
    // hills, tall, seen far as coarse meshes.
    let mut woods = Vec::new();
    for _ in 0..9000 {
        let at = beside(&mut dice, 140.0, 400.0);
        let (turn, size, chance) = (dice.between(0.0, std::f32::consts::TAU), dice.between(5.0, 9.0), dice.roll());
        let inside = (at - land.centre()).abs().max_element() < osage_kart_land::LAND / 2.0 - 30.0;
        if inside && chance < (noise(at / 70.0 + 9.0) - 0.42) * 2.2 && free(at, 8.0) && woods.len() < 700 {
            woods.push(stand(at, 0.2, turn, size));
        }
    }
    all.push(scatter(&models.pine, woods, Use::Variants, 900.0, true));

    // Bushes and boulders.
    let mut bushes = Vec::new();
    let mut boulders = Vec::new();
    for _ in 0..1600 {
        let at = beside(&mut dice, 3.0, 60.0);
        let (turn, roll) = (dice.between(0.0, std::f32::consts::TAU), dice.roll());
        if !free(at, 3.0) {
            continue;
        }
        if roll < 0.3 && bushes.len() < 300 {
            bushes.push(stand(at, 0.15, turn, dice.between(0.7, 1.3)));
        } else if roll < 0.38 && boulders.len() < 90 {
            boulders.push(stand(at, 0.05, turn, dice.between(4.0, 11.0)));
        }
    }
    all.push(scatter(&models.bush, bushes, Use::Variants, 260.0, true));
    all.push(scatter(&models.boulder, boulders, Use::Whole, 250.0, true));

    if detail == Detail::Full {
        // Grass: tufts thick just beyond the barrier, where the camera sees them.
        let mut grass = Vec::new();
        for _ in 0..40_000 {
            let at = beside(&mut dice, 0.6, 8.0);
            let (turn, size) = (dice.between(0.0, std::f32::consts::TAU), dice.between(2.2, 3.6));
            if free(at, 0.6) && grass.len() < 22_000 {
                grass.push(stand(at, 0.02, turn, size));
            }
        }
        all.push(Scatter { tint: Some(GRASS), ..scatter(&models.grass, grass, Use::Variants, 45.0, false) });

        // Wild flowers, ferns, low plants and stones.
        let mut flowers = Vec::new();
        let mut ferns = Vec::new();
        let mut low = [Vec::new(), Vec::new()];
        let mut stones = Vec::new();
        for _ in 0..6000 {
            let at = beside(&mut dice, 1.0, 50.0);
            let (turn, roll) = (dice.between(0.0, std::f32::consts::TAU), dice.roll());
            if !free(at, 1.0) {
                continue;
            }
            match roll {
                r if r < 0.1 && flowers.len() < 500 => flowers.push(stand(at, 0.0, turn, dice.between(1.4, 2.0))),
                r if r < 0.16 && ferns.len() < 260 => ferns.push(stand(at, 0.03, turn, dice.between(1.0, 1.7))),
                r if r < 0.26 => {
                    let kind = (dice.roll() * 2.0) as usize % 2;
                    if low[kind].len() < 200 {
                        low[kind].push(stand(at, 0.02, turn, dice.between(1.5, 2.6)));
                    }
                }
                r if r < 0.34 && stones.len() < 260 => stones.push(stand(at, 0.01, turn, dice.between(3.0, 7.0))),
                _ => {}
            }
        }
        all.push(scatter(&models.dandelion, flowers, Use::Variants, 40.0, false));
        all.push(scatter(&models.fern, ferns, Use::Variants, 70.0, false));
        let [first, second] = low;
        all.push(scatter(&models.low_plants[0], first, Use::Variants, 60.0, false));
        all.push(scatter(&models.low_plants[1], second, Use::Variants, 60.0, false));
        all.push(scatter(&models.stone, stones, Use::Whole, 60.0, false));
    }
    all
}

/// The colour of the grass tufts: their texture is an atlas of blades on
/// black, which darkens them as they shrink.
const GRASS: Color = Color::srgb(0.29, 0.38, 0.15);

/// Grass around the camera (`osage_props::Carpet`): mown short on the
/// verges, tall beyond the wall; none on the road, the kerbs, the wall, in a
/// gravel trap or in the lake.
pub fn carpet(land: &Land, frames: &[Frame], track: &Track, models: &Models) -> osage_props::Carpet {
    let (land, half, wall) = (land.clone(), track.half_width, track.half_width + track.verge + BARRIER);
    let bends: Vec<(Vec2, Vec2, f32)> = frames.iter().map(|f| (f.at, f.right, f.bend)).collect();
    let ground = move |at: Vec2| -> Option<(f32, f32)> {
        let distance = land.distance_to_track(at);
        if distance < half + 1.15 || land.wet(at) || (distance > wall - 0.05 && distance < wall + 0.75) {
            return None;
        }
        if distance < wall {
            // On the verge: no grass in a gravel trap, on the outside of a bend.
            let &(near, right, bend) =
                bends.iter().min_by(|a, b| a.0.distance_squared(at).total_cmp(&b.0.distance_squared(at)))?;
            let side = (at - near).dot(right).signum();
            if bend.abs() > 0.011 && bend.signum() == side {
                return None;
            }
            return Some((land.height(at), 1.25));
        }
        Some((land.height(at) - 0.02, 2.6))
    };
    osage_props::Carpet {
        model: models.grass.clone(),
        using: Use::Variants,
        spacing: 0.42,
        radius: 12.0,
        ground: std::sync::Arc::new(ground),
        tint: Some(GRASS),
    }
}

/// Tyres lying flat in stacks of three, two rows deep, along the inside of
/// the barrier on the outside of every bend: where a kart that runs wide ends.
fn tyre_walls(frames: &[Frame], track: &Track) -> Vec<Transform> {
    let wall = track.half_width + track.verge + BARRIER;
    let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let mut places = Vec::new();
    for (i, frame) in frames.iter().enumerate() {
        if frame.bend.abs() < 0.011 {
            continue;
        }
        // A bend to the left throws karts to the right, and the other way round.
        let outside = frame.bend.signum();
        let next = &frames[(i + 1) % frames.len()];
        let length = frame.at.distance(next.at);
        let count = (length / TYRE).ceil().max(1.0) as usize;
        for step in 0..count {
            let t = step as f32 / count as f32;
            let (at, right) = (frame.at.lerp(next.at, t), frame.right.lerp(next.right, t).normalize_or_zero());
            for (row, inset) in [0.33, 0.93].into_iter().enumerate() {
                let foot = at + right * outside * (wall - inset);
                for level in 0..3 {
                    // Tyres are never stacked quite straight.
                    let wobble = ((i * 7 + step * 3 + row * 5 + level) % 11) as f32 * 0.6;
                    let place = Transform::from_xyz(foot.x, TYRE_HEIGHT * (level as f32 + 0.5), foot.y)
                        .with_rotation(Quat::from_rotation_y(wobble) * flat);
                    places.push(place);
                }
            }
        }
    }
    places
}

#[cfg(test)]
mod tests {
    use osage_kart_land::frames;
    use osage_world_kart::Circuit;

    use super::*;

    fn roadside(detail: Detail) -> (Vec<Scatter>, Land, Circuit) {
        let circuit = Circuit::new();
        let frames = frames(&circuit.track);
        let land = Land::new(&frames, &circuit.track);
        let models = Models {
            tyre: Handle::default(),
            grass: Handle::default(),
            dandelion: Handle::default(),
            fern: Handle::default(),
            low_plants: [Handle::default(), Handle::default()],
            bush: Handle::default(),
            boulder: Handle::default(),
            stone: Handle::default(),
            pine: Handle::default(),
        };
        let clearings = [Site { centre: frames[0].at, radius: 30.0 }];
        (scatters(&land, &frames, &circuit.track, &clearings, &models, detail), land, circuit)
    }

    #[test]
    fn nothing_grows_on_the_road_in_the_lake_or_in_a_clearing() {
        let (scatters, land, circuit) = roadside(Detail::Full);
        let roadside = circuit.track.half_width + circuit.track.verge + BARRIER;
        let clearing = frames(&circuit.track)[0].at;
        for scatter in &scatters[1..] {
            for place in &scatter.places {
                let at = place.translation.xz();
                assert!(land.distance_to_track(at) > roadside, "{at} is on the road side");
                assert!(!land.wet(at), "{at} is in the lake");
                assert!(at.distance(clearing) >= 30.0, "{at} is in the clearing");
                assert!((place.translation.y - land.height(at)).abs() < 0.3, "{at} stands on the ground");
            }
        }
    }

    #[test]
    fn tyre_walls_line_the_outside_of_the_bends() {
        let (scatters, land, circuit) = roadside(Detail::Some);
        let tyres = &scatters[0].places;
        assert!(tyres.len() > 400, "{} tyres", tyres.len());
        let wall = circuit.track.half_width + circuit.track.verge + BARRIER;
        for tyre in tyres {
            let distance = land.distance_to_track(tyre.translation.xz());
            assert!(distance < wall && distance > wall - 1.5, "a tyre {distance} m from the centre line");
            assert!(tyre.translation.y > 0.0 && tyre.translation.y < 0.5);
            // Flat: the tyre's axis points up.
            assert!((tyre.rotation * Vec3::Z).y.abs() > 0.99);
        }
    }

    #[test]
    fn more_detail_places_more_and_the_same_every_time() {
        let count = |scatters: &[Scatter]| scatters.iter().map(|s| s.places.len()).sum::<usize>();
        let (some, ..) = roadside(Detail::Some);
        let (full, ..) = roadside(Detail::Full);
        assert!(count(&full) > 4 * count(&some), "{} against {}", count(&full), count(&some));
        let (again, ..) = roadside(Detail::Full);
        assert_eq!(count(&full), count(&again));
        assert_eq!(full[3].places, again[3].places, "the same circuit gives the same roadside");
        assert!(full.iter().all(|s| !s.places.is_empty()), "every kind found room");
    }
}
