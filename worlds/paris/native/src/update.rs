// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `paris-update [--then LAUNCHER ARGS...]`: brings the Paris walk's
//! folder (the one this program is in) up to the newest `paris-v…`
//! release of osage on GitHub — the programs' zip of this system (Windows
//! x64, or a Mac with Apple silicon) and the content in two packages: the
//! street (Amazon Lumberyard's Bistro exported, which does not change) and
//! the light baked into its blocks (which does) — then runs `LAUNCHER`
//! (walk.bat on Windows, walk.command on a Mac) with `ARGS`. How:
//! `osage_update::folder`.

use osage_update::folder::{Folder, Package, run};

fn main() {
    run(&Folder {
        name: "osage paris",
        repository: "smichea/osage",
        prefix: "paris-v",
        programs: if cfg!(windows) {
            Some("osage-paris-windows-x64.zip")
        } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Some("osage-paris-macos-arm64.zip")
        } else {
            None
        },
        // The street never changes; its light does, a probe at a time.
        content: &[
            Package {
                name: "street",
                zip: "osage-paris-street.zip",
                record: "street",
                marker: "assets/orca/bistro/out/bistro.gltf",
            },
            Package {
                name: "light",
                zip: "osage-paris-light.zip",
                record: "light",
                marker: "assets/orca/bistro/out/tiles/index.json",
            },
        ],
        root: "osage-paris",
        agent: "osage-paris-update",
    });
}
