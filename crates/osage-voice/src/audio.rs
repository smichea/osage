// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The voices as a bevy audio source: add [`VoicePlugin`], put a
//! [`VoiceSource`] made from the player's feed in the assets, and play it
//! with an `AudioPlayer`.

use std::{
    num::NonZero,
    sync::{Arc, Mutex},
    time::Duration,
};

use ::bevy::{
    audio::{AddAudioSource, ChannelCount, Decodable, Sample, SampleRate, Source},
    prelude::*,
};

use crate::{Feed, RATE};

pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<VoiceSource>();
    }
}

/// The feed, handed to the audio output when it starts the source.
#[derive(Asset, TypePath)]
pub struct VoiceSource(pub Arc<Mutex<Option<Feed>>>);

impl VoiceSource {
    pub fn new(feed: Feed) -> Self {
        Self(Arc::new(Mutex::new(Some(feed))))
    }
}

impl Decodable for VoiceSource {
    type Decoder = Heard;

    fn decoder(&self) -> Heard {
        Heard(self.0.lock().ok().and_then(|mut feed| feed.take()))
    }
}

/// The voices as the audio output reads them.
pub struct Heard(Option<Feed>);

impl Iterator for Heard {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        Some(self.0.as_mut().map_or(0.0, Feed::sample))
    }
}

impl Source for Heard {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        NonZero::<u16>::MIN
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(RATE).expect("a rate")
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
