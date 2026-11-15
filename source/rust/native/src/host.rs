use crate::{data::*, refresh};
use anyhow::{Result, anyhow, bail};
use index_core::*;
use std::{
    fs,
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

struct RefreshState {
    running: bool,
    result: Option<std::result::Result<Value, String>>,
}
struct State {
    store: Store,
    build: String,
    key: String,
    port: u16,
    desktop: bool,
    refresh: Mutex<RefreshState>,
    ready: Condvar,
    sessions: AtomicUsize,
    idle: Mutex<Option<Instant>>,
}
fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}
fn value<'a>(request: &'a Request, name: &'static str) -> &'a str {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str())
        .unwrap_or("")
}
fn json_response(request: Request, status: u16, body: Value) {
    let response = Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
        .with_header(header("Cache-Control", "no-store"));
    let _ = request.respond(response);
}
fn error(request: Request, status: u16, message: impl std::fmt::Display) {
    json_response(request, status, json!({"error":message.to_string()}));
}
impl State {
    fn wait_refresh(&self) {
        let mut current = self.refresh.lock().unwrap();
        while current.running && !self.store.stopping.load(Ordering::Acquire) {
            current = self
                .ready
                .wait_timeout(current, Duration::from_millis(200))
                .unwrap()
                .0;
        }
    }
    fn refresh(&self) -> Result<Value> {
        let mut current = self.refresh.lock().unwrap();
        if current.running {
            while current.running {
                current = self.ready.wait(current).unwrap();
            }
            return current
                .result
                .clone()
                .unwrap_or_else(|| Err("The refresh was interrupted.".into()))
                .map_err(|e| anyhow!(e));
        }
        current.running = true;
        drop(current);
        let result = refresh::sync(&self.store, &Remote::new(self.store.stopping.clone()))
            .map_err(|e| e.to_string());
        let mut current = self.refresh.lock().unwrap();
        current.running = false;
        current.result = Some(result.clone());
        self.ready.notify_all();
        result.map_err(|e| anyhow!(e))
    }
    fn disconnect(&self) {
        if self.sessions.fetch_sub(1, Ordering::AcqRel) == 1 {
            *self.idle.lock().unwrap() = Some(Instant::now() + Duration::from_millis(1500));
        }
    }
}
fn session(request: Request, state: Arc<State>) {
    if !state.desktop {
        json_response(request, 204, Value::Null);
        return;
    }
    let key = value(&request, "Sec-WebSocket-Key").to_string();
    if key.is_empty() || !value(&request, "Upgrade").eq_ignore_ascii_case("websocket") {
        error(request, 426, "Open the app using its launcher.");
        return;
    }
    state.sessions.fetch_add(1, Ordering::AcqRel);
    *state.idle.lock().unwrap() = None;
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    let response =
        Response::empty(StatusCode(101)).with_header(header("Sec-WebSocket-Accept", &accept));
    let stream = request.upgrade("websocket", response);
    let mut socket =
        tungstenite::WebSocket::from_raw_socket(stream, tungstenite::protocol::Role::Server, None);
    while let Ok(message) = socket.read() {
        if message.is_close() {
            let _ = socket.close(None);
            break;
        }
        if state.store.stopping.load(Ordering::Acquire) {
            break;
        }
    }
    state.disconnect();
}
fn serve_file(request: Request, file: &Path, download: bool, cache: bool) -> Result<()> {
    let extension = file.extension().and_then(|v| v.to_str()).unwrap_or("");
    let mime = match extension {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    };
    let gzip = PathBuf::from(format!("{}.gz", file.display()));
    let mut bytes = if file.is_file() {
        fs::read(file)?
    } else if gzip.is_file() {
        let bytes = fs::read(gzip)?;
        if value(&request, "Accept-Encoding")
            .split(',')
            .any(|v| v.trim().starts_with("gzip"))
        {
            let mut response = Response::from_data(bytes)
                .with_header(header("Content-Encoding", "gzip"))
                .with_header(header("Content-Type", mime))
                .with_header(header("Vary", "Accept-Encoding"))
                .with_header(header(
                    "Cache-Control",
                    if cache {
                        "public, max-age=31536000, immutable"
                    } else {
                        "no-store"
                    },
                ));
            if download {
                response.add_header(header(
                    "Content-Disposition",
                    "attachment; filename=artificial-analysis-full-precision.json",
                ));
            }
            let _ = request.respond(response);
            return Ok(());
        }
        let mut output = Vec::new();
        flate2::read::GzDecoder::new(bytes.as_slice()).read_to_end(&mut output)?;
        output
    } else {
        let _ = request.respond(Response::empty(404));
        return Ok(());
    };
    let length = bytes.len();
    if *request.method() == Method::Head {
        bytes.clear();
    }
    let mut response = Response::new(
        StatusCode(200),
        vec![
            header("Content-Type", mime),
            header(
                "Cache-Control",
                if cache {
                    "public, max-age=31536000, immutable"
                } else {
                    "no-store"
                },
            ),
        ],
        Cursor::new(bytes),
        Some(length),
        None,
    );
    if download {
        response.add_header(header(
            "Content-Disposition",
            "attachment; filename=artificial-analysis-full-precision.json",
        ));
    }
    let _ = request.respond(response);
    Ok(())
}
fn handle(mut request: Request, state: Arc<State>) {
    let path = request.url().split('?').next().unwrap_or("/").to_string();
    let method = request.method().clone();
    let origin = value(&request, "Origin");
    if path.starts_with("/api/")
        && !origin.is_empty()
        && origin != format!("http://127.0.0.1:{}", state.port)
        && origin != format!("http://localhost:{}", state.port)
    {
        error(
            request,
            403,
            "Open the app on its local address to refresh data.",
        );
        return;
    }
    if path == "/api/app" && method == Method::Get {
        json_response(
            request,
            200,
            json!({"app":"intelligence-index-local","project":state.store.root.to_string_lossy(),"build":state.build,"mode":"production","desktop":state.desktop,"runtime":"rust","ready":!state.store.stopping.load(Ordering::Acquire),"pid":std::process::id()}),
        );
        return;
    }
    if path == "/api/desktop/session" && method == Method::Get {
        session(request, state);
        return;
    }
    if path == "/api/desktop/restart" && method == Method::Post {
        let supplied = value(&request, "x-intelligence-index-key");
        let valid = supplied.len() == state.key.len()
            && supplied
                .as_bytes()
                .iter()
                .zip(state.key.as_bytes())
                .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
                == 0;
        if !origin.is_empty() || !valid {
            error(request, 403, "The restart request is not authorized.");
            return;
        }
        json_response(request, 202, json!({"restarting":true}));
        state.store.stopping.store(true, Ordering::Release);
        state.ready.notify_all();
        return;
    }
    if path == "/api/refresh" && method == Method::Post {
        match state.refresh() {
            Ok(result) => json_response(request, 200, result),
            Err(e) => error(request, 502, e),
        }
        return;
    }
    if path == "/api/selection" && (method == Method::Get || method == Method::Put) {
        let incoming = if method == Method::Put {
            let mut body = String::new();
            if request
                .as_reader()
                .take(128 * 1024 + 1)
                .read_to_string(&mut body)
                .is_err()
                || body.len() > 128 * 1024
            {
                error(request, 413, "The model selection is too large.");
                return;
            }
            let body: Value = match serde_json::from_str(&body) {
                Ok(value) => value,
                Err(e) => {
                    error(request, 400, e);
                    return;
                }
            };
            if !body["selected"].is_array()
                || array(&body["selected"]).iter().any(|v| !v.is_string())
            {
                error(
                    request,
                    400,
                    "Choose models from the model picker to save a selection.",
                );
                return;
            }
            Some(body)
        } else {
            None
        };
        state.wait_refresh();
        let result = state.store.selection(
            incoming.as_ref().map(|v| &v["selected"]),
            incoming.as_ref().is_some_and(|v| b(v, "initialize")),
        );
        match result {
            Ok(value) => json_response(request, 200, value),
            Err(e) => error(request, 503, e),
        }
        return;
    }
    if ["/api/data", "/models.json", "/api/raw"].contains(&path.as_str()) && method == Method::Get {
        let _guard = state.store.gate.lock().unwrap();
        let file = state.store.root.join(if path == "/api/raw" {
            "data/public-models.raw.json"
        } else {
            "public/models.json"
        });
        let _ = serve_file(request, &file, path == "/api/raw", false);
        return;
    }
    if path.starts_with("/api/") {
        error(
            request,
            404,
            "The requested application endpoint was not found.",
        );
        return;
    }
    if method != Method::Get && method != Method::Head {
        let _ = request.respond(Response::empty(405));
        return;
    }
    let decoded = match percent_encoding::percent_decode_str(&path).decode_utf8() {
        Ok(path) => path,
        Err(_) => {
            error(request, 403, "The requested path is invalid.");
            return;
        }
    };
    let relative = Path::new(decoded.trim_start_matches('/'));
    if relative
        .components()
        .any(|v| !matches!(v, Component::Normal(_) | Component::CurDir))
    {
        error(
            request,
            403,
            "The requested path is outside the application.",
        );
        return;
    }
    let logos = decoded.starts_with("/logos/");
    let folder = state.store.root.join(if logos { "public" } else { "dist" });
    let mut file = folder.join(relative);
    if decoded == "/"
        || !logos
            && !file.is_file()
            && !PathBuf::from(format!("{}.gz", file.display())).is_file()
            && file.extension().is_none()
    {
        file = folder.join("index.html");
    }
    let cache = !logos && decoded.starts_with("/assets/");
    let _ = serve_file(request, &file, false, cache);
}
pub fn serve(root: PathBuf, desktop: bool, port: u16) -> Result<()> {
    if !root.join("dist/index.html").is_file()
        || (!root.join("public/models.json").is_file()
            && !root.join("public/models.json.gz").is_file())
    {
        bail!("Keep the complete Intelligence Index folder together.");
    }
    let stopping = Arc::new(AtomicBool::new(false));
    let store = Store::new(root.clone(), stopping.clone());
    let build = fs::read_to_string(root.join("desktop/build-version.txt"))?
        .trim()
        .to_string();
    let key = random_hex()?;
    let server = Server::http(("127.0.0.1", port)).map_err(|e| anyhow!(e.to_string()))?;
    fs::create_dir_all(root.join("data"))?;
    let key_file = root.join("data/server.key");
    fs::write(&key_file, &key)?;
    let state = Arc::new(State {
        store,
        build,
        key,
        port,
        desktop,
        refresh: Mutex::new(RefreshState {
            running: false,
            result: None,
        }),
        ready: Condvar::new(),
        sessions: AtomicUsize::new(0),
        idle: Mutex::new(desktop.then(|| Instant::now() + Duration::from_secs(60))),
    });
    while !stopping.load(Ordering::Acquire) {
        if desktop
            && state.sessions.load(Ordering::Acquire) == 0
            && state
                .idle
                .lock()
                .unwrap()
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            stopping.store(true, Ordering::Release);
            break;
        }
        match server.recv_timeout(Duration::from_millis(100)) {
            Ok(Some(request)) => {
                let state = state.clone();
                let _ = thread::Builder::new()
                    .name("app-request".into())
                    .spawn(move || handle(request, state));
            }
            Ok(None) => {}
            Err(e) => return Err(e.into()),
        }
    }
    state.ready.notify_all();
    let _guard = state.store.gate.lock().unwrap();
    let _ = fs::remove_file(key_file);
    Ok(())
}
