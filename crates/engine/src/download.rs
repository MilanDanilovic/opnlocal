//! Resumable, verified model downloads.
//!
//! - Data goes to `<dest>.part`; `<dest>.part.json` remembers what it belongs to.
//! - Interrupted downloads resume with an HTTP Range request (also after an app restart).
//! - The sha256 is computed while writing; the file is renamed into place only if it matches.
//! - Cancel deletes the partial file. Network hiccups are retried with backoff.
//!
//! Hugging Face redirects to a signed CDN URL that expires; every attempt starts again from the
//! pinned `resolve` URL, so a resume never uses a stale link.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

#[derive(Clone, Debug)]
pub struct DownloadRequest {
    pub url: String,
    pub dest: PathBuf,
    pub size: u64,
    pub sha256: String,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(tag = "stage", rename_all = "snake_case")]
#[ts(export)]
pub enum Progress {
    /// Re-reading a partial file from an earlier attempt to continue its checksum.
    CheckingPartial {
        #[ts(type = "number")]
        done: u64,
        #[ts(type = "number")]
        total: u64,
    },
    Downloading {
        #[ts(type = "number")]
        done: u64,
        #[ts(type = "number")]
        total: u64,
        /// Measured over the last few seconds; None until there's enough data.
        #[ts(type = "number | null")]
        bytes_per_second: Option<u64>,
    },
    /// The connection dropped; waiting before trying again.
    Retrying {
        attempt: u32,
        max_attempts: u32,
        wait_seconds: u32,
    },
}

#[derive(thiserror::Error, Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum DownloadError {
    #[error("download cancelled")]
    Cancelled,
    #[error("no internet connection")]
    Offline,
    #[error("network problem: {message}")]
    Network { message: String },
    #[error("not enough free space")]
    NotEnoughSpace {
        #[ts(type = "number")]
        need_bytes: u64,
        #[ts(type = "number")]
        free_bytes: u64,
    },
    #[error("the downloaded file is damaged (checksum mismatch)")]
    Corrupt,
    #[error("the server answered {status}")]
    Http { status: u16 },
    #[error("could not write the file: {message}")]
    Io { message: String },
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct PartMeta {
    url: String,
    sha256: String,
    size: u64,
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Wait before each retry; its length is the number of retries.
    pub retry_delays: Vec<Duration>,
    /// Minimum time between progress events.
    pub progress_interval: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            retry_delays: [2, 4, 8, 15, 30, 30].map(Duration::from_secs).to_vec(),
            progress_interval: Duration::from_millis(250),
        }
    }
}

pub fn part_path(dest: &Path) -> PathBuf {
    let mut p = dest.as_os_str().to_owned();
    p.push(".part");
    PathBuf::from(p)
}

fn meta_path(dest: &Path) -> PathBuf {
    let mut p = part_path(dest).into_os_string();
    p.push(".json");
    PathBuf::from(p)
}

/// Bytes of a partial download on disk for `dest` (0 if none).
pub fn partial_bytes(dest: &Path) -> u64 {
    std::fs::metadata(part_path(dest))
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Deletes a partial download and its metadata.
pub fn discard_partial(dest: &Path) {
    let _ = std::fs::remove_file(part_path(dest));
    let _ = std::fs::remove_file(meta_path(dest));
}

/// A client suitable for downloads: no overall timeout (files are GBs), but connections that
/// stall for 30 s are dropped and retried.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .user_agent(concat!("opnlocal/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("static client config")
}

pub async fn download(
    client: &reqwest::Client,
    req: &DownloadRequest,
    cancel: &CancellationToken,
    opts: &Options,
    on_progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<(), DownloadError> {
    let result = download_inner(client, req, cancel, opts, on_progress).await;
    // Cancel means "I don't want this": clean up. Corrupt data is useless too.
    if let Err(DownloadError::Cancelled | DownloadError::Corrupt) = &result {
        discard_partial(&req.dest);
    }
    result
}

async fn download_inner(
    client: &reqwest::Client,
    req: &DownloadRequest,
    cancel: &CancellationToken,
    opts: &Options,
    on_progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<(), DownloadError> {
    let part = part_path(&req.dest);
    let meta = PartMeta {
        url: req.url.clone(),
        sha256: req.sha256.to_lowercase(),
        size: req.size,
    };
    if let Some(dir) = req.dest.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(io_err)?;
    }

    // A partial file only counts if it belongs to this exact file and isn't oversized.
    let previous: Option<PartMeta> = std::fs::read(meta_path(&req.dest))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let mut have = partial_bytes(&req.dest);
    if previous.as_ref() != Some(&meta) || have > req.size {
        discard_partial(&req.dest);
        have = 0;
    }
    std::fs::write(meta_path(&req.dest), serde_json::to_vec(&meta).unwrap()).map_err(io_err)?;

    let dir = req.dest.parent().unwrap_or(Path::new("."));
    let free = fs4::available_space(dir).unwrap_or(u64::MAX);
    let need = req.size - have;
    if need > free {
        return Err(DownloadError::NotEnoughSpace {
            need_bytes: need,
            free_bytes: free,
        });
    }

    let mut hasher = Sha256::new();
    if have > 0 {
        hash_existing(&part, have, &mut hasher, cancel, on_progress).await?;
    }

    let mut attempt = 0u32;
    loop {
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        match fetch(client, req, &part, &mut have, &mut hasher, cancel, opts, on_progress).await {
            Ok(()) => break,
            Err(Attempt::Fatal(e)) => return Err(e),
            Err(Attempt::Retryable(e)) => {
                let Some(delay) = opts.retry_delays.get(attempt as usize) else {
                    return Err(e);
                };
                attempt += 1;
                on_progress(Progress::Retrying {
                    attempt,
                    max_attempts: opts.retry_delays.len() as u32,
                    wait_seconds: delay.as_secs() as u32,
                });
                tokio::select! {
                    _ = cancel.cancelled() => return Err(DownloadError::Cancelled),
                    _ = tokio::time::sleep(*delay) => {}
                }
            }
        }
    }

    if have != req.size {
        return Err(DownloadError::Corrupt);
    }
    let digest = hex::encode(hasher.finalize());
    if digest != meta.sha256 {
        return Err(DownloadError::Corrupt);
    }
    tokio::fs::rename(&part, &req.dest).await.map_err(io_err)?;
    let _ = std::fs::remove_file(meta_path(&req.dest));
    Ok(())
}

enum Attempt {
    Retryable(DownloadError),
    Fatal(DownloadError),
}

#[allow(clippy::too_many_arguments)]
async fn fetch(
    client: &reqwest::Client,
    req: &DownloadRequest,
    part: &Path,
    have: &mut u64,
    hasher: &mut Sha256,
    cancel: &CancellationToken,
    opts: &Options,
    on_progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<(), Attempt> {
    if *have == req.size {
        return Ok(());
    }
    let mut request = client.get(&req.url);
    if *have > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={}-", *have));
    }
    let response = tokio::select! {
        _ = cancel.cancelled() => return Err(Attempt::Fatal(DownloadError::Cancelled)),
        r = request.send() => r.map_err(|e| Attempt::Retryable(network_error(&e)))?,
    };

    let status = response.status().as_u16();
    let append = match status {
        206 => {
            // Only trust a partial response that starts exactly where we are.
            let start = response
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("bytes "))
                .and_then(|v| v.split('-').next())
                .and_then(|v| v.parse::<u64>().ok());
            if start != Some(*have) {
                return Err(Attempt::Retryable(DownloadError::Network {
                    message: "server resumed at the wrong position".into(),
                }));
            }
            true
        }
        // Server ignored the Range header: start over.
        200 => false,
        408 | 425 | 429 | 500..=599 => {
            return Err(Attempt::Retryable(DownloadError::Http { status }));
        }
        _ => return Err(Attempt::Fatal(DownloadError::Http { status })),
    };
    if !append {
        *have = 0;
        *hasher = Sha256::new();
    }

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(part)
        .await
        .map_err(|e| Attempt::Fatal(io_err(e)))?;

    let mut stream = response.bytes_stream();
    let mut speed = SpeedMeter::new();
    let mut last_event = Instant::now() - opts.progress_interval;
    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => {
                let _ = file.flush().await;
                return Err(Attempt::Fatal(DownloadError::Cancelled));
            }
            c = stream.next() => c,
        };
        let Some(chunk) = chunk else { break };
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                file.flush().await.map_err(|e| Attempt::Fatal(io_err(e)))?;
                return Err(Attempt::Retryable(network_error(&e)));
            }
        };
        if *have + chunk.len() as u64 > req.size {
            return Err(Attempt::Fatal(DownloadError::Corrupt));
        }
        file.write_all(&chunk)
            .await
            .map_err(|e| Attempt::Fatal(io_err(e)))?;
        hasher.update(&chunk);
        *have += chunk.len() as u64;
        speed.add(chunk.len() as u64);
        if last_event.elapsed() >= opts.progress_interval {
            last_event = Instant::now();
            on_progress(Progress::Downloading {
                done: *have,
                total: req.size,
                bytes_per_second: speed.rate(),
            });
        }
    }
    file.flush().await.map_err(|e| Attempt::Fatal(io_err(e)))?;
    on_progress(Progress::Downloading {
        done: *have,
        total: req.size,
        bytes_per_second: speed.rate(),
    });
    if *have < req.size {
        // Stream ended early without an error (proxy cut us off): resume.
        return Err(Attempt::Retryable(DownloadError::Network {
            message: "connection closed early".into(),
        }));
    }
    Ok(())
}

async fn hash_existing(
    part: &Path,
    len: u64,
    hasher: &mut Sha256,
    cancel: &CancellationToken,
    on_progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<(), DownloadError> {
    let mut f = tokio::fs::File::open(part).await.map_err(io_err)?;
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    let mut done = 0u64;
    while done < len {
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        let want = buf.len().min((len - done) as usize);
        let n = f.read(&mut buf[..want]).await.map_err(io_err)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        done += n as u64;
        on_progress(Progress::CheckingPartial { done, total: len });
    }
    Ok(())
}

fn io_err(e: std::io::Error) -> DownloadError {
    if e.kind() == std::io::ErrorKind::StorageFull {
        DownloadError::NotEnoughSpace {
            need_bytes: 0,
            free_bytes: 0,
        }
    } else {
        DownloadError::Io {
            message: e.to_string(),
        }
    }
}

fn network_error(e: &reqwest::Error) -> DownloadError {
    if e.is_connect() {
        DownloadError::Offline
    } else {
        DownloadError::Network {
            message: e.to_string(),
        }
    }
}

/// Bytes/second over a sliding window of the last few seconds.
struct SpeedMeter {
    samples: std::collections::VecDeque<(Instant, u64)>,
}

impl SpeedMeter {
    const WINDOW: Duration = Duration::from_secs(5);

    fn new() -> Self {
        SpeedMeter {
            samples: Default::default(),
        }
    }

    fn add(&mut self, bytes: u64) {
        let now = Instant::now();
        self.samples.push_back((now, bytes));
        while let Some((t, _)) = self.samples.front() {
            if now.duration_since(*t) > Self::WINDOW {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    fn rate(&self) -> Option<u64> {
        let (first, _) = self.samples.front()?;
        let elapsed = first.elapsed().as_secs_f64();
        if elapsed < 1.0 {
            return None;
        }
        let bytes: u64 = self.samples.iter().map(|(_, b)| b).sum();
        Some((bytes as f64 / elapsed) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::{IntoResponse, Redirect, Response};
    use axum::routing::get;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Server {
        data: Vec<u8>,
        /// Drop the connection after this many body bytes, once.
        fail_after: Mutex<Option<usize>>,
        ignore_range: AtomicBool,
        range_starts: Mutex<Vec<u64>>,
        /// Throttle: sleep this many ms between 64 KiB chunks.
        slow_ms: AtomicU64,
    }

    async fn file(State(s): State<Arc<Server>>, headers: HeaderMap) -> Response {
        let start = headers
            .get("range")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("bytes="))
            .and_then(|v| v.trim_end_matches('-').parse::<usize>().ok())
            .filter(|_| !s.ignore_range.load(Ordering::SeqCst));
        s.range_starts.lock().unwrap().push(start.unwrap_or(0) as u64);
        let from = start.unwrap_or(0);
        let body = s.data[from..].to_vec();
        let fail_after = s.fail_after.lock().unwrap().take();
        let slow = s.slow_ms.load(Ordering::SeqCst);
        let chunks: Vec<Vec<u8>> = body.chunks(64 * 1024).map(|c| c.to_vec()).collect();
        let stream = futures_util::stream::unfold((chunks, 0usize), move |(mut chunks, sent)| async move {
            if chunks.is_empty() || sent == usize::MAX {
                return None;
            }
            if slow > 0 {
                tokio::time::sleep(Duration::from_millis(slow)).await;
            }
            if let Some(limit) = fail_after
                && sent >= limit
            {
                return Some((Err(std::io::Error::other("dropped")), (vec![], usize::MAX)));
            }
            let c = chunks.remove(0);
            let n = c.len();
            Some((Ok::<_, std::io::Error>(c), (chunks, sent + n)))
        });
        let mut resp = Body::from_stream(stream).into_response();
        if let Some(from) = start {
            *resp.status_mut() = StatusCode::PARTIAL_CONTENT;
            resp.headers_mut().insert(
                "content-range",
                format!("bytes {}-{}/{}", from, s.data.len() - 1, s.data.len())
                    .parse()
                    .unwrap(),
            );
        }
        resp
    }

    async fn serve(server: Arc<Server>) -> String {
        let app = axum::Router::new()
            .route("/file", get(file))
            .route("/redirect", get(|| async { Redirect::temporary("/file") }))
            .route("/missing", get(|| async { StatusCode::NOT_FOUND }))
            .with_state(server);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn data(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 31 % 251) as u8).collect()
    }

    fn sha(d: &[u8]) -> String {
        hex::encode(Sha256::digest(d))
    }

    fn fast() -> Options {
        Options {
            retry_delays: vec![Duration::from_millis(10); 3],
            progress_interval: Duration::from_millis(0),
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        req: DownloadRequest,
        server: Arc<Server>,
    }

    async fn fixture(len: usize, path: &str) -> Fixture {
        let bytes = data(len);
        let server = Arc::new(Server {
            data: bytes.clone(),
            ..Default::default()
        });
        let base = serve(server.clone()).await;
        let dir = tempfile::tempdir().unwrap();
        let req = DownloadRequest {
            url: format!("{base}{path}"),
            dest: dir.path().join("models").join("m.gguf"),
            size: bytes.len() as u64,
            sha256: sha(&bytes),
        };
        Fixture {
            _dir: dir,
            req,
            server,
        }
    }

    async fn run(f: &Fixture, cancel: &CancellationToken) -> Result<(), DownloadError> {
        download(&client(), &f.req, cancel, &fast(), &|_| {}).await
    }

    #[tokio::test]
    async fn downloads_and_verifies() {
        let f = fixture(1_000_000, "/file").await;
        run(&f, &CancellationToken::new()).await.unwrap();
        assert_eq!(std::fs::read(&f.req.dest).unwrap(), f.server.data);
        assert!(!part_path(&f.req.dest).exists());
        assert!(!meta_path(&f.req.dest).exists());
    }

    #[tokio::test]
    async fn resumes_after_connection_drop() {
        let f = fixture(1_000_000, "/file").await;
        *f.server.fail_after.lock().unwrap() = Some(400_000);
        run(&f, &CancellationToken::new()).await.unwrap();
        assert_eq!(std::fs::read(&f.req.dest).unwrap(), f.server.data);
        let starts = f.server.range_starts.lock().unwrap().clone();
        assert_eq!(starts.len(), 2);
        assert!(starts[1] >= 400_000, "second request resumes: {starts:?}");
    }

    #[tokio::test]
    async fn resumes_partial_file_from_earlier_session() {
        let f = fixture(1_000_000, "/file").await;
        std::fs::create_dir_all(f.req.dest.parent().unwrap()).unwrap();
        std::fs::write(part_path(&f.req.dest), &f.server.data[..300_000]).unwrap();
        let meta = PartMeta {
            url: f.req.url.clone(),
            sha256: f.req.sha256.clone(),
            size: f.req.size,
        };
        std::fs::write(meta_path(&f.req.dest), serde_json::to_vec(&meta).unwrap()).unwrap();
        let events = Mutex::new(vec![]);
        download(&client(), &f.req, &CancellationToken::new(), &fast(), &|p| {
            events.lock().unwrap().push(p)
        })
        .await
        .unwrap();
        assert_eq!(std::fs::read(&f.req.dest).unwrap(), f.server.data);
        assert_eq!(*f.server.range_starts.lock().unwrap(), vec![300_000]);
        assert!(matches!(
            events.lock().unwrap()[0],
            Progress::CheckingPartial { .. }
        ));
    }

    #[tokio::test]
    async fn partial_from_a_different_file_is_discarded() {
        let f = fixture(200_000, "/file").await;
        std::fs::create_dir_all(f.req.dest.parent().unwrap()).unwrap();
        std::fs::write(part_path(&f.req.dest), vec![0u8; 100_000]).unwrap();
        run(&f, &CancellationToken::new()).await.unwrap();
        assert_eq!(*f.server.range_starts.lock().unwrap(), vec![0]);
        assert_eq!(std::fs::read(&f.req.dest).unwrap(), f.server.data);
    }

    #[tokio::test]
    async fn checksum_mismatch_is_rejected_and_cleaned_up() {
        let mut f = fixture(100_000, "/file").await;
        f.req.sha256 = "0".repeat(64);
        assert_eq!(
            run(&f, &CancellationToken::new()).await,
            Err(DownloadError::Corrupt)
        );
        assert!(!f.req.dest.exists());
        assert!(!part_path(&f.req.dest).exists());
    }

    #[tokio::test]
    async fn cancel_stops_and_deletes_partial() {
        let f = fixture(2_000_000, "/file").await;
        f.server.slow_ms.store(20, Ordering::SeqCst);
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            c2.cancel();
        });
        assert_eq!(run(&f, &cancel).await, Err(DownloadError::Cancelled));
        assert!(!part_path(&f.req.dest).exists());
        assert!(!f.req.dest.exists());
    }

    #[tokio::test]
    async fn restarts_cleanly_when_server_ignores_range() {
        let f = fixture(500_000, "/file").await;
        *f.server.fail_after.lock().unwrap() = Some(200_000);
        f.server.ignore_range.store(true, Ordering::SeqCst);
        run(&f, &CancellationToken::new()).await.unwrap();
        assert_eq!(std::fs::read(&f.req.dest).unwrap(), f.server.data);
    }

    #[tokio::test]
    async fn follows_redirects() {
        let f = fixture(100_000, "/redirect").await;
        run(&f, &CancellationToken::new()).await.unwrap();
        assert_eq!(std::fs::read(&f.req.dest).unwrap(), f.server.data);
    }

    #[tokio::test]
    async fn not_found_fails_without_retrying() {
        let f = fixture(100, "/missing").await;
        assert_eq!(
            run(&f, &CancellationToken::new()).await,
            Err(DownloadError::Http { status: 404 })
        );
    }

    #[tokio::test]
    async fn unreachable_server_reports_offline_after_retries() {
        let dir = tempfile::tempdir().unwrap();
        let req = DownloadRequest {
            // Port 9 (discard) on localhost: connection refused.
            url: "http://127.0.0.1:9/file".into(),
            dest: dir.path().join("m.gguf"),
            size: 10,
            sha256: "0".repeat(64),
        };
        let retries = Mutex::new(0);
        let r = download(&client(), &req, &CancellationToken::new(), &fast(), &|p| {
            if matches!(p, Progress::Retrying { .. }) {
                *retries.lock().unwrap() += 1;
            }
        })
        .await;
        assert_eq!(r, Err(DownloadError::Offline));
        assert_eq!(*retries.lock().unwrap(), 3);
    }
}
