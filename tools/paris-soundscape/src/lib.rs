// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The scene the command comes with: the Pont de Bir-Hakeim, as the JSON of a
//! glTF asset that `osage_sound::scene::Scene` reads.

use osage_sound::scene::Scene;

/// The Pont de Bir-Hakeim: 45 seconds on its upstream pavement.
pub const BIR_HAKEIM: &str = include_str!("../scenes/bir-hakeim.json");

/// The scene of the bridge, read.
pub fn bridge() -> Scene {
    Scene::from_json(BIR_HAKEIM).expect("the scene of the bridge reads")
}
