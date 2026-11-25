// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The engine: a scene heard block by block.
//!
//! Every 128 samples (2.7 ms, a Web Audio render quantum) it asks the tree
//! where everything is, finds the ways each emitter's sound takes to the head
//! ([`ways`](crate::ways)) and how each reaches the ears
//! ([`ears`](crate::ears)); between two blocks every delay and gain moves
//! evenly, sample by sample, so that nothing steps.
//!
//! Each emitter's voice writes its sound into a line of its own, a few
//! seconds long. A way is that line read some time back — the time sound
//! takes over the way's length: an emitter is heard late by its distance,
//! and one that comes nearer is read faster, so it sounds higher (Doppler)
//! with no rule for it; a mirrored way reads the same line further back, with
//! a shift of its own. What is read then goes through four bands (what
//! distance, air, an edge or a material leave of each), the outer ear's
//! echoes, and for each ear its delay and the head's shadow.
//!
//! What is too quiet to matter is not made: the ways are ranked by how loud
//! they arrive and only the loudest are rendered ([`Quality::ways`]); a voice
//! none of whose ways is rendered sleeps; from afar a node's own emitters
//! stand for its whole branch (the tree's cut). A contact — a foot, a wheel
//! on a joint — wakes a voice from a small pool, placed where the contact is
//! on the node that was touched.

use osage_sound_voices::{Drive, Hit, Voice, dsp::Noise, dsp::seed_for};

use crate::{
    ears::{HEAD_RADIUS, Heard, Listening, PINNA, hear_for},
    faces::{Faces, Sheets, sheets},
    field::{Decoder, Far, MOST, far_share, harmonics},
    scene::{Head, Load, Material, Reach, Scene, Touching},
    space::{Placement, Vec3, vec3},
    tree::Motion,
    ways::{Face, SOUND, Scatter, Way, Ways, air_loss, direct, mirrored},
};

pub use osage_sound_voices::RATE;

/// Samples between two looks at the scene.
pub const BLOCK: usize = 128;
const DT: f64 = 1.0 / RATE as f64;
/// Nothing is heard nearer than this, m: a gain of 1/d must end somewhere.
const NEAREST: f64 = 0.3;
/// Where the four bands part, Hz: between their centres.
const PARTS: [f64; 3] = [500.0, 2_000.0, 6_000.0];
/// How long a contact's voice is kept, s.
const CONTACT_SECONDS: f64 = 2.5;
/// From how far a contact is heard, m.
const CONTACT_REACH: f64 = 150.0;
/// The small lines a way keeps for the outer ear and the two ears, samples.
const SHORT: usize = 64;
/// Under this a sample is taken for nothing: 300 dB under full scale.
const TINY: f32 = 1e-15;
/// A voice that wakes swells in over this many samples: 20 ms.
const WAKE: u32 = 960;

/// How a line is read between two of its samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    /// Two samples: cheap, and duller as it falls between them.
    Linear,
    /// Four samples (Lagrange): flat to a few kilohertz.
    Cubic,
    /// Eight samples of a binomially windowed sinc (seventh-order Lagrange).
    Sinc,
}

/// How much is made: what a device can afford.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quality {
    pub name: &'static str,
    pub reading: Reading,
    /// Whether what faces mirror is followed, each as a way of its own.
    pub mirrors: bool,
    /// When it is not: whether its level is still added to the straight
    /// way's, so that a road is as loud on a weak device.
    pub folded: bool,
    /// Whether a mirrored way is checked for what hides it.
    pub careful: bool,
    /// Whether the outer ear's echoes are made (headphones).
    pub outer_ear: bool,
    pub reverb: bool,
    /// The most ways followed at once: the loudest. One that is dropped is
    /// rendered for one block more, as it fades.
    pub ways: usize,
    /// The ways of an emitter are found anew every so many blocks; between,
    /// each goes on evenly to where it was last found to be going. At 4, the
    /// scene is looked at 94 times a second and heard 11 ms late.
    pub every: u64,
    /// Contact voices sounding at once.
    pub contacts: usize,
    /// Whether every voice is made in full, however far.
    pub full_detail: bool,
    /// A way on which the air takes less than this at 4 kHz, dB, is left as
    /// it is; 0 to give every way its air.
    pub clear_db: f64,
}

impl Quality {
    pub const LOW: Quality = Quality {
        name: "low",
        reading: Reading::Linear,
        mirrors: false,
        folded: true,
        careful: false,
        outer_ear: false,
        reverb: false,
        ways: 16,
        every: 4,
        contacts: 4,
        full_detail: false,
        clear_db: 1.0,
    };
    pub const MEDIUM: Quality = Quality {
        name: "medium",
        reading: Reading::Cubic,
        mirrors: true,
        folded: false,
        careful: false,
        outer_ear: false,
        reverb: true,
        ways: 48,
        every: 2,
        contacts: 6,
        full_detail: false,
        clear_db: 0.5,
    };
    pub const HIGH: Quality = Quality {
        name: "high",
        reading: Reading::Cubic,
        mirrors: true,
        folded: false,
        careful: true,
        outer_ear: true,
        reverb: true,
        ways: 128,
        every: 1,
        contacts: 8,
        full_detail: false,
        clear_db: 0.0,
    };
    pub const REFERENCE: Quality = Quality {
        name: "reference",
        reading: Reading::Sinc,
        mirrors: true,
        folded: false,
        careful: true,
        outer_ear: true,
        reverb: true,
        ways: usize::MAX,
        every: 1,
        contacts: 16,
        full_detail: true,
        clear_db: 0.0,
    };
    pub const ALL: [Quality; 4] = [Quality::LOW, Quality::MEDIUM, Quality::HIGH, Quality::REFERENCE];

    pub fn named(name: &str) -> Option<Quality> {
        Quality::ALL.into_iter().find(|quality| quality.name == name)
    }
}

/// What a render is asked for besides its scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub quality: Quality,
    pub listening: Listening,
    /// The head turned by this much about its vertical, radians, to its
    /// left for a positive angle.
    pub turn: f64,
    /// Only emitters whose name holds one of these are heard, if any.
    pub solo: Vec<String>,
    /// Emitters whose name holds one of these are not heard.
    pub mute: Vec<String>,
    /// The instant of the first sample, s: before 0, to let sound that left
    /// earlier arrive.
    pub start: f64,
    /// Far sound heard as a field made at a cube's centre ([`crate::field`]),
    /// or every emitter heard by its own ways.
    pub far: Option<Far>,
}

impl Options {
    /// Whether what bears `name` is heard: soloed if anything is, and not muted.
    pub fn hears(&self, name: &str) -> bool {
        (self.solo.is_empty() || self.solo.iter().any(|text| name.contains(text.as_str())))
            && !self.mute.iter().any(|text| name.contains(text.as_str()))
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            quality: Quality::REFERENCE,
            listening: Listening::Headphones,
            turn: 0.0,
            solo: Vec::new(),
            mute: Vec::new(),
            start: 0.0,
            far: None,
        }
    }
}

/// What a way is brought to by the end of a block.
#[derive(Clone, Copy, Debug, Default)]
struct Aim {
    /// How far back the line is read, samples.
    delay: f64,
    gains: [f32; 4],
    /// The air's low-pass: how much of the way to a new sample it goes.
    air: f32,
    /// What goes to the reverberation.
    send: f32,
    /// For each ear: how much later it hears, samples, and the head's shadow.
    late: [f32; 2],
    shadow: [f32; 2],
    pan: [f32; 2],
    /// The outer ear's echoes, samples.
    pinna: [f32; 5],
    /// For a field: what the way gives each channel.
    field: [f32; MOST],
}

impl Aim {
    fn new(delay: f64, gains: [f32; 4], air: f32, send: f32, heard: &Heard) -> Aim {
        let rate = RATE as f64;
        Aim {
            delay,
            gains,
            air,
            send,
            // Two samples more: the reader needs them on each side.
            late: heard.delay.map(|seconds| (seconds * rate + 2.0) as f32),
            shadow: heard.shadow.map(|alpha| alpha as f32),
            pan: heard.pan.map(|gain| gain as f32),
            pinna: heard.pinna.map(|seconds| (seconds * rate) as f32),
            field: [0.0; MOST],
        }
    }

    fn silent(self) -> Aim {
        Aim { gains: [0.0; 4], send: 0.0, ..self }
    }

    /// This way with nothing for the ears: only what it feeds the space's
    /// own answer.
    fn feeding(self) -> Aim {
        Aim { gains: [0.0; 4], ..self }
    }

    /// Whether it reaches the ears with something.
    fn sounds(&self) -> bool {
        self.gains != [0.0; 4]
    }

    /// Which of two aims comes first when both are as loud: the one that
    /// arrives sooner, then the one more to the left, then by whatever else
    /// tells them apart. Two that nothing does are heard alike, from the
    /// same emitter.
    fn before(&self, other: &Aim) -> std::cmp::Ordering {
        let rest = |aim: &Aim| {
            let [g0, g1, g2, g3] = aim.gains;
            let ([l0, l1], [s0, s1], [p0, p1, p2, p3, p4]) = (aim.late, aim.shadow, aim.pinna);
            [-g0, -g1, -g2, -g3, aim.air, aim.send, l0, l1, s0, s1, aim.pan[1], p0, p1, p2, p3, p4]
        };
        let (one, two) = (rest(self), rest(other));
        let rest = one.iter().zip(&two).map(|(a, b)| a.total_cmp(b)).find(|order| order.is_ne());
        self.delay
            .total_cmp(&other.delay)
            .then(other.pan[0].total_cmp(&self.pan[0]))
            .then(rest.unwrap_or(std::cmp::Ordering::Equal))
    }

    /// This aim gone `share` of the way to `to`.
    fn toward(&self, to: &Aim, share: f32) -> Aim {
        let mix = |a: f32, b: f32| a + (b - a) * share;
        Aim {
            delay: self.delay + (to.delay - self.delay) * f64::from(share),
            gains: std::array::from_fn(|b| mix(self.gains[b], to.gains[b])),
            air: mix(self.air, to.air),
            send: mix(self.send, to.send),
            late: std::array::from_fn(|e| mix(self.late[e], to.late[e])),
            shadow: std::array::from_fn(|e| mix(self.shadow[e], to.shadow[e])),
            pan: std::array::from_fn(|e| mix(self.pan[e], to.pan[e])),
            pinna: std::array::from_fn(|e| mix(self.pinna[e], to.pinna[e])),
            field: std::array::from_fn(|c| mix(self.field[c], to.field[c])),
        }
    }
}

/// One way of one emitter, between blocks.
#[derive(Clone, Debug)]
struct Line {
    live: bool,
    now: Aim,
    /// The three low-passes that part the bands.
    parts: [f32; 3],
    /// The air: two low-passes one after the other.
    air: [f32; 2],
    /// The head's shadow at each ear: what went in and what came out last.
    shadow_in: [f32; 2],
    shadow_out: [f32; 2],
    /// The last samples the bands let through, and of that with the outer
    /// ear's echoes, oldest first.
    dry: [f32; SHORT],
    wet: [f32; SHORT],
}

impl Default for Line {
    fn default() -> Self {
        Self {
            live: false,
            now: Aim::default(),
            parts: [0.0; 3],
            air: [0.0; 2],
            shadow_in: [0.0; 2],
            shadow_out: [0.0; 2],
            dry: [0.0; SHORT],
            wet: [0.0; SHORT],
        }
    }
}

/// An emitter being heard: a scene's own, or a contact's voice.
struct Source {
    /// What solo and mute are told by: its node's path and its own
    /// identifier — for a contact, what was struck and what struck it.
    #[allow(dead_code)]
    name: String,
    node: usize,
    offset: Vec3,
    gain: f64,
    reach: Reach,
    mirrors: bool,
    max_distance: f64,
    /// The scene's emitter, if it is one.
    emitter: Option<usize>,
    voice: Box<dyn Voice>,
    /// The sound it made, kept for as long as it can take to arrive.
    ring: Vec<f32>,
    /// The straight way first, then one for each face.
    lines: Vec<Line>,
    /// Where each way was last found to be going, and in how many blocks it
    /// is to be there.
    targets: Vec<Option<Aim>>,
    left: u32,
    /// Which blocks its ways are found anew in, of those the quality finds
    /// them in: by its name, so that it is the same whoever else is there.
    turn: u64,
    /// The ways rendered this block: the targets the budget keeps.
    aims: Vec<Option<Aim>>,
    /// How long its sound took to arrive when its ways were last found, s,
    /// by each of them: a way mirrored by a face is longer than the straight
    /// one, and what arrives by it left the emitter earlier, from where the
    /// emitter then was.
    lags: Vec<f64>,
    /// Whether its node or anything above it moves or turns. Where it was
    /// at the end of each of the last blocks is then kept, for as long as
    /// its sound can take to arrive: the newest at the block's own number.
    moves: bool,
    trail: Vec<Vec3>,
    /// How many of them are kept since it was last placed.
    trailed: usize,
    /// The node whose emitters stand for its branch from afar, from how far,
    /// and whether this is one of them (or one of the branch).
    cut: Option<(usize, f64, bool)>,
    heard: bool,
    /// Whether its voice is not being made: nothing of it is heard.
    asleep: bool,
    /// Samples left of the swell a voice wakes with: it starts in the middle
    /// of its sound, which must not arrive as a step.
    waking: u32,
    /// A contact's voice is free once this block is past.
    until: u64,
    /// How loud a contact arrived when it was struck, and how much of that
    /// its voice still makes — the largest sample of its last block against
    /// the largest of any: what the voice is worth keeping against another
    /// contact.
    worth: f32,
    top: f32,
    last: f32,
    /// A contact's first block: its level is there at once.
    struck: bool,
    hit: Option<Hit>,
    load: f32,
    speed: f32,
    accel: f32,
    detail: f32,
}

impl Source {
    /// Where it was `ago` blocks before the end of block `block`, at which
    /// it is at `now`: from what was kept, between two of them — and before
    /// the oldest kept, where it then was. The sound that arrives now left
    /// it then, whatever it has done since.
    fn was(&self, now: Vec3, block: usize, ago: f64) -> Vec3 {
        if self.trailed < 2 {
            return now;
        }
        let kept = self.trail.len();
        let oldest = self.trailed - 1;
        let ago = ago.clamp(0.0, oldest as f64);
        let whole = ago as usize;
        let at = |back: usize| self.trail[(block + kept - back) % kept];
        let newer = at(whole);
        if whole >= oldest {
            return newer;
        }
        newer + (at(whole + 1) - newer) * (ago - whole as f64)
    }
}

impl Source {
    /// The farthest it has been, of what its trail keeps, from where it is
    /// at `now`, m: how much farther than now a way it is heard by may have
    /// left it.
    fn slack(&self, now: Vec3, block: usize) -> f64 {
        let kept = self.trail.len();
        (0..self.trailed.min(kept))
            .map(|back| self.trail[(block + kept - back) % kept].distance(now))
            .fold(0.0, f64::max)
    }
}

/// How many blocks sound takes from `metres` away, and a few more.
fn trail_blocks(metres: f64) -> usize {
    (metres / SOUND / (BLOCK as f64 * DT)).ceil() as usize + 3
}

/// The space's own answer: eight lines feeding one another (a feedback
/// delay network, after J.-M. Jot), each losing at every turn what the decay
/// asks of each of its three bands.
struct Tail {
    lines: [Vec<f32>; 8],
    at: [usize; 8],
    /// What each line holds under the first of [`Tail::SPLITS`], and under
    /// the second.
    low: [f32; 8],
    mid: [f32; 8],
    /// The pass of the two low-passes that part a line's bands.
    pass: [f32; 2],
    /// What each line keeps of each band after one turn.
    keep: [[f32; 3]; 8],
}

impl Tail {
    /// Primes, 13 to 47 ms: no two lines repeat together.
    const LENGTHS: [usize; 8] = [631, 809, 997, 1_259, 1_483, 1_741, 1_999, 2_251];
    /// Where a line's three bands part, Hz: between the frequencies a decay
    /// is given at, 250 Hz, 1 kHz and 4 kHz.
    const SPLITS: [f64; 2] = [500.0, 2_000.0];
    const CENTRES: [f64; 3] = [250.0, 1_000.0, 4_000.0];

    fn pass() -> [f64; 2] {
        Self::SPLITS.map(|hz| 1.0 - (-std::f64::consts::TAU * hz * DT).exp())
    }

    /// What a line that keeps `keep` of its three bands keeps of a tone of
    /// `hz`: low·L₁ + mid·(L₂ − L₁) + high·(1 − L₂), with L₁ and L₂ the two
    /// low-passes.
    fn kept(keep: [f64; 3], hz: f64) -> f64 {
        let turn = std::f64::consts::TAU * hz * DT;
        // A low-pass a / (1 − (1 − a)·e^(−iω)), as (real, imaginary).
        let [first, second] = Self::pass().map(|a| {
            let (re, im) = (1.0 - (1.0 - a) * turn.cos(), (1.0 - a) * turn.sin());
            let over = re * re + im * im;
            (a * re / over, -a * im / over)
        });
        let re = keep[0] * first.0 + keep[1] * (second.0 - first.0) + keep[2] * (1.0 - second.0);
        let im = keep[0] * first.1 + keep[1] * (second.1 - first.1) - keep[2] * second.1;
        re.hypot(im)
    }

    /// The shares of its three bands a line must keep for it to keep `asked`
    /// at 250 Hz, 1 kHz and 4 kHz. The low-passes that part the bands are
    /// gentle, 6 dB an octave, and each centre holds some of its neighbours:
    /// the shares are sought, each brought in turn to what its own centre
    /// asks. Decays too far apart for such gentle parting are not met: a
    /// line then keeps, at the frequency it keeps most of, what a decay
    /// twice as long as the longest asked would — whatever the three, the
    /// tail dies, and nowhere more than twice as slowly as asked.
    fn shares(asked: [f64; 3]) -> [f64; 3] {
        let mut keep = asked;
        for _ in 0..48 {
            for band in 0..3 {
                let now = Self::kept(keep, Self::CENTRES[band]).max(1e-12);
                keep[band] = (keep[band] * asked[band] / now).clamp(1e-9, 1.0);
            }
        }
        // What it keeps most of: at no frequency, at a tenth of an octave
        // from 20 Hz up, or at half the rate.
        let tones = (0..=100).map(|tenth| 20.0 * 2f64.powf(f64::from(tenth) / 10.0)).chain([0.0, RATE as f64 / 2.0]);
        let peak = tones.map(|hz| Self::kept(keep, hz)).fold(0.0f64, f64::max);
        let allowed = asked.iter().fold(0.0f64, |most, share| most.max(*share)).sqrt();
        if peak > allowed {
            keep = keep.map(|share| share * allowed / peak);
        }
        keep
    }

    /// A tail that dies by 60 dB in `seconds` at 250 Hz, 1 kHz and 4 kHz.
    fn new(seconds: [f64; 3]) -> Tail {
        let mut tail = Tail {
            lines: Self::LENGTHS.map(|length| vec![0.0; length]),
            at: [0; 8],
            low: [0.0; 8],
            mid: [0.0; 8],
            pass: Self::pass().map(|pass| pass as f32),
            keep: [[0.0; 3]; 8],
        };
        for (i, length) in Self::LENGTHS.into_iter().enumerate() {
            // What a line keeps after one turn of a sound that dies in `t60`.
            let asked = seconds.map(|t60| 10f64.powf(-3.0 * length as f64 / (t60 * RATE as f64)));
            tail.keep[i] = Self::shares(asked).map(|share| share as f32);
        }
        tail
    }

    fn run(&mut self, input: &[f32], left: &mut [f32], right: &mut [f32]) {
        const LEFT: [f32; 8] = [1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
        const RIGHT: [f32; 8] = [1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0];
        for n in 0..input.len() {
            let mut out = [0.0f32; 8];
            let (mut sum, mut l, mut r) = (0.0f32, 0.0f32, 0.0f32);
            for i in 0..8 {
                let read = self.lines[i][self.at[i]];
                l += read * LEFT[i];
                r += read * RIGHT[i];
                self.low[i] += (read - self.low[i]) * self.pass[0];
                self.mid[i] += (read - self.mid[i]) * self.pass[1];
                let keep = self.keep[i];
                out[i] = keep[0] * self.low[i] + keep[1] * (self.mid[i] - self.low[i]) + keep[2] * (read - self.mid[i]);
                sum += out[i];
            }
            // Each line feeds all the others and takes back a little of
            // itself (Householder): nothing is added, nothing lost.
            for (i, fed) in out.into_iter().enumerate() {
                self.lines[i][self.at[i]] = input[n] + fed - sum * 0.25;
                self.at[i] = (self.at[i] + 1) % Self::LENGTHS[i];
            }
            left[n] += l * 0.35;
            right[n] += r * 0.35;
        }
        for state in self.low.iter_mut().chain(&mut self.mid) {
            if state.abs() < TINY {
                *state = 0.0;
            }
        }
    }
}

/// What a node touching the ground remembers. Its contacts are its own: how
/// strong each is and how each sounds comes from the scene's seed, the node
/// and how many it has made — never from who else is in the scene.
struct Touched {
    /// What tells its contacts from every other touch's.
    seed: u32,
    /// What differs from one of its steps to the next.
    chance: Noise,
    /// The contacts it has made, heard or not.
    made: u64,
    /// Which blocks its wheels are looked at in, of those the quality looks
    /// at.
    turn: u64,
    last: Last,
}

enum Last {
    /// Metres gone since the last step, and which foot is next.
    Steps { gone: f64, foot: usize },
    /// The surface under each point.
    Wheels { on: Vec<Option<usize>> },
}

/// A contact about to be made: on which node and by which collision
/// materials, the struck surface's name, where, by what, how strong, at
/// which sample of the block, and what tells it from every other.
struct Blow {
    node: usize,
    material: String,
    id: String,
    by: String,
    by_node: Option<usize>,
    point: Vec3,
    strength: f64,
    at: usize,
    seed: u32,
}

/// A contact as a physics solver tells it (`osage-dynamics` reports where,
/// between which surfaces, how hard and when): it sounds as the two
/// collision materials meeting do, where it is, from its instant.
#[derive(Clone, Debug, PartialEq)]
pub struct Struck {
    /// When, s, on the scene's clock.
    pub at: f64,
    /// Where, in the world, m.
    pub point: Vec3,
    /// What was struck: the node its surface is on, and the collision
    /// material of that surface ([`Scene::collision_surface`] finds both
    /// from the surface's name).
    pub node: usize,
    pub material: String,
    /// What struck it: its collision material, and its node if it is one of
    /// the scene's.
    pub by: String,
    pub by_node: Option<usize>,
    /// How hard, 0 to 1 ([`strength`]).
    pub strength: f64,
}

/// How hard a contact is, 0 to 1, from the impulse a solver measured on a
/// body, N·s, and the body's mass, kg: the change of speed it made, full at
/// 5 m/s — a fall from 1.3 m. A world may tell strength its own way.
pub fn strength(impulse: f64, mass: f64) -> f64 {
    if mass > 0.0 { (impulse.abs() / mass / 5.0).clamp(0.0, 1.0) } else { 0.0 }
}

/// A scene's emitters, surfaces and touches in an order of their own — by
/// the node each is on and by what it is — whatever order the file lists
/// them and their nodes in. What is summed, taken in turn or told apart in
/// the order of a list is then the same for the same scene.
fn settle(scene: &mut Scene) {
    let paths: Vec<String> = (0..scene.tree.nodes.len()).map(|node| scene.tree.path_text(node)).collect();
    // (Any order will do that only what a thing is decides: a number's bits
    // are as good as its size.)
    let bits = |point: Vec3| [point.x.to_bits(), point.y.to_bits(), point.z.to_bits()];
    scene.emitters.sort_by(|a, b| (&paths[a.node], &a.id).cmp(&(&paths[b.node], &b.id)));
    scene.surfaces.sort_by_cached_key(|surface| {
        let material = surface.acoustic.map(|material| {
            let of = |three: [f64; 3]| three.map(f64::to_bits);
            (of(material.absorption), of(material.transmission), material.scattering.to_bits())
        });
        (
            paths[surface.node].clone(),
            surface.quad.map(bits),
            surface.id.clone(),
            (surface.reflects, surface.occludes, surface.strikes),
            material,
            surface.collision.clone(),
        )
    });
    scene.touches.sort_by_cached_key(|touch| {
        let (wheels, stride, points) = match &touch.touching {
            Touching::Steps { stride, feet } => (false, stride.to_bits(), feet),
            Touching::Wheels { points } => (true, 0, points),
        };
        let points: Vec<[u64; 3]> = points.iter().map(|&point| bits(point)).collect();
        (paths[touch.node].clone(), touch.material.clone(), touch.strength.to_bits(), wheels, stride, points)
    });
}

/// What a render did, to say what it cost.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tally {
    pub blocks: u64,
    /// Ways rendered, summed over the blocks, and the most at once.
    pub ways: u64,
    pub most_ways: usize,
    /// Voices made, summed over the blocks.
    pub voices: u64,
    pub contacts: u64,
    /// Bytes of the lines kept for the emitters' sound.
    pub ring_bytes: usize,
    /// Seconds spent: on what touches what, on finding the ways, on making
    /// the voices, on carrying the ways to the ears, on the reverberation.
    pub touching: f64,
    pub finding: f64,
    pub voicing: f64,
    pub carrying: f64,
    pub tailing: f64,
}

pub struct Engine {
    scene: Scene,
    options: Options,
    sources: Vec<Source>,
    /// The scene's surfaces that sound meets, in the order of the faces.
    facing: Vec<usize>,
    /// Those a foot or a wheel can be on.
    treading: Vec<usize>,
    /// Of each face, the sides where its surface ends and its sheet
    /// ([`sheets`]); and of each ground, the same.
    sheet: Sheets,
    tread_sheet: Sheets,
    /// The faces in the world, and the grounds, indexed: those that stay
    /// are made once, those that move each block. Made anew when a node
    /// comes to be driven.
    faces: Faces,
    ground: Faces,
    placed_faces: bool,
    /// Kept from one block to the next: the faces a source's ways are
    /// looked for by.
    near: Vec<usize>,
    /// Contacts told by a solver, soonest first, not yet struck.
    told: std::collections::VecDeque<Struck>,
    /// How many of them have been struck.
    struck: u64,
    /// For each source, the sources that load its voice.
    loading: Vec<Vec<usize>>,
    /// The nodes, parents first, and where each is in the world this block.
    order: Vec<usize>,
    placed: Vec<Placement>,
    /// Kept from one block to the next, so that none makes a list: where
    /// each source is; what each face scatters of the one being looked at.
    nows: Vec<Vec3>,
    scatters: Vec<Option<Scatter>>,
    found: Ways,
    ranked: Vec<(f32, usize, usize)>,
    /// What the air takes at 4 kHz, dB a metre.
    air: f64,
    /// The pass of the air's low-pass above which a way is left as it is.
    clear: f32,
    /// The low-passes that part the bands.
    parts: [f32; 3],
    block: u64,
    tail: Tail,
    touched: Vec<Touched>,
    send: Vec<f32>,
    made: Vec<f32>,
    /// What each ear's sum holds that is not sound: its mean, followed slowly.
    still: [f32; 2],
    /// For each node, whether it is driven from outside ([`Engine::drive`]):
    /// what is placed in it then moves, whatever its pose says now.
    driven: Vec<bool>,
    /// Which sound it makes: all of it, the near part, or the far part as a
    /// field.
    part: Part,
    /// The far part's channels, this block.
    field: [[f32; BLOCK]; MOST],
    /// The listener's head as last placed.
    head: Head,
    pub tally: Tally,
}

/// Which of a scene's sound an engine makes ([`Options::far`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// Every emitter by its own ways.
    Whole,
    /// What is near the cube's centre, by its own ways, to the ears.
    Near,
    /// What is far from it, by its ways to the centre, as a field.
    Far,
}

/// A field heard by one listener: decoded to virtual loudspeakers turned
/// with the head, each heard through the engine's ears — for headphones its
/// delay, shadow and outer ear, for loudspeakers its pan. What it costs
/// depends on the field's order, not on how many emitters made the field.
pub struct FieldEars {
    decoder: Decoder,
    quality: Quality,
    listening: Listening,
    lines: Vec<Line>,
    aims: Vec<Aim>,
    /// The decoder's gains of the last block, above and below the
    /// crossover, to move evenly from.
    before: Vec<[f32; MOST]>,
    before_low: Vec<[f32; MOST]>,
    /// Each channel's low-pass at the crossover.
    parted: [f32; MOST],
    pass: f32,
    /// What the gains above the crossover are brought by, so that the
    /// loudspeakers the ears cannot tell apart — the same time and shadow
    /// at each ear — and so add in step, keep the energy of a sound on
    /// average over every direction.
    high: f32,
}

impl FieldEars {
    pub fn new(order: usize, quality: Quality, listening: Listening) -> FieldEars {
        let decoder = Decoder::new(order);
        let head = Head { at: Vec3::ZERO, forward: vec3(0.0, 0.0, -1.0), right: Vec3::X, up: Vec3::Y };
        let aims = decoder
            .speakers
            .iter()
            .map(|s| {
                let from = vec3(s[0], s[1], -s[2]) * 10.0;
                Aim::new(0.0, [1.0; 4], 1.0, 0.0, &hear_for(from, &head, listening, quality.outer_ear))
            })
            .collect::<Vec<_>>();
        let count = aims.len();
        // The loudspeakers the ears hear alike, in groups.
        let alike = |a: &Aim, b: &Aim| a.late == b.late && a.shadow == b.shadow && a.pinna == b.pinna && a.pan == b.pan;
        let mut group = vec![usize::MAX; count];
        for s in 0..count {
            if group[s] == usize::MAX {
                group[s] = s;
                for o in s + 1..count {
                    if group[o] == usize::MAX && alike(&aims[s], &aims[o]) {
                        group[o] = s;
                    }
                }
            }
        }
        // The energy the ears get of a sound, the groups adding in step, on
        // average over directions spread on the sphere (a golden spiral).
        let mut turned = decoder.clone();
        turned.turn(&head);
        let probes = 256;
        let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
        let channels = crate::field::channels(turned.order);
        let mut energy = 0.0;
        for i in 0..probes {
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / probes as f64;
            let r = (1.0 - y * y).sqrt();
            let h = harmonics(vec3(r * (golden * i as f64).cos(), y, r * (golden * i as f64).sin()), turned.order);
            let mut sums = vec![0.0f64; count];
            for (s, g) in turned.gains().iter().enumerate() {
                sums[group[s]] += (0..channels).map(|c| f64::from(g[c]) * h[c]).sum::<f64>();
            }
            energy += sums.iter().map(|x| x * x).sum::<f64>();
        }
        let high = (probes as f64 / energy).sqrt() as f32;
        FieldEars {
            decoder,
            quality,
            listening,
            lines: (0..count).map(|_| Line::default()).collect(),
            aims,
            before: vec![[0.0; MOST]; count],
            before_low: vec![[0.0; MOST]; count],
            parted: [0.0; MOST],
            pass: (1.0 - (-std::f64::consts::TAU * crate::field::CROSSOVER * DT).exp()) as f32,
            high,
        }
    }

    /// The field's block heard by `head`, added to `left` and `right`.
    pub fn hear(
        &mut self,
        field: &[[f32; BLOCK]; MOST],
        head: &Head,
        left: &mut [f32; BLOCK],
        right: &mut [f32; BLOCK],
    ) {
        self.decoder.turn(head);
        let per = 1.0 / BLOCK as f32;
        let channels = crate::field::channels(self.decoder.order);
        // Each channel parted at the crossover: what is under it, and the rest.
        let mut low = [[0.0f32; BLOCK]; MOST];
        for c in 0..channels {
            let mut state = self.parted[c];
            for n in 0..BLOCK {
                state += (field[c][n] - state) * self.pass;
                low[c][n] = state;
            }
            self.parted[c] = if state.abs() < TINY { 0.0 } else { state };
        }
        for (s, (gains, low_gains)) in self.decoder.gains().iter().zip(self.decoder.low_gains()).enumerate() {
            let mut x = [0.0f32; BLOCK];
            let gains = gains.map(|g| g * self.high);
            for c in 0..channels {
                let (g, step) = (self.before[s][c], (gains[c] - self.before[s][c]) * per);
                let (k, k_step) = (self.before_low[s][c], (low_gains[c] - self.before_low[s][c]) * per);
                for (n, out) in x.iter_mut().enumerate() {
                    let m = (n + 1) as f32;
                    let under = low[c][n];
                    *out += under * (k + k_step * m) + (field[c][n] - under) * (g + step * m);
                }
            }
            self.before[s] = gains;
            self.before_low[s] = *low_gains;
            let aim = self.aims[s];
            if self.listening == Listening::Speakers {
                for n in 0..BLOCK {
                    left[n] += x[n] * aim.pan[0];
                    right[n] += x[n] * aim.pan[1];
                }
            } else {
                ears(&mut self.lines[s], &aim, &aim, &x, self.quality, left, right);
            }
        }
    }
}

/// Where a node is and how it goes, as a game or a server tells it
/// ([`Engine::drive`]): its placement in its parent and its velocity there.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub placement: Placement,
    /// m/s, in the parent's frame. Until the next pose the node goes on at
    /// this velocity: a position told sixty times a second then moves
    /// evenly, and what it carries is not heard to lurch at each one.
    pub velocity: Vec3,
}

/// The wall clock a tally reads the stages by. A browser's WebAssembly
/// (`wasm32-unknown-unknown`) has none: there a tally counts no time.
#[derive(Clone, Copy)]
struct Clock(#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))] std::time::Instant);

impl Clock {
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    fn now() -> Clock {
        Clock(std::time::Instant::now())
    }

    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    fn seconds(self) -> f64 {
        self.0.elapsed().as_secs_f64()
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    fn now() -> Clock {
        Clock()
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    fn seconds(self) -> f64 {
        0.0
    }
}

/// The air's low-pass for a way on which it takes `loss` dB at 4 kHz: two
/// one-poles, one after the other, set where the air has taken 3 dB from
/// each — what the air takes, in decibels, grows as the square of the
/// frequency, which two poles follow up to 5 kHz or so and understate
/// beyond. Returns how far each pole goes towards a new sample: 1 on a short
/// way, where it lets everything through.
fn air_pass(loss: f64) -> f32 {
    let corner = 4_000.0 * (6.0 / loss.max(1e-6)).sqrt();
    (1.0 - (-std::f64::consts::TAU * corner * DT).exp()) as f32
}

/// The pass of the air's low-pass above which a way is left as it is, for
/// a quality that leaves alone the ways on which the air takes less than
/// `clear_db` at 4 kHz; above any pass when every way is given its air.
fn clear_pass(clear_db: f64) -> f32 {
    if clear_db > 0.0 { air_pass(clear_db) } else { 2.0 }
}

/// Phases of the windowed sinc.
const PHASES: usize = 256;

// A binomially windowed sinc is equivalent, after normalization, to the
// Lagrange basis on samples −3…4. Compute each cardinal polynomial at build time.
// All emitters and listeners share the table, including its exact integer phases.
static SINC: [[f32; 8]; PHASES + 1] = {
    let mut rows = [[0.; 8]; PHASES + 1];
    let mut phase = 0;
    while phase <= PHASES {
        let at = 3.0 + phase as f64 / PHASES as f64;
        let mut tap = 0;
        while tap < 8 {
            let mut weight = 1.0;
            let mut other = 0;
            while other < 8 {
                if other != tap {
                    weight *= (at - other as f64) / (tap as f64 - other as f64);
                }
                other += 1;
            }
            rows[phase][tap] = weight as f32;
            tap += 1;
        }
        phase += 1;
    }
    rows
};

impl Engine {
    /// The engine on `scene`: all of its sound, or — when `options.far` is
    /// given — the near part only, the far part being [`Engine::field`]'s.
    pub fn new(scene: Scene, options: Options) -> Engine {
        let part = if options.far.is_some() { Part::Near } else { Part::Whole };
        Self::make(scene, options, part)
    }

    /// The far part of `scene` as a field at the centre `options.far` names,
    /// whoever listens there: [`Engine::render_field`] makes it, and
    /// [`FieldEars`] hears it. Without `options.far`, a field of the first
    /// order at the listener's place at rest, of everything beyond 100 m.
    pub fn field(scene: Scene, mut options: Options) -> Engine {
        if options.far.is_none() {
            let at = scene.tree.placement(scene.listener, options.start).point(scene.ears);
            options.far = Some(Far { centre: at, beyond: 100.0, order: 1 });
        }
        Self::make(scene, options, Part::Far)
    }

    fn make(mut scene: Scene, options: Options, part: Part) -> Engine {
        settle(&mut scene);
        let facing: Vec<usize> = (0..scene.surfaces.len()).filter(|&i| scene.surfaces[i].acoustic.is_some()).collect();
        let treading: Vec<usize> =
            (0..scene.surfaces.len()).filter(|&i| scene.surfaces[i].collision.is_some()).collect();
        let sheet_of = |of: &[usize]| {
            let quads: Vec<[Vec3; 4]> = of.iter().map(|&i| scene.surfaces[i].quad).collect();
            let nodes: Vec<usize> = of.iter().map(|&i| scene.surfaces[i].node).collect();
            let alike = |a: usize, b: usize| {
                let (a, b) = (&scene.surfaces[of[a]], &scene.surfaces[of[b]]);
                (a.acoustic, a.reflects, a.occludes, &a.collision, a.strikes)
                    == (b.acoustic, b.reflects, b.occludes, &b.collision, b.strikes)
            };
            sheets(&quads, &nodes, alike)
        };
        let (sheet, tread_sheet) = (sheet_of(&facing), sheet_of(&treading));
        // The straight way, and one for each sheet: the faces of a sheet
        // are alike, and what they mirror is one way.
        let lines = 1 + (0..sheet.of.len()).filter(|&k| sheet.of[k] == k).count();
        let air = air_loss(4_000.0, scene.air.celsius, scene.air.humidity);
        let mut sources = Vec::new();
        for (i, emitter) in scene.emitters.iter().enumerate() {
            let name = scene.emitter_name(i);
            let moves = Self::moves(&scene, &[], emitter.node);
            // As long as sound takes from as far as it is heard, and room to read.
            let ring = ((emitter.max_distance / SOUND * RATE as f64) as usize + 4 * BLOCK).next_power_of_two();
            let seed = seed_for(scene.seed, &name);
            sources.push(Source {
                voice: osage_sound_voices::make(&emitter.voice, seed, &emitter.params)
                    .expect("a scene names voices that exist"),
                // What is heard from nowhere is near wherever it is.
                heard: options.hears(&name) && !(part == Part::Far && emitter.reach != Reach::Positional),
                turn: u64::from(seed),
                name,
                node: emitter.node,
                offset: emitter.offset,
                gain: emitter.gain,
                reach: emitter.reach,
                mirrors: emitter.reflections,
                max_distance: emitter.max_distance,
                emitter: Some(i),
                ring: vec![0.0; ring],
                lines: vec![Line::default(); lines],
                targets: vec![None; lines],
                left: 1,
                aims: vec![None; lines],
                lags: vec![0.0; lines],
                asleep: false,
                waking: 0,
                moves,
                trail: if moves { vec![Vec3::ZERO; trail_blocks(emitter.max_distance)] } else { Vec::new() },
                trailed: 0,
                cut: match scene.tree.nodes[emitter.node].proxy_beyond {
                    Some(beyond) => Some((emitter.node, beyond, true)),
                    None => scene.tree.proxy_over(emitter.node).map(|(over, beyond)| (over, beyond, false)),
                },
                until: u64::MAX,
                worth: 0.0,
                top: 0.0,
                last: 0.0,
                struck: false,
                hit: None,
                load: 0.0,
                speed: 0.0,
                accel: 0.0,
                detail: 1.0,
            });
        }
        for slot in 0..options.quality.contacts {
            sources.push(Source {
                voice: osage_sound_voices::make("impact", 1, &Default::default()).expect("the impact voice exists"),
                turn: slot as u64,
                // Named, and heard or not, when it is struck: by what was struck and what struck it.
                heard: false,
                name: String::new(),
                node: 0,
                offset: Vec3::ZERO,
                gain: 0.0,
                reach: Reach::Positional,
                mirrors: true,
                max_distance: CONTACT_REACH,
                emitter: None,
                ring: vec![0.0; ((CONTACT_REACH / SOUND * RATE as f64) as usize + 4 * BLOCK).next_power_of_two()],
                lines: vec![Line::default(); lines],
                targets: vec![None; lines],
                left: 1,
                aims: vec![None; lines],
                lags: vec![0.0; lines],
                asleep: false,
                waking: 0,
                moves: false,
                trail: vec![Vec3::ZERO; trail_blocks(CONTACT_REACH)],
                trailed: 0,
                cut: None,
                until: 0,
                worth: 0.0,
                top: 0.0,
                last: 0.0,
                struck: false,
                hit: None,
                load: 0.0,
                speed: 0.0,
                accel: 0.0,
                detail: 1.0,
            });
        }
        let loading = (0..sources.len())
            .map(|i| match sources[i].emitter.map(|e| &scene.emitters[e].load) {
                Some(Load::Near { under, voice, .. }) => (0..scene.emitters.len())
                    .filter(|&j| {
                        let other = &scene.emitters[j];
                        // What stands for a branch from afar does not load: its branch does.
                        other.voice == *voice
                            && scene.tree.under(other.node, *under)
                            && scene.tree.nodes[other.node].proxy_beyond.is_none()
                    })
                    .collect(),
                _ => Vec::new(),
            })
            .collect();
        let touched = (0..scene.touches.len())
            .map(|k| {
                let touch = &scene.touches[k];
                // Which of its node's touches it is, in the engine's order.
                let nth = scene.touches[..k].iter().filter(|other| other.node == touch.node).count();
                let seed = seed_for(scene.seed, &format!("{} : touch {nth}", scene.tree.path_text(touch.node)));
                Touched {
                    seed,
                    chance: Noise::new(seed),
                    made: 0,
                    turn: u64::from(seed),
                    last: match &touch.touching {
                        Touching::Steps { .. } => Last::Steps { gone: 0.0, foot: 0 },
                        Touching::Wheels { points } => Last::Wheels { on: vec![None; points.len()] },
                    },
                }
            })
            .collect();
        let tally = Tally { ring_bytes: sources.iter().map(|s| s.ring.len() * 4).sum(), ..Tally::default() };
        Engine {
            parts: PARTS.map(|frequency| (1.0 - (-std::f64::consts::TAU * frequency * DT).exp()) as f32),
            clear: clear_pass(options.quality.clear_db),
            tail: Tail::new(scene.reverb.seconds),
            // Blocks are counted from the instant 0: a render that starts
            // earlier starts at a block before it.
            block: 0,
            facing,
            treading,
            sheet,
            tread_sheet,
            faces: Faces::default(),
            ground: Faces::default(),
            placed_faces: false,
            near: Vec::new(),
            told: Default::default(),
            struck: 0,
            loading,
            order: scene.tree.order(),
            placed: Vec::new(),
            nows: Vec::new(),
            scatters: Vec::new(),
            found: Ways::default(),
            ranked: Vec::new(),
            air,
            touched,
            send: vec![0.0; BLOCK],
            made: vec![0.0; BLOCK],
            still: [0.0; 2],
            driven: Vec::new(),
            part,
            field: [[0.0; BLOCK]; MOST],
            head: Head { at: Vec3::ZERO, forward: vec3(0.0, 0.0, -1.0), right: Vec3::X, up: Vec3::Y },
            tally,
            scene,
            options,
            sources,
        }
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Whether `node` or anything above it moves, turns or is driven.
    fn moves(scene: &Scene, driven: &[bool], node: usize) -> bool {
        let mut at = Some(node);
        while let Some(i) = at {
            let n = &scene.tree.nodes[i];
            if n.motion.is_some() || n.turning.is_some() || driven.get(i).copied().unwrap_or(false) {
                return true;
            }
            at = n.parent;
        }
        false
    }

    /// The next block of the far part's field ([`Engine::field`]): one
    /// channel for each of the order's harmonics, ACN, SN3D, the rest
    /// silent. An engine of another part leaves them silent.
    pub fn render_field(&mut self, out: &mut [[f32; BLOCK]; MOST]) {
        let (mut l, mut r) = ([0.0f32; BLOCK], [0.0f32; BLOCK]);
        self.render(&mut l, &mut r);
        *out = self.field;
    }

    /// Which part of the scene's sound this engine makes.
    pub fn part(&self) -> Part {
        self.part
    }

    /// The listener's head as the last block placed it (for the far part,
    /// the cube's centre along the world's axes).
    pub fn head(&self) -> Head {
        self.head
    }

    /// The node at `path`, identifiers from a root.
    pub fn node(&self, path: &[&str]) -> Option<usize> {
        self.scene.tree.find(path)
    }

    /// Places `node` as a game or a server tells it, from the next block on:
    /// at `pose.placement` at the instant of the next sample, going on at
    /// `pose.velocity` until it is told again. Whatever motion or turning
    /// the scene gave it is replaced. What is placed in it follows — its
    /// emitters, its faces, its touches, the listener if it is the
    /// listener's — and what it carries leaves a trail from then on, so
    /// that each way is heard from where it was when its sound left it.
    pub fn drive(&mut self, node: usize, pose: Pose) {
        self.drive_at(node, pose, self.instant());
    }

    /// [`Engine::drive`], for a pose that held at the instant `at`, s, on
    /// the scene's clock — a recorded replay's, given at the first block
    /// after it: the node goes on from there at its velocity, so that a
    /// pose given a little late is not heard late.
    pub fn drive_at(&mut self, node: usize, pose: Pose, at: f64) {
        let n = &mut self.scene.tree.nodes[node];
        n.rest = pose.placement;
        let speed = pose.velocity.length();
        n.motion = (speed > 1e-9).then(|| Motion::steady(pose.velocity, at, speed));
        n.turning = None;
        self.driven.resize(self.scene.tree.nodes.len(), false);
        if !self.driven[node] {
            // What is under it now moves: its faces leave the index.
            self.placed_faces = false;
        }
        self.driven[node] = true;
        for source in &mut self.sources {
            if !source.moves && self.scene.tree.under(source.node, node) {
                source.moves = true;
                if source.trail.is_empty() {
                    source.trail = vec![Vec3::ZERO; trail_blocks(source.max_distance)];
                }
                source.trailed = 0;
            }
        }
    }

    /// The instant of the next sample to be rendered, s.
    pub fn instant(&self) -> f64 {
        self.options.start + self.block as f64 * BLOCK as f64 * DT
    }

    /// The four corners of a surface in the world, as the nodes are placed
    /// this block.
    fn corners(&self, surface: usize) -> [Vec3; 4] {
        let placement = &self.placed[self.scene.surfaces[surface].node];
        self.scene.surfaces[surface].quad.map(|corner| placement.point(corner))
    }

    /// A face of the scene as it is in the world this block: the `k`th of
    /// those sound meets, or of the grounds.
    fn face(&self, k: usize, ground: bool) -> Face {
        const NOTHING: Material = Material { absorption: [0.0; 3], transmission: [0.0; 3], scattering: 0.0 };
        let (i, sheet) = if ground { (self.treading[k], &self.tread_sheet) } else { (self.facing[k], &self.sheet) };
        let surface = &self.scene.surfaces[i];
        let mut face = if ground {
            Face::new(self.corners(i), NOTHING, false, false)
        } else {
            Face::new(
                self.corners(i),
                surface.acoustic.expect("a face has a material"),
                surface.reflects,
                surface.occludes,
            )
        };
        for side in 0..4 {
            face.open[side] &= sheet.open[k][side];
        }
        face.sheet = sheet.of[k];
        face
    }

    /// The faces and the grounds where they are this block: all of them
    /// the first time (and when a node has come to be driven), those that
    /// move after that.
    fn place_faces(&mut self) {
        if self.placed_faces {
            for k in self.faces.moving().to_vec() {
                let face = self.face(k, false);
                self.faces.update(k, face);
            }
            for k in self.ground.moving().to_vec() {
                let face = self.face(k, true);
                self.ground.update(k, face);
            }
            return;
        }
        let moves = |i: usize| Self::moves(&self.scene, &self.driven, self.scene.surfaces[i].node);
        let faces = (0..self.facing.len()).map(|k| self.face(k, false)).collect();
        let moving: Vec<bool> = self.facing.iter().map(|&i| moves(i)).collect();
        self.faces = Faces::new(faces, &moving);
        let ground = (0..self.treading.len()).map(|k| self.face(k, true)).collect();
        let moving: Vec<bool> = self.treading.iter().map(|&i| moves(i)).collect();
        self.ground = Faces::new(ground, &moving);
        self.placed_faces = true;
    }

    /// The surface a foot or a wheel at `point` is on: the highest one of
    /// the grounds under it, within a step's height (of two as high, the
    /// first in the engine's order).
    fn under(&self, point: Vec3) -> Option<usize> {
        let mut near = Vec::new();
        self.ground.along(point + Vec3::Y * 0.6, point - Vec3::Y * 1.5, 0.0, |k| near.push(k));
        near.sort_unstable();
        let mut found: Option<(usize, f64)> = None;
        for k in near {
            let face = &self.ground[k];
            if face.normal.y.abs() < 0.3 {
                continue;
            }
            // Straight down from the point to the face's plane.
            let height = face.height(point) / face.normal.y;
            let on = point - Vec3::Y * height;
            if (-0.6..=1.5).contains(&height) && face.inside(on) >= 0.0 && found.is_none_or(|(_, best)| on.y > best) {
                found = Some((self.treading[k], on.y));
            }
        }
        found.map(|(i, _)| i)
    }

    /// How loud what is made at `place` arrives at `ears`, for a level of one
    /// at a metre: by the loudest of the ways a contact's voice is carried
    /// by — straight, through what lets it through or round what hides it,
    /// and mirrored by a face when the quality follows that — in its loudest
    /// band; and nothing when none of them comes within a contact's reach.
    fn arriving(&self, place: Vec3, ears: Vec3, faces: &Faces) -> f64 {
        let quality = self.options.quality;
        let loud = |way: &Way| {
            let most = way.bands.iter().fold(0.0f64, |most, &band| most.max(band));
            if way.length <= CONTACT_REACH { most / way.length.max(NEAREST) } else { 0.0 }
        };
        let mut most = direct(place, ears, faces).as_ref().map_or(0.0, loud);
        if quality.mirrors {
            let mut sheets = Vec::new();
            faces.sheets_within(place, ears, CONTACT_REACH, |sheet| {
                if faces[sheet].reflects {
                    sheets.push(sheet);
                }
            });
            sheets.sort_unstable();
            sheets.dedup();
            let mut ways = Vec::new();
            for sheet in sheets {
                ways.clear();
                mirrored(sheet, place, ears, faces, quality.careful, &mut ways);
                for (_, way) in &ways {
                    most = most.max(loud(way));
                }
            }
        }
        most
    }

    /// A contact: voices from the pool, placed where it is made on the
    /// surface's node — one for what was touched and, if what touched it
    /// sounds too (a clog, a hammer's head), one for that. Both bear the
    /// names of the two that met, the surface as an emitter is named and the
    /// node by its path: solo and mute take either. `ears` is where it is
    /// heard from, among `faces`: what does not reach them takes no voice,
    /// and what is heard less takes none from what is heard more.
    fn strike(&mut self, blow: Blow, ears: Vec3, faces: &Faces) {
        let Blow { node, material, id, by, by_node, point, strength, at, seed } = blow;
        let (Some(struck), Some(striker)) = (self.scene.contacts.get(&material), self.scene.contacts.get(&by)) else {
            return;
        };
        // The softer of the two sets how long the push lasts, for both.
        let hardness = struck.hardness.min(striker.hardness) as f32;
        let gain = struck.gain * striker.gain;
        let mut sounding = vec![(struck.voice.clone(), struck.params.clone())];
        if striker.params.get("tone", 0.0) > 0.0 && striker.params.get("ring", 0.0) > 0.0 {
            sounding.push((striker.voice.clone(), striker.params.clone()));
        }
        let name = format!(
            "{} : {} : contact with {}",
            self.scene.tree.path_text(node),
            id,
            by_node.map_or(by, |by| self.scene.tree.path_text(by))
        );
        let heard = self.options.hears(&name);
        let place = point + Vec3::Y * 0.03;
        let offset = self.placed[node].local(place);
        let first = self.scene.emitters.len();
        let pool = self.sources.len() - first;
        let moves = Self::moves(&self.scene, &self.driven, node);
        let strength = strength.clamp(0.0, 1.0);
        let distance = place.distance(ears);
        // What solo or mute leaves out, what is too far and what makes no
        // sound takes no voice from what is heard.
        if pool == 0 || !heard || distance > CONTACT_REACH || gain <= 1e-6 {
            return;
        }
        // As loud as it could arrive, with nothing in its way; and as loud
        // as it does, by the ways it has among the faces — looked for only
        // once a voice could be taken at all: in a crowd, most steps are
        // told to be too faint by how far they are.
        let most = (gain * strength / distance.max(NEAREST)) as f32;
        let mut arrives: Option<f32> = None;
        for (which, (voice, mut params)) in sounding.into_iter().enumerate() {
            // The voice that matters least: a free one, or the one least
            // heard now — as loud as it arrived, by what its voice still
            // makes of what it made at its loudest (all of it, for one just
            // struck). A contact takes it only if it matters more itself:
            // steps far off or behind a wall do not silence a step that is
            // heard, and a step that has died gives way to any that is.
            let block = self.block;
            let worth = |source: &Source| {
                let still = if source.top > 0.0 { source.last / source.top } else { 1.0 };
                if block > source.until { 0.0 } else { source.worth * still }
            };
            let least = (0..pool).min_by(|&a, &b| {
                let (a, b) = (&self.sources[first + a], &self.sources[first + b]);
                worth(a).total_cmp(&worth(b)).then(a.until.cmp(&b.until))
            });
            let slot = first + least.unwrap_or(0);
            if worth(&self.sources[slot]) >= most {
                continue;
            }
            let loud = *arrives.get_or_insert_with(|| (gain * strength * self.arriving(place, ears, faces)) as f32);
            if loud <= 0.0 || worth(&self.sources[slot]) >= loud {
                continue;
            }
            params.set("hardness", hardness);
            let source = &mut self.sources[slot];
            let seed = seed_for(u64::from(seed), &format!("voice {which}"));
            source.voice = osage_sound_voices::make(&voice, seed, &params).expect("a contact's voice exists");
            source.name.clone_from(&name);
            source.heard = heard;
            source.node = node;
            source.offset = offset;
            source.gain = gain;
            source.until = self.block + (CONTACT_SECONDS / (BLOCK as f64 * DT)) as u64;
            (source.worth, source.top, source.last) = (loud, 0.0, 0.0);
            source.struck = true;
            source.asleep = false;
            source.waking = 0;
            source.hit = Some(Hit { at: at.min(BLOCK - 1), strength: strength as f32 });
            source.lags.fill(0.0);
            source.moves = moves;
            source.trailed = 0;
            source.ring.fill(0.0);
            for line in &mut source.lines {
                *line = Line::default();
            }
            self.tally.contacts += 1;
        }
    }

    /// What the scene's feet and wheels touch during this block of `seconds`,
    /// heard from `ears` among `faces`.
    fn touch(&mut self, t1: f64, seconds: f64, ears: Vec3, faces: &Faces) {
        if self.scene.touches.is_empty() {
            return;
        }
        let every = self.options.quality.every.max(1);
        let mut blows: Vec<(usize, usize, Vec3, f64, usize, u32)> = Vec::new();
        let mut touched = std::mem::take(&mut self.touched);
        for (k, (touch, touched)) in self.scene.touches.iter().zip(&mut touched).enumerate() {
            let (speed, _) = self.scene.tree.going(touch.node, t1);
            let placement = &self.placed[touch.node];
            // A contact is told from every other by whose it is and which
            // one of them.
            let mut blow = |made: &mut u64, surface, point, strength, at| {
                *made += 1;
                let seed = seed_for(u64::from(touched.seed), &format!("contact {made}"));
                blows.push((k, surface, point, strength, at, seed));
            };
            match (&touch.touching, &mut touched.last) {
                (Touching::Steps { stride, feet }, Last::Steps { gone, foot }) => {
                    let step = speed.abs() * seconds;
                    *gone += step;
                    if *gone >= *stride && !feet.is_empty() && step > 0.0 {
                        *gone -= *stride;
                        // When in the block the foot came down.
                        let part = (1.0 - *gone / step).clamp(0.0, 1.0);
                        let point = placement.point(feet[*foot % feet.len()]);
                        *foot += 1;
                        // No two steps alike.
                        let strength = touch.strength * (1.0 + 0.18 * f64::from(touched.chance.next()));
                        if let Some(surface) = self.under(point) {
                            blow(&mut touched.made, surface, point, strength, (part * BLOCK as f64) as usize);
                        }
                    }
                }
                (Touching::Wheels { points }, Last::Wheels { on }) => {
                    // As often as the ways are found: a wheel at 50 km/h goes
                    // 15 cm in four blocks, and no plate is that narrow.
                    if !(self.block + touched.turn).is_multiple_of(every) {
                        continue;
                    }
                    for (i, local) in points.iter().enumerate() {
                        let point = placement.point(*local);
                        let now = self.under(point);
                        let before = std::mem::replace(&mut on[i], now);
                        // Coming onto something that strikes; not what it was on at the start,
                        // nor another face of the same surface (a mesh's next triangle).
                        let surfaces = &self.scene.surfaces;
                        let other = |a: usize, b: usize| {
                            surfaces[a].node != surfaces[b].node || surfaces[a].id != surfaces[b].id
                        };
                        let onto = now.filter(|&s| before.is_some_and(|b| other(b, s)) && surfaces[s].strikes);
                        if let Some(surface) = onto {
                            // A wheel knocks harder the faster it goes: full at 50 km/h.
                            let strength = touch.strength * (speed.abs() / 14.0).clamp(0.15, 1.0);
                            blow(&mut touched.made, surface, point, strength, BLOCK / 2);
                        }
                    }
                }
                _ => {}
            }
        }
        self.touched = touched;
        for (k, surface, point, strength, at, seed) in blows {
            let (touch, surface) = (&self.scene.touches[k], &self.scene.surfaces[surface]);
            let blow = Blow {
                node: surface.node,
                material: surface.collision.clone().expect("a ground has a collision material"),
                id: surface.id.clone(),
                by: touch.material.clone(),
                by_node: Some(touch.node),
                point,
                strength,
                at,
                seed,
            };
            self.strike(blow, ears, faces);
        }
    }

    /// Tells the engine of a contact a solver reports ([`Struck`]): it is
    /// struck at its instant, or at the start of the next block if that is
    /// past. Contacts may be told in any order and ahead of time.
    pub fn strike_told(&mut self, struck: Struck) {
        let at = self.told.partition_point(|other| other.at <= struck.at);
        self.told.insert(at, struck);
    }

    /// The contacts told that fall in this block, struck.
    fn told_contacts(&mut self, ears: Vec3, faces: &Faces) {
        let start = self.instant();
        let end = start + BLOCK as f64 * DT;
        while self.told.front().is_some_and(|struck| struck.at < end) {
            let struck = self.told.pop_front().expect("one is there");
            let at = (((struck.at - start) / DT).floor().max(0.0) as usize).min(BLOCK - 1);
            // Told from every other by what met and when.
            let by = struck.by_node.map_or(struck.by.clone(), |by| self.scene.tree.path_text(by));
            let what = format!(
                "{} : {} : {by} : {}",
                self.scene.tree.path_text(struck.node),
                struck.material,
                struck.at.to_bits()
            );
            let seed = seed_for(self.scene.seed, &what);
            self.struck += 1;
            let blow = Blow {
                node: struck.node,
                id: struck.material.clone(),
                material: struck.material,
                by: struck.by,
                by_node: struck.by_node,
                point: struck.point,
                strength: struck.strength,
                at,
                seed,
            };
            self.strike(blow, ears, faces);
        }
    }

    /// The next [`BLOCK`] samples for each ear, added to nothing: `left` and
    /// `right` are overwritten.
    pub fn render(&mut self, left: &mut [f32; BLOCK], right: &mut [f32; BLOCK]) {
        left.fill(0.0);
        right.fill(0.0);
        self.send.fill(0.0);
        let seconds = BLOCK as f64 * DT;
        let t1 = self.instant() + seconds;
        let quality = self.options.quality;
        let (encoding, order) = match (self.part, self.options.far) {
            (Part::Far, Some(far)) => (true, far.order.clamp(1, 2)),
            _ => (false, 0),
        };
        for channel in &mut self.field {
            channel.fill(0.0);
        }

        // Where every node is at the end of this block: once for all that
        // is placed in them.
        let clock = Clock::now();
        let mut placed = std::mem::take(&mut self.placed);
        self.scene.tree.place_all(t1, &self.order, &mut placed);
        self.placed = placed;
        let ears = &self.placed[self.scene.listener];
        let head = match (self.part, self.options.far) {
            // The field is made at the cube's centre, along the world's axes.
            (Part::Far, Some(far)) => {
                Head { at: far.centre, forward: vec3(0.0, 0.0, -1.0), right: Vec3::X, up: Vec3::Y }
            }
            _ => Head {
                at: ears.point(self.scene.ears),
                forward: ears.direction(-Vec3::Z),
                right: ears.direction(Vec3::X),
                up: ears.direction(Vec3::Y),
            }
            .turned(self.options.turn),
        };
        self.head = head;
        self.place_faces();
        let faces = std::mem::take(&mut self.faces);
        self.touch(t1, seconds, head.at, &faces);
        self.told_contacts(head.at, &faces);
        self.tally.touching += clock.seconds();

        // Where each source is; one that moves leaves a trail, from which
        // where it was when a sound left it is read back.
        let clock = Clock::now();
        let (mut nows, mut scatters) = (std::mem::take(&mut self.nows), std::mem::take(&mut self.scatters));
        let mut near = std::mem::take(&mut self.near);
        let mut mirroring: Vec<(usize, Way)> = Vec::new();
        let block = self.block as usize;
        nows.clear();
        for source in &mut self.sources {
            let now = self.placed[source.node].point(source.offset);
            if source.moves {
                let kept = source.trail.len();
                source.trail[block % kept] = now;
                source.trailed = (source.trailed + 1).min(kept);
            }
            nows.push(now);
        }
        let mut ranked = std::mem::take(&mut self.ranked);
        let mut found = std::mem::take(&mut self.found);
        ranked.clear();
        for i in 0..self.sources.len() {
            // Where it was when the sound now arriving straight left it.
            let now = nows[i];
            let place = self.sources[i].was(now, block, self.sources[i].lags[0] / seconds);
            let straight = place.distance(head.at);
            // What its voice makes now is what it is doing now: the line
            // carries it to the ears in its own time.
            let (speed, accel) = self.scene.tree.going(self.sources[i].node, t1);
            let mut load = 0.5f32;
            let mut near_speed = None;
            match self.sources[i].emitter.map(|e| &self.scene.emitters[e].load) {
                Some(Load::Near { radius, full, .. }) => {
                    // How much of what loads it is near, and how fast that goes.
                    let (mut weight, mut moving) = (0.0f64, 0.0f64);
                    for &j in &self.loading[i] {
                        let near = (1.0 - nows[j].distance(now) / radius).max(0.0);
                        weight += near;
                        moving += near * f64::from(self.sources[j].speed);
                    }
                    load = (weight / full).clamp(0.0, 1.0) as f32;
                    near_speed = (weight > 1e-9).then(|| (moving / weight) as f32);
                }
                Some(Load::Fixed(fixed)) => load = *fixed as f32,
                None => {}
            }
            // How much of it is heard for where the tree is cut: from afar a
            // node's own emitters stand for its branch, nearer the branch's
            // own. Two sounds that are not the same sound cross at equal power.
            let cut = match self.sources[i].cut {
                Some((over, beyond, stands)) => {
                    let distance = self.placed[over].translation.distance(head.at);
                    let x = ((distance - 0.85 * beyond) / (0.15 * beyond)).clamp(0.0, 1.0);
                    let far = x * x * (3.0 - 2.0 * x);
                    if stands { far.sqrt() } else { (1.0 - far).sqrt() }
                }
                None => 1.0,
            };
            // Near or far of the cube's centre: the same for every listener in the cube.
            let cut = match (self.part, self.options.far) {
                (Part::Near, Some(far)) => cut * (1.0 - far_share(now.distance(far.centre), far.beyond)).sqrt(),
                (Part::Far, Some(far)) => cut * far_share(now.distance(far.centre), far.beyond).sqrt(),
                _ => cut,
            };
            let source = &mut self.sources[i];
            source.speed = near_speed.unwrap_or(speed.abs() as f32);
            source.accel = accel as f32;
            source.load = load;
            source.detail = if quality.full_detail { 1.0 } else { (25.0 / straight.max(1.0)).clamp(0.15, 1.0) as f32 };
            // Its ways are found anew when its turn comes, and at once for a
            // contact; until then they go on to where they were going.
            let every = quality.every.max(1);
            if !(source.struck || (self.block + source.turn).is_multiple_of(every)) {
                continue;
            }
            source.targets.fill(None);
            source.left = every as u32;
            let free = source.emitter.is_none() && self.block > source.until;
            let level = source.gain * cut;
            if !source.heard
                || free
                || level <= 1e-6
                || source.reach != Reach::Positional
                || straight > source.max_distance
            {
                continue;
            }
            let mirrors = source.mirrors && (quality.mirrors || quality.folded);
            let feeds = source.mirrors && quality.reverb;
            // A way longer than the emitter is heard from is not one: its
            // sound is no longer in the line, and is never read sooner than
            // it took to come.
            let reach = source.max_distance;
            found.direct = direct(place, head.at, &faces);
            found.mirrored.clear();
            found.mirrored.resize(faces.sheets().len(), None);
            scatters.clear();
            scatters.resize(faces.sheets().len(), None);
            if mirrors || feeds {
                // The faces by which a way could be no longer than the
                // emitter is heard from, from wherever its sound left it.
                let slack = source.slack(now, block);
                near.clear();
                faces.sheets_within(place, head.at, reach + slack, |sheet| {
                    if faces[sheet].reflects {
                        near.push(sheet);
                    }
                });
                near.sort_unstable();
                near.dedup();
                // The others send nothing; they are looked at again from
                // where the straight way's sound left.
                let mut next = 0;
                for (slot, &sheet) in faces.sheets().iter().enumerate() {
                    if near.get(next) == Some(&sheet) {
                        next += 1;
                    } else if faces[sheet].reflects {
                        source.lags[1 + slot] = source.lags[0];
                    }
                }
                for &sheet in &near {
                    // What a sheet sends back left the emitter earlier than
                    // what comes straight: from where the emitter was then,
                    // by the way found last.
                    let slot = faces.slot(sheet);
                    let from = source.was(now, block, source.lags[1 + slot] / seconds);
                    mirroring.clear();
                    let scatter = mirrored(sheet, from, head.at, &faces, quality.careful, &mut mirroring);
                    let length =
                        mirroring.first().map(|(_, way)| way.length).or(scatter.map(|(_, scatter)| scatter.length));
                    source.lags[1 + slot] = length.map_or(source.lags[0], |length| length / SOUND);
                    // The faces under the mirror point mirror one way: from
                    // the same image, by the same point; across a side they
                    // share, their fades sum to one.
                    let way = mirroring.iter().map(|&(_, way)| way).reduce(|one, other| Way {
                        bands: std::array::from_fn(|b| one.bands[b] + other.bands[b]),
                        ..one
                    });
                    found.mirrored[slot] = way.filter(|way| mirrors && way.length <= reach);
                    scatters[slot] =
                        scatter.map(|(_, scatter)| scatter).filter(|scatter| feeds && scatter.length <= reach);
                }
            }
            let aim = |way: &Way, send: f32| {
                let length = way.length.max(NEAREST);
                let gains: [f32; 4] = std::array::from_fn(|b| (level * way.bands[b] / length) as f32);
                // The line holds what the emitter made for as long as sound
                // takes from as far as it is heard, and more.
                let delay = length / SOUND * RATE as f64;
                debug_assert!(delay <= (source.ring.len() - 2 * BLOCK) as f64);
                if encoding {
                    // For a field: no ears, the channels of where it comes from.
                    let mut aim = Aim::new(delay, gains, air_pass(self.air * length), send, &Heard::nothing());
                    aim.field = harmonics(way.from - head.at, order).map(|g| g as f32);
                    aim
                } else {
                    let heard = hear_for(way.from, &head, self.options.listening, quality.outer_ear);
                    Aim::new(delay, gains, air_pass(self.air * length), send, &heard)
                }
            };
            source.lags[0] = found.direct.map_or(straight, |way| way.length) / SOUND;
            if let Some(way) = found.direct.filter(|way| way.length <= reach) {
                let mut way = way;
                if !quality.mirrors {
                    // What the faces send back is not followed as ways of its
                    // own, but its level is kept: with the straight way's in
                    // the low band, where the two arrive in step, and as so
                    // much more power above.
                    for mirrored in found.mirrored.iter().flatten() {
                        let share = way.length.max(NEAREST) / mirrored.length.max(NEAREST);
                        way.bands[0] += mirrored.bands[0] * share;
                        for b in 1..4 {
                            way.bands[b] = way.bands[b].hypot(mirrored.bands[b] * share);
                        }
                    }
                }
                source.targets[0] = Some(aim(&way, 0.0));
            }
            for (k, scatter) in scatters.iter().enumerate() {
                // What a face scatters goes to the space's own answer by the
                // way it takes by that face, at the time that way takes: on
                // the mirrored way when it is followed, and otherwise on a
                // way of its own that reaches the ears with nothing.
                let send =
                    scatter.map_or(0.0, |scatter| (level * scatter.energy.sqrt() * self.scene.reverb.gain) as f32);
                source.targets[1 + k] = match (found.mirrored[k].filter(|_| quality.mirrors), scatter) {
                    (Some(way), _) => Some(aim(&way, send)),
                    (None, Some(scatter)) if send > 0.0 => {
                        Some(aim(&Way { length: scatter.length, from: scatter.from, bands: [0.0; 4] }, send))
                    }
                    _ => None,
                };
            }
        }
        for (i, source) in self.sources.iter_mut().enumerate() {
            source.aims.copy_from_slice(&source.targets);
            for (k, aim) in source.aims.iter().enumerate() {
                // (What only feeds the space's answer is not a way to the
                // ears, and costs next to nothing: it is not counted.)
                if let Some(aim) = aim.filter(Aim::sounds) {
                    // A way already followed is kept against one a little
                    // louder: two near the budget's edge would otherwise
                    // take each other's place block after block.
                    let held = if source.lines[k].live && source.lines[k].now.sounds() { 1.2 } else { 1.0 };
                    ranked.push((aim.gains[1].max(aim.gains[0]) * held, i, k));
                }
            }
        }
        // Only the loudest ways are rendered; one left out still feeds the
        // space's answer what its face scatters. Of two as loud, the one
        // kept is told by what the ways are — never by where their faces
        // are listed.
        if ranked.len() > quality.ways {
            let aim = |&(_, i, k): &(f32, usize, usize)| self.sources[i].aims[k].unwrap_or_default();
            ranked.sort_by(|a, b| {
                b.0.total_cmp(&a.0).then_with(|| aim(a).before(&aim(b))).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2))
            });
            for &(_, i, k) in &ranked[quality.ways..] {
                let aim = &mut self.sources[i].aims[k];
                *aim = aim.map(Aim::feeding).filter(|aim| k > 0 && aim.send > 0.0);
            }
        }
        // An emitter none of whose ways reaches the ears feeds nothing
        // either: its voice sleeps.
        for source in &mut self.sources {
            if !source.aims.iter().flatten().any(Aim::sounds) {
                source.aims.fill(None);
            }
        }
        (self.nows, self.scatters, self.ranked, self.found, self.near) = (nows, scatters, ranked, found, near);
        self.faces = faces;
        self.tally.finding += clock.seconds();

        // Every voice that is heard makes its block, into its own line.
        let clock = Clock::now();
        let base = self.block * BLOCK as u64;
        for source in &mut self.sources {
            let wanted = source.aims.iter().any(Option::is_some) || source.lines.iter().any(|line| line.live);
            let global = matches!(source.reach, Reach::Global { .. }) && source.heard;
            if !(wanted || global) {
                if !source.asleep {
                    // Asleep: its line is silence, so that nothing old is read when it wakes.
                    source.ring.fill(0.0);
                    source.asleep = true;
                }
                // A contact that is not heard is worth nothing against another.
                (source.top, source.last) = (source.top.max(f32::MIN_POSITIVE), 0.0);
                continue;
            }
            if source.asleep {
                source.asleep = false;
                source.waking = WAKE;
            }
            let drive = Drive {
                speed: source.speed,
                accel: source.accel,
                load: source.load,
                hit: source.hit.take(),
                detail: source.detail,
            };
            source.voice.render(&drive, &mut self.made);
            self.tally.voices += 1;
            if source.emitter.is_none() {
                source.last = self.made.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
                source.top = source.top.max(source.last);
            }
            if source.waking > 0 {
                for sample in self.made.iter_mut() {
                    *sample *= 1.0 - source.waking as f32 / WAKE as f32;
                    source.waking = source.waking.saturating_sub(1);
                }
            }
            let mask = source.ring.len() - 1;
            for (n, sample) in self.made.iter().enumerate() {
                // What is too small to be heard is nothing: numbers near the
                // smallest a float holds are a hundred times slower to work on.
                source.ring[(base as usize + n) & mask] = if sample.abs() < TINY { 0.0 } else { *sample };
            }
            if let Reach::Global { pan } = source.reach {
                // From nowhere: straight into the ears.
                let turn = (pan + 1.0) * std::f64::consts::FRAC_PI_4;
                let (l, r) = ((source.gain * turn.cos()) as f32, (source.gain * turn.sin()) as f32);
                for n in 0..BLOCK {
                    left[n] += self.made[n] * l;
                    right[n] += self.made[n] * r;
                }
            }
        }
        self.tally.voicing += clock.seconds();

        // Every way carries its emitter's line to the ears.
        let clock = Clock::now();
        let (mut ways_now, parts) = (0usize, self.parts);
        for source in &mut self.sources {
            if source.reach != Reach::Positional || source.asleep {
                continue;
            }
            for (line, aim) in source.lines.iter_mut().zip(&source.aims) {
                let goal = match aim {
                    Some(target) => {
                        if !line.live {
                            // A new way, with nothing left of an older one: from
                            // silence where it will be — or, for a contact, there
                            // at once (nothing was sounding before).
                            *line = Line::default();
                            line.now = if source.struck { *target } else { target.silent() };
                        }
                        // Evenly, over the blocks left until its ways are found anew.
                        if source.left <= 1 { *target } else { line.now.toward(target, 1.0 / source.left as f32) }
                    }
                    None if line.live => line.now.silent(),
                    None => continue,
                };
                if goal.sounds() || line.now.sounds() {
                    run(
                        line,
                        &goal,
                        &source.ring,
                        base,
                        parts,
                        self.clear,
                        quality,
                        self.options.listening,
                        left,
                        right,
                        &mut self.send,
                        if encoding { Some(&mut self.field) } else { None },
                    );
                    ways_now += 1;
                } else {
                    feed(line, &goal, &source.ring, base, &mut self.send);
                }
                line.live = aim.is_some();
            }
            source.struck = false;
            source.left = source.left.saturating_sub(1).max(1);
        }
        self.tally.carrying += clock.seconds();

        let clock = Clock::now();
        if quality.reverb {
            self.tail.run(&self.send, left, right);
            if encoding {
                // The space's own answer comes from all round: into the field's W.
                for (n, w) in self.field[0].iter_mut().enumerate() {
                    *w += 0.5 * (left[n] + right[n]);
                }
            }
        }
        self.tally.tailing += clock.seconds();
        // No ear is pushed one way for long: what stays under 8 Hz is taken off.
        for (ear, still) in [&mut *left, &mut *right].into_iter().zip(&mut self.still) {
            for sample in ear.iter_mut() {
                *still += (*sample - *still) * 0.001;
                *sample -= *still;
            }
            if still.abs() < TINY {
                *still = 0.0;
            }
        }
        self.tally.blocks += 1;
        self.tally.ways += ways_now as u64;
        self.tally.most_ways = self.tally.most_ways.max(ways_now);
        self.block += 1;
    }
}

/// The ears' stage of a way for headphones: `x`, what the way carries this
/// block, through the outer ear's echoes, each ear's delay and the head's
/// shadow, added to `left` and `right` — everything moving evenly from
/// `from` to `goal`.
#[allow(clippy::too_many_arguments)]
fn ears(
    line: &mut Line,
    from: &Aim,
    goal: &Aim,
    x: &[f32; BLOCK],
    quality: Quality,
    left: &mut [f32; BLOCK],
    right: &mut [f32; BLOCK],
) {
    let per = 1.0 / BLOCK as f32;
    // What came before this block, then the block: the outer ear and
    // the ears read a little back.
    let mut dry = [0.0f32; SHORT + BLOCK];
    dry[..SHORT].copy_from_slice(&line.dry);
    dry[SHORT..].copy_from_slice(x);
    let mut wet = [0.0f32; SHORT + BLOCK];
    wet[..SHORT].copy_from_slice(&line.wet);
    wet[SHORT..].copy_from_slice(x);
    if quality.outer_ear {
        // The outer ear: five echoes, each between two samples. An echo
        // whose delay stays between the same two samples all through the
        // block — nearly every one — is added a stretch at a time.
        for (e, (share, ..)) in PINNA.iter().enumerate() {
            let (late, step) = (from.pinna[e], (goal.pinna[e] - from.pinna[e]) * per);
            let share = *share as f32;
            let whole = late as usize;
            if whole >= 1 && goal.pinna[e] as usize == whole {
                let part = late - whole as f32;
                let (newer, older) =
                    (&dry[SHORT - whole..SHORT - whole + BLOCK], &dry[SHORT - whole - 1..SHORT - whole - 1 + BLOCK]);
                for (n, to) in wet[SHORT..].iter_mut().enumerate() {
                    let f = part + step * (n + 1) as f32;
                    *to += share * (newer[n] + (older[n] - newer[n]) * f);
                }
            } else {
                for (n, to) in wet[SHORT..].iter_mut().enumerate() {
                    let late = late + step * (n + 1) as f32;
                    let whole = late as usize;
                    let f = late - whole as f32;
                    let j = SHORT + n - whole;
                    *to += share * (dry[j] + (dry[j - 1] - dry[j]) * f);
                }
            }
        }
    }
    // The head's shadow, one pole and one zero: b0 = k0 + k1·α, b1 = k0 − k1·α.
    let beta = 2.0 * SOUND / HEAD_RADIUS;
    let k0 = (beta / (beta + 2.0 * RATE as f64)) as f32;
    let k1 = (2.0 * RATE as f64 / (beta + 2.0 * RATE as f64)) as f32;
    let a1 = ((beta - 2.0 * RATE as f64) / (beta + 2.0 * RATE as f64)) as f32;
    // At its best the ears' delay is read as finely as the line.
    let fine = quality.reading == Reading::Sinc;
    for (ear, out) in [&mut *left, &mut *right].into_iter().enumerate() {
        // This ear hears later by the way round the head, and through its shadow.
        let (mut late, late_step) = (from.late[ear], (goal.late[ear] - from.late[ear]) * per);
        let (mut alpha, alpha_step) = (from.shadow[ear], (goal.shadow[ear] - from.shadow[ear]) * per);
        let (mut before, mut shaded) = (line.shadow_in[ear], line.shadow_out[ear]);
        for (n, to) in out.iter_mut().enumerate() {
            late += late_step;
            alpha += alpha_step;
            let whole = late as usize;
            let f = late - whole as f32;
            let j = SHORT + n - whole;
            let v = if fine {
                cubic(wet[j + 1], wet[j], wet[j - 1], wet[j - 2], f)
            } else {
                wet[j] + (wet[j - 1] - wet[j]) * f
            };
            shaded = (k0 + k1 * alpha) * v + (k0 - k1 * alpha) * before - a1 * shaded;
            before = v;
            *to += shaded;
        }
        line.shadow_in[ear] = before;
        line.shadow_out[ear] = shaded;
    }
    line.dry.copy_from_slice(&dry[BLOCK..]);
    line.wet.copy_from_slice(&wet[BLOCK..]);
}

/// A point between the middle two of four samples, `f` of the way from `q`
/// to `r`: Lagrange's curve through the four.
#[inline(always)]
fn cubic(p: f32, q: f32, r: f32, s: f32, f: f32) -> f32 {
    p * (-f * (f - 1.0) * (f - 2.0) / 6.0)
        + q * ((f + 1.0) * (f - 1.0) * (f - 2.0) / 2.0)
        + r * (-(f + 1.0) * f * (f - 2.0) / 2.0)
        + s * ((f + 1.0) * f * (f - 1.0) / 6.0)
}

/// A way that reaches the ears with nothing, for one block: only what it
/// feeds the space's own answer — the emitter's line read `delay` back,
/// between two samples (that answer blurs far more than this does).
fn feed(line: &mut Line, goal: &Aim, ring: &[f32], base: u64, send: &mut [f32]) {
    let mask = ring.len() - 1;
    let from = line.now;
    if from.send != 0.0 || goal.send != 0.0 {
        // The point read, in whole samples and 2³² parts of one, as in `run`.
        let slope = (goal.delay - from.delay) / BLOCK as f64;
        let first = base as f64 + (ring.len() * 4) as f64 - from.delay - slope;
        const ONE: f64 = 4_294_967_296.0;
        let mut point = (((first.floor() as u64) & 0xffff_ffff) << 32) | ((first - first.floor()) * ONE) as u64;
        let advance = ((1.0 - slope) * ONE) as u64;
        let step = (goal.send - from.send) / BLOCK as f32;
        for (n, to) in send.iter_mut().enumerate() {
            let (i, f) = ((point >> 32) as usize, ((point as u32) >> 8) as f32 * (1.0 / 16_777_216.0));
            let here = ring[i & mask];
            *to += (here + (ring[i.wrapping_add(1) & mask] - here) * f) * (from.send + step * (n + 1) as f32);
            point = point.wrapping_add(advance);
        }
    }
    // What it would hold had it been carried to the ears: nothing.
    *line = Line { live: line.live, now: *goal, ..Line::default() };
}

/// One way for one block: the emitter's line read `delay` back, through the
/// bands and the air, the outer ear and the two ears, added to the mix.
/// Everything moves evenly from where the last block left it to `goal`. Done
/// a stage at a time over the block, each a short loop a processor runs
/// well.
#[allow(clippy::too_many_arguments)]
fn run(
    line: &mut Line,
    goal: &Aim,
    ring: &[f32],
    base: u64,
    parts: [f32; 3],
    clear: f32,
    quality: Quality,
    listening: Listening,
    left: &mut [f32; BLOCK],
    right: &mut [f32; BLOCK],
    send: &mut [f32],
    field: Option<&mut [[f32; BLOCK]; MOST]>,
) {
    let mask = ring.len() - 1;
    let per = 1.0 / BLOCK as f32;
    let from = line.now;
    let mut x = [0.0f32; BLOCK];

    // The emitter's line, read as far back as its sound took to come. The
    // point read is counted in whole samples and 2³² parts of one, in whole
    // numbers: no rounding piles up along the block, and a processor steps a
    // whole number faster than it turns a fraction into one. It is kept
    // above zero by whole turns of the line, which the mask takes off again.
    let slope = (goal.delay - from.delay) / BLOCK as f64;
    let first = base as f64 + (ring.len() * 4) as f64 - from.delay - slope;
    const ONE: f64 = 4_294_967_296.0;
    let mut point = (((first.floor() as u64) & 0xffff_ffff) << 32) | ((first - first.floor()) * ONE) as u64;
    let advance = ((1.0 - slope) * ONE) as u64;
    // The 24 highest bits of the part of a sample: all a float holds.
    let part = |point: u64| ((point as u32) >> 8) as f32 * (1.0 / 16_777_216.0);
    match quality.reading {
        Reading::Linear => {
            for out in &mut x {
                let (i, f) = ((point >> 32) as usize, part(point));
                let here = ring[i & mask];
                *out = here + (ring[i.wrapping_add(1) & mask] - here) * f;
                point = point.wrapping_add(advance);
            }
        }
        Reading::Cubic => {
            for out in &mut x {
                let (i, f) = ((point >> 32) as usize, part(point));
                *out = cubic(
                    ring[i.wrapping_sub(1) & mask],
                    ring[i & mask],
                    ring[i.wrapping_add(1) & mask],
                    ring[i.wrapping_add(2) & mask],
                    f,
                );
                point = point.wrapping_add(advance);
            }
        }
        Reading::Sinc => {
            for out in &mut x {
                let (i, f) = ((point >> 32) as usize, part(point));
                let phase = f * PHASES as f32;
                let row = (phase as usize).min(PHASES - 1);
                let between = phase - row as f32;
                let (a, b) = (&SINC[row], &SINC[row + 1]);
                let mut sum = 0.0;
                for tap in 0..8 {
                    sum += ring[i.wrapping_add(tap).wrapping_sub(3) & mask] * (a[tap] + (b[tap] - a[tap]) * between);
                }
                *out = sum;
                point = point.wrapping_add(advance);
            }
        }
    }

    // What the space's own answer is fed.
    // (A level that moves evenly is written from the sample's number, not
    // from the level before: nothing is then carried from a sample to the
    // next, and a processor does several at once.)
    if from.send != 0.0 || goal.send != 0.0 {
        let step = (goal.send - from.send) * per;
        for (n, (to, read)) in send.iter_mut().zip(&x).enumerate() {
            *to += read * (from.send + step * (n + 1) as f32);
        }
    }

    // Four bands, each with what distance, edges and materials leave of it,
    // then what the air took on the way. Most ways are straight and short:
    // their four bands are left alike — then parting them changes nothing,
    // and one gain does — and the air has taken next to nothing.
    let alike = |gains: &[f32; 4]| gains.iter().all(|gain| *gain == gains[0]);
    let flat = alike(&from.gains) && alike(&goal.gains);
    let clear = from.air >= clear && goal.air >= clear;
    let (mut air, air_step) = (from.air, (goal.air - from.air) * per);
    let [mut first, mut second] = line.air;
    if flat {
        let (mut gain, step) = (from.gains[0], (goal.gains[0] - from.gains[0]) * per);
        let last = x[BLOCK - 1];
        if clear {
            for (n, sample) in x.iter_mut().enumerate() {
                *sample *= gain + step * (n + 1) as f32;
            }
            (first, second) = (x[BLOCK - 1], x[BLOCK - 1]);
        } else {
            for sample in &mut x {
                gain += step;
                air += air_step;
                first = first * (1.0 - air) + *sample * gain * air;
                second = second * (1.0 - air) + first * air;
                *sample = second;
            }
        }
        // Where the low-passes would be, near enough, when the bands part again.
        line.parts = [last; 3];
    } else {
        let mut gains = from.gains;
        let steps: [f32; 4] = std::array::from_fn(|b| (goal.gains[b] - from.gains[b]) * per);
        let [mut low, mut mid, mut high] = line.parts;
        // Each low-pass keeps a share of what it held and takes the rest from
        // what comes: written so, a sample waits only for one product and one sum.
        let keep = parts.map(|a| 1.0 - a);
        for sample in &mut x {
            let read = *sample;
            low = low * keep[0] + read * parts[0];
            mid = mid * keep[1] + read * parts[1];
            high = high * keep[2] + read * parts[2];
            for (gain, step) in gains.iter_mut().zip(steps) {
                *gain += step;
            }
            air += air_step;
            let shaped = gains[0] * low + gains[1] * (mid - low) + gains[2] * (high - mid) + gains[3] * (read - high);
            first = first * (1.0 - air) + shaped * air;
            second = second * (1.0 - air) + first * air;
            *sample = second;
        }
        line.parts = [low, mid, high];
    }
    line.air = [first, second];

    if let Some(field) = field {
        // Into the field's channels, each as much as the way's direction asks.
        for (channel, (g, to)) in field.iter_mut().zip(from.field.iter().zip(&goal.field)) {
            let step = (to - g) * per;
            if *g == 0.0 && step == 0.0 {
                continue;
            }
            for (n, out) in channel.iter_mut().enumerate() {
                *out += x[n] * (g + step * (n + 1) as f32);
            }
        }
    } else if listening == Listening::Speakers {
        let (l, r) = (from.pan[0], from.pan[1]);
        let (l_step, r_step) = ((goal.pan[0] - l) * per, (goal.pan[1] - r) * per);
        for n in 0..BLOCK {
            let k = (n + 1) as f32;
            left[n] += x[n] * (l + l_step * k);
            right[n] += x[n] * (r + r_step * k);
        }
    } else {
        ears(line, &from, goal, &x, quality, left, right);
    }
    // A way gone quiet is left at nothing, not at numbers almost nothing.
    for state in line.parts.iter_mut().chain(&mut line.air).chain(&mut line.shadow_in).chain(&mut line.shadow_out) {
        if state.abs() < TINY {
            *state = 0.0;
        }
    }
    line.now = *goal;
}

/// What one way costs, nanoseconds a sample, at `quality` for `listening`:
/// a line of noise read through a way that moves a little, as a passing
/// emitter's does, for `seconds` of sound; how it is heard is worked out
/// anew every block, as at the highest qualities.
pub fn way_cost(quality: Quality, listening: Listening, seconds: f64) -> f64 {
    let mut noise = Noise::new(9);
    let ring: Vec<f32> = (0..1 << 16).map(|_| noise.next() * 0.1).collect();
    let parts = PARTS.map(|frequency| (1.0 - (-std::f64::consts::TAU * frequency * DT).exp()) as f32);
    let head = crate::scene::Head { at: Vec3::ZERO, forward: -Vec3::Z, right: Vec3::X, up: Vec3::Y };
    let (mut left, mut right, mut send) = ([0.0f32; BLOCK], [0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    let mut line = Line { live: true, ..Line::default() };
    let blocks = (seconds * RATE as f64 / BLOCK as f64) as u64;
    let started = crate::bench::cpu_seconds();
    for block in 0..blocks {
        // An emitter passing at 10 m/s, 5 m to the side.
        let along = -40.0 + block as f64 * BLOCK as f64 * DT * 10.0;
        let from = Vec3 { x: 5.0, y: 0.5, z: along };
        let length = from.length();
        let goal = Aim::new(
            length / SOUND * RATE as f64,
            [(0.5 / length) as f32; 4],
            air_pass(0.03 * length),
            0.01,
            &hear_for(from, &head, listening, quality.outer_ear),
        );
        run(
            &mut line,
            &goal,
            &ring,
            (1 << 15) + block * BLOCK as u64,
            parts,
            clear_pass(quality.clear_db),
            quality,
            listening,
            &mut left,
            &mut right,
            &mut send,
            None,
        );
    }
    std::hint::black_box((&left, &right, &send));
    (crate::bench::cpu_seconds() - started) * 1e9 / (blocks * BLOCK as u64) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use osage_sound_voices::dsp::Biquad;

    /// How long the tail takes to fall by 60 dB round `hz`, s: what it answers to one sample, through a
    /// band a third of an octave wide; the energy left after each instant (Schroeder's curve), from 5 dB
    /// down to 25 dB down, three times over.
    fn decay(seconds: [f64; 3], hz: f32) -> f64 {
        let mut tail = Tail::new(seconds);
        let frames = 3 * RATE as usize;
        let mut input = vec![0.0f32; frames];
        input[0] = 1.0;
        let (mut left, mut right) = (vec![0.0f32; frames], vec![0.0f32; frames]);
        tail.run(&input, &mut left, &mut right);
        let (mut band, mut again) = (Biquad::default(), Biquad::default());
        band.band_pass(hz, 4.3);
        again.band_pass(hz, 4.3);
        let mut left_after: Vec<f64> = left.iter().map(|s| f64::from(again.pass(band.pass(*s))).powi(2)).collect();
        for n in (0..frames - 1).rev() {
            left_after[n] += left_after[n + 1];
        }
        let down = |decibels: f64| {
            let level = left_after[0] * 10f64.powf(-decibels / 10.0);
            left_after.iter().position(|energy| *energy < level).expect("it dies") as f64 / RATE as f64
        };
        3.0 * (down(25.0) - down(5.0))
    }

    #[test]
    fn the_tail_dies_in_each_band_as_fast_as_it_is_asked() {
        for seconds in [[0.7, 0.55, 0.35], [1.2, 0.8, 0.4], [0.5, 0.5, 0.5], [0.4, 0.8, 0.4], [0.9, 0.45, 0.9]] {
            for (band, hz) in Tail::CENTRES.into_iter().enumerate() {
                let found = decay(seconds, hz as f32);
                assert!((found / seconds[band] - 1.0).abs() < 0.12, "{seconds:?} at {hz} Hz: {found:.3} s");
            }
        }
        // The middle one alone: twice as long, heard twice as long there.
        let (short, long) = (decay([0.6, 0.4, 0.3], 1_000.0), decay([0.6, 0.8, 0.3], 1_000.0));
        assert!((long / short - 2.0).abs() < 0.2, "{short:.3} s then {long:.3} s");
    }

    #[test]
    fn a_line_keeps_less_than_everything_at_every_frequency() {
        // Whatever the three decays, and however far apart: what a line keeps of a tone is never more than
        // a decay twice as long as the longest asked would keep, so the tail dies.
        let mut noise = Noise::new(11);
        let mut asked: Vec<[f64; 3]> = vec![
            [0.999, 1e-3, 0.999],
            [1e-3, 0.999, 1e-3],
            [0.999, 0.999, 1e-3],
            [1e-3, 0.999, 0.999],
            [0.97, 0.5, 0.97],
            [0.999, 0.999, 0.999],
        ];
        asked.extend((0..200).map(|_| std::array::from_fn(|_| f64::from(noise.unit()).clamp(1e-3, 0.999))));
        for asked in asked {
            let keep = Tail::shares(asked);
            let most = asked.iter().fold(0.0f64, |most, share| most.max(*share)).sqrt();
            assert!(most < 1.0);
            for step in 0..=2_400 {
                let kept = Tail::kept(keep, f64::from(step) * 10.0);
                assert!(kept <= most * 1.000_1, "{asked:?} keeps {kept} at {} Hz", step * 10);
            }
        }
        // Decays as a space has them are met at the three centres, on the longest line and the shortest.
        for seconds in [[0.7, 0.55, 0.35], [1.2, 0.8, 0.4], [0.4, 0.8, 0.4], [2.0, 1.0, 2.0]] {
            for length in [631.0, 2_251.0] {
                let asked = seconds.map(|t60| 10f64.powf(-3.0 * length / (t60 * RATE as f64)));
                let keep = Tail::shares(asked);
                for (band, hz) in Tail::CENTRES.into_iter().enumerate() {
                    let kept = Tail::kept(keep, hz);
                    assert!(
                        (kept / asked[band] - 1.0).abs() < 1e-6,
                        "{seconds:?} at {hz} Hz: {kept} for {}",
                        asked[band]
                    );
                }
                // And at no frequency does the line ring more than twice as long as the longest asked — a
                // tenth longer, for decays that shorten as the pitch rises, as a street's do.
                let peak = (0..=2_400).map(|step| Tail::kept(keep, f64::from(step) * 10.0)).fold(0.0f64, f64::max);
                let longest = -3.0 * length / (peak.log10() * RATE as f64);
                let asked = seconds.iter().fold(0.0f64, |most, t60| most.max(*t60));
                let falling = seconds[0] >= seconds[1] && seconds[1] >= seconds[2];
                assert!(longest < asked * if falling { 1.1 } else { 2.001 }, "{seconds:?}: {longest:.3} s somewhere");
            }
        }
    }
}
