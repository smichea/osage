// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! `osage-server`: the demo world. Options: see [`osage_server::cli`].

fn main() -> anyhow::Result<()> {
    osage_server::cli::run(osage_server::demo::DemoWorld, osage_server::Config::default())
}
