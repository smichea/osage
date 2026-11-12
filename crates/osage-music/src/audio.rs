// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A feed as a bevy audio source: add [`MusicPlugin`], put a
//! [`MusicSource`] made from the player's feed in the assets, and play it
//! with an `AudioPlayer` (looping does not matter: it never ends).

use std::{num::NonZero, time::Duration};

use ::bevy::{
    audio::{AddAudioSource, ChannelCount, Decodable, Sample, SampleRate, Source},
    prelude::*,
};

use crate::{CHANNELS, Feed, Listening, RATE};

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<MusicSource>();
    }
}

#[derive(Asset, TypePath)]
pub struct MusicSource(pub Feed);

impl Decodable for MusicSource {
    type Decoder = Heard;

    fn decoder(&self) -> Heard {
        Heard(self.0.listen())
    }
}

/// A feed as the audio output reads it.
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
