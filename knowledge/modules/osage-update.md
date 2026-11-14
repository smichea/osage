---
type: Module
title: osage-update
description: Keeps a program folder up to date from a project's GitHub releases — the newest release of a tag prefix, downloads checked against GitHub's digests, files replaced in place; with kart-update, the first step of the kart's launchers on Windows and macOS.
resource: https://github.com/smichea/osage/tree/main/crates/osage-update
licence: MIT OR Apache-2.0
status: draft
tags: [release, update, windows, macos, github]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-14T13:00:00Z" }
---

# Purpose

Let people who play from a downloaded zip (Windows or macOS, no package manager) get each new version without unzipping it again by hand (D-047, D-113).

# Concepts

- **Release:** the newest published GitHub release whose tag starts with a prefix (`kart-v`); pre-releases count, drafts do not.
- **Record:** `release.txt` in the folder, `key = value` lines: the commit the programs were built from (written by the release workflow), the tag, and the digest of each content package installed.
- **Asset digest:** GitHub publishes `sha256:<hex>` for each release asset; a download that does not match is deleted.

# Architecture

A library of small steps — `newest`, `api`, `download`, `unpack`, `copy_over`, `tidy`, `Record`, `digest` — and no policy. Downloads use the system's `curl`, unpacking Windows' `tar` (bsdtar; `bsdtar` or `unzip` elsewhere): Windows 10 has both since 2018 and macOS has `curl` and `bsdtar`, so the crate carries no HTTP, TLS or archive code.

`kart-update` (in `worlds/kart/server`) is the kart's policy: repository `smichea/osage`, prefix `kart-v`, the programs' zip of its own system (`osage-kart-windows-x64.zip`, or `osage-kart-macos-arm64.zip` on a Mac with Apple silicon; none elsewhere, where the programs are built from source) when the release's commit (`GET /commits/{tag}`) differs from the record's, the content zip when the folder has none or the published digest changed (a folder installed by hand adopts the published digest) — only a patch of the files that changed when the release has one from the content installed (`Release::patch`: `osage-kart-content-from-<first 16 hex digits of the installed digest>.zip`, D-083), else the whole zip. Offline or failing, it says so and the game starts as it is. `OSAGE_NO_UPDATE=1` or a file `no-update` turns it off.

# Interfaces

- Library: `newest(json, prefix) -> Option<Release>`, `api(url, agent, accept)`, `download(&Asset, to, agent)`, `unpack(zip, aside)`, `copy_over(from, into) -> files`, `tidy(dir)`, `old_name`, `Record::{read, parse, get, set, write}`, `digest(path)`.
- Launchers: each `.bat` runs, before anything else, `if not "%OSAGE_UPDATED%"=="1" if exist kart-update.exe (kart-update.exe --then "%~f0" %* & exit /b)`. cmd reads the whole block before running it, so the launcher may change on disk while kart-update runs; kart-update then runs it again, updated, with `OSAGE_UPDATED=1`. On a Mac each `.command` runs `exec ./kart-update --then "$PWD/$(basename "$0")" "$@"` unless `OSAGE_UPDATED` is 1: `sh` reads a script as it goes, so it gives its place to kart-update, which gives its own to `/bin/sh` running the launcher again (`exec` too), so that Ctrl+C and closing the Terminal window reach the launcher, which stops the world it started.

# Invariants

- Nothing is installed that does not match its published digest.
- Files are replaced or added, never deleted: screenshots and the player's files stay.
- A running program is renamed `NAME.old.EXT` (Windows lets a running program be renamed, not overwritten) and removed at the next run. macOS lets it be overwritten in place: the running program keeps the code it started with, and the next start runs the new file (seen on macOS 26.3 with programs signed ad hoc, as the linker signs them).

# Knowledge

- The release workflows (`.github/workflows/release-windows.yml`, `.github/workflows/release-macos.yml`) build `kart-update` and write `release.txt` into each zip.

# Reuse

Any game or tool shipped as a zip on GitHub releases: pick a prefix and packages, call the steps.

# Limits

Unsigned programs: trust rests on GitHub's HTTPS and digests, not on a signature of the maintainer's. Unauthenticated GitHub API calls are limited to 60 an hour per address. Tested on Linux with a local zip and the real release list; not yet run on Windows. On macOS 26.3 (Apple M3), the folder the macOS workflow packs was run from its zip against the real release list: the quarantine mark lifted, the launcher run again through `exec` with its arguments intact, the world stopped when the game ends or the window closes; an update of the programs from a macOS zip is not yet run, as no release has one.
