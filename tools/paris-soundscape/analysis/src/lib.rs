// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! What a stereo recording tells of where its sounds are, measured the same
//! way whatever made it — this prototype, another engine, a microphone in
//! each ear: so that two renders of a like scene can be compared by numbers
//! before they are compared by ear.
//!
//! [`measure`] gathers everything into a [`Report`]; the modules hold the
//! measures one by one, for a test that asserts a single thing:
//!
//! - [`level`]: peaks at the samples and between them, mean level, offset,
//!   samples at the limits of a 16-bit file;
//! - [`loudness`](mod@loudness): integrated loudness by ITU-R BS.1770-4;
//! - [`bands`]: levels by octave and third of octave, over a file and over
//!   time; the centre of the spectrum;
//! - [`cues`](mod@cues): what the two ears compare — level difference, time
//!   difference, coherence — by band and over time;
//! - [`pitch`]: a tone followed over time, and the shift of one that passes;
//! - [`envelope`](mod@envelope): a level over time, its peak, a passage;
//! - [`fft`], [`resample`](mod@resample): what the others stand on.
//!
//! # Conventions
//!
//! - Signals are slices of `f32` samples, −1…1, with their rate in samples
//!   a second; measures are computed and given in `f64`.
//! - **Levels** are decibels relative to full scale, of mean squares: a sine
//!   of full scale reads −3.01 dB. Silence reads [`level::SILENCE_DB`]
//!   (−180), never minus infinity.
//! - **Every sample of a file counts alike** in a level over the whole of
//!   it, the last as the first: a burst at the very end is in the spectrum
//!   as one in the middle is. So are the two cuts of a file that starts or
//!   ends in the middle of a sound — a click each, 40 dB or more under the
//!   sound, spread over the bands — as a bank of filters would read them.
//! - **A loudness counts the channels a file has**: one for a mono file,
//!   which reads 3.01 LU under the same signal in both channels of a stereo
//!   one.
//! - **Differences between the channels are right minus left.** A level
//!   difference is positive when the RIGHT channel is louder; a time
//!   difference is positive when the sound reaches the RIGHT ear first (the
//!   left channel lags). A source on the right reads positive on both.
//! - **Times** are seconds from the first sample. A value given "every step"
//!   speaks of the stretch of signal centred on its time.
//! - Nothing here is random and nothing reads a clock: the same samples give
//!   the same figures.

pub mod bands;
pub mod cues;
pub mod envelope;
pub mod fft;
pub mod hash;
pub mod level;
pub mod loudness;
pub mod pitch;
pub mod report;
pub mod resample;
pub mod wav;

#[cfg(test)]
mod testing;

pub use bands::{Band, BandLevels, band_levels, band_levels_over_time, octaves, spectral_centroid, third_octaves};
pub use cues::{Cue, TimeDifference, cue, cue_bands, cues, gcc_phat, gcc_phat_between};
pub use envelope::{Envelope, Passage, band_envelope, envelope};
pub use hash::sha256;
pub use level::{Clipping, clipping, correlation, db, dc, rms, sample_peak, true_peak};
pub use loudness::loudness;
pub use pitch::{Shift, Tone, tone_shift, tone_track};
pub use report::{Report, measure};
pub use resample::resample;
pub use wav::Audio;
