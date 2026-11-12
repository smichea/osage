// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `kart-web [DIR] [ADDRESS]`: serves the browser client of the kart world
//! (its built files, `web/` by default) over plain HTTP on the local network
//! (`0.0.0.0:8080` by default), so that a phone or a tablet on the same
//! Wi-Fi plays by opening `http://<this computer>:8080`. The page then joins
//! the world on the same computer, port 7878.
//!
//! A static file server and nothing more: GET and HEAD, files under `DIR`
//! only, no listing. Standard library only.

use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    thread,
    time::Duration,
};

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().unwrap_or_else(|| "web".to_owned()));
    let address = args.next().unwrap_or_else(|| "0.0.0.0:8080".to_owned());
    if !root.join("index.html").is_file() {
        eprintln!("kart-web: no index.html in {}", root.display());
        std::process::exit(2);
    }
    let listener = TcpListener::bind(&address)?;
    println!("kart-web: serving {} on http://{address}", root.display());
    for stream in listener.incoming().flatten() {
        let root = root.clone();
        thread::spawn(move || {
            let _ = serve(stream, &root);
        });
    }
    Ok(())
}

fn serve(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    // The headers: read and ignored.
    let mut line = String::new();
    while reader.read_line(&mut line)? > 2 {
        line.clear();
    }
    let mut parts = request.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    if method != "GET" && method != "HEAD" {
        return respond(&mut stream, "405 Method Not Allowed", "text/plain", b"", method == "HEAD");
    }
    match file(root, target) {
        Some(path) => {
            let body = std::fs::read(&path)?;
            respond(&mut stream, "200 OK", content_type(&path), &body, method == "HEAD")
        }
        None => respond(&mut stream, "404 Not Found", "text/plain", b"not found", method == "HEAD"),
    }
}

/// The file a request names under `root`: never outside it; a path without
/// a file is the page.
fn file(root: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next().unwrap_or("/").replace("%20", " ");
    let relative = Path::new(path.trim_start_matches('/'));
    if relative.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    let wanted = root.join(relative);
    if wanted.is_file() {
        Some(wanted)
    } else if relative.extension().is_none() {
        Some(root.join("index.html"))
    } else {
        None
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "json" => "application/json",
        // WebAssembly is compiled while it streams only with its own type.
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "ogg" => "audio/ogg",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8], head: bool) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    if !head {
        stream.write_all(body)?;
    }
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_stay_inside_the_root_and_paths_without_a_file_are_the_page() {
        let root = std::env::temp_dir().join(format!("kart-web-test-{}", std::process::id()));
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("index.html"), "page").unwrap();
        std::fs::write(root.join("assets/a.js"), "js").unwrap();
        assert_eq!(file(&root, "/assets/a.js?v=1"), Some(root.join("assets/a.js")));
        assert_eq!(file(&root, "/"), Some(root.join("index.html")));
        assert_eq!(file(&root, "/race"), Some(root.join("index.html")));
        assert_eq!(file(&root, "/missing.js"), None);
        assert_eq!(file(&root, "/../secret"), None);
        assert_eq!(file(&root, "/assets/../../x"), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
