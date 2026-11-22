// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The policy the worlds' updaters share (`kart-update`, `paris-update`):
//! `NAME-update [--then LAUNCHER ARGS...]` brings the folder the program
//! is in up to the newest release of its tag prefix, then runs `LAUNCHER`
//! (a `.bat` on Windows, a `.command` on a Mac) with `ARGS`.
//!
//! - The programs: when the release was built from another commit than the
//!   one in `release.txt`, its zip for this system is downloaded, checked
//!   against the digest GitHub publishes, and unpacked over the folder.
//!   Where the releases have no zip for the system, the programs are built
//!   from source, and only the content is kept up to date.
//! - The content: when the folder has none, or the release's content zip
//!   is not the one installed: only the files that changed when the
//!   release has a patch from the content installed
//!   (`<content>-from-<digest>.zip`), else all of it. A folder installed
//!   by hand takes the published content as its own.
//! - Offline, or anything failing: it says so, and the program starts as
//!   it is.
//! - `OSAGE_NO_UPDATE=1`, or a file named `no-update` in the folder, turns
//!   it off.
//!
//! Each launcher runs it as its first line and ends there; it runs the
//! launcher again, updated, with `OSAGE_UPDATED=1` (a launcher being run
//! must not change under the shell's feet: cmd reads the whole line before
//! running it; on a Mac the launcher hands over to the updater with
//! `exec`, and the updater to the launcher again, so that no shell reads
//! on in a file that has changed).

use std::{path::Path, process::Command};

use anyhow::{Context, Result};

use crate::{Asset, Record, api, copy_over, download, newest, tidy, unpack};

/// What a world's folder is kept up to date from.
pub struct Folder {
    /// What the messages call it: "osage kart".
    pub name: &'static str,
    /// The GitHub repository: "smichea/osage".
    pub repository: &'static str,
    /// The start of its releases' tags: "kart-v".
    pub prefix: &'static str,
    /// The programs' zip for this system, if the releases have one.
    pub programs: Option<&'static str>,
    /// The content's zip.
    pub content: &'static str,
    /// The folder inside both zips.
    pub root: &'static str,
    /// What the requests to GitHub say they come from.
    pub agent: &'static str,
}

/// `name` with its first letter a capital, to start a sentence.
fn capital(name: &str) -> String {
    let mut letters = name.chars();
    letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default()
}

/// Runs the updater: `[--then LAUNCHER ARGS...]`, for the folder this
/// program is in.
pub fn run(folder: &Folder) {
    let mut args = std::env::args().skip(1);
    let then = match args.next().as_deref() {
        Some("--then") => args.next(),
        _ => None,
    };
    let rest: Vec<String> = args.collect();
    let dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)).unwrap_or_default();
    tidy(&dir);
    let off = std::env::var_os("OSAGE_NO_UPDATE").is_some_and(|v| v != "0") || dir.join("no-update").exists();
    if !off && let Err(error) = update(folder, &dir) {
        println!("  Update skipped: {error:#}");
        println!("  {} starts as it is.", capital(folder.name));
    }
    if let Some(launcher) = then {
        run_again(&launcher, &rest, &dir);
    }
}

/// Runs the launcher again, updated, with `OSAGE_UPDATED=1`, and ends
/// with it.
#[cfg(windows)]
fn run_again(launcher: &str, args: &[String], dir: &Path) -> ! {
    let status =
        Command::new("cmd").arg("/c").arg(launcher).args(args).env("OSAGE_UPDATED", "1").current_dir(dir).status();
    std::process::exit(status.ok().and_then(|s| s.code()).unwrap_or(1));
}

/// Runs the launcher again, updated, with `OSAGE_UPDATED=1`: `sh` takes
/// this program's place, so that Ctrl+C and closing the Terminal's window
/// reach the launcher, which stops the world it started.
#[cfg(unix)]
fn run_again(launcher: &str, args: &[String], dir: &Path) -> ! {
    use std::os::unix::process::CommandExt;
    let error = Command::new("/bin/sh").arg(launcher).args(args).env("OSAGE_UPDATED", "1").current_dir(dir).exec();
    println!("  Could not run {launcher}: {error}");
    std::process::exit(1);
}

fn update(folder: &Folder, dir: &Path) -> Result<()> {
    let agent = folder.agent;
    println!("  Looking for a newer version of {}...", folder.name);
    let base = format!("https://api.github.com/repos/{}", folder.repository);
    let releases = api(&format!("{base}/releases?per_page=30"), agent, "application/vnd.github+json")?;
    let Some(release) = newest(&releases, folder.prefix)? else {
        println!("  No release published.");
        return Ok(());
    };
    let record_path = dir.join("release.txt");
    let mut record = Record::read(&record_path);
    let aside = dir.join("update");
    std::fs::create_dir_all(&aside)?;

    let commit = api(&format!("{base}/commits/{}", release.tag), agent, "application/vnd.github.sha")?;
    let commit = commit.trim();
    let programs = folder.programs.and_then(|name| release.asset(name));
    match programs {
        Some(asset) if record.get("commit") != Some(commit) => {
            println!("  New version {} ({} MB): downloading...", release.tag, asset.size / 1_000_000);
            install(folder, asset, &aside, dir)?;
            record.set("commit", commit);
            record.set("tag", &release.tag);
            record.write(&record_path)?;
            println!("  Updated to {}.", release.tag);
        }
        _ => println!("  {} is up to date ({}).", capital(folder.name), record.get("tag").unwrap_or(&release.tag)),
    }

    if let Some(asset) = release.asset(folder.content) {
        let has_content = dir.join("assets").join("manifest.toml").exists();
        match record.get("content") {
            None if has_content => record.set("content", &asset.digest),
            Some(installed) if has_content && installed == asset.digest => {}
            installed => {
                // Only what changed, when the release has a patch from the
                // content installed; else all of it.
                match installed.filter(|_| has_content).and_then(|from| release.patch(folder.content, from)) {
                    Some(patch) => {
                        println!("  New content ({} MB of changes): downloading...", (patch.size / 1_000_000).max(1));
                        install(folder, patch, &aside, dir)?;
                    }
                    None => {
                        println!("  New content ({} MB): downloading...", asset.size / 1_000_000);
                        install(folder, asset, &aside, dir)?;
                    }
                }
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
fn install(folder: &Folder, asset: &Asset, aside: &Path, dir: &Path) -> Result<()> {
    let archive = aside.join(&asset.name);
    download(asset, &archive, folder.agent)?;
    let unpacked = aside.join("unpacked");
    unpack(&archive, &unpacked)?;
    let _ = std::fs::remove_file(&archive);
    let files = copy_over(&unpacked.join(folder.root), dir).with_context(|| format!("installing {}", asset.name))?;
    let _ = std::fs::remove_dir_all(&unpacked);
    println!("  {files} files installed.");
    Ok(())
}
