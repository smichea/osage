// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Internet radio: the stations a player lists in a file of their own,
//! and the community directory radio-browser.info, asked only when the
//! player has turned it on — asking it tells its servers what is searched
//! for, from where.
//!
//! A list (`osage-radio/1`): one station a line, `station NAME | URL`;
//! blank lines and lines from `#` are skipped.

use std::fmt::Write as _;

/// The directory's address for any of its servers.
pub const DIRECTORY: &str = "https://all.api.radio-browser.info";
/// The first line of a station list.
pub const HEADER: &str = "osage-radio/1";
/// The codecs a station may stream in to be played.
const PLAYED: [&str; 5] = ["MP3", "AAC", "AAC+", "OGG", "FLAC"];

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Station {
    pub name: String,
    pub url: String,
    /// As the directory says, when it does: `MP3`, `AAC`…
    pub codec: String,
    pub bitrate: u32,
    pub country: String,
    pub tags: String,
    pub homepage: String,
}

impl Station {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self { name: name.into(), url: url.into(), ..Self::default() }
    }

    /// A file extension hinting at what the stream is, when it is known.
    pub(crate) fn extension(&self) -> Option<&'static str> {
        match self.codec.to_ascii_uppercase().as_str() {
            "MP3" => Some("mp3"),
            "AAC" | "AAC+" => Some("aac"),
            "OGG" => Some("ogg"),
            "FLAC" => Some("flac"),
            _ => None,
        }
    }
}

/// The stations of a list, and the lines that are not one: (line, why).
pub fn parse(text: &str) -> (Vec<Station>, Vec<(usize, String)>) {
    let mut stations = Vec::new();
    let mut problems = Vec::new();
    for (n, line) in text.lines().enumerate().map(|(n, l)| (n + 1, l.trim())) {
        if line.is_empty() || line.starts_with('#') || line == HEADER {
            continue;
        }
        let Some(rest) = line.strip_prefix("station ") else {
            problems.push((n, format!("not a station: {line}")));
            continue;
        };
        match rest.rsplit_once('|').map(|(name, url)| (name.trim(), url.trim())) {
            Some((name, url)) if !name.is_empty() && (url.starts_with("http://") || url.starts_with("https://")) => {
                stations.push(Station::new(name, url));
            }
            _ => problems.push((n, "a station is `station NAME | http(s)://…`".to_owned())),
        }
    }
    (stations, problems)
}

/// A list of `stations`, as [`parse`] reads it.
pub fn write(stations: &[Station]) -> String {
    let mut text = format!("{HEADER}\n");
    for station in stations {
        let _ = writeln!(text, "station {} | {}", station.name.replace('|', "/"), station.url);
    }
    text
}

/// What to ask the directory for.
#[derive(Debug, Clone, Default)]
pub struct Search {
    /// Stations with this tag (`electronic`, `rock`…); any if empty.
    pub tag: String,
    /// Stations whose name has this in it; any if empty.
    pub name: String,
    /// Stations of this country (ISO 3166-1 code, `FR`); any if empty.
    pub country: String,
    pub limit: u32,
}

/// The directory's stations that play, most listened to first. This asks
/// the directory's servers: only when the player chose to.
pub fn search(search: &Search) -> std::io::Result<Vec<Station>> {
    let url = format!("{DIRECTORY}/json/stations/search?{}", query(search));
    let mut response = crate::agent().get(&url).call().map_err(std::io::Error::other)?;
    let text = response.body_mut().with_config().limit(4 << 20).read_to_string().map_err(std::io::Error::other)?;
    from_directory(&text)
}

fn query(search: &Search) -> String {
    let mut query = format!(
        "hidebroken=true&order=clickcount&reverse=true&limit={}",
        if search.limit == 0 { 50 } else { search.limit.min(500) }
    );
    for (key, value) in [("tag", &search.tag), ("name", &search.name), ("countrycode", &search.country)] {
        if !value.is_empty() {
            let _ = write!(query, "&{key}={}", escape(value));
        }
    }
    query
}

/// `text` as it goes in a URL's query.
fn escape(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            other => {
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}

/// The stations of a directory answer that play in a codec osage decodes.
pub fn from_directory(json: &str) -> std::io::Result<Vec<Station>> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(std::io::Error::other)?;
    let text = |station: &serde_json::Value, key: &str| station[key].as_str().unwrap_or_default().trim().to_owned();
    Ok(value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|station| {
            let url = Some(text(station, "url_resolved")).filter(|u| !u.is_empty()).unwrap_or(text(station, "url"));
            let codec = text(station, "codec");
            let played = PLAYED.iter().any(|c| c.eq_ignore_ascii_case(&codec));
            (played && url.starts_with("http")).then(|| Station {
                name: text(station, "name"),
                url,
                codec,
                bitrate: station["bitrate"].as_u64().unwrap_or(0) as u32,
                country: text(station, "countrycode"),
                tags: text(station, "tags"),
                homepage: text(station, "homepage"),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_is_read_and_written_and_its_mistakes_are_told() {
        let (stations, problems) = parse(
            "osage-radio/1\n# mine\nstation Late Night | Jazz | https://example.org/live.mp3\n\nstation nowhere\nsong x\n",
        );
        assert_eq!(stations, [Station::new("Late Night | Jazz", "https://example.org/live.mp3")]);
        assert_eq!(problems.iter().map(|p| p.0).collect::<Vec<_>>(), [5, 6]);
        let (again, none) = parse(&write(&stations));
        assert!(none.is_empty());
        assert_eq!(again[0].name, "Late Night / Jazz");
    }

    #[test]
    fn the_directorys_answer_keeps_the_stations_that_play() {
        let json = r#"[
          {"name":" Drive FM ","url":"http://a/x","url_resolved":"https://a/y.mp3","codec":"MP3","bitrate":128,"countrycode":"FR","tags":"electronic,dance","homepage":"https://a"},
          {"name":"Video","url":"https://b/hls.m3u8","url_resolved":"","codec":"UNKNOWN"},
          {"name":"Old","url":"http://c/s","codec":"aac"}
        ]"#;
        let stations = from_directory(json).unwrap();
        assert_eq!(stations.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Drive FM", "Old"]);
        assert_eq!((stations[0].url.as_str(), stations[0].extension()), ("https://a/y.mp3", Some("mp3")));
        assert_eq!(stations[1].url, "http://c/s");
        let asked = query(&Search { tag: "drum & bass".into(), country: "FR".into(), ..Search::default() });
        assert!(asked.ends_with("limit=50&tag=drum%20%26%20bass&countrycode=FR"), "{asked}");
    }
}
