//! `fake-bw serve`: HTTP/1.1 over an inherited file descriptor or a Unix socket.
//!
//! The transport is deliberately dumb: one connection, one request at a time,
//! process exits when the connection closes. That mirrors the real
//! `bw serve --hostname fd+connected://N` behaviour.

use crate::http::{self, Response as FakeResponse};
use crate::state::{self, State};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE, HOST, HeaderName, HeaderValue, ORIGIN};
use hyper::service::service_fn;
use hyper::{Request as HyperRequest, Response as HyperResponse, StatusCode};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::os::unix::io::FromRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::net::{UnixListener, UnixStream};

/// Runs `serve` with the arguments that followed the subcommand.
pub fn run(args: &[String]) -> i32 {
    let Some(hostname) = argument_value(args, "--hostname") else {
        eprintln!("Missing or invalid --hostname.");
        return 1;
    };
    let path = match state::state_path() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    let mut state = match State::load(&path) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    if !state.is_logged_in() {
        eprintln!("You are not logged in.");
        return 1;
    }
    state.record_invocation(args);
    if let Err(error) = state.save(&path) {
        eprintln!("{error}");
        return 1;
    }

    let argv = Arc::new(args.to_vec());
    match serve(&hostname, state, path, argv) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn argument_value(args: &[String], flag: &str) -> Option<String> {
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == flag {
            return args.get(index + 1).cloned();
        }
        if let Some(value) = argument.strip_prefix(&format!("{flag}=")) {
            return Some(value.to_owned());
        }
        index += 1;
    }
    None
}

fn serve(
    hostname: &str,
    state: State,
    path: PathBuf,
    argv: Arc<Vec<String>>,
) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("cannot start the runtime: {error}"))?;
    let hostname = hostname.to_owned();
    runtime.block_on(async move {
        let (stream, cleanup) = connect(&hostname).await?;
        let result = serve_connection(stream, state, path, argv).await;
        if let Some(path) = cleanup {
            let _ = std::fs::remove_file(path);
        }
        result
    })
}

/// Connects to the configured transport. Returns the stream and, for Unix
/// sockets, the path to unlink when the connection ends.
async fn connect(hostname: &str) -> Result<(UnixStream, Option<PathBuf>), String> {
    if let Some(fd) = hostname.strip_prefix("fd+connected://") {
        let fd: i32 = fd.parse().map_err(|_| format!("invalid file descriptor in {hostname}"))?;
        // SAFETY: the caller (lazybw) hands us a connected socket on this exact
        // descriptor and never uses it again; we take ownership of the fd.
        #[expect(
            unsafe_code,
            reason = "from_raw_fd is the only way to adopt the inherited descriptor"
        )]
        let std_stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) };
        std_stream
            .set_nonblocking(true)
            .map_err(|error| format!("cannot configure fd {fd}: {error}"))?;
        let stream = UnixStream::from_std(std_stream)
            .map_err(|error| format!("cannot use fd {fd}: {error}"))?;
        return Ok((stream, None));
    }
    if let Some(raw_path) = hostname.strip_prefix("unix://") {
        let path = PathBuf::from(raw_path);
        let _ = std::fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let listener = UnixListener::bind(&path)
            .map_err(|error| format!("cannot bind {}: {error}", path.display()))?;
        let (stream, _address) = listener
            .accept()
            .await
            .map_err(|error| format!("cannot accept on {}: {error}", path.display()))?;
        return Ok((stream, Some(path)));
    }
    Err(format!("unsupported --hostname {hostname}"))
}

async fn serve_connection(
    stream: UnixStream,
    state: State,
    path: PathBuf,
    argv: Arc<Vec<String>>,
) -> Result<(), String> {
    let state = Arc::new(Mutex::new(state));
    let path = Arc::new(path);
    let faults = Arc::new(Faults::from_env());
    let counter = Arc::new(AtomicUsize::new(0));

    let service = service_fn(move |request: HyperRequest<Incoming>| {
        let state = Arc::clone(&state);
        let path = Arc::clone(&path);
        let argv = Arc::clone(&argv);
        let faults = Arc::clone(&faults);
        let counter = Arc::clone(&counter);
        async move {
            let response = dispatch(state, path, argv, faults, counter, request).await;
            Ok::<_, Infallible>(response)
        }
    });

    hyper::server::conn::http1::Builder::new()
        .serve_connection(TokioIo::new(stream), service)
        .await
        .map_err(|error| format!("serve connection error: {error}"))
}

async fn dispatch(
    state: Arc<Mutex<State>>,
    path: Arc<PathBuf>,
    argv: Arc<Vec<String>>,
    faults: Arc<Faults>,
    counter: Arc<AtomicUsize>,
    request: HyperRequest<Incoming>,
) -> HyperResponse<Full<Bytes>> {
    let count = counter.fetch_add(1, Ordering::SeqCst) + 1;
    if faults.crash_after.is_some_and(|limit| count > limit) {
        std::process::exit(70);
    }

    let (parts, body) = request.into_parts();
    let collected = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => Bytes::new(),
    };
    let parsed_body =
        if collected.is_empty() { None } else { serde_json::from_slice(&collected).ok() };
    let method = parts.method.as_str().to_ascii_uppercase();
    let request_path = parts.uri.path().to_owned();
    let mut request = http::Request::new(&method, &request_path);
    request.query = parts.uri.query().unwrap_or_default().to_owned();
    request.host = header_string(&parts.headers, HOST);
    request.origin = header_string(&parts.headers, ORIGIN);
    request.body = parsed_body;

    let injected = faults.message_for(&endpoint_key(&request_path));
    let response = {
        let mut guard = lock_state(&state);
        // Pick up changes made by a short-lived subprocess (import, export)
        // between requests; the state file is the shared source of truth.
        if let Ok(fresh) = State::load(path.as_path()) {
            *guard = fresh;
        }
        let response = match injected {
            Some(message) => FakeResponse::error(&message),
            None => http::handle(&mut guard, &request),
        };
        guard.record_request(&argv, &method, &request_path);
        if let Err(error) = guard.save(path.as_path()) {
            eprintln!("fake-bw: cannot save state: {error}");
        }
        response
    };

    if !faults.latency.is_zero() {
        tokio::time::sleep(faults.latency).await;
    }
    if faults.crash_after == Some(count) {
        // Let hyper write the response first, then die like a real crash.
        tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            std::process::exit(70);
        });
    }
    to_hyper(response)
}

fn lock_state(state: &Mutex<State>) -> MutexGuard<'_, State> {
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn header_string(headers: &hyper::HeaderMap, name: HeaderName) -> Option<String> {
    headers.get(name).and_then(|value: &HeaderValue| value.to_str().ok()).map(str::to_owned)
}

fn to_hyper(response: FakeResponse) -> HyperResponse<Full<Bytes>> {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = Full::new(Bytes::from(response.body.clone()));
    match HyperResponse::builder()
        .status(status)
        .header(CONTENT_TYPE, http::CONTENT_TYPE)
        .header(CONTENT_LENGTH, response.body.len().to_string())
        .body(body)
    {
        Ok(response) => response,
        Err(_) => {
            let mut fallback = HyperResponse::new(Full::new(Bytes::from_static(
                b"{\"success\":false,\"message\":\"Internal error\",\"data\":null}",
            )));
            *fallback.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            fallback
        }
    }
}

/// The endpoint name used by `FAKE_BW_FAIL`.
fn endpoint_key(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    match segments.as_slice() {
        ["status"] => "status".to_owned(),
        ["unlock"] => "unlock".to_owned(),
        ["lock"] => "lock".to_owned(),
        ["sync"] => "sync".to_owned(),
        ["list", "object", "items"] => "list-items".to_owned(),
        ["list", "object", "folders"] => "list-folders".to_owned(),
        ["object", "item", ..] => "object-item".to_owned(),
        ["object", "folder", ..] => "object-folder".to_owned(),
        ["restore", "item", ..] => "restore-item".to_owned(),
        ["generate"] => "generate".to_owned(),
        _ => "unknown".to_owned(),
    }
}

/// Fault injection configured through the environment.
struct Faults {
    latency: Duration,
    fails: Vec<(String, String)>,
    crash_after: Option<usize>,
}

impl Faults {
    fn from_env() -> Self {
        let latency = std::env::var("FAKE_BW_LATENCY_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or_default();
        let fails =
            std::env::var("FAKE_BW_FAIL").ok().map(|raw| parse_fails(&raw)).unwrap_or_default();
        let crash_after =
            std::env::var("FAKE_BW_CRASH_AFTER").ok().and_then(|value| value.parse::<usize>().ok());
        Self { latency, fails, crash_after }
    }

    fn message_for(&self, endpoint: &str) -> Option<String> {
        self.fails
            .iter()
            .find(|(name, _message)| name == endpoint)
            .map(|(_name, message)| message.clone())
    }
}

fn parse_fails(raw: &str) -> Vec<(String, String)> {
    raw.split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return None;
            }
            match entry.split_once(':') {
                Some((endpoint, message)) => {
                    Some((endpoint.trim().to_owned(), message.trim().to_owned()))
                }
                None => Some((entry.to_owned(), "Injected failure.".to_owned())),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_keys_match_the_documented_fail_names() {
        assert_eq!(endpoint_key("/unlock"), "unlock");
        assert_eq!(endpoint_key("/sync"), "sync");
        assert_eq!(endpoint_key("/list/object/items"), "list-items");
        assert_eq!(endpoint_key("/object/item/abc"), "object-item");
        assert_eq!(endpoint_key("/object/folder/abc"), "object-folder");
        assert_eq!(endpoint_key("/restore/item/abc"), "restore-item");
        assert_eq!(endpoint_key("/generate"), "generate");
        assert_eq!(endpoint_key("/nope"), "unknown");
    }

    #[test]
    fn fail_specs_split_on_commas_and_first_colon() {
        let fails = parse_fails("unlock:Invalid master password.,sync,broken:not really: ok");
        assert_eq!(fails.len(), 3);
        assert_eq!(fails[0].0, "unlock");
        assert_eq!(fails[0].1, "Invalid master password.");
        assert_eq!(fails[1].0, "sync");
        assert_eq!(fails[1].1, "Injected failure.");
        assert_eq!(fails[2].0, "broken");
        assert_eq!(fails[2].1, "not really: ok");
    }

    #[test]
    fn hostname_argument_accepts_both_forms() {
        let args = vec!["--hostname".to_owned(), "unix:///tmp/x.sock".to_owned()];
        assert_eq!(argument_value(&args, "--hostname").as_deref(), Some("unix:///tmp/x.sock"));
        let args = vec!["--hostname=fd+connected://3".to_owned()];
        assert_eq!(argument_value(&args, "--hostname").as_deref(), Some("fd+connected://3"));
    }
}
