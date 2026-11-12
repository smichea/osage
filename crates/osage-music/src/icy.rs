// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A radio stream's titles: asked with `Icy-MetaData: 1`, a station puts
//! a block of metadata after every `icy-metaint` bytes of audio — a length
//! byte (times sixteen), then text such as `StreamTitle='…';`. The reader
//! takes the blocks out, so the decoder sees audio only, and hands each
//! title on.

use std::io::{self, Read};

pub struct Icy<R> {
    inner: R,
    /// Bytes of audio between two blocks.
    every: usize,
    /// Bytes of audio left before the next block.
    left: usize,
    title: Box<dyn FnMut(String) + Send>,
}

impl<R: Read> Icy<R> {
    pub fn new(inner: R, every: usize, title: impl FnMut(String) + Send + 'static) -> Self {
        Self { inner, every, left: every, title: Box::new(title) }
    }

    fn block(&mut self) -> io::Result<()> {
        let mut length = [0u8];
        self.inner.read_exact(&mut length)?;
        let mut text = vec![0u8; usize::from(length[0]) * 16];
        self.inner.read_exact(&mut text)?;
        if let Some(title) = stream_title(&String::from_utf8_lossy(&text)) {
            (self.title)(title);
        }
        Ok(())
    }
}

impl<R: Read> Read for Icy<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.every == 0 {
            return self.inner.read(buf);
        }
        if self.left == 0 {
            self.block()?;
            self.left = self.every;
        }
        let want = buf.len().min(self.left);
        let read = self.inner.read(&mut buf[..want])?;
        self.left -= read;
        Ok(read)
    }
}

/// The title in a block of stream metadata, when it has a non-empty one.
pub fn stream_title(text: &str) -> Option<String> {
    let start = text.find("StreamTitle='")? + "StreamTitle='".len();
    let rest = &text[start..];
    let end = rest.find("';").unwrap_or_else(|| rest.trim_end_matches('\0').trim_end_matches('\'').len());
    let title = rest[..end].trim();
    (!title.is_empty()).then(|| title.to_owned())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn the_titles_are_taken_out_of_the_audio_and_handed_on() {
        let mut stream = Vec::new();
        let meta = |text: &str| {
            let mut block = text.as_bytes().to_vec();
            block.resize(text.len().div_ceil(16) * 16, 0);
            let mut out = vec![(block.len() / 16) as u8];
            out.extend(block);
            out
        };
        stream.extend([1u8; 8]);
        stream.extend(meta("StreamTitle='Night Drive - Someone';StreamUrl='';"));
        stream.extend([2u8; 8]);
        stream.push(0);
        stream.extend([3u8; 5]);
        let titles = Arc::new(Mutex::new(Vec::new()));
        let heard = titles.clone();
        let mut icy = Icy::new(io::Cursor::new(stream), 8, move |t| heard.lock().unwrap().push(t));
        let mut audio = Vec::new();
        icy.read_to_end(&mut audio).unwrap();
        assert_eq!(audio, [[1u8; 8], [2u8; 8]].concat().into_iter().chain([3u8; 5]).collect::<Vec<_>>());
        assert_eq!(*titles.lock().unwrap(), ["Night Drive - Someone"]);
        assert_eq!(stream_title("StreamTitle='';"), None);
    }
}
