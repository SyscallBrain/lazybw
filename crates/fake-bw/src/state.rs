//! Persistent state for the `fake-bw` test double.
//!
//! Every command loads the JSON file pointed to by `FAKE_BW_STATE`, mutates it
//! and writes it back atomically. The file is the single source of truth for
//! the vault contents, the session status and the invocation log.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Version reported by `fake-bw --version` when the state file has none.
pub const DEFAULT_VERSION: &str = "2026.10.1";
/// Master password accepted by `login` and `POST /unlock`.
pub const DEFAULT_MASTER_PASSWORD: &str = "correct horse";
/// Session key reported by `login --raw` and `POST /unlock`.
pub const DEFAULT_SESSION_KEY: &str = "fake-session";

/// The account is not logged in.
pub const STATUS_UNAUTHENTICATED: &str = "unauthenticated";
/// The account is logged in but the vault is locked.
pub const STATUS_LOCKED: &str = "locked";
/// The vault is unlocked and usable.
pub const STATUS_UNLOCKED: &str = "unlocked";

/// Environment variable prefixes copied into the invocation log (names only).
const LOGGED_ENV_PREFIXES: [&str; 4] = ["BW_", "BITWARDENCLI_", "LAZYBW_", "FAKE_BW_"];

/// The JSON document stored at `$FAKE_BW_STATE`.
///
/// Unknown top-level keys are preserved in `extra` so fixtures can carry
/// fields the fake does not care about.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// CLI version reported by `--version`.
    #[serde(default = "default_version")]
    pub version: String,
    /// `unauthenticated`, `locked` or `unlocked`.
    #[serde(default = "default_status")]
    pub status: String,
    /// Configured server URL; `null` means the default cloud.
    #[serde(default)]
    pub server_url: Option<String>,
    /// Timestamp of the last successful sync.
    #[serde(default)]
    pub last_sync: Option<String>,
    /// Account email shown by `status`.
    #[serde(default = "default_email")]
    pub email: Option<String>,
    /// Account identifier shown by `status`.
    #[serde(default = "default_user_id")]
    pub user_id: Option<String>,
    /// Password accepted by `login` and `POST /unlock`.
    #[serde(default = "default_master_password")]
    pub master_password: String,
    /// Session key reported on login and unlock; `null` after logout.
    #[serde(default = "default_session_key")]
    pub session_key: Option<String>,
    /// Whether the interactive login asks for a two-step login code.
    #[serde(default)]
    pub two_factor: bool,
    /// Folder objects exactly as returned by the CLI.
    #[serde(default)]
    pub folders: Vec<Value>,
    /// Item objects exactly as returned by the CLI.
    #[serde(default)]
    pub items: Vec<Value>,
    /// Passwords treated as exposed by `report password-health`.
    #[serde(default)]
    pub exposed_passwords: Vec<String>,
    /// Invocation log entries.
    #[serde(default)]
    pub log: Vec<Value>,
    /// Unknown keys preserved for forward compatibility.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: DEFAULT_VERSION.to_owned(),
            status: STATUS_UNAUTHENTICATED.to_owned(),
            server_url: None,
            last_sync: None,
            email: default_email(),
            user_id: default_user_id(),
            master_password: DEFAULT_MASTER_PASSWORD.to_owned(),
            session_key: default_session_key(),
            two_factor: false,
            folders: Vec::new(),
            items: Vec::new(),
            exposed_passwords: Vec::new(),
            log: Vec::new(),
            extra: Map::new(),
        }
    }
}

fn default_version() -> String {
    DEFAULT_VERSION.to_owned()
}

fn default_status() -> String {
    STATUS_UNAUTHENTICATED.to_owned()
}

fn default_email() -> Option<String> {
    Some("you@example.com".to_owned())
}

fn default_user_id() -> Option<String> {
    Some("00000000-0000-4000-8000-000000000001".to_owned())
}

fn default_master_password() -> String {
    DEFAULT_MASTER_PASSWORD.to_owned()
}

fn default_session_key() -> Option<String> {
    Some(DEFAULT_SESSION_KEY.to_owned())
}

impl State {
    /// Whether the account is logged in (locked or unlocked).
    #[must_use]
    pub fn is_logged_in(&self) -> bool {
        self.status == STATUS_LOCKED || self.status == STATUS_UNLOCKED
    }

    /// Whether the vault is unlocked.
    #[must_use]
    pub fn is_unlocked(&self) -> bool {
        self.status == STATUS_UNLOCKED
    }

    /// The session key, falling back to the documented default.
    #[must_use]
    pub fn session_key(&self) -> String {
        self.session_key.clone().unwrap_or_else(|| DEFAULT_SESSION_KEY.to_owned())
    }

    /// Loads and parses the state file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("cannot read state file {}: {e}", path.display()))?;
        serde_json::from_str(&raw)
            .map_err(|e| format!("cannot parse state file {}: {e}", path.display()))
    }

    /// Serialises the state and atomically replaces the file at `path`.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let data =
            serde_json::to_vec_pretty(self).map_err(|e| format!("cannot serialise state: {e}"))?;
        let dir =
            path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("state.json");
        let tmp =
            dir.join(format!(".{file_name}.tmp-{}-{}", std::process::id(), rand::random::<u32>()));
        let write = (|| -> std::io::Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)?;
            file.write_all(&data)?;
            file.sync_all()?;
            Ok(())
        })();
        if let Err(e) = write {
            let _ = fs::remove_file(&tmp);
            return Err(format!("cannot write state file {}: {e}", path.display()));
        }
        if let Err(e) = fs::rename(&tmp, path) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("cannot replace state file {}: {e}", path.display()));
        }
        Ok(())
    }

    /// Appends one invocation entry (without an HTTP request) to the log.
    pub fn record_invocation(&mut self, cmd: &[String]) {
        self.log.push(log_entry(cmd, None));
    }

    /// Appends one served request entry to the log.
    pub fn record_request(&mut self, cmd: &[String], method: &str, path: &str) {
        self.log.push(log_entry(cmd, Some((method, path))));
    }
}

/// The state file path taken from `$FAKE_BW_STATE`.
pub fn state_path() -> Result<PathBuf, String> {
    match std::env::var_os("FAKE_BW_STATE") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Err("FAKE_BW_STATE is not set.".to_owned()),
    }
}

/// Names (never values) of the environment variables relevant to `bw`.
#[must_use]
pub fn filtered_env_names<I, K, V>(vars: I) -> Vec<String>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mut names: Vec<String> = vars
        .into_iter()
        .map(|(name, _value)| name.as_ref().to_owned())
        .filter(|name| {
            name == "NO_COLOR" || LOGGED_ENV_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The names of the current process environment variables that go into the log.
#[must_use]
pub fn current_env_names() -> Vec<String> {
    filtered_env_names(std::env::vars())
}

/// Builds a log entry. Only environment variable *names* are recorded.
#[must_use]
pub fn log_entry(cmd: &[String], request: Option<(&str, &str)>) -> Value {
    let request = match request {
        Some((method, path)) => json!({"method": method, "path": path}),
        None => Value::Null,
    };
    json!({
        "cmd": cmd,
        "env": current_env_names(),
        "request": request,
    })
}

/// Generates a random UUID-like identifier.
#[must_use]
pub fn new_id() -> String {
    let bytes = rand::random::<[u8; 16]>();
    let mut bytes = bytes;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let mut hex = String::with_capacity(32);
    for byte in bytes {
        hex.push_str(&format!("{byte:02x}"));
    }
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

/// Current UTC time formatted as an ISO 8601 string with milliseconds.
#[must_use]
pub fn now_iso() -> String {
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format_timestamp(duration.as_secs(), duration.subsec_millis())
}

/// Formats a Unix timestamp as an ISO 8601 UTC string.
#[must_use]
pub fn format_timestamp(secs: u64, millis: u32) -> String {
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// Howard Hinnant's `civil_from_days`, valid for the Unix epoch onwards.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year, u32::try_from(m).unwrap_or(0), u32::try_from(d).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_entry_records_env_names_but_never_values() {
        let names = filtered_env_names([
            ("BW_SESSION", "super-secret-key"),
            ("HOME", "/home/user"),
            ("NO_COLOR", "1"),
            ("FAKE_BW_FAIL", "unlock:boom"),
            ("PATH", "/usr/bin"),
        ]);
        assert_eq!(names, vec!["BW_SESSION", "FAKE_BW_FAIL", "NO_COLOR"]);
        assert!(!names.iter().any(|name| name.contains("super-secret-key")));
    }

    #[test]
    fn log_entry_has_command_environment_and_request() {
        let cmd = vec!["serve".to_owned(), "--hostname".to_owned(), "fd+connected://3".to_owned()];
        let entry = log_entry(&cmd, Some(("GET", "/status")));
        assert_eq!(entry["cmd"], json!(cmd));
        assert_eq!(entry["request"]["method"], "GET");
        assert_eq!(entry["request"]["path"], "/status");
        assert!(entry["env"].is_array());
    }

    #[test]
    fn timestamp_format_is_iso8601() {
        assert_eq!(format_timestamp(0, 0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format_timestamp(1_000_000_000, 250), "2001-09-09T01:46:40.250Z");
    }

    #[test]
    fn save_is_atomic_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("fake-bw-test-{}", rand::random::<u32>()));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("state.json");
        let state = State {
            status: STATUS_UNLOCKED.to_owned(),
            items: vec![json!({"id": "one"})],
            ..State::default()
        };
        state.save(&path).expect("save");
        let loaded = State::load(&path).expect("load");
        assert_eq!(loaded.status, STATUS_UNLOCKED);
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.master_password, DEFAULT_MASTER_PASSWORD);
        fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn new_ids_are_distinct_and_uuid_shaped() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.chars().filter(|c| *c == '-').count(), 4);
    }
}
