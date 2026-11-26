// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The recordings a scene's voices play, read from the files its content
//! names: Ogg Vorbis (with `lewton`) or WAV, mixed down to one channel and
//! brought to the engine's rate, with their levels of detail
//! ([`osage_sound_voices::recorded`]).

use std::{path::Path, sync::Arc};

use osage_sound_voices::recorded::Clip;

use crate::scene::Scene;

/// A recording from the bytes of an Ogg Vorbis file, its channels mixed.
pub fn from_ogg(bytes: &[u8], looped: bool) -> Result<Clip, String> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(bytes))
        .map_err(|why| format!("not Ogg Vorbis: {why}"))?;
    let (rate, channels) = (reader.ident_hdr.audio_sample_rate, usize::from(reader.ident_hdr.audio_channels.max(1)));
    let mut samples = Vec::new();
    while let Some(packet) = reader.read_dec_packet_itl().map_err(|why| format!("a broken Ogg packet: {why}"))? {
        samples.extend(
            packet
                .chunks(channels)
                .map(|frame| frame.iter().map(|&s| f32::from(s)).sum::<f32>() / (channels as f32 * 32_768.0)),
        );
    }
    if samples.is_empty() {
        return Err("no sound in it".to_owned());
    }
    Ok(Clip::new(&samples, rate, looped))
}

/// A recording from the bytes of a WAV file (16-bit or float), its
/// channels mixed.
pub fn from_wav(bytes: &[u8], looped: bool) -> Result<Clip, String> {
    let audio = osage_sound_analysis::wav::parse(bytes).map_err(|why| format!("not a WAV file: {why}"))?;
    let channels = audio.channels.len().max(1) as f32;
    let samples: Vec<f32> =
        (0..audio.frames()).map(|i| audio.channels.iter().map(|channel| channel[i]).sum::<f32>() / channels).collect();
    if samples.is_empty() {
        return Err("no sound in it".to_owned());
    }
    Ok(Clip::new(&samples, audio.rate, looped))
}

impl Scene {
    /// Reads every recording the scene names from under `root`, the
    /// assets' folder. A recording that cannot be read is an error: a
    /// world that names one means it to be heard.
    pub fn load_clips(&mut self, root: &Path) -> Result<(), String> {
        for (name, source) in &mut self.clips {
            let path = root.join(&source.uri);
            let bytes =
                std::fs::read(&path).map_err(|why| format!("recording \"{name}\" ({}): {why}", path.display()))?;
            let clip = if source.uri.ends_with(".ogg") {
                from_ogg(&bytes, source.looped)
            } else {
                from_wav(&bytes, source.looped)
            }
            .map_err(|why| format!("recording \"{name}\" ({}): {why}", path.display()))?;
            source.clip = Some(Arc::new(clip));
        }
        Ok(())
    }

    /// Gives the recording `name` its samples, read some other way (a
    /// browser fetches them).
    pub fn set_clip(&mut self, name: &str, clip: impl Into<Arc<Clip>>) -> bool {
        match self.clips.get_mut(name) {
            Some(source) => {
                source.clip = Some(clip.into());
                true
            }
            None => false,
        }
    }
}
