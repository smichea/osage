// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Client-side prediction of the player's own kart (D-031): every step is
//! applied locally at once and sent; when the world's state comes back with
//! the last step it applied, the client restarts from it and replays the
//! steps the world has not applied yet. The difference with what was shown
//! is not snapped but faded out. (The browser client's `prediction.ts`, in
//! Rust.)

use std::collections::VecDeque;

use osage_kart_physics::{Controls, Kart, Track, step, wrap};
use osage_world_kart::proto;

/// Steps already sent that go again with the new ones, against datagram loss.
const REDUNDANCY: usize = 3;
/// At most this many steps in one message.
const MAX_SENT: usize = 24;
/// Beyond this error, metres, the kart jumps instead of gliding back.
const SNAP_DISTANCE: f32 = 4.0;

#[derive(Default)]
pub struct Predictor {
    /// The predicted present state; `None` until the world's first state.
    pub kart: Option<Kart>,
    /// The state one step earlier: frames are drawn between the two.
    previous: Option<Kart>,
    /// Steps not applied by the world yet: sequence, controls, use button.
    pending: VecDeque<(u64, Controls, bool)>,
    sent: u64,
    /// What is drawn minus the prediction (x, z, heading): fades to zero.
    pub error: [f32; 3],
}

impl Predictor {
    /// Applies one step locally; `using` (the use button) goes to the world,
    /// which alone decides what items do.
    pub fn step(&mut self, controls: Controls, using: bool, track: &Track) {
        self.sent += 1;
        self.pending.push_back((self.sent, controls, using));
        self.previous = self.kart;
        if let Some(kart) = &mut self.kart {
            step(kart, controls, track);
        }
    }

    /// How far the prediction runs ahead of the world's last state, seconds:
    /// the steps the world has not applied yet.
    pub fn lead(&self) -> f32 {
        self.pending.len() as f32 * osage_kart_physics::DT
    }

    /// What to send after `fresh` new steps, oldest first: those and a few
    /// sent before, in one message per frame however many steps it took.
    pub fn to_send(&self, fresh: usize) -> Vec<proto::Step> {
        let count = (fresh + REDUNDANCY).min(MAX_SENT).min(self.pending.len());
        let steps = self.pending.iter().skip(self.pending.len() - count);
        steps
            .map(|(sequence, c, using)| proto::Step {
                sequence: *sequence,
                throttle: c.throttle,
                steer: c.steer,
                drift: c.drift,
                use_item: *using,
            })
            .collect()
    }

    /// The world's state of this kart, after it applied step `applied`.
    pub fn reconcile(&mut self, server: Kart, applied: u64, track: &Track) {
        while self.pending.front().is_some_and(|(sequence, ..)| *sequence <= applied) {
            self.pending.pop_front();
        }
        let (mut kart, mut before) = (server, server);
        for (_, controls, _) in &self.pending {
            before = kart;
            step(&mut kart, *controls, track);
        }
        if let Some(old) = self.kart {
            let (dx, dz) = (old.x + self.error[0] - kart.x, old.z + self.error[1] - kart.z);
            self.error = if dx.hypot(dz) > SNAP_DISTANCE {
                [0.0; 3]
            } else {
                [dx, dz, wrap(old.heading + self.error[2] - kart.heading)]
            };
        }
        (self.kart, self.previous) = (Some(kart), Some(before));
    }

    /// Fades the visual error over `seconds`.
    pub fn fade(&mut self, seconds: f32) {
        let keep = (-seconds * 12.0).exp();
        self.error = self.error.map(|e| e * keep);
    }

    /// The kart to draw `alpha` (0..1) of the way from the previous step to
    /// the present one, visual error included.
    pub fn shown(&self, alpha: f32) -> Option<Kart> {
        let (now, before) = (self.kart?, self.previous.unwrap_or(self.kart?));
        let mix = |a: f32, b: f32| a + (b - a) * alpha;
        Some(Kart {
            x: mix(before.x, now.x) + self.error[0],
            z: mix(before.z, now.z) + self.error[1],
            heading: before.heading + wrap(now.heading - before.heading) * alpha + self.error[2],
            ..now
        })
    }

    /// Steps the world has not applied yet.
    #[cfg(test)]
    pub fn unacknowledged(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track::new(vec![[0.0, 0.0], [0.0, 200.0], [-100.0, 200.0], [-100.0, 0.0]], 6.0, 3.0)
    }

    const GO: Controls = Controls { throttle: 1.0, steer: 0.0, drift: false };

    #[test]
    fn replaying_unapplied_steps_gives_the_same_prediction() {
        let track = track();
        let mut predictor = Predictor::default();
        predictor.reconcile(Kart::default(), 0, &track);
        let mut server = Kart::default();
        for i in 1..=20u64 {
            predictor.step(GO, false, &track);
            if i <= 12 {
                step(&mut server, GO, &track);
            }
        }
        let predicted = predictor.kart.unwrap();
        // The world applied steps 1 to 12 the same way: nothing to correct.
        predictor.reconcile(server, 12, &track);
        assert_eq!(predictor.kart.unwrap(), predicted);
        assert_eq!(predictor.unacknowledged(), 8);
        assert_eq!(predictor.error, [0.0; 3]);
    }

    #[test]
    fn a_correction_is_faded_in_not_snapped() {
        let track = track();
        let mut predictor = Predictor::default();
        predictor.reconcile(Kart::default(), 0, &track);
        for _ in 0..10 {
            predictor.step(GO, false, &track);
        }
        let shown_before = predictor.shown(1.0).unwrap();
        // Another kart pushed ours half a metre aside on the world.
        let mut server = Kart::default();
        for _ in 0..10 {
            step(&mut server, GO, &track);
        }
        server.x += 0.5;
        predictor.reconcile(server, 10, &track);
        let shown_after = predictor.shown(1.0).unwrap();
        assert!((shown_after.x - shown_before.x).abs() < 1e-5, "no jump on screen");
        assert!((predictor.kart.unwrap().x - 0.5).abs() < 1e-5, "but the prediction is corrected");
        predictor.fade(1.0);
        assert!(predictor.error[0].abs() < 1e-3, "and the screen catches up");
    }

    #[test]
    fn a_frame_sends_its_steps_with_the_few_before() {
        let track = track();
        let mut predictor = Predictor::default();
        for _ in 0..8 {
            predictor.step(GO, false, &track);
        }
        let sequences = |fresh| predictor.to_send(fresh).iter().map(|s| s.sequence).collect::<Vec<_>>();
        assert_eq!(sequences(1), [5, 6, 7, 8]);
        assert_eq!(sequences(3), [3, 4, 5, 6, 7, 8]);
    }
}
