// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

fn street(cols: usize, rows: usize, floor: impl Fn(usize, usize) -> bool) -> Walk {
    let heights = (0..rows)
        .flat_map(|z| (0..cols).map(move |x| (x, z)))
        .map(|(x, z)| if floor(x, z) { 0. } else { f32::NAN })
        .collect();
    Walk::new(0.25, cols, rows, Vec2::ZERO, heights, Vec3::ZERO)
}

fn replay(mut grid: Walk, ends: [Vec2; 2], limit: u32) -> ([usize; 2], [bool; 2]) {
    let patrols = [ends, [ends[1], ends[0]]];
    grid.patrols = Some(patrols.map(|p| p.map(|end| end.to_array())).to_vec());
    let mut app = app(grid);
    let grid = app.world().resource::<Grid>().0.clone();
    let actors = pair(&mut app);
    let mut visits = [0; 2];
    let mut yielded = [false; 2];
    let mut before = ends;
    for tick in 1..=limit {
        app.update();
        let after = actors.map(|id| app.world().get::<Transform>(id).unwrap().translation.xz());
        for i in 0..2 {
            assert!(grid.clear_segment(before[i], after[i]), "outside floor at tick {tick}");
            assert!(before[i].distance(after[i]) <= WALK / TICK_HZ as f32 + 1e-5);
            yielded[i] |= app.world().get::<Patrol>(actors[i]).unwrap().refuge.is_some();
            if after[i].distance(patrols[i][1 - visits[i] % 2]) < ARRIVAL {
                visits[i] += 1;
            }
        }
        let d = before[0].as_dvec2() - before[1].as_dvec2();
        let travel = after[0].as_dvec2() - after[1].as_dvec2() - d;
        let t =
            if travel.length_squared() > 0. { (-d.dot(travel) / travel.length_squared()).clamp(0., 1.) } else { 0. };
        assert!((d + t * travel).length() >= 2. * f64::from(RADIUS) - 1e-5, "overlap at tick {tick}");
        if visits.iter().all(|n| *n >= 2) {
            break;
        }
        before = after;
    }
    (visits, yielded)
}

#[test]
fn walkers_use_a_passing_place_on_a_straight_or_bent_route() {
    let straight = street(56, 24, |x, z| !(16..40).contains(&x) || (11..13).contains(&z));
    let bend = |width: usize| {
        street(64, 64, |x, z| {
            (x < 16 && z < 24)
                || ((16..32 + width).contains(&x) && (11..11 + width).contains(&z))
                || ((32..32 + width).contains(&x) && (11..40).contains(&z))
                || ((20..48).contains(&x) && (40..60).contains(&z))
        })
    };
    let ends = |offset: f32| [Vec2::new(2., 2.75 + offset), Vec2::new(8. + offset, 12.)];
    let too_narrow = bend(1);
    let [a, b] = ends(0.125);
    assert!(too_narrow.route(a, b).is_none() && too_narrow.route(b, a).is_none());
    // At 50 cm the centreline falls on a grid edge. At 75 cm it falls on cell centres.
    for (grid, ends) in
        [(straight, [Vec2::new(2., 3.), Vec2::new(12., 3.)]), (bend(2), ends(0.25)), (bend(3), ends(0.375))]
    {
        let (visits, yielded) = replay(grid, ends, 1800);
        assert!(visits.iter().all(|n| *n >= 2), "{visits:?}");
        assert!(yielded[1], "the second resident must give way on the first encounter");
    }
}

#[test]
fn a_patrol_starting_inside_can_back_out_beyond_its_endpoint() {
    let grid = street(24, 56, |x, z| !(16..40).contains(&z) || (11..13).contains(&x));
    let (visits, yielded) = replay(grid, [Vec2::new(3., 2.), Vec2::new(3., 9.)], 1800);
    assert!(visits.iter().all(|n| *n >= 2), "{visits:?}");
    assert!(yielded[1], "the second resident must give way on the first encounter");
}

#[test]
fn a_passage_without_any_room_to_pass_keeps_the_two_bodies_apart() {
    let grid = street(56, 24, |_, z| (11..13).contains(&z));
    assert_eq!(replay(grid, [Vec2::new(2., 3.), Vec2::new(12., 3.)], 450), ([0, 0], [false, false]));
}
