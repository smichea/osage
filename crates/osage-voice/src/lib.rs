// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Players talking to one another, as in a conference call: each player's
//! voice goes to a relay beside the world, which hands it to the others;
//! each player hears the others mixed, and can mute any of them.
//!
//! Speech is 16 kHz mono, in frames of 20 ms, coded in IMA ADPCM (four
//! bits a sample: 66 kbit/s a speaker — light for a local network, and
//! each frame stands alone, so a lost one is 20 ms of silence). Sent over
//! UDP ([`wire`]): a late frame is worth less than none.
//!
//! Mechanisms, not policy: the engine opens the microphone only when the
//! game asks ([`Microphone`](mic::Microphone), feature `device`), and sends
//! only the frames the game hands it ([`Voice::say`]) — push to talk, or
//! an open microphone gated by loudness, is the game's choice and its
//! player's. The relay keeps nothing.

pub mod adpcm;
mod client;
pub mod relay;
pub mod wire;

#[cfg(feature = "device")]
pub mod mic;

#[cfg(feature = "bevy")]
pub mod audio;

pub use client::{Feed, Heard, Voice};

/// Speech's sample rate.
pub const RATE: u32 = 16_000;
/// Samples a frame: 20 ms.
pub const FRAME: usize = 320;
/// Bytes a coded frame.
pub const FRAME_BYTES: usize = adpcm::frame_bytes(FRAME);
