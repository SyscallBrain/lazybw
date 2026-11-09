//! The HTTP contract served by `fake-bw serve`.
//!
//! [`handle`] is a pure request handler (state in, status and JSON body out) so
//! the whole endpoint surface can be tested without binding a socket.

use crate::state::{self, State};
use rand::seq::SliceRandom;
use rand::{Rng, RngExt};
use serde_json::{Value, json};

/// A request as seen by [`handle`].
#[derive(Debug, Clone, Default)]
pub struct Request {
    /// HTTP method, upper case.
    pub method: String,
    /// Path without the query string.
    pub path: String,
    /// Raw query string, without the leading `?`.
    pub query: String,
    /// The `Host` header, when present.
    pub host: Option<String>,
    /// The `Origin` header, when present.
    pub origin: Option<String>,
    /// Parsed JSON body, when present.
    pub body: Option<Value>,
}

impl Request {
    /// Creates a request with a method and a path.
    #[must_use]
    pub fn new(method: &str, path: &str) -> Self {
        Self { method: method.to_ascii_uppercase(), path: path.to_owned(), ..Self::default() }
    }

    /// Sets the parsed JSON body.
    #[cfg(test)]
    #[must_use]
    pub fn with_body(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }

    /// Sets the query string.
    #[cfg(test)]
    #[must_use]
    pub fn with_query(mut self, query: &str) -> Self {
        self.query = query.to_owned();
        self
    }

    /// Sets the `Host` header.
    #[cfg(test)]
    #[must_use]
    pub fn with_host(mut self, host: &str) -> Self {
        self.host = Some(host.to_owned());
        self
    }

    /// Sets the `Origin` header.
    #[cfg(test)]
    #[must_use]
    pub fn with_origin(mut self, origin: &str) -> Self {
        self.origin = Some(origin.to_owned());
        self
    }
}

/// A response: HTTP status plus a JSON body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// HTTP status code.
    pub status: u16,
    /// Serialised JSON envelope.
    pub body: String,
}

impl Response {
    /// A successful envelope (`200`).
    #[must_use]
    pub fn ok(data: Value) -> Self {
        Self { status: 200, body: envelope_text(json!({"success": true, "data": data})) }
    }

    /// A failed envelope with the given HTTP status and message.
    #[must_use]
    pub fn fail(status: u16, message: &str) -> Self {
        Self {
            status,
            body: envelope_text(json!({"success": false, "message": message, "data": null})),
        }
    }

    /// A failure with the conventional `400 Bad Request` status.
    #[must_use]
    pub fn error(message: &str) -> Self {
        Self::fail(400, message)
    }

    /// The `403` answer used when the Host/Origin check fails.
    #[must_use]
    pub fn forbidden() -> Self {
        Self::fail(403, "Forbidden")
    }

    /// The `404` answer for unknown endpoints.
    #[must_use]
    pub fn not_found() -> Self {
        Self::fail(404, "Not found.")
    }
}

fn envelope_text(value: Value) -> String {
    serde_json::to_string(&value).unwrap_or_default()
}

/// RuntimeException-safe `Content-Type` value used by the server.
pub const CONTENT_TYPE: &str = "application/json";

/// Handles one request against the current state.
pub fn handle(state: &mut State, request: &Request) -> Response {
    if request.origin.is_some() {
        return Response::forbidden();
    }
    if let Some(host) = &request.host
        && !host_allowed(host)
    {
        return Response::forbidden();
    }

    let segments = path_segments(&request.path);
    match (request.method.as_str(), segments.as_slice()) {
        ("GET", ["status"]) => status_response(state),
        ("POST", ["unlock"]) => unlock(state, request),
        ("POST", ["lock"]) => lock(state),
        ("POST", ["sync"]) => with_unlocked(state, sync_response),
        ("GET", ["list", "object", "items"]) => {
            with_unlocked(state, |state| list_items(state, &request.query))
        }
        ("GET", ["list", "object", "folders"]) => with_unlocked(state, |state| list_folders(state)),
        ("POST", ["object", "item"]) => with_unlocked(state, |state| create_item(state, request)),
        ("GET", ["object", "item", id]) => with_unlocked(state, |state| get_item(state, id)),
        ("PUT", ["object", "item", id]) => {
            with_unlocked(state, |state| update_item(state, id, request))
        }
        ("DELETE", ["object", "item", id]) => {
            with_unlocked(state, |state| delete_item(state, id, &request.query))
        }
        ("POST", ["restore", "item", id]) => with_unlocked(state, |state| restore_item(state, id)),
        ("POST", ["object", "folder"]) => {
            with_unlocked(state, |state| create_folder(state, request))
        }
        ("PUT", ["object", "folder", id]) => {
            with_unlocked(state, |state| update_folder(state, id, request))
        }
        ("DELETE", ["object", "folder", id]) => {
            with_unlocked(state, |state| delete_folder(state, id))
        }
        ("GET", ["generate"]) => with_unlocked(state, |_state| generate_response(&request.query)),
        _ => Response::not_found(),
    }
}

fn with_unlocked(state: &mut State, handler: impl FnOnce(&mut State) -> Response) -> Response {
    if !state.is_unlocked() {
        return Response::error("Vault is locked.");
    }
    handler(state)
}

fn status_response(state: &State) -> Response {
    Response::ok(json!({
        "object": "template",
        "template": {
            "serverUrl": state.server_url,
            "lastSync": state.last_sync,
            "userEmail": state.email,
            "userId": state.user_id,
            "status": state.status,
        },
    }))
}

fn unlock(state: &mut State, request: &Request) -> Response {
    let password = request
        .body
        .as_ref()
        .and_then(|body| body.get("password"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if password != state.master_password {
        // What bw 2026.10.1 answers: the key derived from a wrong password
        // cannot decrypt the account key. Older CLIs said "Invalid master
        // password."; lazybw reads both as a wrong password.
        return Response::error("Cryptography error, The decryption operation failed");
    }
    state.status = state::STATUS_UNLOCKED.to_owned();
    Response::ok(json!({
        "object": "message",
        "title": "Unlocked",
        "message": "Your vault is now unlocked!",
        "raw": state.session_key(),
    }))
}

fn lock(state: &mut State) -> Response {
    state.status = state::STATUS_LOCKED.to_owned();
    Response::ok(json!({
        "object": "message",
        "title": "Locked",
        "message": "Your vault has been locked!",
        "raw": Value::Null,
    }))
}

fn sync_response(state: &mut State) -> Response {
    state.last_sync = Some(state::now_iso());
    Response::ok(json!({
        "object": "message",
        "title": "Syncing complete.",
        "message": "Syncing complete.",
        "raw": Value::Null,
    }))
}

fn list_items(state: &State, query: &str) -> Response {
    let archived = query_bool(query, "archived");
    let trash = query_bool(query, "trash");
    let items: Vec<Value> = state
        .items
        .iter()
        .filter(|item| {
            let deleted = !item.get("deletedDate").is_none_or(Value::is_null);
            let is_archived = !item.get("archivedDate").is_none_or(Value::is_null);
            if trash {
                deleted
            } else if archived {
                is_archived && !deleted
            } else {
                !deleted && !is_archived
            }
        })
        .map(with_item_object)
        .collect();
    Response::ok(json!({"object": "list", "data": items}))
}

fn list_folders(state: &State) -> Response {
    let mut folders: Vec<Value> = state.folders.iter().map(with_folder_object).collect();
    folders.push(json!({"object": "folder", "id": null, "name": "No Folder"}));
    Response::ok(json!({"object": "list", "data": folders}))
}

fn get_item(state: &State, id: &str) -> Response {
    match find_item(state, id) {
        Some(item) => Response::ok(with_item_object(item)),
        None => Response::error("Not found."),
    }
}

fn create_item(state: &mut State, request: &Request) -> Response {
    let Some(mut item) = request.body.clone().filter(Value::is_object) else {
        return Response::error("Invalid item.");
    };
    item["id"] = json!(state::new_id());
    item["object"] = json!("item");
    let now = state::now_iso();
    item["creationDate"] = json!(now);
    item["revisionDate"] = json!(now);
    item["deletedDate"] = Value::Null;
    item["archivedDate"] = Value::Null;
    if item.get("favorite").is_none() {
        item["favorite"] = json!(false);
    }
    if item.get("reprompt").is_none() {
        item["reprompt"] = json!(0);
    }
    if item.get("fields").is_none() {
        item["fields"] = json!([]);
    }
    if item.get("folderId").is_none() {
        item["folderId"] = Value::Null;
    }
    if item.get("passwordHistory").is_none() {
        item["passwordHistory"] = Value::Null;
    }
    state.items.push(item.clone());
    Response::ok(item)
}

fn update_item(state: &mut State, id: &str, request: &Request) -> Response {
    let Some(mut updated) = request.body.clone().filter(Value::is_object) else {
        return Response::error("Invalid item.");
    };
    let Some(index) = state.items.iter().position(|item| item_id(item) == Some(id)) else {
        return Response::error("Not found.");
    };
    let old = state.items[index].clone();

    let old_password = nested_str(&old, "login", "password");
    let new_password = nested_str(&updated, "login", "password");
    let mut history = password_history(&updated, &old);
    if let Some(password) = old_password
        && new_password != Some(password)
    {
        history.push(json!({
            "lastUsedDate": old
                .get("revisionDate")
                .cloned()
                .unwrap_or_else(|| json!(state::now_iso())),
            "password": password,
        }));
    }
    cap_history(&mut history);
    updated["passwordHistory"] = Value::Array(history);

    updated["id"] = json!(id);
    updated["object"] = json!("item");
    if updated.get("creationDate").is_none_or(Value::is_null) {
        updated["creationDate"] =
            old.get("creationDate").cloned().unwrap_or_else(|| json!(state::now_iso()));
    }
    updated["revisionDate"] = json!(state::now_iso());
    if updated.get("deletedDate").is_none() {
        updated["deletedDate"] = Value::Null;
    }
    if updated.get("archivedDate").is_none() {
        updated["archivedDate"] = Value::Null;
    }

    state.items[index] = updated.clone();
    Response::ok(updated)
}

fn delete_item(state: &mut State, id: &str, query: &str) -> Response {
    let Some(index) = state.items.iter().position(|item| item_id(item) == Some(id)) else {
        return Response::error("Not found.");
    };
    if query_bool(query, "permanent") {
        state.items.remove(index);
    } else {
        state.items[index]["deletedDate"] = json!(state::now_iso());
        state.items[index]["revisionDate"] = json!(state::now_iso());
    }
    Response::ok(Value::Null)
}

fn restore_item(state: &mut State, id: &str) -> Response {
    let Some(index) = state.items.iter().position(|item| item_id(item) == Some(id)) else {
        return Response::error("Not found.");
    };
    state.items[index]["deletedDate"] = Value::Null;
    state.items[index]["revisionDate"] = json!(state::now_iso());
    Response::ok(Value::Null)
}

fn create_folder(state: &mut State, request: &Request) -> Response {
    let Some(name) =
        request.body.as_ref().and_then(|body| body.get("name")).and_then(Value::as_str)
    else {
        return Response::error("Invalid folder.");
    };
    let folder = json!({"object": "folder", "id": state::new_id(), "name": name});
    state.folders.push(folder.clone());
    Response::ok(folder)
}

fn update_folder(state: &mut State, id: &str, request: &Request) -> Response {
    let Some(index) = state.folders.iter().position(|folder| folder_id(folder) == Some(id)) else {
        return Response::error("Not found.");
    };
    if let Some(name) =
        request.body.as_ref().and_then(|body| body.get("name")).and_then(Value::as_str)
    {
        state.folders[index]["name"] = json!(name);
    }
    Response::ok(with_folder_object(&state.folders[index]))
}

fn delete_folder(state: &mut State, id: &str) -> Response {
    let Some(index) = state.folders.iter().position(|folder| folder_id(folder) == Some(id)) else {
        return Response::error("Not found.");
    };
    state.folders.remove(index);
    for item in &mut state.items {
        if item.get("folderId").and_then(Value::as_str) == Some(id) {
            item["folderId"] = Value::Null;
        }
    }
    Response::ok(Value::Null)
}

fn generate_response(query: &str) -> Response {
    let data = if bool_or(query, "passphrase", false) {
        generate_passphrase(query)
    } else {
        generate_password(query)
    };
    Response::ok(json!({"object": "string", "data": data}))
}

const UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
const NUMBERS: &str = "0123456789";
const SPECIALS: &str = "!@#$%^&*";
const AMBIGUOUS: &str = "Il1O0o";

fn generate_password(query: &str) -> String {
    let length = query_param(query, "length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(14)
        .max(5);
    let uppercase = bool_or(query, "uppercase", true);
    let lowercase = bool_or(query, "lowercase", true);
    let numbers = bool_or(query, "number", true);
    let specials = bool_or(query, "special", false);
    let avoid_ambiguous = bool_or(query, "ambiguous", true);
    let min_numbers = query_param(query, "minNumber")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(usize::from(numbers));
    let min_specials = query_param(query, "minSpecial")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(usize::from(specials));

    let upper = charset(UPPERCASE, avoid_ambiguous);
    let lower = charset(LOWERCASE, avoid_ambiguous);
    let digits = charset(NUMBERS, avoid_ambiguous);
    let special = charset(SPECIALS, avoid_ambiguous);

    let mut pool: Vec<char> = Vec::new();
    if uppercase {
        pool.extend_from_slice(&upper);
    }
    if lowercase {
        pool.extend_from_slice(&lower);
    }
    if numbers {
        pool.extend_from_slice(&digits);
    }
    if specials {
        pool.extend_from_slice(&special);
    }
    if pool.is_empty() {
        pool.extend_from_slice(&lower);
    }

    let mut rng = rand::rng();
    let mut chars: Vec<char> = Vec::with_capacity(length);
    if numbers {
        for _ in 0..min_numbers.min(length) {
            chars.push(pick(&digits, &mut rng));
        }
    }
    if specials {
        for _ in 0..min_specials.min(length.saturating_sub(chars.len())) {
            chars.push(pick(&special, &mut rng));
        }
    }
    while chars.len() < length {
        chars.push(pick(&pool, &mut rng));
    }
    chars.shuffle(&mut rng);
    chars.into_iter().collect()
}

fn generate_passphrase(query: &str) -> String {
    let words = query_param(query, "words")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3)
        .max(3);
    let separator = match query_param(query, "separator").as_deref() {
        Some("space") => " ".to_owned(),
        Some("empty") => String::new(),
        Some(other) => other.to_owned(),
        None => "-".to_owned(),
    };
    let capitalize = bool_or(query, "capitalize", false);
    let include_number = bool_or(query, "includeNumber", false);

    let mut rng = rand::rng();
    let mut parts: Vec<String> = (0..words).map(|_| pick_word(&mut rng).to_owned()).collect();
    if capitalize {
        for part in &mut parts {
            if let Some(first) = part.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
        }
    }
    if include_number && let Some(last) = parts.last_mut() {
        last.push(char::from(b'0' + rng.random_range(0..10)));
    }
    parts.join(&separator)
}

fn charset(source: &str, avoid_ambiguous: bool) -> Vec<char> {
    source.chars().filter(|c| !avoid_ambiguous || !AMBIGUOUS.contains(*c)).collect()
}

fn pick(chars: &[char], rng: &mut impl Rng) -> char {
    match chars {
        [] => 'a',
        _ => chars[rng.random_range(0..chars.len())],
    }
}

const WORDS: [&str; 32] = [
    "amber", "beacon", "cobalt", "delta", "ember", "falcon", "glacier", "harbor", "indigo",
    "jasper", "kestrel", "lantern", "meadow", "nebula", "onyx", "pepper", "quartz", "raven",
    "signal", "timber", "umbra", "velvet", "willow", "xenon", "yonder", "zephyr", "atlas",
    "breeze", "cinder", "dune", "echo", "fable",
];

fn pick_word(rng: &mut impl Rng) -> &'static str {
    match WORDS.get(rng.random_range(0..WORDS.len())) {
        Some(word) => word,
        None => "amber",
    }
}

/// Whether a `Host` header is accepted by the serve origin check.
///
/// The port defaults to `8087`; an explicit port must be `8087`.
#[must_use]
pub fn host_allowed(host: &str) -> bool {
    let host = host.trim();
    let (name, port) = split_host(host);
    if !port.is_empty() && port != "8087" {
        return false;
    }
    let name = name.to_ascii_lowercase();
    name == "localhost"
        || name == "127.0.0.1"
        || name == "[::1]"
        || Some(name) == machine_hostname()
}

fn split_host(host: &str) -> (&str, &str) {
    if host.starts_with('[') {
        return match host.find(']') {
            Some(end) => {
                let name = &host[..=end];
                let port = host[end + 1..].strip_prefix(':').unwrap_or_default();
                (name, port)
            }
            None => (host, ""),
        };
    }
    match host.rsplit_once(':') {
        Some((name, port)) if !name.contains(':') => (name, port),
        _ => (host, ""),
    }
}

fn machine_hostname() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|name| !name.is_empty())
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|name| name.trim().to_ascii_lowercase())
}

/// A boolean query option: present and empty, or `true`, means true.
#[must_use]
pub fn query_bool(query: &str, key: &str) -> bool {
    match query_param(query, key) {
        Some(value) => value.is_empty() || value.eq_ignore_ascii_case("true"),
        None => false,
    }
}

fn bool_or(query: &str, key: &str, default: bool) -> bool {
    match query_param(query, key) {
        Some(value) => value.is_empty() || value.eq_ignore_ascii_case("true"),
        None => default,
    }
}

/// The decoded value of a query parameter, if present.
#[must_use]
pub fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if percent_decode(&name.replace('+', " ")) == key {
            Some(percent_decode(&value.replace('+', " ")))
        } else {
            None
        }
    })
}

fn path_segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|segment| !segment.is_empty()).collect()
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            out.push(high * 16 + low);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn find_item<'a>(state: &'a State, id: &str) -> Option<&'a Value> {
    state.items.iter().find(|item| item_id(item) == Some(id))
}

fn item_id(item: &Value) -> Option<&str> {
    item.get("id").and_then(Value::as_str)
}

fn folder_id(folder: &Value) -> Option<&str> {
    folder.get("id").and_then(Value::as_str)
}

fn nested_str<'a>(value: &'a Value, outer: &str, inner: &str) -> Option<&'a str> {
    value.get(outer).and_then(|nested| nested.get(inner)).and_then(Value::as_str)
}

fn password_history(updated: &Value, old: &Value) -> Vec<Value> {
    match updated.get("passwordHistory") {
        Some(Value::Array(history)) => history.clone(),
        _ => old.get("passwordHistory").and_then(Value::as_array).cloned().unwrap_or_default(),
    }
}

fn cap_history(history: &mut Vec<Value>) {
    if history.len() > 5 {
        let excess = history.len() - 5;
        history.drain(0..excess);
    }
}

fn with_item_object(item: &Value) -> Value {
    let mut item = item.clone();
    if item.get("object").is_none() {
        item["object"] = json!("item");
    }
    item
}

fn with_folder_object(folder: &Value) -> Value {
    let mut folder = folder.clone();
    if folder.get("object").is_none() {
        folder["object"] = json!("folder");
    }
    folder
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> State {
        let mut state = State::default();
        state.status = state::STATUS_UNLOCKED.to_owned();
        state.master_password = "correct horse".to_owned();
        state.session_key = Some("fake-session".to_owned());
        state.folders = vec![json!({"id": "folder-1", "name": "Personal"})];
        state.items = vec![
            json!({
                "id": "item-1",
                "type": 1,
                "name": "Example",
                "favorite": false,
                "login": {"username": "u", "password": "old"},
                "folderId": "folder-1",
            }),
            json!({
                "id": "item-2",
                "type": 2,
                "name": "Note",
                "secureNote": {"type": 0},
                "folderId": "folder-1",
            }),
        ];
        state
    }

    fn body(response: &Response) -> Value {
        serde_json::from_str(&response.body).expect("valid JSON envelope")
    }

    #[test]
    fn success_envelope_shape() {
        let response = Response::ok(json!({"object": "string", "data": "secret"}));
        assert_eq!(response.status, 200);
        let parsed = body(&response);
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["data"]["object"], "string");
        assert!(parsed.get("message").is_none());
    }

    #[test]
    fn error_envelope_shape() {
        let response = Response::error("Vault is locked.");
        assert_eq!(response.status, 400);
        let parsed = body(&response);
        assert_eq!(parsed["success"], false);
        assert_eq!(parsed["message"], "Vault is locked.");
        assert!(parsed["data"].is_null());
    }

    #[test]
    fn forbidden_when_origin_present() {
        let mut state = test_state();
        let request = Request::new("GET", "/status").with_origin("https://evil.example");
        let response = handle(&mut state, &request);
        assert_eq!(response.status, 403);
        assert_eq!(body(&response)["message"], "Forbidden");
    }

    #[test]
    fn forbidden_for_foreign_host() {
        let mut state = test_state();
        let response =
            handle(&mut state, &Request::new("GET", "/status").with_host("evil.example:8087"));
        assert_eq!(response.status, 403);
        let response =
            handle(&mut state, &Request::new("GET", "/status").with_host("localhost:8087"));
        assert_eq!(response.status, 200);
    }

    #[test]
    fn host_validation_accepts_documented_hosts() {
        assert!(host_allowed("localhost:8087"));
        assert!(host_allowed("localhost"));
        assert!(host_allowed("127.0.0.1:8087"));
        assert!(host_allowed("[::1]:8087"));
        assert!(!host_allowed("localhost:1234"));
        assert!(!host_allowed("example.com:8087"));
        assert!(!host_allowed("127.0.0.1.evil.com:8087"));
    }

    #[test]
    fn status_returns_template() {
        let mut state = test_state();
        let response = handle(&mut state, &Request::new("GET", "/status"));
        let parsed = body(&response);
        assert_eq!(parsed["data"]["object"], "template");
        assert_eq!(parsed["data"]["template"]["status"], "unlocked");
        assert_eq!(parsed["data"]["template"]["userEmail"], "you@example.com");
    }

    #[test]
    fn locked_state_blocks_vault_endpoints() {
        let mut state = test_state();
        state.status = state::STATUS_LOCKED.to_owned();
        for (method, path) in [
            ("GET", "/list/object/items"),
            ("GET", "/list/object/folders"),
            ("GET", "/object/item/item-1"),
            ("POST", "/object/item"),
            ("GET", "/generate"),
            ("POST", "/sync"),
        ] {
            let response = handle(&mut state, &Request::new(method, path));
            assert_eq!(response.status, 400, "{method} {path}");
            assert_eq!(body(&response)["message"], "Vault is locked.");
        }
        assert_eq!(handle(&mut state, &Request::new("GET", "/status")).status, 200);
    }

    #[test]
    fn unlock_validates_the_master_password() {
        let mut state = test_state();
        state.status = state::STATUS_LOCKED.to_owned();
        let wrong = Request::new("POST", "/unlock").with_body(json!({"password": "nope"}));
        let response = handle(&mut state, &wrong);
        assert_eq!(response.status, 400);
        assert_eq!(
            body(&response)["message"],
            "Cryptography error, The decryption operation failed"
        );

        let right = Request::new("POST", "/unlock").with_body(json!({"password": "correct horse"}));
        let response = handle(&mut state, &right);
        assert_eq!(response.status, 200);
        assert_eq!(body(&response)["data"]["raw"], "fake-session");
        assert_eq!(state.status, state::STATUS_UNLOCKED);
    }

    #[test]
    fn list_folders_includes_no_folder_pseudo_entry() {
        let mut state = test_state();
        let response = handle(&mut state, &Request::new("GET", "/list/object/folders"));
        let parsed = body(&response);
        let folders = parsed["data"]["data"].as_array().expect("array");
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[1]["id"], Value::Null);
        assert_eq!(folders[1]["name"], "No Folder");
    }

    #[test]
    fn password_history_is_capped_at_five() {
        let mut state = test_state();
        for index in 0..7 {
            let current = state.items[0].clone();
            let mut updated = current.clone();
            updated["login"]["password"] = json!(format!("p{index}"));
            updated["passwordHistory"] = current["passwordHistory"].clone();
            let response =
                handle(&mut state, &Request::new("PUT", "/object/item/item-1").with_body(updated));
            assert_eq!(response.status, 200);
        }
        let history = state.items[0]["passwordHistory"].as_array().expect("history array");
        assert_eq!(history.len(), 5);
        let passwords: Vec<&str> = history
            .iter()
            .filter_map(|entry| entry.get("password").and_then(Value::as_str))
            .collect();
        assert_eq!(passwords, vec!["p1", "p2", "p3", "p4", "p5"]);
    }

    #[test]
    fn deleting_a_folder_moves_items_to_no_folder() {
        let mut state = test_state();
        let response = handle(&mut state, &Request::new("DELETE", "/object/folder/folder-1"));
        assert_eq!(response.status, 200);
        assert!(state.folders.is_empty());
        assert!(state.items.iter().all(|item| item["folderId"].is_null()));
    }

    #[test]
    fn unknown_item_is_not_found() {
        let mut state = test_state();
        let response = handle(&mut state, &Request::new("GET", "/object/item/missing"));
        assert_eq!(response.status, 400);
        assert_eq!(body(&response)["message"], "Not found.");
    }

    #[test]
    fn unknown_endpoint_is_404_envelope() {
        let mut state = test_state();
        let response = handle(&mut state, &Request::new("GET", "/nope"));
        assert_eq!(response.status, 404);
        assert_eq!(body(&response)["success"], false);
    }

    #[test]
    fn trash_and_archived_lists_are_separate() {
        let mut state = test_state();
        state.items[0]["deletedDate"] = json!("2026-01-01T00:00:00.000Z");
        state.items[1]["archivedDate"] = json!("2026-01-02T00:00:00.000Z");
        let normal = body(&handle(&mut state, &Request::new("GET", "/list/object/items")));
        assert!(normal["data"]["data"].as_array().expect("list").is_empty());
        let trash = body(&handle(
            &mut state,
            &Request::new("GET", "/list/object/items").with_query("trash=true"),
        ));
        assert_eq!(trash["data"]["data"].as_array().expect("list").len(), 1);
        let archived = body(&handle(
            &mut state,
            &Request::new("GET", "/list/object/items").with_query("archived=true"),
        ));
        assert_eq!(archived["data"]["data"].as_array().expect("list").len(), 1);
    }

    #[test]
    fn generate_honours_length_and_character_classes() {
        let mut state = test_state();
        let response = handle(
            &mut state,
            &Request::new("GET", "/generate").with_query(
                "length=24&uppercase=false&lowercase=true&number=false&special=false&ambiguous=false",
            ),
        );
        let password = body(&response)["data"]["data"].as_str().expect("string").to_owned();
        assert_eq!(password.chars().count(), 24);
        assert!(password.chars().all(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn generate_honours_minimum_counts() {
        let mut state = test_state();
        let response = handle(
            &mut state,
            &Request::new("GET", "/generate").with_query(
                "length=12&uppercase=false&lowercase=true&number=true&special=true&minNumber=4&minSpecial=2",
            ),
        );
        let password = body(&response)["data"]["data"].as_str().expect("string").to_owned();
        assert_eq!(password.chars().count(), 12);
        assert!(password.chars().filter(char::is_ascii_digit).count() >= 4);
        assert!(password.chars().filter(|c| SPECIALS.contains(*c)).count() >= 2);
    }

    #[test]
    fn generate_passphrase_honours_words_and_separator() {
        let mut state = test_state();
        let response = handle(
            &mut state,
            &Request::new("GET", "/generate")
                .with_query("passphrase=true&words=5&separator=space&capitalize=true"),
        );
        let phrase = body(&response)["data"]["data"].as_str().expect("string").to_owned();
        let words: Vec<&str> = phrase.split(' ').collect();
        assert_eq!(words.len(), 5);
        assert!(words[0].chars().next().is_some_and(char::is_uppercase));
    }
}
