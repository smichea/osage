// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Independent disk and HTTP reads for district content, shared by its visual and physical consumers.
use super::{models::DecodedModel, tiles::File};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    io::Write,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

pub enum Source {
    Directory(PathBuf),
    Paced(Box<Pool>),
}

#[derive(Default, Clone, Serialize)]
pub struct Traffic {
    pub disk_index_bytes: u64,
    pub disk_content_reads: u64,
    pub disk_content_bytes: u64,
    pub index_requests: u64,
    /// Directory bytes delivered by the HTTP reader after content decoding. A 304 adds zero.
    pub index_body_bytes: u64,
    pub index_revalidations: u64,
    pub content_requests: u64,
    pub content_body_bytes: u64,
    pub cache_hits: u64,
    pub cache_bytes: u64,
    pub invalid_cache_entries: u64,
    pub joined_reads: u64,
    pub deferred_full_queue: u64,
    pub worker_image_decodes: u64,
    pub worker_image_decode_us: u64,
    pub worker_model_decodes: u64,
    pub worker_model_decode_us: u64,
    pub rejected_bodies: u64,
}

struct Client {
    bc7_limit: AtomicU32,
    origin: Origin,
    stats: Mutex<Traffic>,
}

enum Origin {
    Disk(PathBuf),
    Web(Http),
}

struct Http {
    base: String,
    agent: ureq::Agent,
    cache: Option<PathBuf>,
}

#[derive(Deserialize, Serialize)]
struct IndexValidator {
    etag: String,
    sha256: String,
}

impl IndexValidator {
    fn decode(raw: &[u8]) -> Option<(Self, &[u8])> {
        let end = raw.iter().position(|v| *v == b'\n')?;
        let meta: Self = serde_json::from_slice(&raw[..end]).ok()?;
        let bytes = &raw[end + 1..];
        (meta.sha256 == crate::bench::sha(bytes)).then_some((meta, bytes))
    }
}

// The rename publishes complete bytes to all processes sharing this cache.
fn persist(path: &std::path::Path, chunks: &[&[u8]]) -> Result<()> {
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().context("cache directory")?)?;
    for chunk in chunks {
        tmp.write_all(chunk)?;
    }
    tmp.persist(path)?;
    Ok(())
}

impl Http {
    fn request(&self, name: &str, validator: Option<&str>) -> Result<ureq::http::Response<ureq::Body>> {
        // The directory is JSON. Immutable payloads already have their own storage encoding.
        let encoding = if name == "index.json" { "gzip, identity" } else { "identity" };
        let mut request = self.agent.get(format!("{}/{name}", self.base)).header("accept-encoding", encoding);
        if let Some(tag) = validator {
            request = request.header("if-none-match", tag);
        }
        request.call().with_context(|| format!("fetching {name}"))
    }

    fn index(&self, stats: &Mutex<Traffic>) -> Result<Vec<u8>> {
        let path =
            self.cache.as_ref().map(|dir| dir.join(format!("index-{}.http", crate::bench::sha(self.base.as_bytes()))));
        let raw = path.as_ref().and_then(|p| fs::read(p).ok()).unwrap_or_default();
        let cached = IndexValidator::decode(&raw);
        stats.lock().unwrap().index_requests += 1;
        let mut response = self.request("index.json", cached.as_ref().map(|(m, _)| m.etag.as_str()))?;
        if response.status().as_u16() == 304 {
            let (_, bytes) = cached.context("origin returned 304 without a verified district index")?;
            stats.lock().unwrap().index_revalidations += 1;
            return Ok(bytes.to_vec());
        }
        ensure!(response.status().as_u16() == 200, "incomplete district index response");
        let etag = response.headers().get("etag").and_then(|s| s.to_str().ok()).map(str::to_owned);
        let can_store = !response
            .headers()
            .get_all("cache-control")
            .iter()
            .filter_map(|h| h.to_str().ok())
            .flat_map(|h| h.split(','))
            .any(|directive| directive.trim().eq_ignore_ascii_case("no-store"));
        let body = response.body_mut().with_config().limit(64 * 1024 * 1024).read_to_vec()?;
        stats.lock().unwrap().index_body_bytes += body.len() as u64;
        if let Some(path) = path {
            if let Some(etag) = etag.filter(|_| can_store) {
                let meta = IndexValidator { etag, sha256: crate::bench::sha(&body) };
                persist(&path, &[&serde_json::to_vec(&meta)?, b"\n", &body])?;
            } else if !raw.is_empty() {
                match fs::remove_file(path) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                    _ => (),
                }
            }
        }
        Ok(body)
    }

    fn cached(&self, file: &File, stats: &Mutex<Traffic>) -> Result<Option<Vec<u8>>> {
        use std::io::Read;
        let Some(dir) = &self.cache else { return Ok(None) };
        let path = dir.join(&file.file);
        let input = match fs::File::open(&path) {
            Ok(input) => input,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("reading cache {}", path.display())),
        };
        let mut bytes = Vec::new();
        input.take((file.bytes as u64).saturating_add(1)).read_to_end(&mut bytes)?;
        if file.check(&bytes).is_err() {
            stats.lock().unwrap().invalid_cache_entries += 1;
            return Ok(None);
        }
        let mut stats = stats.lock().unwrap();
        stats.cache_hits += 1;
        stats.cache_bytes += bytes.len() as u64;
        Ok(Some(bytes))
    }

    fn content(&self, file: &File, stats: &Mutex<Traffic>) -> Result<Vec<u8>> {
        if let Some(bytes) = self.cached(file, stats)? {
            return Ok(bytes);
        }
        stats.lock().unwrap().content_requests += 1;
        let mut response = self.request(&file.file, None)?;
        ensure!(response.status().as_u16() == 200, "incomplete HTTP response for {}", file.file);
        let bytes = response.body_mut().with_config().limit((file.bytes as u64).saturating_add(1)).read_to_vec()?;
        stats.lock().unwrap().content_body_bytes += bytes.len() as u64;
        if let Err(e) = file.check(&bytes) {
            stats.lock().unwrap().rejected_bodies += 1;
            return Err(e);
        }
        if let Some(dir) = &self.cache {
            persist(&dir.join(&file.file), &[&bytes])?;
        }
        Ok(bytes)
    }
}

impl Client {
    fn index(&self) -> Result<Vec<u8>> {
        match &self.origin {
            Origin::Web(http) => http.index(&self.stats),
            Origin::Disk(dir) => {
                let bytes = fs::read(dir.join("index.json"))?;
                self.stats.lock().unwrap().disk_index_bytes += bytes.len() as u64;
                Ok(bytes)
            }
        }
    }

    fn content(&self, file: &File) -> Result<Vec<u8>> {
        match &self.origin {
            Origin::Web(http) => http.content(file, &self.stats),
            Origin::Disk(dir) => {
                let bytes = fs::read(dir.join(&file.file)).with_context(|| format!("reading {}", file.file))?;
                let mut stats = self.stats.lock().unwrap();
                stats.disk_content_reads += 1;
                stats.disk_content_bytes += bytes.len() as u64;
                drop(stats);
                if let Err(e) = file.check(&bytes) {
                    self.stats.lock().unwrap().rejected_bodies += 1;
                    return Err(e);
                }
                Ok(bytes)
            }
        }
    }

    fn prepare(&self, file: &File, decode: WorkerDecode) -> Result<Payload> {
        let bytes = self.content(file)?;
        if decode.models && file.file.ends_with(".model") {
            let start = std::time::Instant::now();
            let model = DecodedModel::read(&bytes).map_err(|e| format!("{e:#}"));
            let elapsed = start.elapsed().as_micros() as u64;
            let mut stats = self.stats.lock().unwrap();
            stats.worker_model_decodes += 1;
            stats.worker_model_decode_us += elapsed;
            return Ok(Payload::Model { encoded_len: bytes.len(), model });
        }
        let mime = if file.file.ends_with(".png") {
            Some("image/png")
        } else if file.file.ends_with(".jpg") {
            Some("image/jpeg")
        } else if file.file.ends_with(".ktx2") {
            Some("image/ktx2")
        } else {
            None
        };
        let image = if let Some(mime) = mime.filter(|_| decode.images) {
            let start = std::time::Instant::now();
            let pixels = crate::texture::Pixels::read_resource(&bytes, mime, self.bc7_limit.load(Ordering::Relaxed))?;
            let mut stats = self.stats.lock().unwrap();
            stats.worker_image_decodes += 1;
            stats.worker_image_decode_us += start.elapsed().as_micros() as u64;
            Some(pixels)
        } else {
            None
        };
        Ok(Payload::Bytes { bytes, image })
    }
}

enum Payload {
    Bytes { bytes: Vec<u8>, image: Option<Arc<crate::texture::Pixels>> },
    Model { encoded_len: usize, model: std::result::Result<DecodedModel, String> },
}

#[derive(Clone, Copy)]
pub(super) struct WorkerDecode {
    pub images: bool,
    pub models: bool,
}

impl Default for WorkerDecode {
    fn default() -> Self {
        Self { images: true, models: true }
    }
}

type Outcome = OnceLock<std::result::Result<Payload, String>>;

pub struct Ticket {
    outcome: Arc<Outcome>,
    simulated_due: bool,
}

pub struct QueueFull;

impl Ticket {
    pub fn ready(&self) -> bool {
        self.simulated_due || self.outcome.get().is_some()
    }
    fn payload(&self) -> Result<&Payload> {
        self.outcome.wait().as_ref().map_err(|e| anyhow::anyhow!("{e}"))
    }
    pub fn encoded_len(&self) -> Result<usize> {
        Ok(match self.payload()? {
            Payload::Bytes { bytes, .. } => bytes.len(),
            Payload::Model { encoded_len, .. } => *encoded_len,
        })
    }
    pub fn bytes(&self) -> Result<&[u8]> {
        match self.payload()? {
            Payload::Bytes { bytes, .. } => Ok(bytes),
            Payload::Model { .. } => anyhow::bail!("raw bytes released after model preparation"),
        }
    }
    pub fn image(&self) -> Result<Option<Arc<crate::texture::Pixels>>> {
        match self.payload()? {
            Payload::Bytes { image, .. } => Ok(image.clone()),
            Payload::Model { .. } => anyhow::bail!("model was requested as an image"),
        }
    }
    pub(super) fn model(&self) -> Result<Option<&DecodedModel>> {
        match self.payload()? {
            Payload::Model { model, .. } => model.as_ref().map(Some).map_err(|e| anyhow::anyhow!("{e}")),
            Payload::Bytes { .. } => Ok(None),
        }
    }
}

pub struct Pool {
    simulated_images: bool,
    client: Arc<Client>,
    pending: RefCell<BTreeMap<String, Weak<Outcome>>>,
    sender: Option<mpsc::SyncSender<(File, Arc<Outcome>)>>,
    workers: Vec<thread::JoinHandle<()>>,
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.sender.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl Source {
    pub fn open(location: String, cache: Option<PathBuf>, decode: WorkerDecode, wall_clock: bool) -> Result<Self> {
        let origin = if location.starts_with("http://") || location.starts_with("https://") {
            let uri: ureq::http::Uri = location.parse().context("invalid district URL")?;
            ensure!(uri.authority().is_some() && uri.query().is_none(), "expected an HTTP district directory URL");
            if let Some(path) = &cache {
                fs::create_dir_all(path)?;
            }
            Origin::Web(Http {
                base: location.trim_end_matches('/').into(),
                agent: ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).build().into(),
                cache,
            })
        } else {
            ensure!(cache.is_none(), "--content-cache requires an HTTP district");
            if !wall_clock {
                return Ok(Self::Directory(location.into()));
            }
            Origin::Disk(location.into())
        };
        Self::workers(origin, decode, false)
    }

    fn workers(origin: Origin, decode: WorkerDecode, simulated_images: bool) -> Result<Self> {
        let client = Arc::new(Client { origin, stats: Default::default(), bc7_limit: AtomicU32::new(0) });
        let (sender, rx) = mpsc::sync_channel::<(File, Arc<Outcome>)>(8);
        let rx = Arc::new(Mutex::new(rx));
        let mut workers = Vec::new();
        for _ in 0..4 {
            let (rx, client) = (rx.clone(), client.clone());
            workers.push(thread::Builder::new().name("city-content".into()).spawn(move || {
                loop {
                    let next = rx.lock().unwrap().recv();
                    let Ok((file, result)) = next else { break };
                    let loaded = if Arc::strong_count(&result) > 1 {
                        client.prepare(&file, decode).map_err(|e| format!("{e:#}"))
                    } else {
                        Err("request released before the read".to_owned())
                    };
                    let _ = result.set(loaded);
                }
            })?);
        }
        Ok(Self::Paced(Box::new(Pool {
            client,
            simulated_images,
            pending: Default::default(),
            sender: Some(sender),
            workers,
        })))
    }

    pub fn configure_bc7(&mut self, limit: Option<u32>, decode: WorkerDecode) -> Result<()> {
        // A deterministic replay delivers at its scheduled tick. Start its texture work when
        // the read is requested and await that work at delivery without changing simulation time.
        if decode.images
            && let Self::Directory(path) = self
        {
            *self = Self::workers(Origin::Disk(path.clone()), decode, true)?;
        }
        if let Self::Paced(pool) = self {
            pool.client.bc7_limit.store(limit.unwrap_or(0), Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn paced(&self) -> bool {
        matches!(self, Self::Paced(pool) if !pool.simulated_images)
    }

    pub fn index(&self) -> Result<Vec<u8>> {
        match self {
            Self::Directory(dir) => Ok(fs::read(dir.join("index.json"))?),
            Self::Paced(pool) => pool.client.index(),
        }
    }

    pub fn start(&self, file: &File, preload: bool) -> std::result::Result<Option<Ticket>, QueueFull> {
        let Self::Paced(pool) = self else { return Ok(None) };
        if pool.simulated_images && ![".ktx2", ".png", ".jpg"].iter().any(|ext| file.file.ends_with(ext)) {
            return Ok(None);
        }
        let mut pending = pool.pending.borrow_mut();
        if let Some(job) = pending.get(&file.file).and_then(Weak::upgrade) {
            pool.client.stats.lock().unwrap().joined_reads += 1;
            return Ok(Some(Ticket { outcome: job, simulated_due: pool.simulated_images }));
        }
        pending.retain(|_, v| v.strong_count() > 0);
        let job = Arc::new(Outcome::new());
        let sender = pool.sender.as_ref().unwrap();
        let work = (file.clone(), job.clone());
        if preload {
            sender.send(work).expect("content workers active");
        } else if sender.try_send(work).is_err() {
            pool.client.stats.lock().unwrap().deferred_full_queue += 1;
            return Err(QueueFull);
        }
        pending.insert(file.file.clone(), Arc::downgrade(&job));
        Ok(Some(Ticket { outcome: job, simulated_due: pool.simulated_images }))
    }

    pub(super) fn cached(&self, file: &File) -> Result<Option<Vec<u8>>> {
        if let Self::Paced(pool) = self
            && let Origin::Web(http) = &pool.client.origin
        {
            return http.cached(file, &pool.client.stats);
        }
        Ok(None)
    }

    pub(super) fn store_verified(&self, file: &File, bytes: &[u8]) -> Result<()> {
        if let Self::Paced(pool) = self
            && let Origin::Web(http) = &pool.client.origin
            && let Some(dir) = &http.cache
        {
            persist(&dir.join(&file.file), &[bytes])?;
        }
        Ok(())
    }

    pub fn raw(&self, file: &File) -> Result<Vec<u8>> {
        match self {
            Self::Directory(dir) => fs::read(dir.join(&file.file)).with_context(|| format!("reading {}", file.file)),
            Self::Paced(pool) => pool.client.content(file),
        }
    }

    pub fn traffic(&self) -> Option<(&'static str, Traffic)> {
        match self {
            Self::Directory(_) => None,
            Self::Paced(pool) if pool.simulated_images => None,
            Self::Paced(pool) => {
                // Include every read issued by this replay, even if its final frame precedes delivery.
                for entry in pool.pending.borrow().values().filter_map(Weak::upgrade) {
                    let _ = entry.wait();
                }
                let kind = match pool.client.origin {
                    Origin::Disk(_) => "disk",
                    Origin::Web(_) => "http",
                };
                Some((kind, pool.client.stats.lock().unwrap().clone()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        net::TcpListener,
        sync::Barrier,
    };

    #[test]
    fn joined_model_tickets_share_the_decoding_result_and_release_the_compressed_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = b"not a packed mesh";
        let sha256 = crate::bench::sha(bytes);
        let file = File { file: format!("{sha256}.model"), sha256, bytes: bytes.len() };
        fs::write(dir.path().join(&file.file), bytes).unwrap();
        let source = Source::open(dir.path().display().to_string(), None, WorkerDecode::default(), true).unwrap();
        let first = source.start(&file, false).ok().flatten().unwrap();
        let second = source.start(&file, false).ok().flatten().unwrap();
        for ticket in [&first, &second] {
            assert_eq!(ticket.encoded_len().unwrap(), bytes.len());
            assert!(ticket.model().err().unwrap().to_string().contains("invalid packed visual signature"));
            assert!(ticket.bytes().is_err());
        }
        let (_, io) = source.traffic().unwrap();
        assert_eq!((io.disk_content_reads, io.worker_model_decodes, io.joined_reads), (1, 1, 1));
    }

    #[test]
    fn readers_share_one_job_and_cancel_backlogged_content_without_blocking_the_tick() {
        let files = (0..13u8)
            .map(|i| {
                let sha256 = crate::bench::sha(&[i]);
                File { file: format!("{sha256}.vpage"), sha256, bytes: 1 }
            })
            .collect::<Vec<_>>();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let names = files.iter().map(|f| format!("/{}", f.file)).collect::<Vec<_>>();
        let release = Arc::new(Barrier::new(5));
        let gate = release.clone();
        let (accepted, seen) = mpsc::channel();
        let server = thread::spawn(move || {
            let mut tasks = Vec::new();
            for count in 0..5 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut req = BufReader::new(&stream);
                let mut header = String::new();
                req.read_line(&mut header).unwrap();
                let id = names
                    .iter()
                    .position(|name| Some(name.as_str()) == header.split_ascii_whitespace().nth(1))
                    .unwrap();
                loop {
                    header.clear();
                    if req.read_line(&mut header).unwrap() == 0 || header == "\r\n" {
                        break;
                    }
                }
                let gate = gate.clone();
                accepted.send(id).unwrap();
                tasks.push(thread::spawn(move || {
                    if count < 4 {
                        gate.wait();
                    }
                    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n").unwrap();
                    stream.write_all(&[id as u8]).unwrap();
                }));
            }
            for task in tasks {
                task.join().unwrap();
            }
        });
        let src = Source::open(format!("http://{address}"), None, WorkerDecode::default(), false).unwrap();
        let submit = |f: &File| src.start(f, false).ok().flatten().unwrap();
        let mut first = files[..4].iter().map(submit).collect::<Vec<_>>();
        for _ in 0..4 {
            seen.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        let same = submit(&files[0]);
        let cancelled = files[4..12].iter().map(submit).collect::<Vec<_>>();
        assert!(src.start(&files[12], false).is_err());
        drop(cancelled);
        release.wait();
        for (i, job) in first.drain(..).enumerate() {
            assert_eq!(job.bytes().unwrap(), &[i as u8]);
        }
        assert_eq!(same.bytes().unwrap(), &[0]);
        let extra = src.start(&files[12], true).ok().flatten().unwrap();
        assert_eq!(extra.bytes().unwrap(), &[12]);
        assert_eq!(seen.recv_timeout(Duration::from_secs(10)).unwrap(), 12);
        server.join().unwrap();
        let (kind, stats) = src.traffic().unwrap();
        assert_eq!(kind, "http");
        assert_eq!(
            (stats.content_requests, stats.content_body_bytes, stats.joined_reads, stats.deferred_full_queue),
            (5, 5, 1, 1)
        );
    }
}
