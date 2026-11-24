// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! What the engine costs: the processor's time for a second of sound, the
//! memory it keeps, as a device of each quality would pay them — measured on
//! the scene of the bridge, and on the scene the baselines of `bench/` play
//! (a ring of noises turning round a listener), so that the engine can be
//! set beside OpenAL Soft and Web Audio on the same machine.
//!
//! Time is the processor's, not the clock's: a machine busy with something
//! else gives the same figure, near enough.

use crate::{
    engine::{Options, Quality},
    render::render,
    scene::Scene,
};

/// Seconds this thread has spent on a processor; on a system that does not
/// say, seconds on the clock since the first call.
pub fn cpu_seconds() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    // Linux: nanoseconds on a processor, then waiting for one, then turns.
    std::fs::read_to_string("/proc/thread-self/schedstat")
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse::<f64>().ok())
        .map_or_else(|| START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64(), |ns| ns / 1e9)
}

/// The most memory this program has held at once, bytes, where the system
/// says.
pub fn peak_memory() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    Some(line.split_whitespace().nth(1)?.parse::<u64>().ok()? * 1024)
}

/// The scene the baselines play: `sources` noises evenly spread on a ring of
/// 10 m that turns at 10 m/s, heard from 6 m off its centre — each noise
/// from 4 to 16 m away, closing at up to 6 m/s. With `floor`, a hard road
/// 1.6 m under the ears, which mirrors each of them.
pub fn ring(sources: usize, floor: bool) -> Scene {
    let children: Vec<String> = (0..sources).map(|i| (i + 3).to_string()).collect();
    let surfaces = if floor {
        r#", "osageSound": {"surfaces": [{"id": "road", "quadM": [[-60, -1.6, -60], [-60, -1.6, 60], [60, -1.6, 60], [60, -1.6, -60]], "acoustic": "road"}]}"#
    } else {
        ""
    };
    let mut nodes = vec![
        format!(r#"{{"children": [1, 2], "extras": {{"osage": {{"id": "world"}}{surfaces}}}}}"#),
        r#"{"translation": [6, 0, 0], "extras": {"osage": {"id": "ears"}}}"#.to_owned(),
        // 10 m/s at 10 m from the axis: a radian a second.
        format!(
            r#"{{"children": [{}], "extras": {{"osage": {{"id": "ring"}}, "osageSound": {{"turning": {{"axisM": [0, 1, 0], "degreesPerSecond": {}}}}}}}}}"#,
            children.join(", "),
            1f64.to_degrees()
        ),
    ];
    for i in 0..sources {
        let angle = std::f64::consts::TAU * i as f64 / sources as f64;
        nodes.push(format!(
            r#"{{"translation": [{}, 0, {}], "extras": {{"osage": {{"id": "noise-{i}"}}, "osageSound": {{"emitters": [{{"id": "noise", "voice": "noise", "maxDistanceM": 40}}]}}}}}}"#,
            10.0 * angle.cos(),
            10.0 * angle.sin()
        ));
    }
    let text = format!(
        r#"{{
      "asset": {{"version": "2.0", "extras": {{
        "osageObject": {{"version": 1, "id": "ring-of-{sources}", "collisionMaterials": {{}},
          "acousticMaterials": {{"road": {{"absorption": [0.02, 0.03, 0.05], "transmission": [0.0, 0.0, 0.0], "scattering": 0.1}}}}}},
        "osageSound": {{"version": 0, "contactSounds": {{}}}}}}}},
      "scene": 0,
      "scenes": [{{"nodes": [0], "extras": {{"osageSound": {{"seconds": 10, "seed": 6, "listener": {{"node": ["world", "ears"]}},
        "reverb": {{"secondsT60": [0.6, 0.5, 0.3]}}, "air": {{"celsius": 15, "humidityPercent": 70}}}}}}}}],
      "nodes": [{}]
    }}"#,
        nodes.join(",")
    );
    Scene::from_json(&text).expect("the ring reads")
}

/// Far sound for the field's measures: `sources` noises spread over a
/// sphere of `radius` metres round a listener at its centre (a golden
/// spiral, so that every direction has its share), the sphere turning
/// slowly about the vertical. Nothing mirrors: what is measured is the
/// field against the ways, not the faces.
pub fn sphere(sources: usize, radius: f64) -> Scene {
    let children: Vec<String> = (0..sources).map(|i| (i + 3).to_string()).collect();
    let mut nodes = vec![
        r#"{"children": [1, 2], "extras": {"osage": {"id": "world"}}}"#.to_owned(),
        r#"{"extras": {"osage": {"id": "ears"}}}"#.to_owned(),
        format!(
            r#"{{"children": [{}], "extras": {{"osage": {{"id": "sphere"}}, "osageSound": {{"turning": {{"axisM": [0, 1, 0], "degreesPerSecond": 3}}}}}}}}"#,
            children.join(", ")
        ),
    ];
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    for i in 0..sources {
        // From near the top to near the bottom, a turn of the golden angle at each.
        let y = 1.0 - 2.0 * (i as f64 + 0.5) / sources as f64;
        let across = (1.0 - y * y).sqrt();
        let angle = golden * i as f64;
        nodes.push(format!(
            r#"{{"translation": [{}, {}, {}], "extras": {{"osage": {{"id": "far-{i}"}}, "osageSound": {{"emitters": [{{"id": "noise", "voice": "noise", "maxDistanceM": {}}}]}}}}}}"#,
            radius * across * angle.cos(),
            radius * y,
            radius * across * angle.sin(),
            2.0 * radius
        ));
    }
    let text = format!(
        r#"{{
      "asset": {{"version": "2.0", "extras": {{
        "osageObject": {{"version": 1, "id": "sphere-of-{sources}", "collisionMaterials": {{}}, "acousticMaterials": {{}}}},
        "osageSound": {{"version": 0, "contactSounds": {{}}}}}}}},
      "scene": 0,
      "scenes": [{{"nodes": [0], "extras": {{"osageSound": {{"seconds": 10, "seed": 6, "listener": {{"node": ["world", "ears"]}},
        "reverb": {{"secondsT60": [0.6, 0.5, 0.3]}}, "air": {{"celsius": 15, "humidityPercent": 70}}}}}}}}],
      "nodes": [{}]
    }}"#,
        nodes.join(",")
    );
    Scene::from_json(&text).expect("the sphere reads")
}

/// What far sound costs a processor, ms for a second of sound, for
/// `listeners` listeners in one cube: every emitter heard by its own ways
/// for each listener, against one field made for the cube and decoded for
/// each listener. Returns (ways for one listener, making the field, hearing
/// it for one listener).
pub fn field_cost(scene: &Scene, options: &Options, seconds: f64) -> (f64, f64, f64) {
    use crate::engine::{BLOCK, Engine, FieldEars};
    let far = options.far.expect("a field to measure");
    let blocks = (seconds * crate::engine::RATE as f64 / BLOCK as f64).ceil() as usize;
    let per_second = |spent: f64| spent / seconds * 1000.0;
    let (mut l, mut r) = ([0.0f32; BLOCK], [0.0f32; BLOCK]);
    // Every emitter by its own ways, to one listener.
    let mut whole = Engine::new(scene.clone(), Options { far: None, ..options.clone() });
    let began = cpu_seconds();
    for _ in 0..blocks {
        whole.render(&mut l, &mut r);
    }
    let ways = per_second(cpu_seconds() - began);
    // One field for the cube.
    let mut field_engine = Engine::field(scene.clone(), options.clone());
    let mut field = [[0.0f32; BLOCK]; crate::field::MOST];
    let mut made = Vec::with_capacity(blocks);
    let began = cpu_seconds();
    for _ in 0..blocks {
        field_engine.render_field(&mut field);
        made.push(field);
    }
    let making = per_second(cpu_seconds() - began);
    // Heard by one listener.
    let mut ears = FieldEars::new(far.order, options.quality, options.listening);
    let head = field_engine.head();
    let began = cpu_seconds();
    for block in &made {
        ears.hear(block, &head, &mut l, &mut r);
    }
    let hearing = per_second(cpu_seconds() - began);
    std::hint::black_box((&l, &r));
    (ways, making, hearing)
}

/// What a render cost.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cost {
    /// Milliseconds of one processor for a second of sound: under 1 000, it
    /// keeps up with the clock; at 10, a hundred of it would.
    pub ms_per_second: f64,
    /// Ways rendered and voices made, on average at once.
    pub ways: f64,
    pub voices: f64,
    /// Nanoseconds for one way and one sample, voices and the finding of
    /// ways included.
    pub ns_per_way_sample: f64,
    /// Bytes of the lines kept for the emitters' sound.
    pub ring_bytes: usize,
    /// How the time was shared: touching, finding the ways, voices,
    /// carrying the ways to the ears, reverberation.
    pub shares: [f64; 5],
}

/// What `seconds` of `scene` cost at `options`, lead left out: the median of
/// `runs`, as the baselines of `bench/` take theirs.
pub fn cost(scene: &Scene, options: &Options, seconds: f64, runs: usize) -> Cost {
    let mut found: Vec<Cost> = (0..runs.max(1))
        .map(|_| {
            let before = cpu_seconds();
            let rendered = render(scene, options, seconds);
            let spent = cpu_seconds() - before;
            let tally = rendered.tally;
            // The lead is rendered too: its share of the time is taken off.
            let share = seconds / (seconds + crate::render::LEAD);
            let blocks = tally.blocks.max(1) as f64;
            let clocks = [tally.touching, tally.finding, tally.voicing, tally.carrying, tally.tailing];
            let all: f64 = clocks.iter().sum::<f64>().max(1e-12);
            Cost {
                ms_per_second: spent * share / seconds * 1_000.0,
                ways: tally.ways as f64 / blocks,
                voices: tally.voices as f64 / blocks,
                ns_per_way_sample: spent * share * 1e9 / (tally.ways.max(1) as f64 * crate::engine::BLOCK as f64),
                ring_bytes: tally.ring_bytes,
                shares: clocks.map(|clock| clock / all),
            }
        })
        .collect();
    found.sort_by(|a, b| a.ms_per_second.total_cmp(&b.ms_per_second));
    found[found.len() / 2]
}

/// A quality with no budget of ways: every way is rendered, to see what one
/// costs.
pub fn unbounded(quality: Quality) -> Quality {
    Quality { ways: usize::MAX, ..quality }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ears::Listening;

    #[test]
    fn the_ring_is_the_baselines_scene() {
        let scene = ring(8, false);
        assert_eq!(scene.emitters.len(), 8);
        // 4 to 16 m from the listener as it turns, and back where it was after a turn.
        let far =
            |t: f64| scene.tree.point(scene.emitters[0].node, scene.emitters[0].offset, t).distance(scene.head(t).at);
        let distances: Vec<f64> = (0..63).map(|i| far(i as f64 * 0.1)).collect();
        let (nearest, farthest) =
            (distances.iter().cloned().fold(f64::MAX, f64::min), distances.iter().cloned().fold(0.0, f64::max));
        assert!((nearest - 4.0).abs() < 0.05 && (farthest - 16.0).abs() < 0.05, "{nearest} to {farthest}");
        assert!((far(std::f64::consts::TAU) - far(0.0)).abs() < 1e-9);
        // 10 m/s: a tenth of a second is a metre of the ring.
        let (a, b) = (
            scene.tree.point(scene.emitters[0].node, scene.emitters[0].offset, 0.0),
            scene.tree.point(scene.emitters[0].node, scene.emitters[0].offset, 0.1),
        );
        assert!((a.distance(b) - 1.0).abs() < 0.01);
        assert_eq!(ring(4, true).surfaces.len(), 1);
    }

    #[test]
    fn a_cost_counts_every_source_s_way() {
        let options =
            Options { quality: unbounded(Quality::LOW), listening: Listening::Speakers, ..Options::default() };
        let found = cost(&ring(16, false), &options, 0.5, 1);
        assert_eq!((found.ways, found.voices), (16.0, 16.0));
        assert!(found.ms_per_second > 0.0 && found.ns_per_way_sample > 0.0);
        assert!((found.shares.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        // A road under them: each is mirrored once where mirrors are followed.
        let mirrored = Options { quality: unbounded(Quality::MEDIUM), ..options };
        assert_eq!(cost(&ring(16, true), &mirrored, 0.5, 1).ways, 32.0);
        assert!(cpu_seconds() >= 0.0);
    }
}
