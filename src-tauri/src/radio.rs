//! Local streaming proxy for internet radio.
//!
//! The webview (WebView2/Chromium on Windows, WebKitGTK on Linux) can't read ICY
//! metadata, needs CORS to keep the Web Audio graph (EQ + visualizer) untainted,
//! and blocks plain-http streams as mixed content. So we run a tiny loopback HTTP
//! server: the `<audio>` element streams
//! from `http://127.0.0.1:<port>/radio?url=<upstream>`, and we fetch the
//! upstream server-side, strip the interleaved ICY metadata (emitting song
//! titles to the frontend via the `radio:meta` event), and forward clean audio
//! with `Access-Control-Allow-Origin: *`.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tiny_http::{Header, Response, Server, StatusCode};

static PROXY_PORT: AtomicU16 = AtomicU16::new(0);

/// The loopback port the radio proxy listens on (0 until started).
#[tauri::command]
pub fn radio_proxy_port() -> u16 {
    PROXY_PORT.load(Ordering::Relaxed)
}

/// Stream metadata pushed to the frontend. `url` lets the frontend ignore events
/// from a station it has already switched away from.
#[derive(Clone, serde::Serialize)]
struct RadioMeta {
    url: String,
    title: Option<String>,
    codec: Option<String>,
    bitrate: Option<u32>,
    name: Option<String>,
}

/// A connection failure pushed to the frontend so the UI can show why a station
/// didn't play (instead of a silent/never-loading state). `permanent` is true
/// for failures that won't fix themselves (bad URL / auth) so the frontend can
/// stop retrying; transient failures (network/server) keep retrying.
#[derive(Clone, serde::Serialize)]
struct RadioError {
    url: String,
    message: String,
    permanent: bool,
}

/// 4xx auth / not-found errors are permanent; everything else is worth retrying.
fn is_permanent(e: &ureq::Error) -> bool {
    matches!(e, ureq::Error::Status(401 | 403 | 404 | 410, _))
}

/// Turn a ureq error into a short, human-readable Indonesian message.
fn friendly_error(e: &ureq::Error) -> String {
    if let ureq::Error::Status(code, _) = e {
        return match code {
            401 | 403 => format!("Akses ditolak (HTTP {code}) — URL/token mungkin kedaluwarsa"),
            404 => "Stream tidak ditemukan (404)".into(),
            _ => format!("Server menolak (HTTP {code})"),
        };
    }
    "Gagal terhubung ke server".into()
}

/// Start the proxy server on a random loopback port in a background thread.
pub fn start(app: AppHandle) {
    let server = match Server::http("127.0.0.1:0") {
        Ok(s) => s,
        Err(e) => {
            crate::log_line(&format!("[ERROR] radio proxy bind failed: {e}"));
            return;
        }
    };
    if let Some(addr) = server.server_addr().to_ip() {
        PROXY_PORT.store(addr.port(), Ordering::Relaxed);
        crate::log_line(&format!("[INFO] radio proxy on 127.0.0.1:{}", addr.port()));
    }
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let app = app.clone();
            // One thread per stream: it blocks until the client (audio element)
            // disconnects, which kills the upstream fetch too.
            std::thread::spawn(move || handle(app, request));
        }
    });
}

fn handle(app: AppHandle, request: tiny_http::Request) {
    // Local-file route: webkit2gtk (Linux) can't play media from the custom
    // `asset://` scheme, so on Linux the frontend points the <audio> element at
    // `/file?path=<abs>` and we stream the file over loopback HTTP (with Range
    // support for seeking) — same trick as the radio proxy, CORS-clean for EQ.
    if request.url().starts_with("/file") {
        serve_file(request);
        return;
    }

    let stream_url = match url_param(request.url()) {
        Some(u) => u,
        None => {
            let _ = request.respond(Response::empty(StatusCode(400)));
            return;
        }
    };

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .build();
    let upstream = match agent
        .get(&stream_url)
        .set("Icy-MetaData", "1")
        // Many stream servers reject non-browser UAs with 401/403; the webview
        // is Chromium, so present a matching browser UA.
        .set(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
        .call()
    {
        Ok(r) => r,
        Err(e) => {
            crate::log_line(&format!("[ERROR] radio connect failed: {stream_url} | {e}"));
            let _ = app.emit(
                "radio:error",
                RadioError {
                    url: stream_url.clone(),
                    message: friendly_error(&e),
                    permanent: is_permanent(&e),
                },
            );
            let _ = request.respond(Response::empty(StatusCode(502)));
            return;
        }
    };

    let content_type = upstream
        .header("content-type")
        .unwrap_or("audio/mpeg")
        .to_string();
    let metaint: usize = upstream
        .header("icy-metaint")
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    let bitrate: Option<u32> = upstream
        .header("icy-br")
        .and_then(|v| v.split(',').next())
        .and_then(|v| v.trim().parse().ok());
    let name = upstream
        .header("icy-name")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let codec = codec_from_ct(&content_type);

    // Initial metadata: codec / bitrate / station name (title arrives later).
    let _ = app.emit(
        "radio:meta",
        RadioMeta {
            url: stream_url.clone(),
            title: None,
            codec,
            bitrate,
            name,
        },
    );

    let reader = IcyStripReader {
        inner: upstream.into_reader(),
        app,
        url: stream_url,
        metaint,
        until_meta: metaint,
        last_title: None,
    };

    let headers = vec![
        header("Content-Type", &content_type),
        header("Access-Control-Allow-Origin", "*"),
        header("Cache-Control", "no-cache, no-store"),
    ];

    // data_length = None → chunked transfer, which the audio element streams.
    let _ = request.respond(Response::new(StatusCode(200), headers, reader, None, None));
}

/// Stream a local audio file over loopback HTTP, honoring a `Range` request so
/// the <audio> element can seek. Loopback-only, so any readable path is served.
fn serve_file(request: tiny_http::Request) {
    let path = match query_param(request.url(), "path") {
        Some(p) => p,
        None => {
            let _ = request.respond(Response::empty(StatusCode(400)));
            return;
        }
    };

    let p = Path::new(&path);
    let mut file = match File::open(p) {
        Ok(f) => f,
        Err(_) => {
            let _ = request.respond(Response::empty(StatusCode(404)));
            return;
        }
    };
    let total: u64 = file.metadata().map(|m| m.len()).unwrap_or(0);
    let ctype = audio_mime(p);

    // Honor a byte-range request (audio seeking issues `Range: bytes=start-end`).
    let range = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Range"))
        .and_then(|h| parse_range(h.value.as_str(), total));

    match range {
        Some((start, end)) if start <= end && end < total => {
            let len = end - start + 1;
            if file.seek(SeekFrom::Start(start)).is_err() {
                let _ = request.respond(Response::empty(StatusCode(500)));
                return;
            }
            let headers = vec![
                header("Content-Type", ctype),
                header("Accept-Ranges", "bytes"),
                header("Access-Control-Allow-Origin", "*"),
                header("Content-Range", &format!("bytes {start}-{end}/{total}")),
            ];
            let reader = file.take(len);
            // Raise tiny_http's chunked threshold (default 32 KiB) so a known
            // length is always sent as Content-Length, not chunked — webkit's
            // media loader needs it to start playback promptly and to seek.
            let _ = request.respond(
                Response::new(StatusCode(206), headers, reader, Some(len as usize), None)
                    .with_chunked_threshold(usize::MAX),
            );
        }
        _ => {
            let headers = vec![
                header("Content-Type", ctype),
                header("Accept-Ranges", "bytes"),
                header("Access-Control-Allow-Origin", "*"),
            ];
            let _ = request.respond(
                Response::new(StatusCode(200), headers, file, Some(total as usize), None)
                    .with_chunked_threshold(usize::MAX),
            );
        }
    }
}

/// Parse a single-range `bytes=start-end` header into absolute byte offsets.
/// Open-ended forms (`bytes=start-`, `bytes=-suffix`) are resolved against the
/// total size. Returns None for multi-range or unparseable values.
fn parse_range(value: &str, total: u64) -> Option<(u64, u64)> {
    let spec = value.trim().strip_prefix("bytes=")?;
    if spec.contains(',') || total == 0 {
        return None;
    }
    let (a, b) = spec.split_once('-')?;
    let (a, b) = (a.trim(), b.trim());
    if a.is_empty() {
        // Suffix range: the last `b` bytes.
        let n: u64 = b.parse().ok()?;
        let n = n.min(total);
        return Some((total - n, total - 1));
    }
    let start: u64 = a.parse().ok()?;
    let end: u64 = if b.is_empty() {
        total - 1
    } else {
        b.parse::<u64>().ok()?.min(total - 1)
    };
    Some((start, end))
}

/// Map a file extension to an audio MIME type (best-effort; webkit also sniffs).
fn audio_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("mp3") => "audio/mpeg",
        Some("flac") => "audio/flac",
        Some("wav") => "audio/wav",
        Some("ogg" | "opus") => "audio/ogg",
        Some("m4a" | "aac" | "alac") => "audio/mp4",
        Some("aiff" | "aif") => "audio/aiff",
        Some("wma") => "audio/x-ms-wma",
        _ => "application/octet-stream",
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes())
        .unwrap_or_else(|_| Header::from_bytes(&b"X-Invalid"[..], &b"1"[..]).unwrap())
}

/// Wraps the upstream reader, stripping the interleaved ICY metadata blocks
/// (`icy-metaint` audio bytes, then a 1-byte length×16, then the metadata) and
/// emitting `StreamTitle` changes to the frontend.
struct IcyStripReader<R: Read> {
    inner: R,
    app: AppHandle,
    url: String,
    metaint: usize,
    until_meta: usize,
    last_title: Option<String>,
}

impl<R: Read> Read for IcyStripReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.metaint == 0 {
            return self.inner.read(buf); // server sends no interleaved metadata
        }
        if self.until_meta == 0 {
            let mut len = [0u8; 1];
            if !read_full(&mut self.inner, &mut len)? {
                return Ok(0);
            }
            let meta_len = len[0] as usize * 16;
            if meta_len > 0 {
                let mut meta = vec![0u8; meta_len];
                if !read_full(&mut self.inner, &mut meta)? {
                    return Ok(0);
                }
                if let Some(title) = parse_stream_title(&meta) {
                    if self.last_title.as_deref() != Some(title.as_str()) {
                        self.last_title = Some(title.clone());
                        let _ = self.app.emit(
                            "radio:meta",
                            RadioMeta {
                                url: self.url.clone(),
                                title: Some(title),
                                codec: None,
                                bitrate: None,
                                name: None,
                            },
                        );
                    }
                }
            }
            self.until_meta = self.metaint;
        }
        let want = buf.len().min(self.until_meta);
        let n = self.inner.read(&mut buf[..want])?;
        self.until_meta -= n;
        Ok(n)
    }
}

/// Fill `buf` completely; Ok(false) means a clean EOF before it was filled.
fn read_full<R: Read>(r: &mut R, buf: &mut [u8]) -> std::io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => return Ok(false),
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

fn parse_stream_title(meta: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(meta);
    let key = "StreamTitle='";
    let start = s.find(key)? + key.len();
    let rest = &s[start..];
    let end = rest.find("';").or_else(|| rest.find('\''))?;
    let title = rest[..end].trim().to_string();
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

fn codec_from_ct(ct: &str) -> Option<String> {
    let ct = ct.to_lowercase();
    if ct.contains("mpeg") || ct.contains("mp3") {
        Some("MP3".into())
    } else if ct.contains("aac") || ct.contains("mp4") {
        Some("AAC".into())
    } else if ct.contains("ogg") || ct.contains("opus") || ct.contains("vorbis") {
        Some("OGG".into())
    } else if ct.contains("flac") {
        Some("FLAC".into())
    } else {
        None
    }
}

fn url_param(path_and_query: &str) -> Option<String> {
    query_param(path_and_query, "url")
}

/// Extract and percent-decode the named query parameter from a request path.
fn query_param(path_and_query: &str, key: &str) -> Option<String> {
    let q = path_and_query.split_once('?')?.1;
    let prefix = format!("{key}=");
    for pair in q.split('&') {
        if let Some(v) = pair.strip_prefix(&prefix) {
            return Some(percent_decode(v));
        }
    }
    None
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                if let (Some(h), Some(l)) = (hex_val(b[i + 1]), hex_val(b[i + 2])) {
                    out.push(h * 16 + l);
                    i += 3;
                    continue;
                }
                out.push(b[i]);
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}
