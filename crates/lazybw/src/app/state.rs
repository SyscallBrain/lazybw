//! The application state tree.
//!
//! `draw` reads this and nothing else; `update` mutates it and returns
//! effects. Every piece of UI state lives here so integration tests can drive
//! the app without a terminal.

use crate::app::actions::ActionId;
use crate::bw::models::{HealthEntry, Item};
use crate::bw::service::VaultSnapshot;
use crate::clipboard::Clipboard;
use crate::config::{Config, SortOrder, Transport};
use crate::input::dispatcher::Dispatcher;
use crate::theme::Theme;
use crate::vault::cache::{Filter, VaultCache};
use crate::vault::health::{HealthCategory, HealthReport};
use crate::vault::query::Query;
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The full-screen state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Connecting to `bw`.
    Starting,
    /// A fatal startup error.
    Error,
    /// Server and method selection.
    Login,
    /// Master password entry.
    Unlock,
    /// The vault browser.
    Vault,
    /// The generator tab.
    Generator,
    /// The health tab.
    Health,
    /// The item editor.
    Editor,
    /// Settings overlay (full screen).
    Settings,
    /// Keybinding editor.
    KeymapEditor,
    /// Help.
    Help,
    /// Message history.
    Messages,
}

impl Screen {
    /// True for the screens that belong to a session phase rather than to
    /// the open vault: they come and go with [`SessionPhase`].
    #[must_use]
    pub fn is_session_screen(self) -> bool {
        matches!(self, Self::Starting | Self::Error | Self::Login | Self::Unlock)
    }

    /// True for the three tabs.
    #[must_use]
    pub fn is_tab(self) -> bool {
        matches!(self, Self::Vault | Self::Generator | Self::Health)
    }
}

/// Which tab is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Vault.
    Vault,
    /// Generator.
    Generator,
    /// Health.
    Health,
}

impl Tab {
    /// 1-based index.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            Self::Vault => 1,
            Self::Generator => 2,
            Self::Health => 3,
        }
    }

    /// Label in the topbar.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Vault => "Vault",
            Self::Generator => "Generator",
            Self::Health => "Health",
        }
    }

    /// The screen for this tab.
    #[must_use]
    pub fn screen(self) -> Screen {
        match self {
            Self::Vault => Screen::Vault,
            Self::Generator => Screen::Generator,
            Self::Health => Screen::Health,
        }
    }
}

/// The focused pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// Sidebar.
    Sidebar,
    /// Item list.
    List,
    /// Detail pane.
    Detail,
}

impl Focus {
    /// The keymap context.
    #[must_use]
    pub fn context(self) -> &'static str {
        match self {
            Self::Sidebar => "sidebar",
            Self::List => "list",
            Self::Detail => "detail",
        }
    }

    /// Focus to the left, if any.
    #[must_use]
    pub fn left(self) -> Self {
        match self {
            Self::Sidebar => Self::Sidebar,
            Self::List => Self::Sidebar,
            Self::Detail => Self::List,
        }
    }

    /// Focus to the right, if any.
    #[must_use]
    pub fn right(self) -> Self {
        match self {
            Self::Sidebar => Self::List,
            Self::List => Self::Detail,
            Self::Detail => Self::Detail,
        }
    }
}

/// The session lifecycle (`docs/bw-integration.md` §3).
///
/// ```text
/// Starting ──▶ LoggedOut ──login──▶ Locked ──unlock──▶ Loading ──vault──▶ Ready
///    │             ▲                 ▲  ▲                 │                 │
///    │             └──── logout ─────┘  └───── lock, auto-lock, "locked" ───┘
///    └──▶ Failed: start-up or fatal CLI error; `r` retries from Starting
/// ```
///
/// Runtime code changes it with [`AppState::enter_session`], which keeps the
/// screen and the decrypted data in memory in step with the phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionPhase {
    /// Connecting to `bw`.
    Starting,
    /// Not logged in.
    LoggedOut,
    /// Logged in, vault locked.
    Locked,
    /// Unlocked, loading the vault.
    Loading,
    /// Unlocked and loaded.
    Ready,
    /// Stopped on an error the user has to act on (retry, edit the config,
    /// quit).
    Failed(Box<ErrorScreen>),
}

impl SessionPhase {
    /// True when the vault is loaded.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// True in the phases where decrypted vault data may be in memory.
    #[must_use]
    pub fn holds_vault_data(&self) -> bool {
        matches!(self, Self::Loading | Self::Ready)
    }

    /// The screen that shows this phase. `None` while the vault is open,
    /// where the user chooses the screen.
    #[must_use]
    pub fn screen(&self) -> Option<Screen> {
        match self {
            Self::Starting => Some(Screen::Starting),
            Self::LoggedOut => Some(Screen::Login),
            Self::Locked => Some(Screen::Unlock),
            Self::Failed(_) => Some(Screen::Error),
            Self::Loading | Self::Ready => None,
        }
    }
}

/// An entry in `:messages`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogMessage {
    /// Time it was logged.
    pub time: DateTime<Utc>,
    /// Severity.
    pub level: MessageLevel,
    /// Text.
    pub text: String,
}

/// Message severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageLevel {
    /// Normal information.
    Info,
    /// Success.
    Ok,
    /// Something to watch.
    Warn,
    /// Failure.
    Error,
}

/// A toast notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// Severity.
    pub level: MessageLevel,
    /// Title.
    pub title: String,
    /// Optional second line.
    pub body: Option<String>,
    /// When it was created.
    pub created: Instant,
}

impl Toast {
    /// How long the toast stays: 3 s, or 6 s for errors.
    #[must_use]
    pub fn lifetime(&self) -> Duration {
        match self.level {
            MessageLevel::Error => Duration::from_secs(6),
            _ => Duration::from_secs(3),
        }
    }
}

/// The yes/no dialog state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmState {
    /// Title.
    pub title: String,
    /// Body lines.
    pub message: String,
    /// The confirm button label.
    pub confirm_label: String,
    /// Danger styling.
    pub danger: bool,
    /// When set, the user must type this word.
    pub requires_word: Option<String>,
    /// What has been typed so far.
    pub input: String,
    /// What runs on confirmation.
    pub on_confirm: Deferred,
    /// When set, `n` runs this instead of cancelling, and `c`/Esc cancel
    /// (three-way dialogs such as "y save · n discard · c cancel").
    pub on_deny: Option<Deferred>,
    /// Delete/rename subfolders as well.
    pub also_subfolders: bool,
    /// How many subfolders the operation would affect.
    pub subfolder_count: usize,
}

impl ConfirmState {
    /// A yes/no question: `on_confirm` runs on `y` or Enter.
    #[must_use]
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        confirm_label: impl Into<String>,
        on_confirm: impl Into<Deferred>,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            confirm_label: confirm_label.into(),
            danger: false,
            requires_word: None,
            input: String::new(),
            on_confirm: on_confirm.into(),
            on_deny: None,
            also_subfolders: false,
            subfolder_count: 0,
        }
    }

    /// Drawn in the danger colour (deletes, logout, discarding changes).
    #[must_use]
    pub fn danger(self) -> Self {
        self.danger_if(true)
    }

    /// Danger styling when `danger` holds.
    #[must_use]
    pub fn danger_if(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }

    /// The user must type `word` to confirm (emptying the trash).
    #[must_use]
    pub fn word(mut self, word: impl Into<String>) -> Self {
        self.requires_word = Some(word.into());
        self
    }

    /// A three-way dialog: `n` runs `action`, `c` and Esc cancel.
    #[must_use]
    pub fn deny(mut self, action: impl Into<Deferred>) -> Self {
        self.on_deny = Some(action.into());
        self
    }

    /// Offers to include `count` subfolders, starting with `also`.
    #[must_use]
    pub fn subfolders(mut self, count: usize, also: bool) -> Self {
        self.subfolder_count = count;
        self.also_subfolders = also;
        self
    }
}

/// The edtui notes editor and its Vim state.
pub struct NotesState {
    /// The edtui editor state.
    pub editor: edtui::EditorState,
}

impl std::fmt::Debug for NotesState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotesState").finish_non_exhaustive()
    }
}

/// A pane divider being dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragTarget {
    /// The sidebar/list divider.
    Sidebar,
    /// The list/detail divider.
    List,
}

/// A pending action: a registry action with its arguments and count.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Action {
    /// Action id.
    pub id: ActionId,
    /// Arguments.
    pub args: Vec<String>,
    /// Count from the key sequence.
    pub count: Option<u32>,
}

impl Action {
    /// Builds an action.
    #[must_use]
    pub fn new(id: ActionId) -> Self {
        Self { id, args: Vec::new(), count: None }
    }

    /// Builds an action with arguments.
    #[must_use]
    pub fn with(id: ActionId, args: &[&str]) -> Self {
        Self { id, args: args.iter().map(|s| (*s).to_owned()).collect(), count: None }
    }

    /// The action a key binding stands for; `None` for an id that is not in
    /// the registry (the config loader refuses those, so only a stale
    /// session binding gets here).
    #[must_use]
    pub fn from_call(call: &crate::config::keymap::ActionCall) -> Option<Self> {
        let id = ActionId::parse(&call.id)?;
        Some(Self { id, args: call.args.clone(), count: None })
    }

    /// Adds a count.
    #[must_use]
    pub fn counted(mut self, count: Option<u32>) -> Self {
        self.count = count;
        self
    }
}

/// What a dialog runs once it is answered: a registry action, or one of the
/// steps only a dialog can start. The steps carry the items and folders the
/// dialog named, so the answer acts on what was shown, whatever the
/// selection is by then; none of them can be bound to a key or typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deferred {
    /// Runs a registry action.
    Run(Action),
    /// Quits, giving up the editor's unsaved changes.
    QuitDiscarding,
    /// Logs out.
    Logout,
    /// Renames the folders of a sidebar row (and, with `also`, the folders
    /// under it). A row can hold several folders: Bitwarden allows two
    /// folders with the same name.
    RenameFolder {
        /// The row's folders, as (id, new full path).
        folders: Vec<(String, String)>,
        /// The folders under it, as (id, new full path).
        subfolders: Vec<(String, String)>,
        /// Rename the subfolders too.
        also: bool,
    },
    /// Deletes the folders of a sidebar row (and, with `also`, the folders
    /// under it).
    DeleteFolder {
        /// The row's folder ids.
        ids: Vec<String>,
        /// The ids of the folders under it.
        subfolders: Vec<String>,
        /// Delete the subfolders too.
        also: bool,
    },
    /// Deletes everything in the trash for good.
    EmptyTrash,
    /// Moves these items to the trash.
    TrashItems(Vec<String>),
    /// Deletes these items for good.
    DeleteItems(Vec<String>),
    /// Opens a link that is not a website.
    OpenUri(String),
    /// Asks to replace this item's password with a new one.
    ConfirmGeneratePassword(String),
    /// Generates a password and puts it in this item.
    GeneratePassword(String),
    /// Asks to put the generator's password in this item.
    ConfirmUseGenerated(String),
    /// Puts the generator's password in this item.
    UseGenerated(String),
    /// Removes the passkey of this item.
    RemovePasskey(String),
}

impl Default for Deferred {
    fn default() -> Self {
        Self::Run(Action::default())
    }
}

impl From<Action> for Deferred {
    fn from(action: Action) -> Self {
        Self::Run(action)
    }
}

impl From<ActionId> for Deferred {
    fn from(id: ActionId) -> Self {
        Self::Run(Action::new(id))
    }
}

/// What a one-line prompt does with the text typed into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptAction {
    /// Nothing (a closed prompt).
    #[default]
    None,
    /// Creates a folder with this name.
    NewFolder,
    /// Renames the current folder to this path.
    RenameFolder,
    /// Filters the help screen.
    FilterHelp,
    /// Filters the keymap editor.
    FilterKeymap,
    /// Reads an SSH private key from this path into the editor.
    ImportSshKey,
    /// Sets this setting to the text.
    Setting(&'static str),
}

/// The unlock screen state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnlockState {
    /// The masked password.
    pub password: crate::input::secret::SecretInput,
    /// Error message.
    pub error: Option<String>,
    /// Shake animation start.
    pub shake: Option<Instant>,
    /// Enter was pressed while the CLI was still starting: unlock as soon as
    /// it reports a locked vault.
    pub submit_when_ready: bool,
}

/// The server choices on the login screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoginServer {
    /// bitwarden.com.
    #[default]
    UnitedStates,
    /// bitwarden.eu.
    EuropeanUnion,
    /// A server URL the user types.
    SelfHosted,
}

impl LoginServer {
    /// In screen order.
    pub const ALL: [Self; 3] = [Self::UnitedStates, Self::EuropeanUnion, Self::SelfHosted];

    /// The next choice down the list, wrapping.
    #[must_use]
    pub fn next(self) -> Self {
        cycle(&Self::ALL, self, true)
    }

    /// The next choice up the list, wrapping.
    #[must_use]
    pub fn prev(self) -> Self {
        cycle(&Self::ALL, self, false)
    }
}

/// The login methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoginMethod {
    /// Email and master password, through `bw login` in the terminal.
    #[default]
    Password,
    /// Single sign-on, through `bw login --sso` in the terminal.
    Sso,
    /// A personal API key, typed into lazybw's own form.
    ApiKey,
}

impl LoginMethod {
    /// In screen order.
    pub const ALL: [Self; 3] = [Self::Password, Self::Sso, Self::ApiKey];

    /// The next choice down the list, wrapping.
    #[must_use]
    pub fn next(self) -> Self {
        cycle(&Self::ALL, self, true)
    }

    /// The next choice up the list, wrapping.
    #[must_use]
    pub fn prev(self) -> Self {
        cycle(&Self::ALL, self, false)
    }
}

/// The part of the login screen that receives keys.
///
/// ```text
/// Server ⇄ Method ──Enter (API key)──▶ ClientId ⇄ ClientSecret
///   │ Enter on "Self-hosted" with no URL      Esc ◀──┘
///   ▼
/// ServerUrl ──Enter/Esc──▶ Server
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoginFocus {
    /// The server list (`j`/`k` choose).
    #[default]
    Server,
    /// Typing the self-hosted server URL.
    ServerUrl,
    /// The method list (`j`/`k` choose).
    Method,
    /// The API key form, client id field.
    ClientId,
    /// The API key form, client secret field.
    ClientSecret,
}

/// The login screen state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoginState {
    /// What receives keys.
    pub focus: LoginFocus,
    /// The chosen server.
    pub server: LoginServer,
    /// Self-hosted URL input.
    pub server_input: String,
    /// The chosen method.
    pub method: LoginMethod,
    /// API key form: client id.
    pub client_id: String,
    /// API key form: client secret.
    pub client_secret: crate::input::secret::SecretInput,
    /// Error message.
    pub error: Option<String>,
    /// The server URL `bw` was last told to use from this screen. `None`
    /// until then: what `bw` has configured is not known while logged out.
    pub configured_server: Option<String>,
    /// The server URL being sent to `bw` right now.
    pub configuring_server: Option<String>,
    /// Start the chosen login method once `bw` has accepted the server.
    pub login_after_server: bool,
}

impl LoginState {
    /// True when `bw` still has to be told about the chosen server.
    #[must_use]
    pub fn server_needs_configuring(&self) -> bool {
        self.configured_server.as_deref() != Some(self.server_url())
    }

    /// The chosen server URL.
    #[must_use]
    pub fn server_url(&self) -> &str {
        match self.server {
            LoginServer::UnitedStates => "https://vault.bitwarden.com",
            LoginServer::EuropeanUnion => "https://vault.bitwarden.eu",
            LoginServer::SelfHosted => self.server_input.trim(),
        }
    }
}

/// The entry after (or before) `current` in `all`, wrapping around.
fn cycle<T: Copy + PartialEq>(all: &[T], current: T, forward: bool) -> T {
    let index = all.iter().position(|item| *item == current).unwrap_or(0);
    let next = if forward { index + 1 } else { index + all.len() - 1 };
    all[next % all.len()]
}

/// The startup error screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorScreen {
    /// Title.
    pub title: String,
    /// Body lines.
    pub lines: Vec<String>,
    /// Extra rows (stderr tail).
    pub detail: Vec<String>,
    /// Actions shown at the bottom.
    pub actions: String,
    /// True when retrying makes sense.
    pub retry: bool,
    /// Extra action label for the config error (`e` opens the file).
    pub open_file: Option<PathBuf>,
}

/// The search state of the list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchState {
    /// Input is focused.
    pub active: bool,
    /// The typed query.
    pub input: String,
    /// Parsed query.
    pub query: Option<Query>,
    /// The current cursor position in the filtered list.
    pub index: usize,
}

/// The detail pane state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DetailState {
    /// Selected row.
    pub row: usize,
    /// Scroll offset.
    pub offset: usize,
    /// Revealed fields.
    pub revealed: BTreeSet<String>,
    /// Deadline for re-hiding.
    pub hide_at: Option<Instant>,
}

/// One row of the editor form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormRow {
    /// What field this edits.
    pub key: FormKey,
    /// Label.
    pub label: String,
    /// How it is edited.
    pub kind: FormKind,
    /// Section label shown above (only on the first row of a section).
    pub section: Option<&'static str>,
    /// Right-aligned hint.
    pub hint: Option<String>,
}

/// The value kind of a form row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormKind {
    /// Free text.
    Text,
    /// Masked text.
    Secret,
    /// Multi-line notes.
    Notes,
    /// A boolean toggled with Enter.
    Bool,
    /// A fixed set of options cycled with Ctrl-a/Ctrl-x.
    Option(Vec<String>),
    /// A read-only value.
    ReadOnly,
    /// A button.
    Action,
}

/// Keys identifying editor rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FormKey {
    /// Item name.
    Name,
    /// Folder.
    Folder,
    /// Notes.
    Notes,
    /// Login username.
    Username,
    /// Login password.
    Password,
    /// Authenticator key.
    Totp,
    /// Favourite.
    Favorite,
    /// Re-prompt.
    Reprompt,
    /// URI row `n`.
    Uri(usize),
    /// URI match `n`.
    UriMatch(usize),
    /// Add a URI.
    UriAdd,
    /// Custom field name `n`.
    FieldName(usize),
    /// Custom field value `n`.
    FieldValue(usize),
    /// Custom field type `n`.
    FieldType(usize),
    /// Add a custom field.
    FieldAdd,
    /// Cardholder.
    Cardholder,
    /// Card number.
    CardNumber,
    /// Card brand.
    CardBrand,
    /// Expiry month.
    ExpMonth,
    /// Expiry year.
    ExpYear,
    /// Security code.
    CardCode,
    /// Identity title.
    IdTitle,
    /// First name.
    IdFirst,
    /// Middle name.
    IdMiddle,
    /// Last name.
    IdLast,
    /// Username.
    IdUsername,
    /// Company.
    IdCompany,
    /// Email.
    IdEmail,
    /// Phone.
    IdPhone,
    /// Address line 1.
    IdAddress1,
    /// Address line 2.
    IdAddress2,
    /// Address line 3.
    IdAddress3,
    /// City.
    IdCity,
    /// State.
    IdState,
    /// Postal code.
    IdPostal,
    /// Country.
    IdCountry,
    /// SSN.
    IdSsn,
    /// Passport number.
    IdPassport,
    /// Licence number.
    IdLicense,
    /// Generate an SSH key.
    SshGenerate,
    /// Import an SSH key.
    SshImport,
    /// Public key.
    SshPublic,
    /// Fingerprint.
    SshFingerprint,
    /// Private key.
    SshPrivate,
}

impl FormKey {
    /// Rows drawn in the right column of the two-column editor: the lists
    /// (websites, custom fields) and the notes.
    #[must_use]
    pub fn in_right_column(self) -> bool {
        matches!(
            self,
            Self::Uri(_)
                | Self::UriMatch(_)
                | Self::UriAdd
                | Self::FieldName(_)
                | Self::FieldValue(_)
                | Self::FieldType(_)
                | Self::FieldAdd
                | Self::Notes
        )
    }
}

/// The editor state.
#[derive(Debug, Clone)]
pub struct EditorState {
    /// The item being edited.
    pub item: Item,
    /// True for a new item.
    pub is_new: bool,
    /// The original item, for undo and reset.
    pub original_item: Item,
    /// Rows.
    pub rows: Vec<FormRow>,
    /// Selected row.
    pub row: usize,
    /// Scroll offset.
    pub offset: usize,
    /// Editing (INSERT) or not.
    pub insert: bool,
    /// Cursor position in the active value (char index).
    pub cursor: usize,
    /// Field revealed.
    pub revealed: bool,
    /// Dirty.
    pub dirty: bool,
    /// Validation errors by row index.
    pub errors: BTreeMap<usize, String>,
    /// Scroll offset of the notes editor.
    pub notes_offset: usize,
    /// Remote change banner.
    pub remote_change: bool,
    /// Undo and redo for the form (`u`, `Ctrl-r`).
    pub history: EditorHistory,
}

/// A copy of the form, for undo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSnapshot {
    /// The item.
    pub item: Item,
    /// The rows (the item's shape when the snapshot was taken).
    pub rows: Vec<FormRow>,
    /// The selected row.
    pub row: usize,
}

/// Vim-style undo for the editor: every change is a step, and one INSERT
/// session (from `i` to `Esc`) is a single step.
#[derive(Debug, Clone, Default)]
pub struct EditorHistory {
    /// States to go back to, oldest first.
    pub undo: Vec<EditorSnapshot>,
    /// States undone, to redo.
    pub redo: Vec<EditorSnapshot>,
    /// Undo depth at the last save (the item is clean at this depth).
    pub saved_depth: usize,
    /// The current INSERT session already recorded its starting state.
    pub insert_recorded: bool,
    /// Set while undo/redo runs, so the change is not recorded again.
    pub restoring: bool,
    /// Bumped on every change, undo and redo: tells whether the form changed
    /// after a save was sent.
    pub revision: u64,
}

impl EditorState {
    /// The current form, for the undo history.
    #[must_use]
    pub fn snapshot(&self) -> EditorSnapshot {
        EditorSnapshot { item: self.item.clone(), rows: self.rows.clone(), row: self.row }
    }

    /// Puts a snapshot back.
    pub fn restore(&mut self, snapshot: EditorSnapshot) {
        self.item = snapshot.item;
        self.rows = snapshot.rows;
        self.row = snapshot.row.min(self.rows.len().saturating_sub(1));
        self.dirty = self.history.undo.len() != self.history.saved_depth || self.is_new;
    }
}

/// The generator state.
#[derive(Debug, Clone, Default)]
pub struct GeneratorState {
    /// Current options.
    pub request: crate::bw::models::GenerateRequest,
    /// The generated value.
    pub output: String,
    /// Session history: value and when.
    pub history: Vec<(String, Instant)>,
    /// Selected row in the options list.
    pub row: usize,
    /// Popup mode (from the editor).
    pub popup: bool,
    /// The last error.
    pub error: Option<String>,
    /// Regenerate after the current request.
    pub pending: bool,
}

/// The health tab state.
#[derive(Debug, Clone, Default)]
pub struct HealthState {
    /// The latest report.
    pub report: Option<HealthReport>,
    /// Selected category.
    pub category: HealthCategory,
    /// Selected row in the category list.
    pub index: usize,
    /// Offset.
    pub offset: usize,
    /// The raw `bw` entries, kept for a re-render.
    pub raw: Vec<HealthEntry>,
    /// True when the CLI is new enough for exposure data.
    pub exposure_available: bool,
}

/// The quick-open finder state.
#[derive(Debug, Clone, Default)]
pub struct QuickOpenState {
    /// Query input.
    pub input: String,
    /// Cursor.
    pub cursor: usize,
    /// Matches: item ids in ranked order.
    pub results: Vec<String>,
    /// Selected result.
    pub index: usize,
    /// Scroll offset.
    pub offset: usize,
    /// Health flags, if a health run exists.
    pub ignored_health: bool,
}

/// A generic picker entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerEntry {
    /// Display label.
    pub label: String,
    /// Right-aligned note.
    pub note: String,
    /// Searchable text.
    pub filter: String,
    /// The value handed to the action.
    pub value: String,
    /// An accelerator key.
    pub accelerator: Option<char>,
}

/// What a picker is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    /// New item type.
    ItemType,
    /// Folder (move the target items, or the editor's folder field).
    Folder,
    /// Folder to open as the list filter (`<leader>ff`, the rail icon).
    FolderJump,
    /// Theme.
    Theme,
    /// Custom field type.
    FieldType,
    /// URI match detection.
    UriMatch,
    /// Identity title.
    IdentityTitle,
    /// Card brand.
    CardBrand,
    /// Card expiry month.
    ExpMonth,
}

/// A picker overlay.
#[derive(Debug, Clone)]
pub struct PickerState {
    /// Title.
    pub title: String,
    /// What it selects.
    pub kind: PickerKind,
    /// All entries.
    pub entries: Vec<PickerEntry>,
    /// Indices after filtering.
    pub filtered: Vec<usize>,
    /// Filter input.
    pub filter: String,
    /// Selected filtered position.
    pub index: usize,
    /// Offset.
    pub offset: usize,
    /// True when it was opened from the editor (value goes into a row).
    pub from_editor: bool,
}

impl PickerState {
    /// Builds a picker from entries.
    #[must_use]
    pub fn new(title: impl Into<String>, kind: PickerKind, entries: Vec<PickerEntry>) -> Self {
        let filtered = (0..entries.len()).collect();
        Self {
            title: title.into(),
            kind,
            entries,
            filtered,
            filter: String::new(),
            index: 0,
            offset: 0,
            from_editor: false,
        }
    }

    /// Applies the current filter.
    pub fn refilter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| needle.is_empty() || e.filter.to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect();
        self.index = self.index.min(self.filtered.len().saturating_sub(1));
    }

    /// The selected entry.
    #[must_use]
    pub fn selected(&self) -> Option<&PickerEntry> {
        self.filtered.get(self.index).and_then(|i| self.entries.get(*i))
    }
}

/// The `:` command line state.
#[derive(Debug, Clone, Default)]
pub struct CmdlineState {
    /// The typed text (without the colon).
    pub input: String,
    /// Cursor.
    pub cursor: usize,
    /// Completions.
    pub completions: Vec<String>,
    /// Selected completion.
    pub completion_index: usize,
    /// Which completion is shown.
    pub show_completions: bool,
    /// The history entry shown (`Up`/`Down`), counted from the newest.
    pub history_index: Option<usize>,
    /// What was typed before going through the history.
    pub draft: String,
}

/// Password history popup.
#[derive(Debug, Clone, Default)]
pub struct HistoryState {
    /// Item id.
    pub item_id: String,
    /// Passwords.
    pub entries: Vec<crate::bw::models::PasswordHistory>,
    /// Selected.
    pub index: usize,
    /// Revealed.
    pub revealed: BTreeSet<usize>,
    /// Newest password, shown first.
    pub current: Option<String>,
}

/// The item info popup.
#[derive(Debug, Clone, Default)]
pub struct InfoState {
    /// Item id.
    pub item_id: String,
    /// Lines.
    pub lines: Vec<(String, String)>,
}

/// The keymap editor state.
#[derive(Debug, Clone, Default)]
pub struct KeymapEditorState {
    /// Selected context (`""` = All).
    pub context_index: usize,
    /// Filter input.
    pub filter: String,
    /// Selected row.
    pub index: usize,
    /// Offset.
    pub offset: usize,
    /// Capture mode for a rebind.
    pub capture: Option<CaptureState>,
    /// Contexts in display order.
    pub contexts: Vec<String>,
    /// Unsaved changes.
    pub dirty: bool,
}

/// A pending key capture.
#[derive(Debug, Clone, Default)]
pub struct CaptureState {
    /// Action id.
    pub action: String,
    /// Context.
    pub context: String,
    /// Captured keys in notation.
    pub keys: Vec<String>,
    /// Conflict message.
    pub conflict: Option<String>,
    /// True when adding a key rather than replacing.
    pub adding: bool,
}

/// The settings overlay state.
#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    /// Selected row.
    pub index: usize,
    /// Offset.
    pub offset: usize,
}

/// Where the export wizard is: each value is a step together with the
/// field that has focus in it, so keys can only reach the field on screen.
///
/// ```text
/// Format ⇄ Destination ──Enter──▶ FilePassword ⇄ ConfirmPassword ──Enter──▶ MasterPassword
///        └───────────────Enter (no file password)─────────────────────────▲
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportStep {
    /// Step one, the format list (`j`/`k` choose).
    #[default]
    Format,
    /// Step one, the destination path (`Tab` completes it).
    Destination,
    /// Step two, the file password (password-protected JSON only).
    FilePassword,
    /// Step two, the file password again.
    ConfirmPassword,
    /// The last step: the master password, then the export runs.
    MasterPassword,
}

impl ExportStep {
    /// True on the first step (format and destination).
    #[must_use]
    pub fn choosing_format(self) -> bool {
        matches!(self, Self::Format | Self::Destination)
    }

    /// True on the file password step.
    #[must_use]
    pub fn choosing_password(self) -> bool {
        matches!(self, Self::FilePassword | Self::ConfirmPassword)
    }
}

/// The export wizard state.
#[derive(Debug, Clone, Default)]
pub struct ExportState {
    /// The step, and the field in it that has focus.
    pub step: ExportStep,
    /// Format.
    pub format: crate::bw::service::ExportFormat,
    /// Destination path input.
    pub path: String,
    /// File password.
    pub password: crate::input::secret::SecretInput,
    /// Confirmation.
    pub confirmation: crate::input::secret::SecretInput,
    /// Master password.
    pub master: crate::input::secret::SecretInput,
    /// Error.
    pub error: Option<String>,
    /// The existing file the user agreed to replace (a second Enter).
    pub overwrite: Option<String>,
}

impl ExportState {
    /// Types text into the focused field. The format list takes none.
    pub fn insert(&mut self, text: &str) {
        match self.step {
            ExportStep::Format => {}
            ExportStep::Destination => self.path.push_str(text),
            ExportStep::FilePassword => self.password.push_str(text),
            ExportStep::ConfirmPassword => self.confirmation.push_str(text),
            ExportStep::MasterPassword => self.master.push_str(text),
        }
    }

    /// Deletes the last character of the focused field.
    pub fn backspace(&mut self) {
        match self.step {
            ExportStep::Format => None,
            ExportStep::Destination => self.path.pop(),
            ExportStep::FilePassword => self.password.pop(),
            ExportStep::ConfirmPassword => self.confirmation.pop(),
            ExportStep::MasterPassword => self.master.pop(),
        };
    }
}

/// Where the import wizard is. The file step is split by the field that
/// has focus; the preview step carries what it shows, so it cannot outlive
/// the step.
///
/// ```text
/// Format ──Enter──▶ Path ⇄ Password ──Enter──▶ Preview (Bitwarden JSON) ──Enter──▶ import
///   ▲                 │                  └──────Enter (other formats)─────────────▲
///   └──────Esc────────┘◀───────────────Esc──────────┘
/// ```
#[derive(Debug, Clone, Default)]
pub enum ImportStep {
    /// Step one, the filterable format list.
    #[default]
    Format,
    /// Step two, the file path.
    Path,
    /// Step two, the file password (password-protected Bitwarden JSON).
    Password,
    /// Step three, what a Bitwarden JSON file would add.
    Preview(ImportPreview),
}

impl ImportStep {
    /// The step number shown in the corner of the wizard, from 1.
    #[must_use]
    pub fn number(&self) -> usize {
        match self {
            Self::Format => 1,
            Self::Path | Self::Password => 2,
            Self::Preview(_) => 3,
        }
    }
}

/// The preview of a Bitwarden JSON file, checked against the vault.
#[derive(Debug, Clone, Default)]
pub struct ImportPreview {
    /// The items in the file.
    pub items: Vec<Item>,
    /// The folders in the file.
    pub folders: Vec<crate::bw::models::Folder>,
    /// Folder names in the file that the vault does not have yet.
    pub new_folders: Vec<String>,
    /// Items that look like ones already in the vault.
    pub duplicates: Vec<crate::vault::dupes::Duplicate>,
    /// Duplicates (indices into `items`) checked for skipping.
    pub skip: BTreeSet<usize>,
    /// Skip the checked duplicates; off imports everything.
    pub skip_enabled: bool,
    /// Selected duplicate row.
    pub index: usize,
}

impl ImportPreview {
    /// How many duplicates the import leaves out.
    #[must_use]
    pub fn skipped(&self) -> usize {
        if self.skip_enabled { self.skip.len() } else { 0 }
    }
}

/// The import wizard state.
#[derive(Debug, Clone, Default)]
pub struct ImportState {
    /// The step, and the field in it that has focus.
    pub step: ImportStep,
    /// Format.
    pub format: String,
    /// Path input.
    pub path: String,
    /// File password.
    pub password: crate::input::secret::SecretInput,
    /// Error.
    pub error: Option<String>,
    /// Every format the CLI accepts, featured ones first.
    pub formats: Vec<String>,
    /// Filter typed in the format step.
    pub filter: String,
    /// Selected row among the filtered formats.
    pub format_index: usize,
}

impl ImportState {
    /// The preview, on the preview step.
    #[must_use]
    pub fn preview(&self) -> Option<&ImportPreview> {
        match &self.step {
            ImportStep::Preview(preview) => Some(preview),
            _ => None,
        }
    }

    /// True when the chosen format needs a file password.
    #[must_use]
    pub fn needs_password(&self) -> bool {
        self.format == "bitwardenpasswordprotected"
    }

    /// The formats matching the filter.
    #[must_use]
    pub fn filtered_formats(&self) -> Vec<&str> {
        let needle = self.filter.to_lowercase();
        self.formats
            .iter()
            .map(String::as_str)
            .filter(|f| needle.is_empty() || f.to_lowercase().contains(&needle))
            .collect()
    }
}

/// A transient which-key popup.
#[derive(Debug, Clone)]
pub struct WhichKeyState {
    /// Pending key prefix in display form.
    pub prefix: String,
    /// Continuations: key, is_group, label.
    pub continuations: Vec<(String, bool, String)>,
    /// When it appeared.
    pub since: Instant,
}

/// The undo stack entry.
#[derive(Debug, Clone)]
pub enum UndoOp {
    /// Move to trash.
    Trash {
        /// Item ids.
        ids: Vec<String>,
    },
    /// Restore.
    Restore {
        /// Item ids.
        ids: Vec<String>,
    },
    /// Edit an item.
    Edit {
        /// Item id.
        id: String,
        /// Full previous item.
        before: Box<Item>,
        /// Full new item.
        after: Box<Item>,
    },
    /// Create.
    Create {
        /// The created item, as returned by the server.
        item: Box<Item>,
    },
    /// Favorite toggle.
    Favorite {
        /// Ids.
        ids: Vec<String>,
        /// Values before the change.
        before: Vec<bool>,
        /// Values after the change.
        after: Vec<bool>,
    },
    /// Move to folder.
    Move {
        /// Item id, previous folder and target folder.
        items: Vec<(String, Option<String>, Option<String>)>,
    },
    /// Folder creation.
    FolderCreate {
        /// Folder name/path.
        name: String,
    },
    /// Folder rename.
    FolderRename {
        /// Folder id, old path and new path.
        folders: Vec<(String, String, String)>,
    },
    /// Folder delete, with the items that lived in each folder.
    FolderDelete {
        /// Deleted folders and their former item ids.
        folders: Vec<DeletedFolder>,
    },
}

impl UndoOp {
    /// True when the entry changes the item with this id.
    #[must_use]
    pub fn touches(&self, id: &str) -> bool {
        match self {
            Self::Trash { ids } | Self::Restore { ids } | Self::Favorite { ids, .. } => {
                ids.iter().any(|i| i == id)
            }
            Self::Edit { id: edited, .. } => edited == id,
            Self::Create { item } => item.id.as_deref() == Some(id),
            Self::Move { items } => items.iter().any(|(i, _, _)| i == id),
            _ => false,
        }
    }
}

/// A folder removed by a delete operation, kept for undo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedFolder {
    /// Full folder path.
    pub name: String,
    /// Items that were in this folder.
    pub items: Vec<String>,
}

/// The in-flight recreation of deleted folders (undo).
#[derive(Debug, Clone, Default)]
pub struct FolderRestore {
    /// Folders still to recreate, parents first.
    pub folders: Vec<DeletedFolder>,
    /// New id per recreated folder name.
    pub created: BTreeMap<String, String>,
    /// The folder whose creation is pending.
    pub pending: Option<String>,
}

impl FolderRestore {
    /// The next folder to create, if any.
    #[must_use]
    pub fn next(&self) -> Option<&DeletedFolder> {
        self.folders.iter().find(|folder| !self.created.contains_key(&folder.name))
    }
}

impl UndoOp {
    /// The message used in the toast.
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            Self::Trash { ids } if ids.len() == 1 => "moved 1 item to Trash".to_owned(),
            Self::Trash { ids } => format!("moved {} items to Trash", ids.len()),
            Self::Restore { ids } if ids.len() == 1 => "restored 1 item".to_owned(),
            Self::Restore { ids } => format!("restored {} items", ids.len()),
            Self::Edit { .. } => "edited an item".to_owned(),
            Self::Create { .. } => "created an item".to_owned(),
            Self::Favorite { ids, .. } => format!("changed {} favourite(s)", ids.len()),
            Self::Move { items } => format!("moved {} item(s)", items.len()),
            Self::FolderCreate { .. } => "created a folder".to_owned(),
            Self::FolderRename { folders } => format!("renamed {} folder(s)", folders.len()),
            Self::FolderDelete { folders } => format!("deleted {} folder(s)", folders.len()),
        }
    }
}

/// The undo/redo stacks.
#[derive(Debug, Clone, Default)]
pub struct UndoStack {
    /// Undo entries, newest last.
    pub undo: Vec<UndoOp>,
    /// Redo entries, newest last.
    pub redo: Vec<UndoOp>,
}

impl UndoStack {
    /// Maximum entries kept.
    pub const LIMIT: usize = 100;

    /// Pushes a new undo entry and clears redo.
    pub fn push(&mut self, op: UndoOp) {
        self.undo.push(op);
        if self.undo.len() > Self::LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Pops an undo entry.
    pub fn pop_undo(&mut self) -> Option<UndoOp> {
        self.undo.pop()
    }

    /// Pops a redo entry.
    pub fn pop_redo(&mut self) -> Option<UndoOp> {
        self.redo.pop()
    }

    /// Drops the newest entry (in either stack) that involves an item whose
    /// change failed on the server: it describes something that did not
    /// happen, and replaying it would act on a false premise.
    pub fn forget(&mut self, id: &str) {
        for stack in [&mut self.undo, &mut self.redo] {
            if let Some(index) = stack.iter().rposition(|op| op.touches(id)) {
                stack.remove(index);
            }
        }
    }

    /// Clears both stacks (on lock).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

/// The jump list.
#[derive(Debug, Clone, Default)]
pub struct JumpList {
    /// Entries.
    pub entries: Vec<JumpEntry>,
    /// Cursor.
    pub index: usize,
}

/// One jump target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpEntry {
    /// Item id.
    pub item_id: String,
}

impl JumpList {
    /// How many entries are kept.
    pub const LIMIT: usize = 100;

    /// Records a jump to an item, truncating the forward history.
    pub fn push(&mut self, item_id: impl Into<String>) {
        self.entries.truncate(self.index + 1);
        self.entries.push(JumpEntry { item_id: item_id.into() });
        if self.entries.len() > Self::LIMIT {
            self.entries.remove(0);
        }
        self.index = self.entries.len().saturating_sub(1);
    }

    /// Goes back.
    pub fn back(&mut self) -> Option<&JumpEntry> {
        if self.index > 0 {
            self.index -= 1;
            self.entries.get(self.index)
        } else {
            None
        }
    }

    /// Goes forward.
    pub fn forward(&mut self) -> Option<&JumpEntry> {
        if self.index + 1 < self.entries.len() {
            self.index += 1;
            self.entries.get(self.index)
        } else {
            None
        }
    }
}

/// A pending backend request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    /// Starting `bw serve`.
    Starting,
    /// Unlocking.
    Unlock,
    /// Checking the password typed into the re-prompt dialog; the dialog
    /// holds the action to run (see [`RepromptState::request`]).
    Reprompt,
    /// Loading the vault.
    Load,
    /// Syncing.
    Sync,
    /// Locking.
    Lock,
    /// Writing an item.
    Save {
        /// Item id, when editing.
        id: Option<String>,
        /// Close the editor on success.
        close: bool,
        /// For a save from the editor, the form revision that was sent; the
        /// answer only touches the editor if it still shows that item.
        /// `None` for favourites, moves, undo and other background writes.
        editor: Option<u64>,
    },
    /// Deleting an item.
    Delete {
        /// The item id.
        id: String,
        /// Permanent.
        permanent: bool,
    },
    /// Restoring.
    Restore {
        /// The item id.
        id: String,
    },
    /// A folder operation.
    Folder,
    /// Generating.
    Generate {
        /// Keep the password in the session history.
        record: bool,
        /// Copy it when it arrives.
        copy: bool,
    },
    /// Loading an item.
    GetItem,
    /// Exporting.
    Export,
    /// Listing import formats.
    ImportFormats,
    /// Importing.
    Import,
    /// Health.
    Health,
    /// Logging in.
    Login,
    /// Logging out.
    Logout,
    /// Server configuration.
    ConfigServer,
}

/// The modal popup drawn over the current screen.
///
/// Only one is open at a time, so it is one value rather than a field per
/// popup: an overlay that runs an action closes itself first, and opening
/// one replaces whatever was open. While an overlay is open it receives
/// every key (`update::on_key`), its keymap context leads the chain
/// ([`AppState::contexts`]) and `Esc` closes it.
#[derive(Debug, Clone)]
pub enum Overlay {
    /// The fuzzy finder (`Ctrl-p`).
    QuickOpen(QuickOpenState),
    /// The `:` command line.
    Cmdline(CmdlineState),
    /// A list to pick from (folder, item type, theme, …).
    Picker(PickerState),
    /// A yes/no question.
    Confirm(ConfirmState),
    /// The master password re-prompt for a protected item.
    Reprompt(RepromptState),
    /// A one-line text prompt (new folder name, …).
    Prompt(PromptState),
    /// The password history of an item.
    History(HistoryState),
    /// Item details (`:info`).
    Info(InfoState),
    /// The export wizard.
    Export(ExportState),
    /// The import wizard.
    Import(ImportState),
}

impl Overlay {
    /// The keymap context that leads the chain while this overlay is open.
    /// The wizards read keys directly and have none.
    #[must_use]
    pub fn context(&self) -> Option<&'static str> {
        match self {
            Self::QuickOpen(_) => Some("quick_open"),
            Self::Cmdline(_) | Self::Prompt(_) => Some("cmdline"),
            Self::Picker(_) => Some("picker"),
            Self::Confirm(_) => Some("confirm"),
            Self::Reprompt(_) => Some("unlock"),
            Self::History(_) | Self::Info(_) => Some("help"),
            Self::Export(_) | Self::Import(_) => None,
        }
    }
}

impl AppState {
    /// Opens an overlay, replacing whatever overlay was open.
    pub fn open(&mut self, overlay: impl Into<Overlay>) {
        self.overlay = Some(overlay.into());
    }
}

/// Per-kind access to the open overlay: `picker()` reads it, `take_picker()`
/// closes it and hands it back (and leaves any other kind alone), and
/// `PickerState` converts into an [`Overlay`] for [`AppState::open`].
/// Code that changes an overlay in place matches on [`AppState::overlay`],
/// so the borrow stays on that one field.
macro_rules! overlay_kinds {
    ($($get:ident, $take:ident: $variant:ident($ty:ty)),* $(,)?) => {
        impl AppState {
            $(
                #[doc = concat!("The `", stringify!($variant), "` overlay, if that is the one open.")]
                #[must_use]
                pub fn $get(&self) -> Option<&$ty> {
                    match &self.overlay {
                        Some(Overlay::$variant(inner)) => Some(inner),
                        _ => None,
                    }
                }

                #[doc = concat!("Closes the `", stringify!($variant), "` overlay and returns it; any other overlay stays open.")]
                pub fn $take(&mut self) -> Option<$ty> {
                    match self.overlay.take() {
                        Some(Overlay::$variant(inner)) => Some(inner),
                        other => {
                            self.overlay = other;
                            None
                        }
                    }
                }
            )*
        }

        $(
            impl From<$ty> for Overlay {
                fn from(inner: $ty) -> Self {
                    Self::$variant(inner)
                }
            }
        )*
    };
}

overlay_kinds! {
    quick_open, take_quick_open: QuickOpen(QuickOpenState),
    cmdline, take_cmdline: Cmdline(CmdlineState),
    picker, take_picker: Picker(PickerState),
    confirm, take_confirm: Confirm(ConfirmState),
    reprompt, take_reprompt: Reprompt(RepromptState),
    prompt, take_prompt: Prompt(PromptState),
    history, take_history: History(HistoryState),
    info, take_info: Info(InfoState),
    export, take_export: Export(ExportState),
    import, take_import: Import(ImportState),
}

/// The whole application state.
#[derive(Debug)]
pub struct AppState {
    /// Configuration.
    pub config: Config,
    /// Active theme.
    pub theme: Theme,
    /// Keymap.
    pub keymap: crate::config::keymap::Keymap,
    /// Key dispatcher.
    pub dispatcher: Dispatcher,
    /// Current full-screen view.
    pub screen: Screen,
    /// Active tab.
    pub tab: Tab,
    /// Focused pane.
    pub focus: Focus,
    /// Session phase.
    pub session: SessionPhase,
    /// Vault cache.
    pub cache: VaultCache,
    /// Current filter.
    pub filter: Filter,
    /// Selected sidebar row.
    pub sidebar_row: usize,
    /// Collapsed folders.
    pub collapsed: BTreeSet<String>,
    /// Whether the sidebar is shown.
    pub sidebar_visible: bool,
    /// Selected item index in the list.
    pub list_row: usize,
    /// Marks for bulk actions.
    pub marks: BTreeSet<String>,
    /// Visual mode anchor.
    pub visual_anchor: Option<usize>,
    /// Search.
    pub search: SearchState,
    /// Detail.
    pub detail: DetailState,
    /// Editor.
    pub editor: Option<EditorState>,
    /// Generator.
    pub generator: GeneratorState,
    /// Health.
    pub health: HealthState,
    /// The modal popup over the screen, if any. See [`Overlay`].
    pub overlay: Option<Overlay>,
    /// Which-key.
    pub which_key: Option<WhichKeyState>,
    /// Settings.
    pub settings: SettingsState,
    /// Keymap editor.
    pub keymap_editor: KeymapEditorState,
    /// Help filter.
    pub help_filter: String,
    /// The key contexts that were active when help opened.
    pub help_chain: Vec<String>,
    /// Help scroll.
    pub help_offset: usize,
    /// Messages scroll.
    pub messages_offset: usize,
    /// Unlock screen.
    pub unlock: UnlockState,
    /// Login screen.
    pub login: LoginState,
    /// Clipboard.
    pub clipboard: Clipboard,
    /// What was copied, for the message.
    pub clipboard_label: Option<String>,
    /// Toast stack.
    pub toasts: Vec<Toast>,
    /// `:messages` log.
    pub messages: Vec<LogMessage>,
    /// Command lines run with `:`, oldest first; kept between sessions.
    pub command_history: Vec<String>,
    /// Message shown on the message line.
    pub status_message: Option<LogMessage>,
    /// Which-key / statusline showcmd.
    pub showcmd: String,
    /// Account email.
    pub email: Option<String>,
    /// Server URL.
    pub server: Option<String>,
    /// Last sync.
    pub last_sync: Option<DateTime<Utc>>,
    /// Pending backend requests.
    pub pending: BTreeMap<u64, Pending>,
    /// Next correlation id.
    pub next_request_id: u64,
    /// Undo stack.
    pub undo: UndoStack,
    /// Jump list.
    pub jump: JumpList,
    /// The item yanked with `Y`, pasted as a copy with `p`.
    pub yanked_item: Option<Item>,
    /// Set when lazybw should exit after this message.
    pub quit: bool,
    /// The user asked to quit while changes were still on their way to
    /// Bitwarden: quit once they have all been answered.
    pub quit_when_saved: bool,
    /// Dirty flag for redraw.
    pub dirty: bool,
    /// Repaint every cell on the next draw (`<C-l>`), not just the diff.
    pub full_redraw: bool,
    /// The saved folder folds were applied after the first vault load.
    pub folds_restored: bool,
    /// The configured theme while a theme preview runs (picker, `:theme`).
    pub theme_preview_from: Option<String>,
    /// Terminal size (cols, rows).
    pub size: (u16, u16),
    /// Fixed clock.
    pub now: DateTime<Utc>,
    /// Startup time.
    pub started: Instant,
    /// Paths.
    pub paths: crate::config::Paths,
    /// Located CLI path.
    pub bw_path: Option<PathBuf>,
    /// Capabilities.
    pub capabilities: Option<crate::bw::locate::Capabilities>,
    /// Transport.
    pub transport: Transport,
    /// Terminals key enhancement.
    pub keyboard_enhanced: bool,
    /// Last lock activity.
    pub last_input: Instant,
    /// The same moment on the wall clock. `Instant` stops while the machine
    /// sleeps, so on its own a laptop closed for a night would come back
    /// still unlocked; the auto-lock counts whichever clock moved further.
    pub last_input_wall: std::time::SystemTime,
    /// When the last successful sync finished.
    pub last_sync_at: Option<Instant>,
    /// Item whose master password re-prompt just succeeded; consumed by the
    /// action that asked for it.
    pub reprompt_once: Option<String>,
    /// Item whose re-prompt stays valid until the instant (`reprompt_grace`).
    pub reprompt_passed: Option<(String, Instant)>,
    /// The item the detail pane was showing; revealed fields re-hide when it
    /// changes.
    pub detail_item: Option<String>,
    /// (imported, skipped) for the message after the running import.
    pub last_import: Option<(usize, usize)>,
    /// Item id waiting for a generated password.
    pub pending_generate_target: Option<String>,
    /// Animation manager.
    pub fx: std::cell::RefCell<crate::ui::fx::Fx>,
    /// Scroll offsets the renderer settled on, per view, so scrolling is
    /// stable and mouse clicks map to the rows actually on screen.
    pub ui_scroll: std::cell::RefCell<BTreeMap<&'static str, usize>>,
    /// How many rows each view showed in the last frame, for page-sized
    /// motions (`Ctrl-d`, `Ctrl-f`, `H`, `M`, `L`).
    pub ui_pages: std::cell::RefCell<BTreeMap<&'static str, usize>>,
    /// The edtui notes editor, open over the form.
    pub notes: Option<std::cell::RefCell<NotesState>>,
    /// Width of the sidebar while the user drags the divider.
    pub sidebar_width: Option<u16>,
    /// Width of the list pane while the user drags the divider.
    pub list_width: Option<u16>,
    /// Which divider is being dragged.
    pub drag: Option<DragTarget>,
    /// The last TOTP code seen, to animate changes.
    pub last_totp_code: Option<String>,
    /// In-flight recreation of deleted folders.
    pub folder_restore: Option<FolderRestore>,
    /// Reverse the current sort order.
    pub sort_reversed: bool,
    /// Incremented when the health report changes, invalidating the list cache.
    pub health_epoch: u64,
    /// Cached visible item ids, keyed by the inputs that produced them.
    visible_cache: std::cell::RefCell<Option<(VisibleKey, VisibleRows)>>,
}

/// The rows the list pane shows: shared, so a frame can ask repeatedly
/// without rebuilding them.
pub type VisibleRows = std::sync::Arc<[std::sync::Arc<crate::vault::cache::Row>]>;

#[derive(Debug, Clone, PartialEq)]
struct VisibleKey {
    cache_version: u64,
    filter: Filter,
    sort: SortOrder,
    reverse: bool,
    query: Option<Query>,
    search_notes: bool,
    health_epoch: u64,
}

impl VisibleKey {
    /// Compares against the live state without cloning the filter or query.
    #[expect(
        clippy::too_many_arguments,
        reason = "the cache key fields, compared without cloning the filter or query"
    )]
    fn is_current(
        &self,
        cache_version: u64,
        filter: &Filter,
        sort: SortOrder,
        reverse: bool,
        query: Option<&Query>,
        search_notes: bool,
        health_epoch: u64,
    ) -> bool {
        self.cache_version == cache_version
            && self.sort == sort
            && self.reverse == reverse
            && self.search_notes == search_notes
            && self.health_epoch == health_epoch
            && &self.filter == filter
            && self.query.as_ref() == query
    }
}

impl AppState {
    /// Builds the state with a loaded theme and keymap.
    #[must_use]
    pub fn new(
        config: Config,
        theme: Theme,
        keymap: crate::config::keymap::Keymap,
        paths: crate::config::Paths,
    ) -> Self {
        let transport = config.bw.transport;
        let animations_enabled = config.ui.animations && !crate::theme::no_color();
        let size = (80, 24);
        let now = Utc::now();
        Self {
            config,
            theme,
            keymap,
            dispatcher: Dispatcher::default(),
            screen: Screen::Starting,
            tab: Tab::Vault,
            focus: Focus::List,
            session: SessionPhase::Starting,
            cache: VaultCache::new(),
            filter: Filter::All,
            sidebar_row: 0,
            collapsed: BTreeSet::new(),
            sidebar_visible: true,
            list_row: 0,
            marks: BTreeSet::new(),
            visual_anchor: None,
            search: SearchState::default(),
            detail: DetailState::default(),
            editor: None,
            generator: GeneratorState::default(),
            health: HealthState::default(),
            overlay: None,
            which_key: None,
            settings: SettingsState::default(),
            keymap_editor: KeymapEditorState::default(),
            help_filter: String::new(),
            help_offset: 0,
            messages_offset: 0,
            unlock: UnlockState::default(),
            login: LoginState::default(),
            clipboard: Clipboard::system(),
            clipboard_label: None,
            toasts: Vec::new(),
            messages: Vec::new(),
            command_history: Vec::new(),
            status_message: None,
            showcmd: String::new(),
            email: None,
            server: None,
            last_sync: None,
            pending: BTreeMap::new(),
            next_request_id: 1,
            undo: UndoStack::default(),
            jump: JumpList::default(),
            yanked_item: None,
            quit: false,
            quit_when_saved: false,
            dirty: true,
            full_redraw: false,
            folds_restored: false,
            theme_preview_from: None,
            size,
            now,
            started: Instant::now(),
            paths,
            bw_path: None,
            capabilities: None,
            transport,
            keyboard_enhanced: true,
            last_input: Instant::now(),
            last_input_wall: std::time::SystemTime::now(),
            last_sync_at: None,
            reprompt_once: None,
            reprompt_passed: None,
            detail_item: None,
            pending_generate_target: None,
            help_chain: Vec::new(),
            last_import: None,
            ui_scroll: std::cell::RefCell::new(BTreeMap::new()),
            ui_pages: std::cell::RefCell::new(BTreeMap::new()),
            fx: std::cell::RefCell::new(crate::ui::fx::Fx::new(animations_enabled)),
            notes: None,
            sidebar_width: None,
            list_width: None,
            drag: None,
            last_totp_code: None,
            folder_restore: None,
            sort_reversed: false,
            health_epoch: 0,
            visible_cache: std::cell::RefCell::new(None),
        }
    }

    /// The rows of the current list, after filter and search.
    ///
    /// The result is shared, so a frame can ask for it as often as it likes:
    /// an up-to-date call is a refcount bump, not a 5,000-row rebuild.
    #[must_use]
    pub fn visible_rows(&self) -> VisibleRows {
        let cache_version = self.cache.version();
        {
            let cache = self.visible_cache.borrow();
            if let Some((cached, rows)) = cache.as_ref()
                && cached.is_current(
                    cache_version,
                    &self.filter,
                    self.config.vault.sort,
                    self.sort_reversed,
                    self.search.query.as_ref(),
                    self.config.vault.search_notes,
                    self.health_epoch,
                )
            {
                return rows.clone();
            }
        }
        let key = VisibleKey {
            cache_version,
            filter: self.filter.clone(),
            sort: self.config.vault.sort,
            reverse: self.sort_reversed,
            query: self.search.query.clone(),
            search_notes: self.config.vault.search_notes,
            health_epoch: self.health_epoch,
        };
        let rows: Vec<std::sync::Arc<crate::vault::cache::Row>> = if let Some(query) =
            &self.search.query
        {
            let mut matcher = crate::vault::query::QueryMatcher::new(query);
            let health = self.health.report.as_ref().filter(|_| query.uses_health());
            let candidates = self.visible_cache.borrow().as_ref().and_then(|(cached, rows)| {
                let previous = cached.query.as_ref()?;
                (cached.is_current(
                    cache_version,
                    &self.filter,
                    self.config.vault.sort,
                    self.sort_reversed,
                    Some(previous),
                    self.config.vault.search_notes,
                    self.health_epoch,
                ) && query.narrows(previous))
                .then(|| rows.to_vec())
            });
            let candidates = candidates.unwrap_or_else(|| self.cache.rows_matching(&self.filter));
            let hits: Vec<_> = candidates
                .into_iter()
                .filter_map(|row| {
                    let item = self.cache.item(&row.id)?;
                    let flags = health.map(|report| report.flags(&row.id));
                    let result = matcher.matches(
                        item,
                        &row.folder_path,
                        flags,
                        self.config.vault.search_notes,
                    )?;
                    Some((row, result))
                })
                .collect();
            crate::vault::query::rank(hits).into_iter().map(|(row, _)| row).collect()
        } else {
            self.cache.rows_filtered(&self.filter, self.config.vault.sort, self.sort_reversed)
        };
        let rows: VisibleRows = rows.into();
        *self.visible_cache.borrow_mut() = Some((key, rows.clone()));
        rows
    }

    /// Records activity for the auto-lock.
    pub fn touch(&mut self) {
        self.last_input = Instant::now();
        self.last_input_wall = std::time::SystemTime::now();
    }

    /// How long since the last activity, counting time spent asleep.
    #[must_use]
    pub fn idle_for(&self) -> Duration {
        let wall =
            std::time::SystemTime::now().duration_since(self.last_input_wall).unwrap_or_default();
        self.last_input.elapsed().max(wall)
    }

    /// How many rows the view `key` showed in the last frame; a guess
    /// before the first one.
    #[must_use]
    pub fn page(&self, key: &str) -> usize {
        self.ui_pages
            .borrow()
            .get(key)
            .copied()
            .unwrap_or_else(|| (self.size.1 as usize).saturating_sub(6).max(1))
    }

    /// True when the sort is reversed.
    #[must_use]
    pub fn sort_reverse(&self) -> bool {
        self.sort_reversed
    }

    /// The selected row, if any.
    #[must_use]
    pub fn selected_item(&self) -> Option<&Item> {
        let rows = self.visible_rows();
        let row = rows.get(self.list_row.min(rows.len().saturating_sub(1)))?;
        self.cache.item(&row.id)
    }

    /// The selected item id.
    #[must_use]
    pub fn selected_id(&self) -> Option<String> {
        let rows = self.visible_rows();
        rows.get(self.list_row.min(rows.len().saturating_sub(1))).map(|r| r.id.clone())
    }

    /// The current query, for the statusline.
    #[must_use]
    pub fn search_active(&self) -> bool {
        self.search.active
    }

    /// The mode pill text.
    #[must_use]
    pub fn mode_name(&self) -> &'static str {
        if self.editor.as_ref().is_some_and(|e| e.insert) {
            return "INSERT";
        }
        if self.visual_anchor.is_some() {
            return "VISUAL";
        }
        match self.overlay {
            Some(Overlay::Cmdline(_)) => return "COMMAND",
            Some(Overlay::QuickOpen(_)) => return "SEARCH",
            _ if self.search.active => return "SEARCH",
            _ => {}
        }
        match self.screen {
            Screen::Unlock => "LOCKED",
            _ => "NORMAL",
        }
    }

    /// The breadcrumb segments.
    #[must_use]
    pub fn breadcrumb(&self) -> Vec<String> {
        match self.screen {
            Screen::Vault => {
                let mut parts = vec!["Vault".to_owned()];
                let title = self.filter.display_title();
                parts.push(title);
                if self.focus == Focus::Detail
                    && let Some(item) = self.selected_item()
                {
                    parts.push(item.name.clone());
                }
                parts
            }
            Screen::Generator => vec!["Generator".to_owned()],
            Screen::Health => vec!["Health".to_owned(), self.health.category.label().to_owned()],
            Screen::Editor => {
                let name = self.editor.as_ref().map(|e| e.item.name.clone()).unwrap_or_default();
                vec!["Edit".to_owned(), if name.is_empty() { "new item".to_owned() } else { name }]
            }
            Screen::Settings => vec!["Settings".to_owned()],
            Screen::KeymapEditor => vec!["Keybindings".to_owned()],
            Screen::Help => vec!["Help".to_owned()],
            Screen::Messages => vec!["Messages".to_owned()],
            Screen::Login => vec!["Login".to_owned()],
            Screen::Unlock => vec!["Locked".to_owned()],
            Screen::Starting => vec!["Starting".to_owned()],
            Screen::Error => vec!["Error".to_owned()],
        }
    }

    /// The current list position for the statusline pill.
    #[must_use]
    pub fn position(&self) -> (usize, usize) {
        let len = match self.screen {
            Screen::Vault => self.visible_rows().len(),
            Screen::Health => {
                self.health.report.as_ref().map(|r| r.len_in(self.health.category)).unwrap_or(0)
            }
            _ => 0,
        };
        (self.list_row.min(len.saturating_sub(1)) + 1, len)
    }

    /// True when a modal is open: anything that dims the screen behind it.
    #[must_use]
    pub fn overlay_open(&self) -> bool {
        self.generator_popup_open() || self.overlay.is_some()
    }

    /// Is the editor's `Ctrl-g` generator popup showing?
    #[must_use]
    pub fn generator_popup_open(&self) -> bool {
        self.generator.popup && self.screen == Screen::Editor
    }

    /// The keymap context chain for the current focus.
    #[must_use]
    pub fn contexts(&self) -> Vec<String> {
        let mut chain = Vec::new();
        if let Some(overlay) = &self.overlay {
            chain.extend(overlay.context().map(str::to_owned));
        } else {
            match self.screen {
                Screen::Vault => {
                    if self.visual_anchor.is_some() {
                        chain.push("visual".to_owned());
                    }
                    chain.push(self.focus.context().to_owned());
                }
                Screen::Editor if self.generator.popup => {
                    chain.push("generator".to_owned());
                }
                Screen::Editor => {
                    chain.push(if self.editor.as_ref().is_some_and(|e| e.insert) {
                        "insert".to_owned()
                    } else {
                        "editor".to_owned()
                    });
                }
                Screen::Generator => chain.push("generator".to_owned()),
                Screen::Health => chain.push("health".to_owned()),
                Screen::Unlock => chain.push("unlock".to_owned()),
                Screen::Login => chain.push("login".to_owned()),
                Screen::Settings => chain.push("settings".to_owned()),
                Screen::KeymapEditor => chain.push("keymap_editor".to_owned()),
                Screen::Help | Screen::Messages => chain.push("help".to_owned()),
                Screen::Starting | Screen::Error => {}
            }
        }
        chain.push("global".to_owned());
        chain
    }

    /// True while something on screen is still moving: an effect, or a toast
    /// sliding in. The main loop keeps drawing frames until this settles.
    #[must_use]
    pub fn animating(&self) -> bool {
        let fx = self.fx.borrow();
        fx.is_active() || self.toasts.iter().any(|toast| fx.toast_sliding(toast.created))
    }

    /// When a pending key sequence next needs attention: which-key opening
    /// after `which_key_delay`, or an ambiguous sequence resolving after
    /// `timeoutlen`. The main loop wakes up then; the one-second tick alone
    /// would show which-key anywhere from 0.3 to 1.3 seconds after the key.
    #[must_use]
    pub fn key_deadline(&self) -> Option<Instant> {
        let since = self.dispatcher.since.filter(|_| self.dispatcher.is_pending())?;
        let which_key = self.which_key.is_none().then(|| since + self.config.which_key_delay());
        let timeout = self
            .dispatcher
            .timeout_applies(&self.keymap, &self.contexts())
            .then(|| since + self.config.timeoutlen());
        which_key.into_iter().chain(timeout).min()
    }

    /// Adds a toast and logs it.
    pub fn toast(&mut self, level: MessageLevel, title: impl Into<String>) {
        let title = title.into();
        self.toasts.push(Toast {
            level,
            title: title.clone(),
            body: None,
            created: Instant::now(),
        });
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
        self.log(level, title);
        self.dirty = true;
    }

    /// Adds a toast with a body line.
    pub fn toast_with(
        &mut self,
        level: MessageLevel,
        title: impl Into<String>,
        body: impl Into<String>,
    ) {
        let title = title.into();
        let body = body.into();
        self.toasts.push(Toast {
            level,
            title: title.clone(),
            body: Some(body.clone()),
            created: Instant::now(),
        });
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
        self.log(level, format!("{title} {body}"));
        self.dirty = true;
    }

    /// Logs a message and shows it on the message line.
    pub fn log(&mut self, level: MessageLevel, text: impl Into<String>) {
        let message = LogMessage { time: self.now, level, text: text.into() };
        self.status_message = Some(message.clone());
        self.messages.push(message);
        if self.messages.len() > 500 {
            self.messages.remove(0);
        }
    }

    /// Shows a message without a toast.
    pub fn message(&mut self, level: MessageLevel, text: impl Into<String>) {
        self.log(level, text);
        self.dirty = true;
    }

    /// Removes expired toasts and messages.
    pub fn expire(&mut self, now: Instant) {
        self.toasts.retain(|toast| now.duration_since(toast.created) < toast.lifetime());
    }

    /// Applies a loaded vault snapshot.
    pub fn apply_snapshot(&mut self, snapshot: VaultSnapshot) {
        let selected = self.selected_id();
        self.cache.load(snapshot.items, snapshot.folders, snapshot.trash, snapshot.archived);
        self.last_sync = self.last_sync.or(Some(self.now));
        // Only a session screen gives way to the vault; anywhere else (the
        // editor opened while the first load was on its way, a refresh, a
        // reload after a failure) the user stays where they are.
        self.enter_session(SessionPhase::Ready);
        if let Some(id) = selected {
            let rows = self.visible_rows();
            if let Some(index) = rows.iter().position(|row| row.id == id) {
                self.list_row = index;
            }
        }
        self.clamp_selection();
        self.dirty = true;
    }

    /// Keeps the list cursor, and the other end of a visual selection,
    /// inside the visible rows.
    pub fn clamp_selection(&mut self) {
        let len = self.visible_rows().len();
        if len == 0 {
            self.list_row = 0;
            self.visual_anchor = None;
        } else {
            self.list_row = self.list_row.min(len - 1);
            self.visual_anchor = self.visual_anchor.map(|anchor| anchor.min(len - 1));
        }
    }

    /// Moves the session to `phase`. Every session change at run time goes
    /// through here (a test checks it), so three things always agree:
    ///
    /// - the screen: each phase has its own, except Loading and Ready, which
    ///   leave a session screen for the active tab and otherwise keep the
    ///   screen the user is on;
    /// - the vault data in memory: leaving Loading or Ready forgets it;
    /// - the requests in flight: only the session's own steps survive that.
    pub fn enter_session(&mut self, phase: SessionPhase) {
        if let SessionPhase::Failed(error) = &phase {
            // What the screen says, for bug reports. Its stderr rows are
            // already in the log at `debug`, redacted, as they arrived.
            let text: Vec<&str> = error
                .lines
                .iter()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .collect();
            tracing::warn!(title = %error.title, "error screen: {}", text.join(" / "));
        }
        if self.session.holds_vault_data() && !phase.holds_vault_data() {
            self.forget_vault();
        }
        match phase.screen() {
            Some(screen) => self.screen = screen,
            None if self.screen.is_session_screen() => self.screen = self.tab.screen(),
            None => {}
        }
        self.session = phase;
        self.dirty = true;
    }

    /// Locks locally: forgets every decrypted value and shows the unlock
    /// screen, whatever the phase was.
    pub fn drop_secrets(&mut self) {
        if !self.session.holds_vault_data() {
            // enter_session only forgets when leaving Loading or Ready;
            // a lock from anywhere else still clears what may be left.
            self.forget_vault();
        }
        self.enter_session(SessionPhase::Locked);
    }

    /// The startup or fatal error being shown, if the session failed.
    #[must_use]
    pub fn error(&self) -> Option<&ErrorScreen> {
        match &self.session {
            SessionPhase::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// True while a request matching `kind` waits for its answer.
    ///
    /// Progress indicators ask this instead of keeping a `busy` flag of
    /// their own, so they stop however the request ends: answered, failed
    /// to send, or forgotten on lock.
    #[must_use]
    pub fn awaiting(&self, kind: impl Fn(&Pending) -> bool) -> bool {
        self.pending.values().any(kind)
    }

    /// How many changes to the vault are still on their way to Bitwarden:
    /// requests sent and not yet answered, and steps that will send more
    /// (a folder being recreated, a password being generated for an item).
    /// Quitting before they finish would lose them.
    #[must_use]
    pub fn pending_writes(&self) -> usize {
        let requests = self
            .pending
            .values()
            .filter(|pending| {
                matches!(
                    pending,
                    Pending::Save { .. }
                        | Pending::Delete { .. }
                        | Pending::Restore { .. }
                        | Pending::Folder
                )
            })
            .count();
        requests
            + usize::from(self.folder_restore.is_some())
            + usize::from(self.pending_generate_target.is_some())
    }

    /// The master password is being checked.
    #[must_use]
    pub fn unlocking(&self) -> bool {
        self.awaiting(|p| matches!(p, Pending::Unlock))
    }

    /// A login, the server URL or the session after `bw login` is being set up.
    #[must_use]
    pub fn logging_in(&self) -> bool {
        self.awaiting(|p| matches!(p, Pending::Login | Pending::ConfigServer | Pending::Starting))
    }

    /// The open re-prompt dialog's password is being checked.
    #[must_use]
    pub fn verifying_reprompt(&self) -> bool {
        self.reprompt()
            .and_then(|reprompt| reprompt.request)
            .is_some_and(|id| self.pending.contains_key(&id))
    }

    /// An export is running.
    #[must_use]
    pub fn exporting(&self) -> bool {
        self.awaiting(|p| matches!(p, Pending::Export))
    }

    /// An import is running.
    #[must_use]
    pub fn importing(&self) -> bool {
        self.awaiting(|p| matches!(p, Pending::Import))
    }

    /// A health check is running.
    #[must_use]
    pub fn checking_health(&self) -> bool {
        self.awaiting(|p| matches!(p, Pending::Health))
    }

    /// Drops every decrypted value from memory, closes whatever showed one,
    /// and forgets the requests whose answers would carry vault data. The
    /// session phase is left to the caller ([`AppState::enter_session`]).
    pub fn forget_vault(&mut self) {
        crate::vault::health::forget_strength_cache();
        self.cache.clear();
        self.undo.clear();
        self.generator.history.clear();
        self.generator.output.clear();
        self.editor = None;
        self.notes = None;
        self.detail = DetailState::default();
        self.detail_item = None;
        self.overlay = None;
        self.reprompt_once = None;
        self.reprompt_passed = None;
        self.yanked_item = None;
        self.pending_generate_target = None;
        self.search = SearchState::default();
        self.health = HealthState::default();
        self.last_totp_code = None;
        self.folder_restore = None;
        // Folder paths are vault data; the folds file is read again on the
        // next load (it holds ids only).
        self.collapsed.clear();
        self.folds_restored = false;
        self.clear_selection();
        self.clipboard.clear_ours();
        // Answers to requests sent before the lock carry vault data; forget
        // the requests so those answers are dropped when they arrive. Only
        // the session's own steps stay.
        self.pending.retain(|_, pending| {
            matches!(
                pending,
                Pending::Starting
                    | Pending::Unlock
                    | Pending::Lock
                    | Pending::Login
                    | Pending::Logout
                    | Pending::ConfigServer
            )
        });
        self.unlock = UnlockState::default();
        self.dirty = true;
    }

    /// Forgets the marks and leaves visual mode. An action that used the
    /// selection ends it, as an operator does in Vim: marks left behind would
    /// make the next `dd` act on the old items instead of the one under the
    /// cursor.
    pub fn clear_selection(&mut self) {
        self.marks.clear();
        self.visual_anchor = None;
    }

    /// The number of items the next action applies to.
    #[must_use]
    pub fn target_ids(&self, count: Option<u32>) -> Vec<String> {
        if !self.marks.is_empty() {
            return self.marks.iter().cloned().collect();
        }
        if let Some((start, end)) = self.visual_range() {
            // The list can shrink under a selection (a sync, a change made
            // elsewhere); never read past its end.
            let rows = self.visible_rows();
            let end = end.min(rows.len().saturating_sub(1));
            return rows
                .get(start..=end)
                .unwrap_or_default()
                .iter()
                .map(|r| r.id.clone())
                .collect();
        }
        let rows = self.visible_rows();
        if rows.is_empty() {
            return Vec::new();
        }
        let start = self.list_row.min(rows.len() - 1);
        let count = count.unwrap_or(1) as usize;
        rows[start..rows.len().min(start + count)].iter().map(|r| r.id.clone()).collect()
    }

    /// The visual selection range as inclusive indices.
    #[must_use]
    pub fn visual_range(&self) -> Option<(usize, usize)> {
        let anchor = self.visual_anchor?;
        Some((anchor.min(self.list_row), anchor.max(self.list_row)))
    }

    /// True when the item is marked or inside the visual range.
    #[must_use]
    pub fn is_selected(&self, index: usize) -> bool {
        if let Some((start, end)) = self.visual_range() {
            return index >= start && index <= end;
        }
        self.visible_rows().get(index).is_some_and(|row| self.marks.contains(&row.id))
    }
}

/// The master password re-prompt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepromptState {
    /// Item id.
    pub item_id: String,
    /// Item name.
    pub item_name: String,
    /// The password input.
    pub password: crate::input::secret::SecretInput,
    /// Error.
    pub error: Option<String>,
    /// What runs once the password is right.
    pub action: Deferred,
    /// The unlock request checking the typed password, once sent. Only the
    /// answer to this request may run the action, so an answer that comes
    /// back after the dialog was closed (or replaced) does nothing.
    pub request: Option<u64>,
}

/// A row of the settings overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingRow {
    /// Section heading, when this row starts one.
    pub section: Option<&'static str>,
    /// Label.
    pub label: &'static str,
    /// Dotted config key (or `keybindings` for the action row).
    pub key: &'static str,
    /// How the value is edited.
    pub kind: SettingKind,
    /// Current value as text.
    pub value: String,
}

/// How a settings row is edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingKind {
    /// A checkbox.
    Bool,
    /// A fixed list of options.
    Option(Vec<String>),
    /// An integer.
    Number,
    /// Free text.
    Text,
    /// A duration string.
    Duration,
    /// A button that opens another screen.
    Action,
}

/// A one-line prompt overlay (new folder name, export path, …).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptState {
    /// Title.
    pub title: String,
    /// The input.
    pub input: crate::input::line::Line,
    /// What Enter does with the text.
    pub on_accept: PromptAction,
    /// Error message.
    pub error: Option<String>,
}

impl PromptState {
    /// A prompt starting with `text`.
    #[must_use]
    pub fn new(title: impl Into<String>, text: &str, on_accept: PromptAction) -> Self {
        Self {
            title: title.into(),
            input: crate::input::line::Line::new(text),
            on_accept,
            error: None,
        }
    }
}
