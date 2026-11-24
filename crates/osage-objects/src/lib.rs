// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use serde::{Deserialize, Serialize};

#[cfg(feature = "profile")]
mod articulation;
#[cfg(feature = "profile")]
pub use articulation::*;
#[cfg(feature = "profile")]
mod profile;
#[cfg(feature = "profile")]
pub use profile::*;
#[cfg(feature = "profile")]
mod visual_detail;
#[cfg(feature = "profile")]
pub use visual_detail::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollisionMaterial {
    pub friction: f32,
    pub restitution: f32,
}

impl CollisionMaterial {
    pub fn valid(self) -> bool {
        self.friction.is_finite() && self.friction >= 0. && (0. ..=1.).contains(&self.restitution)
    }
}
