// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use bevy::{animation::ActiveAnimation, prelude::*};
use std::time::Duration;

/// Crossfade to a loop. If it is already fading out, keep its running clock and
/// event history. A newly started loop begins at the supplied time in seconds.
pub fn crossfade_loop<'a>(
    player: &'a mut AnimationPlayer,
    transitions: &mut AnimationTransitions,
    node: AnimationNodeIndex,
    fade: Duration,
    start_seconds: f32,
) -> &'a mut ActiveAnimation {
    let running = player.animation(node).copied();
    if running.is_some() && transitions.get_main_animation() == Some(node) {
        return player.animation_mut(node).unwrap().repeat();
    }
    let active = transitions.play(player, node, fade);
    if let Some(running) = running {
        *active = running;
    } else {
        active.seek_to(start_seconds);
    }
    active.repeat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reversal_keeps_the_live_loop_and_a_retired_loop_starts_at_its_entry_time() {
        let mut player = AnimationPlayer::default();
        let mut transitions = AnimationTransitions::new();
        let walk = AnimationNodeIndex::new(1);
        let run = AnimationNodeIndex::new(2);
        let fade = Duration::from_millis(250);
        crossfade_loop(&mut player, &mut transitions, walk, fade, 0.3).set_seek_time(0.72);
        crossfade_loop(&mut player, &mut transitions, run, fade, 0.15);
        assert_eq!(crossfade_loop(&mut player, &mut transitions, walk, fade, 0.3).seek_time(), 0.72);
        assert_eq!(crossfade_loop(&mut player, &mut transitions, walk, fade, 0.3).seek_time(), 0.72);
        player.stop(run);
        assert_eq!(crossfade_loop(&mut player, &mut transitions, run, fade, 0.15).seek_time(), 0.15);
    }
}
