// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `kart-update [--then LAUNCHER ARGS...]`: brings the kart's folder (the
//! one this program is in) up to the newest `kart-v…` release of osage on
//! GitHub, then runs `LAUNCHER` (play.bat, host.bat, join.bat) with `ARGS`.
//!
//! - The programs: when the release was built from another commit than the
//!   one in `release.txt`, its Windows zip is downloaded, checked against
//!   the digest GitHub publishes, and unpacked over the folder.
//! - The content (photographed textures, recorded sounds): when the
//!   folder has none, or the release's content zip is not the one installed.
//!   A folder installed by hand takes the published content as its own.
//! - Offline, or anything failing: it says so, and the game starts as it is.
//! - `OSAGE_NO_UPDATE=1`, or a file named `no-update` in the folder, turns
//!   it off.
//!
//! Each launcher runs it as its first line and ends there; it runs the
//! launcher again, updated, with `OSAGE_UPDATED=1` (a launcher being run
//! must not change under cmd's feet).

use std::{path::Path, process::Command};

use anyhow::{Context, Result};
use osage_update::{Record, api, copy_over, download, newest, tidy, unpack};

const REPOSITORY: &str = "smichea/osage";
const PREFIX: &str = "kart-v";
const PROGRAMS: &str = "osage-kart-windows-x64.zip";
const CONTENT: &str = "osage-kart-content.zip";
/// The folder inside both zips.
const ROOT: &str = "osage-kart";
const AGENT: &str = "osage-kart-update";

fn main() {
    let mut args = std::env::args().skip(1);
    let then = match args.next().as_deref() {
        Some("--then") => args.next(),
        _ => None,
    };
    let rest: Vec<String> = args.collect();
    let dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)).unwrap_or_default();
    tidy(&dir);
    let off = std::env::var_os("OSAGE_NO_UPDATE").is_some_and(|v| v != "0") || dir.join("no-update").exists();
    if !off && let Err(error) = update(&dir) {
        println!("  Update skipped: {error:#}");
        println!("  The game starts as it is.");
    }
    if let Some(launcher) = then {
        let status = Command::new("cmd")
            .arg("/c")
            .arg(&launcher)
            .args(&rest)
            .env("OSAGE_UPDATED", "1")
            .current_dir(&dir)
            .status();
        std::process::exit(status.ok().and_then(|s| s.code()).unwrap_or(1));
    }
}

fn update(dir: &Path) -> Result<()> {
    println!("  Looking for a newer version of osage kart...");
    let base = format!("https://api.github.com/repos/{REPOSITORY}");
    let releases = api(&format!("{base}/releases?per_page=30"), AGENT, "application/vnd.github+json")?;
    let Some(release) = newest(&releases, PREFIX)? else {
        println!("  No release published.");
        return Ok(());
    };
    let record_path = dir.join("release.txt");
    let mut record = Record::read(&record_path);
    let aside = dir.join("update");
    std::fs::create_dir_all(&aside)?;

    let commit = api(&format!("{base}/commits/{}", release.tag), AGENT, "application/vnd.github.sha")?;
    let commit = commit.trim();
    let programs = release.asset(PROGRAMS);
    match programs {
        Some(asset) if record.get("commit") != Some(commit) => {
            println!("  New version {} ({} MB): downloading...", release.tag, asset.size / 1_000_000);
            install(asset, &aside, dir)?;
            record.set("commit", commit);
            record.set("tag", &release.tag);
            record.write(&record_path)?;
            println!("  Updated to {}.", release.tag);
        }
        _ => println!("  The game is up to date ({}).", record.get("tag").unwrap_or(&release.tag)),
    }

    if let Some(asset) = release.asset(CONTENT) {
        let has_content = dir.join("assets").join("manifest.toml").exists();
        match record.get("content") {
            None if has_content => record.set("content", &asset.digest),
            Some(installed) if has_content && installed == asset.digest => {}
            _ => {
                println!("  New content (textures and sounds, {} MB): downloading...", asset.size / 1_000_000);
                install(asset, &aside, dir)?;
                record.set("content", &asset.digest);
                println!("  Content updated.");
            }
        }
        record.write(&record_path)?;
    }
    let _ = std::fs::remove_dir_all(&aside);
    Ok(())
}

/// Downloads `asset` into `aside`, checks it, and unpacks it over `dir`.
fn install(asset: &osage_update::Asset, aside: &Path, dir: &Path) -> Result<()> {
    let archive = aside.join(&asset.name);
    download(asset, &archive, AGENT)?;
    let unpacked = aside.join("unpacked");
    unpack(&archive, &unpacked)?;
    let _ = std::fs::remove_file(&archive);
    let files = copy_over(&unpacked.join(ROOT), dir).with_context(|| format!("installing {}", asset.name))?;
    let _ = std::fs::remove_dir_all(&unpacked);
    println!("  {files} files installed.");
    Ok(())
}
