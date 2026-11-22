// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `kart-update [--then LAUNCHER ARGS...]`: brings the kart's folder (the
//! one this program is in) up to the newest `kart-v…` release of osage on
//! GitHub — the programs' zip of this system (Windows x64, or a Mac with
//! Apple silicon) and the content (photographed textures, recorded
//! sounds) — then runs `LAUNCHER` (play.bat, host.bat, join.bat on
//! Windows; play.command, host.command, join.command on a Mac) with
//! `ARGS`. How: `osage_update::folder`.

use osage_update::folder::{Folder, run};

fn main() {
    run(&Folder {
        name: "osage kart",
        repository: "smichea/osage",
        prefix: "kart-v",
        programs: if cfg!(windows) {
            Some("osage-kart-windows-x64.zip")
        } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Some("osage-kart-macos-arm64.zip")
        } else {
            None
        },
        content: "osage-kart-content.zip",
        root: "osage-kart",
        agent: "osage-kart-update",
    });
}
