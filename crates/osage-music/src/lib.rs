// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Music for a world's players, from wherever it comes: files on the
//! player's computer or shipped with the world, internet radio stations
//! (a list of the player's, or the radio-browser.info directory when the
//! player turns it on), or the playlist of the person hosting the game,
//! shared on the local network and heard by everyone at the same point.
//!
//! A [`Player`] runs a [`Program`] on a thread of its own: it fetches,
//! decodes (Ogg Vorbis, MP3, AAC, FLAC, WAV), and hands the samples to a
//! [`Feed`] — stereo at [`RATE`] — which the audio output reads without
//! ever waiting: a late network gives silence, not a stuck game. With the
//! `bevy` feature, [`MusicPlugin`](audio::MusicPlugin) makes a feed an audio
//! source.
//!
//! Mechanisms, not policy: what is played, whether the directory is asked,
//! and what a host shares are the world's and its players' choices. The
//! engine never asks the network for anything the player did not choose.

mod decode;
mod icy;
mod player;
pub mod playlist;
pub mod radio;
pub mod share;

#[cfg(feature = "bevy")]
pub mod audio;

pub use decode::{Decoded, Tags};
pub use player::{Feed, Listening, NowPlaying, Origin, Player, Program, Status, Track};

/// The feed's sample rate, frames a second.
pub const RATE: u32 = 44_100;
/// The feed's channels: left and right, interleaved.
pub const CHANNELS: u16 = 2;
/// How osage introduces itself to the services it asks.
pub const USER_AGENT: &str = concat!("osage-music/", env!("CARGO_PKG_VERSION"));

/// The files music is read from, by extension.
pub const EXTENSIONS: [&str; 7] = ["ogg", "oga", "mp3", "flac", "wav", "m4a", "aac"];

/// Whether `path` names a file of music osage plays.
pub fn is_music(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|known| known.eq_ignore_ascii_case(e)))
}

/// An HTTP agent for music: a connection is given ten seconds, a stream
/// as long as it plays (but no read is waited on more than fifteen).
pub(crate) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(USER_AGENT)
        .timeout_connect(Some(std::time::Duration::from_secs(10)))
        .timeout_recv_response(Some(std::time::Duration::from_secs(10)))
        .timeout_per_read(Some(std::time::Duration::from_secs(15)))
        .build()
        .into()
}
