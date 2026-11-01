//! The backend actor: owns the `bw serve` connection, the session key and the
//! subprocess runner.
//!
//! Requests are queued and executed one at a time (one connection, and
//! `bw serve` is single threaded). Every request carries a correlation id;
//! results come back as [`BackendEvent`] on the channel the application
//! polls. The session key never leaves this task.

use crate::bw::client::BwClient;
use crate::bw::envelope::BwError;
use crate::bw::models::{Folder, GenerateRequest, HealthEntry, Item, Status};
use crate::bw::serve::{Serve, ServeOptions};
use crate::bw::subprocess;
use crate::config::Transport;
use secrecy::{ExposeSecret, SecretString};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

/// The export formats lazybw offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormat {
    /// Plain JSON.
    #[default]
    Json,
    /// JSON encrypted with a file password.
    PasswordProtectedJson,
    /// JSON encrypted with the account key.
    AccountEncryptedJson,
    /// CSV.
    Csv,
}

impl ExportFormat {
    /// The `bw export --format` value.
    #[must_use]
    pub fn bw_format(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::PasswordProtectedJson | Self::AccountEncryptedJson => "encrypted_json",
            Self::Csv => "csv",
        }
    }

    /// File extension.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            _ => "json",
        }
    }

    /// True when the format needs a file password.
    #[must_use]
    pub fn needs_password(self) -> bool {
        matches!(self, Self::PasswordProtectedJson)
    }

    /// True for plaintext output.
    #[must_use]
    pub fn is_plaintext(self) -> bool {
        matches!(self, Self::Json | Self::Csv)
    }

    /// Human label used in the export wizard.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::PasswordProtectedJson => "JSON encrypted with a password",
            Self::AccountEncryptedJson => "JSON encrypted with your account",
            Self::Csv => "CSV",
        }
    }

    /// Parses a `:export` argument.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "json" => Some(Self::Json),
            "encrypted" | "password" => Some(Self::PasswordProtectedJson),
            "account" => Some(Self::AccountEncryptedJson),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }
}

/// A loaded vault.
#[derive(Debug)]
pub struct VaultSnapshot {
    /// Active items.
    pub items: Vec<Item>,
    /// Folders (without the `No Folder` pseudo entry).
    pub folders: Vec<Folder>,
    /// Trashed items.
    pub trash: Vec<Item>,
    /// Archived items (read-only), empty when unsupported.
    pub archived: Vec<Item>,
    /// The status after loading.
    pub status: Status,
}

/// What happened to a folder.
#[derive(Debug)]
pub enum FolderAction {
    /// Created.
    Created(Folder),
    /// Renamed.
    Renamed(Folder),
    /// Deleted by id.
    Deleted(String),
}

/// A successful export.
#[derive(Debug)]
pub struct ExportOutcome {
    /// Destination path.
    pub path: PathBuf,
    /// Number of bytes written.
    pub bytes: u64,
}

/// A password-protected export or import request.
#[derive(Debug)]
pub struct ImportRequest {
    /// `bw import` format name.
    pub format: String,
    /// File to import.
    pub path: PathBuf,
    /// Password for `bitwardenpasswordprotected`.
    pub password: Option<SecretString>,
    /// Check whether the file is password protected.
    pub password_protected: bool,
    /// The file is a lazybw temporary copy (duplicates removed): shred it
    /// once the CLI has read it.
    pub temporary: bool,
}

/// The commands the UI can send to the backend.
#[derive(Debug)]
pub enum CommandKind {
    /// Spawn `bw serve` and report the status.
    Start,
    /// Replace the session key (e.g. after an interactive login).
    SetSession(SecretString),
    /// Unlock with the master password.
    Unlock(SecretString),
    /// Lock the vault.
    Lock,
    /// Sync.
    Sync {
        /// Force a full sync.
        force: bool,
    },
    /// Load folders, items, trash and archived items.
    LoadVault,
    /// Fetch one item.
    GetItem(String),
    /// Create an item.
    CreateItem(Box<Item>),
    /// Replace an item.
    PutItem {
        /// Item id.
        id: String,
        /// Full item.
        item: Box<Item>,
    },
    /// Move to trash or delete permanently.
    DeleteItem {
        /// Item id.
        id: String,
        /// Permanent delete.
        permanent: bool,
    },
    /// Restore from trash.
    RestoreItem(String),
    /// Create a folder.
    CreateFolder(String),
    /// Rename a folder.
    RenameFolder {
        /// Folder id.
        id: String,
        /// New name.
        name: String,
    },
    /// Delete a folder.
    DeleteFolder(String),
    /// Generate a password.
    Generate(GenerateRequest),
    /// Export the vault.
    Export {
        /// Format.
        format: ExportFormat,
        /// Destination.
        destination: PathBuf,
        /// File password for the protected format.
        password: Option<SecretString>,
        /// The master password the user confirmed the export with; verified
        /// before anything is written.
        master: SecretString,
    },
    /// Import a file.
    Import(ImportRequest),
    /// `bw import --formats`.
    ImportFormats,
    /// Run the password health report.
    Health,
    /// API-key login.
    LoginApiKey {
        /// Client id.
        client_id: SecretString,
        /// Client secret.
        client_secret: SecretString,
    },
    /// Log out.
    Logout,
    /// Configure the server URL.
    ConfigServer(String),
    /// Stop the backend.
    Shutdown,
}

/// A command with its correlation id.
#[derive(Debug)]
pub struct Command {
    /// Correlation id.
    pub id: u64,
    /// The command.
    pub kind: CommandKind,
}

/// A result from the backend.
#[derive(Debug)]
pub enum BwEvent {
    /// `Start` finished.
    Started(Result<Status, BwError>),
    /// A session key was accepted and the vault is locked.
    SessionSet(Result<(), BwError>),
    /// Unlock finished; the service now holds the new session key.
    Unlocked(Result<Status, BwError>),
    /// Lock finished.
    Locked(Result<(), BwError>),
    /// Sync finished.
    Synced(Result<Status, BwError>),
    /// The vault was loaded.
    Vault(Result<VaultSnapshot, BwError>),
    /// One item.
    Item(Result<Item, BwError>),
    /// An item was written (create or edit).
    ItemWritten(Result<Item, BwError>),
    /// An item was deleted (soft or permanent), carrying its id.
    ItemDeleted(Result<String, BwError>),
    /// An item was restored, carrying its id.
    ItemRestored(Result<String, BwError>),
    /// A folder operation.
    Folder(Result<FolderAction, BwError>),
    /// A generated password.
    Generated(Result<String, BwError>),
    /// An export finished.
    Exported(Result<ExportOutcome, BwError>),
    /// An import finished.
    Imported(Result<String, BwError>),
    /// The import formats the CLI supports.
    ImportFormats(Vec<String>),
    /// The health report.
    Health(Result<Vec<HealthEntry>, BwError>),
    /// An API-key login finished.
    LoggedIn(Result<Status, BwError>),
    /// Logout finished.
    LoggedOut(Result<(), BwError>),
    /// Server configuration finished.
    ServerConfigured(Result<(), BwError>),
    /// `bw serve` died and could not be restarted.
    Fatal(String),
}

impl BwEvent {
    /// The error carried by the event, if any.
    #[must_use]
    pub fn error(&self) -> Option<&BwError> {
        match self {
            Self::Started(r) | Self::Unlocked(r) | Self::Synced(r) | Self::LoggedIn(r) => {
                r.as_ref().err()
            }
            Self::SessionSet(r)
            | Self::Locked(r)
            | Self::LoggedOut(r)
            | Self::ServerConfigured(r) => r.as_ref().err(),
            Self::Vault(r) => r.as_ref().err(),
            Self::Item(r) | Self::ItemWritten(r) => r.as_ref().err(),
            Self::ItemDeleted(r)
            | Self::ItemRestored(r)
            | Self::Generated(r)
            | Self::Imported(r) => r.as_ref().err(),
            Self::Folder(r) => r.as_ref().err(),
            Self::Exported(r) => r.as_ref().err(),
            Self::Health(r) => r.as_ref().err(),
            Self::Fatal(_) | Self::ImportFormats(_) => None,
        }
    }

    /// True when the request failed because the `bw serve` connection broke.
    #[must_use]
    pub fn is_disconnected(&self) -> bool {
        self.error().is_some_and(BwError::is_disconnected)
    }
}

/// A backend event with its correlation id.
#[derive(Debug)]
pub struct BackendEvent {
    /// Correlation id of the request.
    pub id: u64,
    /// The result.
    pub event: BwEvent,
}

/// Clonable handle used by the UI.
#[derive(Debug, Clone)]
pub struct Backend {
    sender: mpsc::Sender<Command>,
}

impl Backend {
    /// Sends a command; a closed backend is reported to the caller.
    pub async fn send(&self, id: u64, kind: CommandKind) -> Result<(), BwError> {
        self.sender.send(Command { id, kind }).await.map_err(|_| BwError::Disconnected)
    }
}

/// Static configuration of the backend task.
#[derive(Debug, Clone)]
pub struct BackendConfig {
    /// Path to `bw`.
    pub bw_path: PathBuf,
    /// Transport for `bw serve`.
    pub transport: Transport,
    /// Private runtime directory.
    pub runtime_dir: PathBuf,
    /// Reuse an inherited `BW_SESSION` on the first start.
    pub inherit_session: bool,
    /// Version gates of the located CLI.
    pub capabilities: crate::bw::locate::Capabilities,
    /// Check Have I Been Pwned in the health report.
    pub check_exposed: bool,
}

struct Service {
    config: BackendConfig,
    client: Option<BwClient>,
    serve: Option<Serve>,
    session: Option<SecretString>,
}

impl Service {
    fn new(config: BackendConfig) -> Self {
        Self { config, client: None, serve: None, session: None }
    }

    async fn ensure_client(&mut self) -> Result<&mut BwClient, BwError> {
        if self.serve.as_mut().is_some_and(|serve| serve.has_exited().is_some()) {
            self.client = None;
        }
        if self.client.is_none() {
            self.start_serve().await?;
        }
        self.client.as_mut().ok_or(BwError::Disconnected)
    }

    async fn start_serve(&mut self) -> Result<(), BwError> {
        if let Some(serve) = self.serve.take() {
            serve.shutdown().await;
        }
        self.client = None;
        let options = ServeOptions {
            path: self.config.bw_path.clone(),
            session_key: self.session.as_ref().map(|s| s.expose_secret().to_owned()),
            transport: self.config.transport,
            runtime_dir: self.config.runtime_dir.clone(),
        };
        let mut serve = Serve::spawn(&options)?;
        let client = serve.connect().await?;
        self.serve = Some(serve);
        self.client = Some(client);
        Ok(())
    }

    async fn status(&mut self) -> Result<Status, BwError> {
        let client = self.ensure_client().await?;
        client.status().await
    }

    async fn execute(&mut self, kind: CommandKind) -> BwEvent {
        match kind {
            CommandKind::Start => {
                let result = async {
                    if self.session.is_none()
                        && self.config.inherit_session
                        && let Ok(key) = std::env::var("BW_SESSION")
                        && !key.is_empty()
                    {
                        self.session = Some(SecretString::from(key));
                    }
                    self.start_serve().await?;
                    self.status().await
                }
                .await;
                BwEvent::Started(result)
            }
            CommandKind::SetSession(key) => {
                self.session = Some(key);
                self.client = None;
                if let Some(serve) = self.serve.take() {
                    serve.shutdown().await;
                }
                BwEvent::SessionSet(Ok(()))
            }
            CommandKind::Unlock(password) => {
                let result = self.unlock(password.expose_secret()).await;
                BwEvent::Unlocked(result)
            }
            CommandKind::Lock => {
                let result = async {
                    let client = self.ensure_client().await?;
                    client.lock().await
                }
                .await;
                // The session key is useless once locked; never keep it.
                self.session = None;
                BwEvent::Locked(result)
            }
            CommandKind::Sync { force } => {
                let result = async {
                    let client = self.ensure_client().await?;
                    client.sync(force).await?;
                    client.status().await
                }
                .await;
                BwEvent::Synced(result)
            }
            CommandKind::LoadVault => {
                let result = self.load_vault().await;
                BwEvent::Vault(result)
            }
            CommandKind::GetItem(id) => {
                let result = async {
                    let client = self.ensure_client().await?;
                    client.get_item(&id).await
                }
                .await;
                BwEvent::Item(result)
            }
            CommandKind::CreateItem(item) => {
                let result = async {
                    let client = self.ensure_client().await?;
                    client.create_item(&item).await
                }
                .await;
                BwEvent::ItemWritten(result)
            }
            CommandKind::PutItem { id, item } => {
                let result = async {
                    let client = self.ensure_client().await?;
                    client.put_item(&id, &item).await
                }
                .await;
                BwEvent::ItemWritten(result)
            }
            CommandKind::DeleteItem { id, permanent } => {
                let result = async {
                    let client = self.ensure_client().await?;
                    client.delete_item(&id, permanent).await?;
                    Ok(id)
                }
                .await;
                BwEvent::ItemDeleted(result)
            }
            CommandKind::RestoreItem(id) => {
                let result = async {
                    let client = self.ensure_client().await?;
                    client.restore_item(&id).await?;
                    Ok(id)
                }
                .await;
                BwEvent::ItemRestored(result)
            }
            CommandKind::CreateFolder(name) => {
                let result = async {
                    let client = self.ensure_client().await?;
                    client.create_folder(&name).await.map(FolderAction::Created)
                }
                .await;
                BwEvent::Folder(result)
            }
            CommandKind::RenameFolder { id, name } => {
                let result = async {
                    let client = self.ensure_client().await?;
                    client.rename_folder(&id, &name).await.map(FolderAction::Renamed)
                }
                .await;
                BwEvent::Folder(result)
            }
            CommandKind::DeleteFolder(id) => {
                let result = async {
                    let client = self.ensure_client().await?;
                    client.delete_folder(&id).await?;
                    Ok(FolderAction::Deleted(id))
                }
                .await;
                BwEvent::Folder(result)
            }
            CommandKind::Generate(request) => {
                let result = async {
                    let client = self.ensure_client().await?;
                    client.generate(&request).await
                }
                .await;
                BwEvent::Generated(result)
            }
            CommandKind::Export { format, destination, password, master } => {
                let result = async {
                    // `POST /unlock` checks the master password (and refreshes
                    // the session key); a wrong one fails with the CLI message.
                    self.unlock(master.expose_secret()).await?;
                    self.export(format, &destination, password).await
                }
                .await;
                BwEvent::Exported(result)
            }
            CommandKind::Import(request) => {
                let result = async {
                    let session = self.session()?;
                    let path = self.config.bw_path.clone();
                    let result = subprocess::import(
                        &path,
                        &request.format,
                        &request.path,
                        &session,
                        request.password.as_ref().map(|p| p.expose_secret()),
                        Duration::from_secs(600),
                    )
                    .await;
                    if request.temporary {
                        let _ = crate::security::shred_and_remove(&request.path);
                    }
                    result
                }
                .await;
                BwEvent::Imported(result)
            }
            CommandKind::ImportFormats => {
                let session = self.session.as_ref().map(|s| s.expose_secret().to_owned());
                BwEvent::ImportFormats(
                    subprocess::import_formats(&self.config.bw_path, session.as_deref()).await,
                )
            }
            CommandKind::Health => {
                let result = async {
                    let session = self.session()?;
                    let path = self.config.bw_path.clone();
                    subprocess::report_health(&path, &session, self.config.check_exposed).await
                }
                .await;
                BwEvent::Health(result)
            }
            CommandKind::LoginApiKey { client_id, client_secret } => {
                let result = async {
                    let path = self.config.bw_path.clone();
                    subprocess::login_apikey(
                        &path,
                        client_id.expose_secret(),
                        client_secret.expose_secret(),
                    )
                    .await?;
                    // The account is logged in and locked; serve can start
                    // without a session key but will report `locked`.
                    self.client = None;
                    self.start_serve().await?;
                    self.status().await
                }
                .await;
                BwEvent::LoggedIn(result)
            }
            CommandKind::Logout => {
                let result = async {
                    if let Some(serve) = self.serve.take() {
                        serve.shutdown().await;
                    }
                    self.client = None;
                    self.session = None;
                    let path = self.config.bw_path.clone();
                    subprocess::logout(&path).await
                }
                .await;
                BwEvent::LoggedOut(result)
            }
            CommandKind::ConfigServer(url) => {
                let path = self.config.bw_path.clone();
                BwEvent::ServerConfigured(subprocess::config_server(&path, &url).await)
            }
            CommandKind::Shutdown => {
                if let Some(serve) = self.serve.take() {
                    serve.shutdown().await;
                }
                self.client = None;
                BwEvent::Fatal("backend stopped".to_owned())
            }
        }
    }

    fn session(&self) -> Result<String, BwError> {
        self.session.as_ref().map(|s| s.expose_secret().to_owned()).ok_or(BwError::Locked)
    }

    async fn unlock(&mut self, password: &str) -> Result<Status, BwError> {
        let client = self.ensure_client().await?;
        let response = client.unlock(password).await?;
        if response.raw.is_empty() {
            return Err(BwError::InvalidResponse("unlock returned no session key".to_owned()));
        }
        self.session = Some(SecretString::from(response.raw));
        let client = self.ensure_client().await?;
        client.status().await
    }

    async fn load_vault(&mut self) -> Result<VaultSnapshot, BwError> {
        let archived_enabled = self.config.capabilities.archived_list;
        let client = self.ensure_client().await?;
        let folders = client.list_folders().await?;
        let items = client.list_items(false, false).await?;
        let trash = client.list_items(true, false).await?;
        let archived = if archived_enabled {
            client.list_items(false, true).await.unwrap_or_default()
        } else {
            Vec::new()
        };
        let status = client.status().await?;
        Ok(VaultSnapshot { items, folders, trash, archived, status })
    }

    async fn export(
        &mut self,
        format: ExportFormat,
        destination: &std::path::Path,
        password: Option<SecretString>,
    ) -> Result<ExportOutcome, BwError> {
        let session = self.session()?;
        let path = self.config.bw_path.clone();
        let bytes = match format {
            ExportFormat::PasswordProtectedJson => {
                // bw writes this file itself, so it must not get a guessable
                // name next to the destination (which may be a shared
                // directory): use a fresh name in lazybw's private 0700
                // runtime directory instead.
                let tmp = crate::security::secure_temp_dir()
                    .map_err(|e| BwError::Other(format!("no private temporary directory: {e}")))?
                    .join(format!(
                        "export-{}-{:x}.json",
                        std::process::id(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos())
                            .unwrap_or_default()
                    ));
                let password = password
                    .as_ref()
                    .ok_or_else(|| BwError::Other("a file password is required".to_owned()))?;
                let exported = subprocess::export_password_protected(
                    &path,
                    &tmp,
                    password.expose_secret(),
                    &session,
                    Duration::from_secs(180),
                )
                .await
                .and_then(|()| {
                    std::fs::read(&tmp)
                        .map_err(|e| BwError::Other(format!("could not read the export: {e}")))
                });
                // Whatever happened, nothing stays behind in the runtime dir.
                if tmp.exists() {
                    let _ = crate::security::shred_and_remove(&tmp);
                }
                let bytes = exported?;
                let encrypted =
                    serde_json::from_slice::<serde_json::Value>(&bytes).ok().and_then(|parsed| {
                        parsed.get("encrypted").and_then(serde_json::Value::as_bool)
                    });
                if encrypted != Some(true) {
                    return Err(BwError::Other(
                        "the export is not an encrypted file; nothing was written".to_owned(),
                    ));
                }
                crate::util::atomic::write_atomic(destination, &bytes, 0o600)
                    .map_err(|e| BwError::Other(format!("could not write the export: {e}")))?;
                return Ok(ExportOutcome {
                    path: destination.to_path_buf(),
                    bytes: bytes.len() as u64,
                });
            }
            _ => {
                subprocess::export_raw(
                    &path,
                    format.bw_format(),
                    &session,
                    Duration::from_secs(180),
                )
                .await?
            }
        };
        crate::util::atomic::write_atomic(destination, &bytes, 0o600)
            .map_err(|e| BwError::Other(format!("could not write the export: {e}")))?;
        Ok(ExportOutcome { path: destination.to_path_buf(), bytes: bytes.len() as u64 })
    }
}

/// Spawns the backend task and returns a handle plus the event receiver.
#[must_use]
pub fn spawn(config: BackendConfig) -> (Backend, mpsc::Receiver<BackendEvent>) {
    let (command_tx, mut command_rx) = mpsc::channel::<Command>(64);
    let (event_tx, event_rx) = mpsc::channel::<BackendEvent>(256);
    tokio::spawn(async move {
        let mut service = Service::new(config);
        while let Some(command) = command_rx.recv().await {
            let shutdown = matches!(command.kind, CommandKind::Shutdown);
            let id = command.id;
            let event = service.execute(command.kind).await;
            if event.is_disconnected() {
                // `bw serve` died or closed the socket: restart it once with
                // the cached session key so the next request works again.
                tracing::warn!("bw serve disconnected; restarting it");
                service.client = None;
                if let Err(err) = service.start_serve().await {
                    tracing::warn!(%err, "bw serve restart failed");
                }
            }
            let fatal = matches!(event, BwEvent::Fatal(_));
            if event_tx.send(BackendEvent { id, event }).await.is_err() {
                break;
            }
            if shutdown || fatal {
                // For Shutdown, execute took the serve down already.
                break;
            }
        }
        if let Some(serve) = service.serve.take() {
            serve.shutdown().await;
        }
    });
    (Backend { sender: command_tx }, event_rx)
}

/// A retry wrapper used by the UI for transient disconnections.
pub async fn with_one_restart<T, F, Fut>(mut operation: F) -> Result<T, BwError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, BwError>>,
{
    match operation().await {
        Err(err) if err.is_disconnected() => operation().await,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_format_names() {
        assert_eq!(ExportFormat::Json.bw_format(), "json");
        assert_eq!(ExportFormat::Csv.extension(), "csv");
        assert!(ExportFormat::PasswordProtectedJson.needs_password());
        assert!(ExportFormat::Json.is_plaintext());
        assert_eq!(ExportFormat::parse("account"), Some(ExportFormat::AccountEncryptedJson));
        assert_eq!(ExportFormat::parse("nope"), None);
    }
}
