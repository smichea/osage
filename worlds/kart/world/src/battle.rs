// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Battles (G4.2, D-065): the event `battle:ARENA`. The karts — drivers and
//! bots — start spread round an arena, a wide loop with crates all the way
//! round, each with three lives. Each thing that hits a kart, and each
//! bumper that knocks it, takes one; a kart is spared for a moment after a
//! hit, so that a stack of tyres takes one life, not three. A kart with no
//! life left is out: it coasts, and nothing touches it or is touched by it.
//! The last kart in wins, or the one with most lives when time is up; the
//! others are placed by their lives, then by how long they lasted.

use std::time::Duration;

use osage_kart_physics::Kart;

use crate::Circuit;

/// Lives a kart starts a battle with.
pub const LIVES: u32 = 3;
/// How long a battle lasts at most.
pub const LONGEST: Duration = Duration::from_secs(150);
/// After a hit, a kart cannot lose another life for this long.
pub const SPARED: Duration = Duration::from_millis(1500);

/// A kart's battle: lives left, when it was last hit, when it went out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fight {
    pub lives: u32,
    hit: Option<Duration>,
    pub out: Option<Duration>,
}

impl Default for Fight {
    fn default() -> Self {
        Self { lives: LIVES, hit: None, out: None }
    }
}

impl Fight {
    /// A hit at `now`: a life lost, unless the kart is spared or out
    /// already. Whether it lost one.
    pub fn hit(&mut self, now: Duration) -> bool {
        let spared = self.hit.is_some_and(|hit| now < hit + SPARED);
        if self.lives == 0 || spared {
            return false;
        }
        self.lives -= 1;
        self.hit = Some(now);
        if self.lives == 0 {
            self.out = Some(now);
        }
        true
    }

    pub fn is_out(&self) -> bool {
        self.lives == 0
    }
}

/// Where kart `slot` of `seats` starts in an arena: spread evenly round
/// it, on the centre line, facing the way round.
pub fn spot(circuit: &Circuit, slot: usize, seats: usize) -> Kart {
    let track = &circuit.track;
    let along = circuit.start + slot as f32 * track.length() / seats.max(1) as f32;
    let ([x, z], heading) = track.at(along.rem_euclid(track.length()));
    Kart { x, z, heading, ..Kart::default() }
}

/// The order of a battle, best first: the indices of `fights`, by lives,
/// then by how long each lasted (still in beats any time), then by index.
pub fn ranking(fights: &[Fight]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..fights.len()).collect();
    order.sort_by(|&a, &b| {
        let lasted = |f: &Fight| f.out.unwrap_or(Duration::MAX);
        fights[b].lives.cmp(&fights[a].lives).then(lasted(&fights[b]).cmp(&lasted(&fights[a]))).then(a.cmp(&b))
    });
    order
}

/// Whether the battle is over: one kart or none left in (of several), no
/// driver left in when there are drivers, or time is up.
pub fn over(fights: &[(Fight, bool)], elapsed: Duration) -> bool {
    let in_ = |f: &&(Fight, bool)| !f.0.is_out();
    let left = fights.iter().filter(in_).count();
    let humans = fights.iter().filter(|f| f.1).count();
    let humans_in = fights.iter().filter(|f| f.1).filter(in_).count();
    (fights.len() > 1 && left <= 1) || (humans > 0 && humans_in == 0) || elapsed >= LONGEST
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: fn(u64) -> Duration = Duration::from_secs;

    #[test]
    fn a_hit_takes_a_life_and_a_moment_spares_the_next() {
        let mut fight = Fight::default();
        assert!(fight.hit(S(1)));
        assert!(!fight.hit(S(1) + Duration::from_millis(400)), "spared");
        assert!(fight.hit(S(3)) && fight.hit(S(5)));
        assert!(fight.is_out() && fight.out == Some(S(5)));
        assert!(!fight.hit(S(9)), "out is out");
    }

    #[test]
    fn the_battle_ranks_by_lives_then_by_how_long_each_lasted() {
        let with = |lives, out| Fight { lives, hit: None, out };
        let fights = [with(0, Some(S(30))), with(2, None), with(0, Some(S(50))), with(2, None), with(1, None)];
        assert_eq!(ranking(&fights), [1, 3, 4, 2, 0]);
        let tagged = |f: &[Fight], humans: &[usize]| {
            f.iter().enumerate().map(|(i, f)| (*f, humans.contains(&i))).collect::<Vec<_>>()
        };
        assert!(!over(&tagged(&fights, &[1]), S(10)), "three still in, the driver among them");
        assert!(over(&tagged(&fights, &[0]), S(10)), "the only driver is out");
        assert!(over(&tagged(&fights, &[1]), LONGEST), "time is up");
        let last = [with(0, Some(S(5))), with(1, None)];
        assert!(over(&tagged(&last, &[]), S(10)), "one left");
    }

    #[test]
    fn karts_start_spread_round_the_arena() {
        let arena = Circuit::of(&osage_kart_tracks::arena("yard").unwrap());
        let spots: Vec<Kart> = (0..6).map(|i| spot(&arena, i, 6)).collect();
        for (i, a) in spots.iter().enumerate() {
            for b in &spots[i + 1..] {
                assert!((a.x - b.x).hypot(a.z - b.z) > 20.0, "apart: {a:?} {b:?}");
            }
        }
    }
}
