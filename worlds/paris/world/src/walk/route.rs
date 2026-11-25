// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{RADIUS, STEP, Walk};
use bevy::math::{Vec2, Vec3};
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, VecDeque},
};

type Cell = (usize, usize);

const AXIAL: u64 = 1_000_000;
const DIAGONAL: u64 = 1_414_214;
const NEIGHBOURS: [(isize, isize, u64); 8] = [
    (-1, 0, AXIAL),
    (1, 0, AXIAL),
    (0, -1, AXIAL),
    (0, 1, AXIAL),
    (-1, -1, DIAGONAL),
    (-1, 1, DIAGONAL),
    (1, -1, DIAGONAL),
    (1, 1, DIAGONAL),
];

/// Search positions on the height field. Refinement only changes the positions
/// tried by the planner; terrain and the swept-disc test use the original grid.
#[derive(Clone, Copy)]
struct Lattice {
    origin: Vec2,
    spacing: f32,
    offset: f32,
    columns: usize,
    rows: usize,
}

impl Lattice {
    fn point(self, (x, z): Cell) -> Vec2 {
        self.origin + (Vec2::new(x as f32, z as f32) + self.offset) * self.spacing
    }

    fn nearest(self, p: Vec2) -> Cell {
        let index = (p - self.origin) / self.spacing + (0.5 - self.offset);
        ((index.x as usize).min(self.columns - 1), (index.y as usize).min(self.rows - 1))
    }

    fn refined(self) -> Option<Self> {
        Some(Self {
            spacing: self.spacing * 0.5,
            offset: 1.,
            columns: self.columns.checked_mul(2)?.checked_sub(1)?,
            rows: self.rows.checked_mul(2)?.checked_sub(1)?,
            ..self
        })
    }
}

impl Walk {
    /// Test the entire swept footprint on each interval of constant centre-cell height.
    pub(crate) fn clear_segment(&self, from: Vec2, to: Vec2) -> bool {
        if !from.is_finite() || !to.is_finite() {
            return false;
        }
        let (Some(a), Some(b)) = (self.index(from), self.index(to)) else { return false };
        if a.0.abs_diff(b.0) <= 1
            && a.1.abs_diff(b.1) <= 1
            && self.clear_cell(a)
            && self.clear_cell(b)
            && self.clear_cell((a.0, b.1))
            && self.clear_cell((b.0, a.1))
        {
            return true;
        }
        let Some(start) = self.at(a.0, a.1) else { return false };
        if a == b {
            return self.supports_sweep(from, to, a, start);
        }
        let Some(end) = self.at(b.0, b.1) else { return false };
        // Each axis yields its grid crossings in travel order. Merge equal
        // crossings at corners and stop as soon as the footprint loses support.
        let crossings = |axis: usize| {
            let grid = |p: Vec2| (f64::from(p[axis]) - f64::from(self.origin[axis])) / f64::from(self.cell);
            let start = grid(from);
            let finish = grid(to);
            let first = start.floor() as i64;
            let last = finish.floor() as i64;
            (0..first.abs_diff(last)).map(move |i| {
                let edge = if first < last { first + 1 + i as i64 } else { first - i as i64 };
                ((edge as f64 - start) / (finish - start)) as f32
            })
        };
        let mut x = crossings(0);
        let mut z = crossings(1);
        let mut next_x = x.next().unwrap_or(1.);
        let mut next_z = z.next().unwrap_or(1.);
        let mut t = 0.;
        let mut previous = start;
        while t < 1. {
            let next = next_x.min(next_z);
            if next == next_x {
                next_x = x.next().unwrap_or(1.);
            }
            if next == next_z {
                next_z = z.next().unwrap_or(1.);
            }
            if next <= t {
                continue;
            }
            let a = from.lerp(to, t);
            let b = from.lerp(to, next);
            t = next;
            let Some(cell) = self.index(a.midpoint(b)) else { return false };
            let Some(h) = self.at(cell.0, cell.1) else { return false };
            if (h - previous).abs() > STEP || !self.supports_sweep(a, b, cell, h) {
                return false;
            }
            previous = h;
        }
        (end - previous).abs() <= STEP && self.supports_sweep(to, to, b, end)
    }

    /// With a fixed supporting height, every cell under the moving disc must support it.
    pub(super) fn supports_sweep(&self, a: Vec2, b: Vec2, centre: Cell, floor: f32) -> bool {
        let lo = a.min(b) - RADIUS;
        let hi = a.max(b) + RADIUS;
        let upper = self.origin + Vec2::new(self.columns as f32, self.rows as f32) * self.cell;
        if lo.cmplt(self.origin).any() || hi.cmpgt(upper).any() {
            return false;
        }
        let lo_cell = ((lo - self.origin) / self.cell).floor().as_uvec2();
        let hi_cell = ((hi - self.origin) / self.cell).floor().as_uvec2();
        // On grid boundaries, a rounded endpoint can put a sliver of the
        // footprint beyond the centre cell's certified neighbourhood.
        if self.clear_cell(centre)
            && lo_cell.x as usize >= centre.0 - 1
            && hi_cell.x as usize <= centre.0 + 1
            && lo_cell.y as usize >= centre.1 - 1
            && hi_cell.y as usize <= centre.1 + 1
        {
            return true;
        }
        for z in lo_cell.y as usize..(hi_cell.y as usize + 1).min(self.rows) {
            for x in lo_cell.x as usize..(hi_cell.x as usize + 1).min(self.columns) {
                if self.at(x, z).is_some_and(|h| (h - floor).abs() <= STEP) {
                    continue;
                }
                let corner = self.origin + Vec2::new(x as f32, z as f32) * self.cell;
                if disc_hits_cell(a, b, corner, corner + self.cell) {
                    return false;
                }
            }
        }
        true
    }

    fn route_cell(&self, lattice: Lattice, p: Vec2) -> Option<Cell> {
        let home = lattice.nearest(p);
        if self.clear_segment(p, lattice.point(home)) {
            return Some(home);
        }
        // An off-centre position can support the full disc even when the centre
        // of its own cell does not. Join it to a visible neighbouring centre.
        let mut nearby = Vec::with_capacity(8);
        for dz in -1..=1 {
            for dx in -1..=1 {
                let (Some(x), Some(z)) = (home.0.checked_add_signed(dx), home.1.checked_add_signed(dz)) else {
                    continue;
                };
                if (x, z) != home && x < lattice.columns && z < lattice.rows {
                    nearby.push((x, z));
                }
            }
        }
        nearby.sort_by(|&a, &b| p.distance_squared(lattice.point(a)).total_cmp(&p.distance_squared(lattice.point(b))));
        nearby.into_iter().find(|&c| self.clear_segment(p, lattice.point(c)))
    }

    /// A route between exact horizontal endpoints, including both. Uses the
    /// installed walker's footprint and the height field's eight-neighbour graph.
    pub fn route(&self, from: Vec2, to: Vec2) -> Option<Vec<Vec3>> {
        if !from.is_finite() || !to.is_finite() || !self.holds(from) || !self.holds(to) {
            return None;
        }
        let on_ground = |p: Vec2| Some(Vec3::new(p.x, self.ground(p)?, p.y));
        if from == to {
            return Some(vec![on_ground(from)?]);
        }
        if self.clear_segment(from, to) {
            return Some(vec![on_ground(from)?, on_ground(to)?]);
        }
        let lattice =
            Lattice { origin: self.origin, spacing: self.cell, offset: 0.5, columns: self.columns, rows: self.rows };
        self.search(lattice, from, to).or_else(|| {
            if !self.connected_floor(self.index(from)?, self.index(to)?) {
                return None;
            }
            self.search(lattice.refined()?, from, to)
        })
    }

    /// Every successful sweep visits a chain of touching ground cells within
    /// stepping height. If even a point cannot connect these cells, refining
    /// the disc's search positions cannot open a route.
    fn connected_floor(&self, source: Cell, target: Cell) -> bool {
        let mut seen = vec![0_u64; self.heights.len().div_ceil(64)];
        let mark = |seen: &mut [u64], (x, z): Cell| {
            let i = z * self.columns + x;
            let bit = 1 << (i % 64);
            let fresh = seen[i / 64] & bit == 0;
            seen[i / 64] |= bit;
            fresh
        };
        mark(&mut seen, source);
        let mut open = VecDeque::from([source]);
        while let Some((x, z)) = open.pop_front() {
            if (x, z) == target {
                return true;
            }
            let h = self.heights[z * self.columns + x];
            for (dx, dz, _) in NEIGHBOURS {
                let (Some(nx), Some(nz)) = (x.checked_add_signed(dx), z.checked_add_signed(dz)) else { continue };
                if nx < self.columns
                    && nz < self.rows
                    && (self.heights[nz * self.columns + nx] - h).abs() <= STEP
                    && mark(&mut seen, (nx, nz))
                {
                    open.push_back((nx, nz));
                }
            }
        }
        false
    }

    fn search(&self, lattice: Lattice, from: Vec2, to: Vec2) -> Option<Vec<Vec3>> {
        let on_ground = |p: Vec2| Some(Vec3::new(p.x, self.ground(p)?, p.y));
        let source = self.route_cell(lattice, from)?;
        let target = self.route_cell(lattice, to)?;
        let estimate = |p: Cell| {
            let dx = p.0.abs_diff(target.0) as u64;
            let dz = p.1.abs_diff(target.1) as u64;
            dx.min(dz) * DIAGONAL + dx.abs_diff(dz) * AXIAL
        };
        // Order ties by remaining distance and cell coordinates for repeatable paths.
        let mut open = BinaryHeap::from([Reverse((estimate(source), estimate(source), source))]);
        let mut seen = HashMap::from([(source, (0, source))]);
        while let Some(Reverse((total, remaining, at))) = open.pop() {
            let cost = total - remaining;
            if cost != seen[&at].0 {
                continue;
            }
            if at == target {
                let mut path = vec![to, lattice.point(at)];
                let mut cursor = at;
                while cursor != source {
                    cursor = seen[&cursor].1;
                    path.push(lattice.point(cursor));
                }
                path.push(from);
                path.reverse();
                path.dedup();
                // Keep the last directly reachable point before each obstruction.
                let mut result = vec![on_ground(from)?];
                let mut anchor = from;
                for pair in path[1..].windows(2) {
                    if !self.clear_segment(anchor, pair[1]) {
                        result.push(on_ground(pair[0])?);
                        anchor = pair[0];
                    }
                }
                result.push(on_ground(to)?);
                return Some(result);
            }
            for (dx, dz, length) in NEIGHBOURS {
                let (Some(x), Some(z)) = (at.0.checked_add_signed(dx), at.1.checked_add_signed(dz)) else { continue };
                let next = (x, z);
                let next_cost = cost + length;
                if x >= lattice.columns
                    || z >= lattice.rows
                    || seen.get(&next).is_some_and(|&(old, _)| old <= next_cost)
                {
                    continue;
                }
                if self.clear_segment(lattice.point(at), lattice.point(next)) {
                    seen.insert(next, (next_cost, at));
                    let h = estimate(next);
                    open.push(Reverse((next_cost + h, h, next)));
                }
            }
        }
        None
    }
}

/// The segment is inside a single centre cell. For any other grid square the
/// closest pair includes a segment endpoint or a square corner. Its interior
/// cannot be crossed between these two centre-cell boundary events.
fn disc_hits_cell(a: Vec2, b: Vec2, lo: Vec2, hi: Vec2) -> bool {
    let radius_sq = RADIUS * RADIUS;
    if [a, b].into_iter().any(|p| p.distance_squared(p.clamp(lo, hi)) < radius_sq) {
        return true;
    }
    let v = b - a;
    let vv = v.length_squared();
    if vv == 0. {
        return false;
    }
    [lo, hi, Vec2::new(lo.x, hi.y), Vec2::new(hi.x, lo.y)].into_iter().any(|corner| {
        let t = ((corner - a).dot(v) / vv).clamp(0., 1.);
        corner.distance_squared(a + v * t) < radius_sq
    })
}
