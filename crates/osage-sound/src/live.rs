// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The engine played as it goes: on a thread of its own, one block ahead of
//! what is heard, so that the audio output never waits on it.
//!
//! [`Live::start`] starts the engine on a scene and returns once the sound
//! already on its way when it starts has been made (the same lead as an
//! offline render). A game tells it where nodes are as it learns it
//! ([`Live::drive`]): the listener's node from the camera or the avatar,
//! the emitters' nodes from what the server sends. The audio output reads a
//! [`Feed`] sample by sample ([`Listening::sample`]); with the `bevy`
//! feature, [`bevy::LiveSource`] is that feed as a Bevy audio source.
//!
//! **A few blocks ahead.** The thread keeps `ahead` blocks made and waiting
//! for the output, and makes the next as soon as the output takes one —
//! with every pose told until that moment. An audio output does not take
//! samples one at a time but a buffer's worth at once (often 10 to 20 ms),
//! so `ahead` is that buffer in blocks, and a little more: then the output
//! never finds the engine short. A pose is heard after the blocks already
//! made and the one made with it: from `ahead` to `ahead + 1` blocks of
//! 2.7 ms. [`Report`] measures it as it goes.
//!
//! **No waiting.** If the next block is not there when the output needs
//! it, the output hears silence for that sample and the shortfall is
//! counted ([`Report::underruns`]); the engine never makes the output wait.
//!
//! **The same samples.** Poses told in advance ([`Told`], a recorded
//! replay) are given to the engine at the same blocks as
//! [`crate::render::render_told`] gives them: the same scene, options and
//! replay give the same samples live and offline, block for block.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::Instant,
};

use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded, unbounded};

use crate::{
    engine::{BLOCK, Options, Pose},
    render::{Hearing, LEAD, Replay, Told},
    scene::Scene,
};

pub use crate::engine::RATE;

/// Two channels, left then right in turn.
pub const CHANNELS: u16 = 2;

enum Order {
    Pose { node: usize, pose: Pose, sent: Instant },
}

/// A block made, as the output reads it.
struct Block {
    /// Left and right in turn.
    samples: Vec<f32>,
    /// When the first of the poses given to the engine for this block was
    /// told, if any were.
    told: Option<Instant>,
}

#[derive(Default)]
struct Counts {
    underruns: AtomicU64,
    blocks: AtomicU64,
    latency_count: AtomicU64,
    latency_sum_ns: AtomicU64,
    latency_max_ns: AtomicU64,
}

/// What playing has cost so far.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Report {
    /// Blocks the output has started on.
    pub blocks: u64,
    /// Samples (of each channel) the output found no block for and heard
    /// as silence.
    pub underruns: u64,
    /// From a pose told to the moment the output starts on the first block
    /// made with it, s: the mean and the longest. The output's own buffer
    /// comes on top.
    pub latency_mean: f64,
    pub latency_max: f64,
    /// How many poses that is measured over.
    pub latency_count: u64,
}

/// The engine playing.
pub struct Live {
    orders: Sender<Order>,
    feed: Feed,
    thread: Option<JoinHandle<()>>,
}

impl Live {
    /// Starts `scene` heard as `options` ask, from their `start`, with the
    /// poses of `told` given at their instants, keeping `ahead` blocks made
    /// for the output (at least one). Returns once they are ready.
    pub fn start(scene: Scene, options: Options, told: &[Told], ahead: usize) -> Live {
        let ahead = ahead.max(1);
        let (orders, taken) = unbounded::<Order>();
        let (made, blocks) = bounded::<Block>(ahead);
        let (ready, started) = bounded::<()>(1);
        let counts = Arc::new(Counts::default());
        let mut replay = Replay::new(told);
        let thread = std::thread::Builder::new()
            .name("osage-sound".to_owned())
            .spawn(move || {
                let start = options.start;
                let mut engine = Hearing::new(scene, Options { start: start - LEAD, ..options });
                let (mut l, mut r) = ([0.0f32; BLOCK], [0.0f32; BLOCK]);
                // What left its emitters before the start is on its way.
                let lead = (LEAD * RATE as f64 / BLOCK as f64).round() as usize;
                for _ in 0..lead {
                    replay.tell(&mut engine);
                    engine.render(&mut l, &mut r);
                }
                let mut count = 0;
                loop {
                    let mut told = None;
                    loop {
                        match taken.try_recv() {
                            Ok(Order::Pose { node, pose, sent }) => {
                                engine.drive(node, pose);
                                told = Some(told.map_or(sent, |earlier: Instant| earlier.min(sent)));
                            }
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => return,
                        }
                    }
                    replay.tell(&mut engine);
                    engine.render(&mut l, &mut r);
                    let samples = l.iter().zip(&r).flat_map(|(l, r)| [*l, *r]).collect();
                    // Waits while `ahead` blocks are waiting for the output.
                    if made.send(Block { samples, told }).is_err() {
                        return;
                    }
                    count += 1;
                    if count == ahead {
                        let _ = ready.send(());
                    }
                }
            })
            .expect("a thread for the sound");
        let _ = started.recv();
        Live { orders, feed: Feed { blocks, counts }, thread: Some(thread) }
    }

    /// Places `node` as the game now knows it, from the next block made
    /// ([`crate::engine::Engine::drive`]).
    pub fn drive(&self, node: usize, pose: Pose) {
        let _ = self.orders.send(Order::Pose { node, pose, sent: Instant::now() });
    }

    /// What the audio output reads.
    pub fn feed(&self) -> Feed {
        self.feed.clone()
    }

    pub fn report(&self) -> Report {
        self.feed.report()
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        // The thread ends when it next finds no one to take orders from, or
        // when every feed is gone. It is not waited for: an audio output may
        // still hold a feed.
        let (orders, _) = unbounded();
        drop(std::mem::replace(&mut self.orders, orders));
        while self.feed.blocks.try_recv().is_ok() {}
        drop(self.thread.take());
    }
}

/// The blocks as they are made, for the audio output.
#[derive(Clone)]
pub struct Feed {
    blocks: Receiver<Block>,
    counts: Arc<Counts>,
}

impl Feed {
    pub fn listen(&self) -> Listening {
        Listening { feed: self.clone(), block: Vec::new(), at: 0, left: true }
    }

    pub fn report(&self) -> Report {
        let c = &self.counts;
        let count = c.latency_count.load(Ordering::Relaxed);
        let ns = |n: u64| n as f64 * 1e-9;
        Report {
            blocks: c.blocks.load(Ordering::Relaxed),
            underruns: c.underruns.load(Ordering::Relaxed),
            latency_mean: if count > 0 { ns(c.latency_sum_ns.load(Ordering::Relaxed)) / count as f64 } else { 0.0 },
            latency_max: ns(c.latency_max_ns.load(Ordering::Relaxed)),
            latency_count: count,
        }
    }
}

/// A feed being heard: left and right samples in turn, endlessly.
pub struct Listening {
    feed: Feed,
    block: Vec<f32>,
    at: usize,
    /// Whether the next sample is a left one.
    left: bool,
}

impl Listening {
    /// The next sample, left and right in turn; silence, counted, if the
    /// engine has not made it yet. Never waits.
    pub fn sample(&mut self) -> f32 {
        if self.at >= self.block.len() {
            // A block is begun on the left only; a frame of silence is one
            // sample short in each channel.
            if !self.left {
                self.left = true;
                return 0.0;
            }
            match self.feed.blocks.try_recv() {
                Ok(block) => self.begin(block),
                Err(_) => {
                    self.feed.counts.underruns.fetch_add(1, Ordering::Relaxed);
                    self.left = false;
                    return 0.0;
                }
            }
        }
        let sample = self.block[self.at];
        self.at += 1;
        self.left = self.at.is_multiple_of(2);
        sample
    }

    /// The next whole block, waiting for it: left and right in turn. For a
    /// check that reads every sample the engine makes, not for an output.
    pub fn next_block(&mut self) -> Vec<f32> {
        let block = self.feed.blocks.recv().expect("the engine is playing");
        self.begin(block);
        self.at = self.block.len();
        self.block.clone()
    }

    fn begin(&mut self, block: Block) {
        let counts = &self.feed.counts;
        counts.blocks.fetch_add(1, Ordering::Relaxed);
        if let Some(told) = block.told {
            let ns = told.elapsed().as_nanos() as u64;
            counts.latency_count.fetch_add(1, Ordering::Relaxed);
            counts.latency_sum_ns.fetch_add(ns, Ordering::Relaxed);
            counts.latency_max_ns.fetch_max(ns, Ordering::Relaxed);
        }
        self.block = block.samples;
        self.at = 0;
        self.left = true;
    }
}

#[cfg(feature = "bevy")]
pub mod bevy {
    //! The engine playing as a Bevy audio source: add [`LivePlugin`], put a
    //! [`LiveSource`] made from [`Live::feed`](super::Live::feed) in the
    //! assets and play it with an `AudioPlayer` — it never ends. Bevy's
    //! output resamples it to the device's rate.

    use std::{num::NonZero, time::Duration};

    use ::bevy::{
        audio::{AddAudioSource, ChannelCount, Decodable, Sample, SampleRate, Source},
        prelude::*,
    };

    use super::{CHANNELS, Feed, Listening, RATE};

    pub struct LivePlugin;

    impl Plugin for LivePlugin {
        fn build(&self, app: &mut App) {
            app.add_audio_source::<LiveSource>();
        }
    }

    #[derive(Asset, TypePath)]
    pub struct LiveSource(pub Feed);

    impl Decodable for LiveSource {
        type Decoder = Heard;

        fn decoder(&self) -> Heard {
            Heard(self.0.listen())
        }
    }

    /// The feed as the audio output reads it.
    pub struct Heard(Listening);

    impl Iterator for Heard {
        type Item = Sample;

        fn next(&mut self) -> Option<Sample> {
            Some(self.0.sample())
        }
    }

    impl Source for Heard {
        fn current_span_len(&self) -> Option<usize> {
            None
        }

        fn channels(&self) -> ChannelCount {
            NonZero::new(CHANNELS).expect("two channels")
        }

        fn sample_rate(&self) -> SampleRate {
            NonZero::new(RATE).expect("a rate")
        }

        fn total_duration(&self) -> Option<Duration> {
            None
        }
    }
}
