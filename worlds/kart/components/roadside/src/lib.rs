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
use osage_props::{Scatter, Use, Woods, poly_haven};

/// The models, from the asset manifest.
#[derive(Clone, Default)]
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
    pub fir: Handle<Gltf>,
    pub broadleaf: Handle<Gltf>,
}

impl Models {
    /// Only the trees, for [`woods`]: the other handles load nothing.
    pub fn trees(server: &AssetServer) -> Self {
        let model = |id| poly_haven(server, id);
        Self {
            pine: model("pine_sapling_small"),
            fir: model("fir_sapling_medium"),
            broadleaf: model("island_tree_02"),
            ..Self::default()
        }
    }

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
            fir: model("fir_sapling_medium"),
            broadleaf: model("island_tree_02"),
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

/// Where things may grow along the circuit.
struct Verge<'a> {
    land: &'a Land,
    frames: &'a [Frame],
    /// The barrier line, from the centre line.
    roadside: f32,
    clearings: &'a [Site],
}

impl Verge<'_> {
    /// Whether a plant may grow here: clear of the barrier by `margin`, not
    /// in the lake or on its beach, not in a clearing.
    fn free(&self, at: Vec2, margin: f32) -> bool {
        self.land.distance_to_track(at) > self.roadside + margin
            && !self.land.wet(at)
            && !self.clearings.iter().any(|site| site.centre.distance(at) < site.radius)
    }

    /// A point beside the track, `low` to `high` metres beyond the barrier,
    /// on either side: plants are where they are seen. Anywhere along the
    /// track, not only at its frames, or plants would stand in rows.
    fn beside(&self, dice: &mut Dice, low: f32, high: f32) -> Vec2 {
        let frames = self.frames;
        let i = (dice.roll() * frames.len() as f32) as usize % frames.len();
        let (frame, next) = (&frames[i], &frames[(i + 1) % frames.len()]);
        let t = dice.roll();
        let (at, right) = (frame.at.lerp(next.at, t), frame.right.lerp(next.right, t).normalize_or_zero());
        let side = if dice.roll() < 0.5 { -1.0 } else { 1.0 };
        at + right * side * (self.roadside + dice.between(low, high))
    }

    fn stand(&self, at: Vec2, sink: f32, turn: f32, size: f32) -> Transform {
        Transform::from_xyz(at.x, self.land.height(at) - sink, at.y)
            .with_rotation(Quat::from_rotation_y(turn))
            .with_scale(Vec3::splat(size))
    }
}

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
    let verge = Verge { land, frames, roadside, clearings };
    let mut dice = Dice(11);
    let free = |at: Vec2, margin: f32| verge.free(at, margin);
    let beside = |dice: &mut Dice, low: f32, high: f32| verge.beside(dice, low, high);
    let stand = |at: Vec2, sink: f32, turn: f32, size: f32| verge.stand(at, sink, turn, size);
    // Dense models seen far (trees, bushes, rocks) get coarser meshes with distance.
    let scatter = |model: &Handle<Gltf>, places: Vec<Transform>, using: Use, within: f32, shadows: bool| Scatter {
        model: model.clone(),
        places,
        using,
        within: Some(within),
        shadows,
        lods: within >= 60.0,
        tint: None,
        opaque: false,
        lit_as_ground: false,
    };

    let mut all = vec![scatter(&models.tyre, tyre_walls(frames, track), Use::Whole, 160.0, true)];

    // Pines in groves near the circuit, three sizes of sapling grown tall.
    let mut pines = Vec::new();
    for _ in 0..4000 {
        let at = beside(&mut dice, 8.0, 140.0);
        let (turn, size, chance) = (dice.between(0.0, std::f32::consts::TAU), dice.between(4.0, 7.5), dice.roll());
        if chance < (noise(at / 45.0 + 3.0) - 0.4) * 1.8 && free(at, 8.0) && pines.len() < 300 {
            // A clump: one tall sapling and a few smaller ones close by, as
            // young pines grow, which reads as one full crown from afar.
            pines.push(stand(at, 0.1, turn, size));
            for _ in 0..(2 + (dice.roll() * 3.0) as usize) {
                let near = at + Vec2::from_angle(dice.between(0.0, std::f32::consts::TAU)) * dice.between(0.6, 1.6);
                if free(near, 7.0) {
                    let turn = dice.between(0.0, std::f32::consts::TAU);
                    pines.push(stand(near, 0.1, turn, size * dice.between(0.55, 0.85)));
                }
            }
        }
    }
    all.push(scatter(&models.pine, pines, Use::Variants, 450.0, true));

    // Firs and broadleaf trees, full grown, among the pines.
    let (mut firs, mut broadleaves) = (Vec::new(), Vec::new());
    for _ in 0..3000 {
        let at = beside(&mut dice, 9.0, 150.0);
        let (turn, roll, chance) = (dice.between(0.0, std::f32::consts::TAU), dice.roll(), dice.roll());
        if chance > (noise(at / 50.0 + 21.0) - 0.38) * 1.6 || !free(at, 9.0) {
            continue;
        }
        if roll < 0.62 && firs.len() < 120 {
            firs.push(stand(at, 0.15, turn, dice.between(0.85, 1.25)));
        } else if roll >= 0.62 && broadleaves.len() < 70 {
            broadleaves.push(stand(at, 0.15, turn, dice.between(1.6, 2.4)));
        }
    }
    // Their leaves are geometry, marked blended with textures without alpha: drawn opaque.
    all.push(Scatter { opaque: true, ..scatter(&models.fir, firs, Use::Variants, 500.0, true) });
    all.push(Scatter { opaque: true, ..scatter(&models.broadleaf, broadleaves, Use::Whole, 500.0, true) });

    // The woods further off are not models but pictures of them: [`woods`].

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
        let tufts = scatter(&models.grass, grass, Use::Variants, 45.0, false);
        all.push(Scatter { tint: Some(GRASS), lit_as_ground: true, ..tufts });

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
/// Grass lit as the ground: the meadow's green, a little yellower at the tips' height.
const GRASS: Color = Color::srgb(0.36, 0.42, 0.17);

/// Grass around the camera (`osage_props::Carpet`): mown short on the
/// verges, tall beyond the wall; none on the road, the kerbs, the wall, in a
/// gravel trap or in the lake.
/// Nearer than this to the track's centre line, no tree of the woods: they
/// are cards, and must not be seen close. Where nothing else can stand in
/// for them (T2), they may come nearer: `woods` takes the distance.
pub const WOODS_FROM: f32 = 110.0;

/// The woods off the circuit, out to the edge of the modelled land: groves
/// of pines, firs and broadleaf trees, dense as woods grow (a tree every
/// twenty square metres or so), drawn as cards ([`osage_props::Woods`]):
/// thousands of trees, always at least `from` metres from the camera
/// ([`WOODS_FROM`] where modelled trees stand nearer).
pub fn woods(
    land: &Land,
    frames: &[Frame],
    track: &Track,
    clearings: &[Site],
    models: &Models,
    from: f32,
) -> Vec<Woods> {
    let roadside = track.half_width + track.verge + BARRIER;
    let verge = Verge { land, frames, roadside, clearings };
    let mut dice = Dice(23);
    let edge = osage_kart_land::LAND / 2.0 - 20.0;
    // One tree per cell of a grid at most: trees stand apart.
    const SPACING: f32 = 3.6;
    let mut taken = std::collections::HashSet::new();
    let (mut pines, mut firs, mut broadleaves) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..600 {
        let centre = verge.beside(&mut dice, (from - roadside + 10.0).max(0.0), 430.0);
        let radius = dice.between(14.0, 45.0);
        if noise(centre / 90.0 + 5.0) < 0.42 {
            continue;
        }
        let count = (std::f32::consts::PI * radius * radius / 20.0) as usize;
        for _ in 0..count {
            let at = centre + Vec2::from_angle(dice.between(0.0, std::f32::consts::TAU)) * radius * dice.roll().sqrt();
            // The species by place: stands where pines grow, others of
            // broadleaf trees, firs between, never quite one alone.
            let (turn, roll) = (dice.between(0.0, std::f32::consts::TAU), dice.roll());
            let kind = (0.65 * noise(at / 140.0 + 31.0) + 0.35 * roll).clamp(0.0, 1.0) * 1.25 - 0.12;
            let cell = (at / SPACING).floor().as_ivec2();
            if (at - land.centre()).abs().max_element() > edge
                || land.distance_to_track(at) < from
                || !verge.free(at, 12.0)
                || !taken.insert(cell)
            {
                continue;
            }
            // Pines are saplings grown tall; firs and broadleaf trees full
            // grown. Woods mix ages: mostly grown trees, some young ones,
            // a few old giants above the canopy.
            let age = 0.6 + 0.9 * dice.roll().powf(1.6);
            if kind < 0.45 {
                pines.push(verge.stand(at, 0.2, turn, dice.between(5.5, 9.0) * age));
            } else if kind < 0.85 {
                firs.push(verge.stand(at, 0.2, turn, dice.between(0.9, 1.35) * age));
            } else {
                broadleaves.push(verge.stand(at, 0.2, turn, dice.between(1.7, 2.5) * age));
            }
        }
    }
    vec![
        Woods { model: models.pine.clone(), using: Use::Variants, places: pines },
        Woods { model: models.fir.clone(), using: Use::Variants, places: firs },
        Woods { model: models.broadleaf.clone(), using: Use::Whole, places: broadleaves },
    ]
}

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
        lit_as_ground: true,
    }
}

/// Clumps of tall grass in the meadows beyond the wall, one in each metre
/// out to 36 m around the camera: the meadow has relief where the near
/// carpet ends, before its texture alone carries it.
pub fn meadow_carpet(land: &Land, track: &Track, models: &Models) -> osage_props::Carpet {
    let (land, wall) = (land.clone(), track.half_width + track.verge + BARRIER);
    let ground = move |at: Vec2| -> Option<(f32, f32)> {
        let distance = land.distance_to_track(at);
        (distance > wall + 1.5 && !land.wet(at)).then(|| (land.height(at) - 0.03, 3.4))
    };
    osage_props::Carpet {
        model: models.grass.clone(),
        using: Use::Variants,
        spacing: 1.0,
        radius: 36.0,
        ground: std::sync::Arc::new(ground),
        tint: Some(GRASS),
        lit_as_ground: true,
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
            fir: Handle::default(),
            broadleaf: Handle::default(),
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
    fn woods_are_dense_far_and_on_the_land() {
        let circuit = Circuit::new();
        let frames = frames(&circuit.track);
        let land = Land::new(&frames, &circuit.track);
        let models = Models::default();
        let woods = woods(&land, &frames, &circuit.track, &[], &models, WOODS_FROM);
        let trees: usize = woods.iter().map(|w| w.places.len()).sum();
        assert!(trees > 1500, "{trees} trees");
        assert!(woods.iter().all(|w| !w.places.is_empty()), "pines, firs and broadleaf trees");
        let mut cells = std::collections::HashSet::new();
        for place in woods.iter().flat_map(|w| &w.places) {
            let at = place.translation.xz();
            assert!(land.distance_to_track(at) >= WOODS_FROM, "{at} is too near the track");
            assert!(!land.wet(at), "{at} is in the lake");
            assert!((at - land.centre()).abs().max_element() <= osage_kart_land::LAND / 2.0, "{at} is off the land");
            assert!((place.translation.y - land.height(at)).abs() < 0.3, "{at} stands on the ground");
            assert!(cells.insert((at / 3.6).floor().as_ivec2()), "{at}: two trees in one spot");
        }
        // Species grow by place: around a pine, more pines than in the
        // woods as a whole; around a broadleaf tree, more broadleaf trees.
        let all: Vec<(usize, Vec2)> =
            woods.iter().enumerate().flat_map(|(k, w)| w.places.iter().map(move |p| (k, p.translation.xz()))).collect();
        for species in [0, 2] {
            let share = all.iter().filter(|(k, _)| *k == species).count() as f32 / all.len() as f32;
            let (mut same, mut near) = (0usize, 0usize);
            for (_, at) in all.iter().filter(|(k, _)| *k == species).step_by(7) {
                for (k, other) in &all {
                    if other.distance(*at) < 25.0 && other != at {
                        near += 1;
                        same += (*k == species) as usize;
                    }
                }
            }
            let local = same as f32 / near as f32;
            assert!(local > 1.25 * share, "species {species}: {local} near one, {share} in all");
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
