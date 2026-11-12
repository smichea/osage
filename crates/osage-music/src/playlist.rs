// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Playlists: a file that lists pieces with what to show of them, or a
//! folder of music.
//!
//! A playlist file (`osage-playlist/1`), a piece a line:
//! `track FILE | TITLE | ARTIST | LICENCE | GAIN` — the file relative to
//! the playlist's folder, the gain in decibels (`-3`); blank lines and
//! lines from `#` are skipped.

use std::path::{Path, PathBuf};

use crate::{Track, is_music};

pub const HEADER: &str = "osage-playlist/1";

/// A playlist read: its pieces, and the lines that are not one (line, why).
pub type Read = (Vec<Track>, Vec<(usize, String)>);

/// The pieces of a playlist file read from `folder`, and the lines that
/// are not one: (line, why).
pub fn parse(text: &str, folder: &Path) -> Read {
    let mut tracks = Vec::new();
    let mut problems = Vec::new();
    for (n, line) in text.lines().enumerate().map(|(n, l)| (n + 1, l.trim())) {
        if line.is_empty() || line.starts_with('#') || line == HEADER {
            continue;
        }
        let Some(rest) = line.strip_prefix("track ") else {
            problems.push((n, format!("not a track: {line}")));
            continue;
        };
        let fields: Vec<&str> = rest.split('|').map(str::trim).collect();
        let field = |i: usize| fields.get(i).copied().unwrap_or("").to_owned();
        let decibels = match fields.get(4).filter(|g| !g.is_empty()) {
            None => Ok(0.0),
            Some(g) => g.parse::<f32>().map_err(|_| format!("a gain is decibels: {g}")),
        };
        match (fields[0], decibels) {
            ("", _) => problems.push((n, "a track names its file".to_owned())),
            (_, Err(why)) => problems.push((n, why)),
            (file, Ok(decibels)) => tracks.push(Track {
                title: field(1),
                artist: field(2),
                licence: field(3),
                gain: 10f32.powf(decibels.clamp(-40.0, 12.0) / 20.0),
                ..Track::file(folder.join(file))
            }),
        }
    }
    (tracks, problems)
}

/// The playlist in a file.
pub fn read(path: &Path) -> std::io::Result<Read> {
    let text = std::fs::read_to_string(path)?;
    Ok(parse(&text, path.parent().unwrap_or(Path::new("."))))
}

/// The music files of a folder, by name: their tags are shown as they play.
pub fn folder(folder: &Path) -> std::io::Result<Vec<Track>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(folder)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_file() && is_music(path))
        .collect();
    paths.sort();
    Ok(paths.into_iter().map(Track::file).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_playlist_file_is_read_and_its_mistakes_are_told() {
        let text = "osage-playlist/1\n# bundled\ntrack a.ogg | Hot | Someone | CC0-1.0 | -6\ntrack b.mp3\ntrack c.ogg | x | y | z | loud\ntune d.ogg\n";
        let (tracks, problems) = parse(text, Path::new("/music"));
        assert_eq!(tracks.len(), 2);
        assert_eq!((tracks[0].title.as_str(), tracks[0].licence.as_str()), ("Hot", "CC0-1.0"));
        assert!((tracks[0].gain - 0.501).abs() < 0.01);
        assert_eq!(tracks[1].origin, crate::Origin::File("/music/b.mp3".into()));
        assert_eq!(tracks[1].gain, 1.0);
        assert_eq!(problems.iter().map(|p| p.0).collect::<Vec<_>>(), [5, 6]);
    }
}
