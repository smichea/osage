// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Crash reports (D-073): what went wrong in a program's last run, found
//! at its next start, so that nobody has to ask a player for their logs.
//!
//! [`Watch::start`] keeps, in a folder of the program's:
//!
//! - `running`, a marker written at the start — `starting`, then `started`
//!   once the program says it is ([`Watch::started`]) — and removed by a
//!   clean exit ([`Watch::finish`]);
//! - `last.log`, the run's log, through [`file_layer`];
//! - `crash.txt`, a panic's message and trace, written by its hook;
//! - and a watchdog that writes `hung.txt` if the program has not started
//!   a while after it began.
//!
//! At the next start, [`Watch::start`] gives the [`Report`] of the run
//! before, if there is one to make: a panic; a start that never finished
//! (the marker still says `starting`: the program hung, or was closed
//! before it showed anything); or errors of the renderer in the log. A run
//! that started and was then killed is not reported: closing a window is
//! no crash. The report is cleaned on the player's computer ([`clean`]) —
//! the home folder, the user's name, network addresses, e-mail-like names —
//! and sent ([`Report::send`]) only where and when the program decides: the
//! program asks the player first (mechanisms, not policy).

use std::{
    io::Write as _,
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// Lines of the last log a report carries: the start (the machine, the
/// graphics) and the end (what went wrong).
const HEAD: usize = 60;
const TAIL: usize = 400;
/// Lines of renderer errors that make a report on their own.
const ERRORS: usize = 1;

static LOG: OnceLock<PathBuf> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);

/// A tracing layer that writes the log to the watched folder's `last.log`,
/// for a `tracing` subscriber built on a `Registry` (Bevy's `LogPlugin`
/// takes it as its custom layer). None before [`Watch::start`].
pub fn file_layer() -> Option<Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync>> {
    let path = LOG.get()?;
    let file = std::fs::File::create(path).ok()?;
    Some(Box::new(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(std::sync::Mutex::new(file))))
}

/// What went wrong in the last run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// It panicked.
    Panic,
    /// It never finished starting: hung, or closed before it showed anything.
    Start,
    /// It ran, but the renderer reported errors.
    Render,
}

impl Kind {
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Panic => "panic",
            Kind::Start => "start",
            Kind::Render => "render",
        }
    }
}

/// A report of the last run, cleaned.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub kind: Kind,
    pub message: String,
    pub trace: String,
    pub log: String,
    pub os: String,
    pub gpu: String,
}

impl Report {
    /// Sends the report to `endpoint` (a relay that files it, D-073) as
    /// JSON with `game`, `version` and `tier`; the issue's address back.
    pub fn send(&self, endpoint: &str, game: &str, version: &str, tier: &str) -> Result<String, String> {
        let body = serde_json::json!({
            "game": game,
            "version": version,
            "tier": tier,
            "kind": self.kind.name(),
            "message": self.message,
            "trace": self.trace,
            "log": self.log,
            "os": self.os,
            "gpu": self.gpu,
        });
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .user_agent("osage-report")
            .build()
            .new_agent();
        let mut response = agent
            .post(endpoint)
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .map_err(|e| e.to_string())?;
        let text = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
        let answer: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        Ok(answer["issue"].as_str().unwrap_or("").to_owned())
    }
}

/// The program's watch over its runs.
pub struct Watch {
    folder: PathBuf,
}

impl Watch {
    /// Starts watching this run in `folder`, and gives the report of the
    /// run before, if one is to be made. The watchdog says a start has not
    /// finished after `patience`.
    pub fn start(folder: &Path, patience: Duration) -> (Self, Option<Report>) {
        let _ = std::fs::create_dir_all(folder);
        let report = Self::last(folder);
        for name in ["crash.txt", "hung.txt"] {
            let _ = std::fs::remove_file(folder.join(name));
        }
        let log = folder.join("last.log");
        // The last run's log, kept beside it once read.
        let _ = std::fs::rename(&log, folder.join("previous.log"));
        let _ = LOG.set(log);
        let _ = std::fs::write(folder.join("running"), "starting\n");
        let crash = folder.join("crash.txt");
        let before = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let trace = std::backtrace::Backtrace::force_capture();
            if let Ok(mut file) = std::fs::File::create(&crash) {
                let _ = writeln!(file, "{info}\n--- trace ---\n{trace}");
            }
            before(info);
        }));
        let hung = folder.join("hung.txt");
        let _ = std::thread::Builder::new().name("osage-report-watchdog".into()).spawn(move || {
            std::thread::sleep(patience);
            if !STARTED.load(Ordering::Relaxed) {
                let _ = std::fs::write(&hung, format!("not started after {} s\n", patience.as_secs()));
            }
        });
        (Self { folder: folder.to_owned() }, report)
    }

    /// The program is up: what it shows is shown.
    pub fn started(&self) {
        if !STARTED.swap(true, Ordering::Relaxed) {
            let _ = std::fs::write(self.folder.join("running"), "started\n");
        }
    }

    /// A clean exit: nothing to report next time.
    pub fn finish(&self) {
        let _ = std::fs::remove_file(self.folder.join("running"));
    }

    fn last(folder: &Path) -> Option<Report> {
        let read = |name: &str| std::fs::read_to_string(folder.join(name)).ok();
        let log = read("last.log").unwrap_or_default();
        let marker = read("running");
        let (kind, message, trace) = if let Some(crash) = read("crash.txt") {
            let (message, trace) = crash.split_once("--- trace ---").unwrap_or((&crash, ""));
            (Kind::Panic, message.trim().to_owned(), trace.trim().to_owned())
        } else if marker.as_deref().is_some_and(|m| m.trim() == "starting") {
            let why = read("hung.txt").unwrap_or_else(|| "closed before it had started".to_owned());
            (Kind::Start, format!("the program never finished starting: {}", why.trim()), String::new())
        } else {
            let errors = render_errors(&log);
            if errors.len() < ERRORS {
                return None;
            }
            (Kind::Render, errors[0].clone(), errors.join("\n"))
        };
        let lines: Vec<&str> = log.lines().collect();
        let excerpt = if lines.len() > HEAD + TAIL {
            format!("{}\n[…]\n{}", lines[..HEAD].join("\n"), lines[lines.len() - TAIL..].join("\n"))
        } else {
            log.clone()
        };
        Some(Report {
            kind,
            message: clean(&message),
            trace: clean(&trace),
            log: clean(&excerpt),
            os: clean(&found(&log, r#"os: ""#).unwrap_or_default()),
            gpu: found(&log, r#"AdapterInfo { name: ""#).unwrap_or_default(),
        })
    }
}

/// The renderer's errors in a log: lines at ERROR level from wgpu or Bevy's rendering.
fn render_errors(log: &str) -> Vec<String> {
    log.lines()
        .filter(|line| line.contains("ERROR") && (line.contains("wgpu") || line.contains("render")))
        .map(|line| line.trim().to_owned())
        .take(50)
        .collect()
}

/// The quoted value after `key` in a log line, the first time it appears.
fn found(log: &str, key: &str) -> Option<String> {
    let at = log.find(key)? + key.len();
    let rest = &log[at..];
    Some(rest[..rest.find('"')?].to_owned())
}

/// A text with what identifies the player taken out: the home folder and
/// the user's name, network addresses, e-mail-like names.
pub fn clean(text: &str) -> String {
    let mut out = text.to_owned();
    for var in ["USERPROFILE", "HOME"] {
        if let Ok(home) = std::env::var(var)
            && home.len() > 3
        {
            out = out.replace(&home, "~").replace(&home.replace('\\', "/"), "~");
        }
    }
    for var in ["USERNAME", "USER", "COMPUTERNAME", "HOSTNAME"] {
        if let Ok(name) = std::env::var(var)
            && name.len() > 2
        {
            out = out.replace(&name, "<name>");
        }
    }
    let out = ipv4_out(&out);
    let words: Vec<String> = out
        .split_inclusive(|c: char| c.is_whitespace() || c == ',' || c == ')' || c == '(' || c == '"')
        .map(|word| {
            let (core, tail) = word.split_at(word.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '.').len());
            let shown = if is_address(core) {
                "<address>"
            } else if core.contains('@') && core.split('@').all(|part| !part.is_empty()) {
                "<name>"
            } else {
                core
            };
            format!("{shown}{tail}")
        })
        .collect();
    words.concat()
}

/// Every IPv4 address in a text, wherever it is (in a URL too), as `<address>`.
fn ipv4_out(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        let parts: Vec<&str> = run.split('.').collect();
        let address =
            parts.len() == 4 && parts.iter().all(|p| !p.is_empty() && p.len() <= 3 && p.parse::<u8>().is_ok());
        out.push_str(if address { "<address>" } else { run });
        run.clear();
    };
    for c in text.chars() {
        if c.is_ascii_digit() || c == '.' {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// Whether a word is a network address: IPv4 (with a port, maybe) or IPv6.
fn is_address(word: &str) -> bool {
    let host = word.trim_start_matches('[').split(']').next().unwrap_or(word);
    let v4 = host.split(':').next().unwrap_or(host);
    let parts: Vec<&str> = v4.split('.').collect();
    let ipv4 = parts.len() == 4 && parts.iter().all(|p| !p.is_empty() && p.len() <= 3 && p.parse::<u8>().is_ok());
    let ipv6 = host.matches(':').count() >= 2 && host.chars().all(|c| c.is_ascii_hexdigit() || c == ':');
    ipv4 || ipv6
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_identifies_a_player_is_taken_out() {
        let text = "joined ws://192.168.0.100:7878 as adrien@local from [fe80::1]:7881, 10.0.0.1 ok, version 0.7.0";
        let cleaned = clean(text);
        assert!(
            !cleaned.contains("192.168") && !cleaned.contains("fe80") && !cleaned.contains("10.0.0.1"),
            "{cleaned}"
        );
        assert!(!cleaned.contains("adrien@local") && cleaned.contains("version 0.7.0"), "{cleaned}");
        if let Ok(home) = std::env::var("HOME") {
            assert_eq!(clean(&format!("{home}/osage/kart")), "~/osage/kart");
        }
    }

    #[test]
    fn the_last_run_is_reported_only_when_something_went_wrong() {
        let folder = std::env::temp_dir().join(format!("osage-report-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let log = "SystemInfo { os: \"Windows 11 Home\" }\nAdapterInfo { name: \"NVIDIA GeForce RTX 3070\", backend: Dx12 }\n";
        // A clean exit: nothing.
        assert_eq!(Watch::last(&folder), None);
        // Started, then killed: nothing.
        std::fs::write(folder.join("running"), "started\n").unwrap();
        std::fs::write(folder.join("last.log"), log).unwrap();
        assert_eq!(Watch::last(&folder), None);
        // Never finished starting.
        std::fs::write(folder.join("running"), "starting\n").unwrap();
        let start = Watch::last(&folder).unwrap();
        assert_eq!(
            (start.kind, start.gpu.as_str(), start.os.as_str()),
            (Kind::Start, "NVIDIA GeForce RTX 3070", "Windows 11 Home")
        );
        // Renderer errors, after a start.
        std::fs::write(folder.join("running"), "started\n").unwrap();
        std::fs::write(
            folder.join("last.log"),
            format!("{log}2026 ERROR wgpu_core::device: Validation Error: shader\n"),
        )
        .unwrap();
        assert_eq!(Watch::last(&folder).unwrap().kind, Kind::Render);
        // A panic wins.
        std::fs::write(folder.join("crash.txt"), "panicked at play.rs:4:2:\nboom\n--- trace ---\n   0: kart::main")
            .unwrap();
        let panic = Watch::last(&folder).unwrap();
        assert!(panic.kind == Kind::Panic && panic.message.contains("boom") && panic.trace.contains("kart::main"));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
