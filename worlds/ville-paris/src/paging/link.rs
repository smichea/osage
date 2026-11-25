// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deterministic reliable-content link on the replay's tick clock.
use serde::Serialize;

#[derive(Clone, Default, Serialize)]
pub(super) struct Link {
    pub seed: u64,
    pub jitter_ms: u32,
    /// Both lanes lose connectivity on the half-open tick interval.
    pub outage: Option<[u32; 2]>,
}

impl Link {
    /// Work may cross a single outage boundary. Outstanding work resumes with
    /// the number of service ticks it still needed at disconnection.
    pub fn advance(&self, start: u32, work: u32) -> u32 {
        let nominal = start.saturating_add(work);
        match self.outage {
            Some([a, b]) if start < b && nominal >= a => b.saturating_add(nominal - a.max(start)),
            _ => nominal,
        }
    }

    pub fn latency_ticks(&self, base_ms: u32, hz: u32, request_tick: u32, resource: u64, physical: bool) -> u32 {
        let mut ms = u64::from(base_ms);
        if self.jitter_ms > 0 {
            // Mix the identity of each request so iteration order and thread scheduling
            // cannot influence the delay drawn for a given page, lane and start tick.
            let sample = mix(self.seed ^ mix(resource) ^ mix(u64::from(request_tick) * 2 + u64::from(physical)));
            let span = u64::from(self.jitter_ms);
            ms = (ms + sample % (2 * span + 1)).saturating_sub(span);
        }
        u32::try_from((u128::from(ms) * u128::from(hz)).div_ceil(1000)).unwrap_or(u32::MAX)
    }
}

fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_cut_suspends_both_pending_and_new_reads_and_resumes_the_remaining_work() {
        let l = Link { outage: Some([4, 10]), ..Default::default() };
        for (start, work, end) in [(1, 2, 3), (1, 3, 10), (1, 8, 15), (5, 2, 12), (9, 0, 10), (10, 3, 13)] {
            assert_eq!(l.advance(start, work), end);
        }
        assert_eq!(l.advance(l.advance(0, 6), 1), 13);
    }

    #[test]
    fn seed_controls_jitter_and_the_default_clock_keeps_fixed_latency() {
        let cases = |seed| {
            let l = Link { seed, jitter_ms: 20, ..Default::default() };
            (0..40).map(|t| l.latency_ticks(80, 1000, t, 0x23fa, false)).collect::<Vec<_>>()
        };
        let values = cases(710);
        assert_eq!(values, cases(710));
        assert_ne!(values, cases(711));
        assert!(values.iter().all(|t| (60..=100).contains(t)));
        assert_eq!(Link::default().latency_ticks(80, 60, 11, 0, true), 5);
    }
}
