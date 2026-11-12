// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A piece of music decoded a packet at a time into stereo at [`RATE`]:
//! the format found by probing, the first audio track decoded, its sample
//! rate brought to the feed's by linear interpolation, mono heard on both
//! sides and more than two channels heard as their first two.

use std::io::{self, Read};

use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions},
    errors::Error,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::{MediaSource, MediaSourceStream, ReadOnlySource},
    meta::{MetadataOptions, MetadataRevision, StandardTagKey},
    probe::Hint,
    units::Time,
};

use crate::RATE;

/// What a piece says it is, where it says it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
}

impl Tags {
    fn read(revision: &MetadataRevision, into: &mut Self) {
        for tag in revision.tags() {
            let value = tag.value.to_string().trim().to_owned();
            if value.is_empty() {
                continue;
            }
            match tag.std_key {
                Some(StandardTagKey::TrackTitle) if into.title.is_none() => into.title = Some(value),
                Some(StandardTagKey::Artist) if into.artist.is_none() => into.artist = Some(value),
                _ => {}
            }
        }
    }
}

pub struct Decoded {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track: u32,
    rate: u32,
    frames: Option<u64>,
    tags: Tags,
    resample: Resample,
    buffer: Option<SampleBuffer<f32>>,
}

/// A source read once, front to back, that cannot seek: a stream from the
/// network. Behind a lock so the decoder may hold it from its thread.
struct Once(std::sync::Mutex<Box<dyn Read + Send>>);

impl Read for Once {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.get_mut().map_err(|_| io::Error::other("poisoned"))?.read(buf)
    }
}

impl Decoded {
    /// A piece in memory or in a file: it can be sought in.
    pub fn open(source: impl MediaSource + 'static, extension: Option<&str>) -> io::Result<Self> {
        Self::probe(Box::new(source), extension)
    }

    /// A piece from a stream read once: a radio station, say.
    pub fn stream(source: impl Read + Send + 'static, extension: Option<&str>) -> io::Result<Self> {
        let once = Once(std::sync::Mutex::new(Box::new(source)));
        Self::probe(Box::new(ReadOnlySource::new(once)), extension)
    }

    fn probe(source: Box<dyn MediaSource>, extension: Option<&str>) -> io::Result<Self> {
        let mut hint = Hint::new();
        if let Some(extension) = extension {
            hint.with_extension(extension);
        }
        let stream = MediaSourceStream::new(source, Default::default());
        let options = FormatOptions { enable_gapless: true, ..Default::default() };
        let mut probed = symphonia::default::get_probe()
            .format(&hint, stream, &options, &MetadataOptions::default())
            .map_err(failed)?;
        let mut tags = Tags::default();
        if let Some(revision) = probed.metadata.get().as_ref().and_then(|m| m.current()) {
            Tags::read(revision, &mut tags);
        }
        let mut format = probed.format;
        if let Some(revision) = format.metadata().current() {
            Tags::read(revision, &mut tags);
        }
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no audio in it"))?;
        let params = track.codec_params.clone();
        let decoder = symphonia::default::get_codecs().make(&params, &DecoderOptions::default()).map_err(failed)?;
        let rate = params.sample_rate.unwrap_or(RATE);
        Ok(Self {
            track: track.id,
            format,
            decoder,
            rate,
            frames: params.n_frames,
            tags,
            resample: Resample::new(rate),
            buffer: None,
        })
    }

    pub fn tags(&self) -> &Tags {
        &self.tags
    }

    /// How long it lasts, seconds, when its container says.
    pub fn seconds(&self) -> Option<f64> {
        self.frames.map(|frames| frames as f64 / f64::from(self.rate))
    }

    /// Goes to `seconds` from its start (as near as the format allows).
    pub fn seek(&mut self, seconds: f64) -> io::Result<()> {
        let time = Time::new(seconds.trunc() as u64, seconds.fract());
        self.format.seek(SeekMode::Coarse, SeekTo::Time { time, track_id: Some(self.track) }).map_err(failed)?;
        self.decoder.reset();
        self.resample = Resample::new(self.rate);
        Ok(())
    }

    /// The next samples, interleaved stereo at [`RATE`] (possibly none from
    /// a packet); `None` at the end.
    pub fn samples(&mut self) -> io::Result<Option<Vec<f32>>> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(Error::IoError(e)) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(Error::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => return Err(failed(e)),
            };
            if packet.track_id() != self.track {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                // A damaged packet: a click, not the end of the music.
                Err(Error::DecodeError(_)) => continue,
                Err(e) => return Err(failed(e)),
            };
            let spec = *decoded.spec();
            let channels = spec.channels.count().max(1);
            if spec.rate != self.rate {
                self.rate = spec.rate;
                self.resample = Resample::new(spec.rate);
            }
            let buffer = match &mut self.buffer {
                Some(buffer) if buffer.capacity() >= decoded.capacity() * channels => buffer,
                buffer => buffer.insert(SampleBuffer::new(decoded.capacity() as u64, spec)),
            };
            buffer.copy_interleaved_ref(decoded);
            let frames = buffer.samples().chunks_exact(channels).map(|f| [f[0], f[channels.min(2) - 1]]);
            let mut out = Vec::with_capacity(buffer.samples().len() / channels * 2 + 4);
            self.resample.push(frames, &mut out);
            return Ok(Some(out));
        }
    }

    /// How long it lasts, seconds, counting every frame when the container
    /// does not say: a whole decode, for the pieces a host shares.
    pub fn measure(mut self) -> io::Result<f64> {
        if let Some(seconds) = self.seconds() {
            return Ok(seconds);
        }
        let mut samples = 0usize;
        while let Some(chunk) = self.samples()? {
            samples += chunk.len();
        }
        Ok(samples as f64 / 2.0 / f64::from(RATE))
    }
}

fn failed(error: Error) -> io::Error {
    match error {
        Error::IoError(e) => e,
        other => io::Error::new(io::ErrorKind::InvalidData, other.to_string()),
    }
}

/// From a source's rate to [`RATE`], by linear interpolation between the
/// two frames either side of each one wanted.
struct Resample {
    step: f64,
    at: f64,
    held: Vec<[f32; 2]>,
}

impl Resample {
    fn new(from: u32) -> Self {
        Self { step: f64::from(from) / f64::from(RATE), at: 0.0, held: Vec::new() }
    }

    fn push(&mut self, frames: impl Iterator<Item = [f32; 2]>, out: &mut Vec<f32>) {
        if self.step == 1.0 {
            out.extend(frames.flatten());
            return;
        }
        self.held.extend(frames);
        while (self.at as usize) + 1 < self.held.len() {
            let i = self.at as usize;
            let f = (self.at - i as f64) as f32;
            let (a, b) = (self.held[i], self.held[i + 1]);
            out.push(a[0] + (b[0] - a[0]) * f);
            out.push(a[1] + (b[1] - a[1]) * f);
            self.at += self.step;
        }
        let used = (self.at as usize).min(self.held.len());
        self.held.drain(..used);
        self.at -= used as f64;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A WAV file of `seconds` of a tone, at `rate`, mono or stereo.
    pub fn wav(rate: u32, channels: u16, seconds: f32) -> Vec<u8> {
        let frames = (rate as f32 * seconds) as u32;
        let data = frames * u32::from(channels) * 2;
        let mut out = Vec::new();
        out.extend(b"RIFF");
        out.extend((36 + data).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend((rate * u32::from(channels) * 2).to_le_bytes());
        out.extend((channels * 2).to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend(b"data");
        out.extend(data.to_le_bytes());
        for n in 0..frames {
            let s = ((n as f32 / rate as f32 * 440.0 * std::f32::consts::TAU).sin() * 16_000.0) as i16;
            for _ in 0..channels {
                out.extend(s.to_le_bytes());
            }
        }
        out
    }

    fn all(mut decoded: Decoded) -> Vec<f32> {
        let mut samples = Vec::new();
        while let Some(chunk) = decoded.samples().unwrap() {
            samples.extend(chunk);
        }
        samples
    }

    #[test]
    fn a_piece_is_heard_in_stereo_at_the_feeds_rate_whatever_it_was_made_at() {
        for (rate, channels) in [(44_100, 2), (48_000, 1), (22_050, 2)] {
            let decoded = Decoded::open(io::Cursor::new(wav(rate, channels, 1.0)), Some("wav")).unwrap();
            assert_eq!(decoded.seconds(), Some(1.0));
            let samples = all(decoded);
            let frames = samples.len() / 2;
            assert!(frames.abs_diff(RATE as usize) < 8, "{rate} Hz × {channels}: {frames} frames");
            let loud = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
            assert!((0.4..0.6).contains(&loud), "{loud}");
            assert!(samples.as_chunks::<2>().0.iter().all(|f| f[0] == f[1]), "both sides alike");
        }
    }

    #[test]
    fn a_piece_is_sought_in_and_streamed_once() {
        let mut decoded = Decoded::open(io::Cursor::new(wav(44_100, 2, 2.0)), Some("wav")).unwrap();
        decoded.seek(1.5).unwrap();
        let rest = all(decoded).len() / 2;
        assert!(rest.abs_diff(RATE as usize / 2) < 1_200, "{rest}");
        let streamed = Decoded::stream(io::Cursor::new(wav(44_100, 2, 0.5)), None).unwrap();
        assert_eq!(streamed.measure().unwrap(), 0.5);
        assert!(Decoded::open(io::Cursor::new(b"not music at all".to_vec()), None).is_err());
    }
}
