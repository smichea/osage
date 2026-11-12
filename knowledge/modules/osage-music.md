---
type: Module
title: osage-music
description: "Music for a world's players: playlists of files, internet radio and its directory (opt-in), a host's playlist shared on the local network and heard at the same point by all; decoded on a thread into a feed the audio output never waits on."
resource: https://github.com/smichea/osage/tree/main/crates/osage-music
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [audio, music, radio, streaming, lan, engine]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T22:30:00Z" }
---

# Purpose

Music in a game, from wherever the player wants it (D-059): the game's own pieces, the player's own files, an internet radio station, or the playlist of the person hosting the game, shared with the friends on the same network. An engine mechanism: what plays, whether the directory is asked and what a host shares are the world's and its players' choices.

# Concepts

- **Track:** a piece — title, artist, licence, a file or a URL fetched whole, a gain.
- **Program:** what the player plays — silence, a playlist (round and round from a first track), a station, a host's share.
- **Feed:** the samples, stereo at 44.1 kHz, in chunks of a tenth of a second, a second and a half ahead. Read without waiting: nothing come yet is silence.
- **Generation:** each order (a new program, a skip) starts one; chunks of an older generation are dropped by the feed, even the one being heard, so a change is heard at once.
- **Station:** a name and a stream URL; the codec, bit rate, country and tags when the directory gives them.
- **Share:** the host's folder of music, played on the host's clock: track after track, round and round, whether anyone listens or not.

# Architecture

`Player::new` starts a thread that runs the program: it opens a piece (a file, a URL fetched whole, a station's stream, a share's track), decodes it with Symphonia a packet at a time (Ogg Vorbis, MP3, AAC in MP4/ADTS, FLAC, WAV), brings it to stereo at the feed's rate by linear interpolation (mono heard on both sides), applies the track's gain, and sends chunks on a bounded channel; between packets and while the channel is full it listens for orders. A piece that does not open is told (`Status.problem`) and the next one played; a station that drops is called again (five times, then a pause of two seconds that an order cuts short).

A station is asked with `Icy-MetaData: 1`; when it answers with `icy-metaint`, `Icy` takes the metadata blocks out of the audio and hands each `StreamTitle` on (`Artist - Title` shown as title and artist), the station's `icy-name` as where it comes from.

The directory (`radio::search`) asks radio-browser.info's `/json/stations/search` for a tag, a name or a country, the most listened first, without broken stations; keeps those in a codec osage decodes. It is asked only when called.

The share (`share::serve`) is a small HTTP/1.1 server on a thread: `GET /music/list` (the pieces and their lengths), `/music/now` (the piece and the seconds into it on the host's clock), `/music/track/N` (the file) — nothing else, no path from the request reaches the file system. `share::scan` measures each file of a folder (from its container, or by decoding it whole when the container does not say), leaving out what does not decode. A player tuning in (`Program::Shared`) asks where the playlist is, fetches that track, asks again (the fetch took time the host played on) and starts from there; at the end of the piece it asks again.

With the `bevy` feature, `audio::MusicSource` is a Bevy audio source made from a feed: `AudioPlayer` plays it as any sound, and it never ends.

# Interfaces

`Player::{new, play, skip, set_gain, feed, status}`, `Program`, `Track`, `Origin`, `Status`, `NowPlaying`, `Feed::listen`, `Listening::sample`; `Decoded::{open, stream, samples, seek, seconds, measure, tags}`; `playlist::{parse, read, folder}` (`osage-playlist/1`: `track FILE | TITLE | ARTIST | LICENCE | GAIN_DB`); `radio::{parse, write, search, from_directory, Station, Search}` (`osage-radio/1`: `station NAME | URL`); `share::{scan, serve, Remote, Timeline, Piece}`; `RATE`, `CHANNELS`, `USER_AGENT`, `is_music`. The example `music` probes files (length, tags, peak, loudness), tries the directory and a station, or shares a folder.

# Invariants

- A piece is heard in stereo at the feed's rate whatever it was made at (44.1, 48, 22.05 kHz; mono or stereo), at its loudness; it can be sought in; something that is not music does not open. Tested.
- A playlist is fed and heard; a gain is heard at once; a skip is heard at once; after silence is ordered, nothing fed before is heard. Tested.
- A track that does not play is told and the next one plays. Tested.
- A stream's titles are taken out of its audio and handed on. Tested.
- A list of stations and a playlist file read back what they wrote, and tell their bad lines. Tested.
- The directory's answer keeps the stations that play; a search is escaped into the query. Tested.
- A shared folder is scanned in name order, leaving out what does not decode; a player gets the list, where the playlist is, and exactly the files shared — nothing else. The playlist goes round on the host's clock. Tested.

# Knowledge

- Radio stations master loud (peaks at full scale): under the karts they are played lower than the game's levelled pieces.
- Many stations stream MP3 or AAC at 44.1 kHz; HLS playlists (`.m3u8`) are not streams and are left out.
- The share is plain HTTP on the local network: anyone on that network can listen, as anyone there can join the game.
- Asking the directory tells its servers the player's address and what they searched; the game asks only once the player chooses a kind of music.

# Reuse

Any world with music: a hub's lobby radio, a party's shared playlist, a racing game's soundtrack. The share's clock gives everyone the same moment: the base of a listening party.

# Limits

Linear interpolation, no better resampling; no crossfade between pieces; no Opus; HLS not played; a share's player is in step to the fetch's time (a fraction of a second), not to the sample; the share has no access control beyond the network it is on. Played only in the native client; the browser client has no music yet.
