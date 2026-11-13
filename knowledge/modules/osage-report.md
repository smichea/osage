---
type: Module
title: osage-report
description: "Crash reports for osage programs: a run's log, panic and start watched in a folder; at the next start, a report of a panic, a start that never finished or renderer errors, cleaned of what identifies the player, sent where the program says once the player agrees — to a relay that files GitHub issues without duplicates."
resource: https://github.com/smichea/osage/tree/main/crates/osage-report
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [crash-reports, diagnostics, privacy, engine]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-13T04:00:00Z" }
---

# Purpose

Nobody should have to ask a player for their logs (D-073): a program finds, at its next start, what went wrong in the run before, and sends it — with the player's consent — to where its developers read it.

# Concepts

- **Watched folder:** `running` (a marker: `starting`, then `started`; removed by a clean exit), `last.log` (the run's log; the one before becomes `previous.log`), `crash.txt` (a panic), `hung.txt` (the watchdog's note).
- **Report:** a kind — panic, start (never finished starting), render (the renderer logged errors) — a message, a trace, the log's start and end, the system and the graphics adapter as the log names them; cleaned.
- **Relay:** a service that files reports as issues (`tools/crash-relay`), holding the only key.

# Architecture

`Watch::start(folder, patience)` reads the last run's files and gives its report if there is one, clears them, keeps `last.log` for `file_layer` (a `tracing_subscriber` layer writing the log, given to Bevy's `LogPlugin` as its custom layer), writes `running` = `starting`, installs a panic hook that writes the message and a backtrace before the usual hook, and starts a watchdog thread that writes `hung.txt` if `started` has not been called after `patience`. `started` rewrites the marker; `finish` removes it. A report is made from `crash.txt` first; else from a marker still `starting` (with `hung.txt`'s reason, or "closed before it had started"); else from ERROR lines of wgpu or rendering in the log; else none — a run started then killed is no crash. `clean` takes out the home folder (as `~`), the user's and the machine's names, IPv4 and IPv6 addresses, words with an `@`. `Report::send(endpoint, game, version, tier)` posts the JSON and returns the issue's address.

The relay (`tools/crash-relay/main.py`, Cloud Run, `osage-bench`): `signature` hashes the kind, the message normalised (lower case; hex, numbers, paths, quoted values out) and the first five frames of the trace that are not the runtime's; it searches the repository's issues for it, comments on the one it finds, or opens one titled `[crash] MESSAGE [SIGNATURE]` with the `crash-report` label and, if `DEEPSEEK_API_KEY` is set, DeepSeek's short reading at its head. Report text is made inert (`@` and code fences broken).

In the kart game (`osage-kart-native`, `report.rs`): the folder is `osage/kart-reports/`, the patience 120 s, the game started when the race has been shown a hundred frames; the question to the player (send, always, not now, never) is kept in `consent`.

# Interfaces

`Watch::{start, started, finish}`, `file_layer`, `Report::send`, `Report`, `Kind`, `clean`. The relay: `POST /report` (JSON: game, version, tier, kind, message, trace, log, os, gpu) → `{issue, new, signature}`; `GET /`.

# Invariants

- A clean exit or a run started then killed gives no report; a start never finished gives one with the machine and graphics read from the log; renderer errors give one; a panic wins. Tested.
- Addresses, e-mail-like names and the home folder are taken out of what is sent. Tested.
- The relay gives the same crash on two machines the same signature, and keeps text from reports from mentioning anyone or closing a fence. Tested (`tools/crash-relay/test_main.py`); checked live: a report opened an issue, the same again commented on it.

# Knowledge

- Decision D-073; used by [osage-kart-native](/modules/kart-native.md).

# Reuse

Any osage program: a world chooses its relay, or none, and whether to ask (mechanisms, not policy).

# Limits

A crash that kills the process before the hook runs (a driver taking the process down) shows as a start never finished, or not at all after the start. Reports are public, and read by DeepSeek when its key is set.
