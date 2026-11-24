// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Sound placed in an object tree and carried to a listener's two ears.
//!
//! - [`tree`]: the nodes a sound is placed in, as the object format's;
//! - [`scene`]: what a scene gives sound — emitters, surfaces, touches, a
//!   listener — read from the JSON of a glTF asset;
//! - [`ways`]: the ways sound takes from an emitter to a head: straight,
//!   round an edge, mirrored by a face; what the air takes;
//! - [`ears`]: how a sound from a direction reaches each ear;
//! - [`engine`]: a scene heard block by block, at what a device can afford;
//! - [`render`]: a scene rendered to a file's samples, offline;
//! - [`live`]: a scene played as it goes, a block ahead of the output, its
//!   nodes placed by a game; with the `bevy` feature, a Bevy audio source;
//! - [`bench`]: what all this costs a processor.

pub mod bench;
pub mod ears;
pub mod engine;
pub mod live;
pub mod render;
pub mod scene;
pub mod space;
pub mod tree;
pub mod ways;
