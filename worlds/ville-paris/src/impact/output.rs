// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{Listening, Onset, Playback, Track, frame};
use crate::{
    people::Model,
    physics::{Actor, Physics},
    replay::{Grid, Replay, Route},
    scene::Sector,
};
use anyhow::{Context, Result, ensure};
use osage_paris_voices::RATE;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufWriter, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub struct Live {
    track: Track,
    #[cfg(feature = "sound")]
    _device: Option<rodio::MixerDeviceSink>,
    #[cfg(feature = "sound")]
    mixer: rodio::mixer::Mixer,
    #[cfg(feature = "sound")]
    player: rodio::Player,
    #[cfg(feature = "sound")]
    pending: Option<rodio::queue::SourcesQueueOutput>,
}
impl Live {
    pub fn open(playback: Playback, scene: &Sector) -> Option<Self> {
        #[cfg(feature = "sound")]
        {
            match rodio::DeviceSinkBuilder::open_default_sink() {
                Ok(mut sink) => {
                    sink.log_on_drop(false);
                    Some(Self::on_mixer(sink.mixer().clone(), Some(sink), playback, scene))
                }
                Err(e) => {
                    eprintln!("Ville-Paris audio output unavailable: {e}");
                    None
                }
            }
        }
        #[cfg(not(feature = "sound"))]
        {
            let _ = (playback, scene);
            None
        }
    }
    #[cfg(feature = "sound")]
    fn on_mixer(
        mixer: rodio::mixer::Mixer,
        device: Option<rodio::MixerDeviceSink>,
        playback: Playback,
        scene: &Sector,
    ) -> Self {
        let (player, source) = rodio::Player::new();
        Self { track: Track::new(playback, scene), mixer, player, pending: Some(source), _device: device }
    }

    pub fn reset(&mut self) {
        #[cfg(feature = "sound")]
        {
            self.player.stop();
            let (player, source) = rodio::Player::new();
            self.player = player;
            self.pending = Some(source);
        }
        self.track.previous.clear();
        self.track.strikes.clear();
        if let Some(boundaries) = &mut self.track.mix_boundaries {
            boundaries.clear();
        }
        self.track.frame = 0;
    }
    pub fn pause(&self, paused: bool) {
        #[cfg(feature = "sound")]
        if paused {
            self.player.pause();
        } else {
            self.player.play();
        }
        #[cfg(not(feature = "sound"))]
        let _ = paused;
    }
    fn submit(&mut self, samples: Vec<f32>) {
        #[cfg(feature = "sound")]
        {
            self.player.append(rodio::buffer::SamplesBuffer::new(
                2.try_into().unwrap(),
                RATE.try_into().unwrap(),
                samples,
            ));
            // Attaching an empty player inserts 256 silent stereo frames during conversion.
            if let Some(source) = self.pending.take() {
                self.mixer.add(source);
            }
        }
        #[cfg(not(feature = "sound"))]
        let _ = samples;
    }
    pub fn step(
        &mut self,
        actor: &mut Actor,
        tick: u32,
        replay: &Replay,
        route: &Route,
        scene: &Sector,
        grid: &Grid,
    ) -> Result<()> {
        let (samples, _) = self.track.step(actor, tick, replay, route, scene, grid)?;
        self.submit(samples);
        Ok(())
    }
    pub fn tail(&mut self, ticks: u32, hz: u32) {
        let stop = self.track.frame.saturating_add(frame(ticks, hz));
        while self.track.frame < stop && !self.track.strikes.is_empty() {
            let samples = self.track.mix_until(stop.min(self.track.frame + 4096));
            self.submit(samples);
        }
    }
}

/// Stereo 48 kHz PCM file. The target becomes visible after its final RIFF header is written.
pub(super) struct PcmFile {
    target: PathBuf,
    writer: BufWriter<tempfile::NamedTempFile>,
    frames: u64,
    first: Option<u64>,
    failed: bool,
}

impl PcmFile {
    // Four bytes per stereo frame; RIFF counts another 36 bytes.
    const MAX_FRAMES: u64 = (u32::MAX as u64 - 36) / 4;

    pub fn new(path: &Path) -> Result<Self> {
        let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        fs::create_dir_all(dir)?;
        let mut writer = BufWriter::new(tempfile::NamedTempFile::new_in(dir)?);
        writer.write_all(&[0u8; 44])?;
        Ok(Self { target: path.to_owned(), writer, frames: 0, first: None, failed: false })
    }

    pub fn write(&mut self, samples: Vec<f32>) -> Result<()> {
        ensure!(!self.failed, "a preceding PCM write failed");
        let (pairs, remainder) = samples.as_chunks::<2>();
        ensure!(remainder.is_empty(), "incomplete stereo PCM frame");
        ensure!(pairs.len() as u64 <= Self::MAX_FRAMES - self.frames, "audio exceeds PCM WAV size limit");
        // An I/O failure can leave half a stereo frame in the temporary file.
        self.failed = true;
        for pair in pairs {
            let pcm = pair.map(|x| (x * 32767.).round() as i16);
            if self.first.is_none() && pcm != [0, 0] {
                self.first = Some(self.frames);
            }
            for sample in pcm {
                self.writer.write_all(&sample.to_le_bytes())?;
            }
            self.frames += 1;
        }
        self.failed = false;
        Ok(())
    }

    pub fn finish(self) -> Result<(u64, Option<u64>)> {
        ensure!(!self.failed, "cannot publish a WAV after a failed PCM write");
        let bytes = u32::try_from(self.frames * 4)?;
        let mut file = self.writer.into_inner().map_err(|e| e.into_error())?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(b"RIFF")?;
        file.write_all(&(36 + bytes).to_le_bytes())?;
        file.write_all(b"WAVEfmt ")?;
        file.write_all(&16u32.to_le_bytes())?;
        file.write_all(&1u16.to_le_bytes())?;
        file.write_all(&2u16.to_le_bytes())?;
        file.write_all(&RATE.to_le_bytes())?;
        file.write_all(&(RATE * 4).to_le_bytes())?;
        file.write_all(&4u16.to_le_bytes())?;
        file.write_all(&16u16.to_le_bytes())?;
        file.write_all(b"data")?;
        file.write_all(&bytes.to_le_bytes())?;
        file.persist(&self.target).with_context(|| format!("publishing {}", self.target.display()))?;
        Ok((self.frames, self.first))
    }
}

pub fn record(
    path: &Path,
    physics: Physics,
    model: &Model,
    scene: &Sector,
    replay: &Replay,
    route: &Route,
    playback: Playback,
) -> Result<Value> {
    ensure!(frame(route.ticks, replay.tick_hz) <= PcmFile::MAX_FRAMES, "route exceeds PCM WAV size limit");
    let mut file = PcmFile::new(path)?;
    let mut track = Track::new(playback, scene);
    let mut actor = Actor::new(physics, model, scene, replay, route)?;
    let grid = Grid::new(&scene.supports);
    let mut events = Vec::<Onset>::new();
    for tick in 1..=route.ticks {
        let (samples, onsets) = track.step(&mut actor, tick, replay, route, scene, &grid)?;
        events.extend(onsets);
        file.write(samples)?;
    }
    if let Some(stop) = track.strikes.iter().map(|s| s.end).max() {
        ensure!(stop <= PcmFile::MAX_FRAMES, "impact arrival exceeds PCM WAV size limit");
        while track.frame < stop {
            file.write(track.mix_until(stop.min(track.frame + 4096)))?;
        }
    }
    let (written, first) = file.finish()?;
    let mode = match playback.listening {
        Listening::Speakers => "speakers",
        Listening::Headphones => "headphones",
    };
    let mut report = json!({"listening":mode,"direct_only":playback.direct_only, "route":route.id,"physics":physics.to_string(),"actor":model.to_string(),"sample_rate":RATE,
        "frames":written,"tick_hz":replay.tick_hz,"first_nonzero_pcm_frame":first,"onsets":events});
    if let Some(storage) = scene.object.as_ref().and_then(|object| object.sound.index_storage()) {
        report["acoustic_index"] = serde_json::to_value(storage)?;
    }
    Ok(report)
}

#[cfg(test)]
mod file_tests {
    use super::*;
    #[test]
    fn only_a_finished_stereo_file_replaces_the_previous_recording() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/audio.wav");
        let mut empty = PcmFile::new(&path).unwrap();
        assert!(!path.exists());
        assert!(empty.write(vec![0.]).is_err());
        assert_eq!(empty.finish().unwrap(), (0, None));
        let original = fs::read(&path).unwrap();
        assert_eq!(original.len(), 44);
        let mut partial = PcmFile::new(&path).unwrap();
        partial.write(vec![1., -1.]).unwrap();
        drop(partial);
        assert_eq!(fs::read(&path).unwrap(), original);
        let mut writer = PcmFile::new(&path).unwrap();
        writer.write(vec![0., 0.]).unwrap();
        writer.write(vec![0.5, -0.25, 1., -1.]).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(writer.finish().unwrap(), (3, Some(1)));
        let file = fs::read(&path).unwrap();
        assert_eq!(&file[..4], b"RIFF");
        assert_eq!(&file[4..8], &48u32.to_le_bytes());
        assert_eq!(&file[24..28], &48000u32.to_le_bytes());
        assert_eq!(&file[40..44], &12u32.to_le_bytes());
        let pcm: Vec<_> = file[44..].as_chunks::<2>().0.iter().map(|s| i16::from_le_bytes(*s)).collect();
        assert_eq!(pcm, [0, 0, 16384, -8192, 32767, -32767]);
    }
}

#[cfg(all(test, feature = "sound"))]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn live_stereo_player_matches_the_recorded_drop_and_reset_discards_future_sound() {
        for listening in [Listening::Speakers, Listening::Headphones] {
            check_live(listening, false);
        }
    }

    #[test]
    fn live_mixer_and_file_receive_the_same_filtered_impact_through_the_upper_deck() {
        for listening in [Listening::Speakers, Listening::Headphones] {
            check_live(listening, true);
        }
    }

    fn check_live(listening: Listening, through_deck: bool) {
        let scene = Sector::read(crate::scene::DEFAULT).unwrap();
        let mut replay = Replay::read(crate::replay::DEFAULT, scene.colliders.iter().map(|s| s.id.as_str())).unwrap();
        if through_deck {
            let route = replay.routes.iter_mut().find(|r| r.id == "road").unwrap();
            for key in &mut route.camera {
                key.eye = [0., 18., 0.];
                key.aim = [0., 18., -1.];
            }
        }
        let road = replay.routes.iter().find(|r| r.id == "road").unwrap();
        let grid = Grid::new(&scene.supports);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("road.wav");
        let playback = Playback { listening, direct_only: false };
        let report = record(&path, Physics::Point, &Model::Capsule, &scene, &replay, road, playback).unwrap();
        assert_eq!(report["onsets"].as_array().unwrap().len(), 1);
        if through_deck {
            let layers = report["onsets"][0]["direct_path"]["crossings"].as_array().unwrap();
            assert_eq!(layers.iter().map(|x| x["id"].as_str().unwrap()).collect::<Vec<_>>(), ["underside", "top"]);
            let path = &report["onsets"][0]["diffracted_path"];
            let bends = path["bends"].as_array().unwrap();
            assert_eq!(bends.iter().map(|b| b["surface"].as_str().unwrap()).collect::<Vec<_>>(), ["underside", "top"]);
            assert!(path["arrival_frame"].as_u64().unwrap() > report["onsets"][0]["arrival_frame"].as_u64().unwrap());
        }
        let mut wav = Vec::new();
        std::fs::File::open(&path).unwrap().read_to_end(&mut wav).unwrap();
        let (mixer, mut output) = rodio::mixer::mixer(2.try_into().unwrap(), RATE.try_into().unwrap());
        let mut live = Live::on_mixer(mixer, None, playback, &scene);
        let mut actor = Actor::new(Physics::Point, &Model::Capsule, &scene, &replay, road).unwrap();
        for tick in 1..=road.ticks {
            live.step(&mut actor, tick, &replay, road, &scene, &grid).unwrap();
        }
        let expected = wav[44..].as_chunks::<2>().0;
        let actual: Vec<i16> =
            output.by_ref().take(expected.len() + 8192).map(|x| (x * 32767.).round() as i16).collect();
        let recorded: Vec<i16> = expected.iter().map(|bytes| i16::from_le_bytes(*bytes)).collect();
        let first = recorded.iter().position(|&x| x != 0).unwrap();
        let audible = actual.iter().position(|&x| x != 0).unwrap();
        let lag = audible as isize - first as isize;
        eprintln!(
            "PCM first expected {first} live {audible}, lag {lag} interleaved samples, peaks {:?} {:?}",
            recorded.iter().max(),
            actual.iter().max()
        );
        let length = recorded.len().saturating_sub(first).min(actual.len().saturating_sub(audible));
        let differences = actual[audible..audible + length]
            .iter()
            .zip(&recorded[first..first + length])
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!((lag, differences), (0, 0));
        // Restart during propagation, after the contact but before the wave reaches the camera.
        live.reset();
        let mut actor = Actor::new(Physics::Point, &Model::Capsule, &scene, &replay, road).unwrap();
        let hit = report["onsets"][0]["tick"].as_u64().unwrap() as u32;
        for tick in 1..=hit {
            live.step(&mut actor, tick, &replay, road, &scene, &grid).unwrap();
        }
        assert!(!live.track.strikes.is_empty());
        live.reset();
        assert_eq!(live.track.frame, 0);
        assert!(live.track.strikes.is_empty());
        live.tail(30, replay.tick_hz);
        // Consume past the discarded wave's arrival on the same software output.
        assert!(output.by_ref().take((RATE / 2) as usize).all(|s| s == 0.));

        live.pause(true);
        assert!(live.player.is_paused());
        live.pause(false);
        assert!(!live.player.is_paused());
    }
}
