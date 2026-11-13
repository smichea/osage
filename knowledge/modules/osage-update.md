---
type: Module
title: osage-update
description: Keeps a program folder up to date from a project's GitHub releases — the newest release of a tag prefix, downloads checked against GitHub's digests, files replaced in place; with kart-update, the kart's Windows launcher step.
resource: https://github.com/smichea/osage/tree/main/crates/osage-update
licence: MIT OR Apache-2.0
status: draft
tags: [release, update, windows, github]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T08:00:00Z" }
---

# Purpose

Let people who play from a downloaded zip (Windows, no package manager) get each new version without unzipping it again by hand (D-047).

# Concepts

- **Release:** the newest published GitHub release whose tag starts with a prefix (`kart-v`); pre-releases count, drafts do not.
- **Record:** `release.txt` in the folder, `key = value` lines: the commit the programs were built from (written by the release workflow), the tag, and the digest of each content package installed.
- **Asset digest:** GitHub publishes `sha256:<hex>` for each release asset; a download that does not match is deleted.

# Architecture

A library of small steps — `newest`, `api`, `download`, `unpack`, `copy_over`, `tidy`, `Record`, `digest` — and no policy. Downloads use the system's `curl`, unpacking Windows' `tar` (bsdtar; `bsdtar` or `unzip` elsewhere): Windows 10 has both since 2018, so the crate carries no HTTP, TLS or archive code.

`kart-update` (in `worlds/kart/server`) is the kart's policy: repository `smichea/osage`, prefix `kart-v`, the Windows zip when the release's commit (`GET /commits/{tag}`) differs from the record's, the content zip when the folder has none or the published digest changed (a folder installed by hand adopts the published digest) — only a patch of the files that changed when the release has one from the content installed (`Release::patch`: `osage-kart-content-from-<first 16 hex digits of the installed digest>.zip`, D-083), else the whole zip. Offline or failing, it says so and the game starts as it is. `OSAGE_NO_UPDATE=1` or a file `no-update` turns it off.

# Interfaces

- Library: `newest(json, prefix) -> Option<Release>`, `api(url, agent, accept)`, `download(&Asset, to, agent)`, `unpack(zip, aside)`, `copy_over(from, into) -> files`, `tidy(dir)`, `old_name`, `Record::{read, parse, get, set, write}`, `digest(path)`.
- Launchers: each `.bat` runs, before anything else, `if not "%OSAGE_UPDATED%"=="1" if exist kart-update.exe (kart-update.exe --then "%~f0" %* & exit /b)`. cmd reads the whole block before running it, so the launcher may change on disk while kart-update runs; kart-update then runs it again, updated, with `OSAGE_UPDATED=1`.

# Invariants

- Nothing is installed that does not match its published digest.
- Files are replaced or added, never deleted: screenshots and the player's files stay.
- A running program is renamed `NAME.old.EXT` (Windows lets a running program be renamed, not overwritten) and removed at the next run.

# Knowledge

- The release workflow (`.github/workflows/release-windows.yml`) builds `kart-update.exe` and writes `release.txt` into the zip.

# Reuse

Any game or tool shipped as a zip on GitHub releases: pick a prefix and packages, call the steps.

# Limits

Unsigned programs: trust rests on GitHub's HTTPS and digests, not on a signature of the maintainer's. Unauthenticated GitHub API calls are limited to 60 an hour per address. Tested on Linux with a local zip and the real release list; not yet run on Windows.
