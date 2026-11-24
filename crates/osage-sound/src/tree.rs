// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The object tree a sound is placed in: nodes, each with an identifier, a
//! parent and a placement relative to it — the tree of the object format
//! (`design/08-object-format.md`), which draws and collides from the same
//! nodes. A source is named by its node's path, a list of identifiers from a
//! root, and placed by a position local to that node: when a parent moves or
//! turns, everything under it follows, sound included.
//!
//! A node may move along a straight way at a speed that changes with time:
//! here that stands for the simulation, which moves nodes in the engine.

use crate::space::{Placement, Quat, Vec3};

/// A node's motion along a straight way in its parent's frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Motion {
    /// The way it goes, one metre long, in the parent's frame.
    pub along: Vec3,
    /// The instant, s, at which the node is at its placement at rest.
    pub at: f64,
    /// Its speed, m/s, at instants counted from `at`, s, soonest first:
    /// between two of them the speed changes evenly, before the first and
    /// after the last it holds.
    pub speed: Vec<(f64, f64)>,
}

impl Motion {
    /// A steady speed, m/s.
    pub fn steady(along: Vec3, at: f64, speed: f64) -> Self {
        Self { along: along.unit_or(Vec3::X), at, speed: vec![(0.0, speed)] }
    }

    /// Speed at the instant `t`, m/s.
    pub fn speed(&self, t: f64) -> f64 {
        let t = t - self.at;
        let (Some(first), Some(last)) = (self.speed.first(), self.speed.last()) else { return 0.0 };
        if t <= first.0 {
            return first.1;
        }
        if t >= last.0 {
            return last.1;
        }
        let i = self.speed.partition_point(|knot| knot.0 <= t);
        let (a, b) = (self.speed[i - 1], self.speed[i]);
        a.1 + (b.1 - a.1) * (t - a.0) / (b.0 - a.0)
    }

    /// Acceleration at the instant `t`, m/s².
    pub fn accel(&self, t: f64) -> f64 {
        let t = t - self.at;
        let i = self.speed.partition_point(|knot| knot.0 <= t);
        if i == 0 || i == self.speed.len() {
            return 0.0;
        }
        let (a, b) = (self.speed[i - 1], self.speed[i]);
        (b.1 - a.1) / (b.0 - a.0)
    }

    /// Metres gone since the first instant of the speeds, counted from `at`.
    fn gone(&self, t: f64) -> f64 {
        let Some(first) = self.speed.first() else { return 0.0 };
        if t <= first.0 {
            return first.1 * (t - first.0);
        }
        let mut metres = 0.0;
        for pair in self.speed.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if t <= b.0 {
                let v = a.1 + (b.1 - a.1) * (t - a.0) / (b.0 - a.0);
                return metres + (a.1 + v) / 2.0 * (t - a.0);
            }
            metres += (a.1 + b.1) / 2.0 * (b.0 - a.0);
        }
        let last = self.speed[self.speed.len() - 1];
        metres + last.1 * (t - last.0)
    }

    /// How far along its way the node is from its placement at rest, m, at
    /// the instant `t`.
    pub fn travelled(&self, t: f64) -> f64 {
        self.gone(t - self.at) - self.gone(0.0)
    }
}

/// A node turning on itself at a steady rate: a wheel, a roundabout. What
/// is placed in it goes round with it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turning {
    /// What it turns about, one metre long, in its own frame.
    pub axis: Vec3,
    /// The instant, s, at which it is turned as its placement at rest says.
    pub at: f64,
    /// Radians a second, counter-clockwise seen from the axis' tip.
    pub rate: f64,
}

/// A node of the tree.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Unique among the nodes (`node.extras.osage.id`).
    pub id: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Where it is in its parent — at the instant `motion.at` if it moves.
    pub rest: Placement,
    pub motion: Option<Motion>,
    pub turning: Option<Turning>,
    /// From this far from the listener, m, the node's own emitters stand
    /// for its whole branch; nearer, the branch's own are heard: the cut
    /// through the tree, for sound as for the eye (`subtreeProxy`).
    pub proxy_beyond: Option<f64>,
}

/// A forest of nodes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tree {
    pub nodes: Vec<Node>,
}

impl Tree {
    /// Where the node is in its parent at the instant `t`.
    pub fn local(&self, node: usize, t: f64) -> Placement {
        let n = &self.nodes[node];
        let mut placement = n.rest;
        if let Some(motion) = &n.motion {
            placement.translation += motion.along * motion.travelled(t);
        }
        if let Some(turning) = &n.turning {
            placement.rotation = placement.rotation.after(Quat::about(turning.axis, turning.rate * (t - turning.at)));
        }
        placement
    }

    /// Where the node is in the world at the instant `t`: the product of
    /// the placements on its path, root first.
    pub fn placement(&self, node: usize, t: f64) -> Placement {
        let local = self.local(node, t);
        match self.nodes[node].parent {
            Some(parent) => self.placement(parent, t).then(&local),
            None => local,
        }
    }

    /// The nodes with every parent before its children: the order in which
    /// their placements in the world follow from one another.
    pub fn order(&self) -> Vec<usize> {
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut ahead: Vec<usize> = (0..self.nodes.len()).rev().filter(|&i| self.nodes[i].parent.is_none()).collect();
        while let Some(node) = ahead.pop() {
            order.push(node);
            ahead.extend(self.nodes[node].children.iter().rev());
        }
        order
    }

    /// Where every node is in the world at the instant `t`, into `placed`
    /// (one for each node): each from its parent's, in `order`
    /// ([`Tree::order`]).
    pub fn place_all(&self, t: f64, order: &[usize], placed: &mut Vec<Placement>) {
        placed.resize(self.nodes.len(), Placement::default());
        for &node in order {
            let local = self.local(node, t);
            placed[node] = match self.nodes[node].parent {
                Some(parent) => placed[parent].then(&local),
                None => local,
            };
        }
    }

    /// A point given in the node's frame, in the world at the instant `t`.
    pub fn point(&self, node: usize, local: Vec3, t: f64) -> Vec3 {
        self.placement(node, t).point(local)
    }

    /// The identifiers from a root down to the node.
    pub fn path(&self, node: usize) -> Vec<&str> {
        let mut path = vec![self.nodes[node].id.as_str()];
        let mut at = node;
        while let Some(parent) = self.nodes[at].parent {
            path.push(self.nodes[parent].id.as_str());
            at = parent;
        }
        path.reverse();
        path
    }

    /// The path as one line, its identifiers joined by ` / `.
    pub fn path_text(&self, node: usize) -> String {
        self.path(node).join(" / ")
    }

    /// The node at `path`, identifiers from a root.
    pub fn find<S: AsRef<str>>(&self, path: &[S]) -> Option<usize> {
        let mut found: Option<usize> = None;
        for id in path {
            let id = id.as_ref();
            found = Some(match found {
                None => (0..self.nodes.len()).find(|&i| self.nodes[i].parent.is_none() && self.nodes[i].id == id)?,
                Some(parent) => *self.nodes[parent].children.iter().find(|&&c| self.nodes[c].id == id)?,
            });
        }
        found
    }

    /// Whether `node` is `ancestor` or under it.
    pub fn under(&self, node: usize, ancestor: usize) -> bool {
        let mut at = Some(node);
        while let Some(i) = at {
            if i == ancestor {
                return true;
            }
            at = self.nodes[i].parent;
        }
        false
    }

    /// The nearest node that moves, from `node` up; its way in the world.
    pub fn mover(&self, node: usize) -> Option<usize> {
        let mut at = Some(node);
        while let Some(i) = at {
            if self.nodes[i].motion.is_some() {
                return Some(i);
            }
            at = self.nodes[i].parent;
        }
        None
    }

    /// Speed and acceleration, m/s and m/s², of what carries the node at
    /// the instant `t`: those of the nearest node that moves, from it up.
    pub fn going(&self, node: usize, t: f64) -> (f64, f64) {
        match self.mover(node).and_then(|i| self.nodes[i].motion.as_ref()) {
            Some(motion) => (motion.speed(t), motion.accel(t)),
            None => (0.0, 0.0),
        }
    }

    /// The way the node travels in the world, one metre long, if anything
    /// carries it.
    pub fn heading(&self, node: usize, t: f64) -> Option<Vec3> {
        let mover = self.mover(node)?;
        let along = self.nodes[mover].motion.as_ref()?.along;
        Some(match self.nodes[mover].parent {
            Some(parent) => self.placement(parent, t).direction(along),
            None => along,
        })
    }

    /// The nearest node from `node` up, itself left out, whose emitters
    /// stand for its branch from afar, and from how far.
    pub fn proxy_over(&self, node: usize) -> Option<(usize, f64)> {
        let mut at = self.nodes[node].parent;
        while let Some(i) = at {
            if let Some(beyond) = self.nodes[i].proxy_beyond {
                return Some((i, beyond));
            }
            at = self.nodes[i].parent;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::vec3;

    fn node(id: &str, parent: Option<usize>, at: Vec3) -> Node {
        Node {
            id: id.to_owned(),
            parent,
            children: Vec::new(),
            rest: Placement { translation: at, rotation: Quat::IDENTITY },
            motion: None,
            turning: None,
            proxy_beyond: None,
        }
    }

    /// bridge > deck > (train > bogie, walker).
    fn bridge() -> Tree {
        let mut nodes = vec![
            node("bridge", None, vec3(100.0, 0.0, 0.0)),
            node("deck", Some(0), vec3(0.0, 16.0, 0.0)),
            node("train", Some(1), vec3(-20.0, 0.0, -1.5)),
            node("bogie", Some(2), vec3(5.0, 0.0, 0.0)),
            node("walker", Some(1), vec3(0.0, -8.0, -11.0)),
        ];
        nodes[0].children = vec![1];
        nodes[1].children = vec![2, 4];
        nodes[2].children = vec![3];
        nodes[2].motion =
            Some(Motion { along: -Vec3::X, at: 10.0, speed: vec![(-10.0, 0.0), (0.0, 10.0), (5.0, 10.0)] });
        nodes[2].proxy_beyond = Some(120.0);
        Tree { nodes }
    }

    #[test]
    fn a_source_is_found_by_its_path_and_placed_by_its_parents() {
        let tree = bridge();
        let bogie = tree.find(&["bridge", "deck", "train", "bogie"]).unwrap();
        assert_eq!(tree.path(bogie), ["bridge", "deck", "train", "bogie"]);
        assert_eq!(tree.path_text(bogie), "bridge / deck / train / bogie");
        assert!(tree.find(&["bridge", "train"]).is_none());
        assert!(tree.find(&["deck"]).is_none());
        // At the instant the train is at rest in its motion: the sum of the path.
        assert_eq!(tree.point(bogie, vec3(0.0, 0.3, 0.0), 10.0), vec3(85.0, 16.3, -1.5));
        assert!(tree.under(bogie, 1) && !tree.under(1, bogie));
        assert_eq!(tree.proxy_over(bogie), Some((2, 120.0)));
        assert_eq!(tree.proxy_over(2), None);
    }

    #[test]
    fn moving_or_turning_a_parent_takes_what_is_under_it() {
        let mut tree = bridge();
        let bogie = tree.find(&["bridge", "deck", "train", "bogie"]).unwrap();
        let before = tree.point(bogie, Vec3::ZERO, 10.0);
        tree.nodes[0].rest.translation.x += 7.0;
        assert_eq!(tree.point(bogie, Vec3::ZERO, 10.0), before + vec3(7.0, 0.0, 0.0));
        // The bridge turned a quarter about the vertical: what was along X is along −Z.
        tree.nodes[0].rest =
            Placement { translation: Vec3::ZERO, rotation: Quat::about(Vec3::Y, std::f64::consts::FRAC_PI_2) };
        let p = tree.point(bogie, Vec3::ZERO, 10.0);
        assert!(p.distance(vec3(-1.5, 16.0, 15.0)) < 1e-9, "{p:?}");
        let heading = tree.heading(bogie, 10.0).unwrap();
        assert!(heading.distance(Vec3::Z) < 1e-9, "{heading:?}");
    }

    #[test]
    fn what_is_placed_in_a_turning_node_goes_round_with_it() {
        let mut tree = bridge();
        // The deck turns a quarter a second about the vertical; the walker is 11 m from its axis.
        tree.nodes[1].turning = Some(Turning { axis: Vec3::Y, at: 2.0, rate: std::f64::consts::FRAC_PI_2 });
        let walker = tree.find(&["bridge", "deck", "walker"]).unwrap();
        assert_eq!(tree.point(walker, Vec3::ZERO, 2.0), vec3(100.0, 8.0, -11.0));
        // A second later: what was at −Z of the axis is at −X of it.
        let p = tree.point(walker, Vec3::ZERO, 3.0);
        assert!(p.distance(vec3(89.0, 8.0, 0.0)) < 1e-9, "{p:?}");
        // And its own directions turn with it.
        assert!(tree.placement(walker, 3.0).direction(Vec3::X).distance(-Vec3::Z) < 1e-9);
    }

    #[test]
    fn all_at_once_is_each_by_itself() {
        let mut tree = bridge();
        tree.nodes[1].turning = Some(Turning { axis: Vec3::Y, at: 0.0, rate: 0.3 });
        let order = tree.order();
        assert_eq!(order, [0, 1, 2, 3, 4]);
        let mut placed = Vec::new();
        tree.place_all(7.5, &order, &mut placed);
        assert_eq!(placed.len(), tree.nodes.len());
        for (node, together) in placed.iter().enumerate() {
            let alone = tree.placement(node, 7.5);
            assert!(together.translation.distance(alone.translation) < 1e-9);
            assert!(together.direction(Vec3::X).distance(alone.direction(Vec3::X)) < 1e-12);
        }
    }

    #[test]
    fn a_node_goes_as_far_as_its_speed_says() {
        let tree = bridge();
        let motion = tree.nodes[2].motion.clone().unwrap();
        // Ten seconds from rest to 10 m/s is 50 m; then 10 m/s.
        assert_eq!(motion.speed(0.0), 0.0);
        assert_eq!(motion.speed(5.0), 5.0);
        assert_eq!(motion.accel(5.0), 1.0);
        assert_eq!(motion.speed(12.0), 10.0);
        assert_eq!(motion.accel(12.0), 0.0);
        assert_eq!(motion.speed(99.0), 10.0);
        assert!((motion.travelled(10.0)).abs() < 1e-12);
        assert!((motion.travelled(0.0) + 50.0).abs() < 1e-12);
        assert!((motion.travelled(5.0) + 37.5).abs() < 1e-12);
        assert!((motion.travelled(13.0) - 30.0).abs() < 1e-12);
        assert!((motion.travelled(20.0) - 100.0).abs() < 1e-12);
        // Before its first instant it holds its first speed: at rest.
        assert!((motion.travelled(-30.0) + 50.0).abs() < 1e-12);
        // In the world: along −X of the deck.
        let bogie = tree.find(&["bridge", "deck", "train", "bogie"]).unwrap();
        assert!((tree.point(bogie, Vec3::ZERO, 13.0).x - (85.0 - 30.0)).abs() < 1e-9);
        assert_eq!(tree.going(bogie, 5.0), (5.0, 1.0));
        assert_eq!(tree.going(4, 5.0), (0.0, 0.0));
        let steady = Motion::steady(vec3(0.0, 0.0, 2.0), 3.0, 1.5);
        assert!((steady.travelled(5.0) - 3.0).abs() < 1e-12 && steady.along == Vec3::Z);
    }
}
