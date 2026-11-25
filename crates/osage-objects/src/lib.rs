// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use serde::{Deserialize, Serialize};

#[cfg(feature = "profile")]
mod articulation;
#[cfg(feature = "profile")]
pub use articulation::*;
#[cfg(feature = "profile")]
mod foot_rig;
#[cfg(feature = "profile")]
mod profile;
#[cfg(feature = "profile")]
pub use foot_rig::ObjectFootRig;
#[cfg(feature = "profile")]
pub use profile::*;
#[cfg(feature = "profile")]
mod visual_detail;
#[cfg(feature = "profile")]
pub use visual_detail::*;

/// Shape of the allowed swing about the two transverse joint axes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SwingEnvelope {
    #[default]
    Independent,
    AngularEllipse,
}

impl SwingEnvelope {
    pub fn is_independent(&self) -> bool {
        *self == Self::Independent
    }

    pub fn accepts(self, limits: [[f32; 2]; 3]) -> bool {
        self.is_independent() || limits[1..].iter().all(|&[lo, hi]| hi > 0. && lo == -hi)
    }
}

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
