// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Keeps a program folder up to date from a project's GitHub releases.
//!
//! - The newest release whose tag starts with a prefix (`kart-v`) is the
//!   one to have; pre-releases count, drafts do not.
//! - A folder knows what it holds from a small record (`release.txt`,
//!   `key = value` lines): the commit its programs were built from, and the
//!   digest of each package of content it installed.
//! - A download is kept only if its SHA-256 is the one GitHub gives for the
//!   asset; it is unpacked aside, then copied over the folder. Files are
//!   replaced, never deleted; a running program, which Windows will not let
//!   be overwritten, is first renamed `NAME.old.EXT` and removed next time.
//!
//! Downloads and unpacking use the system's own `curl` and `tar` (both on
//! Windows 10 since 2018; `bsdtar` or `unzip` on Linux and macOS): no HTTP or archive code
//! here. Mechanisms only: which repository, prefix and packages, and
//! whether to ask first, are the caller's.

use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// A release asset, as GitHub describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    /// `sha256:<hex>`.
    pub digest: String,
    pub size: u64,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub tag: String,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

/// The newest release (by publication) whose tag starts with `prefix`,
/// from GitHub's list of releases (`GET /repos/{owner}/{name}/releases`).
pub fn newest(releases_json: &str, prefix: &str) -> Result<Option<Release>> {
    let releases: serde_json::Value = serde_json::from_str(releases_json).context("GitHub's list of releases")?;
    let mut best: Option<(String, Release)> = None;
    for release in releases.as_array().context("a list of releases")? {
        let text = |key: &str| release[key].as_str().unwrap_or("").to_owned();
        let tag = text("tag_name");
        if !tag.starts_with(prefix) || release["draft"].as_bool().unwrap_or(false) {
            continue;
        }
        let published = text("published_at");
        if best.as_ref().is_some_and(|(newest, _)| *newest >= published) {
            continue;
        }
        let assets = release["assets"]
            .as_array()
            .map(|assets| {
                assets
                    .iter()
                    .map(|a| Asset {
                        name: a["name"].as_str().unwrap_or("").to_owned(),
                        digest: a["digest"].as_str().unwrap_or("").to_owned(),
                        size: a["size"].as_u64().unwrap_or(0),
                        url: a["browser_download_url"].as_str().unwrap_or("").to_owned(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        best = Some((published, Release { tag, assets }));
    }
    Ok(best.map(|(_, release)| release))
}

/// What a folder holds: `key = value` lines; `#` begins a comment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record(pub BTreeMap<String, String>);

impl Record {
    pub fn parse(text: &str) -> Self {
        Self(
            text.lines()
                .filter(|line| !line.trim_start().starts_with('#'))
                .filter_map(|line| line.split_once('='))
                .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
                .collect(),
        )
    }

    pub fn read(path: &Path) -> Self {
        fs::read_to_string(path).map(|text| Self::parse(&text)).unwrap_or_default()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.0.insert(key.to_owned(), value.to_owned());
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        let mut text = String::from("# What this folder holds, for its updater.\n");
        for (k, v) in &self.0 {
            text.push_str(&format!("{k} = {v}\n"));
        }
        fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }
}

/// `sha256:<hex>` of a file.
pub fn digest(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("reading {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let hex: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("sha256:{hex}"))
}

/// `curl`, failing on HTTP errors, following redirects, as `agent`.
fn curl(agent: &str) -> Command {
    let mut command = Command::new("curl");
    command.args(["--fail", "--location", "--silent", "--show-error", "--user-agent", agent]);
    command
}

/// The body of a GitHub API `url`, as text.
pub fn api(url: &str, agent: &str, accept: &str) -> Result<String> {
    let output = curl(agent)
        .args(["--max-time", "20", "--header", &format!("Accept: {accept}"), url])
        .output()
        .context("running curl")?;
    if !output.status.success() {
        bail!("{url}: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    String::from_utf8(output.stdout).context("not text")
}

/// Downloads `asset` to `to`, showing progress, and keeps it only if its
/// digest is the one GitHub gives.
pub fn download(asset: &Asset, to: &Path, agent: &str) -> Result<()> {
    if !asset.digest.starts_with("sha256:") {
        bail!("{} has no digest to check it against", asset.name);
    }
    let status =
        curl(agent).arg("--progress-bar").arg("--output").arg(to).arg(&asset.url).status().context("running curl")?;
    if !status.success() {
        bail!("downloading {} failed", asset.name);
    }
    let got = digest(to)?;
    if got != asset.digest {
        let _ = fs::remove_file(to);
        bail!("{} is not what was published ({got}, not {})", asset.name, asset.digest);
    }
    Ok(())
}

/// Unpacks the zip `archive` into `aside` (emptied first) with the
/// system's tools: Windows' `tar` (bsdtar) reads zips; elsewhere `bsdtar`
/// or `unzip`, since GNU tar does not.
pub fn unpack(archive: &Path, aside: &Path) -> Result<()> {
    let _ = fs::remove_dir_all(aside);
    fs::create_dir_all(aside)?;
    let tools: [(&str, &[&str]); 3] = [("bsdtar", &["-xf"]), ("tar", &["-xf"]), ("unzip", &["-q", "-o"])];
    for (tool, flags) in tools {
        if tool == "tar" && !cfg!(windows) {
            continue;
        }
        let mut command = Command::new(tool);
        command.args(flags).arg(archive);
        if tool == "unzip" {
            command.arg("-d").arg(aside);
        } else {
            command.arg("-C").arg(aside);
        }
        match command.status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(_) => bail!("unpacking {} failed ({tool})", archive.display()),
            // Not on this system: the next one.
            Err(_) => {}
        }
    }
    bail!("no tool to unpack {} (tar, bsdtar or unzip)", archive.display())
}

/// The name a file is moved to when it cannot be overwritten: `NAME.old.EXT`.
pub fn old_name(path: &Path) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    match path.extension() {
        Some(ext) => path.with_file_name(format!("{stem}.old.{}", ext.to_string_lossy())),
        None => path.with_file_name(format!("{stem}.old")),
    }
}

/// Copies the tree `from` over `into`: files replaced or added, none
/// deleted. A file that cannot be replaced (a running program) is renamed
/// aside first. Returns how many files were copied.
pub fn copy_over(from: &Path, into: &Path) -> Result<usize> {
    let mut copied = 0;
    fs::create_dir_all(into)?;
    for entry in fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let (source, target) = (entry.path(), into.join(entry.file_name()));
        if entry.file_type()?.is_dir() {
            copied += copy_over(&source, &target)?;
            continue;
        }
        if fs::copy(&source, &target).is_err() {
            let old = old_name(&target);
            let _ = fs::remove_file(&old);
            fs::rename(&target, &old).with_context(|| format!("moving {} aside", target.display()))?;
            fs::copy(&source, &target).with_context(|| format!("writing {}", target.display()))?;
        }
        copied += 1;
    }
    Ok(copied)
}

/// Removes the `NAME.old.EXT` files a previous update left in `dir`.
pub fn tidy(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.contains(".old.") || name.ends_with(".old") {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASES: &str = r#"[
      {"tag_name": "kart-v0.2.0", "draft": true, "published_at": "2026-12-01T00:00:00Z", "assets": []},
      {"tag_name": "kart-v0.1.1", "draft": false, "prerelease": true, "published_at": "2026-11-20T00:00:00Z",
       "assets": [{"name": "game.zip", "digest": "sha256:ab", "size": 10, "browser_download_url": "https://x/game.zip"}]},
      {"tag_name": "kart-v0.1.0", "draft": false, "published_at": "2026-11-11T00:00:00Z", "assets": []},
      {"tag_name": "engine-v9", "draft": false, "published_at": "2026-12-02T00:00:00Z", "assets": []}
    ]"#;

    #[test]
    fn the_newest_published_release_of_the_prefix_is_chosen() {
        let release = newest(RELEASES, "kart-v").unwrap().unwrap();
        assert_eq!(release.tag, "kart-v0.1.1");
        let asset = release.asset("game.zip").unwrap();
        assert_eq!((asset.digest.as_str(), asset.size, asset.url.as_str()), ("sha256:ab", 10, "https://x/game.zip"));
        assert_eq!(newest(RELEASES, "nothing-").unwrap(), None);
    }

    #[test]
    fn a_record_reads_back_what_it_wrote() {
        let dir = std::env::temp_dir().join(format!("osage-update-record-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut record = Record::parse("# comment\ncommit = abc\n");
        record.set("content", "sha256:01");
        record.write(&dir.join("release.txt")).unwrap();
        let again = Record::read(&dir.join("release.txt"));
        assert_eq!((again.get("commit"), again.get("content")), (Some("abc"), Some("sha256:01")));
        assert_eq!(Record::read(&dir.join("missing.txt")), Record::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn files_are_copied_over_and_none_deleted() {
        let root = std::env::temp_dir().join(format!("osage-update-copy-{}", std::process::id()));
        let (from, into) = (root.join("new"), root.join("folder"));
        fs::create_dir_all(from.join("web")).unwrap();
        fs::create_dir_all(&into).unwrap();
        fs::write(from.join("game.exe"), "2").unwrap();
        fs::write(from.join("web/index.html"), "page").unwrap();
        fs::write(into.join("game.exe"), "1").unwrap();
        fs::write(into.join("screenshot.png"), "mine").unwrap();
        assert_eq!(copy_over(&from, &into).unwrap(), 2);
        assert_eq!(fs::read_to_string(into.join("game.exe")).unwrap(), "2");
        assert_eq!(fs::read_to_string(into.join("web/index.html")).unwrap(), "page");
        assert_eq!(fs::read_to_string(into.join("screenshot.png")).unwrap(), "mine");
        assert_eq!(digest(&into.join("game.exe")).unwrap().len(), 7 + 64);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn old_names_keep_the_extension_and_are_tidied() {
        assert_eq!(old_name(Path::new("a/kart-update.exe")), PathBuf::from("a/kart-update.old.exe"));
        let dir = std::env::temp_dir().join(format!("osage-update-tidy-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("kart-update.old.exe"), "").unwrap();
        fs::write(dir.join("kart-update.exe"), "").unwrap();
        tidy(&dir);
        assert!(!dir.join("kart-update.old.exe").exists() && dir.join("kart-update.exe").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
