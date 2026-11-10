//! Typed HTTP client for `bw serve` over a Unix socket.
//!
//! One connection, one request at a time. Every request carries the exact
//! headers `bw serve` requires (`Host: localhost:8087`, no `Origin`), and
//! every endpoint has its own timeout from bw-integration.md §6.2.

use crate::bw::envelope::BwError;
use crate::bw::envelope::StatusTemplate;
use crate::bw::models::{Folder, GenerateRequest, Item, Status};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::Request;
use hyper::client::conn::http1::{self, SendRequest};
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::net::UnixStream;

/// The `Host` header `bw serve` accepts.
pub const HOST_HEADER: &str = "localhost:8087";

/// Per-endpoint timeouts.
pub mod timeouts {
    use std::time::Duration;
    /// `/status`
    pub const STATUS: Duration = Duration::from_secs(10);
    /// `/unlock`
    pub const UNLOCK: Duration = Duration::from_secs(120);
    /// `/lock`
    pub const LOCK: Duration = Duration::from_secs(10);
    /// `/sync`
    pub const SYNC: Duration = Duration::from_secs(120);
    /// list items and trash
    pub const LIST: Duration = Duration::from_secs(60);
    /// folders
    pub const FOLDERS: Duration = Duration::from_secs(30);
    /// item objects
    pub const ITEM: Duration = Duration::from_secs(30);
    /// `/generate`
    pub const GENERATE: Duration = Duration::from_secs(10);
}

/// A `{object, data}` list payload.
#[derive(Debug, Clone, Deserialize)]
pub struct ListResponse<T> {
    /// Payload kind (`list`).
    #[serde(default)]
    pub object: String,
    /// Entries.
    #[serde(default)]
    pub data: Vec<T>,
}

/// A `{object, data}` string payload.
#[derive(Debug, Clone, Deserialize)]
pub struct StringResponse {
    /// Payload kind (`string`).
    #[serde(default)]
    pub object: String,
    /// The value.
    pub data: String,
}

/// The `/unlock` payload.
#[derive(Debug, Clone, Deserialize)]
pub struct UnlockResponse {
    /// Session key.
    #[serde(default)]
    pub raw: String,
    /// Human message.
    #[serde(default)]
    pub message: String,
}

/// An open HTTP/1.1 connection to `bw serve`.
#[derive(Debug)]
pub struct BwClient {
    sender: SendRequest<Full<Bytes>>,
    /// The task driving the HTTP connection; owned so it ends with the
    /// client instead of outliving it.
    driver: tokio::task::JoinHandle<()>,
}

impl Drop for BwClient {
    fn drop(&mut self) {
        self.driver.abort();
    }
}

impl BwClient {
    /// Performs the HTTP handshake over an established socket and spawns the
    /// connection driver.
    pub async fn handshake(stream: UnixStream) -> Result<Self, BwError> {
        let io = TokioIo::new(stream);
        let (sender, connection) = http1::handshake(io)
            .await
            .map_err(|e| BwError::InvalidResponse(format!("handshake failed: {e}")))?;
        let driver = tokio::spawn(async move {
            if let Err(err) = connection.await {
                tracing::debug!(error = %err, "bw serve connection ended");
            }
        });
        Ok(Self { sender, driver })
    }

    async fn request(
        &mut self,
        method: hyper::Method,
        path: &str,
        body: Option<Value>,
        timeout: Duration,
    ) -> Result<(u16, Bytes), BwError> {
        let endpoint = path.split('?').next().unwrap_or(path).to_owned();
        let fut = async {
            let builder = Request::builder()
                .method(method)
                .uri(path)
                .header(hyper::header::HOST, HOST_HEADER)
                .header(hyper::header::ACCEPT, "application/json");
            let request = match body {
                Some(value) => {
                    let bytes = serde_json::to_vec(&value)
                        .map_err(|e| BwError::InvalidResponse(e.to_string()))?;
                    builder
                        .header(hyper::header::CONTENT_TYPE, "application/json")
                        .header(hyper::header::CONTENT_LENGTH, bytes.len())
                        .body(Full::new(Bytes::from(bytes)))
                        .map_err(|e| BwError::InvalidResponse(e.to_string()))?
                }
                None => builder
                    .body(Full::new(Bytes::new()))
                    .map_err(|e| BwError::InvalidResponse(e.to_string()))?,
            };
            self.sender.ready().await.map_err(|_| BwError::Disconnected)?;
            let response =
                self.sender.send_request(request).await.map_err(|_| BwError::Disconnected)?;
            let status = response.status().as_u16();
            let collected =
                response.into_body().collect().await.map_err(|_| BwError::Disconnected)?;
            Ok::<(u16, Bytes), BwError>((status, collected.to_bytes()))
        };
        let (status, body) = tokio::time::timeout(timeout, fut)
            .await
            .map_err(|_| BwError::Timeout { endpoint })??;
        if status != 200 && status != 400 && status != 403 {
            return Err(BwError::InvalidResponse(format!("HTTP {status}")));
        }
        Ok((status, body))
    }

    async fn call<T: DeserializeOwned>(
        &mut self,
        method: hyper::Method,
        path: &str,
        body: Option<Value>,
        timeout: Duration,
    ) -> Result<T, BwError> {
        let (status, body) = self.request(method, path, body, timeout).await?;
        crate::bw::envelope::decode_response(status, &body)
    }

    /// `GET /status`.
    pub async fn status(&mut self) -> Result<Status, BwError> {
        let template: StatusTemplate =
            self.call(hyper::Method::GET, "/status", None, timeouts::STATUS).await?;
        Ok(template.template)
    }

    /// `POST /unlock`.
    pub async fn unlock(&mut self, password: &str) -> Result<UnlockResponse, BwError> {
        self.call(
            hyper::Method::POST,
            "/unlock",
            Some(json!({ "password": password })),
            timeouts::UNLOCK,
        )
        .await
        .map_err(unlock_error)
    }

    /// `POST /lock`.
    pub async fn lock(&mut self) -> Result<(), BwError> {
        self.empty(hyper::Method::POST, "/lock", None, timeouts::LOCK).await
    }

    /// `POST /sync`.
    pub async fn sync(&mut self, force: bool) -> Result<String, BwError> {
        let path = if force { "/sync?force=true" } else { "/sync" };
        let response: Value = self.call(hyper::Method::POST, path, None, timeouts::SYNC).await?;
        Ok(response
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Syncing complete.")
            .to_owned())
    }

    /// `GET /list/object/items`.
    pub async fn list_items(&mut self, trash: bool, archived: bool) -> Result<Vec<Item>, BwError> {
        let mut path = String::from("/list/object/items");
        let mut separator = '?';
        if trash {
            path.push_str("?trash=true");
            separator = '&';
        }
        if archived {
            path.push_str(&format!("{separator}archived=true"));
        }
        let response: ListResponse<Item> =
            self.call(hyper::Method::GET, &path, None, timeouts::LIST).await?;
        Ok(response.data)
    }

    /// `GET /list/object/folders`, skipping the `No Folder` pseudo entry.
    pub async fn list_folders(&mut self) -> Result<Vec<Folder>, BwError> {
        let response: ListResponse<Folder> =
            self.call(hyper::Method::GET, "/list/object/folders", None, timeouts::FOLDERS).await?;
        Ok(response.data.into_iter().filter(|f| f.id.is_some()).collect())
    }

    /// `GET /object/item/{id}`.
    pub async fn get_item(&mut self, id: &str) -> Result<Item, BwError> {
        self.call(hyper::Method::GET, &format!("/object/item/{id}"), None, timeouts::ITEM).await
    }

    /// `POST /object/item`.
    pub async fn create_item(&mut self, item: &Item) -> Result<Item, BwError> {
        let body = serde_json::to_value(item.for_create())
            .map_err(|e| BwError::InvalidResponse(e.to_string()))?;
        self.call(hyper::Method::POST, "/object/item", Some(body), timeouts::ITEM).await
    }

    /// `PUT /object/item/{id}` with the complete item.
    pub async fn put_item(&mut self, id: &str, item: &Item) -> Result<Item, BwError> {
        let body =
            serde_json::to_value(item).map_err(|e| BwError::InvalidResponse(e.to_string()))?;
        self.call(hyper::Method::PUT, &format!("/object/item/{id}"), Some(body), timeouts::ITEM)
            .await
    }

    /// `DELETE /object/item/{id}` (soft delete unless `permanent`).
    pub async fn delete_item(&mut self, id: &str, permanent: bool) -> Result<(), BwError> {
        let path = if permanent {
            format!("/object/item/{id}?permanent=true")
        } else {
            format!("/object/item/{id}")
        };
        self.empty(hyper::Method::DELETE, &path, None, timeouts::ITEM).await
    }

    /// `POST /restore/item/{id}`.
    pub async fn restore_item(&mut self, id: &str) -> Result<(), BwError> {
        self.empty(hyper::Method::POST, &format!("/restore/item/{id}"), None, timeouts::ITEM).await
    }

    /// `POST /object/folder`.
    pub async fn create_folder(&mut self, name: &str) -> Result<Folder, BwError> {
        self.call(
            hyper::Method::POST,
            "/object/folder",
            Some(json!({ "name": name })),
            timeouts::FOLDERS,
        )
        .await
    }

    /// `PUT /object/folder/{id}`.
    pub async fn rename_folder(&mut self, id: &str, name: &str) -> Result<Folder, BwError> {
        self.call(
            hyper::Method::PUT,
            &format!("/object/folder/{id}"),
            Some(json!({ "name": name })),
            timeouts::FOLDERS,
        )
        .await
    }

    /// `DELETE /object/folder/{id}`.
    pub async fn delete_folder(&mut self, id: &str) -> Result<(), BwError> {
        self.empty(hyper::Method::DELETE, &format!("/object/folder/{id}"), None, timeouts::FOLDERS)
            .await
    }

    /// `GET /generate`.
    pub async fn generate(&mut self, request: &GenerateRequest) -> Result<String, BwError> {
        let query = request
            .query()
            .into_iter()
            .map(|(k, v)| {
                let value = match v.as_str() {
                    "space" => "%20".to_owned(),
                    "empty" => String::new(),
                    other => urlencode(other),
                };
                format!("{k}={value}")
            })
            .collect::<Vec<_>>()
            .join("&");
        let response: StringResponse = self
            .call(hyper::Method::GET, &format!("/generate?{query}"), None, timeouts::GENERATE)
            .await?;
        Ok(response.data)
    }

    async fn empty(
        &mut self,
        method: hyper::Method,
        path: &str,
        body: Option<Value>,
        timeout: Duration,
    ) -> Result<(), BwError> {
        let (status, body) = self.request(method, path, body, timeout).await?;
        crate::bw::envelope::check(status, &body)
    }
}

fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Reads an unlock failure. bw 2026.10.1 no longer answers a wrong master
/// password with "Invalid master password." but with "Cryptography error,
/// The decryption operation failed": the key derived from the password cannot
/// decrypt the account key. On `POST /unlock` that can only mean a wrong
/// password; elsewhere the same message means something else, so the mapping
/// stays here rather than in [`BwError::from_message`].
fn unlock_error(err: BwError) -> BwError {
    match err {
        BwError::Other(message)
            if message.to_ascii_lowercase().contains("decryption operation failed") =>
        {
            BwError::InvalidPassword
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_failed_decryption_on_unlock_is_a_wrong_password() {
        let modern = BwError::from_message("Cryptography error, The decryption operation failed");
        assert!(matches!(unlock_error(modern), BwError::InvalidPassword));
        let older = BwError::from_message("Invalid master password.");
        assert!(matches!(unlock_error(older), BwError::InvalidPassword));
        let other = BwError::from_message("Something else.");
        assert!(matches!(unlock_error(other), BwError::Other(_)));
    }
    use super::*;

    #[test]
    fn encodes_query_values() {
        assert_eq!(urlencode("a b&c"), "a%20b%26c");
        assert_eq!(urlencode("-"), "-");
    }

    #[test]
    fn generate_query_contains_every_option() {
        let request = GenerateRequest::default();
        let query = request.query();
        let keys: Vec<_> = query.iter().map(|(k, _)| *k).collect();
        for expected in [
            "uppercase",
            "lowercase",
            "number",
            "special",
            "length",
            "minNumber",
            "minSpecial",
            "ambiguous",
            "passphrase",
            "words",
            "separator",
            "capitalize",
            "includeNumber",
        ] {
            assert!(keys.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn list_payload_decodes() {
        let body =
            br#"{"success":true,"data":{"object":"list","data":[{"id":"1","name":"x","type":1}]}}"#;
        let list: ListResponse<Item> = crate::bw::envelope::decode(body).unwrap();
        assert_eq!(list.data.len(), 1);
        assert_eq!(list.data[0].name, "x");
    }
}
