// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The holograms' own small HTTP/1.1: a server beside a world that sends
//! block files and answers what a viewer needs, and a client that asks
//! for them over one kept-alive connection.
//!
//! - `GET /h/PATH`: the file at PATH under the server's folder of holograms.
//! - `GET /need?QUERY`: what the world answers for QUERY (a viewer's place
//!   and speed, say), as JSON.

use std::{
    io::{self, BufRead as _, BufReader, Read as _, Write as _},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::Arc,
    thread,
    time::Duration,
};

/// What a world answers to `/need?QUERY`: JSON, or none for a bad query.
pub type Need = dyn Fn(&str) -> Option<String> + Send + Sync;

/// Serves `root`'s files and `need` on `listener`, a thread a connection;
/// runs until the program ends.
pub fn serve(listener: TcpListener, root: PathBuf, need: Arc<Need>) -> io::Result<thread::JoinHandle<()>> {
    thread::Builder::new().name("hologram-http".to_owned()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let (root, need) = (root.clone(), need.clone());
            let _ = thread::Builder::new().name("hologram-http-client".to_owned()).spawn(move || {
                let _ = answer(stream, &root, &*need);
            });
        }
    })
}

fn answer(stream: TcpStream, root: &std::path::Path, need: &Need) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    stream.set_nodelay(true)?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        // The headers: none is needed (no request has a body).
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
                break;
            }
        }
        let mut parts = line.split_whitespace();
        let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        if method != "GET" {
            reply(&mut writer, "405 Method Not Allowed", "text/plain", b"GET only")?;
        } else if let Some(query) = target.strip_prefix("/need?") {
            match need(query) {
                Some(json) => reply(&mut writer, "200 OK", "application/json", json.as_bytes())?,
                None => reply(&mut writer, "400 Bad Request", "text/plain", b"bad query")?,
            }
        } else if let Some(path) = target.strip_prefix("/h/").filter(|p| safe(p)) {
            match std::fs::read(root.join(path)) {
                Ok(bytes) => {
                    let kind = if path.ends_with(".json") { "application/json" } else { "application/octet-stream" };
                    reply(&mut writer, "200 OK", kind, &bytes)?;
                }
                Err(_) => reply(&mut writer, "404 Not Found", "text/plain", b"no such file")?,
            }
        } else {
            reply(&mut writer, "404 Not Found", "text/plain", b"no such thing")?;
        }
    }
}

/// A path of plain names: letters, digits, `-`, `_`, `.`, `+`, `/`, never `..`.
fn safe(path: &str) -> bool {
    !path.is_empty()
        && !path.contains("..")
        && !path.starts_with('/')
        && path.chars().all(|c| c.is_ascii_alphanumeric() || "-_.+/".contains(c))
}

fn reply(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) -> io::Result<()> {
    let head = format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\r\n", body.len());
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// Asks a hologram server, over one connection kept alive (made again when
/// the server closes it).
pub struct Client {
    address: String,
    connection: Option<BufReader<TcpStream>>,
}

impl Client {
    /// A client of the server at `address` (`host:port`).
    pub fn new(address: impl Into<String>) -> Self {
        Self { address: address.into(), connection: None }
    }

    /// The body of `GET path` (`/h/...` or `/need?...`); an error unless 200.
    pub fn get(&mut self, path: &str) -> io::Result<Vec<u8>> {
        // Once again on a fresh connection, if the kept one had closed.
        match self.try_get(path) {
            Err(error) if self.connection.is_none() => Err(error),
            Err(_) => {
                self.connection = None;
                self.try_get(path)
            }
            ok => ok,
        }
    }

    fn try_get(&mut self, path: &str) -> io::Result<Vec<u8>> {
        if self.connection.is_none() {
            let stream = TcpStream::connect(&self.address)?;
            stream.set_nodelay(true)?;
            stream.set_read_timeout(Some(Duration::from_secs(30)))?;
            self.connection = Some(BufReader::new(stream));
        }
        let Some(connection) = self.connection.as_mut() else { unreachable!() };
        let request = format!("GET {path} HTTP/1.1\r\nHost: {}\r\nUser-Agent: osage-hologram\r\n\r\n", self.address);
        connection.get_mut().write_all(request.as_bytes())?;
        let mut status = String::new();
        if connection.read_line(&mut status)? == 0 {
            self.connection = None;
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the server closed the connection"));
        }
        let mut length = None;
        loop {
            let mut header = String::new();
            if connection.read_line(&mut header)? == 0 || header.trim().is_empty() {
                break;
            }
            if let Some((name, value)) = header.split_once(':')
                && name.trim().eq_ignore_ascii_case("content-length")
            {
                length = value.trim().parse::<usize>().ok();
            }
        }
        let length = length.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no Content-Length"))?;
        let mut body = vec![0; length];
        connection.read_exact(&mut body)?;
        if !status.split_whitespace().nth(1).is_some_and(|code| code == "200") {
            return Err(io::Error::other(format!("{path}: {}", status.trim())));
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_and_needs_come_over_one_connection() {
        let root = std::env::temp_dir().join(format!("osage-hologram-http-{}", std::process::id()));
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::write(root.join("a/view-b00-00.ivf"), b"block").unwrap();
        std::fs::create_dir_all(root.join("+x_1_0_-2")).unwrap();
        std::fs::write(root.join("+x_1_0_-2/mask.ivf"), b"tile").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let need: Arc<Need> = Arc::new(|query: &str| (query == "x=1").then(|| "[\"a\"]".to_owned()));
        serve(listener, root.clone(), need).unwrap();
        let mut client = Client::new(address);
        assert_eq!(client.get("/h/a/view-b00-00.ivf").unwrap(), b"block");
        assert_eq!(client.get("/h/+x_1_0_-2/mask.ivf").unwrap(), b"tile", "a tile's key");
        assert_eq!(client.get("/need?x=1").unwrap(), b"[\"a\"]");
        assert!(client.get("/need?x=2").is_err());
        assert!(client.get("/h/../secret").is_err());
        assert!(client.get("/h/a/missing.ivf").is_err());
        assert_eq!(client.get("/h/a/view-b00-00.ivf").unwrap(), b"block", "still served after errors");
        std::fs::remove_dir_all(root).unwrap();
    }
}
