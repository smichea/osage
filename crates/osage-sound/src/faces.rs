// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The faces sound meets, kept so that a way is looked for among those near
//! it and not among all of them, and made from meshes as an object gives
//! them:
//!
//! - **an index**: a tree of boxes over the faces that stay where they are
//!   (a bounding volume hierarchy, split at the middle face along the
//!   longest side), and beside it the few that move, looked at each time. A
//!   way asks for the faces near a segment ([`Faces::along`]) or those a way
//!   of a given length by them could reach ([`Faces::within`]); what it
//!   finds among them is what it would find among all of them, to the last
//!   bit, since the way search never depends on the order faces are given
//!   in;
//! - **sheets**: faces of one node that share a side and lie in one plane —
//!   the triangles of a mesh, or the quads made of them — are one surface.
//!   Their shared sides are not edges: sound does not bend round them, a
//!   reflection crosses them from one face to the next, and a straight way
//!   is stopped by exactly one of the two ([`sheets`]). A sheet mirrors
//!   from its plane, by the faces the mirror point is on, and scatters once
//!   from its point nearest the mirror point: however finely it is cut, it
//!   costs and sounds as one face;
//! - **quads from triangles**: two triangles of a mesh that share a side,
//!   lie in one plane and make a convex quad become that quad
//!   ([`quads`]): half the faces to look at.

use std::{collections::HashMap, ops::Deref};

use crate::{
    space::{Vec3, vec3},
    ways::Face,
};

/// Boxes to a leaf of a tree, at most.
const LEAF: usize = 4;

#[derive(Clone, Debug)]
struct Node {
    lo: Vec3,
    hi: Vec3,
    /// A leaf: `count` boxes of `order` from `first`; otherwise the two
    /// children, the first just after it and the second at `first`.
    first: usize,
    count: usize,
}

/// Boxes, numbered, and a tree over those that stay (a bounding volume
/// hierarchy, split at the middle box along the longest side); those that
/// move are kept beside it and looked at each time.
#[derive(Clone, Debug, Default)]
struct Boxes {
    bounds: Vec<[Vec3; 2]>,
    /// The boxes that stay, in the order of the tree's leaves.
    order: Vec<usize>,
    nodes: Vec<Node>,
    loose: Vec<usize>,
}

impl Boxes {
    fn new(bounds: Vec<[Vec3; 2]>, moving: impl Fn(usize) -> bool) -> Boxes {
        let (order, loose) = (0..bounds.len()).partition(|&i| !moving(i));
        let mut boxes = Boxes { bounds, order, nodes: Vec::new(), loose };
        if !boxes.order.is_empty() {
            let count = boxes.order.len();
            boxes.build(0, count);
        }
        boxes
    }

    fn build(&mut self, first: usize, count: usize) -> usize {
        let at = self.nodes.len();
        let mut whole =
            [vec3(f64::INFINITY, f64::INFINITY, f64::INFINITY), vec3(-f64::INFINITY, -f64::INFINITY, -f64::INFINITY)];
        for &i in &self.order[first..first + count] {
            whole = union(whole, self.bounds[i]);
        }
        let [lo, hi] = whole;
        self.nodes.push(Node { lo, hi, first, count });
        if count <= LEAF {
            return at;
        }
        // Split at the middle box along the longest side.
        let size = hi - lo;
        let axis = if size.x >= size.y && size.x >= size.z {
            0
        } else if size.y >= size.z {
            1
        } else {
            2
        };
        let centre = |b: &[Vec3; 2]| (b[0].array()[axis] + b[1].array()[axis]) / 2.0;
        let bounds = &self.bounds;
        self.order[first..first + count]
            .sort_unstable_by(|&a, &b| centre(&bounds[a]).total_cmp(&centre(&bounds[b])).then(a.cmp(&b)));
        let half = count / 2;
        self.build(first, half);
        let second = self.build(first + half, count - half);
        self.nodes[at].first = second;
        self.nodes[at].count = 0;
        at
    }

    fn each(&self, test: impl Fn(Vec3, Vec3) -> bool, visit: &mut impl FnMut(usize)) {
        for &i in &self.loose {
            let [lo, hi] = self.bounds[i];
            if test(lo, hi) {
                visit(i);
            }
        }
        if self.nodes.is_empty() {
            return;
        }
        let mut pending = vec![0usize];
        while let Some(at) = pending.pop() {
            let node = &self.nodes[at];
            if !test(node.lo, node.hi) {
                continue;
            }
            if node.count > 0 {
                for &i in &self.order[node.first..node.first + node.count] {
                    let [lo, hi] = self.bounds[i];
                    if test(lo, hi) {
                        visit(i);
                    }
                }
            } else {
                pending.push(at + 1);
                pending.push(node.first);
            }
        }
    }

    fn along(&self, a: Vec3, b: Vec3, radius: f64, mut visit: impl FnMut(usize)) {
        let r = vec3(radius, radius, radius);
        self.each(|lo, hi| meets(a, b, lo - r, hi + r), &mut visit);
    }

    fn within(&self, a: Vec3, b: Vec3, length: f64, mut visit: impl FnMut(usize)) {
        self.each(|lo, hi| outside(a, lo, hi) + outside(b, lo, hi) <= length, &mut visit);
    }
}

fn union(a: [Vec3; 2], b: [Vec3; 2]) -> [Vec3; 2] {
    [
        vec3(a[0].x.min(b[0].x), a[0].y.min(b[0].y), a[0].z.min(b[0].z)),
        vec3(a[1].x.max(b[1].x), a[1].y.max(b[1].y), a[1].z.max(b[1].z)),
    ]
}

/// The faces of a scene and their index: a tree over the faces, and a tree
/// over the sheets.
#[derive(Clone, Debug, Default)]
pub struct Faces {
    list: Vec<Face>,
    faces: Boxes,
    /// The sheets' boxes, by their place among [`Faces::sheets`].
    sheets: Boxes,
    /// The faces of each sheet, by its place.
    members: Vec<Vec<usize>>,
    /// Of each sheet, its edges: the sides where it ends, those along one
    /// line end to end made one.
    edges: HashMap<usize, Vec<Edge>>,
    /// The first face of each sheet, in their order.
    roots: Vec<usize>,
}

impl Deref for Faces {
    type Target = [Face];
    fn deref(&self) -> &[Face] {
        &self.list
    }
}

impl Faces {
    /// `list`, indexed but for the faces `moving` names (a face is taken to
    /// stay when `moving` does not reach it).
    pub fn new(mut list: Vec<Face>, moving: &[bool]) -> Faces {
        for (i, face) in list.iter_mut().enumerate() {
            if face.sheet == usize::MAX {
                face.sheet = i;
            }
        }
        let roots: Vec<usize> = (0..list.len()).filter(|&i| list[i].sheet == i).collect();
        let mut members = vec![Vec::new(); roots.len()];
        for (i, face) in list.iter().enumerate() {
            members[roots.binary_search(&face.sheet).expect("a sheet's first face is its own")].push(i);
        }
        let edges = edges(&list);
        let moves = |i: usize| moving.get(i).copied().unwrap_or(false);
        let faces = Boxes::new(list.iter().map(Face::bounds).collect(), moves);
        let whole = |m: &Vec<usize>| m.iter().map(|&i| faces.bounds[i]).reduce(union).expect("a sheet has a face");
        // A sheet's faces are on one node: they all move, or none.
        let sheets = Boxes::new(members.iter().map(whole).collect(), |slot| moves(roots[slot]));
        Faces { list, faces, sheets, members, edges, roots }
    }

    /// Puts a face that moves where it now is.
    pub fn update(&mut self, i: usize, mut face: Face) {
        debug_assert!(self.faces.loose.contains(&i), "only a face that moves is moved");
        if face.sheet == usize::MAX {
            face.sheet = i;
        }
        self.faces.bounds[i] = face.bounds();
        let slot = self.slot(face.sheet);
        self.list[i] = face;
        self.sheets.bounds[slot] =
            self.members[slot].iter().map(|&k| self.faces.bounds[k]).reduce(union).expect("a sheet has a face");
    }

    /// The edges of the sheet `sheet`, where they are now: each its two
    /// ends and the face it starts on. A sheet's sides along one line end
    /// to end are one edge: a road cut into a thousand strips has the four
    /// edges it had whole.
    pub fn edges(&self, sheet: usize) -> impl Iterator<Item = (Vec3, Vec3, usize)> + '_ {
        let at = |(face, corner): (usize, usize)| self.list[face].corners[corner];
        self.edges.get(&sheet).into_iter().flatten().map(move |edge| (at(edge.from), at(edge.to), edge.from.0))
    }

    /// Whether the sheet `sheet` is one face.
    pub fn alone(&self, sheet: usize) -> bool {
        self.members[self.slot(sheet)].len() == 1
    }

    /// The sheets, each by its first face, in their order.
    pub fn sheets(&self) -> &[usize] {
        &self.roots
    }

    /// Where the sheet `sheet` is among [`Faces::sheets`].
    pub fn slot(&self, sheet: usize) -> usize {
        self.roots.binary_search(&sheet).expect("a sheet is named by its first face")
    }

    /// The faces that move.
    pub fn moving(&self) -> &[usize] {
        &self.faces.loose
    }

    /// Calls `visit` with each face whose box comes within `radius` of the
    /// segment from `a` to `b` — and perhaps a few more; in no order.
    pub fn along(&self, a: Vec3, b: Vec3, radius: f64, visit: impl FnMut(usize)) {
        self.faces.along(a, b, radius, visit);
    }

    /// Calls `visit` with each face by which a way from `a` to `b` could be
    /// no longer than `length` — and perhaps a few more; in no order.
    pub fn within(&self, a: Vec3, b: Vec3, length: f64, visit: impl FnMut(usize)) {
        self.faces.within(a, b, length, visit);
    }

    /// [`Faces::along`] for sheets: each by its first face.
    pub fn sheets_along(&self, a: Vec3, b: Vec3, radius: f64, mut visit: impl FnMut(usize)) {
        self.sheets.along(a, b, radius, |slot| visit(self.roots[slot]));
    }

    /// [`Faces::within`] for sheets: each by its first face.
    pub fn sheets_within(&self, a: Vec3, b: Vec3, length: f64, mut visit: impl FnMut(usize)) {
        self.sheets.within(a, b, length, |slot| visit(self.roots[slot]));
    }
}

/// An edge of a sheet, from a corner of a face to a corner of a face (each
/// a face and the corner's number): it moves with them.
#[derive(Clone, Copy, Debug)]
struct Edge {
    from: (usize, usize),
    to: (usize, usize),
}

/// The edges of each sheet among `faces`: its open sides, in the order of
/// its faces and their sides, each joined to the open sides that go on
/// along its line from either end.
fn edges(faces: &[Face]) -> HashMap<usize, Vec<Edge>> {
    // The open sides, and those that start or end at each corner of a sheet.
    let sides: Vec<(usize, usize)> =
        (0..faces.len()).flat_map(|i| (0..4).filter(move |&s| faces[i].open[s]).map(move |s| (i, s))).collect();
    let ends = |(i, s): (usize, usize)| (faces[i].corners[s], faces[i].corners[(s + 1) % 4]);
    let mut at: HashMap<(usize, [i64; 3]), Vec<usize>> = HashMap::new();
    for (n, &(i, s)) in sides.iter().enumerate() {
        let (a, b) = ends((i, s));
        at.entry((faces[i].sheet, key(a))).or_default().push(n);
        at.entry((faces[i].sheet, key(b))).or_default().push(n);
    }
    let mut used = vec![false; sides.len()];
    let mut out: HashMap<usize, Vec<Edge>> = HashMap::new();
    for n in 0..sides.len() {
        if used[n] {
            continue;
        }
        used[n] = true;
        let (i, s) = sides[n];
        let sheet = faces[i].sheet;
        let (a, b) = ends((i, s));
        let along = (b - a).unit_or(Vec3::X);
        let mut from = (i, s);
        let mut to = (i, (s + 1) % 4);
        // Goes on from `end` along the line: the next open side there, if
        // one lies on it; returns its far corner.
        let extend = |end: (usize, usize), used: &mut Vec<bool>| -> Option<(usize, usize)> {
            let p = faces[end.0].corners[end.1];
            for &m in at.get(&(sheet, key(p)))? {
                if used[m] {
                    continue;
                }
                let (j, t) = sides[m];
                let (c, d) = ends((j, t));
                let dir = (d - c).unit_or(Vec3::X);
                if dir.cross(along).length() > 1e-9 {
                    continue;
                }
                used[m] = true;
                return Some(if key(c) == key(p) { (j, (t + 1) % 4) } else { (j, t) });
            }
            None
        };
        while let Some(next) = extend(to, &mut used) {
            to = next;
        }
        while let Some(next) = extend(from, &mut used) {
            from = next;
        }
        out.entry(sheet).or_default().push(Edge { from, to });
    }
    out
}

/// How far a point is from a box, m: 0 inside it.
fn outside(p: Vec3, lo: Vec3, hi: Vec3) -> f64 {
    let d = |x: f64, lo: f64, hi: f64| (lo - x).max(0.0).max(x - hi);
    vec3(d(p.x, lo.x, hi.x), d(p.y, lo.y, hi.y), d(p.z, lo.z, hi.z)).length()
}

/// Whether the segment from `a` to `b` meets the closed box.
fn meets(a: Vec3, b: Vec3, lo: Vec3, hi: Vec3) -> bool {
    let (mut enter, mut exit) = (0.0f64, 1.0f64);
    let (a, d, lo, hi) = (a.array(), (b - a).array(), lo.array(), hi.array());
    for axis in 0..3 {
        if d[axis].abs() < 1e-300 {
            if a[axis] < lo[axis] || a[axis] > hi[axis] {
                return false;
            }
        } else {
            let (s, t) = ((lo[axis] - a[axis]) / d[axis], (hi[axis] - a[axis]) / d[axis]);
            enter = enter.max(s.min(t));
            exit = exit.min(s.max(t));
            if enter > exit {
                return false;
            }
        }
    }
    true
}

/// A corner as a key: to 10 µm.
fn key(p: Vec3) -> [i64; 3] {
    p.array().map(|x| (x * 1e5).round() as i64)
}

/// Of each of `faces` (each on the node `node[i]`, in that node's frame):
/// the sides that end its surface, and its sheet (the number of the sheet's
/// first face). Two faces of one node that share a side, lie in one plane
/// and are `alike` (the same material and the same flags: a pane beside a
/// wall leaves the wall its edge) are of one sheet, and that side is shut
/// in both.
pub fn sheets(faces: &[[Vec3; 4]], node: &[usize], alike: impl Fn(usize, usize) -> bool) -> Sheets {
    const NOTHING: crate::scene::Material =
        crate::scene::Material { absorption: [0.0; 3], transmission: [0.0; 3], scattering: 0.0 };
    let made: Vec<Face> = faces.iter().map(|&corners| Face::new(corners, NOTHING, false, false)).collect();
    let mut open: Vec<[bool; 4]> = made.iter().map(|face| face.open).collect();
    // By node and corners: the faces and which of their sides.
    type Side = (usize, [i64; 3], [i64; 3]);
    let mut sides: HashMap<Side, Vec<(usize, usize)>> = HashMap::new();
    for (i, face) in made.iter().enumerate() {
        for side in (0..4).filter(|&side| face.open[side]) {
            let (a, b) = (key(face.corners[side]), key(face.corners[(side + 1) % 4]));
            sides.entry((node[i], a.min(b), a.max(b))).or_default().push((i, side));
        }
    }
    // Sheets, by joining faces (the smallest face of each stands for it).
    let mut sheet: Vec<usize> = (0..faces.len()).collect();
    fn root(sheet: &mut [usize], mut i: usize) -> usize {
        while sheet[i] != i {
            sheet[i] = sheet[sheet[i]];
            i = sheet[i];
        }
        i
    }
    for meeting in sides.values() {
        for (n, &(i, si)) in meeting.iter().enumerate() {
            for &(j, sj) in &meeting[n + 1..] {
                // In one plane, and on either side of the side they share:
                // the two faces of a panel seen from both sides are not one
                // sheet but two, and its sides are edges.
                if i != j
                    && alike(i, j)
                    && made[i].normal.dot(made[j].normal).abs() > 1.0 - 1e-9
                    && made[i].inward(si).dot(made[j].inward(sj)) < 0.0
                {
                    open[i][si] = false;
                    open[j][sj] = false;
                    let (a, b) = (root(&mut sheet, i), root(&mut sheet, j));
                    sheet[a.max(b)] = a.min(b);
                }
            }
        }
    }
    let of = (0..faces.len()).map(|i| root(&mut sheet, i)).collect();
    Sheets { open, of }
}

/// What [`sheets`] tells of each face.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sheets {
    pub open: Vec<[bool; 4]>,
    pub of: Vec<usize>,
}

/// The faces of a mesh's triangles, `corners` indexed by `triangles`: two
/// triangles that share a side, lie in one plane (within a micrometre) and
/// make a convex quad become that quad; the rest stay triangles (a quad
/// whose last two corners are one). In the order of the triangles, each
/// quad where its first triangle was.
pub fn quads(corners: &[Vec3], triangles: &[[u32; 3]]) -> Vec<[Vec3; 4]> {
    let at = |i: u32| corners[i as usize];
    let normal = |t: &[u32; 3]| (at(t[1]) - at(t[0])).cross(at(t[2]) - at(t[0]));
    let mut sides: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (n, t) in triangles.iter().enumerate() {
        for s in 0..3 {
            let (a, b) = (t[s], t[(s + 1) % 3]);
            sides.entry((a.min(b), a.max(b))).or_default().push(n);
        }
    }
    let mut used = vec![false; triangles.len()];
    let mut out = Vec::with_capacity(triangles.len());
    for (n, t) in triangles.iter().enumerate() {
        if used[n] {
            continue;
        }
        used[n] = true;
        let mut made = None;
        'sides: for s in 0..3 {
            let (a, b, c) = (t[s], t[(s + 1) % 3], t[(s + 2) % 3]);
            let Some(others) = sides.get(&(a.min(b), a.max(b))) else { continue };
            for &m in others {
                if used[m] || m == n {
                    continue;
                }
                let o = &triangles[m];
                let Some(&d) = o.iter().find(|&&v| v != a && v != b) else { continue };
                // The fourth corner on the first triangle's plane.
                let up = normal(t).unit_or(Vec3::Y);
                if (at(d) - at(a)).dot(up).abs() > 1e-6 || normal(o).length() < 1e-12 {
                    continue;
                }
                // Round the quad: a, d, b, c — the shared side a-b is its diagonal.
                let quad = [at(a), at(d), at(b), at(c)];
                if convex(&quad, up) {
                    used[m] = true;
                    made = Some(quad);
                    break 'sides;
                }
            }
        }
        out.push(made.unwrap_or([at(t[0]), at(t[1]), at(t[2]), at(t[2])]));
    }
    out
}

/// Whether four corners round a quad of the plane whose normal is `up` make
/// it convex, with no three on one line.
fn convex(quad: &[Vec3; 4], up: Vec3) -> bool {
    let turn = |i: usize| (quad[(i + 1) % 4] - quad[i]).cross(quad[(i + 2) % 4] - quad[(i + 1) % 4]).dot(up);
    let turns = [turn(0), turn(1), turn(2), turn(3)];
    turns.iter().all(|&t| t > 1e-12) || turns.iter().all(|&t| t < -1e-12)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_triangles_of_a_square_make_the_square_and_a_bent_pair_does_not() {
        let corners = [vec3(0.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), vec3(1.0, 0.0, 1.0), vec3(0.0, 0.0, 1.0)];
        let quads_of = quads(&corners, &[[0, 1, 2], [0, 2, 3]]);
        assert_eq!(quads_of.len(), 1);
        let face = Face::new(
            quads_of[0],
            crate::scene::Material { absorption: [0.0; 3], transmission: [0.0; 3], scattering: 0.0 },
            true,
            true,
        );
        assert!((face.area() - 1.0).abs() < 1e-12);
        let mut bent = corners;
        bent[3].y = 0.5;
        assert_eq!(quads(&bent, &[[0, 1, 2], [0, 2, 3]]).len(), 2);
    }

    #[test]
    fn faces_of_one_plane_that_share_a_side_are_one_sheet() {
        let square = |x: f64, y: f64| {
            [vec3(x, y, 0.0), vec3(x + 1.0, y, 0.0), vec3(x + 1.0, y + 1.0, 0.0), vec3(x, y + 1.0, 0.0)]
        };
        // Two squares side by side; a third at right angles on the far side; a fourth on another node.
        let wall = [vec3(2.0, 0.0, 0.0), vec3(2.0, 0.0, 1.0), vec3(2.0, 1.0, 1.0), vec3(2.0, 1.0, 0.0)];
        let all = [square(0.0, 0.0), square(1.0, 0.0), wall, square(0.0, 1.0)];
        let Sheets { open, of } = sheets(&all, &[0, 0, 0, 1], |_, _| true);
        assert_eq!(of, [0, 0, 2, 3]);
        assert_eq!(open[0], [true, false, true, true]);
        assert_eq!(open[1], [true, true, true, false]);
        assert_eq!(open[2], [true; 4]);
        assert_eq!(open[3], [true; 4]);
        // Two faces that are not alike keep their edge.
        let Sheets { open, of } = sheets(&all, &[0, 0, 0, 1], |i, j| i.min(j) != 0 || i.max(j) != 1);
        assert_eq!((open[0], of[1]), ([true; 4], 1));
    }

    #[test]
    fn a_strip_cut_in_ten_has_the_four_edges_it_had_whole() {
        let hard = crate::scene::Material { absorption: [0.0; 3], transmission: [0.0; 3], scattering: 0.0 };
        let cells: Vec<[Vec3; 4]> = (0..10)
            .map(|i| {
                let x = i as f64;
                [vec3(x, 0.0, 0.0), vec3(x + 1.0, 0.0, 0.0), vec3(x + 1.0, 0.0, 1.0), vec3(x, 0.0, 1.0)]
            })
            .collect();
        let Sheets { open, of } = sheets(&cells, &[0; 10], |_, _| true);
        let list: Vec<Face> = (0..10)
            .map(|i| {
                let mut face = Face::new(cells[i], hard, true, true);
                face.open = open[i];
                face.sheet = of[i];
                face
            })
            .collect();
        let faces = Faces::new(list, &[]);
        assert_eq!(faces.sheets(), [0]);
        assert!(!faces.alone(0));
        let mut edges: Vec<f64> = faces.edges(0).map(|(a, b, _)| a.distance(b)).collect();
        edges.sort_by(f64::total_cmp);
        assert_eq!(edges, [1.0, 1.0, 10.0, 10.0]);
    }

    #[test]
    fn the_index_finds_what_a_search_of_all_finds() {
        let hard = crate::scene::Material { absorption: [0.0; 3], transmission: [0.0; 3], scattering: 0.0 };
        let mut list = Vec::new();
        for i in 0..200 {
            let (x, z) = ((i % 20) as f64 * 3.0, (i / 20) as f64 * 3.0);
            list.push(Face::new(
                [vec3(x, 0.0, z), vec3(x + 1.0, 0.0, z), vec3(x + 1.0, 2.0, z), vec3(x, 2.0, z)],
                hard,
                true,
                true,
            ));
        }
        let moving: Vec<bool> = (0..200).map(|i| i % 17 == 0).collect();
        let faces = Faces::new(list.clone(), &moving);
        let (a, b) = (vec3(-5.0, 1.0, 4.0), vec3(70.0, 1.5, 20.0));
        for radius in [0.0, 1.0, 6.0] {
            let mut found = Vec::new();
            faces.along(a, b, radius, |i| found.push(i));
            found.sort_unstable();
            let all: Vec<usize> = (0..200)
                .filter(|&i| {
                    let [lo, hi] = list[i].bounds();
                    meets(a, b, lo - vec3(radius, radius, radius), hi + vec3(radius, radius, radius))
                })
                .collect();
            assert_eq!(found, all);
        }
        let mut found = Vec::new();
        faces.within(a, b, 90.0, |i| found.push(i));
        found.sort_unstable();
        let all: Vec<usize> = (0..200)
            .filter(|&i| {
                let [lo, hi] = list[i].bounds();
                outside(a, lo, hi) + outside(b, lo, hi) <= 90.0
            })
            .collect();
        assert_eq!(found, all);
        assert!(all.len() < 200);
    }
}
